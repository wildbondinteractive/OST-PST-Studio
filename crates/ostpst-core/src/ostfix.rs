//! Make an OST's store and folders look to Outlook the way a PST's do.
//!
//! An OST is Exchange's cache, and the things Outlook uses to find its way around a
//! store are Exchange's: every entry id on the message store and on the folders names
//! the Exchange store's GUID, not this file's record key. A PST reader resolves an entry
//! id by matching that GUID against the store it has open, so an id carrying a foreign
//! GUID points nowhere. The first conversion opened in Outlook (upstream issue #1) showed
//! what the store needs; opening the folders showed the rest: Outlook sat spinning on a
//! folder whose special-folder links (`PidTagIpmAppointmentEntryId` and friends) all
//! pointed into a store it could not find.
//!
//! So three things are rewritten, and only these. The **message store** gets the entry
//! ids MS-PST 2.4.3.1 requires plus the optional ones a PST normally carries (Outbox,
//! Sent Items, Views, Common Views), each pointing at the OST folder of that role, and
//! loses the properties that carry Exchange ids. The **root folder** and the **Inbox** get
//! the special-folder entry ids a PST keeps there (Calendar, Contacts, Journal, Notes,
//! Tasks, Reminders, Drafts), resolved by container class and name among the OST's own
//! folders. Any **other folder** that carries one of those tags with a foreign id has it
//! removed, because a link that points nowhere is worse than none: Outlook creates a
//! fresh special folder when a link is absent, and spins when it is wrong.
//!
//! Nothing else about a folder changes, and no message is touched. Roles are resolved
//! by container class first and by the English folder name second, so a localised OST
//! still gets its Calendar and Contacts by class; only Drafts, Outbox and Sent Items
//! depend on the name, and a miss there costs a link, not data.

use crate::convert::{pc_heap, Hnid};
use crate::ltp::{bth_records, is_inline, read_node_pc, Heap, NID_ROOT_FOLDER};
use crate::ndb::{Node, Pst};
use std::collections::BTreeMap;

const PID_RECORD_KEY: u16 = 0x0FF9;
const PID_DISPLAY_NAME: u16 = 0x3001;
const PID_CONTAINER_CLASS: u16 = 0x3613;
const PID_VALID_FOLDER_MASK: u16 = 0x35DF;
const PID_IPM_SUBTREE: u16 = 0x35E0;
const PID_IPM_OUTBOX: u16 = 0x35E2;
const PID_WASTEBASKET: u16 = 0x35E3;
const PID_IPM_SENTMAIL: u16 = 0x35E4;
const PID_IPM_VIEWS: u16 = 0x35E5;
const PID_IPM_COMMON_VIEWS: u16 = 0x35E6;
const PID_FINDER: u16 = 0x35E7;
const PID_PST_PASSWORD: u16 = 0x67FF;
/// The OST's own store GUID, which is what every Exchange entry id in it names.
const PID_OST_STORE_GUID: u16 = 0x6615;
/// A string the OST keeps on its store that is not text in a PST.
const PID_OST_JUNK_STRING: u16 = 0x6812;
const PID_ITEM_PROC_SEARCH: u16 = 0x3416;
const PID_IPM_APPOINTMENT: u16 = 0x36D0;
const PID_IPM_CONTACT: u16 = 0x36D1;
const PID_IPM_JOURNAL: u16 = 0x36D2;
const PID_IPM_NOTE: u16 = 0x36D3;
const PID_IPM_TASK: u16 = 0x36D4;
const PID_REM_SEARCH: u16 = 0x36D5;
const PID_IPM_DRAFTS: u16 = 0x36D7;
const PID_ADDITIONAL_REN: u16 = 0x36D8;
const PID_ADDITIONAL_REN_EX: u16 = 0x36D9;
const PID_FREEBUSY_ENTRYIDS: u16 = 0x36E4;
const PID_OST_FOLDER_LINK: u16 = 0x6814;
const PID_OST_STORE_LINK: u16 = 0x681B;
const NID_MESSAGE_STORE: u32 = 0x21;
const NID_TYPE_FOLDER: u8 = 0x02;
const NID_TYPE_SEARCH_FOLDER: u8 = 0x03;
const NID_TYPE_RECEIVE_FOLDER_TABLE: u8 = 0x0B;

// PidTagValidFolderMask bits, MS-OXCSTOR.
const MASK_SUBTREE: u32 = 0x01;
const MASK_INBOX: u32 = 0x02;
const MASK_OUTBOX: u32 = 0x04;
const MASK_WASTEBASKET: u32 = 0x08;
const MASK_SENTMAIL: u32 = 0x10;
const MASK_VIEWS: u32 = 0x20;
const MASK_COMMON_VIEWS: u32 = 0x40;
const MASK_FINDER: u32 = 0x80;

/// The tags whose value is an entry id, on the store or on a folder.
const ENTRY_ID_TAGS: [u16; 14] = [
    PID_ITEM_PROC_SEARCH,
    PID_IPM_SUBTREE,
    PID_IPM_OUTBOX,
    PID_WASTEBASKET,
    PID_IPM_SENTMAIL,
    PID_IPM_VIEWS,
    PID_IPM_COMMON_VIEWS,
    PID_FINDER,
    PID_IPM_APPOINTMENT,
    PID_IPM_CONTACT,
    PID_IPM_JOURNAL,
    PID_IPM_NOTE,
    PID_IPM_TASK,
    PID_REM_SEARCH,
];

struct FolderMeta {
    nid: u32,
    parent: u32,
    search: bool,
    name: String,
    class: String,
}

/// Every folder role a PST links to, resolved among the OST's folders.
pub struct Special {
    record_key: Vec<u8>,
    ost_guid: Option<Vec<u8>>,
    pub subtree: u32,
    pub wastebasket: u32,
    pub finder: u32,
    pub inbox: Option<u32>,
    pub outbox: Option<u32>,
    pub sentmail: Option<u32>,
    pub views: Option<u32>,
    pub common_views: Option<u32>,
    pub drafts: Option<u32>,
    pub appointment: Option<u32>,
    pub contact: Option<u32>,
    pub journal: Option<u32>,
    pub note: Option<u32>,
    pub task: Option<u32>,
    pub reminders: Option<u32>,
    pub item_proc: Option<u32>,
    has_receive_table: bool,
}

/// A property context's records as stored: property type and where the value lives.
pub(crate) type RawProps = BTreeMap<u16, (u16, Hnid)>;

/// A property context's records, exactly as stored, keyed by property id.
pub(crate) fn raw_props(pst: &mut Pst, bid_data: u64) -> Result<(Heap, RawProps), String> {
    let heap = Heap::new(pst.node_blocks(bid_data)?)?;
    let (_, records) = bth_records(&heap)?;
    let mut props = BTreeMap::new();
    for r in records.iter().filter(|r| r.len() >= 8) {
        let (id, ptype) = (
            u16::from_le_bytes([r[0], r[1]]),
            u16::from_le_bytes([r[2], r[3]]),
        );
        let raw = u32::from_le_bytes([r[4], r[5], r[6], r[7]]);
        let v = if is_inline(ptype) {
            Hnid::Inline(raw)
        } else if raw & 0x1F == 0 {
            Hnid::Heap(heap.item(raw)?.to_vec())
        } else {
            Hnid::Sub(raw)
        };
        props.insert(id, (ptype, v));
    }
    Ok((heap, props))
}

impl Special {
    /// Work out which OST folder plays which role. Fails only when the mailbox itself
    /// cannot be found, which is the one case a store cannot be built at all.
    pub fn resolve(pst: &mut Pst, nodes: &[Node]) -> Result<Special, String> {
        let store = nodes
            .iter()
            .find(|n| n.nid == NID_MESSAGE_STORE)
            .ok_or("the file has no message store node")?;
        let (_, store_props) = raw_props(pst, store.bid_data)?;
        let record_key = match store_props.get(&PID_RECORD_KEY) {
            Some((_, Hnid::Heap(k))) if k.len() == 16 => k.clone(),
            _ => {
                return Err("the message store has no record key, so no folder can be named".into())
            }
        };
        let ost_guid = match store_props.get(&PID_OST_STORE_GUID) {
            Some((_, Hnid::Heap(g))) if g.len() == 16 => Some(g.clone()),
            _ => None,
        };

        let mut folders: Vec<FolderMeta> = Vec::new();
        for n in nodes
            .iter()
            .filter(|n| matches!(n.nid_type(), NID_TYPE_FOLDER | NID_TYPE_SEARCH_FOLDER))
        {
            if n.nid == NID_ROOT_FOLDER {
                continue;
            }
            let Ok(pc) = read_node_pc(pst, n) else {
                continue;
            };
            folders.push(FolderMeta {
                nid: n.nid,
                parent: n.nid_parent,
                search: n.nid_type() == NID_TYPE_SEARCH_FOLDER,
                name: pc.str(PID_DISPLAY_NAME).unwrap_or("").to_string(),
                class: pc
                    .str(PID_CONTAINER_CLASS)
                    .unwrap_or("")
                    .to_ascii_lowercase(),
            });
        }
        let child = |parent: u32, name: &str| -> Option<u32> {
            folders
                .iter()
                .find(|f| f.parent == parent && !f.search && f.name.eq_ignore_ascii_case(name))
                .map(|f| f.nid)
        };
        let mailbox = child(NID_ROOT_FOLDER, "Root - Mailbox").ok_or(
            "no \"Root - Mailbox\" folder under the root, so there is no mailbox to point at",
        )?;
        let subtree = child(mailbox, "IPM_SUBTREE").ok_or("no IPM_SUBTREE in the mailbox")?;
        let finder = child(mailbox, "Finder").ok_or("no Finder folder in the mailbox")?;
        let wastebasket = child(subtree, "Deleted Items")
            .or_else(|| {
                folders
                    .iter()
                    .filter(|f| f.parent == subtree && !f.search)
                    .map(|f| f.nid)
                    .min()
            })
            .ok_or("the IPM subtree has no folders in it at all")?;

        // A role by container class, among the subtree's own folders first and the rest
        // of the mailbox second; a name that starts with the English default breaks ties.
        let by_class = |class: &str, preferred: &str| -> Option<u32> {
            let candidates: Vec<&FolderMeta> = folders
                .iter()
                .filter(|f| !f.search && f.class.starts_with(class))
                .collect();
            candidates
                .iter()
                .find(|f| f.parent == subtree && f.name.starts_with(preferred))
                .or_else(|| candidates.iter().find(|f| f.parent == subtree))
                .or_else(|| candidates.first())
                .map(|f| f.nid)
        };
        let search_by_class = |class: &str| -> Option<u32> {
            folders
                .iter()
                .find(|f| f.search && f.class == class)
                .map(|f| f.nid)
        };

        Ok(Special {
            record_key,
            ost_guid,
            subtree,
            wastebasket,
            finder,
            inbox: child(subtree, "Inbox"),
            outbox: child(subtree, "Outbox"),
            sentmail: child(subtree, "Sent Items"),
            views: child(mailbox, "Views"),
            common_views: child(mailbox, "Common Views"),
            drafts: child(subtree, "Drafts"),
            appointment: by_class("ipf.appointment", "Calendar"),
            contact: by_class("ipf.contact", "Contacts"),
            journal: by_class("ipf.journal", "Journal"),
            note: by_class("ipf.stickynote", "Notes"),
            task: by_class("ipf.task", "Tasks"),
            reminders: search_by_class("outlook.reminder"),
            item_proc: search_by_class("outlook.itemprocessor"),
            has_receive_table: nodes
                .iter()
                .any(|n| n.nid_type() == NID_TYPE_RECEIVE_FOLDER_TABLE),
        })
    }

    /// An entry id in a PST: four flag bytes, the store's record key, and a node id.
    /// Checked against the fixture Outlook wrote, whose IPM subtree id names node 0x8022.
    pub fn entry(&self, nid: u32) -> Vec<u8> {
        let mut e = vec![0u8; 4];
        e.extend_from_slice(&self.record_key);
        e.extend_from_slice(&nid.to_le_bytes());
        e
    }

    /// True for an entry id that names some other store: this file's record key is not
    /// the GUID in it.
    fn is_foreign(&self, bytes: &[u8]) -> bool {
        bytes.len() >= 24 && bytes[4..20] != self.record_key[..]
    }

    /// True for any binary that carries the OST's own store GUID, whatever the tag.
    fn carries_ost_guid(&self, bytes: &[u8]) -> bool {
        match &self.ost_guid {
            Some(g) => bytes.len() >= 20 && bytes[4..20] == g[..],
            None => false,
        }
    }

    /// Set `tag` to point at `nid`, or remove it when the role was not found.
    fn link(&self, props: &mut RawProps, tag: u16, nid: Option<u32>) -> bool {
        match nid {
            Some(n) => {
                props.insert(tag, (0x0102, Hnid::Heap(self.entry(n))));
                true
            }
            None => {
                props.remove(tag_key(&tag));
                false
            }
        }
    }

    /// The message store as a PST needs it: every property the OST's store had that a
    /// PST can use, the entry ids MS-PST 2.4.3.1 requires and the ones a PST normally
    /// carries, a display name, and a password of 0.
    pub fn store_heap(&self, pst: &mut Pst, store: &Node) -> Result<Vec<u8>, String> {
        let (_, mut props) = raw_props(pst, store.bid_data)?;

        let mut mask = MASK_SUBTREE | MASK_WASTEBASKET | MASK_FINDER;
        self.link(&mut props, PID_IPM_SUBTREE, Some(self.subtree));
        self.link(&mut props, PID_WASTEBASKET, Some(self.wastebasket));
        self.link(&mut props, PID_FINDER, Some(self.finder));
        if self.link(&mut props, PID_IPM_OUTBOX, self.outbox) {
            mask |= MASK_OUTBOX;
        }
        if self.link(&mut props, PID_IPM_SENTMAIL, self.sentmail) {
            mask |= MASK_SENTMAIL;
        }
        if self.link(&mut props, PID_IPM_VIEWS, self.views) {
            mask |= MASK_VIEWS;
        }
        if self.link(&mut props, PID_IPM_COMMON_VIEWS, self.common_views) {
            mask |= MASK_COMMON_VIEWS;
        }
        if self.has_receive_table && self.inbox.is_some() {
            mask |= MASK_INBOX;
        }
        props.insert(PID_VALID_FOLDER_MASK, (0x0003, Hnid::Inline(mask)));
        // The two links a real PST's store also carries, when their targets exist.
        if props.contains_key(&PID_IPM_DRAFTS) || self.drafts.is_some() {
            self.link(&mut props, PID_IPM_DRAFTS, self.drafts);
        }
        if props.contains_key(&PID_ITEM_PROC_SEARCH) || self.item_proc.is_some() {
            self.link(&mut props, PID_ITEM_PROC_SEARCH, self.item_proc);
        }
        props.insert(PID_PST_PASSWORD, (0x0003, Hnid::Inline(0)));
        props.remove(&PID_OST_JUNK_STRING);
        props.remove(&PID_OST_STORE_LINK);
        // Exchange's own store GUID has no meaning in a PST, and leaving it in is how a
        // reader could be led to believe the file is still a cache of that store.
        props.remove(&PID_OST_STORE_GUID);
        self.drop_foreign(&mut props);

        let has_name =
            matches!(props.get(&PID_DISPLAY_NAME), Some((_, Hnid::Heap(b))) if !b.is_empty());
        if !has_name {
            let name: Vec<u8> = "Personal Folders"
                .encode_utf16()
                .flat_map(|c| c.to_le_bytes())
                .collect();
            props.insert(PID_DISPLAY_NAME, (0x001F, Hnid::Heap(name)));
        }
        pc_heap(&props)
    }

    /// A folder's properties as a PST needs them, or `None` when nothing in them needs
    /// to change and the node can be copied as it is.
    ///
    /// The root folder and the Inbox get the special-folder links a PST keeps there.
    /// Any folder loses links that name another store.
    pub fn folder_heap(&self, pst: &mut Pst, folder: &Node) -> Result<Option<Vec<u8>>, String> {
        let is_root = folder.nid == NID_ROOT_FOLDER;
        let is_inbox = Some(folder.nid) == self.inbox;
        let (_, mut props) = raw_props(pst, folder.bid_data)?;
        let before = props.len();
        let had_foreign = props.iter().any(|(id, (_, v))| match v {
            Hnid::Heap(b) => {
                (ENTRY_ID_TAGS.contains(id)
                    || matches!(
                        *id,
                        PID_IPM_DRAFTS
                            | PID_ADDITIONAL_REN_EX
                            | PID_OST_FOLDER_LINK
                            | PID_OST_STORE_LINK
                    ))
                    && self.is_foreign(b)
                    || (*id == PID_ADDITIONAL_REN || *id == PID_FREEBUSY_ENTRYIDS)
                        && self.carries_ost_guid_anywhere(b)
            }
            _ => false,
        });
        if !is_root && !is_inbox && !had_foreign {
            return Ok(None);
        }

        self.drop_foreign(&mut props);
        if is_root || is_inbox {
            self.link(&mut props, PID_IPM_APPOINTMENT, self.appointment);
            self.link(&mut props, PID_IPM_CONTACT, self.contact);
            self.link(&mut props, PID_IPM_JOURNAL, self.journal);
            self.link(&mut props, PID_IPM_NOTE, self.note);
            self.link(&mut props, PID_IPM_TASK, self.task);
            self.link(&mut props, PID_REM_SEARCH, self.reminders);
            self.link(&mut props, PID_IPM_DRAFTS, self.drafts);
        }
        let _ = before;
        pc_heap(&props).map(Some)
    }

    /// Remove every property whose value is an entry id (or a list of them) that names
    /// a store other than this one. A link that points nowhere makes Outlook search for
    /// the missing store; a missing link makes it create the folder afresh.
    fn drop_foreign(&self, props: &mut RawProps) {
        let doomed: Vec<u16> = props
            .iter()
            .filter(|(id, (ptype, v))| {
                let Hnid::Heap(b) = v else { return false };
                let known = ENTRY_ID_TAGS.contains(id)
                    || matches!(
                        **id,
                        PID_IPM_DRAFTS
                            | PID_ADDITIONAL_REN_EX
                            | PID_OST_FOLDER_LINK
                            | PID_OST_STORE_LINK
                    );
                if known && self.is_foreign(b) {
                    return true;
                }
                if (**id == PID_ADDITIONAL_REN || **id == PID_FREEBUSY_ENTRYIDS)
                    && self.carries_ost_guid_anywhere(b)
                {
                    return true;
                }
                // Named properties and anything else binary that names the OST's store.
                *ptype == 0x0102 && **id >= 0x8000 && self.carries_ost_guid(b)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in doomed {
            props.remove(&id);
        }
    }

    /// A multi-valued binary (`PidTagAdditionalRenEntryIds`) holds the GUID several
    /// times over; any occurrence is enough to condemn it.
    fn carries_ost_guid_anywhere(&self, bytes: &[u8]) -> bool {
        match &self.ost_guid {
            Some(g) => bytes.windows(16).any(|w| w == g.as_slice()),
            None => false,
        }
    }
}

fn tag_key(tag: &u16) -> &u16 {
    tag
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_resolve_by_class_and_name_in_the_ost_fixture() {
        let src = "../../fixtures/data/example-2013.ost";
        if !std::path::Path::new(src).exists() {
            return;
        }
        let mut ost = Pst::open(src).unwrap();
        let nodes = ost.nodes();
        let s = Special::resolve(&mut ost, &nodes).expect("the OST's mailbox resolves");
        assert_eq!(s.subtree, 0x2142);
        assert_eq!(s.wastebasket, 0x2162);
        assert_eq!(s.finder, 0x20E2);
        assert_eq!(s.inbox, Some(0x2182));
        assert_eq!(s.outbox, Some(0x21A2));
        assert_eq!(s.sentmail, Some(0x21C2));
        assert_eq!(s.views, Some(0x2122));
        assert_eq!(s.common_views, Some(0x20C2));
        assert_eq!(s.drafts, Some(0x8062));
        assert_eq!(s.appointment, Some(0x81C2), "Calendar by IPF.Appointment");
        assert_eq!(
            s.contact,
            Some(0x81E2),
            "Contacts by IPF.Contact, not the Contact Search folder"
        );
        assert_eq!(s.journal, Some(0x8202));
        assert_eq!(s.note, Some(0x8222));
        assert_eq!(s.task, Some(0x8242), "Tasks by IPF.Task, not To-Do Search");
        assert_eq!(s.reminders, Some(0x80023));
        assert_eq!(s.item_proc, Some(0x80043));
        assert!(s.ost_guid.is_some(), "the OST names its own store GUID");
        assert!(s.is_foreign(&[0u8; 24]));
        assert!(!s.is_foreign(&s.entry(0x2142)));
    }
}
