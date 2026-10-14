//! Saved scans: the node and block index of a file, kept so that reopening a large or
//! damaged mailbox does not repeat the sweep and the carve.
//!
//! A cache entry is only ever reused for the exact file it was made from. Identity is
//! the path, the size, the modification time and the header's two checksums; if any of
//! them differs the entry is ignored and rebuilt. The cache holds structure only: node
//! ids and block offsets, never message contents. It lives in the application's own
//! cache directory and can be deleted at any time with no effect but a slower reopen.

use crate::model::{IndexSource, RecoveryInfo};
use crate::ndb::{Block, Node};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIdentity {
    pub path: String,
    pub size: u64,
    pub modified_ms: i64,
    pub crc_partial: u32,
    pub crc_full: u32,
}

impl FileIdentity {
    pub fn of(path: &Path) -> Result<FileIdentity, String> {
        let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let modified_ms = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0);
        let mut head = [0u8; 528];
        {
            use std::io::Read;
            let mut f =
                std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let n = f.read(&mut head).map_err(|e| e.to_string())?;
            if n < 528 {
                head[n..].fill(0);
            }
        }
        Ok(FileIdentity {
            path: path.to_string_lossy().to_string(),
            size: meta.len(),
            modified_ms,
            crc_partial: u32::from_le_bytes([head[4], head[5], head[6], head[7]]),
            crc_full: u32::from_le_bytes([head[524], head[525], head[526], head[527]]),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanCache {
    pub version: u32,
    pub identity: FileIdentity,
    pub salvage: bool,
    pub index_source: IndexSource,
    pub recovery: Option<RecoveryInfo>,
    pub nodes: Vec<Node>,
    pub blocks: Vec<Block>,
    pub saved_ms: i64,
}

/// A stable file name for the cache entry of `identity`.
fn entry_name(identity: &FileIdentity, salvage: bool) -> String {
    let key = format!("{}|{}", identity.path.to_lowercase(), salvage);
    let h = crate::mailbox::sha256_hex(key.as_bytes());
    format!("{}.scan.json", &h[..24])
}

pub fn load(dir: &Path, identity: &FileIdentity, salvage: bool) -> Option<ScanCache> {
    let p = dir.join(entry_name(identity, salvage));
    let text = std::fs::read_to_string(p).ok()?;
    let c: ScanCache = serde_json::from_str(&text).ok()?;
    if c.version != CACHE_VERSION
        || c.salvage != salvage
        || c.identity.size != identity.size
        || c.identity.modified_ms != identity.modified_ms
        || c.identity.crc_partial != identity.crc_partial
        || c.identity.crc_full != identity.crc_full
        || !c.identity.path.eq_ignore_ascii_case(&identity.path)
    {
        return None;
    }
    Some(c)
}

pub fn save(dir: &Path, cache: &ScanCache) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let p = dir.join(entry_name(&cache.identity, cache.salvage));
    let text = serde_json::to_string(cache).map_err(|e| e.to_string())?;
    std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))?;
    Ok(p)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheInfo {
    pub dir: String,
    pub entries: usize,
    pub bytes: u64,
}

pub fn info(dir: &Path) -> CacheInfo {
    let mut entries = 0;
    let mut bytes = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().extension().is_some_and(|x| x == "json") {
                entries += 1;
                bytes += e.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    CacheInfo {
        dir: dir.to_string_lossy().to_string(),
        entries,
        bytes,
    }
}

pub fn clear(dir: &Path) -> Result<usize, String> {
    let mut n = 0;
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            if e.path().to_string_lossy().ends_with(".scan.json") {
                std::fs::remove_file(e.path())
                    .map_err(|x| format!("{}: {x}", e.path().display()))?;
                n += 1;
            }
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cache_entry_is_only_reused_for_the_same_file() {
        let dir = std::env::temp_dir().join(format!("ostpst-cache-{}", std::process::id()));
        let file = dir.join("a.pst");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&file, vec![0u8; 1024]).unwrap();
        let id = FileIdentity::of(&file).unwrap();
        let c = ScanCache {
            version: CACHE_VERSION,
            identity: id.clone(),
            salvage: false,
            index_source: IndexSource::Header,
            recovery: None,
            nodes: vec![Node {
                nid: 0x21,
                bid_data: 4,
                bid_sub: 0,
                nid_parent: 0x21,
            }],
            blocks: vec![],
            saved_ms: 1,
        };
        save(&dir, &c).unwrap();
        assert!(load(&dir, &id, false).is_some());
        assert!(
            load(&dir, &id, true).is_none(),
            "salvage is part of the key"
        );
        // Change the file: the entry must be ignored.
        std::fs::write(&file, vec![1u8; 2048]).unwrap();
        let id2 = FileIdentity::of(&file).unwrap();
        assert!(load(&dir, &id2, false).is_none());
        assert_eq!(info(&dir).entries, 1);
        assert_eq!(clear(&dir).unwrap(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
