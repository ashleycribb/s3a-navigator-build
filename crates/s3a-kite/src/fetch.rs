//! Source fetch client (§5.2.4): std `TcpStream` + rustls, redirect chain,
//! gzip/deflate decode, caps, wall-clock timeout. No proxy (AT-122).
//!
//! The `fetch_error` vocabulary is returned in SPEC order:
//! `scheme_not_allowed`, `egress_denied`, `egress_not_allowlisted`,
//! `connect_failed`, `timeout`, `too_many_redirects`, `http_status`,
//! `unknown_encoding`, `headers_too_large`, `body_incomplete`,
//! `decode_failed`, `body_too_large`, `snapshot_too_large` (the last is
//! produced by the snapshot publish step, not by this client).

use std::io::{Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::egress::{
    allowlist_matches, is_denied_address, is_denied_hostname, lrs_deny_union, AllowEntry,
};
use crate::flags::LrsUrl;

/// One fetch error string (§5.2.4 vocabulary).
pub type FetchError = &'static str;

/// Fetch configuration (from the validated host `Config`).
pub struct FetchConfig<'a> {
    pub lrs: Option<&'a LrsUrl>,
    pub allowlist: Option<&'a [AllowEntry]>,
    pub egress_test_loopback_port: Option<u16>,
    pub fetch_timeout_secs: u64,
    pub max_fetch_bytes: u64,
    pub max_redirects: u64,
}

/// The outcome of a fetch: the decoded body and the final URL, or a
/// `fetch_error` string.
pub enum FetchOutcome {
    Ok { body: Vec<u8>, final_url: String },
    Err(FetchError),
}

/// A parsed fetch URL (http/https only by the time it reaches the wire).
struct ParsedUrl {
    scheme: String,
    host: String,
    port: u16,
    path: String,
}

/// Parse `scheme://authority/path`. Returns `None` when the URL has no
/// `://`, an empty host, or a userinfo/query/fragment the client does not
/// support (a `source_url` never carries those; a redirect Location that
/// does is rejected as `scheme_not_allowed` by the caller).
fn parse_url(url: &str) -> Option<ParsedUrl> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() {
        return None;
    }
    // Authority ends at the first '/' (or the end of the string).
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let (host, port) = if let Some(rest) = authority.strip_prefix('[') {
        // [ipv6] or [ipv6]:port.
        let (h, p) = match rest.split_once(']') {
            Some((h, "")) => (h.to_string(), None),
            Some((h, p)) => {
                let p = p.strip_prefix(':')?;
                (h.to_string(), Some(p.parse::<u16>().ok()?))
            }
            None => return None,
        };
        (h, p)
    } else if let Some((h, p)) = authority.rsplit_once(':') {
        // A bare IPv6 literal without brackets has more than one colon
        // and is not a host:port pair.
        if h.contains(':') {
            (authority.to_string(), None)
        } else {
            (h.to_string(), Some(p.parse::<u16>().ok()?))
        }
    } else {
        (authority.to_string(), None)
    };
    if host.is_empty() {
        return None;
    }
    let port = port.unwrap_or(if scheme.eq_ignore_ascii_case("https") {
        443
    } else {
        80
    });
    Some(ParsedUrl {
        scheme: scheme.to_ascii_lowercase(),
        host,
        port,
        path,
    })
}

/// Resolve a redirect `Location` against the current URL: an absolute URL
/// is used as-is; a root-relative path keeps the scheme + authority; any
/// other relative reference resolves against the current path's directory.
fn resolve_url(current: &str, location: &str) -> String {
    if location.contains("://") {
        return location.to_string();
    }
    if let Some(rest) = location.strip_prefix('/') {
        let _ = rest;
        if let Some((scheme, authority)) = current
            .split_once("://")
            .map(|(s, r)| (s, r.split('/').next().unwrap_or(r)))
        {
            return format!("{scheme}://{authority}{location}");
        }
        return location.to_string();
    }
    // Relative reference: replace the last path segment.
    let base = current.rsplit_once('/').map(|(b, _)| b).unwrap_or(current);
    format!("{base}/{location}")
}

/// Resolve a hostname to IPs (an IP literal resolves to itself). A DNS
/// failure returns `None` (the caller maps it to `connect_failed`).
/// The std resolver is blocking and has no deadline; the caller checks
/// the fetch deadline after this returns.
fn resolve_host(host: &str, port: u16) -> Option<Vec<IpAddr>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(vec![ip]);
    }
    let addrs: Vec<IpAddr> = (host, port)
        .to_socket_addrs()
        .ok()?
        .map(|s| s.ip())
        .collect();
    if addrs.is_empty() {
        None
    } else {
        Some(addrs)
    }
}

/// The egress checks (§5.2.4), in SPEC order: LRS-host deny union (name,
/// then resolved IPs) → `egress_test` exception → general deny list →
/// allowlist. Returns the `fetch_error` when denied, or the resolved IPs
/// to connect to when allowed. A DNS failure is `connect_failed`.
fn egress_check(
    p: &ParsedUrl,
    cfg: &FetchConfig,
    deadline: Instant,
) -> Result<Vec<IpAddr>, FetchError> {
    // LRS-host deny by name, before any DNS query.
    if let Some(lrs) = cfg.lrs {
        if p.host.eq_ignore_ascii_case(&lrs.host) {
            return Err("egress_denied");
        }
    }
    let ips = resolve_host(&p.host, p.port).ok_or("connect_failed")?;
    if Instant::now() >= deadline {
        return Err("timeout");
    }
    // LRS-host deny by resolved IP (startup resolution; the connect-time
    // resolution is this same lookup).
    if let Some(lrs) = cfg.lrs {
        if ips.iter().any(|ip| lrs_deny_union(&lrs.resolved, &[], ip)) {
            return Err("egress_denied");
        }
    }
    // `egress_test` exception: loopback at the configured port, only in
    // an `egress_test` build. It bypasses the general deny list only;
    // the LRS deny above already ran, and the allowlist below still runs.
    let egress_test_ok = cfg!(feature = "egress_test")
        && cfg.egress_test_loopback_port == Some(p.port)
        && ips.iter().any(|ip| {
            *ip == IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))
                || *ip == IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        });
    if !egress_test_ok {
        if is_denied_hostname(&p.host) {
            return Err("egress_denied");
        }
        if ips.iter().any(|ip| is_denied_address(ip)) {
            return Err("egress_denied");
        }
    }
    if let Some(entries) = cfg.allowlist {
        if !allowlist_matches(entries, &p.scheme, &p.host, p.port) {
            return Err("egress_not_allowlisted");
        }
    }
    Ok(ips)
}

/// A connected socket: plain TCP, or TLS over TCP (rustls).
pub enum Conn {
    Plain(TcpStream),
    Tls(Box<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>),
}

impl Conn {
    /// Set the socket read/write timeouts to the time remaining until
    /// `deadline` (zero when the deadline has passed, so the next I/O
    /// fails promptly with `timeout`).
    pub fn set_deadline(&mut self, deadline: Instant) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let _ = match self {
            Conn::Plain(s) => {
                let _ = s.set_read_timeout(Some(remaining));
                s.set_write_timeout(Some(remaining))
            }
            Conn::Tls(st) => {
                let tcp = &mut st.sock;
                let _ = tcp.set_read_timeout(Some(remaining));
                tcp.set_write_timeout(Some(remaining))
            }
        };
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.read(buf),
            Conn::Tls(st) => st.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Conn::Plain(s) => s.write(buf),
            Conn::Tls(st) => st.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Conn::Plain(s) => s.flush(),
            Conn::Tls(st) => st.flush(),
        }
    }
}

/// A rustls client config with the webpki roots (for https fetches and
/// the LRS client).
pub fn tls_client_config() -> Arc<rustls::ClientConfig> {
    let mut roots = rustls::RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    Arc::new(
        rustls::ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
            .expect("TLS 1.2/1.3 with ring")
            .with_root_certificates(roots)
            .with_no_client_auth(),
    )
}

/// Connect to one of `ips` (in order) for `host:port`, plain or TLS.
/// The TLS handshake runs on the first read/write. Returns
/// `connect_failed` when no address accepts, `timeout` when the
/// deadline passed first. A TLS failure surfaces as `connect_failed`
/// for a fetch (the fetch vocabulary has no TLS error); the LRS client
/// distinguishes certificate failures itself.
pub fn connect(
    host: &str,
    port: u16,
    ips: &[IpAddr],
    https: bool,
    deadline: Instant,
) -> Result<Conn, FetchError> {
    for ip in ips {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("timeout");
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
                    Err(_) => return Err("connect_failed"),
                };
                match rustls::ClientConnection::new(tls_client_config(), name) {
                    Ok(conn) => {
                        return Ok(Conn::Tls(Box::new(rustls::StreamOwned::new(
                            conn, tcp,
                        ))))
                    }
                    Err(_) => return Err("connect_failed"),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::TimedOut => return Err("timeout"),
            Err(_) => continue,
        }
    }
    if Instant::now() >= deadline {
        Err("timeout")
    } else {
        Err("connect_failed")
    }
}

/// A buffered line reader over a `Conn`: reads the status line and header
/// lines one at a time, counting every byte (status line + CRLFs) against
/// the header-block cap. `unknown_encoding` is detected while reading
/// headers line-by-line, before the header-cap check fires for that line.
pub struct HeadReader<'a> {
    conn: &'a mut Conn,
    buf: Vec<u8>,
    pos: usize,
    deadline: Instant,
    /// Total header-block bytes read so far (status line + header lines).
    pub total: u64,
    /// The `Content-Encoding` value of the last such header seen
    /// (lowercased, comma-split tokens), or `None`.
    pub content_encoding: Option<Vec<String>>,
}

impl<'a> HeadReader<'a> {
    pub fn new(conn: &'a mut Conn, deadline: Instant) -> Self {
        HeadReader {
            conn,
            buf: Vec::new(),
            pos: 0,
            deadline,
            total: 0,
            content_encoding: None,
        }
    }

    /// Fill the buffer from the socket (honouring the deadline).
    fn fill(&mut self) -> Result<usize, FetchError> {
        if Instant::now() >= self.deadline {
            return Err("timeout");
        }
        self.conn.set_deadline(self.deadline);
        let mut chunk = [0u8; 8192];
        match self.conn.read(&mut chunk) {
            Ok(0) => Ok(0),
            Ok(n) => {
                self.buf.extend_from_slice(&chunk[..n]);
                Ok(n)
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Err("timeout")
            }
            Err(_) => Err("connect_failed"),
        }
    }

    /// Read one CRLF-terminated line (without the CRLF). Returns `None`
    /// on clean EOF before any byte of the line. The line's bytes
    /// (including CRLF) are added to `total`; the caller checks the cap.
    /// A `Content-Encoding` line is validated here: an unknown encoding
    /// returns `unknown_encoding` before the cap check for that line.
    pub fn read_line(&mut self) -> Result<Option<String>, FetchError> {
        loop {
            if let Some(idx) = find_crlf(&self.buf[self.pos..]) {
                let line_end = self.pos + idx;
                let raw = &self.buf[self.pos..line_end];
                let line = String::from_utf8_lossy(raw).into_owned();
                let consumed = idx + 2; // line + CRLF
                self.pos += consumed;
                self.total += consumed as u64;
                // Compact the buffer occasionally.
                if self.pos > 65536 {
                    self.buf.drain(..self.pos);
                    self.pos = 0;
                }
                if let Some((name, value)) = line.split_once(':') {
                    if name.trim().eq_ignore_ascii_case("content-encoding") {
                        let tokens = parse_encoding(value)?;
                        self.content_encoding = Some(tokens);
                    }
                }
                return Ok(Some(line));
            }
            if self.fill()? == 0 {
                if self.pos < self.buf.len() {
                    // A final line without CRLF (tolerated at EOF).
                    let raw = self.buf[self.pos..].to_vec();
                    let line = String::from_utf8_lossy(&raw).into_owned();
                    self.total += raw.len() as u64;
                    self.pos = self.buf.len();
                    return Ok(Some(line));
                }
                return Ok(None);
            }
        }
    }

    /// The unread buffered bytes (start of the body, already read from
    /// the socket while scanning headers).
    pub fn take_buffered(&mut self) -> Vec<u8> {
        let out = self.buf[self.pos..].to_vec();
        self.buf.clear();
        self.pos = 0;
        out
    }
}

fn find_crlf(buf: &[u8]) -> Option<usize> {
    buf.windows(2).position(|w| w == b"\r\n")
}

/// Parse a `Content-Encoding` header value into its tokens. The only
/// known tokens are `identity`, `gzip`, `deflate`; anything else is
/// `unknown_encoding`.
fn parse_encoding(value: &str) -> Result<Vec<String>, FetchError> {
    let mut out = Vec::new();
    for tok in value.split(',') {
        let t = tok.trim().to_ascii_lowercase();
        if t.is_empty() {
            continue;
        }
        if t != "identity" && t != "gzip" && t != "deflate" {
            return Err("unknown_encoding");
        }
        out.push(t);
    }
    Ok(out)
}

/// The body framing of a 2xx response.
enum Framing {
    /// Exactly N raw bytes follow.
    Length(u64),
    /// Chunked transfer-encoding.
    Chunked,
    /// Read until the connection closes.
    Close,
}

/// Read the raw body bytes for one framing from the connection, honouring
/// the deadline. Returns `body_incomplete` when the connection closes
/// before the declared length, or a chunk is truncated.
fn read_raw_body(
    conn: &mut Conn,
    buffered: Vec<u8>,
    framing: &Framing,
    deadline: Instant,
) -> Result<Vec<u8>, FetchError> {
    let mut out = buffered;
    let mut src = BodySource {
        conn,
        buffered: Vec::new(),
        deadline,
    };
    // The buffered bytes are the first raw body bytes.
    let mut body = std::mem::take(&mut out);
    match framing {
        Framing::Length(n) => {
            let mut remaining = *n as usize;
            if body.len() > remaining {
                body.truncate(remaining);
                return Ok(body);
            }
            remaining -= body.len();
            while remaining > 0 {
                let mut chunk = vec![0u8; remaining.min(65536)];
                let read = src.read_exact_into(&mut chunk)?;
                if read == 0 {
                    return Err("body_incomplete");
                }
                body.extend_from_slice(&chunk[..read]);
                remaining -= read;
            }
            Ok(body)
        }
        Framing::Close => {
            loop {
                let mut chunk = [0u8; 65536];
                let n = src.read_into(&mut chunk)?;
                if n == 0 {
                    break;
                }
                body.extend_from_slice(&chunk[..n]);
            }
            Ok(body)
        }
        Framing::Chunked => {
            // Chunked bodies are read through the chunk reader, which
            // consumes the buffered bytes first.
            let mut reader = ChunkReader {
                src,
                buffered: body,
            };
            let mut out = Vec::new();
            loop {
                let chunk = reader.next_chunk()?;
                if chunk.is_empty() {
                    break;
                }
                out.extend_from_slice(&chunk);
            }
            Ok(out)
        }
    }
}

/// A small read helper over a `Conn` with a deadline.
struct BodySource<'a> {
    conn: &'a mut Conn,
    #[allow(dead_code)]
    buffered: Vec<u8>,
    deadline: Instant,
}

impl<'a> BodySource<'a> {
    fn read_into(&mut self, buf: &mut [u8]) -> Result<usize, FetchError> {
        if Instant::now() >= self.deadline {
            return Err("timeout");
        }
        self.conn.set_deadline(self.deadline);
        match self.conn.read(buf) {
            Ok(n) => Ok(n),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock
                || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                Err("timeout")
            }
            Err(_) => Err("connect_failed"),
        }
    }

    fn read_exact_into(&mut self, buf: &mut [u8]) -> Result<usize, FetchError> {
        let mut done = 0;
        while done < buf.len() {
            let n = self.read_into(&mut buf[done..])?;
            if n == 0 {
                break;
            }
            done += n;
        }
        Ok(done)
    }
}

/// Chunked transfer-encoding reader: `size CRLF data CRLF ... 0 CRLF`.
struct ChunkReader<'a> {
    src: BodySource<'a>,
    buffered: Vec<u8>,
}

impl<'a> ChunkReader<'a> {
    /// Read one chunk (empty at the terminal 0 chunk).
    fn next_chunk(&mut self) -> Result<Vec<u8>, FetchError> {
        let size_line = self.read_line_raw()?;
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| "body_incomplete")?;
        if size == 0 {
            // Consume the trailing CRLF after the 0 chunk (and any
            // trailers up to the blank line).
            loop {
                let line = self.read_line_raw()?;
                if line.is_empty() {
                    break;
                }
            }
            return Ok(Vec::new());
        }
        let mut out = vec![0u8; size];
        let mut done = 0;
        while done < size {
            // Serve from the buffered bytes first.
            if !self.buffered.is_empty() {
                let take = self.buffered.len().min(size - done);
                out[done..done + take]
                    .copy_from_slice(&self.buffered[..take]);
                self.buffered.drain(..take);
                done += take;
                continue;
            }
            let n = self.src.read_into(&mut out[done..])?;
            if n == 0 {
                return Err("body_incomplete");
            }
            done += n;
        }
        // The CRLF after the chunk data.
        let mut crlf = [0u8; 2];
        let mut done = 0;
        while done < 2 {
            if !self.buffered.is_empty() {
                crlf[done] = self.buffered[0];
                self.buffered.drain(..1);
                done += 1;
                continue;
            }
            let n = self.src.read_into(&mut crlf[done..])?;
            if n == 0 {
                return Err("body_incomplete");
            }
            done += n;
        }
        Ok(out)
    }

    /// Read one CRLF-terminated line from the chunked stream (buffered
    /// bytes first, then the socket).
    fn read_line_raw(&mut self) -> Result<String, FetchError> {
        let mut line = Vec::new();
        loop {
            if let Some(idx) = find_crlf(&self.buffered) {
                let raw = self.buffered[..idx].to_vec();
                self.buffered.drain(..idx + 2);
                return Ok(String::from_utf8_lossy(&raw).into_owned());
            }
            // No CRLF in the buffer yet; move what we have into the line
            // and read more (a CRLF split across reads is handled by
            // keeping the last byte back).
            if self.buffered.len() > 1 {
                let keep = self.buffered.len() - 1;
                line.extend_from_slice(&self.buffered[..keep]);
                self.buffered.drain(..keep);
            }
            let mut chunk = [0u8; 8192];
            let n = self.src.read_into(&mut chunk)?;
            if n == 0 {
                if line.is_empty() && self.buffered.is_empty() {
                    return Err("body_incomplete");
                }
                // EOF mid-line: return what we have.
                let mut tail = std::mem::take(&mut line);
                tail.extend_from_slice(&self.buffered);
                self.buffered.clear();
                return Ok(String::from_utf8_lossy(&tail).into_owned());
            }
            self.buffered.extend_from_slice(&chunk[..n]);
            // Re-check for a CRLF now that the buffer grew (loop top).
            if let Some(idx) = find_crlf(&self.buffered) {
                let mut raw = std::mem::take(&mut line);
                raw.extend_from_slice(&self.buffered[..idx]);
                self.buffered.drain(..idx + 2);
                return Ok(String::from_utf8_lossy(&raw).into_owned());
            }
            // If the buffer is huge without a CRLF, keep accumulating
            // into the line (a chunk-size line is short in practice).
            if self.buffered.len() > 8192 {
                line.extend_from_slice(&self.buffered);
                self.buffered.clear();
            }
        }
    }
}

/// The result of one request in the redirect chain.
enum Once {
    /// 2xx with the decoded body.
    Ok(Vec<u8>),
    /// A redirect (3xx with a Location header).
    Redirect(String),
    Err(FetchError),
}

/// Decode `raw` with `encoding` (`identity`/`gzip`/`deflate`), capping
/// the decoded output at `max_fetch_bytes` (+1 to prove oversize).
fn decode_with(raw: Vec<u8>, encoding: &[String], max_fetch_bytes: u64) -> Once {
    let single = encoding.last().cloned().unwrap_or_else(|| "identity".to_string());
    // A multi-token list other than a single known token is not a
    // supported encoding (the header reader already rejected unknown
    // tokens; a list like `gzip, deflate` has no defined decode order).
    if encoding.len() > 1 {
        return Once::Err("unknown_encoding");
    }
    match single.as_str() {
        "identity" => {
            if raw.len() as u64 > max_fetch_bytes {
                return Once::Err("body_too_large");
            }
            Once::Ok(raw)
        }
        "gzip" => decode_stream(
            flate2::read::GzDecoder::new(std::io::Cursor::new(raw)),
            max_fetch_bytes,
        ),
        "deflate" => decode_stream(
            flate2::read::DeflateDecoder::new(std::io::Cursor::new(raw)),
            max_fetch_bytes,
        ),
        _ => Once::Err("unknown_encoding"),
    }
}

/// Stream-decode `reader`, capping the decoded output at
/// `max_fetch_bytes` (+1 to prove oversize). A corrupt stream is
/// `decode_failed`.
fn decode_stream<R: Read>(mut reader: R, max_fetch_bytes: u64) -> Once {
    let mut out = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                out.extend_from_slice(&chunk[..n]);
                if out.len() as u64 > max_fetch_bytes {
                    return Once::Err("body_too_large");
                }
            }
            Err(_) => return Once::Err("decode_failed"),
        }
    }
    Once::Ok(out)
}

/// Parse an HTTP status line (`HTTP/1.1 200 OK`) into its status code.
fn parse_status(line: &str) -> Option<u16> {
    let mut parts = line.split_whitespace();
    let _version = parts.next()?;
    let code = parts.next()?;
    if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    code.parse().ok()
}

/// Fetch `url` with the redirect chain, egress checks, and caps.
/// `--fetch-timeout-secs` bounds the whole chain, wall-clock from before
/// the first DNS query.
pub fn fetch_url(url: &str, cfg: &FetchConfig) -> FetchOutcome {
    let deadline = Instant::now() + Duration::from_secs(cfg.fetch_timeout_secs.max(1));
    let mut current = url.to_string();
    let mut redirects: u64 = 0;
    loop {
        let Some(p) = parse_url(&current) else {
            return FetchOutcome::Err("scheme_not_allowed");
        };
        if p.scheme != "http" && p.scheme != "https" {
            return FetchOutcome::Err("scheme_not_allowed");
        }
        let ips = match egress_check(&p, cfg, deadline) {
            Ok(ips) => ips,
            Err(e) => return FetchOutcome::Err(e),
        };
        // fetch_once needs the Content-Encoding for the decode; run the
        // exchange and decode inline here so the encoding is available.
        match fetch_once_decoded(&p, &ips, cfg, deadline) {
            Once::Ok(body) => {
                return FetchOutcome::Ok {
                    body,
                    final_url: current,
                }
            }
            Once::Redirect(location) => {
                if redirects >= cfg.max_redirects {
                    return FetchOutcome::Err("too_many_redirects");
                }
                redirects += 1;
                current = resolve_url(&current, &location);
            }
            Once::Err(e) => return FetchOutcome::Err(e),
        }
    }
}

/// Like `fetch_once`, but threads the recorded `Content-Encoding`
/// through to the decoder.
fn fetch_once_decoded(
    p: &ParsedUrl,
    ips: &[IpAddr],
    cfg: &FetchConfig,
    deadline: Instant,
) -> Once {
    let https = p.scheme == "https";
    let mut conn = match connect(&p.host, p.port, ips, https, deadline) {
        Ok(c) => c,
        Err(e) => return Once::Err(e),
    };
    let host_header = if (https && p.port == 443) || (!https && p.port == 80) {
        p.host.clone()
    } else if p.host.contains(':') {
        format!("[{}]:{}", p.host, p.port)
    } else {
        format!("{}:{}", p.host, p.port)
    };
    let request = format!(
        "GET {} HTTP/1.1\r\nHost: {}\r\nUser-Agent: s3a-kite/0.1.0\r\nAccept-Encoding: gzip, deflate\r\nConnection: close\r\n\r\n",
        p.path, host_header
    );
    conn.set_deadline(deadline);
    if let Err(e) = conn.write_all(request.as_bytes()).and_then(|_| conn.flush()) {
        if e.kind() == std::io::ErrorKind::WouldBlock
            || e.kind() == std::io::ErrorKind::TimedOut
            || Instant::now() >= deadline
        {
            return Once::Err("timeout");
        }
        return Once::Err("connect_failed");
    }
    let mut head = HeadReader::new(&mut conn, deadline);
    let status_line = match head.read_line() {
        Ok(Some(l)) => l,
        Ok(None) => return Once::Err("connect_failed"),
        Err(e) => return Once::Err(e),
    };
    if head.total > cfg.max_fetch_bytes {
        return Once::Err("headers_too_large");
    }
    let Some(status) = parse_status(&status_line) else {
        return Once::Err("connect_failed");
    };
    let mut location: Option<String> = None;
    let mut content_length: Option<u64> = None;
    let mut chunked = false;
    loop {
        let line = match head.read_line() {
            Ok(Some(l)) => l,
            Ok(None) => return Once::Err("body_incomplete"),
            Err(e) => return Once::Err(e),
        };
        if head.total > cfg.max_fetch_bytes {
            return Once::Err("headers_too_large");
        }
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let name = name.trim();
        let value = value.trim();
        if name.eq_ignore_ascii_case("location") {
            location = Some(value.to_string());
        } else if name.eq_ignore_ascii_case("content-length") {
            content_length = value.parse::<u64>().ok();
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            if value
                .split(',')
                .any(|t| t.trim().eq_ignore_ascii_case("chunked"))
            {
                chunked = true;
            }
        }
    }
    if matches!(status, 301 | 302 | 303 | 307 | 308) {
        if let Some(loc) = location {
            if !loc.is_empty() {
                return Once::Redirect(loc);
            }
        }
        return Once::Err("http_status");
    }
    if !(200..300).contains(&status) {
        return Once::Err("http_status");
    }
    let framing = if chunked {
        Framing::Chunked
    } else if let Some(n) = content_length {
        Framing::Length(n)
    } else {
        Framing::Close
    };
    let encoding = head.content_encoding.clone().unwrap_or_default();
    let buffered = head.take_buffered();
    drop(head);
    let raw = match read_raw_body(&mut conn, buffered, &framing, deadline) {
        Ok(b) => b,
        Err(e) => return Once::Err(e),
    };
    decode_with(raw, &encoding, cfg.max_fetch_bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_url_forms() {
        let p = parse_url("http://127.0.0.1:8080/paper-1").unwrap();
        assert_eq!(p.scheme, "http");
        assert_eq!(p.host, "127.0.0.1");
        assert_eq!(p.port, 8080);
        assert_eq!(p.path, "/paper-1");
        let p = parse_url("https://example.test/a/b?x=1").unwrap();
        assert_eq!(p.port, 443);
        assert_eq!(p.path, "/a/b?x=1");
        let p = parse_url("http://[::1]:9000/x").unwrap();
        assert_eq!(p.host, "::1");
        assert_eq!(p.port, 9000);
        assert!(parse_url("ftp://example.test/x").is_some());
        assert!(parse_url("example.test/x").is_none());
        assert!(parse_url("http://user@example.test/").is_none());
    }

    #[test]
    fn resolve_url_forms() {
        assert_eq!(
            resolve_url("http://127.0.0.1:9/c0", "/c1"),
            "http://127.0.0.1:9/c1"
        );
        assert_eq!(
            resolve_url("http://127.0.0.1:9/c0", "ftp://example.test/x"),
            "ftp://example.test/x"
        );
        assert_eq!(
            resolve_url("http://127.0.0.1:9/a/b", "c"),
            "http://127.0.0.1:9/a/c"
        );
    }

    #[test]
    fn encoding_tokens() {
        assert_eq!(parse_encoding("gzip").unwrap(), vec!["gzip"]);
        assert_eq!(parse_encoding("identity").unwrap(), vec!["identity"]);
        assert!(parse_encoding("br").is_err());
        assert!(parse_encoding("gzip, br").is_err());
    }
}
