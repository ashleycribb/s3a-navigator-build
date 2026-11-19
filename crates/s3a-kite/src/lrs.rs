//! The LRS step-3 client (§5.5, DD-65/DD-131/DD-134/DD-137): statement
//! build, POST, outcome classification, and the retry queue
//! (`--data-dir/lrs-retry/`). Retries and replay are Step 11; Step 10
//! writes the retry file for a queued or permanent outcome and reports
//! `lrs_status`.

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::os::unix::io::RawFd;
use std::time::{Duration, Instant};

use s3a_pack_scholar::xapi::{
    resolve_verb_by_id, EXT_CONTENT_SHA256, EXT_PAPER_ID, EXT_SOURCE_URL,
};

use crate::fetch::Conn;
use crate::flags::LrsUrl;
use crate::paths::{close_fd, fsync_dir, openat_child_rw, unlink_at};
use crate::timefmt::rfc3339_secs;

/// The `actor_hash` for an inclusion record: the FNV-1a hash of the
/// researcher id (the same `hash_iri` the pin uses for actor IRIs).
pub fn actor_hash(researcher_id: &str) -> u64 {
    s3a_pack_scholar::xapi::hash_iri(researcher_id)
}

/// A fresh random 128-bit session UUID for one inclusion record (SPEC
/// fixes no value; zeros would falsely link distinct records as one
/// session).
pub fn new_session_uuid() -> [u64; 2] {
    let id = uuid::Uuid::new_v4();
    let bytes = id.as_bytes();
    let hi = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    let lo = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
    [hi, lo]
}

/// A fresh statement id (UUID v4, generated at first emit; reused on
/// retry from the retry file, Step 11).
pub fn new_statement_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// The fixed actor-homepage prefix (DD-157): lab mode, or study mode
/// with a study id. `--lrs-actor-homepage` is not a flag (Step 9
/// rejects it at startup).
pub fn actor_homepage(study_id: Option<&str>) -> String {
    match study_id {
        Some(id) => format!("https://34.74.163.88.sslip.io/scholar/study/{id}"),
        None => "https://34.74.163.88.sslip.io/scholar/lab".to_string(),
    }
}

/// Percent-encode a paper identity for the fallback `object.id`
/// (unreserved characters pass through; everything else is `%XX`).
pub fn percent_encode_identity(identity: &str) -> String {
    let mut out = String::new();
    for b in identity.bytes() {
        let c = b as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
            out.push(c);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The statements request-target for a configured LRS URL: `/xapi/statements`
/// whether the URL ends with `/xapi` or is a bare origin (DD-65 — no
/// doubled `/xapi` path).
pub fn statements_target(lrs: &LrsUrl) -> String {
    let _ = lrs;
    "/xapi/statements".to_string()
}

/// Build one xAPI 1.0.3 statement (DD-65). `verb_id` is resolved through
/// `s3a-pack-scholar` `xapi.rs` (Q-07 — no invented IRI). `object.id` is
/// the candidate `source_url` when present, else the fallback IRI +
/// percent-encoded identity. `EXT_CONTENT_SHA256` is present iff the
/// index has a non-null `snapshot_sha256` for the key at step 3 (DD-137).
/// `timestamp` is the RFC3339 seconds UTC of the inclusion write.
#[allow(clippy::too_many_arguments)]
pub fn build_statement(
    statement_id: &str,
    researcher_id: &str,
    study_id: Option<&str>,
    verb_id: u32,
    identity: &str,
    source_url: Option<&str>,
    content_sha256: Option<&str>,
    timestamp_sec: u64,
) -> serde_json::Value {
    let verb_iri = resolve_verb_by_id(verb_id)
        .map(|s| s.iri)
        .unwrap_or("https://ashecribb.dev/xapi/verbs/opened");
    let object_id = match source_url {
        Some(url) => url.to_string(),
        None => format!(
            "https://ashecribb.dev/xapi/activities/paper/{}",
            percent_encode_identity(identity)
        ),
    };
    let mut extensions = serde_json::Map::new();
    extensions.insert(
        EXT_PAPER_ID.to_string(),
        serde_json::Value::String(identity.to_string()),
    );
    if let Some(url) = source_url {
        extensions.insert(
            EXT_SOURCE_URL.to_string(),
            serde_json::Value::String(url.to_string()),
        );
    }
    if let Some(sha) = content_sha256 {
        extensions.insert(
            EXT_CONTENT_SHA256.to_string(),
            serde_json::Value::String(sha.to_string()),
        );
    }
    let ts = rfc3339_secs(
        std::time::UNIX_EPOCH + Duration::from_secs(timestamp_sec),
    );
    serde_json::json!({
        "id": statement_id,
        "actor": {
            "objectType": "Agent",
            "account": {
                "homePage": actor_homepage(study_id),
                "name": researcher_id,
            }
        },
        "verb": { "id": verb_iri },
        "object": {
            "id": object_id,
            "definition": { "extensions": extensions },
        },
        "timestamp": ts,
    })
}

/// Parse an LRS credentials file (AT-182, s3a-lrs-server format): lines
/// `user:password[:scopes[:archive]]`; `#` comments and blank lines
/// ignored; the first credential line wins. Each line is split on every
/// `:`; a password cannot contain `:`. A line with fewer than two fields,
/// or an empty username/password, is an error naming the file. A file
/// with no credential line (after comments/blanks) is an error naming
/// the file. Returns `(username, password)`.
pub fn parse_credentials_file(path: &std::path::Path) -> Result<(String, String), String> {
    let display = path.display().to_string();
    let bytes = std::fs::read(path)
        .map_err(|e| format!("{display}: cannot read LRS credentials file: {e}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| format!("{display}: LRS credentials file is not UTF-8"))?;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() < 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(format!(
                "{display}: invalid LRS credential line (expected user:password[:scopes[:archive]])"
            ));
        }
        return Ok((parts[0].to_string(), parts[1].to_string()));
    }
    Err(format!(
        "{display}: LRS credentials file has no credential line"
    ))
}

/// The outcome of one LRS POST attempt (DD-134), before the caller
/// records it under the mutex.
pub enum PostOutcome {
    /// 2xx or 409.
    Sent,
    /// 408, 429, or 5xx (retryable).
    Retryable,
    /// Any other 4xx, a 3xx (outside the DD-134 classes — treated as
    /// permanent, fail-closed), or a TLS certificate failure.
    Permanent,
    /// No HTTP response (timeout, refused, DNS failure, I/O, reset).
    NoResponse,
    /// The POST was aborted by a consent revoke (DD-144): the
    /// cancellation token changed mid-flight. The caller treats this
    /// like a skipped emit (the revoke is in flight, so the inhibit is
    /// active).
    Aborted,
}

/// The error detail for a failed LRS attempt (for the DD-156 failure
/// record, AT-236). Only the code and message are recorded — never the
/// statement body, the researcher id, paper identity, or source URL.
#[derive(Debug, Clone)]
pub struct LrsAttemptError {
    /// A stable code: the HTTP status (`"503"`, `"400"`), or
    /// `"no_response"`, `"tls_certificate"`, `"header_too_large"`,
    /// `"aborted"`.
    pub code: String,
    /// A human-readable message (no researcher data).
    pub message: String,
}

fn attempt_error(code: &str, message: String) -> LrsAttemptError {
    LrsAttemptError {
        code: code.to_string(),
        message,
    }
}

/// POST one statement to the LRS and classify the outcome (DD-134).
/// Only the status line is needed: after it is read, an oversized
/// remainder follows the status class and is not read further (AT-191).
/// The header block is bounded by `--max-open-bytes`; if it is exceeded
/// before a status line is read, the attempt is `Permanent`.
/// `ips` are the LRS host's startup-resolved addresses. `cancel` is the
/// DD-144 cancellation token (`&Latch` + the generation registered
/// before the POST); when it is `Some`, the connect/read loops use
/// short socket timeouts (250 ms reads, 1 s connect slices) and poll
/// the token, aborting as `Aborted` when the token changes (AT-156).
/// Returns the outcome plus the error detail for a failed attempt
/// (`None` when `Sent`).
#[allow(clippy::too_many_arguments)]
pub fn post_statement(
    lrs: &LrsUrl,
    ips: &[IpAddr],
    credentials: &(String, String),
    statement: &serde_json::Value,
    timeout_secs: u64,
    max_open_bytes: u64,
    cancel: Option<(&crate::latch::Latch, u64)>,
) -> (PostOutcome, Option<LrsAttemptError>) {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs.max(1));
    let port = lrs.port.unwrap_or(if lrs.https { 443 } else { 80 });
    let body = serde_json::to_vec(statement).unwrap_or_default();
    let target = statements_target(lrs);
    let auth = base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        format!("{}:{}", credentials.0, credentials.1),
    );
    let host_header = if (lrs.https && port == 443) || (!lrs.https && port == 80) {
        lrs.host.clone()
    } else if lrs.host.contains(':') {
        format!("[{}]:{}", lrs.host, port)
    } else {
        format!("{}:{}", lrs.host, port)
    };
    let request = format!(
        "POST {target} HTTP/1.1\r\nHost: {host_header}\r\nContent-Type: application/json\r\nX-Experience-API-Version: 1.0.3\r\nAuthorization: Basic {auth}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    // Connect (TCP; TLS for https). A TLS certificate failure is
    // distinguished from a plain connect failure below.
    let mut conn = match connect_lrs(&lrs.host, port, ips, lrs.https, deadline, cancel) {
        Ok(c) => c,
        Err((outcome, detail)) => return (outcome, Some(detail)),
    };
    if cancelled(cancel) {
        return (
            PostOutcome::Aborted,
            Some(attempt_error(
                "aborted",
                "LRS POST aborted by a consent revoke".to_string(),
            )),
        );
    }
    // The request is a single small buffer; the kernel accepts it
    // into the send buffer immediately. The pre-write cancel check
    // above (and the read-loop polling below) cover the revoke case;
    // the write itself keeps the full deadline.
    conn.set_deadline(deadline);
    let mut request_bytes = request.into_bytes();
    request_bytes.extend_from_slice(&body);
    if let Err(e) = conn.write_all(&request_bytes).and_then(|_| conn.flush()) {
        if cancelled(cancel) {
            return (
                PostOutcome::Aborted,
                Some(attempt_error(
                    "aborted",
                    "LRS POST aborted by a consent revoke".to_string(),
                )),
            );
        }
        let (outcome, detail) = io_post_error(e, lrs.https, deadline);
        return (outcome, Some(detail));
    }
    // Read the status line (bounded by --max-open-bytes). Only the
    // status matters; the body is never read.
    let status = read_lrs_status(&mut conn, deadline, max_open_bytes, cancel);
    match status {
        LrsRead::Status(code) => {
            let outcome = classify_status(code);
            match outcome {
                PostOutcome::Sent => (PostOutcome::Sent, None),
                _ => (
                    outcome,
                    Some(attempt_error(
                        &code.to_string(),
                        format!("LRS returned HTTP {code}"),
                    )),
                ),
            }
        }
        LrsRead::HeaderTooLargeBeforeStatus => (
            PostOutcome::Permanent,
            Some(attempt_error(
                "header_too_large",
                "LRS header block exceeded --max-open-bytes before a status line was read"
                    .to_string(),
            )),
        ),
        LrsRead::NoResponse => (
            PostOutcome::NoResponse,
            Some(attempt_error(
                "no_response",
                "no HTTP response from the LRS (timeout, refused, DNS, or I/O)".to_string(),
            )),
        ),
        LrsRead::TlsFailure => (
            PostOutcome::Permanent,
            Some(attempt_error(
                "tls_certificate",
                "LRS TLS certificate validation failed".to_string(),
            )),
        ),
        LrsRead::Aborted => (
            PostOutcome::Aborted,
            Some(attempt_error(
                "aborted",
                "LRS POST aborted by a consent revoke".to_string(),
            )),
        ),
    }
}

/// Has the DD-144 cancellation token changed since registration?
fn cancelled(cancel: Option<(&crate::latch::Latch, u64)>) -> bool {
    match cancel {
        Some((latch, registered)) => latch.token_changed(registered),
        None => false,
    }
}

/// Connect to the LRS host (TCP, then a TLS handshake for https).
/// Returns the connection, or the outcome + detail for a connect
/// failure: `NoResponse` for refused/DNS/timeout, `Permanent` for a
/// TLS certificate failure, `Aborted` when the cancellation token
/// changed mid-connect. When a cancel token is set, the connect is
/// sliced into 1 s pieces so a revoke aborts a stalled connect
/// promptly (AT-156).
#[allow(clippy::too_many_arguments)]
fn connect_lrs(
    host: &str,
    port: u16,
    ips: &[IpAddr],
    https: bool,
    deadline: Instant,
    cancel: Option<(&crate::latch::Latch, u64)>,
) -> Result<Conn, (PostOutcome, LrsAttemptError)> {
    // Resolve when the startup list is empty (a hostname that resolved
    // at startup always has entries; an IP literal has one).
    let owned: Vec<IpAddr>;
    let ips: &[IpAddr] = if ips.is_empty() {
        match (host, port).to_socket_addrs() {
            Ok(addrs) => {
                owned = addrs.map(|s| s.ip()).collect();
                &owned
            }
            Err(_) => {
                return Err((
                    PostOutcome::NoResponse,
                    attempt_error(
                        "no_response",
                        format!("cannot resolve LRS host {host}"),
                    ),
                ))
            }
        }
    } else {
        ips
    };
    for ip in ips {
        if cancelled(cancel) {
            return Err((
                PostOutcome::Aborted,
                attempt_error("aborted", "LRS POST aborted by a consent revoke".to_string()),
            ));
        }
        let mut remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err((
                PostOutcome::NoResponse,
                attempt_error("no_response", "LRS connect timed out".to_string()),
            ));
        }
        // One connect attempt per slice (1 s when a cancel token is
        // set, else the full remaining time); a slice timeout retries
        // until the overall deadline.
        loop {
            if cancelled(cancel) {
                return Err((
                    PostOutcome::Aborted,
                    attempt_error("aborted", "LRS POST aborted by a consent revoke".to_string()),
                ));
            }
            let slice = if cancel.is_some() {
                remaining.min(Duration::from_secs(1))
            } else {
                remaining
            };
            let sock = std::net::SocketAddr::new(*ip, port);
            match TcpStream::connect_timeout(&sock, slice) {
                Ok(tcp) => {
                    let _ = tcp.set_nodelay(true);
                    if !https {
                        return Ok(Conn::Plain(tcp));
                    }
                    let name = match rustls::pki_types::ServerName::try_from(host.to_string()) {
                        Ok(n) => n,
                        Err(_) => {
                            return Err((
                                PostOutcome::Permanent,
                                attempt_error(
                                    "tls_certificate",
                                    format!("invalid LRS TLS server name {host}"),
                                ),
                            ))
                        }
                    };
                    let config = crate::fetch::tls_client_config();
                    match rustls::ClientConnection::new(config, name) {
                        Ok(client) => {
                            let mut stream =
                                rustls::StreamOwned::new(client, tcp);
                            // Force the handshake now, so a certificate
                            // failure is distinguished from a later I/O
                            // failure. A timeout here is NoResponse.
                            let remaining =
                                deadline.saturating_duration_since(Instant::now());
                            let _ = stream
                                .sock
                                .set_read_timeout(Some(remaining));
                            let _ = stream
                                .sock
                                .set_write_timeout(Some(remaining));
                            return handshake(&mut stream, deadline, cancel).map(|()| {
                                Conn::Tls(Box::new(stream))
                            });
                        }
                        Err(_) => {
                            return Err((
                                PostOutcome::Permanent,
                                attempt_error(
                                    "tls_certificate",
                                    "LRS TLS client setup failed".to_string(),
                                ),
                            ))
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                    if cancel.is_none() {
                        return Err((
                            PostOutcome::NoResponse,
                            attempt_error("no_response", "LRS connect timed out".to_string()),
                        ));
                    }
                    remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err((
                            PostOutcome::NoResponse,
                            attempt_error("no_response", "LRS connect timed out".to_string()),
                        ));
                    }
                    continue;
                }
                Err(_) => break, // refused/reset: try the next address
            }
        }
    }
    if cancelled(cancel) {
        Err((
            PostOutcome::Aborted,
            attempt_error("aborted", "LRS POST aborted by a consent revoke".to_string()),
        ))
    } else {
        Err((
            PostOutcome::NoResponse,
            attempt_error(
                "no_response",
                "no HTTP response from the LRS (connection refused)".to_string(),
            ),
        ))
    }
}

/// Run the TLS handshake to completion (the connection is usable after
/// this returns). A certificate failure is `Permanent`; a timeout is
/// `NoResponse`; any other I/O failure is `NoResponse`. When a cancel
/// token is set, the handshake polls the token every 250 ms and aborts
/// when it changes (AT-156).
fn handshake(
    stream: &mut rustls::StreamOwned<rustls::ClientConnection, TcpStream>,
    deadline: Instant,
    cancel: Option<(&crate::latch::Latch, u64)>,
) -> Result<(), (PostOutcome, LrsAttemptError)> {
    loop {
        if !stream.conn.is_handshaking() {
            return Ok(());
        }
        if cancelled(cancel) {
            return Err((
                PostOutcome::Aborted,
                attempt_error("aborted", "LRS POST aborted by a consent revoke".to_string()),
            ));
        }
        if cancel.is_some() {
            let slice = Duration::from_millis(250);
            let _ = stream.sock.set_read_timeout(Some(slice));
            let _ = stream.sock.set_write_timeout(Some(slice));
        }
        match stream.conn.complete_io(&mut stream.sock) {
            Ok(_) => continue,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                {
                    if cancelled(cancel) {
                        return Err((
                            PostOutcome::Aborted,
                            attempt_error(
                                "aborted",
                                "LRS POST aborted by a consent revoke".to_string(),
                            ),
                        ));
                    }
                    if cancel.is_some() && Instant::now() < deadline {
                        continue;
                    }
                    return Err((
                        PostOutcome::NoResponse,
                        attempt_error("no_response", "LRS TLS handshake timed out".to_string()),
                    ));
                }
                if is_tls_error(&e) {
                    return Err((
                        PostOutcome::Permanent,
                        attempt_error(
                            "tls_certificate",
                            "LRS TLS certificate validation failed".to_string(),
                        ),
                    ));
                }
                return Err((
                    PostOutcome::NoResponse,
                    attempt_error(
                        "no_response",
                        format!("LRS TLS handshake failed: {e}"),
                    ),
                ));
            }
        }
    }
}

/// Is this I/O error a TLS error (a rustls error in the source chain)?
/// Used to classify a certificate failure as permanent (DD-134).
fn is_tls_error(e: &std::io::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> =
        e.get_ref().map(|r| r as &(dyn std::error::Error + 'static));
    while let Some(err) = source {
        if err.is::<rustls::Error>() {
            return true;
        }
        // rustls wraps webpki errors; a webpki error in the chain is
        // also a certificate failure.
        if format!("{err:?}").contains("webpki") {
            return true;
        }
        source = err.source();
    }
    false
}

/// Map a post-write I/O error to an outcome + detail: a timeout is
/// `NoResponse`; a TLS error is `Permanent`; anything else is
/// `NoResponse`.
fn io_post_error(
    e: std::io::Error,
    https: bool,
    deadline: Instant,
) -> (PostOutcome, LrsAttemptError) {
    if e.kind() == std::io::ErrorKind::WouldBlock
        || e.kind() == std::io::ErrorKind::TimedOut
        || Instant::now() >= deadline
    {
        return (
            PostOutcome::NoResponse,
            attempt_error("no_response", "LRS POST write timed out".to_string()),
        );
    }
    if https && is_tls_error(&e) {
        return (
            PostOutcome::Permanent,
            attempt_error("tls_certificate", "LRS TLS certificate validation failed".to_string()),
        );
    }
    (
        PostOutcome::NoResponse,
        attempt_error("no_response", format!("LRS POST write failed: {e}")),
    )
}

enum LrsRead {
    Status(u16),
    /// The header block exceeded the cap before a status line was read.
    HeaderTooLargeBeforeStatus,
    /// No HTTP response (timeout / refused / reset mid-read).
    NoResponse,
    /// A TLS failure while reading.
    TlsFailure,
    /// The cancellation token changed mid-read (AT-156).
    Aborted,
}

/// Read the LRS response status line (and the remainder of the header
/// block, up to the cap). The body is never read. If the header block
/// exceeds the cap after the status line was read, the status class
/// still decides (the remainder is not read). EOF after a complete
/// status line also leaves the status class to decide. When a cancel
/// token is set, reads use 250 ms slices and poll the token, aborting
/// as `Aborted` when it changes (AT-156).
fn read_lrs_status(
    conn: &mut Conn,
    deadline: Instant,
    max_open_bytes: u64,
    cancel: Option<(&crate::latch::Latch, u64)>,
) -> LrsRead {
    let mut buf: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    // Phase 1: read until the first CRLF (the status line).
    let status: u16;
    let mut total: u64 = 0;
    loop {
        if let Some(idx) = find_crlf(&buf) {
            let line = String::from_utf8_lossy(&buf[..idx]).into_owned();
            status = match parse_status_line(&line) {
                Some(s) => s,
                None => return LrsRead::NoResponse,
            };
            total += (idx + 2) as u64;
            buf.drain(..idx + 2);
            // `buf` may already hold header bytes read past the status
            // line in the same chunk; count them too.
            total += buf.len() as u64;
            break;
        }
        if buf.len() as u64 > max_open_bytes {
            return LrsRead::HeaderTooLargeBeforeStatus;
        }
        if Instant::now() >= deadline {
            return LrsRead::NoResponse;
        }
        if cancelled(cancel) {
            return LrsRead::Aborted;
        }
        set_read_slice(conn, deadline, cancel);
        match conn.read(&mut chunk) {
            Ok(0) => return LrsRead::NoResponse,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => match read_err(e, cancel, deadline) {
                ReadErr::Again => continue,
                ReadErr::Done(r) => return r,
            },
        }
    }
    // Phase 2: read the header block until the blank line, capped.
    // `buf` already holds bytes read past the status line.
    loop {
        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
            return LrsRead::Status(status);
        }
        if buf.starts_with(b"\r\n") {
            // An empty header block (status line followed by a blank line).
            return LrsRead::Status(status);
        }
        if total > max_open_bytes {
            return LrsRead::Status(status);
        }
        if Instant::now() >= deadline {
            return LrsRead::NoResponse;
        }
        if cancelled(cancel) {
            return LrsRead::Aborted;
        }
        set_read_slice(conn, deadline, cancel);
        match conn.read(&mut chunk) {
            Ok(0) => return LrsRead::Status(status),
            Ok(n) => {
                total += n as u64;
                buf.extend_from_slice(&chunk[..n]);
                if total > max_open_bytes {
                    return LrsRead::Status(status);
                }
            }
            Err(e) => match read_err(e, cancel, deadline) {
                ReadErr::Again => continue,
                ReadErr::Done(r) => return r,
            },
        }
    }
}

/// Set the socket read/write timeouts for one read slice: the full
/// remaining time when no cancel token is set, else a 250 ms slice
/// (so a revoke aborts a stalled read promptly).
fn set_read_slice(
    conn: &mut Conn,
    deadline: Instant,
    cancel: Option<(&crate::latch::Latch, u64)>,
) {
    let slice = if cancel.is_some() {
        Instant::now() + Duration::from_millis(250)
    } else {
        deadline
    };
    conn.set_deadline(slice);
}

/// What a read error means for the read loop.
enum ReadErr {
    /// A 250 ms slice elapsed with no data and no cancel: poll again.
    Again,
    /// The read loop is done with this outcome.
    Done(LrsRead),
}

/// Map a read error while awaiting the LRS response: a timeout is
/// `NoResponse` (or `Again` for a 250 ms slice with a cancel token
/// set); a TLS error is `TlsFailure`; anything else is `NoResponse`.
/// A changed cancellation token is `Aborted`.
fn read_err(
    e: std::io::Error,
    cancel: Option<(&crate::latch::Latch, u64)>,
    deadline: Instant,
) -> ReadErr {
    if e.kind() == std::io::ErrorKind::WouldBlock
        || e.kind() == std::io::ErrorKind::TimedOut
    {
        if cancelled(cancel) {
            return ReadErr::Done(LrsRead::Aborted);
        }
        if cancel.is_some() && Instant::now() < deadline {
            return ReadErr::Again;
        }
        return ReadErr::Done(LrsRead::NoResponse);
    }
    if is_tls_error(&e) {
        return ReadErr::Done(LrsRead::TlsFailure);
    }
    ReadErr::Done(LrsRead::NoResponse)
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

fn parse_status_line(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    let _version = parts.next()?;
    let code = parts.next()?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    code.parse().ok()
}

/// Classify an LRS response status (DD-134). A 3xx is outside the
/// listed classes; it is treated as permanent (fail-closed — the LRS
/// is a configured endpoint and must not redirect).
fn classify_status(code: u16) -> PostOutcome {
    match code {
        200..=299 | 409 => PostOutcome::Sent,
        408 | 429 | 500..=599 => PostOutcome::Retryable,
        _ => PostOutcome::Permanent,
    }
}

/// The retry-queue directory name inside `--data-dir`.
pub const RETRY_DIR: &str = "lrs-retry";

/// The revoke-pending file name inside `--data-dir` (DD-113).
pub const REVOKE_PENDING: &str = "revoke-pending";

/// The failure-record file name inside `--data-dir` (DD-156, AT-236).
pub const FAILURES_FILE: &str = "lrs-failures.jsonl";

/// Rename one entry inside a dirfd (renameat, same directory).
fn rename_in_dir(dirfd: RawFd, from: &str, to: &str) -> Result<(), String> {
    let from_c = std::ffi::CString::new(from).unwrap();
    let to_c = std::ffi::CString::new(to).unwrap();
    let rc = unsafe { libc::renameat(dirfd, from_c.as_ptr(), dirfd, to_c.as_ptr()) };
    if rc != 0 {
        return Err(format!(
            "rename {from} -> {to} failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

/// Open the retry dir through the data dirfd (`O_NOFOLLOW|O_DIRECTORY`).
/// Returns `Ok(None)` when the directory does not exist.
fn open_retry_dir(data_dirfd: RawFd) -> Result<Option<RawFd>, String> {
    match crate::paths::openat_child(data_dirfd, RETRY_DIR, false) {
        Ok(fd) => {
            let st = crate::paths::fstat_fd(fd)
                .map_err(|e| format!("cannot fstat {RETRY_DIR}: {e}"))?;
            if !st.is_dir {
                crate::paths::close_fd(fd);
                return Err(format!("{RETRY_DIR} is not a directory"));
            }
            Ok(Some(fd))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot open {RETRY_DIR}: {e}")),
    }
}

/// The entry names inside `--data-dir/lrs-retry/` (empty when the
/// directory does not exist). Symlinks are listed by name (never
/// followed).
pub fn list_retry_files(data_dirfd: RawFd) -> Vec<String> {
    let Ok(Some(fd)) = open_retry_dir(data_dirfd) else {
        return Vec::new();
    };
    let names = crate::paths::read_dir_entries(fd)
        .map(|entries| entries.into_iter().map(|e| e.name).collect())
        .unwrap_or_default();
    close_fd(fd);
    names
}

/// Read one retry-queue file's bytes (the statement JSON) through the
/// data dirfd. The caller bounds the read by `--max-open-bytes`.
pub fn read_retry_file(
    data_dirfd: RawFd,
    name: &str,
    max_open_bytes: u64,
) -> Result<Vec<u8>, String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        return Err(format!("{RETRY_DIR} does not exist"));
    };
    let result = (|| {
        let fd = crate::paths::openat_child(retry_fd, name, false)
            .map_err(|e| format!("cannot open {RETRY_DIR}/{name}: {e}"))?;
        let st = crate::paths::fstat_fd(fd)
            .map_err(|e| format!("cannot fstat {RETRY_DIR}/{name}: {e}"))?;
        if !st.is_regular {
            crate::paths::close_fd(fd);
            return Err(format!("{RETRY_DIR}/{name} is not a regular file"));
        }
        let bytes = crate::paths::read_fd_bounded(fd, max_open_bytes)
            .map_err(|e| format!("cannot read {RETRY_DIR}/{name}: {e}"))?;
        crate::paths::close_fd(fd);
        Ok(bytes)
    })();
    close_fd(retry_fd);
    result
}

/// Claim a sendable retry file for one send attempt: rename
/// `{id}.json` to `{id}.claimed` under the write mutex (DD-51). The
/// claim is resolved by `resolve_claimed_*` (or left `.claimed` when
/// the send is aborted/skipped).
pub fn claim_retry_file(data_dirfd: RawFd, statement_id: &str) -> Result<(), String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        return Err(format!("{RETRY_DIR} does not exist"));
    };
    let result = rename_in_dir(
        retry_fd,
        &format!("{statement_id}.json"),
        &format!("{statement_id}.claimed"),
    );
    if result.is_ok() {
        let _ = fsync_dir(retry_fd);
    }
    close_fd(retry_fd);
    result
}

/// Resolve a claimed file after a successful send: delete `{id}.claimed`.
pub fn resolve_claimed_sent(data_dirfd: RawFd, statement_id: &str) -> Result<(), String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        return Ok(());
    };
    let result = unlink_at(retry_fd, &format!("{statement_id}.claimed"));
    if result.is_ok() {
        let _ = fsync_dir(retry_fd);
    }
    close_fd(retry_fd);
    result.map_err(|e| format!("cannot delete {statement_id}.claimed: {e}"))
}

/// Resolve a claimed file after a retryable failure: rename
/// `{id}.claimed` back to `{id}.json` (attempts+1 is tracked by the
/// caller, in memory).
pub fn resolve_claimed_retryable(data_dirfd: RawFd, statement_id: &str) -> Result<(), String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        return Err(format!("{RETRY_DIR} does not exist"));
    };
    let result = rename_in_dir(
        retry_fd,
        &format!("{statement_id}.claimed"),
        &format!("{statement_id}.json"),
    );
    if result.is_ok() {
        let _ = fsync_dir(retry_fd);
    }
    close_fd(retry_fd);
    result
}

/// Resolve a claimed file after a permanent failure: rename
/// `{id}.claimed` to `{id}.permanent`.
pub fn resolve_claimed_permanent(data_dirfd: RawFd, statement_id: &str) -> Result<(), String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        return Err(format!("{RETRY_DIR} does not exist"));
    };
    let result = rename_in_dir(
        retry_fd,
        &format!("{statement_id}.claimed"),
        &format!("{statement_id}.permanent"),
    );
    if result.is_ok() {
        let _ = fsync_dir(retry_fd);
    }
    close_fd(retry_fd);
    result
}

/// At a Scholar start with consent on (AT-91), rename every leftover
/// `{id}.claimed` back to `{id}.json` (an interrupted replay). A
/// rename failure leaves that file `.claimed` (it is not retried this
/// run); the error is returned for the startup log.
pub fn rename_claimed_to_json(data_dirfd: RawFd) -> Result<(), String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        return Ok(());
    };
    let entries = crate::paths::read_dir_entries(retry_fd)
        .map_err(|e| format!("cannot list {RETRY_DIR}: {e}"))?;
    let mut first_err: Option<String> = None;
    for entry in entries {
        if !entry.is_file {
            continue;
        }
        let Some(id) = entry.name.strip_suffix(".claimed") else {
            continue;
        };
        let target = format!("{id}.json");
        if rename_in_dir(retry_fd, &entry.name, &target).is_err() {
            first_err.get_or_insert_with(|| format!("cannot rename {} to {target}", entry.name));
        }
    }
    let _ = fsync_dir(retry_fd);
    close_fd(retry_fd);
    match first_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Delete the whole `--data-dir/lrs-retry/` queue (DD-113): unlink
/// every entry inside the dir (through a dirfd opened
/// `O_NOFOLLOW|O_DIRECTORY`, never through a replaced name), then
/// remove the directory itself and fsync the data dir. The
/// `fail_lrs_retry_delete` hook injects a failure before any entry is
/// unlinked (AT-124, AT-199). A missing directory is a no-op.
pub fn delete_retry_dir(data_dirfd: RawFd, hooks: &crate::hooks::Hooks) -> Result<(), String> {
    let Some(retry_fd) = open_retry_dir(data_dirfd)? else {
        // A missing retry dir is a no-op (nothing to delete), even
        // with the failure hook armed — the hook simulates a delete
        // failure, and there is no delete to fail.
        return Ok(());
    };
    if hooks.fail_errno("fail_lrs_retry_delete").is_some() {
        close_fd(retry_fd);
        return Err("retry queue delete failed (hook fail_lrs_retry_delete)".to_string());
    }
    let entries = crate::paths::read_dir_entries(retry_fd)
        .map_err(|e| format!("cannot list {RETRY_DIR}: {e}"))?;
    for entry in entries {
        if entry.is_dir {
            close_fd(retry_fd);
            return Err(format!("{RETRY_DIR}/{} is a directory", entry.name));
        }
        // Regular files and symlinks are unlinked by name (a symlink
        // is removed, never followed).
        unlink_at(retry_fd, &entry.name).map_err(|e| {
            format!("cannot unlink {RETRY_DIR}/{}: {e}", entry.name)
        })?;
    }
    let _ = fsync_dir(retry_fd);
    close_fd(retry_fd);
    // Remove the (now empty) directory itself.
    let c = std::ffi::CString::new(RETRY_DIR).unwrap();
    let rc = unsafe { libc::unlinkat(data_dirfd, c.as_ptr(), libc::AT_REMOVEDIR) };
    if rc != 0 {
        return Err(format!(
            "cannot remove {RETRY_DIR}: {}",
            std::io::Error::last_os_error()
        ));
    }
    fsync_dir(data_dirfd).map_err(|e| format!("data dir fsync failed: {e}"))?;
    Ok(())
}

/// Write `--data-dir/revoke-pending` (DD-113 step 1): contents `off` +
/// RFC3339 timestamp, temp + fsync + rename, mode `0600`. The
/// `fail_revoke_pending_write` hook injects a failure (AT-189).
pub fn write_revoke_pending(
    data_dirfd: RawFd,
    hooks: &crate::hooks::Hooks,
) -> Result<(), String> {
    let pid = std::process::id();
    let tmp_name = format!(
        "s3a-kite-tmp-{pid}-revoke-pending-{}",
        crate::consent::random_suffix()
    );
    let tmp_fd = openat_child_rw(data_dirfd, &tmp_name, true)
        .map_err(|e| format!("revoke-pending temp create failed: {e}"))?;
    if hooks.fail_errno("fail_revoke_pending_write").is_some() {
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err("revoke-pending write failed (hook fail_revoke_pending_write)".to_string());
    }
    let now = rfc3339_secs(std::time::SystemTime::now());
    let bytes = format!("off\n{now}\n").into_bytes();
    let mut written = 0;
    while written < bytes.len() {
        let n = unsafe {
            libc::write(
                tmp_fd,
                bytes[written..].as_ptr() as *const libc::c_void,
                bytes.len() - written,
            )
        };
        if n < 0 {
            close_fd(tmp_fd);
            let _ = unlink_at(data_dirfd, &tmp_name);
            return Err(format!(
                "revoke-pending temp write failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        written += n as usize;
    }
    if unsafe { libc::fsync(tmp_fd) } != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err(format!(
            "revoke-pending temp fsync failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let tmp_c = std::ffi::CString::new(tmp_name.clone()).unwrap();
    let final_c = std::ffi::CString::new(REVOKE_PENDING).unwrap();
    let rc = unsafe { libc::renameat(data_dirfd, tmp_c.as_ptr(), data_dirfd, final_c.as_ptr()) };
    if rc != 0 {
        let e = std::io::Error::last_os_error();
        close_fd(tmp_fd);
        let _ = unlink_at(data_dirfd, &tmp_name);
        return Err(format!("revoke-pending rename failed: {e}"));
    }
    close_fd(tmp_fd);
    fsync_dir(data_dirfd).map_err(|e| format!("data dir fsync failed: {e}"))?;
    Ok(())
}

/// Remove `--data-dir/revoke-pending` (DD-113 step 4, and the
/// qualifying-`on` cleanup). The `fail_revoke_pending_remove` hook
/// injects a failure (AT-229). A missing file is a no-op.
pub fn remove_revoke_pending(
    data_dirfd: RawFd,
    hooks: &crate::hooks::Hooks,
) -> Result<(), String> {
    match crate::paths::fstatat_nofollow(data_dirfd, REVOKE_PENDING) {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(format!("cannot lstat {REVOKE_PENDING}: {e}")),
    }
    if hooks.fail_errno("fail_revoke_pending_remove").is_some() {
        return Err("revoke-pending remove failed (hook fail_revoke_pending_remove)".to_string());
    }
    unlink_at(data_dirfd, REVOKE_PENDING)
        .map_err(|e| format!("cannot remove {REVOKE_PENDING}: {e}"))?;
    fsync_dir(data_dirfd).map_err(|e| format!("data dir fsync failed: {e}"))?;
    Ok(())
}

/// The state of `--data-dir/revoke-pending` (DD-113, AT-181).
pub enum RevokePendingStatus {
    /// No file present.
    NotPresent,
    /// A regular file (a revoke was interrupted).
    RegularFile,
    /// Present but not a regular file — a startup error naming the path.
    NonRegular,
}

/// `lstat` `--data-dir/revoke-pending` without following symlinks.
pub fn revoke_pending_status(data_dirfd: RawFd) -> Result<RevokePendingStatus, String> {
    match crate::paths::fstatat_nofollow(data_dirfd, REVOKE_PENDING) {
        Ok(st) => {
            if st.is_regular {
                Ok(RevokePendingStatus::RegularFile)
            } else {
                Ok(RevokePendingStatus::NonRegular)
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Ok(RevokePendingStatus::NotPresent)
        }
        Err(e) => Err(format!("cannot lstat {REVOKE_PENDING}: {e}")),
    }
}

/// Append one failure record to `--data-dir/lrs-failures.jsonl`
/// (DD-156, AT-236): `{"statement_id","attempts","error_code",
/// "error_message","recorded_at"}`. The file is created mode `0600`,
/// appended + fsynced. The record contains only the statement id, the
/// error code/message, the attempt count, and a timestamp — never the
/// statement body, `--researcher-id`, paper identity, or source URL.
pub fn write_failure_record(
    data_dirfd: RawFd,
    statement_id: &str,
    attempts: u32,
    error: &LrsAttemptError,
) -> Result<(), String> {
    let now = rfc3339_secs(std::time::SystemTime::now());
    let record = serde_json::json!({
        "statement_id": statement_id,
        "attempts": attempts,
        "error_code": error.code,
        "error_message": error.message,
        "recorded_at": now,
    });
    let line = format!("{}\n", serde_json::to_string(&record).unwrap());
    let fd = openat_child_rw(data_dirfd, FAILURES_FILE, true)
        .map_err(|e| format!("cannot open {FAILURES_FILE}: {e}"))?;
    let off = unsafe { libc::lseek(fd, 0, libc::SEEK_END) };
    if off < 0 {
        close_fd(fd);
        return Err(format!(
            "cannot seek {FAILURES_FILE}: {}",
            std::io::Error::last_os_error()
        ));
    }
    let bytes = line.as_bytes();
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
            close_fd(fd);
            return Err(format!(
                "cannot write {FAILURES_FILE}: {}",
                std::io::Error::last_os_error()
            ));
        }
        written += n as usize;
    }
    if unsafe { libc::fsync(fd) } != 0 {
        close_fd(fd);
        return Err(format!(
            "cannot fsync {FAILURES_FILE}: {}",
            std::io::Error::last_os_error()
        ));
    }
    close_fd(fd);
    Ok(())
}

/// Write one retry-queue file (`{id}.json` sendable, or `{id}.permanent`
/// permanent) under `--data-dir/lrs-retry/`, mode `0600`. The directory
/// is created mode `0700` when missing. The write is temp + fsync +
/// rename + fsync of the retry dir (DD-122); the temp is held `LOCK_EX`.
/// Returns an error when the write fails (the caller reports `failed`
/// and the process stays up — not a DD-138 fail-closed event). The
/// `fail_lrs_retry_write` hook injects a failure here (AT-191, Step 11
/// test).
pub fn write_retry_file(
    data_dirfd: RawFd,
    statement_id: &str,
    body: &[u8],
    suffix: &str,
    hooks: &crate::hooks::Hooks,
) -> Result<(), String> {
    // Open (or create) the retry dir through the data dirfd.
    let retry_fd = match crate::paths::openat_child(data_dirfd, RETRY_DIR, false) {
        Ok(fd) => {
            let st = crate::paths::fstat_fd(fd)
                .map_err(|e| format!("cannot fstat {RETRY_DIR}: {e}"))?;
            if !st.is_dir {
                crate::paths::close_fd(fd);
                return Err(format!("{RETRY_DIR} is not a directory"));
            }
            fd
        }
        Err(_) => {
            // Create it mode 0700 (mkdirat through the data dirfd).
            let c = std::ffi::CString::new(RETRY_DIR).unwrap();
            let rc = unsafe { libc::mkdirat(data_dirfd, c.as_ptr(), 0o700) };
            if rc != 0 {
                let e = std::io::Error::last_os_error();
                if e.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(format!("cannot create {RETRY_DIR}: {e}"));
                }
            }
            crate::paths::openat_child(data_dirfd, RETRY_DIR, false)
                .map_err(|e| format!("cannot open {RETRY_DIR}: {e}"))?
        }
    };
    let result = write_retry_file_in(retry_fd, statement_id, body, suffix, hooks);
    close_fd(retry_fd);
    result
}

fn write_retry_file_in(
    retry_fd: RawFd,
    statement_id: &str,
    body: &[u8],
    suffix: &str,
    hooks: &crate::hooks::Hooks,
) -> Result<(), String> {
    let pid = std::process::id();
    let tmp_name = format!(
        "s3a-kite-tmp-{pid}-retry-{}",
        crate::consent::random_suffix()
    );
    let tmp_fd = openat_child_rw(retry_fd, &tmp_name, true)
        .map_err(|e| format!("retry temp create failed: {e}"))?;
    let flock_rc = unsafe { libc::flock(tmp_fd, libc::LOCK_EX) };
    if flock_rc != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(retry_fd, &tmp_name);
        return Err("retry temp lock failed".to_string());
    }
    if hooks.fail_errno("fail_lrs_retry_write").is_some() {
        close_fd(tmp_fd);
        let _ = unlink_at(retry_fd, &tmp_name);
        return Err("retry file write failed (hook fail_lrs_retry_write)".to_string());
    }
    let mut written = 0;
    while written < body.len() {
        let n = unsafe {
            libc::write(
                tmp_fd,
                body[written..].as_ptr() as *const libc::c_void,
                body.len() - written,
            )
        };
        if n < 0 {
            close_fd(tmp_fd);
            let _ = unlink_at(retry_fd, &tmp_name);
            return Err(format!(
                "retry temp write failed: {}",
                std::io::Error::last_os_error()
            ));
        }
        written += n as usize;
    }
    if unsafe { libc::fsync(tmp_fd) } != 0 {
        close_fd(tmp_fd);
        let _ = unlink_at(retry_fd, &tmp_name);
        return Err(format!(
            "retry temp fsync failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let final_name = format!("{statement_id}.{suffix}");
    let tmp_c = std::ffi::CString::new(tmp_name.clone()).unwrap();
    let final_c = std::ffi::CString::new(final_name).unwrap();
    let rc = unsafe { libc::renameat(retry_fd, tmp_c.as_ptr(), retry_fd, final_c.as_ptr()) };
    if rc != 0 {
        let e = std::io::Error::last_os_error();
        close_fd(tmp_fd);
        let _ = unlink_at(retry_fd, &tmp_name);
        return Err(format!("retry rename failed: {e}"));
    }
    close_fd(tmp_fd);
    fsync_dir(retry_fd).map_err(|e| format!("retry dir fsync failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homepage_modes() {
        assert_eq!(
            actor_homepage(None),
            "https://34.74.163.88.sslip.io/scholar/lab"
        );
        assert_eq!(
            actor_homepage(Some("PROJ-42")),
            "https://34.74.163.88.sslip.io/scholar/study/PROJ-42"
        );
    }

    #[test]
    fn statement_shape() {
        let stmt = build_statement(
            "11111111-2222-4333-8444-555555555555",
            "researcher-1",
            None,
            13,
            "SRC-0007",
            Some("https://example.test/p"),
            Some("ab12cd"),
            1794000000,
        );
        assert_eq!(stmt["verb"]["id"], "https://ashecribb.dev/xapi/verbs/saved");
        assert_eq!(stmt["object"]["id"], "https://example.test/p");
        assert_eq!(
            stmt["object"]["definition"]["extensions"]
                ["https://ashecribb.dev/xapi/ext/content-sha256"],
            "ab12cd"
        );
        assert_eq!(
            stmt["actor"]["account"]["homePage"],
            "https://34.74.163.88.sslip.io/scholar/lab"
        );
        // No source_url → the fallback object id.
        let stmt2 = build_statement(
            "11111111-2222-4333-8444-555555555555",
            "researcher-1",
            None,
            11,
            "SRC-0007",
            None,
            None,
            1794000000,
        );
        assert_eq!(
            stmt2["object"]["id"],
            "https://ashecribb.dev/xapi/activities/paper/SRC-0007"
        );
        assert!(stmt2["object"]["definition"]["extensions"]
            ["https://ashecribb.dev/xapi/ext/content-sha256"]
            .is_null());
    }

    #[test]
    fn classify() {
        assert!(matches!(classify_status(200), PostOutcome::Sent));
        assert!(matches!(classify_status(409), PostOutcome::Sent));
        assert!(matches!(classify_status(503), PostOutcome::Retryable));
        assert!(matches!(classify_status(429), PostOutcome::Retryable));
        assert!(matches!(classify_status(400), PostOutcome::Permanent));
        assert!(matches!(classify_status(302), PostOutcome::Permanent));
    }
}
