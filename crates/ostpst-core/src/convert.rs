// Carried over from pstfree 0.5.1 (MIT, https://github.com/sp00nznet/pstfree),
// Copyright (c) 2026 pstfree contributors. Modified for OST/PST Studio; see
// docs/IMPLEMENTATION_STATUS.md for the list of changes.

//! Turn a file a PST cannot hold into one: an Outlook 2013+ OST, or an ANSI PST.
//!
//! This is the third thing the payware sells, "OST to PST Converter", and the one
//! [`repair`](crate::repair) refused, correctly, because it is not a repair. A repair
//! copies blocks byte for byte and rebuilds only the index around them, which is why it
//! cannot corrupt a property. A conversion cannot do that: the 4K-page format an Outlook
//! 2013 OST uses stores its blocks compressed, up to 64KB each, and a PST's blocks are
//! plain and hold 8176 bytes at the very most. Every block has to be decoded, and some of
//! them have to be taken apart.
//!
//! An ANSI PST, Outlook 97 to 2002, arrives here for a different reason and leaves by
//! the same door. Nothing in it is compressed and its blocks are already small enough,
//! but every id and offset in it is 32 bits and both its trailers are a different shape,
//! so a block copied across would carry a trailer no Unicode reader can parse. Rebuilding
//! a level up sidesteps all of that: the *contents* of a stream are the same bytes in
//! both formats, and it is only the plumbing around them that has to be rewritten. The
//! same pass also lifts the 2GB ceiling the ANSI format is stuck behind, which is the
//! other half of why anyone wants this conversion.
//!
//! So this rebuilds the file a level up. It walks the nodes, and for each one takes the
//! *data stream* behind it, whatever chain of blocks and XBLOCKs the OST used to hold it
//!, and lays that stream out again as a PST would: plain blocks, an XBLOCK over them
//! when there is more than one, an XXBLOCK when there are more than fit in an XBLOCK.
//! Subnode trees are rebuilt the same way, recursively, since an attachment is a subnode
//! and an embedded message is a subnode tree hanging off one.
//!
//! **Heaps are rebuilt, everything else keeps its block boundaries.** A heap-on-node,
//! which is how every property context and every table is stored, addresses its
//! allocations by *which block* they are in (MS-PST 2.3.1.1, the `hidBlockIndex` field of
//! an HID), and a 4K-page OST even packs that id differently from a PST. So a heap is
//! read out allocation by allocation and written again in the PST's layout, with the
//! values that no longer fit moved out into subnodes; see [`heapw`](crate::heapw). A
//! stream that is not a heap, a body or an attachment, is addressed by offset, so it is
//! carried over with its blocks as they were and only ever divided when one is too big
//! for the target format to hold.
//!
//! Nothing is re-encoded: the output declares `NDB_CRYPT_NONE` and its blocks are written
//! plain. The encoding is keyless and protects nothing, that is the whole argument of
//! this project, so preserving it would be work in exchange for nothing.

use crate::heapw::{copies_cleanly, rebuild_pc, rebuild_tc, write_pc, RebuiltHeap};
use crate::ltp::{Heap, Lookup, CLIENT_SIG_PC, CLIENT_SIG_TC};
use crate::ndb::{crc32, Node, Pst};
use crate::ostfix::RawProps;
use crate::repair::{
    build_tree, pad, place_aligned, put16, put32, put64, sig, Rebuilt, AMAP_EVERY, AMAP_FIRST,
    BBTENTRY, BLOCK_ALIGN, ESSENTIAL, MAP_SLOTS, NBTENTRY, OFF_BID_NEXT_B, OFF_BID_NEXT_P, PAGE,
    PTYPE_BBT, PTYPE_NBT,
};
use std::collections::HashSet;
use std::io::{Seek, SeekFrom, Write};

/// The most a PST data block can hold: 8KB all in, less the 16-byte trailer.
/// MS-PST 2.2.2.8.
pub(crate) const MAX_DATA: usize = 8176;
/// An XBLOCK is a header and then 8 bytes an entry, in one block.
const X_ENTRIES: usize = (MAX_DATA - 8) / 8;
/// An SLBLOCK entry is a local id and two block ids, padded to 24.
const SL_ENTRIES: usize = (MAX_DATA - 8) / 24;
/// An SIBLOCK entry is a local id and the block below it.
const SI_ENTRIES: usize = (MAX_DATA - 8) / 16;

/// The PST client signature, 'SM'. An OST says 'SO' and is otherwise the same header.
const MAGIC_PST: u16 = 0x4D53;
/// `wVer` for the 512-byte-page Unicode format, and `wVerClient` to go with it. An
/// Outlook 2013 OST carries 36 and 12; both fixtures agree on what a PST carries.
const VER_UNICODE: u16 = 23;
const VER_CLIENT: u16 = 19;

/// How deep a subnode tree may nest before this gives up. An attachment is a subnode, an
/// embedded message is a subnode tree under one, and its attachments are subnodes of
/// that, real files go two or three deep. A hundred is a damaged file pointing at
/// itself, and the recursion here is the thing that would notice by overflowing a stack.
const MAX_NEST: u32 = 100;

/// A block id's low bits: bit 1 says the block is an index or a subnode tree rather than
/// data, which is what tells a reader not to try to decode it. MS-PST 2.2.2.2.
const BID_INTERNAL: u64 = 2;

/// The file being written, and the index of it built up as it goes.
///
/// Everything is written in one forward pass, so this never holds more than a single data
/// stream at a time. The header is the one thing written twice: a placeholder first,
/// because the B-tree roots it names do not exist until the end, and the real one after a
/// seek back to nought.
struct Out {
    f: std::io::BufWriter<std::fs::File>,
    name: String,
    /// Where the next block goes, before alignment and the map slots are accounted for.
    pos: u64,
    /// BBTENTRY leaves, in the order the blocks were written, which is bid order.
    bbt: Vec<(u64, Vec<u8>)>,
    /// The counter behind every id handed out. A BID's low two bits are flags, so the
    /// counter is the id shifted down by two, exactly as `bidNextB` in the header is.
    next: u64,
    problems: Vec<String>,
    /// The source is a 4K-page OST, whose heaps must be laid out again.
    wide: bool,
}

impl Out {
    fn new(path: &str, first_bid_index: u64, wide: bool) -> Result<Out, String> {
        let f = std::fs::File::create(path).map_err(|e| format!("{path}: {e}"))?;
        let mut o = Out {
            f: std::io::BufWriter::with_capacity(1 << 20, f),
            name: path.to_string(),
            pos: 0,
            bbt: Vec::new(),
            next: first_bid_index,
            problems: Vec::new(),
            wide,
        };
        // Zeroes over the header and the first section's map slots, so the file actually
        // contains them: the real header is written back over these at the end, once the
        // B-tree roots it has to name exist. Skipping the space rather than writing it
        // would leave the front of the file missing, which is the sort of thing that
        // still reads correctly by sweeping and is wrong all the same.
        let start = AMAP_FIRST + MAP_SLOTS * PAGE;
        pad(&mut o.f, &mut o.pos, start).map_err(|e| format!("{path}: {e}"))?;
        Ok(o)
    }

    fn io(&self, e: std::io::Error) -> String {
        format!("{}: {e}", self.name)
    }

    fn take_bid(&mut self, internal: bool) -> u64 {
        let bid = (self.next << 2) | if internal { BID_INTERNAL } else { 0 };
        self.next += 1;
        bid
    }

    /// Write one block and return the id it was given.
    ///
    /// Every block in the output goes through here, which is why there is exactly one
    /// place that knows a trailer's shape, and one place that knows a block may not sit
    /// where an allocation map page goes.
    fn block(&mut self, data: &[u8], internal: bool) -> Result<u64, String> {
        if data.len() > MAX_DATA {
            return Err(format!(
                "a {}-byte block was built, and {MAX_DATA} is the most a PST can hold, \
                 this is a bug in the converter, not in the file",
                data.len()
            ));
        }
        let bid = self.take_bid(internal);
        let span = (data.len() as u64 + 16).div_ceil(BLOCK_ALIGN) * BLOCK_ALIGN;
        let at = place_aligned(self.pos, span, BLOCK_ALIGN);

        let mut buf = vec![0u8; span as usize];
        buf[..data.len()].copy_from_slice(data);
        // BLOCKTRAILER: the length of the data, the signature over id and position, the
        // checksum over the data as written, and the id. MS-PST 2.2.2.8.1.
        let t = span as usize - 16;
        put16(&mut buf, t, data.len() as u16);
        put16(&mut buf, t + 2, sig(at, bid));
        put32(&mut buf, t + 4, crc32(data));
        put64(&mut buf, t + 8, bid);

        pad(&mut self.f, &mut self.pos, at).map_err(|e| self.io(e))?;
        self.f.write_all(&buf).map_err(|e| self.io(e))?;
        self.pos = at + span;

        // BBTENTRY: the block's id and offset, then its length and reference count.
        let mut e = vec![0u8; BBTENTRY];
        put64(&mut e, 0, bid);
        put64(&mut e, 8, at);
        put16(&mut e, 16, data.len() as u16);
        // Everything written here is named by exactly one parent, and a singly-referenced
        // block counts 2. No reader checks it; it is written honestly anyway.
        put16(&mut e, 18, 2);
        self.bbt.push((bid, e));
        Ok(bid)
    }

    /// Lay a node's data stream out again, and return the id that now names all of it.
    ///
    /// The stream comes back from the OST already decoded and inflated, as the list of
    /// blocks it was stored in. That list is kept: see the note at the top about heaps
    /// addressing themselves by block number.
    fn stream(
        &mut self,
        pst: &mut Pst,
        src: u64,
        what: &str,
        may_be_heap: bool,
    ) -> Result<u64, String> {
        if src == 0 {
            return Ok(0);
        }
        let mut parts: Vec<(u64, u64)> = Vec::new(); // (bid, bytes it holds)
        for b in pst.node_blocks(src)? {
            if b.len() <= MAX_DATA {
                parts.push((self.block(&b, false)?, b.len() as u64));
                continue;
            }
            // Too big for the target format, so it has to come apart. Fine for a message
            // body or an attachment, which are addressed by offset across the whole
            // stream; not fine for a heap, whose contents point at each other by block.
            if may_be_heap && b.len() > 2 && b[2] == 0xEC {
                self.problems.push(format!(
                    "{what}: a {}-byte heap block had to be divided to fit a PST's 8176-byte \
                     limit. Heaps address themselves by block number, so anything stored in \
                     that one may now read back wrong.",
                    b.len()
                ));
            }
            for chunk in b.chunks(MAX_DATA) {
                parts.push((self.block(chunk, false)?, chunk.len() as u64));
            }
        }
        match parts.len() {
            0 => Ok(0),
            1 => Ok(parts[0].0),
            _ => self.xblock(&parts),
        }
    }

    /// An XBLOCK over a list of data blocks, or an XXBLOCK over XBLOCKs when one is not
    /// enough. MS-PST 2.2.2.8.3.2.
    fn xblock(&mut self, parts: &[(u64, u64)]) -> Result<u64, String> {
        if parts.len() > X_ENTRIES {
            let mut level1 = Vec::new();
            for group in parts.chunks(X_ENTRIES) {
                let bytes = group.iter().map(|(_, n)| n).sum();
                level1.push((self.xblock(group)?, bytes));
            }
            return self.array(&level1, 2);
        }
        self.array(parts, 1)
    }

    /// The body both levels share: a type, a level, a count, the bytes underneath, and
    /// then the ids.
    fn array(&mut self, parts: &[(u64, u64)], level: u8) -> Result<u64, String> {
        let mut body = vec![0u8; 8 + parts.len() * 8];
        body[0] = 0x01;
        body[1] = level;
        put16(&mut body, 2, parts.len() as u16);
        put32(
            &mut body,
            4,
            parts.iter().map(|(_, n)| n).sum::<u64>() as u32,
        );
        for (i, (bid, _)) in parts.iter().enumerate() {
            put64(&mut body, 8 + i * 8, *bid);
        }
        self.block(&body, true)
    }

    /// Write a list of ready-made blocks as one data stream.
    fn blocks(&mut self, blocks: &[Vec<u8>]) -> Result<u64, String> {
        let mut parts = Vec::with_capacity(blocks.len());
        for b in blocks {
            parts.push((self.block(b, false)?, b.len() as u64));
        }
        match parts.len() {
            0 => Ok(0),
            1 => Ok(parts[0].0),
            _ => self.xblock(&parts),
        }
    }

    /// A node, or a subnode: its data stream and its subnode tree, as the output names
    /// them.
    ///
    /// From a 4K-page OST, a heap is laid out again rather than copied; anything else,
    /// and everything from an ANSI PST, goes through [`Out::stream`] as it is.
    ///
    /// `lookup` finds the node behind a table row (a sibling of this node), which is
    /// how a table's cells are told apart when an OST keeps them in more than one heap.
    ///
    /// `who` is the node's own id and the id of the node it is reported under. A
    /// subnode of type 0x1F holds a value, a body or an attachment or a row matrix,
    /// never a heap, however much its first bytes may look like one.
    fn node(
        &mut self,
        pst: &mut Pst,
        src: (u64, u64),
        who: (u32, u32),
        depth: u32,
        what: &str,
        lookup: Lookup,
    ) -> Result<(u64, u64), String> {
        let (bid_data, bid_sub) = src;
        let (own, nid) = who;
        let value = own & 0x1F == 0x1F;
        if self.wide && bid_data != 0 && !value {
            let blocks = pst.node_blocks(bid_data)?;
            let is_heap = blocks
                .first()
                .is_some_and(|b| b.len() >= 12 && b[2] == 0xEC);
            // A heap of a type this cannot take apart is copied when that is known to
            // be exact, and reported when it is not.
            let known = is_heap && matches!(blocks[0][3], CLIENT_SIG_PC | CLIENT_SIG_TC);
            if is_heap && (known || !copies_cleanly(&blocks)) {
                let client = blocks[0][3];
                let mut subs: HashSet<u32> = pst.subnodes(bid_sub)?.keys().copied().collect();
                let rebuilt = match client {
                    CLIENT_SIG_PC => {
                        Heap::with_layout(blocks, true).and_then(|h| rebuild_pc(&h, &mut subs))
                    }
                    CLIENT_SIG_TC => rebuild_tc(pst, bid_data, bid_sub, &mut subs, Some(lookup)),
                    other => Err(format!("a heap of client type 0x{other:02X}")),
                };
                match rebuilt {
                    Ok(rb) => return self.rebuilt(pst, rb, bid_sub, nid, depth, what),
                    Err(e) => self.problems.push(format!(
                        "{what}: its heap could not be laid out again for a PST ({e}); it \
                         was copied block for block and may not read back correctly"
                    )),
                }
            }
        }
        let data = self.stream(pst, bid_data, what, !value)?;
        let sub = self.subtree(pst, bid_sub, nid, depth, Vec::new(), &[])?;
        Ok((data, sub))
    }

    /// A node whose properties were rewritten: the heap built from them, and its subnode
    /// tree with whatever had to spill out of that heap.
    fn pc_node(
        &mut self,
        pst: &mut Pst,
        props: &RawProps,
        n: &Node,
        what: &str,
    ) -> Result<(u64, u64), String> {
        let mut subs: HashSet<u32> = pst.subnodes(n.bid_sub)?.keys().copied().collect();
        let rb = write_pc(props, &mut subs)?;
        self.rebuilt(pst, rb, n.bid_sub, n.nid, 0, what)
    }

    fn rebuilt(
        &mut self,
        pst: &mut Pst,
        rb: RebuiltHeap,
        bid_sub: u64,
        nid: u32,
        depth: u32,
        what: &str,
    ) -> Result<(u64, u64), String> {
        if rb.unresolved > 0 {
            self.problems.push(format!(
                "{what}: {} value(s) pointed at heap space the source does not have and \
                 were left out",
                rb.unresolved
            ));
        }
        let data = self.blocks(&rb.blocks)?;
        let sub = self.subtree(pst, bid_sub, nid, depth, rb.spilled, &rb.dropped)?;
        Ok((data, sub))
    }

    /// Rebuild a node's subnode tree: every subnode's own data and its own subnodes, then
    /// a fresh SLBLOCK naming them. MS-PST 2.2.2.8.3.3.
    ///
    /// `extra` are subnodes to add, or whose data to replace, with their blocks already
    /// cut; `dropped` are subnodes of the source to leave out.
    fn subtree(
        &mut self,
        pst: &mut Pst,
        src: u64,
        nid: u32,
        depth: u32,
        extra: Vec<(u32, Vec<Vec<u8>>)>,
        dropped: &[u32],
    ) -> Result<u64, String> {
        if src == 0 && extra.is_empty() {
            return Ok(0);
        }
        if depth > MAX_NEST {
            return Err(format!(
                "subnode trees nest more than {MAX_NEST} deep under node 0x{nid:X}, \
                 the file points at itself"
            ));
        }
        // `subnodes` hands back the leaves with any SIBLOCK level above them already
        // flattened away, so what comes back is the whole set and the shape is this
        // code's to choose again.
        let mut subs: Vec<(u32, Option<crate::ndb::Sub>)> = pst
            .subnodes(src)?
            .into_iter()
            .filter(|(k, _)| !dropped.contains(k))
            .map(|(k, s)| (k, Some(s)))
            .collect();
        for (k, _) in &extra {
            if !subs.iter().any(|(n, _)| n == k) {
                subs.push((*k, None));
            }
        }
        subs.sort_by_key(|(k, _)| *k);

        let siblings: std::collections::HashMap<u32, (u64, u64)> = subs
            .iter()
            .filter_map(|(k, s)| s.map(|s| (*k, (s.data, s.sub))))
            .collect();
        let lookup = |id: u32| siblings.get(&id).copied();

        let mut entries = Vec::new();
        for (sub_nid, s) in subs {
            let what = format!("node 0x{nid:X}, subnode 0x{sub_nid:X}");
            if let Some((_, chunks)) = extra.iter().find(|(k, _)| *k == sub_nid) {
                let data = self.blocks(chunks)?;
                let below = match s {
                    Some(s) => self.subtree(pst, s.sub, nid, depth + 1, Vec::new(), &[])?,
                    None => 0,
                };
                entries.push((sub_nid, data, below));
                continue;
            }
            let s = s.expect("a subnode without an override comes from the source");
            let (data, below) = self.node(
                pst,
                (s.data, s.sub),
                (sub_nid, nid),
                depth + 1,
                &what,
                &lookup,
            )?;
            entries.push((sub_nid, data, below));
        }
        if entries.is_empty() {
            return Ok(0);
        }
        self.slblock(&entries)
    }

    fn slblock(&mut self, entries: &[(u32, u64, u64)]) -> Result<u64, String> {
        if entries.len() > SL_ENTRIES {
            let mut level1 = Vec::new();
            for group in entries.chunks(SL_ENTRIES) {
                level1.push((group[0].0, self.slblock(group)?));
            }
            return self.siblock(&level1);
        }
        let mut body = vec![0u8; 8 + entries.len() * 24];
        body[0] = 0x02;
        put16(&mut body, 2, entries.len() as u16);
        for (i, (nid, data, sub)) in entries.iter().enumerate() {
            let e = 8 + i * 24;
            put32(&mut body, e, *nid);
            put64(&mut body, e + 8, *data);
            put64(&mut body, e + 16, *sub);
        }
        self.block(&body, true)
    }

    fn siblock(&mut self, children: &[(u32, u64)]) -> Result<u64, String> {
        if children.len() > SI_ENTRIES {
            return Err(format!(
                "a node has more than {} subnodes, which needs a third level this does \
                 not write",
                SI_ENTRIES * SL_ENTRIES
            ));
        }
        let mut body = vec![0u8; 8 + children.len() * 16];
        body[0] = 0x02;
        body[1] = 1;
        put16(&mut body, 2, children.len() as u16);
        for (i, (nid, bid)) in children.iter().enumerate() {
            let e = 8 + i * 16;
            put32(&mut body, e, *nid);
            put64(&mut body, e + 8, *bid);
        }
        self.block(&body, true)
    }
}

/// The message store's node. MS-PST 2.4.1.
const NID_MESSAGE_STORE: u32 = 0x21;

/// Where a property's value lives: in the record itself, in the heap, or in a subnode.
pub(crate) enum Hnid {
    Inline(u32),
    Heap(Vec<u8>),
    Sub(u32),
}

/// Write an OST or an ANSI PST out as a Unicode PST, node by node.
pub fn convert(
    pst: &mut Pst,
    nodes: &[Node],
    out: &str,
    on: crate::Progress,
) -> Result<Rebuilt, String> {
    // Start the ids past everything the source used. Nothing carries over, every block
    // in the output is newly built, but a fresh range costs nothing and means an id in
    // the result can never be confused with an id in the file it came from.
    let first = (nodes
        .iter()
        .map(|n| n.bid_data.max(n.bid_sub))
        .max()
        .unwrap_or(0)
        >> 2)
        + 1;
    let ansi = pst.is_ansi();
    let mut o = Out::new(out, first, !ansi && !pst.is_small_page())?;

    let mut nbt: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut dropped_nodes = 0;
    let mut keep = nodes.to_vec();
    keep.sort_by_key(|n| n.nid);

    // An OST's store and folders carry Exchange's ids, which a PST reader cannot follow.
    // The roles are resolved once here; see `ostfix` for what gets rewritten and why.
    // An ANSI PST's store already is a PST's, with a password its owner may have set.
    let fix = if ansi {
        None
    } else {
        match crate::ostfix::Special::resolve(pst, &keep) {
            Ok(s) => Some(s),
            Err(e) => {
                o.problems.push(format!(
                    "The message store was copied as it was, which Outlook will show as \
                     an empty mailbox until SCANPST has been run over it: {e}"
                ));
                None
            }
        }
    };
    let mut rewritten: Vec<u32> = Vec::new();
    let all: std::collections::HashMap<u32, (u64, u64)> = keep
        .iter()
        .map(|n| (n.nid, (n.bid_data, n.bid_sub)))
        .collect();
    let lookup = |id: u32| all.get(&id).copied();

    for (i, n) in keep.iter().enumerate() {
        if !on(i as u64 + 1, keep.len() as u64) {
            return Err(crate::CANCELLED.into());
        }
        let what = format!("node 0x{:X}", n.nid);
        // A node whose data will not come out of the OST is left out and named, rather
        // than taking the whole conversion down with it. Its blocks may already be in the
        // output, unreferenced, which costs space and nothing else.
        // The store of an OST is rebuilt rather than copied; see `pst_store`. An ANSI
        // PST's store already is one, with a password its owner may have set.
        let built = match &fix {
            Some(fix) if n.nid == NID_MESSAGE_STORE => match fix.store_props(pst, n) {
                Ok(props) => {
                    rewritten.push(n.nid);
                    o.pc_node(pst, &props, n, &what)
                }
                Err(e) => {
                    o.problems.push(format!(
                        "The message store was copied as it was, which Outlook will show as \
                         an empty mailbox until SCANPST has been run over it: {e}"
                    ));
                    o.node(
                        pst,
                        (n.bid_data, n.bid_sub),
                        (n.nid, n.nid),
                        0,
                        &what,
                        &lookup,
                    )
                }
            },
            Some(fix) if n.nid_type() == 0x02 => match fix.folder_props(pst, n) {
                Ok(Some(props)) => {
                    rewritten.push(n.nid);
                    o.pc_node(pst, &props, n, &what)
                }
                Ok(None) => o.node(
                    pst,
                    (n.bid_data, n.bid_sub),
                    (n.nid, n.nid),
                    0,
                    &what,
                    &lookup,
                ),
                Err(e) => {
                    o.problems.push(format!(
                        "{what}: its folder links could not be re-pointed at this file ({e}); \
                         it was copied as it was and Outlook may not open it cleanly"
                    ));
                    o.node(
                        pst,
                        (n.bid_data, n.bid_sub),
                        (n.nid, n.nid),
                        0,
                        &what,
                        &lookup,
                    )
                }
            },
            _ => o.node(
                pst,
                (n.bid_data, n.bid_sub),
                (n.nid, n.nid),
                0,
                &what,
                &lookup,
            ),
        };
        match built {
            Ok((data, sub)) => {
                // NBTENTRY: the id, what holds its data, what holds its subnodes, and the
                // folder it belongs to.
                let mut e = vec![0u8; NBTENTRY];
                put64(&mut e, 0, n.nid as u64);
                put64(&mut e, 8, data);
                put64(&mut e, 16, sub);
                put32(&mut e, 24, n.nid_parent);
                nbt.push((n.nid as u64, e));
            }
            Err(e) => {
                dropped_nodes += 1;
                o.problems.push(format!("{what} was left out: {e}"));
            }
        }
    }
    if nbt.is_empty() {
        return Err("no node could be read out of this file, there is nothing to convert".into());
    }

    let have: std::collections::HashSet<u64> = nbt.iter().map(|(k, _)| *k).collect();
    let missing: Vec<String> = ESSENTIAL
        .iter()
        .filter(|(nid, _)| !have.contains(&(*nid as u64)))
        .map(|(nid, what)| format!("{what} (node 0x{nid:X})"))
        .collect();

    // The two trees go after the blocks, in the same layout a repair writes them in.
    let blocks = o.bbt.len();
    let node_count = nbt.len();
    let bbt = std::mem::take(&mut o.bbt);
    let mut next = o.pos;
    let mut next_bid = o.next << 2;
    let (nbt_bid, nbt_at, nbt_pages) =
        build_tree(nbt, NBTENTRY, PTYPE_NBT, &mut next, &mut next_bid);
    let (bbt_bid, bbt_at, bbt_pages) =
        build_tree(bbt, BBTENTRY, PTYPE_BBT, &mut next, &mut next_bid);

    for (at, page) in nbt_pages.iter().chain(bbt_pages.iter()) {
        pad(&mut o.f, &mut o.pos, *at).map_err(|e| o.io(e))?;
        o.f.write_all(page).map_err(|e| o.io(e))?;
        o.pos += page.len() as u64;
    }
    let eof = o.pos.div_ceil(BLOCK_ALIGN) * BLOCK_ALIGN;
    pad(&mut o.f, &mut o.pos, eof).map_err(|e| o.io(e))?;

    // Now the header, over the placeholder left at the front. Most of it is the source's
    // and none of this code's business; what changes is what the file now *is*.
    let mut header = pst.header_bytes()?;
    put16(&mut header, 8, MAGIC_PST); // wMagicClient: this is a PST now
    put16(&mut header, 10, VER_UNICODE); // wVer: 512-byte pages
    put16(&mut header, 12, VER_CLIENT); // wVerClient to match
    header[513] = 0; // bCryptMethod: the blocks went out plain
    put64(&mut header, 184, eof); // ibFileEof
    put64(&mut header, 216, nbt_bid); // ROOT.BREFNBT
    put64(&mut header, 224, nbt_at);
    put64(&mut header, 232, bbt_bid); // ROOT.BREFBBT
    put64(&mut header, 240, bbt_at);
    header[248] = 0; // fAMapValid = INVALID_AMAP
    put64(
        &mut header,
        192,
        AMAP_FIRST + (eof - AMAP_FIRST) / AMAP_EVERY * AMAP_EVERY,
    );
    put64(&mut header, 200, 0); // cbAMapFree
    put64(&mut header, 208, 0); // cbPMapFree
    put64(&mut header, OFF_BID_NEXT_P, next_bid);
    put64(&mut header, OFF_BID_NEXT_B, next_bid);
    // The partial checksum covers everything up to bidNextB, the full one takes that in
    // too, and neither range includes either checksum field. Computed before being
    // written, because the second range covers the first one's result.
    let partial = crc32(&header[8..8 + 471]);
    put32(&mut header, 4, partial);
    let full = crc32(&header[8..8 + 516]);
    put32(&mut header, 524, full);

    o.f.flush().map_err(|e| o.io(e))?;
    let mut f = o.f.into_inner().map_err(|e| format!("{out}: {e}"))?;
    f.seek(SeekFrom::Start(0))
        .map_err(|e| format!("{out}: {e}"))?;
    f.write_all(&header).map_err(|e| format!("{out}: {e}"))?;
    f.flush().map_err(|e| format!("{out}: {e}"))?;

    Ok(Rebuilt {
        nodes: node_count,
        blocks,
        dropped_blocks: 0,
        dropped_nodes,
        missing,
        bytes: eof,
        converted: true,
        source: if ansi {
            "an ANSI PST (Outlook 97-2002)"
        } else {
            "an Outlook 2013 OST"
        },
        problems: o.problems,
        rewritten,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ltp::read_node_pc;
    const PID_RECORD_KEY: u16 = 0x0FF9;
    const PID_DISPLAY_NAME: u16 = 0x3001;
    const PID_VALID_FOLDER_MASK: u16 = 0x35DF;
    const PID_IPM_SUBTREE: u16 = 0x35E0;
    const PID_WASTEBASKET: u16 = 0x35E3;
    const PID_FINDER: u16 = 0x35E7;
    const PID_PST_PASSWORD: u16 = 0x67FF;

    /// Every block of a node's data stream, so the two files can be held against each
    /// other at the level the conversion actually promises: the same bytes, in the same
    /// blocks, in the same order.
    fn stream(p: &mut Pst, bid: u64) -> Vec<Vec<u8>> {
        if bid == 0 {
            return Vec::new();
        }
        p.node_blocks(bid).expect("a stream should read back")
    }

    /// A heap is laid out again on the way through, so it is compared by what it holds:
    /// the same properties with the same values, or the same rows with the same cells.
    /// Anything that is not a heap must come through byte for byte, block for block.
    fn same_data(
        a: &mut Pst,
        xa: (u64, u64),
        b: &mut Pst,
        xb: (u64, u64),
        what: &str,
        la: Lookup,
        lb: Lookup,
    ) {
        let sa = stream(a, xa.0);
        let sb = stream(b, xb.0);
        let is_heap = |s: &Vec<Vec<u8>>| s.first().is_some_and(|f| f.len() >= 12 && f[2] == 0xEC);
        if is_heap(&sa) {
            match sa[0][3] {
                CLIENT_SIG_PC => {
                    let pa = crate::ltp::read_pc(a, xa.0, xa.1).expect("source pc");
                    let pb = crate::ltp::read_pc(b, xb.0, xb.1).expect("output pc");
                    assert_eq!(
                        format!("{:?}", pa.props),
                        format!("{:?}", pb.props),
                        "{what}: different properties"
                    );
                    return;
                }
                CLIENT_SIG_TC => {
                    let ta = crate::ltp::read_tc_with(a, xa.0, xa.1, Some(la)).expect("source tc");
                    let tb = crate::ltp::read_tc_with(b, xb.0, xb.1, Some(lb)).expect("output tc");
                    let rows = |t: crate::ltp::Tc| -> Vec<String> {
                        t.rows
                            .iter()
                            .map(|r| format!("{:X} {:?}", r.id, r.props))
                            .collect()
                    };
                    assert_eq!(rows(ta), rows(tb), "{what}: different rows");
                    return;
                }
                _ => {}
            }
        }
        assert_eq!(sa, sb, "{what}: holds different bytes");
    }

    /// Compare a subnode tree in the source against the one in the output: the same local
    /// ids, each with the same data and the same tree under it. The block ids differ by
    /// design, every one of them is new, so the comparison is on what they hold.
    ///
    /// Subnodes of type 0x1F are where a heap keeps values and row matrices too big for
    /// itself; a rebuilt heap may need more of them or fewer, so they are left out here
    /// and judged through the properties and rows that refer to them.
    fn same_subtree(
        a: &mut Pst,
        ab: u64,
        b: &mut Pst,
        bb: u64,
        what: &str,
        la: Lookup,
        lb: Lookup,
    ) {
        // An OST's auxiliary table heaps (client 0xA5) are folded into the table's own
        // heap on the way through, so they have no counterpart in the output either.
        let source: Vec<_> = a
            .subnodes(ab)
            .expect("source subnodes")
            .into_iter()
            .collect();
        let mut sa: Vec<_> = source
            .into_iter()
            .filter(|(k, _)| k & 0x1F != 0x1F)
            .filter(|(_, s)| {
                let blocks = a.node_blocks(s.data).unwrap_or_default();
                !blocks
                    .first()
                    .is_some_and(|f| f.len() >= 12 && f[2] == 0xEC && f[3] == 0xA5)
            })
            .collect();
        let mut sb: Vec<_> = b
            .subnodes(bb)
            .expect("output subnodes")
            .into_iter()
            .filter(|(k, _)| k & 0x1F != 0x1F)
            .collect();
        sa.sort_by_key(|(k, _)| *k);
        sb.sort_by_key(|(k, _)| *k);
        assert_eq!(
            sa.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            sb.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
            "{what}: different subnodes"
        );
        for ((nid, x), (_, y)) in sa.iter().zip(sb.iter()) {
            same_data(
                a,
                (x.data, x.sub),
                b,
                (y.data, y.sub),
                &format!("{what}: subnode 0x{nid:X}"),
                la,
                lb,
            );
            same_subtree(a, x.sub, b, y.sub, &format!("{what} / 0x{nid:X}"), la, lb);
        }
    }

    /// The milestone, checked at the level it is written at: an OST goes in, a PST comes
    /// out, and every node in it holds the same bytes in the same blocks.
    ///
    /// The block boundaries are asserted, not just the contents. A heap addresses its own
    /// allocations by block number, so a stream that came back with the same bytes split
    /// differently would parse and answer wrongly, which is the failure this whole module
    /// is arranged to avoid, and the one a bytes-only comparison would sail past.
    #[test]
    fn an_ost_converts_to_a_pst_holding_the_same_mail() {
        let src = "../../fixtures/data/example-2013.ost";
        if !std::path::Path::new(src).exists() {
            eprintln!("skipping: run tests/fetch-fixtures.ps1");
            return;
        }
        let mut ost = Pst::open(src).expect("fixture should open");
        assert!(!ost.is_small_page(), "the fixture should be a 4K-page OST");
        let nodes = ost.nodes();

        let out = std::env::temp_dir().join("ostpst-converted.pst");
        let out = out.to_str().unwrap();
        let r = convert(&mut ost, &nodes, out, &mut |_, _| true).expect("conversion should work");
        assert!(r.converted, "it should say it converted");
        assert!(r.missing.is_empty(), "converted without {:?}", r.missing);
        assert_eq!(r.dropped_nodes, 0, "a node was left behind");
        assert!(r.problems.is_empty(), "{:?}", r.problems);

        let mut pst = Pst::open(out).expect("the converted file should open");
        assert!(
            pst.is_small_page(),
            "the output should be a 512-byte-page file"
        );
        assert!(!pst.is_ost, "the output should say it is a PST");
        assert_eq!(pst.crypt, crate::ndb::Crypt::None, "blocks went out plain");

        // The maps go out invalid on purpose and a reader is right to say so; anything
        // else means a structure this wrote does not check out against its own checksum.
        let unexpected: Vec<_> = pst
            .warnings
            .iter()
            .filter(|w| !w.contains("allocation map"))
            .collect();
        assert!(unexpected.is_empty(), "{unexpected:?}");

        let after = pst.nodes();
        assert_eq!(after.len(), nodes.len(), "a node went missing");
        let by_nid: std::collections::BTreeMap<u32, Node> =
            after.iter().map(|n| (n.nid, *n)).collect();
        let la_map: std::collections::HashMap<u32, (u64, u64)> = nodes
            .iter()
            .map(|n| (n.nid, (n.bid_data, n.bid_sub)))
            .collect();
        let lb_map: std::collections::HashMap<u32, (u64, u64)> = after
            .iter()
            .map(|n| (n.nid, (n.bid_data, n.bid_sub)))
            .collect();
        let la: Lookup = &|id| la_map.get(&id).copied();
        let lb: Lookup = &|id| lb_map.get(&id).copied();

        for n in &nodes {
            let m = by_nid[&n.nid];
            assert_eq!(
                m.nid_parent, n.nid_parent,
                "node 0x{:X} changed parent",
                n.nid
            );
            // The store and the folders whose links were re-pointed are rebuilt on
            // purpose; they have tests of their own.
            if !r.rewritten.contains(&n.nid) {
                same_data(
                    &mut ost,
                    (n.bid_data, n.bid_sub),
                    &mut pst,
                    (m.bid_data, m.bid_sub),
                    &format!("node 0x{:X}", n.nid),
                    la,
                    lb,
                );
            }
            same_subtree(
                &mut ost,
                n.bid_sub,
                &mut pst,
                m.bid_sub,
                &format!("node 0x{:X}", n.nid),
                la,
                lb,
            );
        }
        let _ = std::fs::remove_file(out);
    }

    /// Issue #1: the converted store must say where the folders are, and must not ask
    /// for a password. Checked against the OST's own layout, and the entry id shape
    /// against a store Outlook itself wrote.
    #[test]
    fn a_converted_store_points_at_the_mailbox_and_has_no_password() {
        let (src, native) = (
            "../../fixtures/data/example-2013.ost",
            "../../fixtures/data/dist-list.pst",
        );
        if !std::path::Path::new(src).exists() || !std::path::Path::new(native).exists() {
            eprintln!("skipping: run tests/fetch-fixtures.ps1");
            return;
        }
        let bytes = |pc: &crate::ltp::Pc, id: u16| match pc.props.get(&id) {
            Some(crate::ltp::Value::Bytes(b)) => b.clone(),
            other => panic!("0x{id:04X} is {other:?}"),
        };
        let store = |p: &mut Pst| {
            let n = p.nodes().into_iter().find(|n| n.nid == 0x21).unwrap();
            read_node_pc(p, &n).expect("the store should read")
        };
        let entry = |key: &[u8], nid: u32| [&[0u8; 4][..], key, &nid.to_le_bytes()[..]].concat();

        // The shape, from a file Outlook wrote: its subtree is node 0x8022.
        let mut outlook = Pst::open(native).unwrap();
        let pc = store(&mut outlook);
        let key = bytes(&pc, PID_RECORD_KEY);
        assert_eq!(bytes(&pc, PID_IPM_SUBTREE), entry(&key, 0x8022));

        let mut ost = Pst::open(src).unwrap();
        let nodes = ost.nodes();
        let before = store(&mut ost);
        assert_eq!(before.int(PID_PST_PASSWORD), Some(-1), "the OST carries -1");
        assert!(!before.props.contains_key(&PID_IPM_SUBTREE));

        let out = std::env::temp_dir().join("ostpst-store.pst");
        let out = out.to_str().unwrap();
        let r = convert(&mut ost, &nodes, out, &mut |_, _| true).unwrap();
        assert!(r.problems.is_empty(), "{:?}", r.problems);

        let mut pst = Pst::open(out).unwrap();
        let after = store(&mut pst);
        let key = bytes(&after, PID_RECORD_KEY);
        assert_eq!(after.int(PID_PST_PASSWORD), Some(0));
        // Root - Mailbox\IPM_SUBTREE, its Deleted Items, and Root - Mailbox\Finder, then
        // the links a PST normally carries as well: Outbox, Sent Items, Views, Common
        // Views, Drafts and the item-processor search folder.
        assert_eq!(bytes(&after, PID_IPM_SUBTREE), entry(&key, 0x2142));
        assert_eq!(bytes(&after, PID_WASTEBASKET), entry(&key, 0x2162));
        assert_eq!(bytes(&after, PID_FINDER), entry(&key, 0x20E2));
        assert_eq!(bytes(&after, 0x35E2), entry(&key, 0x21A2));
        assert_eq!(bytes(&after, 0x35E4), entry(&key, 0x21C2));
        assert_eq!(bytes(&after, 0x35E5), entry(&key, 0x2122));
        assert_eq!(bytes(&after, 0x35E6), entry(&key, 0x20C2));
        assert_eq!(bytes(&after, 0x36D7), entry(&key, 0x8062));
        assert_eq!(bytes(&after, 0x3416), entry(&key, 0x80043));
        let mask = after.int(PID_VALID_FOLDER_MASK).unwrap();
        assert_eq!(mask & 0xFD, 0xFD, "mask 0x{mask:X}");
        assert_eq!(after.str(PID_DISPLAY_NAME), Some("Personal Folders"));
        // Nothing on the store names the OST's own store GUID any more.
        let ost_guid = bytes(&before, 0x6615);
        for (id, v) in &after.props {
            if let crate::ltp::Value::Bytes(b) = v {
                assert!(
                    !b.windows(16).any(|w| w == ost_guid.as_slice()),
                    "0x{id:04X} still carries the OST's store GUID"
                );
            }
        }
        assert!(
            !after.props.contains_key(&0x6812),
            "the OST's junk string is gone"
        );
        // Everything else the OST's store held comes across unchanged.
        let rewritten = [
            PID_IPM_SUBTREE,
            PID_WASTEBASKET,
            PID_FINDER,
            0x35E2,
            0x35E4,
            0x35E5,
            0x35E6,
            0x36D7,
            0x3416,
            PID_VALID_FOLDER_MASK,
            PID_PST_PASSWORD,
            PID_DISPLAY_NAME,
            0x6812,
            0x681B,
            0x8025,
            0x6615,
        ];
        for (id, v) in before
            .props
            .iter()
            .filter(|(id, _)| !rewritten.contains(id))
        {
            assert_eq!(
                format!("{:?}", Some(v)),
                format!("{:?}", after.props.get(id)),
                "0x{id:04X} changed"
            );
        }

        // The root folder and the Inbox now carry the special-folder links a PST keeps
        // there, each pointing at the OST's own folder of that role, and the Inbox has
        // lost the Exchange ids it used to carry.
        let folder = |p: &mut Pst, nid: u32| {
            let n = p.nodes().into_iter().find(|n| n.nid == nid).unwrap();
            read_node_pc(p, &n).expect("folder should read")
        };
        for nid in [0x122u32, 0x2182] {
            let pc = folder(&mut pst, nid);
            assert_eq!(
                bytes(&pc, 0x36D0),
                entry(&key, 0x81C2),
                "0x{nid:X}: Calendar"
            );
            assert_eq!(
                bytes(&pc, 0x36D1),
                entry(&key, 0x81E2),
                "0x{nid:X}: Contacts"
            );
            assert_eq!(
                bytes(&pc, 0x36D2),
                entry(&key, 0x8202),
                "0x{nid:X}: Journal"
            );
            assert_eq!(bytes(&pc, 0x36D3), entry(&key, 0x8222), "0x{nid:X}: Notes");
            assert_eq!(bytes(&pc, 0x36D4), entry(&key, 0x8242), "0x{nid:X}: Tasks");
            assert_eq!(
                bytes(&pc, 0x36D5),
                entry(&key, 0x80023),
                "0x{nid:X}: Reminders"
            );
            assert_eq!(bytes(&pc, 0x36D7), entry(&key, 0x8062), "0x{nid:X}: Drafts");
            assert!(
                !pc.props.contains_key(&0x36D8),
                "0x{nid:X}: AdditionalRenEntryIds gone"
            );
            for (id, v) in &pc.props {
                if let crate::ltp::Value::Bytes(b) = v {
                    assert!(
                        !b.windows(16).any(|w| w == ost_guid.as_slice()),
                        "folder 0x{nid:X} property 0x{id:04X} still carries the OST's GUID"
                    );
                }
            }
            // And the folder is still a folder: name, counts, subfolder flag intact.
            assert!(pc.props.contains_key(&0x3001) && pc.props.contains_key(&0x3602));
        }
        assert!(
            r.rewritten.contains(&0x21)
                && r.rewritten.contains(&0x122)
                && r.rewritten.contains(&0x2182)
        );
        let _ = std::fs::remove_file(out);
    }

    /// The two levels no fixture reaches: a stream too long for one XBLOCK, and a node
    /// with more subnodes than one SLBLOCK holds.
    ///
    /// A 20MB attachment needs the first and nothing smaller does, so it is exactly the
    /// path that would be found by a user rather than by a test. Driven directly, because
    /// manufacturing an OST big enough to reach it is a bigger job than checking the
    /// arithmetic that decides it.
    #[test]
    fn the_second_level_of_each_tree_is_built_when_one_is_not_enough() {
        let path = std::env::temp_dir().join("ostpst-levels.bin");
        let path = path.to_str().unwrap();

        // Enough pieces for three XBLOCKs and an XXBLOCK over them.
        let mut o = Out::new(path, 1, false).expect("temp file");
        let parts: Vec<(u64, u64)> = (0..X_ENTRIES as u64 * 2 + 5)
            .map(|i| (i * 4, 100))
            .collect();
        let before = o.bbt.len();
        o.xblock(&parts).expect("an XXBLOCK should be built");
        let made = o.bbt.len() - before;
        assert_eq!(made, 4, "three XBLOCKs and one XXBLOCK");
        let last = o.bbt.len() - 1;
        let top = read_back(&mut o, path, last);
        assert_eq!(top[0], 0x01, "the top of a data tree is btype 0x01");
        assert_eq!(top[1], 2, "and level 2, which is what makes it an XXBLOCK");
        assert_eq!(
            u16::from_le_bytes([top[2], top[3]]),
            3,
            "over three XBLOCKs"
        );
        assert_eq!(
            u32::from_le_bytes([top[4], top[5], top[6], top[7]]) as u64,
            parts.len() as u64 * 100,
            "an XXBLOCK counts every byte underneath it"
        );

        // And enough subnodes for two SLBLOCKs under an SIBLOCK.
        let entries: Vec<(u32, u64, u64)> = (0..SL_ENTRIES as u32 + 1).map(|i| (i, 4, 0)).collect();
        let before = o.bbt.len();
        o.slblock(&entries).expect("an SIBLOCK should be built");
        let made = o.bbt.len() - before;
        assert_eq!(made, 3, "two SLBLOCKs and one SIBLOCK");
        let last = o.bbt.len() - 1;
        let top = read_back(&mut o, path, last);
        assert_eq!(top[0], 0x02, "a subnode tree is btype 0x02");
        assert_eq!(top[1], 1, "and level 1, which is what makes it an SIBLOCK");
        assert_eq!(u16::from_le_bytes([top[2], top[3]]), 2, "over two SLBLOCKs");

        let _ = std::fs::remove_file(path);
    }

    /// Pull one written block's data back off disk, by the offset and length its own BBT
    /// entry records.
    fn read_back(o: &mut Out, path: &str, which: usize) -> Vec<u8> {
        o.f.flush().expect("flush");
        let e = &o.bbt[which].1;
        let at = u64::from_le_bytes(e[8..16].try_into().unwrap());
        let cb = u16::from_le_bytes([e[16], e[17]]) as usize;
        let all = std::fs::read(path).expect("the temp file should read");
        all[at as usize..at as usize + cb].to_vec()
    }
}
