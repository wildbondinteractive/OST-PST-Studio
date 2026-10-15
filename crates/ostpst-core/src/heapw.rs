//! Writing heaps-on-node the way a Unicode PST stores them.
//!
//! A heap-on-node (MS-PST 2.3.1) is how every property context and every table is kept:
//! a node's data blocks, each with a small allocation map at its end, and 32-bit heap
//! ids that name an allocation by *which block* it is in and *which entry* of that
//! block's map it is. A PST packs the block number into the top 16 bits of the id and
//! the entry into the 11 bits below it. An Outlook 2013+ OST, whose blocks are up to
//! eight times larger and hold far more allocations, uses 13 and 14 bits instead. So a
//! heap cannot be carried from one format to the other block for block: the ids inside
//! it would all mean something else in the new file.
//!
//! This module rebuilds a heap instead. It reads every allocation out of the source heap
//! with the source's id layout, and writes a fresh one with the PST's layout, blocks of
//! at most 8176 bytes and allocations of at most 3580 (MS-PST 2.3.1.2), moving anything
//! larger out into a subnode as Outlook itself does. For a property context that means
//! re-serialising the property B-tree; for a table context it means re-pointing every
//! variable cell in the row matrix and re-packing the rows, because the number of rows
//! a block holds is derived from the block size too.

use crate::convert::Hnid;
use crate::ltp::{
    bth_records_at, column_homes, is_inline, is_variable, parse_tcinfo, row_blocks, table_heaps,
    Heap, Lookup, CLIENT_SIG_PC, CLIENT_SIG_TC,
};
use crate::ndb::Pst;
use crate::ostfix::RawProps;
use std::collections::HashSet;

/// The most a PST data block can hold: 8KB less the 16-byte trailer. MS-PST 2.2.2.8.
pub const MAX_DATA: usize = 8176;
/// The largest single heap allocation Outlook makes. MS-PST 2.3.1.2.
pub const MAX_ALLOC: usize = 3580;
/// Allocation index is 11 bits and 0 is reserved.
const MAX_INDEX: usize = 2047;
/// The node id type Outlook gives subnodes that hold values too big for the heap.
const NID_TYPE_LTP: u32 = 0x1F;

/// A heap laid out for a PST, plus everything that had to leave it.
pub struct RebuiltHeap {
    /// The heap's data blocks, in order. Every one fits a PST block.
    pub blocks: Vec<Vec<u8>>,
    /// Subnodes to add to the node's subnode tree, or to replace there: the local id,
    /// and the data blocks exactly as they must be written. Never re-chunked, because a
    /// table's row matrix is addressed by block and rows may not straddle one.
    pub spilled: Vec<(u32, Vec<Vec<u8>>)>,
    /// Subnodes of the source that nothing refers to any more.
    pub dropped: Vec<u32>,
    /// Heap ids in the source that pointed nowhere. Their values are gone; each is
    /// counted so the report can say so.
    pub unresolved: usize,
}

/// A PST heap id: block number above, allocation index below, low five bits zero.
pub fn hid(block: usize, index: usize) -> u32 {
    ((block as u32) << 16) | ((index as u32) << 5)
}

/// Accumulates allocations into blocks and hands out their ids.
pub struct HeapBuilder {
    client_sig: u8,
    blocks: Vec<Vec<Vec<u8>>>,
}

/// HNHDR on the first block, HNBITMAPHDR on block 8 and every 128th after it,
/// HNPAGEHDR elsewhere. MS-PST 2.3.1.2 to 2.3.1.4.
fn header_len(block: usize) -> usize {
    if block == 0 {
        12
    } else if block % 128 == 8 {
        66
    } else {
        2
    }
}

impl HeapBuilder {
    pub fn new(client_sig: u8) -> HeapBuilder {
        HeapBuilder {
            client_sig,
            blocks: vec![Vec::new()],
        }
    }

    fn fits(&self, block: usize, len: usize) -> bool {
        let n = self.blocks[block].len();
        if n >= MAX_INDEX {
            return false;
        }
        let body: usize = self.blocks[block].iter().map(|a| a.len()).sum();
        let mut total = header_len(block) + body + len;
        total += total % 2;
        total + 4 + 2 * (n + 2) <= MAX_DATA
    }

    /// Add an allocation and get the id that will name it.
    pub fn alloc(&mut self, data: &[u8]) -> Result<u32, String> {
        if data.len() > MAX_ALLOC {
            return Err(format!(
                "a {}-byte heap allocation was requested and {MAX_ALLOC} is the most a \
                 PST heap holds; this is a bug in the converter",
                data.len()
            ));
        }
        let mut b = self.blocks.len() - 1;
        if !self.fits(b, data.len()) {
            self.blocks.push(Vec::new());
            b += 1;
        }
        self.blocks[b].push(data.to_vec());
        Ok(hid(b, self.blocks[b].len()))
    }

    /// Replace the bytes of an allocation made earlier, with the same length. This is
    /// how a header that names things allocated after it gets its final contents.
    pub fn patch(&mut self, id: u32, data: &[u8]) {
        let (b, i) = ((id >> 16) as usize, ((id >> 5) & 0x7FF) as usize);
        let slot = &mut self.blocks[b][i - 1];
        assert_eq!(
            slot.len(),
            data.len(),
            "patching a heap allocation to a new size"
        );
        slot.copy_from_slice(data);
    }

    /// Lay the blocks out: header, allocations back to back, a pad byte if needed, then
    /// the page map (count, free count, and count+1 offsets).
    pub fn finish(self, user_root: u32) -> Vec<Vec<u8>> {
        let mut out: Vec<Vec<u8>> = Vec::with_capacity(self.blocks.len());
        for (b, allocs) in self.blocks.iter().enumerate() {
            let mut buf = vec![0u8; header_len(b)];
            let mut offsets = Vec::with_capacity(allocs.len() + 1);
            for a in allocs {
                offsets.push(buf.len() as u16);
                buf.extend_from_slice(a);
            }
            offsets.push(buf.len() as u16);
            if buf.len() % 2 == 1 {
                buf.push(0);
            }
            let map = buf.len();
            buf.extend_from_slice(&(allocs.len() as u16).to_le_bytes());
            buf.extend_from_slice(&0u16.to_le_bytes());
            for o in &offsets {
                buf.extend_from_slice(&o.to_le_bytes());
            }
            buf[0..2].copy_from_slice(&(map as u16).to_le_bytes());
            out.push(buf);
        }
        // Fill levels: one nibble a block, low nibble first. Advisory, kept honest.
        let fill: Vec<u8> = out.iter().map(|b| fill_level(MAX_DATA - b.len())).collect();
        let nibbles = |from: usize, count: usize| -> Vec<u8> {
            (0..count / 2)
                .map(|i| {
                    let lo = fill.get(from + 2 * i).copied().unwrap_or(0);
                    let hi = fill.get(from + 2 * i + 1).copied().unwrap_or(0);
                    lo | (hi << 4)
                })
                .collect()
        };
        for b in 0..out.len() {
            if b == 0 {
                out[0][2] = 0xEC;
                out[0][3] = self.client_sig;
                out[0][4..8].copy_from_slice(&user_root.to_le_bytes());
                let f = nibbles(0, 8);
                out[0][8..12].copy_from_slice(&f);
            } else if b % 128 == 8 {
                let f = nibbles(b, 128);
                out[b][2..66].copy_from_slice(&f);
            }
        }
        out
    }
}

/// How much room is left in a heap block, in the sixteen steps MS-PST 2.3.1.2 counts in.
fn fill_level(free: usize) -> u8 {
    const AT_LEAST: [usize; 15] = [
        3584, 2560, 2048, 1792, 1536, 1280, 1024, 768, 512, 256, 128, 64, 32, 16, 8,
    ];
    AT_LEAST.iter().position(|&n| free >= n).unwrap_or(15) as u8
}

/// Write a B-tree-on-heap over records and return the id of its header. The records
/// are sorted by key here, because a reader walks a tree's pages in whatever order it
/// likes and a tree must be searchable by key whatever order it was read in. `header`
/// is a placeholder allocated earlier to patch, when the caller wants the header to sit
/// first in the heap as Outlook puts it.
pub fn bth(
    hb: &mut HeapBuilder,
    key: usize,
    ent: usize,
    records: &[Vec<u8>],
    header: Option<u32>,
) -> Result<u32, String> {
    let mut records: Vec<&Vec<u8>> = records.iter().filter(|r| r.len() >= key + ent).collect();
    // Keys are little-endian integers, so numeric order is byte order read backwards.
    records.sort_by(|a, b| a[..key].iter().rev().cmp(b[..key].iter().rev()));
    let records = records;
    let (mut root, mut levels) = (0u32, 0u8);
    if !records.is_empty() {
        let per = MAX_ALLOC / (key + ent);
        let mut level: Vec<(Vec<u8>, u32)> = Vec::new();
        for page in records.chunks(per) {
            let bytes: Vec<u8> = page.iter().flat_map(|r| r.iter().copied()).collect();
            level.push((page[0][..key].to_vec(), hb.alloc(&bytes)?));
        }
        while level.len() > 1 {
            let per = MAX_ALLOC / (key + 4);
            let mut up = Vec::new();
            for page in level.chunks(per) {
                let mut bytes = Vec::with_capacity(page.len() * (key + 4));
                for (k, h) in page {
                    bytes.extend_from_slice(k);
                    bytes.extend_from_slice(&h.to_le_bytes());
                }
                up.push((page[0].0.clone(), hb.alloc(&bytes)?));
            }
            level = up;
            levels += 1;
        }
        root = level[0].1;
    }
    let mut h = vec![0xB5, key as u8, ent as u8, levels];
    h.extend_from_slice(&root.to_le_bytes());
    match header {
        Some(id) => {
            hb.patch(id, &h);
            Ok(id)
        }
        None => hb.alloc(&h),
    }
}

/// Whether a heap of a type this module cannot take apart would still read correctly
/// if its blocks were copied as they are: one block a PST can hold, with few enough
/// allocations that both id layouts name them identically.
pub(crate) fn copies_cleanly(blocks: &[Vec<u8>]) -> bool {
    let [b] = blocks else { return false };
    if b.len() > MAX_DATA || b.len() < 12 {
        return false;
    }
    let map = u16::from_le_bytes([b[0], b[1]]) as usize;
    if map + 2 > b.len() {
        return false;
    }
    (u16::from_le_bytes([b[map], b[map + 1]]) as usize) <= MAX_INDEX
}

/// A local id for a new subnode that collides with nothing in the tree.
fn fresh_nid(subs: &mut HashSet<u32>) -> u32 {
    let mut i = 1u32;
    loop {
        let nid = (i << 5) | NID_TYPE_LTP;
        if subs.insert(nid) {
            return nid;
        }
        i += 1;
    }
}

fn chunked(bytes: &[u8]) -> Vec<Vec<u8>> {
    bytes.chunks(MAX_DATA).map(<[u8]>::to_vec).collect()
}

/// A property context from its properties: B-tree header first, as Outlook writes it,
/// then the values, then the B-tree's pages. Values over the allocation limit become
/// subnodes.
pub(crate) fn write_pc(props: &RawProps, subs: &mut HashSet<u32>) -> Result<RebuiltHeap, String> {
    let mut hb = HeapBuilder::new(CLIENT_SIG_PC);
    let header = hb.alloc(&[0u8; 8])?;
    let mut spilled = Vec::new();
    let mut records = Vec::with_capacity(props.len());
    for (id, (ptype, v)) in props {
        let hnid = match v {
            Hnid::Inline(x) | Hnid::Sub(x) => *x,
            Hnid::Heap(b) if b.is_empty() => 0,
            Hnid::Heap(b) if b.len() <= MAX_ALLOC => hb.alloc(b)?,
            Hnid::Heap(b) => {
                let nid = fresh_nid(subs);
                spilled.push((nid, chunked(b)));
                nid
            }
        };
        let mut r = Vec::with_capacity(8);
        r.extend_from_slice(&id.to_le_bytes());
        r.extend_from_slice(&ptype.to_le_bytes());
        r.extend_from_slice(&hnid.to_le_bytes());
        records.push(r);
    }
    let root = bth(&mut hb, 2, 6, &records, Some(header))?;
    Ok(RebuiltHeap {
        blocks: hb.finish(root),
        spilled,
        dropped: Vec::new(),
        unresolved: 0,
    })
}

/// Read every property out of a source heap, with the source's own id layout.
pub(crate) fn read_raw_props(heap: &Heap) -> Result<(RawProps, usize), String> {
    if heap.client_sig != CLIENT_SIG_PC {
        return Err(format!(
            "heap holds client type 0x{:02X}, not a property context",
            heap.client_sig
        ));
    }
    let (key, records) = bth_records_at(heap, heap.user_root)?;
    if key != 2 {
        return Err(format!("property B-tree has {key}-byte keys, expected 2"));
    }
    let mut props = RawProps::new();
    let mut unresolved = 0;
    for r in records.iter().filter(|r| r.len() >= 8) {
        let id = u16::from_le_bytes([r[0], r[1]]);
        let ptype = u16::from_le_bytes([r[2], r[3]]);
        let raw = u32::from_le_bytes([r[4], r[5], r[6], r[7]]);
        let v = if is_inline(ptype) {
            Hnid::Inline(raw)
        } else if raw & 0x1F != 0 {
            Hnid::Sub(raw)
        } else {
            match heap.item(raw) {
                Ok(b) => Hnid::Heap(b.to_vec()),
                Err(_) => {
                    unresolved += 1;
                    continue;
                }
            }
        };
        props.insert(id, (ptype, v));
    }
    Ok((props, unresolved))
}

/// A property context from another format, laid out again for a PST.
pub(crate) fn rebuild_pc(heap: &Heap, subs: &mut HashSet<u32>) -> Result<RebuiltHeap, String> {
    let (props, unresolved) = read_raw_props(heap)?;
    let mut out = write_pc(&props, subs)?;
    out.unresolved = unresolved;
    Ok(out)
}

/// A table context from another format, laid out again for a PST.
///
/// The table header and its column descriptors are kept as they are; the rows keep
/// their order, their ids and every fixed-size cell. What changes is where each
/// variable cell's bytes live, where the row index B-tree and the row matrix live,
/// and how the row matrix is cut into blocks. Cells that an OST kept in an auxiliary
/// heap are brought into the table's own, and the auxiliary subnodes are dropped, as
/// a PST has neither.
pub(crate) fn rebuild_tc(
    pst: &mut Pst,
    bid_data: u64,
    bid_sub: u64,
    subs: &mut HashSet<u32>,
    lookup: Option<Lookup>,
) -> Result<RebuiltHeap, String> {
    let heaps = table_heaps(pst, bid_data, bid_sub)?;
    let mut info = heaps.main.item(heaps.main.user_root)?.to_vec();
    let tc = parse_tcinfo(&info)?;
    let old_rows = row_blocks(pst, &heaps, &tc, bid_sub)?;
    let homes = column_homes(pst, &heaps, &tc, &old_rows, lookup);
    let width = tc.width;

    let (key, index_records) = if tc.hid_row_index == 0 {
        (4, Vec::new())
    } else {
        bth_records_at(&heaps.main, tc.hid_row_index)?
    };
    if key != 4 {
        return Err(format!("row index B-tree has {key}-byte keys, expected 4"));
    }

    let mut hb = HeapBuilder::new(CLIENT_SIG_TC);
    let index_header = hb.alloc(&[0u8; 8])?;
    let tcinfo = hb.alloc(&info)?;
    let mut spilled = Vec::new();
    let mut unresolved = 0;

    let mut rows: Vec<u8> = Vec::new();
    for b in &old_rows {
        for r in b.chunks_exact(width) {
            let mut row = r.to_vec();
            for (ci, c) in tc.columns.iter().enumerate() {
                if !is_variable(c.ptype) || c.cb < 4 || !c.present(&row, tc.ceb_start) {
                    continue;
                }
                let (ib, bit) = (c.ib, c.bit);
                let old = u32::from_le_bytes([row[ib], row[ib + 1], row[ib + 2], row[ib + 3]]);
                if old == 0 || old & 0x1F != 0 {
                    continue; // nothing, or a subnode, which keeps its id
                }
                let new = match heaps.heap(homes[ci]).item(old) {
                    Ok(&[]) => 0,
                    Ok(bytes) if bytes.len() <= MAX_ALLOC => hb.alloc(bytes)?,
                    Ok(bytes) => {
                        let nid = fresh_nid(subs);
                        spilled.push((nid, chunked(bytes)));
                        nid
                    }
                    Err(_) => {
                        unresolved += 1;
                        row[tc.ceb_start + bit / 8] &= !(0x80 >> (bit % 8));
                        0
                    }
                };
                row[ib..ib + 4].copy_from_slice(&new.to_le_bytes());
            }
            rows.extend_from_slice(&row);
        }
    }

    bth(&mut hb, 4, 4, &index_records, Some(index_header))?;

    let mut dropped: Vec<u32> = heaps.aux.iter().map(|(nid, _)| *nid).collect();
    let hnid_rows = tc.hnid_rows;
    let rows_ref = if rows.is_empty() {
        if hnid_rows & 0x1F != 0 {
            dropped.push(hnid_rows);
        }
        0
    } else if rows.len() <= MAX_ALLOC {
        if hnid_rows & 0x1F != 0 {
            dropped.push(hnid_rows);
        }
        hb.alloc(&rows)?
    } else {
        // Rows per block follows from the block size, so the matrix is cut again here.
        let per = MAX_DATA / width;
        let chunks: Vec<Vec<u8>> = rows.chunks(per * width).map(<[u8]>::to_vec).collect();
        let nid = if hnid_rows & 0x1F != 0 {
            hnid_rows
        } else {
            fresh_nid(subs)
        };
        spilled.push((nid, chunks));
        nid
    };
    info[10..14].copy_from_slice(&index_header.to_le_bytes());
    info[14..18].copy_from_slice(&rows_ref.to_le_bytes());
    hb.patch(tcinfo, &info);

    Ok(RebuiltHeap {
        blocks: hb.finish(tcinfo),
        spilled,
        dropped,
        unresolved,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ltp::Heap;

    #[test]
    fn fill_levels_follow_the_spec_table() {
        assert_eq!(fill_level(8000), 0);
        assert_eq!(fill_level(3583), 1);
        assert_eq!(fill_level(8), 14);
        assert_eq!(fill_level(7), 15);
    }

    #[test]
    fn allocations_fill_blocks_and_read_back() {
        let mut hb = HeapBuilder::new(CLIENT_SIG_PC);
        let mut ids = Vec::new();
        for i in 0..40u32 {
            let bytes = vec![i as u8; 1000];
            ids.push((hb.alloc(&bytes).unwrap(), bytes));
        }
        let blocks = hb.finish(ids[0].0);
        assert!(blocks.len() > 1);
        assert!(blocks.iter().all(|b| b.len() <= MAX_DATA));
        let heap = Heap::new(blocks).unwrap();
        for (id, bytes) in &ids {
            assert_eq!(heap.item(*id).unwrap(), &bytes[..]);
        }
    }

    #[test]
    fn bitmap_header_block_keeps_its_map() {
        let mut hb = HeapBuilder::new(CLIENT_SIG_TC);
        let mut ids = Vec::new();
        // Two allocations a block, so block 8 is reached quickly.
        for i in 0..30u32 {
            ids.push((hb.alloc(&vec![i as u8; 3500]).unwrap(), i as u8));
        }
        let blocks = hb.finish(ids[0].0);
        assert!(blocks.len() > 9, "{}", blocks.len());
        assert_eq!(
            u16::from_le_bytes([blocks[8][0], blocks[8][1]]) as usize,
            66 + 2 * 3500
        );
        let heap = Heap::new(blocks).unwrap();
        for (id, v) in ids {
            let item = heap.item(id).unwrap();
            assert_eq!(item.len(), 3500);
            assert!(item.iter().all(|&b| b == v));
        }
    }

    #[test]
    fn bth_with_an_index_level_reads_back_in_order() {
        let mut hb = HeapBuilder::new(CLIENT_SIG_PC);
        let records: Vec<Vec<u8>> = (0..3000u32)
            .map(|k| {
                let mut r = k.to_le_bytes().to_vec();
                r.extend_from_slice(&(k * 7).to_le_bytes());
                r
            })
            .collect();
        let root = bth(&mut hb, 4, 4, &records, None).unwrap();
        let heap = Heap::new(hb.finish(root)).unwrap();
        let (key, mut back) = bth_records_at(&heap, root).unwrap();
        assert_eq!(key, 4);
        back.sort();
        let mut expected = records.clone();
        expected.sort();
        assert_eq!(back, expected);
    }

    #[test]
    fn bth_sorts_its_records_by_key() {
        let mut hb = HeapBuilder::new(CLIENT_SIG_PC);
        let records: Vec<Vec<u8>> = [300u32, 5, 70000, 1]
            .iter()
            .map(|k| [k.to_le_bytes(), [0, 0, 0, 0]].concat())
            .collect();
        let root = bth(&mut hb, 4, 4, &records, None).unwrap();
        let heap = Heap::new(hb.finish(root)).unwrap();
        let (_, back) = bth_records_at(&heap, root).unwrap();
        let keys: Vec<u32> = back
            .iter()
            .map(|r| u32::from_le_bytes([r[0], r[1], r[2], r[3]]))
            .collect();
        assert_eq!(keys, vec![1, 5, 300, 70000]);
    }

    #[test]
    fn a_property_context_round_trips_with_a_spilled_value() {
        let mut props = RawProps::new();
        props.insert(0x0037, (0x001F, Hnid::Heap(vec![b'a'; 100])));
        props.insert(0x0E07, (0x0003, Hnid::Inline(5)));
        props.insert(0x1000, (0x001F, Hnid::Heap(vec![b'b'; 10_000])));
        let mut subs = HashSet::new();
        let rb = write_pc(&props, &mut subs).unwrap();
        assert_eq!(rb.spilled.len(), 1);
        assert_eq!(rb.spilled[0].1.concat().len(), 10_000);
        let heap = Heap::new(rb.blocks).unwrap();
        let (p2, unresolved) = read_raw_props(&heap).unwrap();
        assert_eq!(unresolved, 0);
        assert!(matches!(p2.get(&0x0037), Some((0x001F, Hnid::Heap(b))) if b.len() == 100));
        assert!(matches!(p2.get(&0x0E07), Some((0x0003, Hnid::Inline(5)))));
        assert!(matches!(p2.get(&0x1000), Some((0x001F, Hnid::Sub(n))) if *n == rb.spilled[0].0));
    }
}
