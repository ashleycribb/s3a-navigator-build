//! Hand-rolled HTTP/1.1 server (std threads + rustls).
//!
//! Why not hyper/tokio: the SPEC's connection-level behaviors need exact
//! control over accept, handshake timing, and read deadlines —
//! AT-230 (idle deadline from accept, including the TLS handshake, and
//! restarting after each response), AT-231 (accept-then-close the 9th
//! non-loopback connection), AT-162 (header-block 413 before auth).
//!
//! The server owns the transport-level precedence: a header block over
//! --max-request-bytes is 413 before auth; a malformed request line or
//! header is 400 `invalid_param`; `Transfer-Encoding: chunked` is 400
//! `invalid_param`. Everything else is delegated to the `Handler`.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One parsed HTTP request.
#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub is_tls: bool,
    /// True for loopback TCP peers and for Unix-socket peers.
    pub peer_is_loopback: bool,
    /// The request body would exceed --max-request-bytes. The body was
    /// not read; the handler returns 413 `request_too_large` after
    /// authentication, 405, and 415 (AT-162, AT-171).
    pub body_over_cap: bool,
}

impl Request {
    /// First header value with this (case-insensitive) name.
    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(n, _)| *n == lower)
            .map(|(_, v)| v.as_str())
    }

    pub fn query_param(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn content_type(&self) -> Option<&str> {
        self.header("content-type")
    }
}

/// One HTTP response.
#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

impl Response {
    pub fn json(status: u16, body: Vec<u8>) -> Response {
        Response {
            status,
            content_type: "application/json",
            body,
        }
    }

    pub fn text(status: u16, body: &str) -> Response {
        Response {
            status,
            content_type: "text/plain",
            body: body.as_bytes().to_vec(),
        }
    }

    /// `{"error": "<code>", "message": "<text>"}` — the SPEC's error body
    /// shape (DD-83). No secrets or identifiers in the message.
    pub fn error(status: u16, code: &str, message: &str) -> Response {
        let body = serde_json::json!({"error": code, "message": message});
        Response::json(status, serde_json::to_vec(&body).unwrap())
    }
}

/// The route handler. `handle` is called once per request; `on_5xx` is
/// called when a response with status >= 500 is produced (for the log line).
pub trait Handler: Send + Sync {
    fn handle(&self, req: &Request) -> Response;
    fn on_5xx(&self, _status: u16, _method: &str, _path: &str) {}
}

/// Server configuration (from the validated flags).
pub struct ServerConfig {
    pub tls: Option<Arc<rustls::ServerConfig>>,
    pub max_request_bytes: u64,
    pub handle_idle: Duration,
    pub shutdown: Arc<AtomicBool>,
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        100 => "Continue",
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        409 => "Conflict",
        413 => "Content Too Large",
        415 => "Unsupported Media Type",
        500 => "Internal Server Error",
        _ => "Unknown",
    }
}

/// Serve the TCP listener until `cfg.shutdown` is set. Each connection
/// runs on its own thread.
pub fn serve_tcp(
    listener: TcpListener,
    cfg: Arc<ServerConfig>,
    handler: Arc<dyn Handler>,
) {
    let non_loopback_conns = Arc::new(AtomicUsize::new(0));
    loop {
        if cfg.shutdown.load(Ordering::Relaxed) {
            return;
        }
        match listener.accept() {
            Ok((stream, _addr)) => {
                let peer_loopback = stream
                    .peer_addr()
                    .map(|a| crate::flags::is_loopback_ip(a.ip()))
                    .unwrap_or(false);
                if !peer_loopback {
                    // AT-231: non-loopback concurrent connections are capped
                    // at 8; the 9th is accepted then closed immediately,
                    // without a handshake or a response.
                    let n = non_loopback_conns.fetch_add(1, Ordering::SeqCst) + 1;
                    if n > 8 {
                        non_loopback_conns.fetch_sub(1, Ordering::SeqCst);
                        drop(stream);
                        continue;
                    }
                }
                let cfg = cfg.clone();
                let handler = handler.clone();
                let counter = non_loopback_conns.clone();
                let guard = ConnGuard {
                    counter: if peer_loopback { None } else { Some(counter) },
                };
                std::thread::spawn(move || {
                    let _guard = guard;
                    handle_tcp_conn(stream, peer_loopback, &cfg, &handler);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

/// Serve the Unix listener until `cfg.shutdown` is set.
pub fn serve_unix(
    listener: UnixListener,
    cfg: Arc<ServerConfig>,
    handler: Arc<dyn Handler>,
) {
    loop {
        if cfg.shutdown.load(Ordering::Relaxed) {
            return;
        }
        match listener.accept() {
            Ok((stream, _addr)) => {
                let cfg = cfg.clone();
                let handler = handler.clone();
                std::thread::spawn(move || {
                    handle_unix_conn(stream, &cfg, &handler);
                });
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(_) => {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

struct ConnGuard {
    counter: Option<Arc<AtomicUsize>>,
}

impl Drop for ConnGuard {
    fn drop(&mut self) {
        if let Some(c) = &self.counter {
            c.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

/// A byte stream with a read buffer, a per-read deadline, and (for TLS)
/// an optional rustls server connection.
enum Conn {
    Plain(PlainConn),
    Tls(TlsConn),
}

struct PlainConn {
    stream: PlainStream,
    buf: Vec<u8>,
}

struct TlsConn {
    conn: rustls::ServerConnection,
    stream: TcpStream,
    buf: Vec<u8>,
}

/// A plaintext stream (TCP or Unix). Kept concrete (not a trait object)
/// so the read/write timeouts can be set per read.
enum PlainStream {
    Tcp(TcpStream),
    Unix(UnixStream),
}

impl Read for PlainStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            PlainStream::Tcp(s) => s.read(buf),
            PlainStream::Unix(s) => s.read(buf),
        }
    }
}

impl Write for PlainStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            PlainStream::Tcp(s) => s.write(buf),
            PlainStream::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            PlainStream::Tcp(s) => s.flush(),
            PlainStream::Unix(s) => s.flush(),
        }
    }
}

impl PlainStream {
    fn set_timeouts(&mut self, timeout: Duration) {
        match self {
            PlainStream::Tcp(s) => {
                let _ = s.set_read_timeout(Some(timeout));
                let _ = s.set_write_timeout(Some(timeout));
            }
            PlainStream::Unix(s) => {
                let _ = s.set_read_timeout(Some(timeout));
                let _ = s.set_write_timeout(Some(timeout));
            }
        }
    }
}

impl Conn {
    /// Perform ONE read (bounded by the deadline as a socket timeout) and
    /// append the bytes to the buffer. Returns `Ok(true)` when bytes were
    /// read, `Ok(false)` on a clean EOF, and `Err` on timeout or I/O
    /// error. Callers loop until they have enough bytes.
    fn fill_once(&mut self, deadline: Instant) -> std::io::Result<bool> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "idle deadline exceeded",
            ));
        }
        match self {
            Conn::Plain(c) => {
                let mut chunk = [0u8; 8192];
                c.stream.set_timeouts(remaining);
                match c.stream.read(&mut chunk) {
                    Ok(0) => Ok(false),
                    Ok(n) => {
                        c.buf.extend_from_slice(&chunk[..n]);
                        Ok(true)
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "idle deadline exceeded",
                        ))
                    }
                    Err(e) => Err(e),
                }
            }
            Conn::Tls(c) => {
                let mut chunk = [0u8; 8192];
                let _ = c.stream.set_read_timeout(Some(remaining));
                let _ = c.stream.set_write_timeout(Some(remaining));
                // 1. Drain any already-decrypted plaintext. NOTE: rustls
                // Reader::read returns Err(WouldBlock) when no plaintext
                // is buffered — that is not an error, it means "drive the
                // connection below". Ok(0) means a clean close_notify.
                match c.conn.reader().read(&mut chunk) {
                    Ok(n) if n > 0 => {
                        c.buf.extend_from_slice(&chunk[..n]);
                        return Ok(true);
                    }
                    Ok(_) => return Ok(false),
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock => {}
                    Err(e) => return Err(e),
                }
                // 2. Drive the TLS connection (reads TLS records from the
                // socket, blocking up to the deadline via the timeout).
                if let Err(e) = c.conn.complete_io(&mut c.stream) {
                    if e.kind() == std::io::ErrorKind::WouldBlock {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::TimedOut,
                            "idle deadline exceeded",
                        ));
                    }
                    return Err(e);
                }
                // 3. Read whatever was decrypted.
                match c.conn.reader().read(&mut chunk) {
                    Ok(n) if n > 0 => {
                        c.buf.extend_from_slice(&chunk[..n]);
                        Ok(true)
                    }
                    Ok(_) => Ok(false),
                    Err(ref e)
                        if e.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        // complete_io made no progress and no plaintext
                        // arrived; the peer closed or the deadline
                        // passed. Report EOF and let the caller decide.
                        Ok(false)
                    }
                    Err(e) => Err(e),
                }
            }
        }
    }

    fn write_all_deadline(&mut self, bytes: &[u8], deadline: Instant) -> std::io::Result<()> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "idle deadline exceeded",
            ));
        }
        match self {
            Conn::Plain(c) => {
                c.stream.set_timeouts(remaining);
                c.stream.write_all(bytes)?;
                c.stream.flush()
            }
            Conn::Tls(c) => {
                let _ = c.stream.set_write_timeout(Some(remaining));
                c.conn.writer().write_all(bytes)?;
                while c.conn.wants_write() {
                    c.conn.complete_io(&mut c.stream)?;
                }
                Ok(())
            }
        }
    }

    fn take_buf(&mut self) -> Vec<u8> {
        match self {
            Conn::Plain(c) => std::mem::take(&mut c.buf),
            Conn::Tls(c) => std::mem::take(&mut c.buf),
        }
    }

    /// Graceful close after an early 413 (header-block or body over cap):
    /// shutdown the write half (the 413 is already queued) and drain the
    /// unread request bytes briefly, so the kernel does not RST the
    /// connection — a RST would discard the 413 on the client's side.
    /// Bounded by a 2s drain; the caller drops the connection afterwards.
    fn graceful_close(&mut self) {
        let drain_deadline = Instant::now() + Duration::from_secs(2);
        let mut buf = [0u8; 8192];
        match self {
            Conn::Plain(c) => {
                let _ = match &c.stream {
                    PlainStream::Tcp(s) => s.shutdown(std::net::Shutdown::Write),
                    PlainStream::Unix(s) => s.shutdown(std::net::Shutdown::Write),
                };
                loop {
                    let remaining = drain_deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    c.stream.set_timeouts(remaining);
                    match c.stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }
            }
            Conn::Tls(c) => {
                c.conn.send_close_notify();
                let _ = c.conn.complete_io(&mut c.stream);
                let _ = c.stream.shutdown(std::net::Shutdown::Write);
                loop {
                    let remaining = drain_deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    let _ = c.stream.set_read_timeout(Some(remaining));
                    match c.stream.read(&mut buf) {
                        Ok(0) => break,
                        Ok(_) => continue,
                        Err(_) => break,
                    }
                }
            }
        }
    }

    fn set_buf(&mut self, buf: Vec<u8>) {
        match self {
            Conn::Plain(c) => c.buf = buf,
            Conn::Tls(c) => c.buf = buf,
        }
    }
}

fn handle_tcp_conn(stream: TcpStream, peer_loopback: bool, cfg: &Arc<ServerConfig>, handler: &Arc<dyn Handler>) {
    let accept_time = Instant::now();
    let mut conn = if let Some(tls) = &cfg.tls {
        let Ok(server_conn) = rustls::ServerConnection::new(tls.clone()) else {
            return;
        };
        let tls_conn = TlsConn {
            conn: server_conn,
            stream,
            buf: Vec::new(),
        };
        // AT-230: the idle deadline runs from accept, including the TLS
        // handshake.
        let deadline = accept_time + cfg.handle_idle;
        let mut tls = Conn::Tls(tls_conn);
        match handshake(&mut tls, deadline) {
            Ok(()) => tls,
            Err(_) => return,
        }
    } else {
        Conn::Plain(PlainConn {
            stream: PlainStream::Tcp(stream),
            buf: Vec::new(),
        })
    };
    let is_tls = cfg.tls.is_some();
    serve_conn(&mut conn, is_tls, peer_loopback, accept_time, cfg, handler);
}

fn handshake(conn: &mut Conn, deadline: Instant) -> std::io::Result<()> {
    let Conn::Tls(c) = conn else {
        return Ok(());
    };
    while c.conn.is_handshaking() {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "handshake deadline exceeded",
            ));
        }
        let _ = c.stream.set_read_timeout(Some(remaining));
        let _ = c.stream.set_write_timeout(Some(remaining));
        c.conn.complete_io(&mut c.stream)?;
    }
    Ok(())
}

fn handle_unix_conn(stream: UnixStream, cfg: &Arc<ServerConfig>, handler: &Arc<dyn Handler>) {
    let accept_time = Instant::now();
    let mut conn = Conn::Plain(PlainConn {
        stream: PlainStream::Unix(stream),
        buf: Vec::new(),
    });
    serve_conn(&mut conn, false, true, accept_time, cfg, handler);
}

/// The keep-alive loop for one connection. `accept_time` starts the
/// AT-230 idle deadline; the deadline restarts after each response.
fn serve_conn(
    conn: &mut Conn,
    is_tls: bool,
    peer_loopback: bool,
    accept_time: Instant,
    cfg: &Arc<ServerConfig>,
    handler: &Arc<dyn Handler>,
) {
    let mut deadline = accept_time + cfg.handle_idle;
    loop {
        if cfg.shutdown.load(Ordering::Relaxed) {
            return;
        }
        let req = match read_request(conn, deadline, cfg.max_request_bytes) {
            Ok(Some(req)) => req,
            Ok(None) => return, // client closed
            Err(ReadError::TooLarge) => {
                let resp = Response::error(
                    413,
                    "request_too_large",
                    "request header block exceeds --max-request-bytes",
                );
                let _ = write_response(conn, &resp, deadline, false);
                conn.graceful_close();
                return;
            }
            Err(ReadError::BadRequest) => {
                let resp = Response::error(400, "invalid_param", "malformed request");
                let _ = write_response(conn, &resp, deadline, false);
                return;
            }
            Err(ReadError::Closed) | Err(ReadError::Timeout) | Err(ReadError::Io) => return,
        };
        let mut req = req;
        req.is_tls = is_tls;
        req.peer_is_loopback = peer_loopback;
        let keep_alive = wants_keep_alive(&req);
        let resp = handler.handle(&req);
        if resp.status >= 500 {
            handler.on_5xx(resp.status, &req.method, &req.path);
        }
        if write_response(conn, &resp, deadline, keep_alive).is_err() {
            return;
        }
        if req.body_over_cap {
            // The unread body bytes are still in the socket; close
            // gracefully so the 413 is delivered instead of RST away.
            conn.graceful_close();
            return;
        }
        if !keep_alive {
            return;
        }
        // AT-230: the deadline restarts after each response.
        deadline = Instant::now() + cfg.handle_idle;
    }
}

fn wants_keep_alive(req: &Request) -> bool {
    match req.header("connection").map(|v| v.to_ascii_lowercase()) {
        Some(v) if v.split(',').any(|t| t.trim() == "close") => false,
        Some(v) if v.split(',').any(|t| t.trim() == "keep-alive") => true,
        _ => true, // HTTP/1.1 default
    }
}

enum ReadError {
    TooLarge,
    BadRequest,
    Closed,
    Timeout,
    Io,
}

/// Read one request (headers + body) from the connection buffer.
/// Returns `Ok(None)` when the client closed the connection cleanly
/// before sending a request.
fn read_request(
    conn: &mut Conn,
    deadline: Instant,
    max_request_bytes: u64,
) -> Result<Option<Request>, ReadError> {
    let mut buf = conn.take_buf();
    // Find the header-block terminator, reading more as needed.
    let header_end = loop {
        if let Some(pos) = find_subslice(&buf, b"\r\n\r\n") {
            break pos + 4;
        }
        if buf.len() as u64 > max_request_bytes {
            // AT-162: header-block over --max-request-bytes is 413
            // before auth.
            return Err(ReadError::TooLarge);
        }
        conn.set_buf(std::mem::take(&mut buf));
        match conn.fill_once(deadline) {
            Ok(true) => {
                buf = conn.take_buf();
            }
            Ok(false) => {
                // EOF with a partial (or empty) header block.
                buf = conn.take_buf();
                return if buf.is_empty() {
                    Ok(None)
                } else {
                    Err(ReadError::Closed)
                };
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return Err(ReadError::Timeout),
            Err(_) => return Err(ReadError::Io),
        }
    };
    if header_end as u64 > max_request_bytes {
        return Err(ReadError::TooLarge);
    }
    let header_bytes = buf[..header_end].to_vec();
    let mut rest = buf[header_end..].to_vec();
    let text = match std::str::from_utf8(&header_bytes) {
        Ok(t) => t.to_string(),
        Err(_) => return Err(ReadError::BadRequest),
    };
    let mut lines = text.lines();
    let request_line = lines.next().ok_or(ReadError::BadRequest)?;
    let mut parts = request_line.splitn(3, ' ');
    let method = parts.next().ok_or(ReadError::BadRequest)?.to_string();
    let target = parts.next().ok_or(ReadError::BadRequest)?.to_string();
    let version = parts.next().ok_or(ReadError::BadRequest)?;
    if !version.starts_with("HTTP/1.") {
        return Err(ReadError::BadRequest);
    }
    if method.is_empty() || target.is_empty() {
        return Err(ReadError::BadRequest);
    }
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines {
        if line.is_empty() {
            continue;
        }
        let (name, value) = line.split_once(':').ok_or(ReadError::BadRequest)?;
        headers.push((
            name.trim().to_ascii_lowercase(),
            value.trim().to_string(),
        ));
    }
    // Transfer-Encoding: chunked is 400 invalid_param (documented choice).
    if let Some(te) = headers
        .iter()
        .find(|(n, _)| n == "transfer-encoding")
        .map(|(_, v)| v.to_ascii_lowercase())
    {
        if te.trim() != "identity" {
            return Err(ReadError::BadRequest);
        }
    }
    let content_length: usize = match headers
        .iter()
        .find(|(n, _)| n == "content-length")
        .map(|(_, v)| v.as_str())
    {
        Some(v) => v
            .trim()
            .parse::<usize>()
            .map_err(|_| ReadError::BadRequest)?,
        None => 0,
    };
    // Split the target into path + query first: the body-over-cap
    // return below needs them.
    let (path, query_str) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), Some(q.to_string())),
        None => (target.clone(), None),
    };
    let mut query = Vec::new();
    if let Some(q) = query_str {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = match pair.split_once('=') {
                Some((k, v)) => (k.to_string(), v.to_string()),
                None => (pair.to_string(), String::new()),
            };
            query.push((percent_decode(&k), percent_decode(&v)));
        }
    }
    if header_end as u64 + content_length as u64 > max_request_bytes {
        // Body overflow is 413 *after* authentication (AT-162, AT-171):
        // return the request with the flag set and an empty body; the
        // handler checks auth, 405, and 415 first, then 413.
        return Ok(Some(Request {
            method,
            path,
            query,
            headers,
            body: Vec::new(),
            is_tls: false,
            peer_is_loopback: false,
            body_over_cap: true,
        }));
    }
    // Expect: 100-continue — send the interim response, then read the body.
    if headers
        .iter()
        .find(|(n, _)| n == "expect")
        .map(|(_, v)| v.to_ascii_lowercase())
        .as_deref()
        == Some("100-continue")
    {
        let _ = conn.write_all_deadline(b"HTTP/1.1 100 Continue\r\n\r\n", deadline);
    }
    // Read the body (from the buffer first, then the socket).
    while rest.len() < content_length {
        conn.set_buf(std::mem::take(&mut rest));
        match conn.fill_once(deadline) {
            Ok(true) => {
                rest = conn.take_buf();
            }
            Ok(false) => return Err(ReadError::Closed),
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return Err(ReadError::Timeout),
            Err(_) => return Err(ReadError::Io),
        }
    }
    let body = rest[..content_length].to_vec();
    let leftover = rest[content_length..].to_vec();
    conn.set_buf(leftover);
    Ok(Some(Request {
        method,
        path,
        query,
        headers,
        body,
        is_tls: false,
        peer_is_loopback: false,
        body_over_cap: false,
    }))
}

fn write_response(
    conn: &mut Conn,
    resp: &Response,
    deadline: Instant,
    keep_alive: bool,
) -> std::io::Result<()> {
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: {}\r\n\r\n",
        resp.status,
        reason_phrase(resp.status),
        resp.content_type,
        resp.body.len(),
        if keep_alive { "keep-alive" } else { "close" },
    );
    let mut bytes = header.into_bytes();
    bytes.extend_from_slice(&resp.body);
    conn.write_all_deadline(&bytes, deadline)
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(n) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(n);
                i += 3;
                continue;
            }
        }
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}
