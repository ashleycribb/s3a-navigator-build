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
}

/// The `lrs_status` values (DD-131).
pub type LrsStatus = &'static str;

/// POST one statement to the LRS and classify the outcome (DD-134).
/// Only the status line is needed: after it is read, an oversized
/// remainder follows the status class and is not read further (AT-191).
/// The header block is bounded by `--max-open-bytes`; if it is exceeded
/// before a status line is read, the attempt is `Permanent`.
/// `ips` are the LRS host's startup-resolved addresses.
#[allow(clippy::too_many_arguments)]
pub fn post_statement(
    lrs: &LrsUrl,
    ips: &[IpAddr],
    credentials: &(String, String),
    statement: &serde_json::Value,
    timeout_secs: u64,
    max_open_bytes: u64,
) -> PostOutcome {
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
    let mut conn = match connect_lrs(&lrs.host, port, ips, lrs.https, deadline) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };
    conn.set_deadline(deadline);
    let mut request_bytes = request.into_bytes();
    request_bytes.extend_from_slice(&body);
    if let Err(e) = conn.write_all(&request_bytes).and_then(|_| conn.flush()) {
        return io_post_error(e, lrs.https, deadline);
    }
    // Read the status line (bounded by --max-open-bytes). Only the
    // status matters; the body is never read.
    let status = read_lrs_status(&mut conn, deadline, max_open_bytes);
    match status {
        LrsRead::Status(code) => classify_status(code),
        LrsRead::HeaderTooLargeBeforeStatus => PostOutcome::Permanent,
        LrsRead::NoResponse => PostOutcome::NoResponse,
        LrsRead::TlsFailure => PostOutcome::Permanent,
    }
}

/// Connect to the LRS host (TCP, then a TLS handshake for https).
/// Returns the connection, or the `PostOutcome` for a connect failure:
/// `NoResponse` for refused/DNS/timeout, `Permanent` for a TLS
/// certificate failure.
fn connect_lrs(
    host: &str,
    port: u16,
    ips: &[IpAddr],
    https: bool,
    deadline: Instant,
) -> Result<Conn, PostOutcome> {
    // Resolve when the startup list is empty (a hostname that resolved
    // at startup always has entries; an IP literal has one).
    let owned: Vec<IpAddr>;
    let ips: &[IpAddr] = if ips.is_empty() {
        match (host, port).to_socket_addrs() {
            Ok(addrs) => {
                owned = addrs.map(|s| s.ip()).collect();
                &owned
            }
            Err(_) => return Err(PostOutcome::NoResponse),
        }
    } else {
        ips
    };
    for ip in ips {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(PostOutcome::NoResponse);
        }
        let sock = std::net::SocketAddr::new(*ip, port);
        match TcpStream::connect_timeout(&sock, remaining) {
            Ok(tcp) => {
                let _ = tcp.set_nodelay(true);
                if !https {
                    return Ok(Conn::Plain(tcp));
                }
                let name = match rustls::pki_types::ServerName::try_from(host.to_string()) {
                    Ok(n) => n,
                    Err(_) => return Err(PostOutcome::Permanent),
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
                        match handshake(&mut stream) {
                            Ok(()) => return Ok(Conn::Tls(Box::new(stream))),
                            Err(outcome) => return Err(outcome),
                        }
                    }
                    Err(_) => return Err(PostOutcome::Permanent),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => {
                return Err(PostOutcome::NoResponse)
            }
            Err(_) => continue,
        }
    }
    if Instant::now() >= deadline {
        Err(PostOutcome::NoResponse)
    } else {
        Err(PostOutcome::NoResponse)
    }
}

/// Run the TLS handshake to completion (the connection is usable after
/// this returns). A certificate failure is `Permanent`; a timeout is
/// `NoResponse`; any other I/O failure is `NoResponse`.
fn handshake(
    stream: &mut rustls::StreamOwned<rustls::ClientConnection, TcpStream>,
) -> Result<(), PostOutcome> {
    loop {
        if !stream.conn.is_handshaking() {
            return Ok(());
        }
        match stream.conn.complete_io(&mut stream.sock) {
            Ok(_) => continue,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                {
                    return Err(PostOutcome::NoResponse);
                }
                if is_tls_error(&e) {
                    return Err(PostOutcome::Permanent);
                }
                return Err(PostOutcome::NoResponse);
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

/// Map a post-write I/O error to a `PostOutcome`: a timeout is
/// `NoResponse`; a TLS error is `Permanent`; anything else is
/// `NoResponse`.
fn io_post_error(e: std::io::Error, https: bool, deadline: Instant) -> PostOutcome {
    if e.kind() == std::io::ErrorKind::WouldBlock
        || e.kind() == std::io::ErrorKind::TimedOut
        || Instant::now() >= deadline
    {
        return PostOutcome::NoResponse;
    }
    if https && is_tls_error(&e) {
        return PostOutcome::Permanent;
    }
    PostOutcome::NoResponse
}

enum LrsRead {
    Status(u16),
    /// The header block exceeded the cap before a status line was read.
    HeaderTooLargeBeforeStatus,
    /// No HTTP response (timeout / refused / reset mid-read).
    NoResponse,
    /// A TLS failure while reading.
    TlsFailure,
}

/// Read the LRS response status line (and the remainder of the header
/// block, up to the cap). The body is never read. If the header block
/// exceeds the cap after the status line was read, the status class
/// still decides (the remainder is not read). EOF after a complete
/// status line also leaves the status class to decide.
fn read_lrs_status(conn: &mut Conn, deadline: Instant, max_open_bytes: u64) -> LrsRead {
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
        conn.set_deadline(deadline);
        match conn.read(&mut chunk) {
            Ok(0) => return LrsRead::NoResponse,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) => return read_err(e),
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
        conn.set_deadline(deadline);
        match conn.read(&mut chunk) {
            Ok(0) => return LrsRead::Status(status),
            Ok(n) => {
                total += n as u64;
                buf.extend_from_slice(&chunk[..n]);
                if total > max_open_bytes {
                    return LrsRead::Status(status);
                }
            }
            Err(e) => return read_err(e),
        }
    }
}

/// Map a read error while awaiting the LRS response: a timeout is
/// `NoResponse`; a TLS error is `TlsFailure`; anything else is
/// `NoResponse`.
fn read_err(e: std::io::Error) -> LrsRead {
    if e.kind() == std::io::ErrorKind::WouldBlock
        || e.kind() == std::io::ErrorKind::TimedOut
    {
        return LrsRead::NoResponse;
    }
    if is_tls_error(&e) {
        return LrsRead::TlsFailure;
    }
    LrsRead::NoResponse
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
