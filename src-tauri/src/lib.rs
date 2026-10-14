//! The desktop host: Tauri commands over the engine, a job manager, and settings.
//!
//! Every command is thin. Mailbox operations run on a blocking thread against a mutex
//! around the engine's `Mailbox`; long jobs open their own handle on the file so the
//! window stays responsive while they run, and report through events. Nothing here
//! touches the network, and nothing reads a file the user did not name.

mod jobs;
mod settings;

use jobs::{JobManager, JobSpec};
use ostpst_core::mailbox::{Mailbox, OpenOptions};
use ostpst_core::model::*;
use serde::{Deserialize, Serialize};
use settings::{History, Settings};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

/// Everything the window shares across commands.
pub struct AppState {
    mailboxes: Mutex<HashMap<String, Arc<Mutex<Mailbox>>>>,
    jobs: JobManager,
    settings: Mutex<Settings>,
    history: Mutex<History>,
    search_cancel: Mutex<HashMap<String, Arc<std::sync::atomic::AtomicBool>>>,
    data_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenedMailbox {
    pub id: String,
    pub info: FileInfo,
    pub folders: Vec<FolderInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub path: String,
    pub file_name: String,
    pub exists: bool,
    pub size_bytes: u64,
    pub is_pst_or_ost: bool,
    pub is_ost: bool,
    pub format: Option<FileFormat>,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentPreview {
    pub info: AttachmentInfo,
    pub mime: String,
    /// Base64 of the bytes, for images and small text. Empty when too big to preview.
    pub base64: String,
    pub text: Option<String>,
    pub too_large: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub engine_version: String,
    pub data_dir: String,
    pub license: String,
    pub third_party: Vec<(String, String, String)>,
}

fn mailbox(state: &AppState, id: &str) -> Result<Arc<Mutex<Mailbox>>, String> {
    state
        .mailboxes
        .lock()
        .map_err(|_| "mailbox registry is poisoned".to_string())?
        .get(id)
        .cloned()
        .ok_or_else(|| "that mailbox is no longer open".to_string())
}

pub(crate) fn new_id_pub(prefix: &str) -> String {
    new_id(prefix)
}

fn new_id(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(1);
    format!(
        "{prefix}-{}-{}",
        now_ms(),
        N.fetch_add(1, Ordering::Relaxed)
    )
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

#[tauri::command]
async fn probe_file(path: String) -> Result<ProbeResult, String> {
    blocking(move || {
        let p = Path::new(&path);
        let file_name = p
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        if !p.exists() {
            return Ok(ProbeResult {
                path,
                file_name,
                exists: false,
                size_bytes: 0,
                is_pst_or_ost: false,
                is_ost: false,
                format: None,
                warnings: vec![],
                error: Some("the file does not exist".into()),
            });
        }
        let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
        match ostpst_core::ndb::Pst::open(&path) {
            Ok(pst) => Ok(ProbeResult {
                path: path.clone(),
                file_name,
                exists: true,
                size_bytes: size,
                is_pst_or_ost: true,
                is_ost: pst.is_ost,
                format: Some(if pst.is_ansi() {
                    FileFormat::Ansi
                } else if pst.is_small_page() {
                    FileFormat::Unicode
                } else {
                    FileFormat::Unicode4k
                }),
                warnings: pst.warnings.clone(),
                error: None,
            }),
            Err(e) => Ok(ProbeResult {
                path,
                file_name,
                exists: true,
                size_bytes: size,
                is_pst_or_ost: false,
                is_ost: false,
                format: None,
                warnings: vec![],
                error: Some(e),
            }),
        }
    })
    .await
}

#[tauri::command]
async fn open_mailbox(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    salvage: bool,
) -> Result<OpenedMailbox, String> {
    let id = new_id("mb");
    let id2 = id.clone();
    let app2 = app.clone();
    let path2 = path.clone();
    let opened = blocking(move || {
        let mut on = |done: u64, _: u64| {
            let _ = app2.emit(
                "mailbox://opening",
                serde_json::json!({ "id": id2, "path": path2, "step": done }),
            );
            true
        };
        let mb = Mailbox::open(Path::new(&path2), OpenOptions { salvage }, &mut on)?;
        let info = mb.info().clone();
        let folders = mb.folders();
        Ok((mb, info, folders))
    })
    .await?;
    let (mb, info, folders) = opened;
    state
        .mailboxes
        .lock()
        .map_err(|_| "mailbox registry is poisoned".to_string())?
        .insert(id.clone(), Arc::new(Mutex::new(mb)));
    {
        let mut h = state
            .history
            .lock()
            .map_err(|_| "history is poisoned".to_string())?;
        h.touch_file(&path, &info);
        h.save(&state.data_dir);
    }
    Ok(OpenedMailbox { id, info, folders })
}

#[tauri::command]
fn close_mailbox(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state
        .mailboxes
        .lock()
        .map_err(|_| "mailbox registry is poisoned".to_string())?
        .remove(&id);
    Ok(())
}

#[tauri::command]
fn list_open_mailboxes(state: State<'_, AppState>) -> Result<Vec<OpenedMailbox>, String> {
    let reg = state
        .mailboxes
        .lock()
        .map_err(|_| "mailbox registry is poisoned".to_string())?;
    let mut out = Vec::new();
    for (id, mb) in reg.iter() {
        if let Ok(m) = mb.lock() {
            out.push(OpenedMailbox {
                id: id.clone(),
                info: m.info().clone(),
                folders: m.folders(),
            });
        }
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

#[tauri::command]
async fn get_folders(state: State<'_, AppState>, id: String) -> Result<Vec<FolderInfo>, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || Ok(mb.lock().map_err(|_| "mailbox is poisoned")?.folders())).await
}

#[tauri::command]
async fn list_messages(
    state: State<'_, AppState>,
    id: String,
    folder: u32,
    offset: usize,
    limit: usize,
    sort: SortKey,
    descending: bool,
) -> Result<MessagePage, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        Ok(mb.lock().map_err(|_| "mailbox is poisoned")?.list_messages(
            folder,
            offset,
            limit.clamp(1, 2000),
            sort,
            descending,
        ))
    })
    .await
}

#[tauri::command]
async fn get_message(
    state: State<'_, AppState>,
    id: String,
    nid: u32,
) -> Result<MessageDetail, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        mb.lock()
            .map_err(|_| "mailbox is poisoned".to_string())?
            .message(nid)
    })
    .await
}

#[tauri::command]
async fn get_embedded_message(
    state: State<'_, AppState>,
    id: String,
    nid: u32,
    attachment: u32,
) -> Result<MessageDetail, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        mb.lock()
            .map_err(|_| "mailbox is poisoned".to_string())?
            .embedded_message(nid, attachment)
    })
    .await
}

#[tauri::command]
async fn node_properties(
    state: State<'_, AppState>,
    id: String,
    nid: u32,
) -> Result<Vec<PropertyView>, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        mb.lock()
            .map_err(|_| "mailbox is poisoned".to_string())?
            .node_properties(nid)
    })
    .await
}

const PREVIEW_LIMIT: usize = 8 * 1024 * 1024;

#[tauri::command]
async fn attachment_preview(
    state: State<'_, AppState>,
    id: String,
    nid: u32,
    attachment: u32,
) -> Result<AttachmentPreview, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        let (info, bytes) = mb
            .lock()
            .map_err(|_| "mailbox is poisoned".to_string())?
            .attachment_data(nid, attachment)?;
        let mime = info.mime.clone();
        let too_large = bytes.len() > PREVIEW_LIMIT;
        let is_image = mime.starts_with("image/") && !mime.contains("svg");
        let is_text = mime.starts_with("text/")
            || matches!(
                info.extension.as_str(),
                "txt" | "log" | "csv" | "json" | "xml" | "md" | "ini" | "eml"
            );
        let base64 = if is_image && !too_large {
            b64(&bytes)
        } else {
            String::new()
        };
        let text = if is_text && !too_large {
            Some(ostpst_core::html::decode(
                &bytes[..bytes.len().min(512 * 1024)],
                None,
            ))
        } else {
            None
        };
        Ok(AttachmentPreview {
            info,
            mime,
            base64,
            text,
            too_large,
        })
    })
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedAttachment {
    pub info: AttachmentInfo,
    pub path: String,
}

#[tauri::command]
async fn save_attachment(
    state: State<'_, AppState>,
    id: String,
    nid: u32,
    attachment: u32,
    destination: String,
) -> Result<SavedAttachment, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        let (info, bytes) = mb
            .lock()
            .map_err(|_| "mailbox is poisoned".to_string())?
            .attachment_data(nid, attachment)?;
        let p = PathBuf::from(&destination);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&p, &bytes).map_err(|e| format!("{}: {e}", p.display()))?;
        Ok(SavedAttachment {
            info,
            path: destination,
        })
    })
    .await
}

/// Every by-value attachment of the given messages into `directory`, names
/// sanitised and made unique. Returns what was written and what failed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractResult {
    pub written: Vec<SavedAttachment>,
    pub failed: Vec<String>,
    pub skipped_embedded: usize,
}

#[tauri::command]
async fn extract_attachments(
    state: State<'_, AppState>,
    id: String,
    messages: Vec<u32>,
    directory: String,
) -> Result<ExtractResult, String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        let dir = PathBuf::from(&directory);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut r = ExtractResult {
            written: vec![],
            failed: vec![],
            skipped_embedded: 0,
        };
        let mut m = mb.lock().map_err(|_| "mailbox is poisoned".to_string())?;
        for nid in messages {
            let detail = match m.message(nid) {
                Ok(d) => d,
                Err(e) => {
                    r.failed.push(format!("message 0x{nid:X}: {e}"));
                    continue;
                }
            };
            for a in detail.attachments {
                if a.is_embedded_message {
                    r.skipped_embedded += 1;
                    continue;
                }
                match m.attachment_data(nid, a.nid) {
                    Ok((info, bytes)) => {
                        let (stem, ext) = match info.name.rsplit_once('.') {
                            Some((s, e)) if !s.is_empty() && e.len() <= 10 => {
                                (s.to_string(), e.to_string())
                            }
                            _ => (info.name.clone(), "bin".into()),
                        };
                        let p = ostpst_core::fsutil::unique_in(&dir, &stem, &ext);
                        match std::fs::write(&p, &bytes) {
                            Ok(()) => r.written.push(SavedAttachment {
                                info,
                                path: p.to_string_lossy().to_string(),
                            }),
                            Err(e) => r.failed.push(format!("{}: {e}", p.display())),
                        }
                    }
                    Err(e) => r.failed.push(format!("message 0x{nid:X}, {}: {e}", a.name)),
                }
            }
        }
        Ok(r)
    })
    .await
}

#[tauri::command]
async fn search(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    query: SearchQuery,
) -> Result<SearchResult, String> {
    let mb = mailbox(&state, &id)?;
    let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
    {
        let mut reg = state
            .search_cancel
            .lock()
            .map_err(|_| "search registry is poisoned")?;
        if let Some(prev) = reg.insert(id.clone(), cancel.clone()) {
            prev.store(true, std::sync::atomic::Ordering::Relaxed);
        }
    }
    let id2 = id.clone();
    let result = blocking(move || {
        let mut last = std::time::Instant::now();
        let mut on = |done: u64, total: u64| {
            if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                return false;
            }
            if last.elapsed().as_millis() > 150 {
                last = std::time::Instant::now();
                let _ = app.emit(
                    "search://progress",
                    serde_json::json!({ "id": id2, "done": done, "total": total }),
                );
            }
            true
        };
        Ok(mb
            .lock()
            .map_err(|_| "mailbox is poisoned")?
            .search(&query, &mut on))
    })
    .await;
    if let Ok(mut reg) = state.search_cancel.lock() {
        reg.remove(&id);
    }
    result
}

#[tauri::command]
fn cancel_search(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if let Some(c) = state
        .search_cancel
        .lock()
        .map_err(|_| "search registry is poisoned")?
        .get(&id)
    {
        c.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

#[tauri::command]
fn start_job(app: AppHandle, state: State<'_, AppState>, spec: JobSpec) -> Result<String, String> {
    let max = state
        .settings
        .lock()
        .map(|s| s.max_concurrent_jobs)
        .unwrap_or(2);
    let id = state
        .jobs
        .start(app, spec, max.max(1) as usize, state.data_dir.clone());
    Ok(id)
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.jobs.cancel(&id);
    Ok(())
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> Result<Vec<jobs::JobRecord>, String> {
    Ok(state.jobs.list())
}

#[tauri::command]
fn get_job(state: State<'_, AppState>, id: String) -> Result<Option<jobs::JobRecord>, String> {
    Ok(state.jobs.get(&id))
}

#[tauri::command]
fn clear_finished_jobs(state: State<'_, AppState>) -> Result<(), String> {
    state.jobs.clear_finished();
    Ok(())
}

#[tauri::command]
fn retry_job(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<String, String> {
    let spec = state
        .jobs
        .spec_of(&id)
        .ok_or("that job is no longer known")?;
    let max = state
        .settings
        .lock()
        .map(|s| s.max_concurrent_jobs)
        .unwrap_or(2);
    Ok(state
        .jobs
        .start(app, spec, max.max(1) as usize, state.data_dir.clone()))
}

#[tauri::command]
fn save_report(
    state: State<'_, AppState>,
    id: String,
    path: String,
    format: String,
) -> Result<(), String> {
    let rec = state.jobs.get(&id).ok_or("that job is no longer known")?;
    let report = rec.report.ok_or("the job has no report yet")?;
    let body = match format.as_str() {
        "json" => report.to_json(),
        "html" => report.to_html(),
        _ => report.to_text(),
    };
    std::fs::write(&path, body).map_err(|e| format!("{path}: {e}"))
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    state
        .settings
        .lock()
        .map(|s| s.clone())
        .map_err(|_| "settings are poisoned".into())
}

#[tauri::command]
fn set_settings(state: State<'_, AppState>, settings: Settings) -> Result<Settings, String> {
    let mut s = state.settings.lock().map_err(|_| "settings are poisoned")?;
    *s = settings.normalised();
    s.save(&state.data_dir);
    Ok(s.clone())
}

#[tauri::command]
fn get_history(state: State<'_, AppState>) -> Result<History, String> {
    state
        .history
        .lock()
        .map(|h| h.clone())
        .map_err(|_| "history is poisoned".into())
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    let mut h = state.history.lock().map_err(|_| "history is poisoned")?;
    *h = History::default();
    h.save(&state.data_dir);
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathInfo {
    pub exists: bool,
    pub is_dir: bool,
    pub is_file: bool,
    pub size_bytes: u64,
    pub free_space: Option<u64>,
    pub is_empty_dir: Option<bool>,
}

#[tauri::command]
fn path_info(path: String) -> Result<PathInfo, String> {
    let p = Path::new(&path);
    let meta = std::fs::metadata(p).ok();
    let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
    Ok(PathInfo {
        exists: meta.is_some(),
        is_dir,
        is_file: meta.as_ref().is_some_and(|m| m.is_file()),
        size_bytes: meta.as_ref().map(|m| m.len()).unwrap_or(0),
        free_space: ostpst_core::fsutil::free_space(p),
        is_empty_dir: if is_dir {
            Some(
                std::fs::read_dir(p)
                    .map(|mut d| d.next().is_none())
                    .unwrap_or(false),
            )
        } else {
            None
        },
    })
}

/// A suggested output path beside the source (or in the default output directory).
#[tauri::command]
fn suggest_output(
    state: State<'_, AppState>,
    source: String,
    extension: String,
) -> Result<String, String> {
    let src = Path::new(&source);
    let stem = src
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "output".into());
    let dir = state
        .settings
        .lock()
        .ok()
        .and_then(|s| s.default_output_dir.clone())
        .filter(|d| !d.trim().is_empty() && Path::new(d).is_dir())
        .map(PathBuf::from)
        .or_else(|| src.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let name = if extension.is_empty() {
        format!("{stem}-export")
    } else {
        format!("{stem}-converted")
    };
    let p = if extension.is_empty() {
        let mut p = dir.join(&name);
        let mut n = 2;
        while p.exists() {
            p = dir.join(format!("{name} ({n})"));
            n += 1;
        }
        p
    } else {
        ostpst_core::fsutil::unique_in(&dir, &name, &extension)
    };
    // Dialogs hand back backslashes; a path typed with forward slashes should not
    // produce a mixed one.
    Ok(p.to_string_lossy().replace('/', "\\"))
}

#[tauri::command]
fn app_info(state: State<'_, AppState>) -> Result<AppInfo, String> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        engine_version: ostpst_core::VERSION.into(),
        data_dir: state.data_dir.to_string_lossy().to_string(),
        license: include_str!("../../LICENSE").into(),
        third_party: vec![
            (
                "pstfree".into(),
                "MIT".into(),
                "https://github.com/sp00nznet/pstfree".into(),
            ),
            (
                "miniz_oxide".into(),
                "MIT".into(),
                "https://github.com/Frommi/miniz_oxide".into(),
            ),
            (
                "windows-sys".into(),
                "MIT".into(),
                "https://github.com/microsoft/windows-rs".into(),
            ),
            ("Tauri".into(), "MIT".into(), "https://tauri.app".into()),
            ("React".into(), "MIT".into(), "https://react.dev".into()),
            (
                "Tailwind CSS".into(),
                "MIT".into(),
                "https://tailwindcss.com".into(),
            ),
            (
                "Radix UI".into(),
                "MIT".into(),
                "https://www.radix-ui.com".into(),
            ),
            ("Lucide".into(), "ISC".into(), "https://lucide.dev".into()),
            (
                "DOMPurify".into(),
                "Apache-2.0 / MPL-2.0".into(),
                "https://github.com/cure53/DOMPurify".into(),
            ),
            (
                "TanStack Virtual".into(),
                "MIT".into(),
                "https://tanstack.com/virtual".into(),
            ),
            (
                "Zustand".into(),
                "MIT".into(),
                "https://github.com/pmndrs/zustand".into(),
            ),
            (
                "libpff (optional, validation only)".into(),
                "LGPL-3.0".into(),
                "https://github.com/libyal/libpff".into(),
            ),
        ],
    })
}

/// A JSON description of the whole mailbox for technical analysis.
#[tauri::command]
async fn export_structure_json(
    state: State<'_, AppState>,
    id: String,
    path: String,
) -> Result<(), String> {
    let mb = mailbox(&state, &id)?;
    blocking(move || {
        let json = {
            let mut m = mb.lock().map_err(|_| "mailbox is poisoned".to_string())?;
            ostpst_core::exporters::mailbox_json(&mut m, &mut ostpst_core::silent())?
        };
        std::fs::write(&path, json).map_err(|e| format!("{path}: {e}"))
    })
    .await
}

/// Write text the window already holds (a report from the history) to a path the user
/// chose in a save dialog.
#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("{path}: {e}"))
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len() * 4 / 3 + 4);
    for c in data.chunks(3) {
        let n = ((c[0] as u32) << 16)
            | ((*c.get(1).unwrap_or(&0) as u32) << 8)
            | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 {
            T[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if c.len() > 2 {
            T[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_config_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("ostpst-studio"));
            let _ = std::fs::create_dir_all(&data_dir);
            let settings = Settings::load(&data_dir);
            let history = History::load(&data_dir);
            app.manage(AppState {
                mailboxes: Mutex::new(HashMap::new()),
                jobs: JobManager::new(),
                settings: Mutex::new(settings),
                history: Mutex::new(history),
                search_cancel: Mutex::new(HashMap::new()),
                data_dir,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            probe_file,
            open_mailbox,
            close_mailbox,
            list_open_mailboxes,
            get_folders,
            list_messages,
            get_message,
            get_embedded_message,
            node_properties,
            attachment_preview,
            save_attachment,
            extract_attachments,
            search,
            cancel_search,
            start_job,
            cancel_job,
            list_jobs,
            get_job,
            clear_finished_jobs,
            retry_job,
            save_report,
            get_settings,
            set_settings,
            get_history,
            clear_history,
            path_info,
            suggest_output,
            app_info,
            export_structure_json,
            write_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running OST/PST Studio");
}
