//! The serialisable shape of everything the engine hands to a front end.
//!
//! Every type here crosses a boundary: Tauri commands return them to the browser, the
//! CLI prints them as JSON, and reports are written from them. They are deliberately
//! plain data with `camelCase` names, so the TypeScript side mirrors them one to one.

use serde::{Deserialize, Serialize};

/// Which physical layout a file is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FileFormat {
    /// Outlook 2003 and later PST, and pre-2013 OST: 512-byte pages, 64-bit ids.
    Unicode,
    /// Outlook 2013 and later OST: 4K pages, zlib-compressed blocks.
    Unicode4k,
    /// Outlook 97 to 2002: 512-byte pages, 32-bit ids, 2GB ceiling.
    Ansi,
}

/// Where the node and block index came from when the file was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexSource {
    /// Both B-trees were walked from the header's roots and were usable.
    Header,
    /// The header's index was unusable or bypassed, and the index was rebuilt by
    /// sweeping surviving B-tree pages and carving blocks out of the file itself.
    Recovered,
}

/// How a file was judged on opening. This is the headline a user sees first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HealthStatus {
    /// Header, index and every structure touched so far checks out.
    Healthy,
    /// Opened from its own index, but something was wrong along the way: a failed
    /// checksum, a truncated tail, invalid allocation maps.
    Damaged,
    /// The index was gone or unusable and the contents were recovered by sweeping.
    Recovered,
    /// Opened, but essential structures (message store, root folder) are missing.
    Critical,
}

/// A node recovered at a revision that cannot be vouched for.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StaleNode {
    pub nid: u32,
    pub versions: usize,
    pub dangling: bool,
    pub bid_data: u64,
}

/// Everything known about an opened file before any folder is read.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub file_name: String,
    pub size_bytes: u64,
    pub declared_bytes: u64,
    /// `true` for an OST by header signature, whatever the file is called.
    pub is_ost: bool,
    pub format: FileFormat,
    pub format_version: u16,
    pub encoding: String,
    pub amap_valid: bool,
    pub index_source: IndexSource,
    pub health: HealthStatus,
    pub node_count: usize,
    pub block_count: usize,
    pub folder_count: usize,
    pub message_count: usize,
    /// Nodes of every type the file holds, for the diagnostics view.
    pub node_types: Vec<NodeTypeCount>,
    pub warnings: Vec<String>,
    /// Total problems seen, including any past the listing cap on `warnings`.
    pub problem_count: usize,
    /// Set when the index had to be recovered: how the sweep went.
    pub recovery: Option<RecoveryInfo>,
    pub essentials_missing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeTypeCount {
    pub nid_type: u8,
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryInfo {
    pub pages_scanned: u64,
    pub nbt_pages: usize,
    pub bbt_pages: usize,
    pub damaged_pages: usize,
    pub superseded: usize,
    pub unresolved: usize,
    pub carved_blocks: usize,
    pub swept_nodes: usize,
    pub stale: Vec<StaleNode>,
}

/// What a folder is for, as far as its container class and name say.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FolderKind {
    Root,
    Inbox,
    Outbox,
    SentItems,
    DeletedItems,
    Drafts,
    JunkEmail,
    Archive,
    Calendar,
    Contacts,
    Tasks,
    Notes,
    Journal,
    Search,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderInfo {
    pub nid: u32,
    pub parent: u32,
    pub name: String,
    /// Display path from the mailbox root, `Top of Personal Folders/Inbox`.
    pub path: String,
    pub depth: usize,
    pub kind: FolderKind,
    pub container_class: Option<String>,
    /// What the folder's own properties say it holds.
    pub content_count: i64,
    pub unread_count: i64,
    /// What the index actually holds for it: messages whose parent is this folder.
    pub message_count: usize,
    pub subfolder_count: usize,
    pub children: Vec<u32>,
    pub unreadable: bool,
    /// Not reachable from the root folder: a damaged parent chain.
    pub orphan: bool,
    pub has_children: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ItemType {
    Mail,
    Appointment,
    Contact,
    DistributionList,
    Task,
    Note,
    Journal,
    MeetingRequest,
    Report,
    Other,
}

/// One row of a message list. Small on purpose: a 200,000-message folder is listed a
/// page at a time, and this is what one row costs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageSummary {
    pub nid: u32,
    pub folder: u32,
    pub subject: String,
    pub sender_name: String,
    pub sender_email: String,
    pub display_to: String,
    /// Unix milliseconds, UTC. Delivery time, else client submit time.
    pub date_ms: Option<i64>,
    pub size: i64,
    pub has_attachments: bool,
    /// 0 low, 1 normal, 2 high.
    pub importance: i32,
    pub read: bool,
    pub item_type: ItemType,
    pub message_class: String,
    /// The summary came from the folder's contents table rather than the message's
    /// own properties. Cheaper, and the fields a table does not carry are blank.
    pub from_table: bool,
    /// The index holds this message but the folder's own contents table does not list
    /// it. On a file opened in salvage mode that is a deleted item the sweep brought
    /// back; on an intact file it is a disagreement worth looking at.
    pub recovered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePage {
    pub folder: u32,
    pub offset: usize,
    pub total: usize,
    pub items: Vec<MessageSummary>,
    /// Messages in this folder whose properties could not be read at all.
    pub unreadable: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SortKey {
    Date,
    Sender,
    Subject,
    Size,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecipientInfo {
    /// 1 To, 2 Cc, 3 Bcc.
    pub kind: i64,
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    /// The attachment's local subnode id within its message.
    pub nid: u32,
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// PidTagAttachMethod: 1 by value, 5 embedded message, 2/3/4 links, 6 OLE.
    pub method: i64,
    pub is_embedded_message: bool,
    pub extension: String,
    /// SHA-256 of the bytes, when they were read.
    pub sha256: Option<String>,
}

/// One raw MAPI property, for the properties view and the JSON export.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PropertyView {
    pub tag: u16,
    pub ptype: u16,
    pub name: Option<String>,
    pub type_name: String,
    pub value: String,
    /// Bytes for a binary value under this many bytes, as hex, for the viewer.
    pub bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageDetail {
    pub summary: MessageSummary,
    pub recipients: Vec<RecipientInfo>,
    pub sender_representing_name: String,
    pub sender_representing_email: String,
    pub body_text: Option<String>,
    /// The HTML body decoded to a string, untouched. The front end sanitises it.
    pub body_html: Option<String>,
    pub transport_headers: Option<String>,
    pub internet_message_id: Option<String>,
    pub conversation_topic: Option<String>,
    pub created_ms: Option<i64>,
    pub modified_ms: Option<i64>,
    pub sent_ms: Option<i64>,
    pub received_ms: Option<i64>,
    pub creator_name: Option<String>,
    pub last_modifier_name: Option<String>,
    pub sensitivity: i64,
    pub attachments: Vec<AttachmentInfo>,
    pub properties: Vec<PropertyView>,
    /// Fields a contact, appointment or task carries, when the item is one.
    pub extra: Vec<(String, String)>,
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchQuery {
    pub text: String,
    pub in_subject: bool,
    pub in_sender: bool,
    pub in_recipients: bool,
    pub in_body: bool,
    pub in_attachment_names: bool,
    /// Restrict to one folder and everything under it.
    pub folder: Option<u32>,
    pub date_from_ms: Option<i64>,
    pub date_to_ms: Option<i64>,
    pub has_attachments: Option<bool>,
    pub attachment_name: Option<String>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    pub item_types: Vec<ItemType>,
    pub importance: Option<i32>,
    pub unread_only: bool,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub summary: MessageSummary,
    pub matched: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub hits: Vec<SearchHit>,
    pub scanned: usize,
    pub total: usize,
    pub truncated: bool,
    pub cancelled: bool,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportFormat {
    Pst,
    Eml,
    Msg,
    Mbox,
    Html,
    Txt,
    Csv,
    Vcf,
    Ics,
    Json,
}

impl ExportFormat {
    pub fn parse(s: &str) -> Option<ExportFormat> {
        Some(match s.to_ascii_lowercase().as_str() {
            "pst" => ExportFormat::Pst,
            "eml" => ExportFormat::Eml,
            "msg" => ExportFormat::Msg,
            "mbox" => ExportFormat::Mbox,
            "html" => ExportFormat::Html,
            "txt" | "text" => ExportFormat::Txt,
            "csv" => ExportFormat::Csv,
            "vcf" | "vcard" => ExportFormat::Vcf,
            "ics" | "ical" => ExportFormat::Ics,
            "json" => ExportFormat::Json,
            _ => return None,
        })
    }

    pub fn extension(self) -> &'static str {
        match self {
            ExportFormat::Pst => "pst",
            ExportFormat::Eml => "eml",
            ExportFormat::Msg => "msg",
            ExportFormat::Mbox => "mbox",
            ExportFormat::Html => "html",
            ExportFormat::Txt => "txt",
            ExportFormat::Csv => "csv",
            ExportFormat::Vcf => "vcf",
            ExportFormat::Ics => "ics",
            ExportFormat::Json => "json",
        }
    }
}

/// What an export covers. Folders include everything under them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Selection {
    pub entire_mailbox: bool,
    pub folders: Vec<u32>,
    pub messages: Vec<u32>,
}

/// What to do when an output path already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum Collision {
    /// Refuse and report, which is the only safe default.
    #[default]
    Fail,
    /// Write beside it with a numeric suffix.
    Rename,
    /// Replace it. Only when the user said so.
    Overwrite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvertOptions {
    pub source: String,
    pub destination: String,
    /// Ignore the header's index and rebuild it by sweeping, even if it looks fine.
    pub salvage: bool,
    pub collision: Collision,
    /// Run the post-write validation (reopen, compare counts, check essentials).
    pub validate: bool,
    /// Write valid allocation maps so Outlook opens the file without SCANPST.
    pub write_allocation_maps: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportOptions {
    pub source: String,
    pub destination: String,
    pub format: ExportFormat,
    pub selection: Selection,
    pub salvage: bool,
    pub collision: Collision,
    /// For attachment extraction alongside EML/HTML/TXT: write attachments as files.
    pub extract_attachments: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobKind {
    Convert,
    Repair,
    Export,
    Verify,
    ExtractAttachments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobStatus {
    Queued,
    Running,
    Completed,
    CompletedWithWarnings,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub phase: String,
    pub done: u64,
    pub total: u64,
    /// `false` when the total is unknown and `done` is a running count.
    pub determinate: bool,
    pub items_ok: u64,
    pub items_failed: u64,
    pub warnings: u64,
    pub bytes_written: u64,
    pub elapsed_ms: u64,
    pub eta_ms: Option<u64>,
}

/// Milliseconds since the Unix epoch, UTC, from a Windows FILETIME. `None` for zero.
pub fn filetime_to_ms(ft: u64) -> Option<i64> {
    if ft == 0 {
        return None;
    }
    Some(ft as i64 / 10_000 - 11_644_473_600_000)
}

/// A Windows FILETIME from Unix milliseconds.
pub fn ms_to_filetime(ms: i64) -> u64 {
    ((ms + 11_644_473_600_000) * 10_000).max(0) as u64
}

/// Now, as Unix milliseconds.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
