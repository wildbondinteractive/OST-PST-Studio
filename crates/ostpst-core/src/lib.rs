//! OST/PST Studio engine: reading, searching, exporting, converting and repairing
//! Outlook PST and OST files.
//!
//! The low layers are carried over from pstfree 0.5.1 (MIT) and keep its shape: [`ndb`]
//! is blocks and B-trees, [`ltp`] is heaps, properties and tables on top of them,
//! [`convert`] and [`repair`] write files back out, [`export`] turns messages into mail
//! formats, and [`crypt`], [`cfbf`] and [`html`] are the self-contained formats needed
//! along the way.
//!
//! Above those sits what the desktop application and the command line share:
//! [`mailbox`] is the high-level, cancellation-aware API (open with index fallback,
//! folder tree, paged listings, message detail, attachments, search, jobs), [`model`]
//! is the serialisable shape of everything it hands back, [`report`] is the record a
//! job leaves behind, [`validate`] checks a written file against its source, and
//! [`fsutil`] is safe output handling (temporary files, collisions, disk space).

/// What a long job calls as it goes: how much of it is done, out of how much.
///
/// One shape for every long job. Called once per unit; throttling is the caller's
/// business. Returns whether to continue: `false` asks the job to stop at the next safe
/// point, and the job then fails with [`CANCELLED`] (or, for an export, reports
/// `cancelled` in its statistics).
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64) -> bool;

/// The error a cancelled job reports. Compared by string because every engine error is
/// a `String`, which keeps the low layers free of an error type they would otherwise
/// have to carry through hundreds of call sites.
pub const CANCELLED: &str = "The operation was cancelled.";

/// True when an engine error is the cancellation sentinel rather than a real failure.
pub fn is_cancelled(e: &str) -> bool {
    e == CANCELLED
}

/// A progress callback that never cancels, for callers with nothing to report to.
pub fn silent() -> impl FnMut(u64, u64) -> bool {
    |_, _| true
}

pub mod amap;
pub mod cfbf;
pub mod convert;
pub mod crypt;
pub mod export;
pub mod exporters;
pub mod fsutil;
pub mod html;
pub mod jobs;
pub mod ltp;
pub mod mailbox;
pub mod model;
pub mod ndb;
pub mod props;
pub mod repair;
pub mod report;
pub mod validate;

/// Version of the engine, from the crate manifest.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
