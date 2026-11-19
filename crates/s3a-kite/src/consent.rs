//! Consent file (§5.2.3, DD-50/DD-96) and in-memory consent state.
//!
//! File format: `on` or `off`, a newline, and an RFC3339-seconds-UTC
//! timestamp of the last change, mode `0600`, owner euid. GET and POST
//! return the host's **in-memory** state, read under the write mutex, not
//! a raw reread of the file. A same-state POST is a no-op that keeps the
//! timestamp (the DD-144 latch actions on a same-state `off` are Step 11).

use std::os::unix::io::RawFd;

use crate::paths::{close_fd, fstat_fd, openat_child_rw, unlink_at};
use crate::timefmt::{parse_rfc3339_secs, rfc3339_secs};

/// In-memory consent state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsentState {
    pub on: bool,
    /// RFC3339 seconds UTC of the last change, or `None` when no consent
    /// file is configured (no-LRS hosts always report off/null).
    pub timestamp: Option<String>,
}

impl ConsentState {
    pub fn off() -> Self {
        ConsentState {
            on: false,
            timestamp: None,
        }
    }
}

/// Parse consent file bytes: `on|off`, newline, RFC3339-seconds-UTC
/// timestamp, optional trailing newline. A present but malformed file is a
/// startup error that names the file (AT-151).
pub fn parse_consent_file(bytes: &[u8], file_display: &str) -> Result<ConsentState, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| format!("{file_display}: consent file is not UTF-8"))?;
    let mut lines = text.lines();
    let state = lines.next().ok_or_else(|| {
        format!("{file_display}: consent file is empty")
    })?;
    let on = match state {
        "on" => true,
        "off" => false,
        _ => {
            return Err(format!(
                "{file_display}: consent file state must be on or off, got {state:?}"
            ))
        }
    };
    let ts = lines.next().ok_or_else(|| {
        format!("{file_display}: consent file has no timestamp line")
    })?;
    if parse_rfc3339_secs(ts).is_none() {
        return Err(format!(
            "{file_display}: consent file timestamp is not RFC3339 seconds UTC: {ts:?}"
        ));
    }
    if lines.next().is_some() {
        return Err(format!("{file_display}: consent file has extra lines"));
    }
    Ok(ConsentState {
        on,
        timestamp: Some(ts.to_string()),
    })
}

/// Serialize a consent state to file bytes (`on|off`, newline, timestamp,
/// newline). `timestamp` must be `Some` when a file is written.
pub fn render_consent_file(state: ConsentState) -> Vec<u8> {
    let ts = state.timestamp.unwrap_or_else(|| rfc3339_secs(std::time::SystemTime::now()));
    format!("{}\n{}\n", if state.on { "on" } else { "off" }, ts).into_bytes()
}

/// Write a consent file atomically from the parent dirfd: temp file in the
/// parent directory named `s3a-kite-tmp-{pid}-*`, fsync of the temp, atomic
/// rename, fsync of the parent directory, mode `0600` (the DD-144
/// persistence discipline; the latch itself is Step 11).
pub fn write_consent_file(
    parent_fd: RawFd,
    basename: &str,
    state: ConsentState,
) -> Result<(), String> {
    let pid = std::process::id();
    let tmp_name = format!("s3a-kite-tmp-{pid}-consent-{}", random_suffix());
    let tmp_fd = openat_child_rw(parent_fd, &tmp_name, true)
        .map_err(|e| format!("consent file temp create failed: {e}"))?;
    let bytes = render_consent_file(state);
    let write_result = write_all_and_fsync(tmp_fd, &bytes);
    if let Err(e) = write_result {
        close_fd(tmp_fd);
        let _ = unlink_at(parent_fd, &tmp_name);
        return Err(format!("consent file temp write failed: {e}"));
    }
    // Atomic rename over the old file.
    let rc = unsafe {
        libc::renameat(
            parent_fd,
            std::ffi::CString::new(tmp_name.clone()).unwrap().as_ptr(),
            parent_fd,
            std::ffi::CString::new(basename).unwrap().as_ptr(),
        )
    };
    if rc != 0 {
        let e = std::io::Error::last_os_error();
        close_fd(tmp_fd);
        let _ = unlink_at(parent_fd, &tmp_name);
        return Err(format!("consent file rename failed: {e}"));
    }
    close_fd(tmp_fd);
    // fsync the parent directory so the rename is durable.
    let rc = unsafe { libc::fsync(parent_fd) };
    if rc != 0 {
        return Err(format!(
            "consent file parent fsync failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

fn write_all_and_fsync(fd: RawFd, bytes: &[u8]) -> std::io::Result<()> {
    let mut written = 0;
    while written < bytes.len() {
        let n = unsafe {
            libc::write(
                fd,
                bytes[written..].as_ptr() as *const libc::c_void,
                bytes.len() - written,
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        written += n as usize;
    }
    let rc = unsafe { libc::fsync(fd) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// 8 random hex chars from /dev/urandom (fallback: pid + nanos).
pub fn random_suffix() -> String {
    let mut buf = [0u8; 4];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        use std::io::Read;
        if f.read_exact(&mut buf).is_ok() {
            return buf.iter().map(|b| format!("{b:02x}")).collect();
        }
    }
    format!(
        "{:x}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    )
}

/// Read a consent file's bytes through its parent dirfd (safe-open:
/// O_NOFOLLOW|O_NONBLOCK, regular file required).
pub fn read_consent_file_bytes(parent_fd: RawFd, basename: &str) -> Result<Vec<u8>, String> {
    let fd = crate::paths::openat_child(parent_fd, basename, false)
        .map_err(|e| format!("cannot open consent file: {e}"))?;
    let st = fstat_fd(fd).map_err(|e| format!("cannot fstat consent file: {e}"))?;
    if !st.is_regular {
        close_fd(fd);
        return Err("consent file is not a regular file".to_string());
    }
    let bytes = crate::paths::read_fd_bounded(fd, u64::MAX)
        .map_err(|e| format!("cannot read consent file: {e}"))?;
    close_fd(fd);
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        let state = ConsentState {
            on: false,
            timestamp: Some("2026-11-18T21:00:00Z".to_string()),
        };
        let bytes = render_consent_file(state.clone());
        assert_eq!(parse_consent_file(&bytes, "c").unwrap(), state);
        let on = ConsentState {
            on: true,
            timestamp: Some("2026-11-18T21:00:00Z".to_string()),
        };
        assert_eq!(parse_consent_file(&render_consent_file(on.clone()), "c").unwrap(), on);
        assert!(parse_consent_file(b"maybe\n2026-11-18T21:00:00Z\n", "c").is_err());
        assert!(parse_consent_file(b"off\nnot-a-time\n", "c").is_err());
        assert!(parse_consent_file(b"off\n", "c").is_err());
        assert!(parse_consent_file(b"", "c").is_err());
    }
}
