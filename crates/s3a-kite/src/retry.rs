//! The background LRS retry worker (DD-51/DD-66, AT-236, AT-91): one
//! detached thread per Scholar host with `--lrs-url`. It scans
//! `--data-dir/lrs-retry/` every 500 ms for due `{id}.json` files
//! (last attempt ≥ 10 s ago, attempts < 3 in the current episode),
//! claims them under the write mutex (rename to `{id}.claimed`),
//! sends with the DD-144 register/re-check order, and resolves the
//! claim (sent → delete; retryable → rename back with attempts+1;
//! permanent → rename to `{id}.permanent`; aborted/skipped → leave
//! `.claimed`). A new episode starts at process start and after each
//! later successful LRS emit (the worker resets attempts for files
//! whose episode is stale). The worker never creates `lrs-retry/`; if
//! the directory is gone it does nothing.
//!
//! The retry state (`retry_state`, keyed by statement id) is only
//! touched while holding the write mutex — both `emit_step3` (which
//! registers a file it just queued, attempts=1) and the worker (which
//! checks due-ness and claims) serialize on it. A file found at
//! startup with no state entry starts a fresh episode at attempts=0.

use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::lrs::{
    claim_retry_file, list_retry_files, post_statement, read_retry_file,
    resolve_claimed_permanent, resolve_claimed_retryable, resolve_claimed_sent,
    write_failure_record, PostOutcome,
};
use crate::server::Host;

/// One statement's retry state in the current episode.
#[derive(Debug, Clone)]
pub struct RetryEntry {
    /// The episode this entry belongs to (`Host::retry_episode`).
    pub episode: u64,
    /// Tries completed in this episode (try 1 in the request counts).
    pub attempts: u32,
    /// When the last try ran in this episode (`None` = never).
    pub last_attempt: Option<Instant>,
}

/// The backoff between tries of one statement (DD-51, Q-09).
pub const RETRY_BACKOFF: Duration = Duration::from_secs(10);

/// The maximum tries per statement per episode (DD-51, Q-09).
pub const RETRY_MAX_TRIES: u32 = 3;

/// Spawn the background retry worker for a Scholar host with an LRS
/// configured. The thread is detached; it exits when the host shuts
/// down (or the process exits).
pub fn spawn_retry_worker(host: &Arc<Host>) -> std::thread::JoinHandle<()> {
    let host = Arc::clone(host);
    std::thread::spawn(move || retry_loop(&host))
}

fn retry_loop(host: &Host) {
    loop {
        if host.shutdown.load(Ordering::SeqCst) {
            return;
        }
        if host.fail_closed.load(Ordering::SeqCst) {
            return;
        }
        retry_scan(host);
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// One scan of the retry queue: send every due `{id}.json` file.
fn retry_scan(host: &Host) {
    let Some(data_fd) = host.data_dirfd() else {
        return;
    };
    let Some(lrs) = host.started.config.lrs_url.clone() else {
        return;
    };
    let Some(credentials) = host.started.lrs_credentials.clone() else {
        return;
    };
    let names = list_retry_files(data_fd);
    for name in names {
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let id = id.to_string();
        if host.shutdown.load(Ordering::SeqCst) || host.fail_closed.load(Ordering::SeqCst) {
            return;
        }
        retry_one(host, data_fd, &lrs, &credentials, &id);
    }
}

/// Try one queued statement (if due): claim, send, resolve.
fn retry_one(
    host: &Host,
    data_fd: std::os::unix::io::RawFd,
    lrs: &crate::flags::LrsUrl,
    credentials: &(String, String),
    id: &str,
) {
    // Claim + register + re-check under the write mutex (DD-51/DD-144).
    // The due check runs here too, so a file written by `emit_step3`
    // (which registers attempts=1 in the same hold as the file write)
    // is never claimed before its state entry exists.
    let (token, send) = {
        let _guard = host.write_mu.lock().unwrap();
        let episode = host.retry_episode.load(Ordering::SeqCst);
        let due = {
            let mut state = host.retry_state.lock().unwrap();
            let entry = state.entry(id.to_string()).or_insert(RetryEntry {
                episode,
                attempts: 0,
                last_attempt: None,
            });
            if entry.episode != episode {
                entry.episode = episode;
                entry.attempts = 0;
                entry.last_attempt = None;
            }
            if entry.attempts >= RETRY_MAX_TRIES {
                false
            } else {
                match entry.last_attempt {
                    None => true,
                    Some(t) => t.elapsed() >= RETRY_BACKOFF,
                }
            }
        };
        if !due {
            return;
        }
        if claim_retry_file(data_fd, id).is_err() {
            // Already claimed or gone.
            return;
        }
        let token = host.latch.register_token();
        let consent_on = host.consent.lock().unwrap().on;
        let inhibit_active = host.latch.inhibit_active();
        (token, consent_on && !inhibit_active)
    };
    if !send {
        // Aborted/skipped replay: leave the `.claimed` file for the
        // revoke delete (no rename). Drop the state entry — the file
        // is `.claimed` now, so it will not be scanned again.
        let _guard = host.write_mu.lock().unwrap();
        host.retry_state.lock().unwrap().remove(id);
        return;
    }
    // Read the claimed statement bytes (the file is `.claimed` now).
    let body = match read_retry_file(
        data_fd,
        &format!("{id}.claimed"),
        host.started.config.max_open_bytes,
    ) {
        Ok(b) => b,
        Err(_) => {
            let _guard = host.write_mu.lock().unwrap();
            host.retry_state.lock().unwrap().remove(id);
            return;
        }
    };
    let statement: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            // A corrupt queue file is a permanent failure: keep it out
            // of the retry loop as `.permanent`.
            let _guard = host.write_mu.lock().unwrap();
            let _ = resolve_claimed_permanent(data_fd, id);
            host.retry_state.lock().unwrap().remove(id);
            return;
        }
    };
    let (outcome, detail) = post_statement(
        lrs,
        &lrs.resolved,
        credentials,
        &statement,
        host.started.config.lrs_timeout_secs.unwrap_or(30),
        host.started.config.max_open_bytes,
        Some((&host.latch, token)),
    );
    // Resolve the claim under the write mutex.
    let _guard = host.write_mu.lock().unwrap();
    match outcome {
        PostOutcome::Sent => {
            let _ = resolve_claimed_sent(data_fd, id);
            host.retry_state.lock().unwrap().remove(id);
            // A later successful emit starts a new episode (a
            // previously-exhausted file gets another set of tries).
            host.bump_retry_episode();
        }
        PostOutcome::Retryable | PostOutcome::NoResponse => {
            let _ = resolve_claimed_retryable(data_fd, id);
            let mut state = host.retry_state.lock().unwrap();
            let episode = host.retry_episode.load(Ordering::SeqCst);
            let entry = state.entry(id.to_string()).or_insert(RetryEntry {
                episode,
                attempts: 0,
                last_attempt: None,
            });
            entry.attempts += 1;
            entry.last_attempt = Some(Instant::now());
            if entry.attempts >= RETRY_MAX_TRIES {
                if let Some(detail) = detail {
                    let _ = write_failure_record(
                        data_fd,
                        id,
                        entry.attempts,
                        &detail,
                    );
                }
            }
        }
        PostOutcome::Permanent => {
            let _ = resolve_claimed_permanent(data_fd, id);
            host.retry_state.lock().unwrap().remove(id);
        }
        PostOutcome::Aborted => {
            // Leave the `.claimed` file for the revoke delete.
            host.retry_state.lock().unwrap().remove(id);
        }
    }
}
