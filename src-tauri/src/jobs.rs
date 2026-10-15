//! Background jobs: a queue with bounded concurrency, cancellation, progress events,
//! and a record of everything that ran this session (persisted to the history).

use ostpst_core::jobs::{run_convert, run_export, run_verify};
use ostpst_core::model::*;
use ostpst_core::report::JobReport;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum JobSpec {
    Convert { options: ConvertOptions },
    Repair { options: ConvertOptions },
    Export { options: ExportOptions },
    Verify { source: String },
}

impl JobSpec {
    fn kind(&self) -> JobKind {
        match self {
            JobSpec::Convert { .. } => JobKind::Convert,
            JobSpec::Repair { .. } => JobKind::Repair,
            JobSpec::Export { .. } => JobKind::Export,
            JobSpec::Verify { .. } => JobKind::Verify,
        }
    }
    fn source(&self) -> String {
        match self {
            JobSpec::Convert { options } | JobSpec::Repair { options } => options.source.clone(),
            JobSpec::Export { options } => options.source.clone(),
            JobSpec::Verify { source } => source.clone(),
        }
    }
    fn destination(&self) -> String {
        match self {
            JobSpec::Convert { options } | JobSpec::Repair { options } => {
                options.destination.clone()
            }
            JobSpec::Export { options } => options.destination.clone(),
            JobSpec::Verify { .. } => String::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRecord {
    pub id: String,
    pub kind: JobKind,
    pub status: JobStatus,
    pub source: String,
    pub destination: String,
    pub created_ms: i64,
    pub started_ms: Option<i64>,
    pub finished_ms: Option<i64>,
    pub progress: JobProgress,
    pub error: Option<String>,
    pub report: Option<JobReport>,
}

struct Entry {
    record: JobRecord,
    spec: JobSpec,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    jobs: HashMap<String, Entry>,
    order: Vec<String>,
    queue: VecDeque<String>,
    running: usize,
}

pub struct JobManager {
    inner: Arc<Mutex<Inner>>,
}

impl JobManager {
    pub fn new() -> JobManager {
        JobManager {
            inner: Arc::new(Mutex::new(Inner::default())),
        }
    }

    pub fn start<R: Runtime>(
        &self,
        app: AppHandle<R>,
        spec: JobSpec,
        max_concurrent: usize,
        data_dir: PathBuf,
    ) -> String {
        let id = super::new_id_pub("job");
        let record = JobRecord {
            id: id.clone(),
            kind: spec.kind(),
            status: JobStatus::Queued,
            source: spec.source(),
            destination: spec.destination(),
            created_ms: now_ms(),
            started_ms: None,
            finished_ms: None,
            progress: JobProgress::default(),
            error: None,
            report: None,
        };
        {
            let mut inner = self.inner.lock().unwrap();
            inner.jobs.insert(
                id.clone(),
                Entry {
                    record: record.clone(),
                    spec,
                    cancel: Arc::new(AtomicBool::new(false)),
                },
            );
            inner.order.push(id.clone());
            inner.queue.push_back(id.clone());
        }
        let _ = app.emit("job://update", &record);
        self.pump(app, max_concurrent, data_dir);
        id
    }

    /// Start queued jobs while there is room.
    fn pump<R: Runtime>(&self, app: AppHandle<R>, max_concurrent: usize, data_dir: PathBuf) {
        loop {
            let next = {
                let mut inner = self.inner.lock().unwrap();
                if inner.running >= max_concurrent {
                    return;
                }
                let Some(id) = inner.queue.pop_front() else {
                    return;
                };
                let cancelled = match inner.jobs.get(&id) {
                    Some(e) => e.cancel.load(Ordering::Relaxed),
                    None => continue,
                };
                if !cancelled {
                    inner.running += 1;
                }
                let Some(e) = inner.jobs.get_mut(&id) else {
                    continue;
                };
                if cancelled {
                    e.record.status = JobStatus::Cancelled;
                    e.record.finished_ms = Some(now_ms());
                    let _ = app.emit("job://update", &e.record);
                    continue;
                }
                e.record.status = JobStatus::Running;
                e.record.started_ms = Some(now_ms());
                let _ = app.emit("job://update", &e.record);
                (id, e.spec.clone(), e.cancel.clone())
            };
            let (id, spec, cancel) = next;
            let inner = self.inner.clone();
            let app2 = app.clone();
            let data_dir2 = data_dir.clone();
            let mgr = JobManager {
                inner: inner.clone(),
            };
            std::thread::Builder::new()
                .name(format!("job-{id}"))
                .spawn(move || {
                    let id_for_cb = id.clone();
                    let inner_cb = inner.clone();
                    let app_cb = app2.clone();
                    let mut last_emit =
                        std::time::Instant::now() - std::time::Duration::from_secs(1);
                    let mut on = move |p: &JobProgress| -> bool {
                        if cancel.load(Ordering::Relaxed) {
                            return false;
                        }
                        let force = p.done >= p.total || last_emit.elapsed().as_millis() > 120;
                        if force {
                            last_emit = std::time::Instant::now();
                            if let Ok(mut inner) = inner_cb.lock() {
                                if let Some(e) = inner.jobs.get_mut(&id_for_cb) {
                                    e.record.progress = p.clone();
                                    let _ = app_cb.emit("job://progress", &e.record);
                                }
                            }
                        }
                        true
                    };
                    let report = match &spec {
                        JobSpec::Convert { options } => {
                            run_convert(&id, JobKind::Convert, options, &mut on)
                        }
                        JobSpec::Repair { options } => {
                            run_convert(&id, JobKind::Repair, options, &mut on)
                        }
                        JobSpec::Export { options } => run_export(&id, options, &mut on),
                        JobSpec::Verify { source } => run_verify(&id, source, &mut on),
                    };
                    {
                        let mut inner = inner.lock().unwrap();
                        inner.running = inner.running.saturating_sub(1);
                        if let Some(e) = inner.jobs.get_mut(&id) {
                            e.record.status = report.status;
                            e.record.finished_ms = Some(now_ms());
                            e.record.error = report.errors.first().cloned();
                            e.record.destination = report.destination.path.clone();
                            e.record.report = Some(report);
                            let _ = app2.emit("job://update", &e.record);
                            let _ = app2.emit("job://done", &e.record);
                            // Persist a line of history for the dashboard.
                            let rec = e.record.clone();
                            drop(inner);
                            let mut h = super::settings::History::load(&data_dir2);
                            h.push_job(&rec);
                            h.save(&data_dir2);
                        }
                    }
                    mgr.pump(app2, max_concurrent, data_dir2);
                })
                .expect("spawn job thread");
        }
    }

    pub fn cancel(&self, id: &str) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(e) = inner.jobs.get_mut(id) {
            e.cancel.store(true, Ordering::Relaxed);
            if e.record.status == JobStatus::Queued {
                e.record.status = JobStatus::Cancelled;
                e.record.finished_ms = Some(now_ms());
            }
        }
        inner.queue.retain(|q| q != id);
    }

    pub fn list(&self) -> Vec<JobRecord> {
        let inner = self.inner.lock().unwrap();
        inner
            .order
            .iter()
            .filter_map(|id| inner.jobs.get(id).map(|e| e.record.clone()))
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<JobRecord> {
        self.inner
            .lock()
            .unwrap()
            .jobs
            .get(id)
            .map(|e| e.record.clone())
    }

    pub fn spec_of(&self, id: &str) -> Option<JobSpec> {
        self.inner
            .lock()
            .unwrap()
            .jobs
            .get(id)
            .map(|e| e.spec.clone())
    }

    /// Jobs queued or running: what an update install or closing the window would interrupt.
    pub fn active_count(&self) -> usize {
        self.inner
            .lock()
            .map(|inner| {
                inner
                    .jobs
                    .values()
                    .filter(|e| matches!(e.record.status, JobStatus::Queued | JobStatus::Running))
                    .count()
            })
            .unwrap_or(0)
    }

    /// Ask every queued and running job to stop at its next safe point. Running jobs
    /// discard their `.partial` output when they stop; originals are never touched.
    pub fn cancel_all(&self) {
        let ids: Vec<String> = self
            .inner
            .lock()
            .map(|inner| inner.jobs.keys().cloned().collect())
            .unwrap_or_default();
        for id in ids {
            self.cancel(&id);
        }
    }

    pub fn clear_finished(&self) {
        let mut inner = self.inner.lock().unwrap();
        let done: Vec<String> = inner
            .jobs
            .iter()
            .filter(|(_, e)| !matches!(e.record.status, JobStatus::Queued | JobStatus::Running))
            .map(|(id, _)| id.clone())
            .collect();
        for id in &done {
            inner.jobs.remove(id);
        }
        inner.order.retain(|id| !done.contains(id));
    }
}
