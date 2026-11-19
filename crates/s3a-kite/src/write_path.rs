// Step 10 — the Scholar write path (§5.2.1, §5.2.2, §5.5).
//
// Steps 0/1 run in one write-mutex hold (open the working archive, the
// AT-46 ADDON_EDU check, the name-binding re-check, the step-1
// inclusion write, the inclusion-index append). Step 2 fetches with
// the mutex released and publishes in the publish hold; step 2b
// delivers under the mutex; step 3 posts the LRS statement with no
// mutex across the POST.
//
// The inclusion write is the `add_paper` gate's counterpart: the
// Navigator writes a tile-10 inclusion record only for a verified
// dossier (the candidates listing already ran the §5.5 dossier checks,
// including the `DossierVerification` block). Sealed = trusted;
// unverified items never enter the sealed store.

use std::os::unix::io::{FromRawFd, RawFd};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use s3a_core::{
    S3ACoordinate, ADDON_EDU, FILE_HEADER_SIZE, FileHeader,
};
use s3a_engine::{resolve_active_header, TileWriter};
use s3a_pack_scholar::{HumanLrsRecord, ScholarTileType};
use sha2::{Digest, Sha256};

use crate::candidates::{open_relative, open_relative_rw, Candidate};
use crate::fetch::{fetch_url, FetchConfig, FetchError, FetchOutcome};
use crate::http::Response;
use crate::index::{
    append_index, file_uuid_hex, find_last, last_snapshot_sha, read_index, IndexLine,
};
use crate::lrs::{
    actor_hash, build_statement, new_session_uuid, new_statement_id, post_statement,
    write_retry_file, PostOutcome,
};
use crate::paths::{
    close_fd, flock_ex_nb, fstat_fd, fstatat_nofollow, fsync_dir, lexical_normalize,
    openat_child, openat_child_rw, read_fd_bounded, read_fd_up_to, unlink_at,
};
use crate::server::{err, Host};
use crate::snapshot::{
    check_snapshot, publish_snapshot, reassemble_blob, sha256_hex, PublishOutcome,
};
use crate::timefmt::rfc3339_secs;

/// The current Unix time (seconds). A select/deselect statement's
/// timestamp is the event time (a select writes no inclusion, so there
/// is no inclusion-write time); a download's is the tile-10 write time.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------
// In-flight registry (step 2 coordination, DD-104)
// ---------------------------------------------------------------------

/// The outcome of a step-2 fetch, shared with waiters. A waiter never
/// starts a GET and never appends an index line.
#[derive(Clone, Debug)]
pub enum Step2Outcome {
    /// The snapshot was published (or already existed). The sha names
    /// `--snapshot-dir/{sha}.s3a`.
    Published { sha256: String },
    /// The fetch failed (or the capture was too large to seal).
    FetchFailed { fetch_error: FetchError },
    /// A newly fetched capture's `{sha}.s3a` already existed and failed
    /// the DD-103 reuse check (AT-180).
    SnapshotConflict,
    /// A snapshot or index write failed during the publish (AT-212/213).
    WriteFailed,
}

/// One in-flight step-2 fetch, keyed by `(file_uuid_hex, identity)`.
pub struct InFlightFetch {
    pub done: Mutex<bool>,
    pub condvar: Condvar,
    pub outcome: Mutex<Option<Arc<Step2Outcome>>>,
}

impl InFlightFetch {
    pub fn new() -> Self {
        InFlightFetch {
            done: Mutex::new(false),
            condvar: Condvar::new(),
            outcome: Mutex::new(None),
        }
    }

    /// Wait for the starter to finish and return its outcome.
    pub fn wait(&self) -> Arc<Step2Outcome> {
        let mut done = self.done.lock().unwrap();
        while !*done {
            done = self.condvar.wait(done).unwrap();
        }
        self.outcome.lock().unwrap().clone().unwrap()
    }

    /// Finish the fetch: record the outcome, remove the registry entry,
    /// and wake every waiter.
    pub fn finish(&self, host: &Host, key: &(String, String), outcome: Step2Outcome) {
        *self.outcome.lock().unwrap() = Some(Arc::new(outcome));
        *self.done.lock().unwrap() = true;
        host.inflight.lock().unwrap().remove(key);
        self.condvar.notify_all();
    }
}

// ---------------------------------------------------------------------
// Drive-path resolution (DD-59 + DD-146)
// ---------------------------------------------------------------------

/// A validated on-disk source: the fd opened through the DD-146 walk
/// and its size at open time. The fd is closed when this is dropped.
pub struct DriveFd {
    pub fd: RawFd,
    /// Size at open time, from the validation fstat.
    pub size: u64,
}

impl Drop for DriveFd {
    fn drop(&mut self) {
        close_fd(self.fd);
    }
}

/// Resolve a candidate's raw front-matter `drive_path` to a validated
/// fd, once per request, outside the write mutex. Returns `None` when
/// the candidate has no usable on-disk source (the source table then
/// falls through to fetch / `no_source`). A failed open means "not a
/// usable on-disk source" — it is never `path_escape` and never holds
/// the mutex (AT-202).
pub fn resolve_drive_path(host: &Host, cand: &Candidate) -> Option<DriveFd> {
    let raw = cand.drive_path.as_deref()?;
    if raw.is_empty() {
        return None;
    }
    let started = &host.started;
    let canon = &started.canon;
    let dirfds = &started.dirfds;
    // Resolve relative paths against the dossier parent, then lexically
    // normalize.
    let dossier_parent = cand
        .page_path
        .as_ref()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))?;
    let joined = if std::path::Path::new(raw).is_absolute() {
        std::path::PathBuf::from(raw)
    } else {
        dossier_parent.join(raw)
    };
    let normalized = lexical_normalize(&joined)?;
    // Match to the UNIQUE allowed root among wiki/corpus/view (if set),
    // as a path-component prefix.
    let mut matches: Vec<(RawFd, &std::path::Path)> = Vec::new();
    for (fd, root) in [
        (dirfds.wiki_root, canon.wiki_root.as_deref()),
        (dirfds.corpus_root, canon.corpus_root.as_deref()),
        (dirfds.view_root, canon.view_root.as_deref()),
    ] {
        let (Some(fd), Some(root)) = (fd, root) else {
            continue;
        };
        if normalized.starts_with(root) && normalized != root {
            matches.push((fd, root));
        }
    }
    if matches.len() != 1 {
        return None;
    }
    let (root_fd, root) = matches[0];
    // Not under the snapshot or download dirs.
    if let Some(snap) = &canon.snapshot_dir {
        if normalized.starts_with(snap) {
            return None;
        }
    }
    if let Some(down) = &canon.download_dir {
        if normalized.starts_with(down) {
            return None;
        }
    }
    let rel = normalized.strip_prefix(root).ok()?;
    // The DD-146 walk: O_NOFOLLOW per component, the final component
    // O_NOFOLLOW|O_NONBLOCK. When the `grow_drive_path_during_copy`
    // hook is configured, the final component is opened O_RDWR so the
    // host can extend the file itself during the 2b copy (§5.2.7).
    let grow = started.config.hooks.grow_at("grow_drive_path_during_copy");
    let fd = if grow {
        open_relative_rw(root_fd, rel)?
    } else {
        open_relative(root_fd, rel)?
    };
    let st = fstat_fd(fd).ok()?;
    if !st.is_regular {
        close_fd(fd);
        return None;
    }
    if st.size > host.started.config.max_open_bytes {
        close_fd(fd);
        return None;
    }
    Some(DriveFd { fd, size: st.size })
}

// ---------------------------------------------------------------------
// The step-0/1 hold
// ---------------------------------------------------------------------

/// What can go wrong while opening the working archive and checking
/// the AT-46 hold.
#[derive(Debug)]
pub enum HoldError {
    /// The archive is missing, replaced, or unreadable (500).
    WriteFailed,
    /// The archive's addon_mask lacks ADDON_EDU (409).
    AddonMissing,
    /// The engine exclusive lock could not be taken (500).
    LockFailed,
}

/// The result of the step-0 snapshot test (DD-104).
#[derive(Debug, Clone)]
pub enum Step0Snapshot {
    /// No usable snapshot: no line, or the last recorded sha's file is
    /// missing (ENOENT) — a re-fetch is allowed (DD-152).
    None,
    /// A usable snapshot exists.
    Usable { sha256: String },
    /// The last recorded sha's file is present but fails DD-103
    /// (AT-209): no fetch, no drive_path fallback.
    Invalid,
}

/// The shared state of the step-0/1 hold, held across the request.
pub struct Hold {
    /// The working archive fd, opened O_RDWR via the parent dirfd.
    pub archive_fd: RawFd,
    /// The TileWriter attached to a dup of the archive fd (download
    /// only; `None` for a select, which writes nothing).
    pub writer: Option<TileWriter>,
    /// The archive's file_uuid, as 32 lowercase hex chars.
    pub file_uuid_hex: String,
    /// The inclusion-index lines read in the hold.
    pub index: Vec<IndexLine>,
    /// Whether a matching verb line for (file_uuid, identity, verb)
    /// was already present — step 1 and step 3 are then skipped.
    pub step1_skipped: bool,
    /// The step-0 snapshot state.
    pub snapshot: Step0Snapshot,
}

impl Drop for Hold {
    fn drop(&mut self) {
        // The writer holds a dup of the archive fd; dropping it closes
        // the dup, and this closes the original.
        self.writer = None;
        close_fd(self.archive_fd);
    }
}

/// Open the working archive fd via the parent dirfd and check that
/// its (dev, ino) still matches the startup identity (AT-46/AT-219).
fn open_archive_fd(host: &Host) -> Result<RawFd, HoldError> {
    let started = &host.started;
    let parent = started
        .dirfds
        .working_archive_parent
        .ok_or(HoldError::WriteFailed)?;
    let name = started
        .canon
        .working_archive_basename
        .as_deref()
        .ok_or(HoldError::WriteFailed)?;
    let fd = openat_child_rw(parent, name, false).map_err(|_| HoldError::WriteFailed)?;
    // The (dev, ino) identity check: a mid-process replace is a
    // `write_failed` (AT-46).
    let st = fstat_fd(fd).map_err(|_| HoldError::WriteFailed)?;
    if let Some((d, i)) = started.working_archive_identity {
        if st.dev != d || st.ino != i {
            close_fd(fd);
            return Err(HoldError::WriteFailed);
        }
    }
    Ok(fd)
}

/// Read the active header from the archive fd (a 512-byte read from
/// offset 0) and return (file_uuid_hex, addon_mask). `None` when the
/// header cannot be read or is invalid.
fn read_header(fd: RawFd) -> Option<(String, u32)> {
    let header = read_fd_up_to(fd, FILE_HEADER_SIZE as u64).ok()?;
    if header.len() < FILE_HEADER_SIZE {
        return None;
    }
    let slot_a: FileHeader = *bytemuck::from_bytes::<FileHeader>(&header[0..64]);
    let slot_b: FileHeader = *bytemuck::from_bytes::<FileHeader>(&header[64..128]);
    let (active, _slot) = resolve_active_header(&slot_a, &slot_b).ok()?;
    Some((file_uuid_hex(active.file_uuid), active.addon_mask))
}

/// The name-binding re-check (§5.2.2/§5.6): after the pause hook, the
/// final component at the parent dirfd must still be the same
/// (dev, ino) as the opened fd. `false` when it moved.
fn name_binding_ok(host: &Host, fd: RawFd) -> bool {
    let started = &host.started;
    let Some(parent) = started.dirfds.working_archive_parent else {
        return false;
    };
    let Some(name) = started.canon.working_archive_basename.as_deref() else {
        return false;
    };
    let Ok(st) = fstat_fd(fd) else {
        return false;
    };
    match fstatat_nofollow(parent, name) {
        Ok(st2) => st.dev == st2.dev && st.ino == st2.ino,
        Err(_) => false,
    }
}

/// The data dirfd (Scholar mode always has one).
fn data_dirfd(host: &Host) -> Option<RawFd> {
    host.started.dirfds.data_dir
}

/// Read the inclusion index in the hold (a missing file reads as
/// empty here; startup created it).
fn read_index_in(host: &Host) -> Vec<IndexLine> {
    match data_dirfd(host) {
        Some(fd) => read_index(fd).unwrap_or_default(),
        None => Vec::new(),
    }
}

/// Run the step-0 snapshot test (DD-104) for this identity: the last
/// recorded sha's file, present and passing DD-103, is usable; present
/// but failing is `snapshot_invalid`; missing is a re-fetch.
fn step0_snapshot(host: &Host, index: &[IndexLine], file_uuid_hex: &str, identity: &str) -> Step0Snapshot {
    let Some(line) = index
        .iter()
        .rev()
        .find(|l| {
            l.file_uuid == file_uuid_hex
                && l.paper_identity == identity
                && l.snapshot_sha256.is_some()
        })
    else {
        return Step0Snapshot::None;
    };
    let sha = line.snapshot_sha256.clone().unwrap();
    let Some(snap_fd) = host.started.dirfds.snapshot_dir else {
        return Step0Snapshot::None;
    };
    match check_snapshot(snap_fd, &sha, host.started.config.max_open_bytes) {
        Ok(true) => Step0Snapshot::Usable { sha256: sha },
        Ok(false) => Step0Snapshot::None,
        Err(()) => Step0Snapshot::Invalid,
    }
}

/// The select hold: open the archive, read the header, the ADDON_EDU
/// check, the pause + name-binding re-check, then the index read and
/// the step-0 snapshot test. A select takes no engine flock (DD-114
/// lock 2 is "taken for each tile-10 write"; a select writes none),
/// so `lock_failed` is vacuous for a select.
fn open_hold_select(host: &Host, identity: &str) -> Result<Hold, HoldError> {
    let fd = open_archive_fd(host)?;
    let (file_uuid, addon_mask) = read_header(fd).ok_or_else(|| {
        close_fd(fd);
        HoldError::WriteFailed
    })?;
    if addon_mask & ADDON_EDU == 0 {
        close_fd(fd);
        return Err(HoldError::AddonMissing);
    }
    // The pause hook runs after the header + addon check, in the same
    // hold; the name-binding re-check runs immediately before the
    // step-1 work (§5.2.7/§5.6).
    if host.started.config.hooks.pause_at("pause_before_name_binding_check") {
        host.started
            .config
            .hooks
            .wait_pause("pause_before_name_binding_check");
    }
    if !name_binding_ok(host, fd) {
        close_fd(fd);
        return Err(HoldError::WriteFailed);
    }
    let index = read_index_in(host);
    let snapshot = step0_snapshot(host, &index, &file_uuid, identity);
    Ok(Hold {
        archive_fd: fd,
        writer: None,
        file_uuid_hex: file_uuid,
        index,
        step1_skipped: false,
        snapshot,
    })
}

/// The download hold: open the archive, attach the TileWriter (which
/// takes the engine exclusive lock), the ADDON_EDU check, the pause +
/// name-binding re-check, then the index read and the step-0 test.
fn open_hold_download(host: &Host, identity: &str) -> Result<Hold, HoldError> {
    let fd = open_archive_fd(host)?;
    // Attach the writer to a dup of the fd and take the engine
    // exclusive lock (DD-114 lock 2). The writer takes the lock on the
    // dup; the fd-taking constructor does not re-resolve the path.
    let dup = {
        let raw = unsafe { libc::dup(fd) };
        if raw < 0 {
            close_fd(fd);
            return Err(HoldError::WriteFailed);
        }
        raw
    };
    let file = unsafe { std::fs::File::from_raw_fd(dup) };
    let writer = match TileWriter::open_append_from_file(file) {
        Ok(w) => w,
        Err(_) => {
            close_fd(fd);
            return Err(HoldError::WriteFailed);
        }
    };
    if writer.try_lock_exclusive().is_err() {
        close_fd(fd);
        return Err(HoldError::LockFailed);
    }
    let file_uuid = file_uuid_hex(writer.file_uuid());
    if writer.addon_mask() & ADDON_EDU == 0 {
        close_fd(fd);
        return Err(HoldError::AddonMissing);
    }
    if host
        .started
        .config
        .hooks
        .pause_at("pause_before_name_binding_check")
    {
        host.started
            .config
            .hooks
            .wait_pause("pause_before_name_binding_check");
    }
    if !name_binding_ok(host, fd) {
        close_fd(fd);
        return Err(HoldError::WriteFailed);
    }
    let index = read_index_in(host);
    let snapshot = step0_snapshot(host, &index, &file_uuid, identity);
    Ok(Hold {
        archive_fd: fd,
        writer: Some(writer),
        file_uuid_hex: file_uuid,
        index,
        step1_skipped: false,
        snapshot,
    })
}

/// The step-1 inclusion write (in the hold): write the tile-10 record
/// and append the index line. Returns `Ok(None)` when a matching verb
/// line was already present (step 1 skipped), or `Ok(Some(ts))` with
/// the inclusion-write timestamp on success.
fn step1_write(
    host: &Host,
    hold: &mut Hold,
    identity: &str,
    verb_id: u32,
) -> Result<Option<u64>, Step1Error> {
    let matched =
        find_last(&hold.index, &hold.file_uuid_hex, identity, verb_id).is_some();
    if matched {
        hold.step1_skipped = true;
        return Ok(None);
    }
    let writer = hold.writer.as_mut().ok_or(Step1Error::WriteFailed)?;
    let now = now_unix();
    let record = HumanLrsRecord::new(
        new_session_uuid(),
        now,
        actor_hash(
            host.started
                .config
                .researcher_id
                .as_deref()
                .unwrap_or(""),
        ),
        verb_id,
        0.0,
        S3ACoordinate::default(),
        crc32c::crc32c(identity.as_bytes()),
    );
    let bytes = bytemuck::bytes_of(&record).to_vec();
    // Write the tile-10 record. `write_raw_hyper_tile` fills
    // data_crc32 / payload_bytes / file_uuid / generation when zero,
    // and assigns the tile_id from `current_tile_id`.
    let mut header = s3a_core::HyperTileHeader::new(0, ScholarTileType::HUMAN_LRS);
    header.record_count = 1;
    if writer.write_raw_hyper_tile(&header, &bytes).is_err() {
        return Err(Step1Error::WriteFailed);
    }
    // The index line records the inclusion (snapshot_sha256 is null
    // until a snapshot is published).
    let line = IndexLine {
        paper_identity: identity.to_string(),
        verb_id,
        object_hash: crc32c::crc32c(identity.as_bytes()),
        timestamp_sec: now,
        working_archive: host
            .started
            .canon
            .working_archive
            .as_ref()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default(),
        file_uuid: hold.file_uuid_hex.clone(),
        snapshot_sha256: None,
    };
    match data_dirfd(host) {
        Some(fd) => {
            if append_index(fd, &line, "fail_index_append", &host.started.config.hooks)
                .is_err()
            {
                // The tile-10 record stays (AT-158): the inclusion is
                // kept, the response is 500 `write_failed` with
                // `included: true`.
                hold.index.push(line);
                return Err(Step1Error::IndexFailed);
            }
        }
        None => {
            hold.index.push(line);
            return Err(Step1Error::IndexFailed);
        }
    }
    hold.index.push(line);
    Ok(Some(now))
}

/// What can go wrong in step 1.
#[derive(Debug)]
pub enum Step1Error {
    /// The tile-10 write failed (the inclusion was not written).
    WriteFailed,
    /// The tile-10 write succeeded but the index append failed.
    IndexFailed,
}

// ---------------------------------------------------------------------
// Step 3 — the LRS emit
// ---------------------------------------------------------------------

/// The LRS step-3 emit (§5.5). Runs with no mutex across the POST.
/// The pre-send consent check and the outcome recording each take a
/// brief mutex hold. Returns the `lrs_status` for the response.
///
/// Callers must NOT hold the write mutex when calling this (it takes
/// the mutex itself for the pre-send check and the outcome record).
///
/// DD-144 (Step 11): every LRS sender (1) registers with the current
/// token, (2) re-checks inhibit inactive + flag on, and (3) only then
/// starts the POST. The POST is cancel-aware: a revoke aborts it
/// mid-flight (`Aborted`), and the post-send check then reports
/// `skipped`. Writing a new retry file and the consent/inhibit check
/// that allows it run in the same mutex hold (AT-156), with the
/// `pause_before_lrs_retry_write` hook parked just before the write.
pub fn emit_step3(
    host: &Host,
    verb_id: u32,
    identity: &str,
    source_url: Option<&str>,
    timestamp_sec: u64,
    file_uuid_hex: &str,
) -> &'static str {
    let Some(lrs) = host.started.config.lrs_url.clone() else {
        return "not_configured";
    };
    let Some(credentials) = host.started.lrs_credentials.clone() else {
        return "failed";
    };
    // (1) Register with the current token and (2) re-check inhibit
    // inactive + flag on, under a brief mutex hold, plus the index
    // read for EXT_CONTENT_SHA256 (DD-137).
    let (token, consent_on, inhibit_active, content_sha256) = {
        let _guard = host.write_mu.lock().unwrap();
        let token = host.latch.register_token();
        let consent_on = host.consent.lock().unwrap().on;
        let inhibit_active = host.latch.inhibit_active();
        let index = read_index_in(host);
        let sha = last_snapshot_sha(&index, file_uuid_hex, identity);
        (token, consent_on, inhibit_active, sha)
    };
    if !consent_on || inhibit_active {
        return "skipped";
    }
    let statement_id = new_statement_id();
    let statement = build_statement(
        &statement_id,
        host.started
            .config
            .researcher_id
            .as_deref()
            .unwrap_or(""),
        host.started.config.study_id.as_deref(),
        verb_id,
        identity,
        source_url,
        content_sha256.as_deref(),
        timestamp_sec,
    );
    // (3) Only then start the POST (cancel-aware: a revoke aborts it).
    let (outcome, _detail) = post_statement(
        &lrs,
        &lrs.resolved,
        &credentials,
        &statement,
        host.started.config.lrs_timeout_secs.unwrap_or(30),
        host.started.config.max_open_bytes,
        Some((&host.latch, token)),
    );
    // Record the outcome under a brief mutex hold, re-checking consent
    // AND the inhibit (a consent-off transition or a revoke during the
    // POST means `skipped`, DD-144). The retry-file write and the
    // check that allows it run in this same hold (AT-156).
    let _guard = host.write_mu.lock().unwrap();
    let consent_on = host.consent.lock().unwrap().on;
    let inhibit_active = host.latch.inhibit_active();
    if !consent_on || inhibit_active {
        return "skipped";
    }
    match outcome {
        PostOutcome::Sent => {
            // A later successful emit starts a new retry episode.
            host.bump_retry_episode();
            "sent"
        }
        PostOutcome::Retryable | PostOutcome::NoResponse | PostOutcome::Aborted => {
            // A retryable outcome (or an aborted POST that survived
            // the re-check above) keeps a `{id}.json` retry file
            // (the worker replays it). The hook parks just before the
            // write, inside this hold, so a revoke that lands there
            // is seen by the re-check below.
            if host
                .started
                .config
                .hooks
                .pause_at("pause_before_lrs_retry_write")
            {
                host.started
                    .config
                    .hooks
                    .wait_pause("pause_before_lrs_retry_write");
            }
            // Re-check after the pause: a revoke during the park
            // means `skipped` and no retry file (AT-156).
            let consent_on = host.consent.lock().unwrap().on;
            let inhibit_active = host.latch.inhibit_active();
            if !consent_on || inhibit_active {
                return "skipped";
            }
            let body = serde_json::to_vec(&statement).unwrap_or_default();
            match data_dirfd(host) {
                Some(fd) => {
                    match write_retry_file(
                        fd,
                        &statement_id,
                        &body,
                        "json",
                        &host.started.config.hooks,
                    ) {
                        Ok(()) => {
                            // Register the queued file: try 1 ran in
                            // this request, so it has attempts=1 and
                            // is not due for another 10 s.
                            register_queued(host, &statement_id);
                            "queued"
                        }
                        Err(_) => "failed",
                    }
                }
                None => "failed",
            }
        }
        PostOutcome::Permanent => {
            let body = serde_json::to_vec(&statement).unwrap_or_default();
            if let Some(fd) = data_dirfd(host) {
                let _ = write_retry_file(
                    fd,
                    &statement_id,
                    &body,
                    "permanent",
                    &host.started.config.hooks,
                );
            }
            "failed"
        }
    }
}

/// Register a file just queued by `emit_step3` in the retry state
/// (Step 11): try 1 ran in the request, so it has attempts=1 and its
/// last attempt is now. Called under the write mutex, in the same hold
/// as the file write, so the worker can never claim the file before
/// its state entry exists.
fn register_queued(host: &Host, statement_id: &str) {
    let episode = host.retry_episode.load(std::sync::atomic::Ordering::SeqCst);
    host.retry_state.lock().unwrap().insert(
        statement_id.to_string(),
        crate::retry::RetryEntry {
            episode,
            attempts: 1,
            last_attempt: Some(std::time::Instant::now()),
        },
    );
}

/// The `lrs_status` for a request whose step 0 matched a verb line
/// (step 3 skipped): `not_configured` takes precedence over `skipped`.
fn skipped_lrs_status(host: &Host) -> &'static str {
    if host.started.config.lrs_url.is_none() {
        "not_configured"
    } else {
        "skipped"
    }
}

/// Step 3 for a download: run the emit when step 1 wrote a new
/// inclusion, or report the skipped status when step 0 matched.
fn download_lrs_status(
    host: &Host,
    step1_skipped: bool,
    inclusion_ts: Option<u64>,
    identity: &str,
    source_url: Option<&str>,
    file_uuid_hex: &str,
) -> &'static str {
    if step1_skipped {
        skipped_lrs_status(host)
    } else {
        let ts = inclusion_ts.unwrap_or_else(now_unix);
        emit_step3(host, 13, identity, source_url, ts, file_uuid_hex)
    }
}

// ---------------------------------------------------------------------
// Step 2 — the fetch (starter; the mutex is released)
// ---------------------------------------------------------------------

/// The result of the step-2 fetch (no publish yet — the publish runs
/// in the publish hold, under the mutex, so two starters for the same
/// sha serialize and the first capture wins, AT-157).
pub enum FetchResult {
    Body {
        bytes: Vec<u8>,
        captured_sec: u64,
        captured_rfc3339: String,
    },
    Failed(FetchError),
}

fn run_step2_fetch(host: &Host, source_url: &str) -> FetchResult {
    let cfg = &host.started.config;
    let fetch_cfg = FetchConfig {
        lrs: cfg.lrs_url.as_ref(),
        allowlist: cfg.egress_allowlist.as_deref(),
        egress_test_loopback_port: cfg.egress_test_loopback_port,
        fetch_timeout_secs: cfg.fetch_timeout_secs.unwrap_or(30),
        max_fetch_bytes: cfg.max_fetch_bytes.unwrap_or(1_048_576),
        max_redirects: cfg.max_redirects.unwrap_or(5),
    };
    match fetch_url(source_url, &fetch_cfg) {
        FetchOutcome::Ok { body, .. } => {
            let captured_sec = now_unix();
            let captured_rfc3339 =
                rfc3339_secs(UNIX_EPOCH + Duration::from_secs(captured_sec));
            FetchResult::Body {
                bytes: body,
                captured_sec,
                captured_rfc3339,
            }
        }
        FetchOutcome::Err(err) => FetchResult::Failed(err),
    }
}

// ---------------------------------------------------------------------
// Step 2b — delivery
// ---------------------------------------------------------------------

/// Decode a 64-char lowercase hex SHA-256 into its 32 bytes.
fn hex_decode_32(hex: &str) -> Option<[u8; 32]> {
    let bytes = hex.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for i in 0..32 {
        let hi = (bytes[2 * i] as char).to_digit(16)? as u8;
        let lo = (bytes[2 * i + 1] as char).to_digit(16)? as u8;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

/// Read a snapshot file through the snapshot dirfd and reassemble its
/// BLOB tiles. `None` when the file cannot be read or the bytes do not
/// reassemble (the caller reports `snapshot_invalid`).
fn read_snapshot_bytes(host: &Host, sha256: &str) -> Option<Vec<u8>> {
    let snap_fd = host.started.dirfds.snapshot_dir?;
    let name = format!("{}.s3a", sha256);
    let fd = openat_child(snap_fd, &name, false).ok()?;
    let st = fstat_fd(fd).ok()?;
    if !st.is_regular {
        close_fd(fd);
        return None;
    }
    let bytes = read_fd_bounded(fd, host.started.config.max_open_bytes + 1).ok()?;
    close_fd(fd);
    if bytes.len() as u64 > host.started.config.max_open_bytes {
        return None;
    }
    let key = hex_decode_32(sha256)?;
    reassemble_blob(&bytes, &key)
}

/// Deliver the bytes of a usable snapshot (reassembled from its BLOB
/// tiles) into `--download-dir`, named by the SHA-256 of the delivered
/// bytes (DD-62). Returns the absolute destination path, or a
/// `DownloadError`.
fn deliver_from_snapshot(host: &Host, sha256: &str) -> Result<String, DownloadError> {
    let bytes = read_snapshot_bytes(host, sha256).ok_or(DownloadError::SnapshotInvalid)?;
    let name = sha256_hex(&bytes);
    deliver_bytes(host, &bytes, &name)
}

/// Deliver the bytes read from a validated drive-path fd into
/// `--download-dir`, hashing as written. The grow hook pauses after
/// the first read chunk; a size change is `source_changed`.
fn deliver_from_drive(host: &Host, drive: &DriveFd) -> Result<String, DownloadError> {
    let started = &host.started;
    let max_open = started.config.max_open_bytes;
    let Some(download_dir) = started.dirfds.download_dir else {
        return Err(DownloadError::WriteFailed);
    };
    let pid = std::process::id();
    let temp_name = format!(
        "s3a-kite-tmp-{}-download-{}",
        pid,
        crate::consent::random_suffix()
    );
    let temp_fd =
        openat_child_rw(download_dir, &temp_name, true).map_err(|_| DownloadError::WriteFailed)?;
    // Hold LOCK_EX on the temp for its whole life (DD-122).
    if flock_ex_nb(temp_fd).is_err() {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    // The fail_download_write hook fires at the temp write.
    if started
        .config
        .hooks
        .fail_errno("fail_download_write")
        .is_some()
    {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut chunk = [0u8; 65536];
    let mut first_chunk = true;
    let mut copy_err = false;
    loop {
        let n = unsafe { libc::read(drive.fd, chunk.as_mut_ptr() as *mut _, chunk.len()) };
        if n < 0 {
            copy_err = true;
            break;
        }
        if n == 0 {
            break;
        }
        let n = n as usize;
        total += n as u64;
        if total > max_open {
            // More than `--max-open-bytes` arrived: the source changed
            // (grew) mid-copy.
            close_fd(temp_fd);
            let _ = unlink_at(download_dir, &temp_name);
            return Err(DownloadError::SourceChanged);
        }
        let mut written = 0;
        while written < n {
            let w = unsafe {
                libc::write(
                    temp_fd,
                    chunk.as_ptr().add(written) as *const _,
                    n - written,
                )
            };
            if w <= 0 {
                copy_err = true;
                break;
            }
            written += w as usize;
        }
        if copy_err {
            break;
        }
        hasher.update(&chunk[..n]);
        if first_chunk {
            first_chunk = false;
            // The grow hook (§5.2.7 line 523): the HOST extends the
            // already-opened `drive_path` so that more than
            // `--max-open-bytes` arrive during this copy. The fd was
            // opened O_RDWR for this (see resolve_drive_path). The
            // extension is written at the open-time size, so the copy
            // loop's next read picks it up and the `total > max_open`
            // check above fires `source_changed`.
            if host
                .started
                .config
                .hooks
                .grow_at("grow_drive_path_during_copy")
            {
                let need = (max_open + 1).saturating_sub(drive.size);
                if need > 0 {
                    let filler = vec![b'x'; need as usize];
                    let mut off = drive.size as i64;
                    let mut written = 0usize;
                    while written < filler.len() {
                        let w = unsafe {
                            libc::pwrite(
                                drive.fd,
                                filler.as_ptr().add(written) as *const _,
                                filler.len() - written,
                                off,
                            )
                        };
                        if w <= 0 {
                            break;
                        }
                        written += w as usize;
                        off += w as i64;
                    }
                }
            }
        }
    }
    if copy_err {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    // The source must be the same size it was at open time.
    let size_now = match fstat_fd(drive.fd) {
        Ok(st) => st.size,
        Err(_) => {
            close_fd(temp_fd);
            let _ = unlink_at(download_dir, &temp_name);
            return Err(DownloadError::WriteFailed);
        }
    };
    if size_now != drive.size {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::SourceChanged);
    }
    let name = {
        let digest = hasher.finalize();
        digest.iter().map(|b| format!("{:02x}", b)).collect::<String>()
    };
    if unsafe { libc::fsync(temp_fd) } != 0 {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    publish_delivery_temp(host, temp_fd, &temp_name, &name)
}

/// The shared publish tail for a delivery temp: check the destination
/// name, then rename the temp onto it (fsync the dir). The temp fd is
/// closed on every path.
fn publish_delivery_temp(
    host: &Host,
    temp_fd: RawFd,
    temp_name: &str,
    name: &str,
) -> Result<String, DownloadError> {
    let started = &host.started;
    let Some(download_dir) = started.dirfds.download_dir else {
        close_fd(temp_fd);
        return Err(DownloadError::WriteFailed);
    };
    let dest_path = started
        .canon
        .download_dir
        .as_ref()
        .map(|d| d.join(name).to_string_lossy().into_owned())
        .ok_or_else(|| {
            close_fd(temp_fd);
            DownloadError::WriteFailed
        })?;
    // The existing-file check (AT-94/106/216/223): a regular file at
    // the destination whose bytes hash to the name is a success no-op;
    // a symlink/FIFO/non-regular file, an oversize file, or a
    // non-matching hash is `download_conflict`.
    if let Ok(st) = fstatat_nofollow(download_dir, name) {
        let outcome = if !st.is_regular {
            Err(DownloadError::Conflict)
        } else if st.size > started.config.max_open_bytes {
            Err(DownloadError::Conflict)
        } else {
            match openat_child(download_dir, name, false) {
                Ok(fd) => {
                    let existing =
                        read_fd_bounded(fd, started.config.max_open_bytes + 1);
                    close_fd(fd);
                    match existing {
                        Ok(bytes)
                            if bytes.len() as u64
                                <= started.config.max_open_bytes =>
                        {
                            if sha256_hex(&bytes) == name {
                                Ok(dest_path.clone())
                            } else {
                                Err(DownloadError::Conflict)
                            }
                        }
                        _ => Err(DownloadError::Conflict),
                    }
                }
                Err(_) => Err(DownloadError::Conflict),
            }
        };
        match outcome {
            Ok(path) => {
                close_fd(temp_fd);
                let _ = unlink_at(download_dir, temp_name);
                return Ok(path);
            }
            Err(e) => {
                close_fd(temp_fd);
                let _ = unlink_at(download_dir, temp_name);
                return Err(e);
            }
        }
    }
    // No existing file: rename the temp onto the destination name and
    // fsync the download dir.
    let ctemp = match std::ffi::CString::new(temp_name) {
        Ok(c) => c,
        Err(_) => {
            close_fd(temp_fd);
            return Err(DownloadError::WriteFailed);
        }
    };
    let cdest = match std::ffi::CString::new(name) {
        Ok(c) => c,
        Err(_) => {
            close_fd(temp_fd);
            return Err(DownloadError::WriteFailed);
        }
    };
    let renamed = unsafe {
        libc::renameat(download_dir, ctemp.as_ptr(), download_dir, cdest.as_ptr())
    };
    close_fd(temp_fd);
    if renamed != 0 {
        let _ = unlink_at(download_dir, temp_name);
        return Err(DownloadError::WriteFailed);
    }
    let _ = fsync_dir(download_dir);
    Ok(dest_path)
}

/// Deliver already-assembled bytes into `--download-dir` under their
/// SHA-256 name, with the same existing-file check as the copy path.
fn deliver_bytes(host: &Host, bytes: &[u8], name: &str) -> Result<String, DownloadError> {
    let started = &host.started;
    let Some(download_dir) = started.dirfds.download_dir else {
        return Err(DownloadError::WriteFailed);
    };
    let pid = std::process::id();
    let temp_name = format!(
        "s3a-kite-tmp-{}-download-{}",
        pid,
        crate::consent::random_suffix()
    );
    let temp_fd =
        openat_child_rw(download_dir, &temp_name, true).map_err(|_| DownloadError::WriteFailed)?;
    if flock_ex_nb(temp_fd).is_err() {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    if started
        .config
        .hooks
        .fail_errno("fail_download_write")
        .is_some()
    {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    let mut written = 0;
    while written < bytes.len() {
        let w = unsafe {
            libc::write(
                temp_fd,
                bytes.as_ptr().add(written) as *const _,
                bytes.len() - written,
            )
        };
        if w <= 0 {
            close_fd(temp_fd);
            let _ = unlink_at(download_dir, &temp_name);
            return Err(DownloadError::WriteFailed);
        }
        written += w as usize;
    }
    if unsafe { libc::fsync(temp_fd) } != 0 {
        close_fd(temp_fd);
        let _ = unlink_at(download_dir, &temp_name);
        return Err(DownloadError::WriteFailed);
    }
    publish_delivery_temp(host, temp_fd, &temp_name, name)
}

/// What can go wrong in step 2b.
#[derive(Debug)]
pub enum DownloadError {
    /// The recorded snapshot's bytes cannot be reassembled.
    SnapshotInvalid,
    /// The destination name is taken by different bytes, a
    /// non-regular file, or an oversize file (409).
    Conflict,
    /// The on-disk source changed mid-copy.
    SourceChanged,
    /// A temp write / fsync / rename failed (500).
    WriteFailed,
}

// ---------------------------------------------------------------------
// Route entry points
// ---------------------------------------------------------------------

/// The absolute path of a snapshot file for a sha (`--snapshot-dir/
/// {sha}.s3a`).
fn snapshot_path_for(host: &Host, sha256: &str) -> Option<String> {
    host.started.canon.snapshot_dir.as_ref().map(|d| {
        d.join(format!("{}.s3a", sha256))
            .to_string_lossy()
            .into_owned()
    })
}

/// The select route (the step-0/1 hold + step 3; select writes
/// nothing durable).
pub fn run_select(host: &Host, cand: &Candidate) -> Response {
    let identity = &cand.id;
    // The drive_path is opened + validated once per request before
    // step 0, outside the mutex — validation-only for a select; the
    // fd is closed unread.
    let _drive = resolve_drive_path(host, cand);
    // The step-0/1 hold.
    let (file_uuid_hex, snapshot, snapshot_error) = {
        let _guard = host.write_mu.lock().unwrap();
        let hold = match open_hold_select(host, identity) {
            Ok(h) => h,
            Err(HoldError::AddonMissing) => {
                return err(409, "addon_missing", "working archive lacks ADDON_EDU");
            }
            Err(_) => {
                return err(
                    500,
                    "write_failed",
                    "working archive is missing, replaced, or unreadable",
                );
            }
        };
        let file_uuid_hex = hold.file_uuid_hex.clone();
        // A usable snapshot is reported on the select 200 (and a
        // present but failing one is `snapshot_invalid`).
        let (snapshot, snapshot_error) = match &hold.snapshot {
            Step0Snapshot::Usable { sha256 } => {
                (snapshot_path_for(host, sha256), None)
            }
            Step0Snapshot::Invalid => (None, Some("snapshot_invalid".to_string())),
            Step0Snapshot::None => (None, None),
        };
        // An in-flight download for this key: wait for its outcome
        // (the hold and the mutex are released first, so the starter
        // can publish) and report its snapshot.
        let waiter = match &hold.snapshot {
            Step0Snapshot::None => {
                let key = (file_uuid_hex.clone(), identity.clone());
                host.inflight.lock().unwrap().get(&key).cloned()
            }
            _ => None,
        };
        drop(hold);
        drop(_guard);
        let snapshot = match waiter {
            Some(inflight) => {
                let outcome = inflight.wait();
                match outcome.as_ref() {
                    Step2Outcome::Published { sha256 } => {
                        snapshot_path_for(host, sha256)
                    }
                    _ => None,
                }
            }
            None => snapshot,
        };
        (file_uuid_hex, snapshot, snapshot_error)
    };
    // Step 3: a repeat select (already in the in-memory selected set)
    // emits no new statement; a first select inserts and emits. The
    // mutex is released here (emit_step3 takes it itself).
    let lrs_status = {
        let mut selected = host.selected.lock().unwrap();
        if selected.contains(identity) {
            skipped_lrs_status(host)
        } else {
            selected.insert(identity.clone());
            drop(selected);
            let ts = now_unix();
            emit_step3(
                host,
                11,
                identity,
                cand.source_url.as_deref(),
                ts,
                &file_uuid_hex,
            )
        }
    };
    Response::json(
        200,
        serde_json::json!({
            "ok": true,
            "lrs_status": lrs_status,
            "snapshot": snapshot,
            "snapshot_error": snapshot_error,
        })
        .to_string()
        .into_bytes(),
    )
}

/// The download route (steps 0/1/2/2b/3).
pub fn run_download(host: &Host, cand: &Candidate) -> Response {
    let identity = &cand.id;
    // The drive_path is resolved once per request, outside the mutex;
    // the fd is kept for the 2b copy.
    let drive = resolve_drive_path(host, cand);
    let source_url = cand.source_url.clone();
    // ---- Steps 0/1 (one mutex hold). ----
    let guard = host.write_mu.lock().unwrap();
    let mut hold = match open_hold_download(host, identity) {
        Ok(h) => h,
        Err(HoldError::AddonMissing) => {
            return err(409, "addon_missing", "working archive lacks ADDON_EDU");
        }
        Err(HoldError::LockFailed) => {
            return err(
                500,
                "lock_failed",
                "working archive lock could not be taken",
            );
        }
        Err(_) => {
            return err(
                500,
                "write_failed",
                "working archive is missing, replaced, or unreadable",
            );
        }
    };
    let file_uuid_hex = hold.file_uuid_hex.clone();
    // Step 1: the inclusion write (skipped when a matching verb line
    // was already present).
    let inclusion_ts = match step1_write(host, &mut hold, identity, 13) {
        Ok(ts) => ts,
        Err(Step1Error::IndexFailed) => {
            // The tile-10 record stays; steps 2/2b/3 are skipped.
            return err(500, "write_failed", "inclusion index append failed")
                .with_included(true);
        }
        Err(Step1Error::WriteFailed) => {
            return err(500, "write_failed", "inclusion record write failed");
        }
    };
    let step1_skipped = hold.step1_skipped;
    // Step 0's snapshot decision, made in the hold.
    let snapshot_state = hold.snapshot.clone();
    // ---- Step 2 decision (still in the hold for the registry). ----
    // A present-but-invalid recorded snapshot (AT-209): 200 with
    // `snapshot_invalid`, no fetch, no drive_path fallback.
    if let Step0Snapshot::Invalid = snapshot_state {
        drop(guard);
        drop(hold);
        let lrs_status = download_lrs_status(
            host,
            step1_skipped,
            inclusion_ts,
            identity,
            source_url.as_deref(),
            &file_uuid_hex,
        );
        return Response::json(
            200,
            serde_json::json!({
                "ok": true,
                "path": null,
                "snapshot": null,
                "snapshot_error": "snapshot_invalid",
                "fetch_error": null,
                "download_error": "snapshot_invalid",
                "lrs_status": lrs_status,
            })
            .to_string()
            .into_bytes(),
        );
    }
    // A usable snapshot: deliver from it (2b), no fetch.
    if let Step0Snapshot::Usable { sha256 } = &snapshot_state {
        let sha256 = sha256.clone();
        drop(guard);
        drop(hold);
        return download_deliver_snapshot(
            host,
            cand,
            &file_uuid_hex,
            &sha256,
            step1_skipped,
            inclusion_ts,
        );
    }
    // No usable snapshot: the source table.
    if drive.is_some() {
        // Prefer the on-disk source; no fetch, no snapshot.
        let drive = drive.unwrap();
        drop(guard);
        drop(hold);
        return download_deliver_drive(
            host,
            cand,
            &file_uuid_hex,
            drive,
            step1_skipped,
            inclusion_ts,
        );
    }
    let Some(url) = source_url.clone() else {
        // Neither an on-disk source nor a source_url: the inclusion is
        // written, and the download reports `no_source`.
        drop(guard);
        drop(hold);
        let lrs_status = download_lrs_status(
            host,
            step1_skipped,
            inclusion_ts,
            identity,
            None,
            &file_uuid_hex,
        );
        return Response::json(
            200,
            serde_json::json!({
                "ok": true,
                "path": null,
                "snapshot": null,
                "snapshot_error": null,
                "fetch_error": null,
                "download_error": "no_source",
                "lrs_status": lrs_status,
            })
            .to_string()
            .into_bytes(),
        );
    };
    // A fetch is needed: check the registry. A waiter receives the
    // starter's outcome and appends no index line.
    let key = (file_uuid_hex.clone(), identity.clone());
    let entry = host.inflight.lock().unwrap().get(&key).cloned();
    match entry {
        Some(inflight) => {
            drop(guard);
            drop(hold);
            let outcome = inflight.wait();
            download_after_step2(
                host,
                cand,
                &file_uuid_hex,
                outcome,
                step1_skipped,
                inclusion_ts,
            )
        }
        None => {
            // Starter: insert the registry entry in this hold, then
            // release the mutex and fetch.
            let inflight = Arc::new(InFlightFetch::new());
            host.inflight
                .lock()
                .unwrap()
                .insert(key.clone(), inflight.clone());
            drop(guard);
            drop(hold);
            let fetch_result = run_step2_fetch(host, &url);
            // The publish hold: publish the snapshot (or record the
            // fetch failure), append the snapshot index line, remove
            // the registry entry, and wake the waiters.
            let outcome = {
                let _guard = host.write_mu.lock().unwrap();
                publish_step2(
                    host,
                    &file_uuid_hex,
                    identity,
                    &url,
                    fetch_result,
                )
            };
            inflight.finish(host, &key, outcome.clone());
            download_after_step2(
                host,
                cand,
                &file_uuid_hex,
                Arc::new(outcome),
                step1_skipped,
                inclusion_ts,
            )
        }
    }
}

/// The step-2 publish hold: publish the fetched capture as a snapshot
/// (serializing two starters for the same sha so the first capture
/// wins, AT-157) and append the snapshot index line. Returns the
/// outcome to share with waiters.
fn publish_step2(
    host: &Host,
    file_uuid_hex: &str,
    identity: &str,
    source_url: &str,
    fetch_result: FetchResult,
) -> Step2Outcome {
    let FetchResult::Body {
        bytes,
        captured_sec,
        captured_rfc3339,
    } = fetch_result
    else {
        let FetchResult::Failed(err) = fetch_result else {
            unreachable!()
        };
        return Step2Outcome::FetchFailed { fetch_error: err };
    };
    let Some(snap_fd) = host.started.dirfds.snapshot_dir else {
        return Step2Outcome::WriteFailed;
    };
    match publish_snapshot(
        snap_fd,
        &bytes,
        source_url,
        &captured_rfc3339,
        host.started.config.max_open_bytes,
        &host.started.config.hooks,
    ) {
        PublishOutcome::Published { sha256 } => {
            // §5.5 (SPEC line 725): a DD-152 re-fetch hashing to the
            // recorded sha appends NO new index line — "the existing
            // index line stands and no new line is required." Only a
            // different sha earns a new line.
            if let Some(fd) = data_dirfd(host) {
                if let Ok(index) = read_index(fd) {
                    if last_snapshot_sha(&index, file_uuid_hex, identity)
                        == Some(sha256.clone())
                    {
                        return Step2Outcome::Published { sha256 };
                    }
                }
            }
            let line = IndexLine {
                paper_identity: identity.to_string(),
                verb_id: 13,
                object_hash: crc32c::crc32c(identity.as_bytes()),
                timestamp_sec: captured_sec,
                working_archive: host
                    .started
                    .canon
                    .working_archive
                    .as_ref()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                file_uuid: file_uuid_hex.to_string(),
                snapshot_sha256: Some(sha256.clone()),
            };
            match data_dirfd(host) {
                Some(fd) => {
                    match append_index(
                        fd,
                        &line,
                        "fail_snapshot_index_append",
                        &host.started.config.hooks,
                    ) {
                        Ok(()) => Step2Outcome::Published { sha256 },
                        // AT-213: the snapshot file is left for later
                        // reuse; the request is 500 `write_failed`.
                        Err(_) => Step2Outcome::WriteFailed,
                    }
                }
                None => Step2Outcome::WriteFailed,
            }
        }
        PublishOutcome::Conflict => Step2Outcome::SnapshotConflict,
        PublishOutcome::TooLarge => Step2Outcome::FetchFailed {
            fetch_error: "snapshot_too_large",
        },
        PublishOutcome::WriteFailed => Step2Outcome::WriteFailed,
    }
}

/// Continue a download after step 2 (as a starter or a waiter):
/// deliver from the published snapshot, or report the fetch outcome.
fn download_after_step2(
    host: &Host,
    cand: &Candidate,
    file_uuid_hex: &str,
    outcome: Arc<Step2Outcome>,
    step1_skipped: bool,
    inclusion_ts: Option<u64>,
) -> Response {
    let identity = &cand.id;
    let source_url = cand.source_url.clone();
    match outcome.as_ref() {
        Step2Outcome::Published { sha256 } => download_deliver_snapshot(
            host,
            cand,
            file_uuid_hex,
            sha256,
            step1_skipped,
            inclusion_ts,
        ),
        Step2Outcome::FetchFailed { fetch_error } => {
            // A fetch failure: 200 with `fetch_error` and
            // `download_error: "fetch_failed"`; the inclusion is kept
            // and step 3 runs (the fetch is retried on a later
            // request, AT-186).
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            Response::json(
                200,
                serde_json::json!({
                    "ok": true,
                    "path": null,
                    "snapshot": null,
                    "snapshot_error": null,
                    "fetch_error": fetch_error,
                    "download_error": "fetch_failed",
                    "lrs_status": lrs_status,
                })
                .to_string()
                .into_bytes(),
            )
        }
        Step2Outcome::SnapshotConflict => {
            // AT-180: a newly fetched capture whose `{sha}.s3a`
            // already existed and failed DD-103 is 409 with
            // `included: true`; step 3 runs.
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            err(
                409,
                "snapshot_conflict",
                "snapshot file already exists and failed reuse checks",
            )
            .with_included(true)
            .with_lrs_status(lrs_status)
        }
        Step2Outcome::WriteFailed => {
            // AT-212/213: a snapshot or index write failed during the
            // publish; step 3 still runs.
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            err(500, "write_failed", "snapshot publish failed")
                .with_included(true)
                .with_lrs_status(lrs_status)
        }
    }
}

/// Step 2b + step 3 for a download delivered from a snapshot.
fn download_deliver_snapshot(
    host: &Host,
    cand: &Candidate,
    file_uuid_hex: &str,
    sha256: &str,
    step1_skipped: bool,
    inclusion_ts: Option<u64>,
) -> Response {
    let identity = &cand.id;
    let source_url = cand.source_url.clone();
    let snapshot_path = snapshot_path_for(host, sha256);
    // Step 2b (mutex): reassemble and deliver.
    let delivered = {
        let _guard = host.write_mu.lock().unwrap();
        deliver_from_snapshot(host, sha256)
    };
    match delivered {
        Ok(path) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            Response::json(
                200,
                serde_json::json!({
                    "ok": true,
                    "path": path,
                    "snapshot": snapshot_path,
                    "snapshot_error": null,
                    "fetch_error": null,
                    "download_error": null,
                    "lrs_status": lrs_status,
                })
                .to_string()
                .into_bytes(),
            )
        }
        Err(DownloadError::SnapshotInvalid) => {
            // The recorded snapshot's bytes cannot be reassembled:
            // 200 with `snapshot_invalid` and no fetch (AT-209).
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            Response::json(
                200,
                serde_json::json!({
                    "ok": true,
                    "path": null,
                    "snapshot": null,
                    "snapshot_error": "snapshot_invalid",
                    "fetch_error": null,
                    "download_error": "snapshot_invalid",
                    "lrs_status": lrs_status,
                })
                .to_string()
                .into_bytes(),
            )
        }
        Err(DownloadError::Conflict) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            err(
                409,
                "download_conflict",
                "destination file conflicts with the delivered bytes",
            )
            .with_included(true)
            .with_lrs_status(lrs_status)
        }
        Err(_) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            err(500, "write_failed", "download delivery failed")
                .with_included(true)
                .with_lrs_status(lrs_status)
        }
    }
}

/// Step 2b + step 3 for a download delivered from an on-disk source.
fn download_deliver_drive(
    host: &Host,
    cand: &Candidate,
    file_uuid_hex: &str,
    drive: DriveFd,
    step1_skipped: bool,
    inclusion_ts: Option<u64>,
) -> Response {
    let identity = &cand.id;
    let source_url = cand.source_url.clone();
    // Step 2b (mutex): copy the drive fd into the download dir.
    let delivered = {
        let _guard = host.write_mu.lock().unwrap();
        deliver_from_drive(host, &drive)
    };
    match delivered {
        Ok(path) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            Response::json(
                200,
                serde_json::json!({
                    "ok": true,
                    "path": path,
                    "snapshot": null,
                    "snapshot_error": null,
                    "fetch_error": null,
                    "download_error": null,
                    "lrs_status": lrs_status,
                })
                .to_string()
                .into_bytes(),
            )
        }
        Err(DownloadError::SourceChanged) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            Response::json(
                200,
                serde_json::json!({
                    "ok": true,
                    "path": null,
                    "snapshot": null,
                    "snapshot_error": null,
                    "fetch_error": null,
                    "download_error": "source_changed",
                    "lrs_status": lrs_status,
                })
                .to_string()
                .into_bytes(),
            )
        }
        Err(DownloadError::Conflict) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            err(
                409,
                "download_conflict",
                "destination file conflicts with the delivered bytes",
            )
            .with_included(true)
            .with_lrs_status(lrs_status)
        }
        Err(_) => {
            let lrs_status = download_lrs_status(
                host,
                step1_skipped,
                inclusion_ts,
                identity,
                source_url.as_deref(),
                file_uuid_hex,
            );
            err(500, "write_failed", "download delivery failed")
                .with_included(true)
                .with_lrs_status(lrs_status)
        }
    }
}

/// The deselect route: `already_included` (index lookup for a `saved`
/// line) → `not_selected` (the in-memory set) → remove from the set →
/// step 3 (verb `removed`). A successful deselect writes no tile-10
/// and no inclusion-index line.
pub fn run_deselect(host: &Host, cand: &Candidate) -> Response {
    let identity = &cand.id;
    let _guard = host.write_mu.lock().unwrap();
    // The index lookup needs the current working archive's file_uuid.
    let file_uuid_hex = {
        let fd = match open_archive_fd(host) {
            Ok(fd) => fd,
            Err(_) => {
                return err(
                    500,
                    "write_failed",
                    "working archive is missing, replaced, or unreadable",
                );
            }
        };
        let uuid = read_header(fd).map(|(u, _)| u);
        close_fd(fd);
        match uuid {
            Some(u) => u,
            None => {
                return err(
                    500,
                    "write_failed",
                    "working archive is missing, replaced, or unreadable",
                );
            }
        }
    };
    let index = read_index_in(host);
    // A `saved` line for this identity in this working archive means
    // the paper is already included (AT-235).
    if find_last(&index, &file_uuid_hex, identity, 13).is_some() {
        return err(
            409,
            "already_included",
            "paper is already included in the corpus",
        );
    }
    let mut selected = host.selected.lock().unwrap();
    if !selected.contains(identity) {
        return err(404, "not_selected", "paper is not selected");
    }
    selected.remove(identity);
    drop(selected);
    drop(_guard);
    let lrs_status = emit_step3(
        host,
        14,
        identity,
        cand.source_url.as_deref(),
        now_unix(),
        &file_uuid_hex,
    );
    Response::json(
        200,
        serde_json::json!({
            "ok": true,
            "lrs_status": lrs_status,
        })
        .to_string()
        .into_bytes(),
    )
}

/// A `Response` with an added `included` / `lrs_status` field (for
/// 500/409 bodies that report whether the inclusion record is present
/// and what the LRS step did).
pub trait ResponseExt {
    fn with_included(self, included: bool) -> Response;
    fn with_lrs_status(self, lrs_status: &'static str) -> Response;
}

impl ResponseExt for Response {
    fn with_included(self, included: bool) -> Response {
        add_field(self, "included", serde_json::json!(included))
    }
    fn with_lrs_status(self, lrs_status: &'static str) -> Response {
        add_field(self, "lrs_status", serde_json::json!(lrs_status))
    }
}

/// Add a top-level field to a JSON `Response` body.
fn add_field(resp: Response, key: &str, value: serde_json::Value) -> Response {
    let mut body: serde_json::Value =
        serde_json::from_slice(&resp.body).unwrap_or(serde_json::json!({}));
    if let serde_json::Value::Object(map) = &mut body {
        map.insert(key.to_string(), value);
    }
    Response::json(resp.status, serde_json::to_vec(&body).unwrap_or_default())
}
