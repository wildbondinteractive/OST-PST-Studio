//! The high-level API: one opened file, and everything a front end asks of it.
//!
//! Everything below this module takes `&mut Pst` and hands back raw nodes and property
//! maps. This module owns the `Pst`, keeps the index and the folder tree, and answers in
//! the serialisable types of [`crate::model`]. It is what the desktop application's
//! commands and the command-line tool both call.
//!
//! Memory is bounded on purpose. The index (one 32-byte entry per node) and the folder
//! tree are held; message summaries are built per folder on first listing and kept in a
//! small cache; bodies and attachments are read on demand and never retained.

use crate::ltp::{
    self, body_text, clean_subject, read_node_pc, read_pc, read_recipients, read_tc_with, Names,
    Pc, Value, NID_ROOT_FOLDER, NID_TYPE_ATTACHMENT, NID_TYPE_CONTENTS_TABLE, PID_ATTACH_DATA,
    PID_ATTACH_FILENAME, PID_ATTACH_LONG_FILENAME, PID_ATTACH_METHOD, PID_ATTACH_MIME_TAG,
    PID_BODY_HTML, PID_CONTENT_COUNT, PID_DELIVERY_TIME, PID_DISPLAY_NAME, PID_DISPLAY_TO,
    PID_INTERNET_CODEPAGE, PID_INTERNET_MSG_ID, PID_MESSAGE_SIZE, PID_SENDER_EMAIL,
    PID_SENDER_NAME, PID_SUBJECT, PID_SUBMIT_TIME, PID_TRANSPORT_HEADERS, PID_UNREAD_COUNT,
};
use crate::model::*;
use crate::ndb::{nid_type_name, Block, Node, Pst};
use crate::props::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

const NID_TYPE_FOLDER: u8 = 0x02;
const NID_TYPE_SEARCH_FOLDER: u8 = 0x03;
const NID_TYPE_MESSAGE: u8 = 0x04;
const NID_MESSAGE_STORE: u32 = 0x21;
const MSGFLAG_READ: i64 = 0x1;
/// How many folders' listings are kept before the cache is emptied. A listing is a few
/// hundred bytes per message, so this bounds memory at a few tens of megabytes even on
/// a mailbox with hundreds of thousands of messages in a handful of folders.
const LIST_CACHE_FOLDERS: usize = 8;
/// Largest binary property rendered as hex in the properties view.
const HEX_PREVIEW: usize = 64;

/// One opened PST or OST.
pub struct Mailbox {
    pst: Pst,
    path: PathBuf,
    nodes: Vec<Node>,
    blocks: Vec<Block>,
    by_nid: HashMap<u32, Node>,
    folders: BTreeMap<u32, FolderInfo>,
    /// Message nids per folder, in index order.
    messages_in: HashMap<u32, Vec<u32>>,
    names: Names,
    info: FileInfo,
    lists: HashMap<u32, Vec<MessageSummary>>,
    list_unreadable: HashMap<u32, usize>,
}

/// How to open: follow the header's index, or sweep regardless.
#[derive(Debug, Clone, Default)]
pub struct OpenOptions {
    pub salvage: bool,
    /// Where saved scans live. `None` disables the cache. An entry is reused only when
    /// the file's path, size, modification time and header checksums all match.
    pub cache_dir: Option<PathBuf>,
}

impl Mailbox {
    /// Open a file and build its index and folder tree.
    ///
    /// Falls back to sweeping and carving when the header's index is unusable, which is
    /// the ordinary case for the file somebody actually needs help with. Progress is
    /// reported in two phases, both indeterminate on a damaged file.
    pub fn open(path: &Path, opts: OpenOptions, on: crate::Progress) -> Result<Mailbox, String> {
        let path_str = path.to_string_lossy().to_string();
        let mut pst = Pst::open(&path_str)?;
        if !on(0, 0) {
            return Err(crate::CANCELLED.into());
        }

        // A saved scan, when there is one for exactly this file, replaces the walk (or
        // the sweep and carve, which is where the time goes on a damaged file).
        let identity = opts
            .cache_dir
            .as_deref()
            .and_then(|_| crate::cache::FileIdentity::of(path).ok());
        let cached = match (&opts.cache_dir, &identity) {
            (Some(dir), Some(id)) => crate::cache::load(dir, id, opts.salvage),
            _ => None,
        };
        let from_cache = cached.is_some();
        let (nodes, blocks, recovery, index_source) = match cached {
            Some(c) => {
                if c.index_source == IndexSource::Recovered {
                    pst.adopt(&c.blocks);
                }
                (c.nodes, c.blocks, c.recovery, c.index_source)
            }
            None => build_index(&mut pst, opts.salvage),
        };
        if !on(1, 0) {
            return Err(crate::CANCELLED.into());
        }
        if let (false, Some(dir), Some(id)) = (from_cache, &opts.cache_dir, identity) {
            // Best effort: a cache that cannot be written costs a slower reopen, nothing
            // else, so the error is not worth failing the open for.
            let _ = crate::cache::save(
                dir,
                &crate::cache::ScanCache {
                    version: 1,
                    identity: id,
                    salvage: opts.salvage,
                    index_source,
                    recovery: recovery.clone(),
                    nodes: nodes.clone(),
                    blocks: blocks.clone(),
                    saved_ms: now_ms(),
                },
            );
        }

        let by_nid: HashMap<u32, Node> = nodes.iter().map(|n| (n.nid, *n)).collect();
        let names = ltp::read_names(&mut pst, &nodes);

        let essentials_missing: Vec<String> = [
            (NID_MESSAGE_STORE, "message store"),
            (NID_ROOT_FOLDER, "root folder"),
        ]
        .iter()
        .filter(|(nid, _)| !by_nid.contains_key(nid))
        .map(|(nid, what)| format!("{what} (node 0x{nid:X})"))
        .collect();

        let mut mb = Mailbox {
            pst,
            path: path.to_path_buf(),
            nodes,
            blocks,
            by_nid,
            folders: BTreeMap::new(),
            messages_in: HashMap::new(),
            names,
            info: FileInfo {
                path: path_str.clone(),
                file_name: path
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or(path_str),
                size_bytes: 0,
                declared_bytes: 0,
                is_ost: false,
                format: FileFormat::Unicode,
                format_version: 0,
                encoding: String::new(),
                amap_valid: true,
                index_source,
                health: HealthStatus::Healthy,
                node_count: 0,
                block_count: 0,
                folder_count: 0,
                message_count: 0,
                node_types: Vec::new(),
                warnings: Vec::new(),
                problem_count: 0,
                recovery,
                essentials_missing,
            },
            lists: HashMap::new(),
            list_unreadable: HashMap::new(),
        };
        mb.build_folders();
        mb.refresh_info();
        Ok(mb)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn info(&self) -> &FileInfo {
        &self.info
    }

    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    pub fn pst_mut(&mut self) -> &mut Pst {
        &mut self.pst
    }

    pub fn names(&self) -> &Names {
        &self.names
    }

    /// Every folder, in node-id order. The tree is in `parent` and `children`.
    pub fn folders(&self) -> Vec<FolderInfo> {
        self.folders.values().cloned().collect()
    }

    pub fn folder(&self, nid: u32) -> Option<&FolderInfo> {
        self.folders.get(&nid)
    }

    /// Plain names, for the exporter's directory tree.
    pub fn folder_names(&self) -> BTreeMap<u32, String> {
        self.folders
            .iter()
            .map(|(nid, f)| (*nid, f.name.clone()))
            .collect()
    }

    /// A folder and every folder under it.
    pub fn subtree(&self, root: u32) -> Vec<u32> {
        let mut out = Vec::new();
        let mut stack = vec![root];
        let mut seen = HashSet::new();
        while let Some(f) = stack.pop() {
            if !seen.insert(f) {
                continue;
            }
            out.push(f);
            if let Some(info) = self.folders.get(&f) {
                stack.extend(info.children.iter().copied());
            }
        }
        out
    }

    /// The message nids a selection covers, de-duplicated, in index order.
    pub fn select(&self, sel: &Selection) -> Vec<u32> {
        if sel.entire_mailbox {
            return self
                .nodes
                .iter()
                .filter(|n| n.nid_type() == NID_TYPE_MESSAGE)
                .map(|n| n.nid)
                .collect();
        }
        let mut folders = HashSet::new();
        for f in &sel.folders {
            folders.extend(self.subtree(*f));
        }
        let wanted: HashSet<u32> = sel.messages.iter().copied().collect();
        self.nodes
            .iter()
            .filter(|n| n.nid_type() == NID_TYPE_MESSAGE)
            .filter(|n| folders.contains(&n.nid_parent) || wanted.contains(&n.nid))
            .map(|n| n.nid)
            .collect()
    }

    /// The nodes an export needs: every folder (for the directory tree) plus the
    /// selected messages.
    pub fn nodes_for(&self, messages: &[u32]) -> Vec<Node> {
        let wanted: HashSet<u32> = messages.iter().copied().collect();
        self.nodes
            .iter()
            .filter(|n| n.nid_type() == NID_TYPE_FOLDER || wanted.contains(&n.nid))
            .copied()
            .collect()
    }

    fn refresh_info(&mut self) {
        let p = &self.pst;
        let format = if p.is_ansi() {
            FileFormat::Ansi
        } else if p.is_small_page() {
            FileFormat::Unicode
        } else {
            FileFormat::Unicode4k
        };
        let mut counts: BTreeMap<u8, usize> = BTreeMap::new();
        for n in &self.nodes {
            *counts.entry(n.nid_type()).or_default() += 1;
        }
        let message_count = counts.get(&NID_TYPE_MESSAGE).copied().unwrap_or(0);
        let amap_valid = !p
            .warnings
            .iter()
            .any(|w| w.contains("allocation map is marked invalid"));
        let health = if !self.info.essentials_missing.is_empty() {
            HealthStatus::Critical
        } else if self.info.index_source == IndexSource::Recovered {
            HealthStatus::Recovered
        } else if p.warnings.iter().any(|w| !w.contains("allocation map")) {
            HealthStatus::Damaged
        } else {
            HealthStatus::Healthy
        };
        self.info.size_bytes = p.actual_len;
        self.info.declared_bytes = p.declared_len;
        self.info.is_ost = p.is_ost;
        self.info.format = format;
        self.info.format_version = p.ver;
        self.info.encoding = match p.crypt {
            crate::ndb::Crypt::None => "none".into(),
            crate::ndb::Crypt::Permute => "permute (keyless)".into(),
            crate::ndb::Crypt::Cyclic => "cyclic (keyless)".into(),
            crate::ndb::Crypt::Unknown(b) => format!("unknown (0x{b:02X})"),
        };
        self.info.amap_valid = amap_valid;
        self.info.health = health;
        self.info.node_count = self.nodes.len();
        self.info.block_count = self.blocks.len();
        self.info.folder_count = self.folders.len();
        self.info.message_count = message_count;
        self.info.node_types = counts
            .iter()
            .map(|(t, c)| NodeTypeCount {
                nid_type: *t,
                name: nid_type_name(*t).to_string(),
                count: *c,
            })
            .collect();
        self.info.warnings = p.warnings.clone();
        self.info.problem_count = p.problem_count();
    }

    /// Read every folder's properties once and build the tree.
    fn build_folders(&mut self) {
        let mut folders: BTreeMap<u32, FolderInfo> = BTreeMap::new();
        let mut children: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
        let folder_nodes: Vec<Node> = self
            .nodes
            .iter()
            .filter(|n| matches!(n.nid_type(), NID_TYPE_FOLDER | NID_TYPE_SEARCH_FOLDER))
            .copied()
            .collect();

        for n in &folder_nodes {
            let (name, content_count, unread_count, container_class, unreadable) =
                match read_node_pc(&mut self.pst, n) {
                    Ok(pc) => (
                        pc.str(PID_DISPLAY_NAME)
                            .filter(|s| !s.trim().is_empty())
                            .map(str::to_string)
                            .unwrap_or_else(|| {
                                if n.nid == NID_ROOT_FOLDER {
                                    "(root)".into()
                                } else {
                                    format!("(unnamed folder 0x{:X})", n.nid)
                                }
                            }),
                        pc.int(PID_CONTENT_COUNT).unwrap_or(0),
                        pc.int(PID_UNREAD_COUNT).unwrap_or(0),
                        pc.str(PID_CONTAINER_CLASS).map(str::to_string),
                        false,
                    ),
                    Err(_) => (
                        format!("(unreadable folder 0x{:X})", n.nid),
                        0,
                        0,
                        None,
                        true,
                    ),
                };
            if n.nid != n.nid_parent {
                children.entry(n.nid_parent).or_default().push(n.nid);
            }
            folders.insert(
                n.nid,
                FolderInfo {
                    nid: n.nid,
                    parent: n.nid_parent,
                    name,
                    path: String::new(),
                    depth: 0,
                    kind: if n.nid_type() == NID_TYPE_SEARCH_FOLDER {
                        FolderKind::Search
                    } else {
                        FolderKind::Other
                    },
                    container_class,
                    content_count,
                    unread_count,
                    message_count: 0,
                    subfolder_count: 0,
                    children: Vec::new(),
                    unreadable,
                    orphan: true,
                    has_children: false,
                },
            );
        }

        // Messages per folder, from the index's parent pointers.
        let mut messages_in: HashMap<u32, Vec<u32>> = HashMap::new();
        for n in self
            .nodes
            .iter()
            .filter(|n| n.nid_type() == NID_TYPE_MESSAGE)
        {
            messages_in.entry(n.nid_parent).or_default().push(n.nid);
        }
        for (nid, f) in folders.iter_mut() {
            f.message_count = messages_in.get(nid).map_or(0, Vec::len);
            if let Some(c) = children.get(nid) {
                f.subfolder_count = c.len();
                f.has_children = !c.is_empty();
                f.children = c.clone();
            }
        }
        // Children sorted by name needs the names, which the borrow above cannot see;
        // done in a second pass with a snapshot.
        let name_of: HashMap<u32, String> =
            folders.iter().map(|(k, v)| (*k, v.name.clone())).collect();
        for f in folders.values_mut() {
            f.children.sort_by_key(|k| {
                folders_name_key(name_of.get(k).map(String::as_str).unwrap_or(""), *k)
            });
        }

        // Paths, depth and reachability, walking down from the root.
        let mut stack: Vec<(u32, String, usize)> = vec![(NID_ROOT_FOLDER, String::new(), 0)];
        let mut seen = HashSet::new();
        while let Some((nid, path, depth)) = stack.pop() {
            if !seen.insert(nid) {
                continue;
            }
            let Some(f) = folders.get_mut(&nid) else {
                continue;
            };
            f.orphan = false;
            f.depth = depth;
            f.path = path.clone();
            let kids = f.children.clone();
            for c in kids {
                let child_path = match folders.get(&c) {
                    Some(cf) if path.is_empty() => cf.name.clone(),
                    Some(cf) => format!("{path}/{}", cf.name),
                    None => continue,
                };
                stack.push((c, child_path, depth + 1));
            }
        }
        // Orphans keep their own name as a path, at depth 0, so they still show up.
        for f in folders.values_mut() {
            if f.orphan {
                f.path = f.name.clone();
            }
        }
        if let Some(root) = folders.get_mut(&NID_ROOT_FOLDER) {
            root.kind = FolderKind::Root;
        }

        self.folders = folders;
        self.messages_in = messages_in;
        self.classify_folders();
    }

    /// Decide what each folder is for: the store's entry ids first, the container class
    /// next, and the well-known English names last.
    fn classify_folders(&mut self) {
        let mut special: HashMap<u32, FolderKind> = HashMap::new();
        if let Some(store) = self.by_nid.get(&NID_MESSAGE_STORE).copied() {
            if let Ok(pc) = read_node_pc(&mut self.pst, &store) {
                let entry_nid = |id: u16| -> Option<u32> {
                    match pc.props.get(&id) {
                        Some(Value::Bytes(b)) if b.len() == 24 => {
                            Some(u32::from_le_bytes([b[20], b[21], b[22], b[23]]))
                        }
                        _ => None,
                    }
                };
                for (id, kind) in [
                    (0x35E3u16, FolderKind::DeletedItems),
                    (0x35E2, FolderKind::Outbox),
                    (0x35E4, FolderKind::SentItems),
                    (0x35E7, FolderKind::Search),
                ] {
                    if let Some(nid) = entry_nid(id) {
                        special.insert(nid, kind);
                    }
                }
            }
        }
        for f in self.folders.values_mut() {
            if f.nid == NID_ROOT_FOLDER {
                continue;
            }
            if let Some(k) = special.get(&f.nid) {
                f.kind = *k;
                continue;
            }
            let by_class = match f.container_class.as_deref().map(|c| c.to_ascii_lowercase()) {
                Some(c) if c.starts_with("ipf.appointment") => Some(FolderKind::Calendar),
                Some(c) if c.starts_with("ipf.contact") => Some(FolderKind::Contacts),
                Some(c) if c.starts_with("ipf.task") => Some(FolderKind::Tasks),
                Some(c) if c.starts_with("ipf.stickynote") => Some(FolderKind::Notes),
                Some(c) if c.starts_with("ipf.journal") => Some(FolderKind::Journal),
                _ => None,
            };
            if let Some(k) = by_class {
                f.kind = k;
                continue;
            }
            if f.kind == FolderKind::Search {
                continue;
            }
            f.kind = match f.name.to_ascii_lowercase().as_str() {
                "inbox" => FolderKind::Inbox,
                "outbox" => FolderKind::Outbox,
                "sent items" | "sent" => FolderKind::SentItems,
                "deleted items" | "trash" => FolderKind::DeletedItems,
                "drafts" => FolderKind::Drafts,
                "junk e-mail" | "junk email" | "junk" | "spam" => FolderKind::JunkEmail,
                "archive" => FolderKind::Archive,
                "calendar" => FolderKind::Calendar,
                "contacts" => FolderKind::Contacts,
                "tasks" => FolderKind::Tasks,
                "notes" => FolderKind::Notes,
                "journal" => FolderKind::Journal,
                _ => FolderKind::Other,
            };
        }
    }

    /// Build (or fetch) the summaries for one folder.
    fn summaries(&mut self, folder: u32) -> &Vec<MessageSummary> {
        if !self.lists.contains_key(&folder) {
            if self.lists.len() >= LIST_CACHE_FOLDERS {
                self.lists.clear();
                self.list_unreadable.clear();
            }
            let (list, unreadable) = self.build_summaries(folder);
            self.lists.insert(folder, list);
            self.list_unreadable.insert(folder, unreadable);
        }
        &self.lists[&folder]
    }

    fn build_summaries(&mut self, folder: u32) -> (Vec<MessageSummary>, usize) {
        let nids = self.messages_in.get(&folder).cloned().unwrap_or_default();
        if nids.is_empty() {
            return (Vec::new(), 0);
        }

        // The folder's contents table carries the listing columns inline, which is far
        // cheaper than one property-context read per message. It is trusted only for
        // messages the index also knows, and anything it does not list is read the slow
        // way, so a recovered file lists everything the sweep found.
        let mut rows: HashMap<u32, BTreeMap<u16, Value>> = HashMap::new();
        let mut have_table = false;
        let table_nid = (folder & !0x1F) | NID_TYPE_CONTENTS_TABLE;
        // On a recovered index the table's rows may come from a carved block that is
        // not the revision the index expected, so they are used only to tell live
        // messages from swept ones; every row is read from its own properties.
        let trust_table = self.info.index_source == IndexSource::Header;
        if let Some(t) = self.by_nid.get(&table_nid).copied() {
            let by_nid = &self.by_nid;
            let lookup = |id: u32| by_nid.get(&id).map(|n| (n.bid_data, n.bid_sub));
            if let Ok(tc) = read_tc_with(&mut self.pst, t.bid_data, t.bid_sub, Some(&lookup)) {
                have_table = true;
                for r in tc.rows {
                    rows.insert(r.id, r.props);
                }
            }
        }

        let mut out = Vec::with_capacity(nids.len());
        let mut unreadable = 0;
        for nid in nids {
            let Some(node) = self.by_nid.get(&nid).copied() else {
                continue;
            };
            if let Some(props) = rows.get(&nid).filter(|_| trust_table) {
                // A table row has no sender e-mail or display-to in most files, and that
                // is fine for a list. The subject is the one thing worth insisting on,
                // and a row whose text holds control characters is a row from a block
                // that is not what the table thinks it is, so it is not used either.
                if props.contains_key(&PID_SUBJECT) || props.contains_key(&PID_MESSAGE_CLASS) {
                    let s = summary_from_props(nid, folder, props, true);
                    // A mail with no sender in its row is a row the table could not
                    // fill in; the message itself knows, and one read settles it.
                    let senderless = s.item_type == ItemType::Mail
                        && s.sender_name.is_empty()
                        && s.sender_email.is_empty();
                    if !has_control_chars(&s.subject)
                        && !has_control_chars(&s.sender_name)
                        && !senderless
                    {
                        out.push(s);
                        continue;
                    }
                }
            }
            match read_node_pc(&mut self.pst, &node) {
                Ok(pc) => {
                    let mut s = summary_from_props(nid, folder, &pc.props, false);
                    if !s.has_attachments {
                        s.has_attachments = has_attachment_subnode(&mut self.pst, &node);
                    }
                    s.recovered = have_table && !rows.contains_key(&nid);
                    out.push(s);
                }
                Err(_) => {
                    unreadable += 1;
                    out.push(MessageSummary {
                        nid,
                        folder,
                        subject: format!("(unreadable message 0x{nid:X})"),
                        sender_name: String::new(),
                        sender_email: String::new(),
                        display_to: String::new(),
                        date_ms: None,
                        size: 0,
                        has_attachments: false,
                        importance: 1,
                        read: true,
                        item_type: ItemType::Other,
                        message_class: String::new(),
                        from_table: false,
                        recovered: have_table && !rows.contains_key(&nid),
                    });
                }
            }
        }
        (out, unreadable)
    }

    /// One page of a folder's messages, sorted.
    pub fn list_messages(
        &mut self,
        folder: u32,
        offset: usize,
        limit: usize,
        sort: SortKey,
        descending: bool,
    ) -> MessagePage {
        let unreadable = *self.list_unreadable.get(&folder).unwrap_or(&0);
        let all = self.summaries(folder);
        let mut order: Vec<usize> = (0..all.len()).collect();
        order.sort_by(|&a, &b| {
            let (x, y) = (&all[a], &all[b]);
            let ord = match sort {
                SortKey::Date => x.date_ms.cmp(&y.date_ms),
                SortKey::Sender => x
                    .sender_name
                    .to_lowercase()
                    .cmp(&y.sender_name.to_lowercase()),
                SortKey::Subject => x.subject.to_lowercase().cmp(&y.subject.to_lowercase()),
                SortKey::Size => x.size.cmp(&y.size),
            };
            if descending {
                ord.reverse()
            } else {
                ord
            }
        });
        let total = order.len();
        let items = order
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|i| all[i].clone())
            .collect();
        MessagePage {
            folder,
            offset,
            total,
            items,
            unreadable: self
                .list_unreadable
                .get(&folder)
                .copied()
                .unwrap_or(unreadable),
        }
    }

    /// Every summary in a folder, for the exporter and the search.
    pub fn all_summaries(&mut self, folder: u32) -> Vec<MessageSummary> {
        self.summaries(folder).clone()
    }

    /// One message in full.
    pub fn message(&mut self, nid: u32) -> Result<MessageDetail, String> {
        let node = *self
            .by_nid
            .get(&nid)
            .ok_or_else(|| format!("no node 0x{nid:X} in this file"))?;
        if node.nid_type() != NID_TYPE_MESSAGE {
            return Err(format!(
                "node 0x{nid:X} is a {}, not a message",
                nid_type_name(node.nid_type())
            ));
        }
        let pc = read_node_pc(&mut self.pst, &node)?;
        let mut problems = Vec::new();
        let mut summary = summary_from_props(nid, node.nid_parent, &pc.props, false);
        let attachments = self.attachments_of(&node, &mut problems);
        summary.has_attachments = summary.has_attachments || !attachments.is_empty();

        let recipients = read_recipients(&mut self.pst, node.bid_sub)
            .into_iter()
            .map(|r| RecipientInfo {
                kind: r.kind,
                name: r.name,
                email: r.email,
            })
            .collect();

        let body_html = match pc.props.get(&PID_BODY_HTML) {
            Some(Value::Bytes(b)) => {
                let s = crate::html::decode(
                    b,
                    pc.int(PID_INTERNET_CODEPAGE)
                        .and_then(|cp| u32::try_from(cp).ok()),
                );
                // PidTagHtml does not always hold markup. Outlook's own test message
                // puts plain text there; a renderer would fold its line breaks away.
                if s.contains('<') {
                    Some(s)
                } else {
                    None
                }
            }
            Some(Value::Str(s)) if s.contains('<') => Some(s.clone()),
            _ => None,
        };
        let text = body_text(&pc);
        for (id, v) in &pc.props {
            if let Value::MissingSubnode(s) = v {
                problems.push(format!(
                    "property 0x{id:04X} points at subnode 0x{s:08X}, which is not in the message's subnode tree; its value is lost"
                ));
            }
        }

        let properties = self.property_views(&pc);
        let extra = extra_fields(&pc, &self.names);

        Ok(MessageDetail {
            summary,
            recipients,
            sender_representing_name: pc.str(PID_SENT_REPRESENTING_NAME).unwrap_or("").into(),
            sender_representing_email: pc.str(PID_SENT_REPRESENTING_EMAIL).unwrap_or("").into(),
            body_text: text,
            body_html,
            transport_headers: pc.str(PID_TRANSPORT_HEADERS).map(str::to_string),
            internet_message_id: pc.str(PID_INTERNET_MSG_ID).map(str::to_string),
            conversation_topic: pc.str(PID_CONVERSATION_TOPIC).map(str::to_string),
            created_ms: pc.time(PID_CREATION_TIME).and_then(filetime_to_ms),
            modified_ms: pc.time(PID_LAST_MODIFICATION_TIME).and_then(filetime_to_ms),
            sent_ms: pc.time(PID_SUBMIT_TIME).and_then(filetime_to_ms),
            received_ms: pc.time(PID_DELIVERY_TIME).and_then(filetime_to_ms),
            creator_name: pc.str(PID_CREATOR_NAME).map(str::to_string),
            last_modifier_name: pc.str(PID_LAST_MODIFIER_NAME).map(str::to_string),
            sensitivity: pc.int(PID_SENSITIVITY).unwrap_or(0),
            attachments,
            properties,
            extra,
            problems,
        })
    }

    /// Every attachment of a message, without their bytes.
    fn attachments_of(&mut self, node: &Node, problems: &mut Vec<String>) -> Vec<AttachmentInfo> {
        let Ok(subs) = self.pst.subnodes(node.bid_sub) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut entries: Vec<(u32, crate::ndb::Sub)> = subs
            .into_iter()
            .filter(|(nid, _)| nid & 0x1F == NID_TYPE_ATTACHMENT)
            .collect();
        entries.sort_by_key(|(k, _)| *k);
        for (anid, sub) in entries {
            match read_pc(&mut self.pst, sub.data, sub.sub) {
                Ok(pc) => out.push(attachment_info(anid, &pc)),
                Err(e) => problems.push(format!("attachment 0x{anid:X} could not be read: {e}")),
            }
        }
        out
    }

    /// The bytes of one attachment, with its name. By-value attachments only; an
    /// embedded message is a message, not a file, and is read through
    /// [`Mailbox::embedded_message`].
    pub fn attachment_data(
        &mut self,
        message: u32,
        attachment: u32,
    ) -> Result<(AttachmentInfo, Vec<u8>), String> {
        let node = *self
            .by_nid
            .get(&message)
            .ok_or_else(|| format!("no node 0x{message:X} in this file"))?;
        let subs = self.pst.subnodes(node.bid_sub)?;
        let sub = *subs
            .get(&attachment)
            .ok_or_else(|| format!("message 0x{message:X} has no attachment 0x{attachment:X}"))?;
        let pc = read_pc(&mut self.pst, sub.data, sub.sub)?;
        let info = attachment_info(attachment, &pc);
        if info.is_embedded_message {
            return Err(
                "this attachment is an embedded message, not a file; open it to read it".into(),
            );
        }
        match pc.props.get(&PID_ATTACH_DATA) {
            Some(Value::Bytes(b)) => {
                let mut info = info;
                info.size = b.len() as u64;
                info.sha256 = Some(sha256_hex(b));
                Ok((info, b.clone()))
            }
            Some(Value::MissingSubnode(s)) => Err(format!(
                "the attachment's bytes were in subnode 0x{s:08X}, which is not in the file; the data is lost"
            )),
            _ => Err("the attachment has no data (it may be a link to a file elsewhere)".into()),
        }
    }

    /// An embedded message (attach method 5) as a detail, read from the attachment's
    /// own subnode tree.
    pub fn embedded_message(
        &mut self,
        message: u32,
        attachment: u32,
    ) -> Result<MessageDetail, String> {
        let node = *self
            .by_nid
            .get(&message)
            .ok_or_else(|| format!("no node 0x{message:X} in this file"))?;
        let subs = self.pst.subnodes(node.bid_sub)?;
        let sub = *subs
            .get(&attachment)
            .ok_or_else(|| format!("message 0x{message:X} has no attachment 0x{attachment:X}"))?;
        let apc = read_pc(&mut self.pst, sub.data, sub.sub)?;
        // PidTagAttachDataObject is a subnode id naming the embedded message's own
        // property context inside the attachment's subnode tree.
        let inner = match apc.props.get(&PID_ATTACH_DATA) {
            Some(Value::Raw {
                ptype: 0x000D,
                bytes,
            }) if bytes.len() >= 4 => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            _ => {
                // The LTP layer resolves subnode-backed values eagerly, so an embedded
                // message arrives as its bytes. Look the subnode up by type instead.
                let inner_subs = self.pst.subnodes(sub.sub)?;
                inner_subs
                    .keys()
                    .copied()
                    .find(|k| (k & 0x1F) as u8 == NID_TYPE_MESSAGE)
                    .ok_or("the attachment does not contain an embedded message")?
            }
        };
        let inner_subs = self.pst.subnodes(sub.sub)?;
        let m = *inner_subs
            .get(&inner)
            .ok_or("the embedded message's subnode is missing")?;
        let pc = read_pc(&mut self.pst, m.data, m.sub)?;
        let mut summary = summary_from_props(inner, message, &pc.props, false);
        summary.has_attachments = false;
        let properties = self.property_views(&pc);
        Ok(MessageDetail {
            summary,
            recipients: read_recipients(&mut self.pst, m.sub)
                .into_iter()
                .map(|r| RecipientInfo {
                    kind: r.kind,
                    name: r.name,
                    email: r.email,
                })
                .collect(),
            sender_representing_name: String::new(),
            sender_representing_email: String::new(),
            body_text: body_text(&pc),
            body_html: match pc.props.get(&PID_BODY_HTML) {
                Some(Value::Bytes(b)) => {
                    let s = crate::html::decode(b, None);
                    s.contains('<').then_some(s)
                }
                _ => None,
            },
            transport_headers: pc.str(PID_TRANSPORT_HEADERS).map(str::to_string),
            internet_message_id: pc.str(PID_INTERNET_MSG_ID).map(str::to_string),
            conversation_topic: None,
            created_ms: None,
            modified_ms: None,
            sent_ms: pc.time(PID_SUBMIT_TIME).and_then(filetime_to_ms),
            received_ms: pc.time(PID_DELIVERY_TIME).and_then(filetime_to_ms),
            creator_name: None,
            last_modifier_name: None,
            sensitivity: 0,
            attachments: Vec::new(),
            properties,
            extra: extra_fields(&pc, &self.names),
            problems: Vec::new(),
        })
    }

    /// Every property on one node, labelled, for the raw view.
    pub fn node_properties(&mut self, nid: u32) -> Result<Vec<PropertyView>, String> {
        let node = *self
            .by_nid
            .get(&nid)
            .ok_or_else(|| format!("no node 0x{nid:X} in this file"))?;
        let pc = read_node_pc(&mut self.pst, &node)?;
        Ok(self.property_views(&pc))
    }

    fn property_views(&self, pc: &Pc) -> Vec<PropertyView> {
        pc.props
            .iter()
            .map(|(tag, v)| {
                let name = tag_name(*tag)
                    .map(str::to_string)
                    .or_else(|| self.names.get(*tag).map(|n| n.to_string()));
                let (ptype, type_name_s, value, bytes) = describe(v);
                PropertyView {
                    tag: *tag,
                    ptype,
                    name,
                    type_name: type_name_s.to_string(),
                    value,
                    bytes,
                }
            })
            .collect()
    }

    /// Search with filters. Cheap filters are applied to the listing summaries first;
    /// a message's body, recipients or attachment names are read only when everything
    /// cheaper has already matched.
    pub fn search(&mut self, q: &SearchQuery, on: crate::Progress) -> SearchResult {
        let started = std::time::Instant::now();
        let needle = q.text.trim().to_lowercase();
        let any_text =
            q.in_subject || q.in_sender || q.in_recipients || q.in_body || q.in_attachment_names;
        let folders: Vec<u32> = match q.folder {
            Some(f) => self.subtree(f),
            None => self.folders.keys().copied().collect(),
        };
        let total: usize = folders
            .iter()
            .map(|f| self.messages_in.get(f).map_or(0, Vec::len))
            .sum();
        let limit = q.limit.unwrap_or(5000).max(1);
        let attach_needle = q
            .attachment_name
            .as_deref()
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty());

        let mut hits = Vec::new();
        let mut scanned = 0usize;
        let mut cancelled = false;
        let mut truncated = false;

        'outer: for folder in folders {
            let list = self.all_summaries(folder);
            for s in list {
                scanned += 1;
                if !on(scanned as u64, total as u64) {
                    cancelled = true;
                    break 'outer;
                }
                if !cheap_filters_pass(q, &s) {
                    continue;
                }
                let mut matched = String::new();
                if !needle.is_empty() && any_text {
                    if q.in_subject && s.subject.to_lowercase().contains(&needle) {
                        matched = "subject".into();
                    } else if q.in_sender
                        && (s.sender_name.to_lowercase().contains(&needle)
                            || s.sender_email.to_lowercase().contains(&needle))
                    {
                        matched = "sender".into();
                    }
                }
                let needs_deep = (matched.is_empty()
                    && !needle.is_empty()
                    && any_text
                    && (q.in_body || q.in_recipients || q.in_attachment_names || s.from_table))
                    || attach_needle.is_some();
                if needs_deep {
                    let Some(node) = self.by_nid.get(&s.nid).copied() else {
                        continue;
                    };
                    let Ok(pc) = read_node_pc(&mut self.pst, &node) else {
                        continue;
                    };
                    // A table row carries no sender e-mail, so the sender check is
                    // repeated against the full properties before the body is read.
                    if matched.is_empty() && !needle.is_empty() && q.in_sender {
                        let email = pc.str(PID_SENDER_EMAIL).unwrap_or("").to_lowercase();
                        let rep = pc
                            .str(PID_SENT_REPRESENTING_NAME)
                            .unwrap_or("")
                            .to_lowercase();
                        if email.contains(&needle) || rep.contains(&needle) {
                            matched = "sender".into();
                        }
                    }
                    if matched.is_empty() && !needle.is_empty() && q.in_recipients {
                        let to = pc.str(PID_DISPLAY_TO).unwrap_or("").to_lowercase();
                        let cc = pc.str(ltp::PID_DISPLAY_CC).unwrap_or("").to_lowercase();
                        let bcc = pc.str(PID_DISPLAY_BCC).unwrap_or("").to_lowercase();
                        if to.contains(&needle) || cc.contains(&needle) || bcc.contains(&needle) {
                            matched = "recipients".into();
                        } else {
                            let rs = read_recipients(&mut self.pst, node.bid_sub);
                            if rs.iter().any(|r| {
                                r.name.to_lowercase().contains(&needle)
                                    || r.email.to_lowercase().contains(&needle)
                            }) {
                                matched = "recipients".into();
                            }
                        }
                    }
                    let mut attach_names: Option<Vec<String>> = None;
                    if attach_needle.is_some()
                        || (matched.is_empty() && q.in_attachment_names && !needle.is_empty())
                    {
                        let mut problems = Vec::new();
                        let names: Vec<String> = self
                            .attachments_of(&node, &mut problems)
                            .into_iter()
                            .map(|a| a.name.to_lowercase())
                            .collect();
                        attach_names = Some(names);
                    }
                    if let (Some(want), Some(names)) = (&attach_needle, &attach_names) {
                        if !names.iter().any(|n| n.contains(want)) {
                            continue;
                        }
                    }
                    if matched.is_empty() && !needle.is_empty() && q.in_attachment_names {
                        if let Some(names) = &attach_names {
                            if names.iter().any(|n| n.contains(&needle)) {
                                matched = "attachment".into();
                            }
                        }
                    }
                    if matched.is_empty()
                        && !needle.is_empty()
                        && q.in_body
                        && body_text(&pc).is_some_and(|b| b.to_lowercase().contains(&needle))
                    {
                        matched = "body".into();
                    }
                }
                if !needle.is_empty() && any_text && matched.is_empty() {
                    continue;
                }
                if matched.is_empty() {
                    matched = "filters".into();
                }
                hits.push(SearchHit {
                    summary: s,
                    matched,
                });
                if hits.len() >= limit {
                    truncated = true;
                    break 'outer;
                }
            }
        }
        hits.sort_by_key(|h| std::cmp::Reverse(h.summary.date_ms));
        SearchResult {
            hits,
            scanned,
            total,
            truncated,
            cancelled,
            elapsed_ms: started.elapsed().as_millis() as u64,
        }
    }

    /// Reopen the same file on a fresh handle, for a long job to run on its own thread
    /// while this one keeps answering the window.
    pub fn reopen(&self, on: crate::Progress) -> Result<Mailbox, String> {
        Mailbox::open(
            &self.path,
            OpenOptions {
                salvage: self.info.index_source == IndexSource::Recovered,
                cache_dir: None,
            },
            on,
        )
    }
}

fn folders_name_key(name: &str, nid: u32) -> (String, u32) {
    (name.to_lowercase(), nid)
}

/// The node and block index, following the header's roots and falling back to the
/// sweep and the carve when they are unusable (or when asked to ignore them).
fn build_index(
    pst: &mut Pst,
    salvage: bool,
) -> (Vec<Node>, Vec<Block>, Option<RecoveryInfo>, IndexSource) {
    let (mut nodes, mut blocks) = if salvage {
        (Vec::new(), Vec::new())
    } else {
        (pst.nodes(), pst.blocks())
    };
    if !nodes.is_empty() && !blocks.is_empty() {
        return (nodes, blocks, None, IndexSource::Header);
    }

    let r = pst.scan();
    let mut info = RecoveryInfo {
        pages_scanned: r.pages_scanned,
        nbt_pages: r.nbt_pages,
        bbt_pages: r.bbt_pages,
        damaged_pages: r.damaged_pages,
        superseded: r.superseded,
        unresolved: r.unresolved,
        carved_blocks: 0,
        swept_nodes: r.nodes.len(),
        stale: Vec::new(),
    };
    if blocks.is_empty() {
        let carved = pst.carve();
        info.carved_blocks = carved.len();
        let mut merged: BTreeMap<u64, Block> = carved.iter().map(|b| (b.bid & !1, *b)).collect();
        for b in &r.blocks {
            if !merged.contains_key(&(b.bid & !1)) && pst.block_intact(b) {
                merged.insert(b.bid & !1, *b);
            }
        }
        blocks = merged.into_values().collect();
        blocks.sort_by_key(|b| b.bid);
        pst.adopt(&blocks);
    }
    if nodes.is_empty() {
        nodes = r.nodes;
        info.stale = r
            .stale
            .iter()
            .map(|s| StaleNode {
                nid: s.nid,
                versions: s.versions,
                dangling: s.dangling,
                bid_data: s.bid_data,
            })
            .collect();
    }
    (nodes, blocks, Some(info), IndexSource::Recovered)
}

/// Text that is not text: anything below a space other than tab, newline and return.
fn has_control_chars(s: &str) -> bool {
    s.chars()
        .any(|c| (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r'))
}

fn has_attachment_subnode(pst: &mut Pst, node: &Node) -> bool {
    if node.bid_sub == 0 {
        return false;
    }
    pst.subnodes(node.bid_sub)
        .map(|s| s.keys().any(|k| k & 0x1F == NID_TYPE_ATTACHMENT))
        .unwrap_or(false)
}

/// A listing row from either a table row or a full property context.
fn summary_from_props(
    nid: u32,
    folder: u32,
    props: &BTreeMap<u16, Value>,
    from_table: bool,
) -> MessageSummary {
    let s = |id: u16| -> &str {
        match props.get(&id) {
            Some(Value::Str(s)) => s,
            _ => "",
        }
    };
    let i = |id: u16| -> Option<i64> {
        match props.get(&id) {
            Some(Value::Int(v)) => Some(*v),
            Some(Value::Bool(b)) => Some(*b as i64),
            _ => None,
        }
    };
    let t = |id: u16| -> Option<u64> {
        match props.get(&id) {
            Some(Value::Time(v)) => Some(*v),
            _ => None,
        }
    };
    let class = s(PID_MESSAGE_CLASS).to_string();
    let flags = i(PID_MESSAGE_FLAGS).unwrap_or(0);
    let subject = clean_subject(s(PID_SUBJECT)).to_string();
    MessageSummary {
        nid,
        folder,
        subject: if subject.trim().is_empty() {
            "(no subject)".into()
        } else {
            subject
        },
        // A contents-table row usually carries the "sent representing" pair rather than
        // the sender's own, and for ordinary mail the two are the same person.
        sender_name: {
            let n = s(PID_SENDER_NAME);
            if n.is_empty() {
                s(PID_SENT_REPRESENTING_NAME)
            } else {
                n
            }
            .to_string()
        },
        sender_email: {
            let e = s(PID_SENDER_EMAIL);
            let e = if e.is_empty() { s(0x5D01) } else { e };
            let e = if e.is_empty() {
                s(PID_SENT_REPRESENTING_EMAIL)
            } else {
                e
            };
            if e.is_empty() { s(0x5D02) } else { e }.to_string()
        },
        display_to: s(PID_DISPLAY_TO).to_string(),
        date_ms: t(PID_DELIVERY_TIME)
            .or_else(|| t(PID_SUBMIT_TIME))
            .or_else(|| t(PID_LAST_MODIFICATION_TIME))
            .or_else(|| t(PID_CREATION_TIME))
            .and_then(filetime_to_ms),
        size: i(PID_MESSAGE_SIZE).unwrap_or(0),
        has_attachments: i(PID_HAS_ATTACHMENTS).unwrap_or(0) != 0 || flags & 0x10 != 0,
        importance: i(PID_IMPORTANCE).unwrap_or(1) as i32,
        read: flags & MSGFLAG_READ != 0,
        item_type: item_type(&class),
        message_class: class,
        from_table,
        recovered: false,
    }
}

pub fn item_type(class: &str) -> ItemType {
    let c = class.to_ascii_lowercase();
    if c.starts_with("ipm.appointment") {
        ItemType::Appointment
    } else if c.starts_with("ipm.schedule.meeting") {
        ItemType::MeetingRequest
    } else if c.starts_with("ipm.contact") {
        ItemType::Contact
    } else if c.starts_with("ipm.distlist") {
        ItemType::DistributionList
    } else if c.starts_with("ipm.task") {
        ItemType::Task
    } else if c.starts_with("ipm.stickynote") {
        ItemType::Note
    } else if c.starts_with("ipm.activity") {
        ItemType::Journal
    } else if c.starts_with("report.") {
        ItemType::Report
    } else if c.starts_with("ipm.note") || c == "ipm" || c.is_empty() {
        ItemType::Mail
    } else {
        ItemType::Other
    }
}

fn attachment_info(nid: u32, pc: &Pc) -> AttachmentInfo {
    let method = pc.int(PID_ATTACH_METHOD).unwrap_or(ltp::ATTACH_BY_VALUE);
    let raw_name = pc
        .str(PID_ATTACH_LONG_FILENAME)
        .or(pc.str(PID_ATTACH_FILENAME))
        .or(pc.str(PID_DISPLAY_NAME))
        .unwrap_or("")
        .trim()
        .to_string();
    let is_embedded = method == 5;
    let name = if raw_name.is_empty() {
        if is_embedded {
            let subj = pc.str(PID_SUBJECT).unwrap_or("Embedded message").trim();
            format!(
                "{}.msg",
                if subj.is_empty() {
                    "Embedded message"
                } else {
                    subj
                }
            )
        } else {
            format!("attachment-{nid:X}")
        }
    } else {
        raw_name
    };
    let size = match pc.props.get(&PID_ATTACH_DATA) {
        Some(Value::Bytes(b)) => b.len() as u64,
        _ => pc.int(PID_ATTACH_SIZE).unwrap_or(0).max(0) as u64,
    };
    let extension = pc
        .str(PID_ATTACH_EXTENSION)
        .map(|e| e.trim_start_matches('.').to_ascii_lowercase())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| {
            name.rsplit_once('.')
                .map(|(_, e)| e.to_ascii_lowercase())
                .unwrap_or_default()
        });
    AttachmentInfo {
        nid,
        name: crate::fsutil::safe_name(&name, nid),
        mime: pc
            .str(PID_ATTACH_MIME_TAG)
            .map(str::to_string)
            .unwrap_or_else(|| mime_for(&extension).to_string()),
        size,
        method,
        is_embedded_message: is_embedded,
        extension,
        sha256: None,
    }
}

/// A MIME type for the handful of extensions worth knowing; everything else is bytes.
pub fn mime_for(ext: &str) -> &'static str {
    match ext {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "txt" | "log" => "text/plain",
        "htm" | "html" => "text/html",
        "csv" => "text/csv",
        "xml" => "application/xml",
        "json" => "application/json",
        "zip" => "application/zip",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "msg" => "application/vnd.ms-outlook",
        "eml" => "message/rfc822",
        "ics" => "text/calendar",
        "vcf" => "text/vcard",
        _ => "application/octet-stream",
    }
}

fn cheap_filters_pass(q: &SearchQuery, s: &MessageSummary) -> bool {
    if let Some(from) = q.date_from_ms {
        if s.date_ms.is_none_or(|d| d < from) {
            return false;
        }
    }
    if let Some(to) = q.date_to_ms {
        if s.date_ms.is_none_or(|d| d > to) {
            return false;
        }
    }
    if let Some(h) = q.has_attachments {
        if s.has_attachments != h {
            return false;
        }
    }
    if let Some(min) = q.min_size {
        if s.size < min {
            return false;
        }
    }
    if let Some(max) = q.max_size {
        if s.size > max {
            return false;
        }
    }
    if !q.item_types.is_empty() && !q.item_types.contains(&s.item_type) {
        return false;
    }
    if let Some(i) = q.importance {
        if s.importance != i {
            return false;
        }
    }
    if q.unread_only && s.read {
        return false;
    }
    true
}

/// A property value as text, for the properties view.
fn describe(v: &Value) -> (u16, &'static str, String, Option<usize>) {
    match v {
        Value::Int(i) => (0x0003, "Integer", i.to_string(), None),
        Value::Float(f) => (0x0005, "Floating", f.to_string(), None),
        Value::Bool(b) => (0x000B, "Boolean", b.to_string(), None),
        Value::Time(t) => (0x0040, "Time", ltp::filetime(*t).trim().to_string(), None),
        Value::Str(s) => (0x001F, "String", s.clone(), None),
        Value::Bytes(b) => (
            0x0102,
            "Binary",
            if b.len() <= HEX_PREVIEW {
                hex(b)
            } else {
                format!("{} ({} bytes)", hex(&b[..HEX_PREVIEW]), b.len())
            },
            Some(b.len()),
        ),
        Value::MissingSubnode(n) => (
            0x0000,
            "Missing",
            format!("subnode 0x{n:08X} is not in the subnode tree; the value is lost"),
            None,
        ),
        Value::Raw { ptype, bytes } => (
            *ptype,
            type_name(*ptype),
            describe_raw(*ptype, bytes),
            Some(bytes.len()),
        ),
    }
}

fn describe_raw(ptype: u16, b: &[u8]) -> String {
    match ptype {
        0x101F => multi_string(b, true).join("; "),
        0x101E => multi_string(b, false).join("; "),
        0x1003 if b.len() >= 4 => {
            let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
            (0..n.min((b.len() - 4) / 4))
                .map(|i| {
                    i32::from_le_bytes(b[4 + i * 4..8 + i * 4].try_into().unwrap()).to_string()
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
        0x1102 if b.len() >= 4 => {
            let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            format!("{n} binary values ({} bytes)", b.len())
        }
        _ if b.len() <= HEX_PREVIEW => hex(b),
        _ => format!("{} ({} bytes)", hex(&b[..HEX_PREVIEW]), b.len()),
    }
}

/// PtypMultipleString as stored in a PC value: a count, then offsets, then the strings.
pub fn multi_string(b: &[u8], wide: bool) -> Vec<String> {
    if b.len() < 4 {
        return Vec::new();
    }
    let n = u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize;
    if n == 0 || 4 + n * 4 > b.len() {
        return Vec::new();
    }
    let offs: Vec<usize> = (0..n)
        .map(|i| u32::from_le_bytes(b[4 + i * 4..8 + i * 4].try_into().unwrap()) as usize)
        .collect();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let start = offs[i];
        let end = if i + 1 < n { offs[i + 1] } else { b.len() };
        if start > end || end > b.len() {
            continue;
        }
        let s = &b[start..end];
        out.push(if wide {
            let u: Vec<u16> = s
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u)
        } else {
            crate::html::decode(s, None)
        });
    }
    out
}

fn hex(b: &[u8]) -> String {
    b.iter()
        .map(|x| format!("{x:02X}"))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Contact, appointment and task fields, resolved through the file's own name map.
///
/// Named-property ids are per file, so the map is consulted for every one: the id that
/// means "appointment start" in this PST means something else in the next.
fn extra_fields(pc: &Pc, names: &Names) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let class = pc.str(PID_MESSAGE_CLASS).unwrap_or("");
    let kind = item_type(class);

    // The id in this file whose name-map entry is `set lid`, if any.
    let named = |set: &str, lid: u32| -> Option<&Value> {
        let want = format!("{set} 0x{lid:04X}");
        (0x8000u32..=0xFFFF)
            .map(|i| i as u16)
            .find(|id| names.get(*id) == Some(want.as_str()))
            .and_then(|id| pc.props.get(&id))
    };
    let named_str = |set: &str, lid: u32| -> Option<String> {
        match named(set, lid) {
            Some(Value::Str(s)) if !s.trim().is_empty() => Some(s.clone()),
            _ => None,
        }
    };
    let named_time = |set: &str, lid: u32| -> Option<String> {
        match named(set, lid) {
            Some(Value::Time(t)) if *t != 0 => Some(ltp::filetime(*t).trim().to_string()),
            _ => None,
        }
    };
    let named_bool = |set: &str, lid: u32| -> Option<bool> {
        match named(set, lid) {
            Some(Value::Bool(b)) => Some(*b),
            Some(Value::Int(i)) => Some(*i != 0),
            _ => None,
        }
    };
    let push = |out: &mut Vec<(String, String)>, k: &str, v: Option<String>| {
        if let Some(v) = v {
            out.push((k.to_string(), v));
        }
    };
    let fixed = |id: u16| {
        pc.str(id)
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    };

    match kind {
        ItemType::Contact | ItemType::DistributionList => {
            push(&mut out, "Full name", fixed(PID_DISPLAY_NAME));
            push(
                &mut out,
                "File as",
                named_str("PSETID_Address", LID_FILE_UNDER),
            );
            push(&mut out, "First name", fixed(PID_GIVEN_NAME));
            push(&mut out, "Middle name", fixed(PID_MIDDLE_NAME));
            push(&mut out, "Last name", fixed(PID_SURNAME));
            push(&mut out, "Company", fixed(PID_COMPANY_NAME));
            push(&mut out, "Job title", fixed(PID_TITLE));
            push(&mut out, "Department", fixed(PID_DEPARTMENT_NAME));
            push(
                &mut out,
                "E-mail",
                named_str("PSETID_Address", LID_EMAIL1_ADDRESS),
            );
            push(
                &mut out,
                "E-mail 2",
                named_str("PSETID_Address", LID_EMAIL2_ADDRESS),
            );
            push(
                &mut out,
                "E-mail 3",
                named_str("PSETID_Address", LID_EMAIL3_ADDRESS),
            );
            push(&mut out, "Business phone", fixed(PID_BUSINESS_PHONE));
            push(&mut out, "Home phone", fixed(PID_HOME_PHONE));
            push(&mut out, "Mobile", fixed(PID_MOBILE_PHONE));
            push(&mut out, "Business fax", fixed(PID_BUSINESS_FAX));
            push(&mut out, "Web page", fixed(PID_BUSINESS_HOMEPAGE));
            push(
                &mut out,
                "IM address",
                named_str("PSETID_Address", LID_IM_ADDRESS),
            );
            let home = [
                fixed(PID_HOME_ADDRESS_STREET),
                fixed(PID_HOME_ADDRESS_CITY),
                fixed(PID_HOME_ADDRESS_STATE),
                fixed(PID_HOME_ADDRESS_POSTAL_CODE),
                fixed(PID_HOME_ADDRESS_COUNTRY),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            if !home.is_empty() {
                out.push(("Home address".into(), home));
            }
            let work = [
                fixed(PID_BUSINESS_ADDRESS_STREET),
                fixed(PID_BUSINESS_ADDRESS_CITY),
                fixed(PID_BUSINESS_ADDRESS_STATE),
                fixed(PID_BUSINESS_ADDRESS_POSTAL_CODE),
                fixed(PID_BUSINESS_ADDRESS_COUNTRY),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
            if !work.is_empty() {
                out.push(("Business address".into(), work));
            }
            push(
                &mut out,
                "Birthday",
                pc.time(PID_BIRTHDAY)
                    .filter(|t| *t != 0)
                    .map(|t| ltp::filetime(t).trim().to_string()),
            );
            push(
                &mut out,
                "Anniversary",
                pc.time(PID_WEDDING_ANNIVERSARY)
                    .filter(|t| *t != 0)
                    .map(|t| ltp::filetime(t).trim().to_string()),
            );
            push(&mut out, "Nickname", fixed(PID_NICKNAME));
            push(&mut out, "Spouse", fixed(PID_SPOUSE_NAME));
            push(&mut out, "Assistant", fixed(PID_ASSISTANT));
        }
        ItemType::Appointment | ItemType::MeetingRequest => {
            push(
                &mut out,
                "Start",
                named_time("PSETID_Appointment", LID_APPT_START_WHOLE),
            );
            push(
                &mut out,
                "End",
                named_time("PSETID_Appointment", LID_APPT_END_WHOLE),
            );
            push(
                &mut out,
                "Location",
                named_str("PSETID_Appointment", LID_LOCATION),
            );
            push(
                &mut out,
                "All day",
                named_bool("PSETID_Appointment", LID_APPT_ALL_DAY).map(|b| b.to_string()),
            );
            push(
                &mut out,
                "Recurring",
                named_bool("PSETID_Appointment", LID_RECURRING).map(|b| b.to_string()),
            );
            push(
                &mut out,
                "Show as",
                match named("PSETID_Appointment", LID_BUSY_STATUS) {
                    Some(Value::Int(0)) => Some("Free".into()),
                    Some(Value::Int(1)) => Some("Tentative".into()),
                    Some(Value::Int(2)) => Some("Busy".into()),
                    Some(Value::Int(3)) => Some("Out of office".into()),
                    Some(Value::Int(4)) => Some("Working elsewhere".into()),
                    _ => None,
                },
            );
            push(
                &mut out,
                "Reminder",
                named_bool("PSETID_Common", LID_REMINDER_SET).map(|b| b.to_string()),
            );
        }
        ItemType::Task => {
            push(
                &mut out,
                "Start date",
                named_time("PSETID_Task", LID_TASK_START_DATE),
            );
            push(
                &mut out,
                "Due date",
                named_time("PSETID_Task", LID_TASK_DUE_DATE),
            );
            push(
                &mut out,
                "Status",
                match named("PSETID_Task", LID_TASK_STATUS) {
                    Some(Value::Int(0)) => Some("Not started".into()),
                    Some(Value::Int(1)) => Some("In progress".into()),
                    Some(Value::Int(2)) => Some("Complete".into()),
                    Some(Value::Int(3)) => Some("Waiting on someone else".into()),
                    Some(Value::Int(4)) => Some("Deferred".into()),
                    _ => None,
                },
            );
            push(
                &mut out,
                "Percent complete",
                match named("PSETID_Task", LID_TASK_PERCENT_COMPLETE) {
                    Some(Value::Float(f)) => Some(format!("{:.0}%", f * 100.0)),
                    _ => None,
                },
            );
            push(
                &mut out,
                "Complete",
                named_bool("PSETID_Task", LID_TASK_COMPLETE).map(|b| b.to_string()),
            );
        }
        _ => {}
    }
    out
}

/// SHA-256, self-contained: an attachment hash is a report field, not a security
/// boundary, and a dependency for forty lines of arithmetic is not worth taking.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut msg = data.to_vec();
    let bit_len = (data.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());
    for chunk in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    h.iter().map(|x| format!("{x:08x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Option<Mailbox> {
        let p = PathBuf::from(format!("../../fixtures/data/{name}"));
        if !p.exists() {
            eprintln!("skipping {name}: run scripts/fetch-fixtures.ps1");
            return None;
        }
        Some(
            Mailbox::open(&p, OpenOptions::default(), &mut crate::silent()).expect("fixture opens"),
        )
    }

    #[test]
    fn sha256_known_answer() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn opens_a_pst_and_builds_the_tree() {
        let Some(mut mb) = fixture("dist-list.pst") else {
            return;
        };
        let info = mb.info().clone();
        assert_eq!(info.health, HealthStatus::Healthy, "{:?}", info.warnings);
        assert_eq!(info.index_source, IndexSource::Header);
        assert!(info.folder_count > 5);
        assert!(info.message_count >= 4);
        let folders = mb.folders();
        let inbox = folders
            .iter()
            .find(|f| f.name == "Inbox")
            .expect("an Inbox");
        assert_eq!(inbox.kind, FolderKind::Inbox);
        assert!(!inbox.orphan);
        assert!(inbox.path.ends_with("/Inbox"), "{}", inbox.path);
        let contacts = folders.iter().find(|f| f.name == "Contacts").unwrap();
        assert_eq!(contacts.kind, FolderKind::Contacts);
        let page = mb.list_messages(contacts.nid, 0, 50, SortKey::Date, true);
        assert_eq!(page.total, contacts.message_count);
        assert!(page.items.iter().all(|m| !m.subject.is_empty()));
        assert!(page
            .items
            .iter()
            .any(|m| matches!(m.item_type, ItemType::Contact | ItemType::DistributionList)));
        let first = page.items[0].nid;
        let detail = mb.message(first).expect("message detail");
        assert_eq!(detail.summary.nid, first);
        assert!(!detail.properties.is_empty());
    }

    #[test]
    fn opens_an_ost_and_lists_the_inbox_from_its_table() {
        let Some(mut mb) = fixture("example-2013.ost") else {
            return;
        };
        assert!(mb.info().is_ost);
        assert_eq!(mb.info().format, FileFormat::Unicode4k);
        let inbox = mb
            .folders()
            .into_iter()
            .find(|f| f.name == "Inbox")
            .unwrap();
        let page = mb.list_messages(inbox.nid, 0, 10, SortKey::Date, true);
        assert!(page.total >= 2, "{}", page.total);
        assert!(
            page.items.iter().any(|m| m.from_table),
            "contents table was not used"
        );
        let d = mb.message(page.items[0].nid).unwrap();
        assert!(d.body_text.is_some() || d.body_html.is_some());
    }

    #[test]
    fn search_finds_by_subject_and_by_body() {
        let Some(mut mb) = fixture("example-2013.ost") else {
            return;
        };
        let q = SearchQuery {
            text: "test".into(),
            in_subject: true,
            in_sender: true,
            in_body: true,
            in_recipients: true,
            ..Default::default()
        };
        let r = mb.search(&q, &mut crate::silent());
        assert!(!r.hits.is_empty());
        assert!(!r.cancelled);
        let none = mb.search(
            &SearchQuery {
                text: "zzz-no-such-string-zzz".into(),
                in_subject: true,
                in_body: true,
                ..Default::default()
            },
            &mut crate::silent(),
        );
        assert!(none.hits.is_empty());
        // Filters alone, with no text.
        let dated = mb.search(
            &SearchQuery {
                has_attachments: Some(false),
                ..Default::default()
            },
            &mut crate::silent(),
        );
        assert_eq!(dated.hits.len(), dated.scanned);
    }

    #[test]
    fn search_can_be_cancelled() {
        let Some(mut mb) = fixture("example-2013.ost") else {
            return;
        };
        let r = mb.search(
            &SearchQuery {
                text: "a".into(),
                in_subject: true,
                ..Default::default()
            },
            &mut |_, _| false,
        );
        assert!(r.cancelled);
    }

    /// Upstream's bake-off found that sweeping the OST returns seven messages where the
    /// intact index has three: four deleted items whose entries the index dropped and
    /// the freed pages kept. Those are exactly the ones the folder tables do not list,
    /// and they must be labelled as such rather than shown as ordinary mail.
    #[test]
    fn salvage_mode_labels_swept_deleted_items_as_recovered() {
        let p = PathBuf::from("../../fixtures/data/example-2013.ost");
        if !p.exists() {
            return;
        }
        let mut intact = Mailbox::open(&p, OpenOptions::default(), &mut crate::silent()).unwrap();
        let mut salvaged = Mailbox::open(
            &p,
            OpenOptions {
                salvage: true,
                cache_dir: None,
            },
            &mut crate::silent(),
        )
        .unwrap();
        assert_eq!(salvaged.info().index_source, IndexSource::Recovered);
        assert!(salvaged.info().message_count > intact.info().message_count);
        let folders: Vec<u32> = salvaged.folders().iter().map(|f| f.nid).collect();
        let mut recovered = 0;
        let mut plain = 0;
        for f in folders {
            for m in salvaged.all_summaries(f) {
                if m.recovered {
                    recovered += 1;
                } else {
                    plain += 1;
                }
            }
        }
        assert_eq!(
            plain,
            intact.info().message_count,
            "the live messages are still plain"
        );
        assert_eq!(
            recovered,
            salvaged.info().message_count - intact.info().message_count,
            "every extra message is labelled recovered"
        );
        // And on the intact file nothing is labelled.
        let inbox = intact
            .folders()
            .into_iter()
            .find(|f| f.name == "Inbox")
            .unwrap();
        assert!(intact.all_summaries(inbox.nid).iter().all(|m| !m.recovered));
    }

    #[test]
    fn a_torn_file_opens_as_recovered() {
        let src = PathBuf::from("../../fixtures/data/dist-list.pst");
        if !src.exists() {
            return;
        }
        let mut b = std::fs::read(&src).unwrap();
        let nbt = u64::from_le_bytes(b[224..232].try_into().unwrap()) as usize;
        let bbt = u64::from_le_bytes(b[240..248].try_into().unwrap()) as usize;
        b[nbt..nbt + 512].fill(0);
        b[bbt..bbt + 512].fill(0);
        let torn = std::env::temp_dir().join("ostpst-mailbox-torn.pst");
        std::fs::write(&torn, &b).unwrap();
        let mut mb = Mailbox::open(&torn, OpenOptions::default(), &mut crate::silent()).unwrap();
        assert_eq!(mb.info().index_source, IndexSource::Recovered);
        assert_eq!(mb.info().health, HealthStatus::Recovered);
        assert!(mb.info().recovery.as_ref().unwrap().carved_blocks > 100);
        let inbox = mb
            .folders()
            .into_iter()
            .find(|f| f.name == "Contacts")
            .unwrap();
        let page = mb.list_messages(inbox.nid, 0, 10, SortKey::Date, true);
        assert!(page.total >= 2);
        let _ = std::fs::remove_file(torn);
    }
}
