//! The inclusion index (§5.5, F-01/F-02/F-03):
//! `--data-dir/inclusion-index.jsonl`, append-only, one JSON object per
//! inclusion. Each mutation copies the current file to a temp in
//! `--data-dir`, appends the new line, `fsync`s the temp, publishes with
//! an atomic rename, `fsync`s `--data-dir`, mode `0600`. There is no
//! in-place rewrite of a JSONL line.
//!
//! The idempotency key is the current working-archive `file_uuid` +
//! paper identity string + verb id; lookup uses the **last** matching
//! line (DD-104).

use std::os::unix::io::RawFd;

use crate::paths::{close_fd, fsync_dir, openat_child, openat_child_rw, read_fd_bounded, unlink_at};

/// The index file name inside `--data-dir`.
pub const INDEX_NAME: &str = "inclusion-index.jsonl";

/// The required keys of an index line (AT-158: a present line missing a
/// required key is a startup error).
pub const REQUIRED_KEYS: &[&str] = &[
    "paper_identity",
    "verb_id",
    "object_hash",
    "timestamp_sec",
    "working_archive",
    "file_uuid",
    "snapshot_sha256",
];

/// One inclusion-index line.
#[derive(Debug, Clone)]
pub struct IndexLine {
    pub paper_identity: String,
    pub verb_id: u32,
    pub object_hash: u32,
    pub timestamp_sec: u64,
    pub working_archive: String,
    /// 32 lowercase hex chars (`{:016x}{:016x}` of the two u64s).
    pub file_uuid: String,
    pub snapshot_sha256: Option<String>,
}

impl IndexLine {
    /// Serialize one line (SPEC field order; `snapshot_sha256` is null
    /// until a successful snapshot exists).
    pub fn to_json(&self) -> String {
        let sha = match &self.snapshot_sha256 {
            Some(s) => format!("\"{s}\""),
            None => "null".to_string(),
        };
        format!(
            "{{\"paper_identity\":{},\"verb_id\":{},\"object_hash\":{},\"timestamp_sec\":{},\"working_archive\":{},\"file_uuid\":{},\"snapshot_sha256\":{}}}",
            serde_json::to_string(&self.paper_identity).unwrap(),
            self.verb_id,
            self.object_hash,
            self.timestamp_sec,
            serde_json::to_string(&self.working_archive).unwrap(),
            serde_json::to_string(&self.file_uuid).unwrap(),
            sha,
        )
    }

    /// Parse one line. Returns `None` when the line is not a JSON object
    /// or is missing a required key (AT-158).
    pub fn from_json(line: &str) -> Option<IndexLine> {
        let v: serde_json::Value = serde_json::from_str(line).ok()?;
        let obj = v.as_object()?;
        for key in REQUIRED_KEYS {
            if !obj.contains_key(*key) {
                return None;
            }
        }
        Some(IndexLine {
            paper_identity: obj.get("paper_identity")?.as_str()?.to_string(),
            verb_id: obj.get("verb_id")?.as_u64()? as u32,
            object_hash: obj.get("object_hash")?.as_u64()? as u32,
            timestamp_sec: obj.get("timestamp_sec")?.as_u64()?,
            working_archive: obj.get("working_archive")?.as_str()?.to_string(),
            file_uuid: obj.get("file_uuid")?.as_str()?.to_string(),
            snapshot_sha256: obj.get("snapshot_sha256")?.as_str().map(|s| s.to_string()),
        })
    }
}

/// Format a `FileHeader.file_uuid` (`[u64; 2]`) as 32 lowercase hex
/// chars. SPEC fixes no byte order; the index is only ever read by this
/// host.
pub fn file_uuid_hex(file_uuid: [u64; 2]) -> String {
    format!("{:016x}{:016x}", file_uuid[0], file_uuid[1])
}

/// Read every line of the index through the data dirfd. Returns an
/// error when the file cannot be read or a line is malformed (the
/// startup validation guarantees well-formedness; a failure here means
/// the file changed mid-process).
pub fn read_index(data_dirfd: RawFd) -> Result<Vec<IndexLine>, String> {
    let fd = openat_child(data_dirfd, INDEX_NAME, false)
        .map_err(|e| format!("cannot open {INDEX_NAME}: {e}"))?;
    let bytes = read_fd_bounded(fd, u64::MAX)
        .map_err(|e| format!("cannot read {INDEX_NAME}: {e}"))?;
    close_fd(fd);
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{INDEX_NAME} is not UTF-8"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let parsed = IndexLine::from_json(line)
            .ok_or_else(|| format!("{INDEX_NAME}: malformed line"))?;
        out.push(parsed);
    }
    Ok(out)
}

/// The last line for `(file_uuid, identity, verb)` (DD-104: last
/// matching line wins). Lines whose `file_uuid` does not match the open
/// archive do not suppress a new inclusion.
pub fn find_last<'a>(
    lines: &'a [IndexLine],
    file_uuid: &str,
    identity: &str,
    verb_id: u32,
) -> Option<&'a IndexLine> {
    lines
        .iter()
        .rev()
        .find(|l| l.file_uuid == file_uuid && l.paper_identity == identity && l.verb_id == verb_id)
}

/// The last line for `(file_uuid, identity)` (any verb) with a non-null
/// `snapshot_sha256` — the step-0 snapshot test (DD-104).
pub fn find_last_snapshot<'a>(
    lines: &'a [IndexLine],
    file_uuid: &str,
    identity: &str,
) -> Option<&'a IndexLine> {
    lines.iter().rev().find(|l| {
        l.file_uuid == file_uuid
            && l.paper_identity == identity
            && l.snapshot_sha256.is_some()
    })
}

/// The last line for `(file_uuid, identity)` (any verb) with a non-null
/// `snapshot_sha256`, as the sha string.
pub fn last_snapshot_sha(lines: &[IndexLine], file_uuid: &str, identity: &str) -> Option<String> {
    find_last_snapshot(lines, file_uuid, identity)
        .and_then(|l| l.snapshot_sha256.clone())
}

/// Append one line to the index: copy the current file to a temp in
/// `--data-dir`, append the line, `fsync` the temp, publish with an
/// atomic rename, `fsync` `--data-dir`, mode `0600`. The temp is held
/// `LOCK_EX` on its own fd (DD-122). `hook_point` is the §5.2.7 point
/// for this append (`fail_index_append` for step 1, or
/// `fail_snapshot_index_append` for the step-2 snapshot append).
pub fn append_index(
    data_dirfd: RawFd,
    line: &IndexLine,
    hook_point: &str,
    hooks: &crate::hooks::Hooks,
) -> Result<(), String> {
    // Read the current file (missing is treated as empty; startup
    // creates it, so a missing file here is a mid-process deletion).
    let current: Vec<u8> = match openat_child(data_dirfd, INDEX_NAME, false) {
        Ok(fd) => {
            let b = read_fd_bounded(fd, u64::MAX)
                .map_err(|e| format!("cannot read {INDEX_NAME}: {e}"))?;
            close_fd(fd);
            b
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("cannot open {INDEX_NAME}: {e}")),
    };
    let pid = std::process::id();
    let tmp_name = format!(
        "s3a-kite-tmp-{pid}-index-{}",
        crate::consent::random_suffix()
    );
    let tmp_fd = openat_child_rw(data_dirfd, &tmp_name, true)
        .map_err(|e| format!("index temp create failed: {e}"))?;
    let flock_rc = unsafe { libc::flock(tmp_fd, libc::LOCK_EX) };
    if flock_rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err("index temp lock failed".to_string());
    }
    if hooks.fail_errno(hook_point).is_some() {
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err(format!("index append failed (hook {hook_point})"));
    }
    // Write the old bytes plus the new line, then fsync.
    let mut payload = current;
    payload.extend_from_slice(line.to_json().as_bytes());
    payload.push(b'\n');
    let write_rc = {
        let mut written = 0;
        let mut rc = 0;
        while written < payload.len() {
            let n = unsafe {
                libc::write(
                    tmp_fd,
                    payload[written..].as_ptr() as *const libc::c_void,
                    payload.len() - written,
                )
            };
            if n < 0 {
                rc = -1;
                break;
            }
            written += n as usize;
        }
        rc
    };
    if write_rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err(format!(
            "index temp write failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let fsync_rc = unsafe { libc::fsync(tmp_fd) };
    if fsync_rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err(format!(
            "index temp fsync failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let tmp_c = std::ffi::CString::new(tmp_name.clone()).unwrap();
    let idx_c = std::ffi::CString::new(INDEX_NAME).unwrap();
    let rc = unsafe { libc::renameat(data_dirfd, tmp_c.as_ptr(), data_dirfd, idx_c.as_ptr()) };
    if rc != 0 {
        let e = std::io::Error::last_os_error();
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err(format!("index rename failed: {e}"));
    }
    close_fd(tmp_fd);
    fsync_dir(data_dirfd).map_err(|e| format!("data dir fsync failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_round_trip() {
        let line = IndexLine {
            paper_identity: "SRC-0007".to_string(),
            verb_id: 13,
            object_hash: 0xdeadbeef,
            timestamp_sec: 1794000000,
            working_archive: "/x/working.s3a".to_string(),
            file_uuid: "00000000000000010000000000000002".to_string(),
            snapshot_sha256: None,
        };
        let json = line.to_json();
        let back = IndexLine::from_json(&json).unwrap();
        assert_eq!(back.paper_identity, "SRC-0007");
        assert_eq!(back.verb_id, 13);
        assert_eq!(back.snapshot_sha256, None);
        // A missing key is malformed.
        assert!(IndexLine::from_json(
            "{\"paper_identity\": \"SRC-0007\", \"verb_id\": 13}"
        )
        .is_none());
        assert!(IndexLine::from_json("not json").is_none());
    }

    #[test]
    fn file_uuid_hex_shape() {
        assert_eq!(file_uuid_hex([1, 2]), "00000000000000010000000000000002");
    }

    #[test]
    fn last_matching_line_wins() {
        let mk = |verb: u32, sha: Option<&str>| IndexLine {
            paper_identity: "SRC-0007".to_string(),
            verb_id: verb,
            object_hash: 1,
            timestamp_sec: 1,
            working_archive: "/x".to_string(),
            file_uuid: "aa".to_string(),
            snapshot_sha256: sha.map(|s| s.to_string()),
        };
        let lines = vec![mk(13, None), mk(11, Some("sha1")), mk(13, Some("sha2"))];
        assert_eq!(
            find_last(&lines, "aa", "SRC-0007", 13).unwrap().snapshot_sha256.as_deref(),
            Some("sha2")
        );
        assert_eq!(
            last_snapshot_sha(&lines, "aa", "SRC-0007").as_deref(),
            Some("sha2")
        );
        assert!(find_last(&lines, "bb", "SRC-0007", 13).is_none());
        assert!(find_last(&lines, "aa", "SRC-0007", 14).is_none());
    }
}
