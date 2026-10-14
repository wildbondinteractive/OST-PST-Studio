//! Allocation maps for a written PST, so Outlook opens it without SCANPST first.
//!
//! The rebuild and conversion writers lay the file out and mark the header
//! `INVALID_AMAP`, which MS-PST 2.6.1.3.7 defines as "rebuild the maps before you
//! allocate". Outlook reads that as a file that was not closed properly and sends the
//! user to the Inbox Repair Tool. This module does the rebuild itself, exactly as the
//! specification describes it: mark every map free, then walk the block B-tree and
//! both B-trees' pages and mark what they occupy as allocated, plus the map pages
//! themselves.
//!
//! What is written, all from MS-PST 2.2.2.7:
//!
//! - **AMap** (ptype 0x84) at 0x4400 and every 253,952 bytes: 496 bytes of bits, one
//!   per 64 bytes of the section that starts at the page itself. Its own first byte is
//!   0xFF because the map maps itself.
//! - **PMap** (ptype 0x83) at 0x4600 and every 2,031,616 bytes (every eighth AMap):
//!   one bit per 512-byte page, 0 meaning the page is free. Deprecated, but MUST be
//!   present with a valid checksum for older clients.
//! - **FMap** (ptype 0x82): one byte per AMap, the longest run of free bits in it. The
//!   header's own 128-byte `rgbFM` is deprecated and filled with 0xFF; the page form
//!   covers 496 AMaps each.
//! - **FPMap** (ptype 0x85): one bit per PMap, 1 meaning no free pages. 496 bytes cover
//!   3,968 PMaps.
//! - **DList** (ptype 0x86) at 0x4200: AMap page numbers sorted by free space.
//! - **ROOT**: `fAMapValid = VALID_AMAP2`, `cbAMapFree`, `cbPMapFree = 0`,
//!   `ibAMapLast`, and both header CRCs recomputed.
//!
//! **Where FMap and FPMap pages recur is the one thing the specification only draws.**
//! The prose fixes what the header's built-in maps cover (the first 128 AMaps and the
//! first 1,024 PMaps) and what a page covers (496 AMaps, 3,968 PMaps). From those, the
//! first FMap page belongs with AMap number 128 and the next with 624, 1120 and so on;
//! the first FPMap page with AMap number 8,192 (PMap 1,024) and the next 31,744 AMaps
//! later. Figure 3 shows the four map kinds in the order AMap, PMap, FMap, FPMap at the
//! head of a section, and every writer in this project keeps all four 512-byte slots at
//! the head of every section clear of data, so an FMap is written in the third slot and
//! an FPMap in the fourth. That placement is derived rather than observed: no public PST
//! is large enough to contain either page (125MB and 2GB respectively). Outlook 2007
//! SP2 and later do not read FMaps or FPMaps at all (they use the DList), so for modern
//! Outlook what matters is the AMaps, the PMaps and the header, all of which the
//! specification states exactly.

use crate::ndb::{crc32, Pst};
use crate::repair::{put16, put32, put64, AMAP_EVERY, AMAP_FIRST, PAGE};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

const PTYPE_FMAP: u8 = 0x82;
const PTYPE_PMAP: u8 = 0x83;
const PTYPE_AMAP: u8 = 0x84;
const PTYPE_FPMAP: u8 = 0x85;
const PTYPE_DLIST: u8 = 0x86;
const MAP_BYTES: usize = 496;
const TRAILER_AT: usize = 496;
/// AMaps per PMap, FMaps per page, and the header's own coverage.
const AMAPS_PER_PMAP: u64 = 8;
const AMAPS_PER_FMAP_PAGE: u64 = 496;
const HEADER_FMAP_COVERS: u64 = 128;
const PMAPS_PER_FPMAP_PAGE: u64 = 496 * 8;
const HEADER_FPMAP_COVERS: u64 = 1024;
const DLIST_AT: u64 = 0x4200;
const DLIST_MAX_ENTRIES: usize = 476 / 4;
const OFF_IB_FILE_EOL: usize = 184;
const OFF_IB_AMAP_LAST: usize = 192;
const OFF_CB_AMAP_FREE: usize = 200;
const OFF_CB_PMAP_FREE: usize = 208;
const OFF_F_AMAP_VALID: usize = 248;
const VALID_AMAP2: u8 = 0x02;
const OFF_BID_NEXT_P: usize = 32;

/// One section's maps, built in memory before being written.
struct Section {
    /// Absolute offset of the AMap page, which is the start of the section.
    at: u64,
    bits: [u8; MAP_BYTES],
}

impl Section {
    fn mark(&mut self, from: u64, len: u64) {
        let start = self.at;
        let end = start + AMAP_EVERY;
        let lo = from.max(start);
        let hi = (from + len).min(end);
        if lo >= hi {
            return;
        }
        let first = (lo - start) / 64;
        let last = (hi - start).div_ceil(64);
        for bit in first..last {
            self.bits[(bit / 8) as usize] |= 0x80 >> (bit % 8);
        }
    }
    fn free_slots(&self) -> u32 {
        self.bits.iter().map(|b| b.count_zeros()).sum()
    }
    fn longest_free_run(&self) -> u8 {
        let (mut best, mut run) = (0u32, 0u32);
        for byte in self.bits {
            for i in 0..8 {
                if byte & (0x80 >> i) == 0 {
                    run += 1;
                    best = best.max(run);
                } else {
                    run = 0;
                }
            }
        }
        best.min(255) as u8
    }
    /// True when the 512-byte page at `page` (absolute) has no allocated 64-byte slot.
    fn page_free(&self, page: u64) -> bool {
        let bit = (page - self.at) / 64;
        (bit..bit + 8).all(|b| self.bits[(b / 8) as usize] & (0x80 >> (b % 8)) == 0)
    }
}

/// Which map pages a section at AMap index `i` carries, beyond the AMap itself.
fn has_pmap(i: u64) -> bool {
    i.is_multiple_of(AMAPS_PER_PMAP)
}
fn has_fmap(i: u64) -> bool {
    i >= HEADER_FMAP_COVERS && (i - HEADER_FMAP_COVERS).is_multiple_of(AMAPS_PER_FMAP_PAGE)
}
fn has_fpmap(i: u64) -> bool {
    let pmaps_before_header_end = HEADER_FPMAP_COVERS * AMAPS_PER_PMAP;
    i >= pmaps_before_header_end
        && (i - pmaps_before_header_end).is_multiple_of(PMAPS_PER_FPMAP_PAGE * AMAPS_PER_PMAP)
}

/// A map page: 496 bytes of data and a trailer whose id is its own offset.
fn map_page(ptype: u8, at: u64, data: &[u8; MAP_BYTES]) -> Vec<u8> {
    let mut page = vec![0u8; PAGE as usize];
    page[..MAP_BYTES].copy_from_slice(data);
    page[TRAILER_AT] = ptype;
    page[TRAILER_AT + 1] = ptype;
    put16(&mut page, TRAILER_AT + 2, 0); // wSig is zero for map pages
    put64(&mut page, TRAILER_AT + 8, at); // bid == ib by convention
    let crc = crc32(&page[..TRAILER_AT]);
    put32(&mut page, TRAILER_AT + 4, crc);
    page
}

/// Rebuild every allocation structure of a Unicode 512-byte-page PST in place and mark
/// the header valid. Returns a one-line summary for the report.
pub fn write_allocation_maps(path: &Path) -> Result<String, String> {
    let path_str = path.to_string_lossy().to_string();
    let mut pst = Pst::open(&path_str)?;
    if !pst.is_small_page() || pst.is_ost {
        return Err("allocation maps are only written for a Unicode 512-byte-page PST".into());
    }
    let eof = pst.actual_len;
    if eof < AMAP_FIRST + PAGE {
        return Err("the file is too short to hold an allocation map".into());
    }

    // Everything the file occupies: every block, every B-tree page.
    let blocks = pst.blocks();
    let pages = pst.page_offsets();
    if blocks.is_empty() || pages.is_empty() {
        return Err("the block index could not be read, so nothing can be mapped".into());
    }

    let sections_n = (eof - AMAP_FIRST).div_ceil(AMAP_EVERY);
    let mut sections: Vec<Section> = (0..sections_n)
        .map(|i| Section {
            at: AMAP_FIRST + i * AMAP_EVERY,
            bits: [0u8; MAP_BYTES],
        })
        .collect();
    let section_of = |at: u64| -> Option<usize> {
        if at < AMAP_FIRST {
            None
        } else {
            Some(((at - AMAP_FIRST) / AMAP_EVERY) as usize)
        }
    };
    let mark = |sections: &mut Vec<Section>, from: u64, len: u64| {
        let Some(first) = section_of(from) else {
            return;
        };
        let last = section_of(from + len - 1).unwrap_or(first);
        for s in first..=last.min(sections.len() - 1) {
            sections[s].mark(from, len);
        }
    };

    for b in &blocks {
        let span = (b.cb as u64 + 16).div_ceil(64) * 64;
        mark(&mut sections, b.ib, span);
    }
    for at in &pages {
        mark(&mut sections, *at, PAGE);
    }
    // The map pages map themselves.
    for i in 0..sections_n {
        let at = AMAP_FIRST + i * AMAP_EVERY;
        mark(&mut sections, at, PAGE);
        if has_pmap(i) {
            mark(&mut sections, at + PAGE, PAGE);
        }
        if has_fmap(i) {
            mark(&mut sections, at + 2 * PAGE, PAGE);
        }
        if has_fpmap(i) {
            mark(&mut sections, at + 3 * PAGE, PAGE);
        }
    }
    // Past the end of the file nothing is free: the last section's tail does not exist.
    if let Some(last) = sections.last_mut() {
        let section_end = last.at + AMAP_EVERY;
        if eof < section_end {
            last.mark(eof, section_end - eof);
        }
    }

    // Write the pages.
    let mut f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let io = |e: std::io::Error| format!("{}: {e}", path.display());

    let mut total_free: u64 = 0;
    let mut fmap_bytes: Vec<u8> = Vec::with_capacity(sections_n as usize);
    let mut pmap_full: Vec<bool> = Vec::new();
    for (i, s) in sections.iter().enumerate() {
        total_free += s.free_slots() as u64 * 64;
        fmap_bytes.push(s.longest_free_run());
        f.seek(SeekFrom::Start(s.at)).map_err(io)?;
        f.write_all(&map_page(PTYPE_AMAP, s.at, &s.bits))
            .map_err(io)?;

        if has_pmap(i as u64) {
            // One bit per page across this PMap's eight sections; 0 means free.
            let mut bits = [0u8; MAP_BYTES];
            let mut any_free = false;
            for k in 0..AMAPS_PER_PMAP as usize {
                let Some(sec) = sections.get(i + k) else {
                    break;
                };
                for p in 0..(AMAP_EVERY / PAGE) {
                    let page = sec.at + p * PAGE;
                    let bit = k as u64 * (AMAP_EVERY / PAGE) + p;
                    if page + PAGE > eof || !sec.page_free(page) {
                        bits[(bit / 8) as usize] |= 0x80 >> (bit % 8);
                    } else {
                        any_free = true;
                    }
                }
            }
            // Pages beyond this PMap's sections do not exist: marked unavailable.
            let used_bits = AMAPS_PER_PMAP * (AMAP_EVERY / PAGE);
            for bit in used_bits..(MAP_BYTES as u64 * 8) {
                bits[(bit / 8) as usize] |= 0x80 >> (bit % 8);
            }
            pmap_full.push(!any_free);
            let at = s.at + PAGE;
            f.seek(SeekFrom::Start(at)).map_err(io)?;
            f.write_all(&map_page(PTYPE_PMAP, at, &bits)).map_err(io)?;
        }
    }
    // FMap pages, each covering 496 AMaps starting at its own.
    for (i, s) in sections.iter().enumerate() {
        if !has_fmap(i as u64) {
            continue;
        }
        let mut data = [0u8; MAP_BYTES];
        for (k, slot) in data.iter_mut().enumerate() {
            *slot = fmap_bytes.get(i + k).copied().unwrap_or(0);
        }
        let at = s.at + 2 * PAGE;
        f.seek(SeekFrom::Start(at)).map_err(io)?;
        f.write_all(&map_page(PTYPE_FMAP, at, &data)).map_err(io)?;
    }
    // FPMap pages, one bit per PMap, 1 meaning no free pages.
    for (i, s) in sections.iter().enumerate() {
        if !has_fpmap(i as u64) {
            continue;
        }
        let first_pmap = i as u64 / AMAPS_PER_PMAP;
        let mut data = [0xFFu8; MAP_BYTES];
        for bit in 0..(MAP_BYTES as u64 * 8) {
            if let Some(false) = pmap_full.get((first_pmap + bit) as usize) {
                data[(bit / 8) as usize] &= !(0x80 >> (bit % 8))
            }
        }
        let at = s.at + 3 * PAGE;
        f.seek(SeekFrom::Start(at)).map_err(io)?;
        f.write_all(&map_page(PTYPE_FPMAP, at, &data)).map_err(io)?;
    }

    // The density list: AMap page numbers with their free slots, emptiest first.
    let mut dl: Vec<(u32, u32)> = sections
        .iter()
        .enumerate()
        .map(|(i, s)| (i as u32, s.free_slots()))
        .filter(|(_, free)| *free > 0)
        .collect();
    dl.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    dl.truncate(DLIST_MAX_ENTRIES);
    let mut dlist = vec![0u8; PAGE as usize];
    dlist[0] = 0x01; // DFL_BACKFILL_COMPLETE
    dlist[1] = dl.len() as u8;
    put32(&mut dlist, 4, dl.first().map(|e| e.0).unwrap_or(0)); // ulCurrentPage
    for (k, (page, free)) in dl.iter().enumerate() {
        let ent = (page & 0x000F_FFFF) | ((free & 0xFFF) << 20);
        put32(&mut dlist, 8 + k * 4, ent);
    }
    // DList pages carry a real signature and a page bid from bidNextP.
    let mut header = vec![0u8; 564];
    f.seek(SeekFrom::Start(0)).map_err(io)?;
    f.read_exact(&mut header).map_err(io)?;
    let mut next_p = u64::from_le_bytes(
        header[OFF_BID_NEXT_P..OFF_BID_NEXT_P + 8]
            .try_into()
            .unwrap(),
    );
    let dl_bid = next_p;
    next_p += 4;
    dlist[TRAILER_AT] = PTYPE_DLIST;
    dlist[TRAILER_AT + 1] = PTYPE_DLIST;
    put16(
        &mut dlist,
        TRAILER_AT + 2,
        crate::repair::sig(DLIST_AT, dl_bid),
    );
    put64(&mut dlist, TRAILER_AT + 8, dl_bid);
    let crc = crc32(&dlist[..TRAILER_AT]);
    put32(&mut dlist, TRAILER_AT + 4, crc);
    f.seek(SeekFrom::Start(DLIST_AT)).map_err(io)?;
    f.write_all(&dlist).map_err(io)?;

    // The header: valid maps, the free count, the last AMap, and fresh checksums.
    put64(&mut header, OFF_IB_FILE_EOL, eof);
    put64(
        &mut header,
        OFF_IB_AMAP_LAST,
        AMAP_FIRST + (sections_n - 1) * AMAP_EVERY,
    );
    put64(&mut header, OFF_CB_AMAP_FREE, total_free);
    put64(&mut header, OFF_CB_PMAP_FREE, 0);
    header[OFF_F_AMAP_VALID] = VALID_AMAP2;
    put64(&mut header, OFF_BID_NEXT_P, next_p);
    header[256..384].fill(0xFF); // rgbFM, deprecated, MUST be 0xFF
    header[384..512].fill(0xFF); // rgbFP, likewise
    let partial = crc32(&header[8..8 + 471]);
    put32(&mut header, 4, partial);
    let full = crc32(&header[8..8 + 516]);
    put32(&mut header, 524, full);
    f.seek(SeekFrom::Start(0)).map_err(io)?;
    f.write_all(&header).map_err(io)?;
    f.flush().map_err(io)?;

    let pmaps = (0..sections_n).filter(|i| has_pmap(*i)).count();
    let fmaps = (0..sections_n).filter(|i| has_fmap(*i)).count();
    let fpmaps = (0..sections_n).filter(|i| has_fpmap(*i)).count();
    Ok(format!(
        "Wrote {sections_n} allocation map(s), {pmaps} page map(s), {fmaps} free map page(s), {fpmaps} free page map(s) and the density list; {} free in the file. The header is marked VALID_AMAP2.",
        crate::fsutil::human_bytes(total_free)
    ))
}

/// Read back the maps of a file, for the tests and the verifier: every AMap bit, keyed
/// by section offset.
pub fn read_amaps(pst: &mut Pst) -> Result<BTreeMap<u64, [u8; MAP_BYTES]>, String> {
    let mut out = BTreeMap::new();
    let mut at = AMAP_FIRST;
    while at + PAGE <= pst.actual_len {
        let page = pst.raw_page(at)?;
        if page[TRAILER_AT] != PTYPE_AMAP || page[TRAILER_AT + 1] != PTYPE_AMAP {
            return Err(format!("no AMap page at offset {at}"));
        }
        let crc = crc32(&page[..TRAILER_AT]);
        let stored = u32::from_le_bytes(page[TRAILER_AT + 4..TRAILER_AT + 8].try_into().unwrap());
        if crc != stored {
            return Err(format!("AMap at {at} fails its checksum"));
        }
        let mut bits = [0u8; MAP_BYTES];
        bits.copy_from_slice(&page[..MAP_BYTES]);
        out.insert(at, bits);
        at += AMAP_EVERY;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_page_recurrence_follows_the_coverage_arithmetic() {
        assert!(has_pmap(0) && has_pmap(8) && !has_pmap(3));
        assert!(!has_fmap(0) && !has_fmap(127) && has_fmap(128) && has_fmap(624) && !has_fmap(129));
        assert!(!has_fpmap(0) && has_fpmap(8192) && has_fpmap(8192 + 31744) && !has_fpmap(8200));
        // An FMap or FPMap section is always a PMap section, so the slot order holds.
        assert!(has_pmap(128) && has_pmap(624) && has_pmap(8192));
    }

    #[test]
    fn a_rebuilt_pst_gets_valid_maps_that_cover_every_block_and_page() {
        let src = "../../fixtures/data/dist-list.pst";
        if !std::path::Path::new(src).exists() {
            eprintln!("skipping: run scripts/fetch-fixtures.ps1");
            return;
        }
        let mut pst = Pst::open(src).unwrap();
        let nodes = pst.nodes();
        let blocks = pst.blocks();
        let out = std::env::temp_dir().join("ostpst-amap-test.pst");
        let out_s = out.to_str().unwrap().to_string();
        crate::repair::rebuild(&mut pst, &nodes, &blocks, &out_s, &mut crate::silent()).unwrap();

        let summary = write_allocation_maps(&out).expect("maps should write");
        assert!(summary.contains("VALID_AMAP2"));

        let mut back = Pst::open(&out_s).expect("reopens");
        assert!(
            back.warnings.is_empty(),
            "a file with valid maps should reopen clean: {:?}",
            back.warnings
        );
        let maps = read_amaps(&mut back).unwrap();
        assert!(!maps.is_empty());
        let allocated = |at: u64| -> bool {
            let sec = AMAP_FIRST + (at - AMAP_FIRST) / AMAP_EVERY * AMAP_EVERY;
            let bits = &maps[&sec];
            let bit = (at - sec) / 64;
            bits[(bit / 8) as usize] & (0x80 >> (bit % 8)) != 0
        };
        // Every block and every page is marked allocated, start and end.
        for b in back.blocks() {
            let span = (b.cb as u64 + 16).div_ceil(64) * 64;
            assert!(allocated(b.ib), "block {} start unmapped", b.bid);
            assert!(allocated(b.ib + span - 64), "block {} end unmapped", b.bid);
        }
        for p in back.page_offsets() {
            assert!(allocated(p), "page at {p} unmapped");
        }
        // The map pages map themselves, and the DList is where it should be.
        for sec in maps.keys() {
            assert!(allocated(*sec));
            assert_eq!(maps[sec][0], 0xFF);
        }
        let dl = back.raw_page(DLIST_AT).unwrap();
        assert_eq!(dl[TRAILER_AT], PTYPE_DLIST);
        assert_eq!(
            crc32(&dl[..TRAILER_AT]),
            u32::from_le_bytes(dl[500..504].try_into().unwrap())
        );
        // And the nodes are all still there.
        assert_eq!(back.nodes().len(), nodes.len());
        let _ = std::fs::remove_file(out);
    }
}
