//! Sealed snapshots (§5.6, DD-47/DD-103/DD-122): encode a captured body
//! as BLOB + PROVENANCE chunk tiles in a fresh archive, seal it on the
//! temp file's own fd, and publish it atomically; reuse an existing
//! snapshot only when it passes the DD-103 checks.

use std::os::unix::io::FromRawFd;
use std::os::unix::io::RawFd;

use s3a_core::{HyperTileHeader, TileType};
use s3a_engine::{
    seal_archive_from_file, verify_sealed_bytes, MmapReader, TileWriter,
    DEFAULT_SEAL_K, DEFAULT_SEAL_M,
};
use sha2::{Digest, Sha256};

use crate::paths::{close_fd, fstat_fd, fsync_dir, openat_child_rw, read_fd_bounded, unlink_at};
use crate::timefmt::rfc3339_secs;

/// Lowercase hex SHA-256 of bytes.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

/// The PROVENANCE document for a fetched snapshot (DD-47): exactly
/// `sha256=<hex>\nurl=<source_url>\ncaptured=<RFC3339 seconds UTC>\n`
/// (LF, final newline). `url=` is the candidate's listed `source_url`
/// (pre-redirect).
pub fn provenance_document(sha_hex: &str, source_url: &str, captured: &str) -> Vec<u8> {
    format!("sha256={sha_hex}\nurl={source_url}\ncaptured={captured}\n").into_bytes()
}

/// Encode a captured body + provenance document into `(tile_type,
/// payload)` pairs via the pin's `chunk_tiles` (40-byte prefix:
/// 32-byte blob_key + LE chunk index + LE total). The BLOB key is the
/// SHA-256 of the captured bytes; the PROVENANCE key is the SHA-256 of
/// the provenance document.
pub fn snapshot_tiles(body: &[u8], source_url: &str, captured: &str) -> (Vec<(u32, Vec<u8>)>, String) {
    let sha_hex = sha256_hex(body);
    let mut body_key = [0u8; 32];
    body_key.copy_from_slice(&Sha256::digest(body));
    let mut tiles = s3a_pack_clean::archive::chunk_tiles(TileType::BLOB, &body_key, body);
    let doc = provenance_document(&sha_hex, source_url, captured);
    let mut doc_key = [0u8; 32];
    doc_key.copy_from_slice(&Sha256::digest(&doc));
    tiles.extend(s3a_pack_clean::archive::chunk_tiles(
        TileType::PROVENANCE,
        &doc_key,
        &doc,
    ));
    (tiles, sha_hex)
}

/// Reassemble the BLOB payload for `blob_key` from a snapshot's tiles.
/// Returns the reassembled bytes, or `None` when the tiles are missing
/// or do not reassemble.
pub fn reassemble_blob(bytes: &[u8], blob_key: &[u8; 32]) -> Option<Vec<u8>> {
    let reader = MmapReader::from_bytes(bytes).ok()?;
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for i in 0..reader.tile_count() as usize {
        let Some((header, payload)) = reader.get_tile(i) else {
            continue;
        };
        if header.tile_type == TileType::BLOB {
            payloads.push(payload.to_vec());
        }
    }
    let map = s3a_pack_clean::archive::unchunk(&payloads).ok()?;
    map.get(blob_key).cloned()
}

/// The DD-103 reuse checks on snapshot file bytes for `sha_hex`:
/// the bytes verify as a sealed archive, and the BLOB chunks whose
/// `blob_key` equals the SHA-256 reassemble to bytes hashing to it.
pub fn snapshot_reuse_ok(bytes: &[u8], sha_hex: &str) -> bool {
    if verify_sealed_bytes(bytes).is_err() {
        return false;
    }
    let key: [u8; 32] = match hex_to_32(sha_hex) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let Some(doc) = reassemble_blob(bytes, &key) else {
        return false;
    };
    sha256_hex(&doc) == sha_hex
}

fn hex_to_32(hex: &str) -> Result<[u8; 32], ()> {
    let b = hex.as_bytes();
    if b.len() != 64 {
        return Err(());
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = hex_val(b[2 * i]).ok_or(())?;
        let lo = hex_val(b[2 * i + 1]).ok_or(())?;
        out[i] = (hi << 4) | lo;
    }
    Ok(out)
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

/// Read a snapshot file through the snapshot dirfd (safe-open:
/// `O_NOFOLLOW|O_NONBLOCK`, regular file, at most `max_open_bytes` + 1
/// bytes) and run the DD-103 reuse checks for `sha_hex`. Returns
/// `Ok(true)` when the file is present and usable, `Ok(false)` when it
/// is missing (ENOENT — a re-fetch, DD-152), and `Err(())` when it is
/// present but fails a check (a symlink, a non-regular file, oversize,
/// or a failed seal/chunk/hash check).
pub fn check_snapshot(
    snapshot_dirfd: RawFd,
    sha_hex: &str,
    max_open_bytes: u64,
) -> Result<bool, ()> {
    let name = format!("{sha_hex}.s3a");
    let fd = match crate::paths::openat_child(snapshot_dirfd, &name, false) {
        Ok(fd) => fd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(_) => return Err(()),
    };
    let st = match fstat_fd(fd) {
        Ok(st) => st,
        Err(_) => {
            close_fd(fd);
            return Err(());
        }
    };
    if !st.is_regular {
        close_fd(fd);
        return Err(());
    }
    let bytes = match read_fd_bounded(fd, max_open_bytes) {
        Ok(b) => b,
        Err(_) => {
            close_fd(fd);
            return Err(());
        }
    };
    close_fd(fd);
    if bytes.len() as u64 > max_open_bytes {
        return Err(());
    }
    if snapshot_reuse_ok(&bytes, sha_hex) {
        Ok(true)
    } else {
        Err(())
    }
}

/// The outcome of publishing a snapshot in the step-2 publish hold.
pub enum PublishOutcome {
    /// The snapshot file `{sha}.s3a` is present and usable (newly
    /// published, or an existing file that passed DD-103).
    Published { sha256: String },
    /// An existing `{sha}.s3a` failed the DD-103 checks (409).
    Conflict,
    /// The sealed snapshot exceeds `--max-open-bytes` (200, AT-227).
    TooLarge,
    /// A temp write, fsync, rename, or seal failure (500, AT-212).
    WriteFailed,
}

/// Publish a snapshot for a freshly fetched body in the step-2 publish
/// hold (the write mutex is held by the caller). `hooks` carries the
/// §5.2.7 fail/pause points. The temp file is held `LOCK_EX` on its own
/// fd from create until publish/delete (DD-122); it is written and
/// sealed through fd-taking engine APIs on that same description (no
/// path reopen, no `/proc/self/fd`). If `{sha}.s3a` already exists, the
/// DD-103 checks decide reuse vs `snapshot_conflict`; the existing file
/// is never overwritten. The `captured` time is the fetch-completion
/// time (RFC3339 seconds UTC).
pub fn publish_snapshot(
    snapshot_dirfd: RawFd,
    body: &[u8],
    source_url: &str,
    captured: &str,
    max_open_bytes: u64,
    hooks: &crate::hooks::Hooks,
) -> PublishOutcome {
    let (tiles, sha_hex) = snapshot_tiles(body, source_url, captured);
    let final_name = format!("{sha_hex}.s3a");
    // An existing file is reused only when it passes DD-103.
    match check_snapshot(snapshot_dirfd, &sha_hex, max_open_bytes) {
        Ok(true) => return PublishOutcome::Published { sha256: sha_hex },
        Ok(false) => {}
        Err(()) => return PublishOutcome::Conflict,
    }
    // Write the temp: create (0600, O_NOFOLLOW), LOCK_EX on its own fd.
    let pid = std::process::id();
    let tmp_name = format!(
        "s3a-kite-tmp-{pid}-snapshot-{}",
        crate::consent::random_suffix()
    );
    let tmp_fd = match openat_child_rw(snapshot_dirfd, &tmp_name, true) {
        Ok(fd) => fd,
        Err(_) => return PublishOutcome::WriteFailed,
    };
    let flock_rc = unsafe { libc::flock(tmp_fd, libc::LOCK_EX) };
    if flock_rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    if let Some(errno) = hooks.fail_errno("fail_snapshot_write_temp") {
        let _ = errno;
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    // Build the archive on the temp fd (a dup, so the TileWriter owns
    // its own description of the same file; the host's LOCK_EX is on
    // the same description).
    let dup_fd = unsafe { libc::dup(tmp_fd) };
    if dup_fd < 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    let file = unsafe { std::fs::File::from_raw_fd(dup_fd) };
    let mut writer = match TileWriter::create_from_file(file) {
        Ok(w) => w,
        Err(_) => {
            close_fd(tmp_fd);
            let _ = unlink_at(snapshot_dirfd, &tmp_name);
            return PublishOutcome::WriteFailed;
        }
    };
    let mut write_failed = false;
    for (tile_type, payload) in &tiles {
        let mut header = HyperTileHeader::new(0, *tile_type);
        header.record_count = 1;
        header.payload_bytes = payload.len() as u32;
        header.data_crc32 = crc32c::crc32c(payload);
        if writer.write_raw_hyper_tile(&header, payload).is_err() {
            write_failed = true;
            break;
        }
    }
    if write_failed {
        drop(writer);
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    // Seal in place on the same file description (fd-taking API).
    let mut file = match writer.into_file() {
        Ok(f) => f,
        Err(_) => {
            close_fd(tmp_fd);
            let _ = unlink_at(snapshot_dirfd, &tmp_name);
            return PublishOutcome::WriteFailed;
        }
    };
    if seal_archive_from_file(&mut file, DEFAULT_SEAL_K, DEFAULT_SEAL_M).is_err() {
        drop(file);
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    // The sealed size decides `snapshot_too_large` (AT-227): the host
    // does not publish a snapshot larger than --max-open-bytes.
    let sealed_len = match file.metadata() {
        Ok(m) => m.len(),
        Err(_) => {
            drop(file);
            close_fd(tmp_fd);
            let _ = unlink_at(snapshot_dirfd, &tmp_name);
            return PublishOutcome::WriteFailed;
        }
    };
    drop(file);
    if sealed_len > max_open_bytes {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::TooLarge;
    }
    if hooks.fail_errno("fail_snapshot_write_fsync").is_some() {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    // fsync the temp (its own fd), then the pause hook, then rename.
    let fsync_rc = unsafe { libc::fsync(tmp_fd) };
    if fsync_rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    if hooks.pause_at("pause_before_snapshot_rename") {
        hooks.wait_pause("pause_before_snapshot_rename");
    }
    if hooks.fail_errno("fail_snapshot_write_rename").is_some() {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    let tmp_c = std::ffi::CString::new(tmp_name.clone()).unwrap();
    let final_c = std::ffi::CString::new(final_name.clone()).unwrap();
    let rc = unsafe {
        libc::renameat(
            snapshot_dirfd,
            tmp_c.as_ptr(),
            snapshot_dirfd,
            final_c.as_ptr(),
        )
    };
    if rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(snapshot_dirfd, &tmp_name);
        return PublishOutcome::WriteFailed;
    }
    close_fd(tmp_fd);
    if fsync_dir(snapshot_dirfd).is_err() {
        return PublishOutcome::WriteFailed;
    }
    PublishOutcome::Published { sha256: sha_hex }
}

/// The current time as an RFC3339-seconds-UTC string (for the
/// PROVENANCE `captured` field and the LRS statement `timestamp`).
pub fn now_rfc3339() -> String {
    rfc3339_secs(std::time::SystemTime::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_document_shape() {
        let doc = provenance_document(
            "ab12",
            "https://example.test/p",
            "2026-11-19T00:00:00Z",
        );
        assert_eq!(
            String::from_utf8(doc).unwrap(),
            "sha256=ab12\nurl=https://example.test/p\ncaptured=2026-11-19T00:00:00Z\n"
        );
    }

    #[test]
    fn chunk_round_trip() {
        let body = b"hello snapshot world\n".repeat(10);
        let (tiles, sha) = snapshot_tiles(&body, "https://example.test/p", "2026-11-19T00:00:00Z");
        assert_eq!(sha, sha256_hex(&body));
        // The BLOB tiles reassemble to the body.
        let blob_tiles: Vec<Vec<u8>> = tiles
            .iter()
            .filter(|(t, _)| *t == TileType::BLOB)
            .map(|(_, p)| p.clone())
            .collect();
        let map = s3a_pack_clean::archive::unchunk(&blob_tiles).unwrap();
        let mut key = [0u8; 32];
        key.copy_from_slice(&Sha256::digest(&body));
        assert_eq!(map.get(&key).unwrap(), &body.to_vec());
    }
}
