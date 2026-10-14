//! Safe output handling: temporary files, collision policy, disk space, names.
//!
//! A half-written PST looks exactly like a PST. Every file this engine writes therefore
//! goes to a `.partial` beside the target and is renamed into place only once the writer
//! has finished and flushed, so an interrupted job can never leave something behind that
//! Outlook, or this program, would mistake for a finished file.

use crate::model::Collision;
use std::path::{Path, PathBuf};

/// Where the finished file will go, after the collision policy has been applied.
///
/// `Fail` refuses an existing path; `Rename` finds `name (2).ext`, `name (3).ext` and so
/// on; `Overwrite` returns the path as given. The decision is made before any work
/// starts, so a long conversion never ends with "the file already existed".
pub fn resolve_output(path: &Path, policy: Collision) -> Result<PathBuf, String> {
    if !path.exists() {
        return Ok(path.to_path_buf());
    }
    match policy {
        Collision::Fail => Err(format!(
            "{} already exists. Choose another name, or allow renaming or overwriting in the options.",
            path.display()
        )),
        Collision::Overwrite => Ok(path.to_path_buf()),
        Collision::Rename => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("output");
            let ext = path.extension().and_then(|s| s.to_str());
            let dir = path.parent().unwrap_or(Path::new("."));
            for n in 2..10_000 {
                let name = match ext {
                    Some(e) => format!("{stem} ({n}).{e}"),
                    None => format!("{stem} ({n})"),
                };
                let candidate = dir.join(name);
                if !candidate.exists() {
                    return Ok(candidate);
                }
            }
            Err(format!("could not find a free name beside {}", path.display()))
        }
    }
}

/// The scratch path a writer uses until it is done.
pub fn partial_path(target: &Path) -> PathBuf {
    let mut name = target
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(".partial");
    target.with_file_name(name)
}

/// Move a finished `.partial` into place. `Overwrite` is the only policy under which the
/// target may already exist by now; the others resolved to a free name up front.
pub fn commit_partial(partial: &Path, target: &Path, policy: Collision) -> Result<(), String> {
    if target.exists() {
        if policy != Collision::Overwrite {
            let _ = std::fs::remove_file(partial);
            return Err(format!(
                "{} appeared while the job was running; the output was left as {}",
                target.display(),
                partial.display()
            ));
        }
        std::fs::remove_file(target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    std::fs::rename(partial, target).map_err(|e| {
        format!(
            "could not move {} into place as {}: {e}",
            partial.display(),
            target.display()
        )
    })
}

/// Remove a scratch file after a failed or cancelled job. Best effort.
pub fn discard_partial(partial: &Path) {
    let _ = std::fs::remove_file(partial);
}

/// Free space on the volume holding `path` (or its nearest existing ancestor), in bytes.
///
/// Windows only; `None` when it cannot be determined, which is a reason to proceed, not
/// to refuse.
pub fn free_space(path: &Path) -> Option<u64> {
    let mut probe = path.to_path_buf();
    while !probe.exists() {
        probe = probe.parent()?.to_path_buf();
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = probe
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut free: u64 = 0;
        let mut total: u64 = 0;
        let mut total_free: u64 = 0;
        // SAFETY: the buffer is NUL-terminated and the three out pointers are valid for
        // the duration of the call.
        let ok = unsafe {
            windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free,
                &mut total,
                &mut total_free,
            )
        };
        if ok != 0 {
            return Some(free);
        }
        None
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Refuse to start a job that cannot possibly fit. `needed` is an estimate; the margin
/// is there because a PST rebuilt from an OST can be larger than the compressed source.
pub fn check_space(target: &Path, needed: u64) -> Result<(), String> {
    if let Some(free) = free_space(target) {
        let margin = needed / 10 + 16 * 1024 * 1024;
        if free < needed + margin {
            return Err(format!(
                "not enough disk space for {}: about {} needed, {} free",
                target.display(),
                human_bytes(needed + margin),
                human_bytes(free)
            ));
        }
    }
    Ok(())
}

pub fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["bytes", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} bytes")
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

/// Turn a folder name, subject or attachment name into one safe path component.
///
/// This is a trust boundary: the text comes from the file, so it can contain separators,
/// `..`, control characters, or a reserved device name. None of those may reach the
/// filesystem. `unique` is appended when the result would otherwise be empty, reserved
/// or truncated, so two different inputs cannot collapse to the same name by accident.
pub fn safe_name(s: &str, unique: u32) -> String {
    let mut out: String = s
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if (c as u32) < 0x20 || c == '\u{7F}' => '_',
            c => c,
        })
        .collect();
    out = out.trim().trim_end_matches('.').trim().to_string();
    const RESERVED: [&str; 22] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let stem = out.split('.').next().unwrap_or("").to_ascii_uppercase();
    if out.is_empty() || RESERVED.contains(&stem.as_str()) || out == "." || out == ".." {
        return format!("{out}_{unique:X}");
    }
    if out.chars().count() > 80 {
        out = out.chars().take(80).collect::<String>();
        return format!("{}_{unique:X}", out.trim_end());
    }
    out
}

/// A unique path for a new file in `dir`: `base.ext`, then `base (2).ext`, and so on.
pub fn unique_in(dir: &Path, base: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{base}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..100_000 {
        let p = dir.join(format!("{base} ({n}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{base}-{}.{ext}", crate::model::now_ms()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_cannot_escape_the_directory() {
        for nasty in [
            "../../etc/passwd",
            "..\\..\\windows",
            "a/b",
            "a\\b",
            "C:evil",
            "..",
            ".",
            "/",
            "CON.txt",
            "nul",
            "",
            "   ",
            "trailing...",
        ] {
            let s = safe_name(nasty, 1);
            let mut parts = Path::new(&s).components();
            assert!(
                matches!(parts.next(), Some(std::path::Component::Normal(_))),
                "{nasty:?} -> {s:?} is not a plain name"
            );
            assert!(
                parts.next().is_none(),
                "{nasty:?} -> {s:?} has several parts"
            );
            assert!(!s.is_empty());
            assert!(!s.ends_with('.'));
        }
    }

    #[test]
    fn long_names_truncate_on_a_character_boundary() {
        let s = safe_name(&"é".repeat(300), 7);
        assert!(s.chars().count() <= 90);
        assert!(s.is_char_boundary(s.len()));
    }

    #[test]
    fn collision_policies() {
        let dir = std::env::temp_dir().join(format!("ostpst-fsutil-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let target = dir.join("out.pst");
        assert_eq!(resolve_output(&target, Collision::Fail).unwrap(), target);
        std::fs::write(&target, b"x").unwrap();
        assert!(resolve_output(&target, Collision::Fail).is_err());
        assert_eq!(
            resolve_output(&target, Collision::Overwrite).unwrap(),
            target
        );
        assert_eq!(
            resolve_output(&target, Collision::Rename).unwrap(),
            dir.join("out (2).pst")
        );
        let partial = partial_path(&target);
        assert!(partial.to_string_lossy().ends_with("out.pst.partial"));
        std::fs::write(&partial, b"new").unwrap();
        commit_partial(&partial, &target, Collision::Overwrite).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn human_sizes() {
        assert_eq!(human_bytes(512), "512 bytes");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(16_818_176), "16.0 MB");
    }
}
