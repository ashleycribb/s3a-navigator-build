//! Shared host-spawn + HTTP harness for step-9 tests.
//!
//! The host binary comes from `S3A_KITE_BIN` (default build, no
//! test features) or `S3A_KITE_BIN_HOOKS` (features
//! `native-window,test_hooks,egress_test`); each test fails naming the
//! variable when it is unset. The s3a-cli oracle (AT-10 pattern) is not
//! used by the host tests.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub const TOKEN: &str = "test-token-0123456789abcdefghij0123456789";
pub const MAX_OPEN_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_FRAME_PIXELS: u64 = 2_000_000;
pub const MAX_REQUEST_BYTES: u64 = 1024 * 1024;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn host_bin() -> String {
    env_bin(
        "S3A_KITE_BIN",
        "a built s3a-kite host binary (cargo build -p s3a-kite)",
    )
}

pub fn host_bin_hooks() -> String {
    env_bin(
        "S3A_KITE_BIN_HOOKS",
        "a built s3a-kite host binary with features native-window,test_hooks,egress_test",
    )
}

fn env_bin(var: &str, what: &str) -> String {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => v,
        _ => panic!("{var} is not set: set it to {what}"),
    }
}

/// A temp dir that is removed on drop.
pub struct TempDir {
    pub path: PathBuf,
}

impl TempDir {
    pub fn new(name: &str) -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "kite-host-{name}-{}-{}",
            std::process::id(),
            n
        ));
        std::fs::create_dir_all(&path).unwrap();
        // The sandbox umask is 0007, so fresh dirs are group-writable
        // (0770) — the host's AT-174 Unix-socket parent check rejects a
        // group-writable parent. Test dirs are private (0700).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        TempDir { path }
    }

    pub fn join(&self, rel: &str) -> PathBuf {
        self.path.join(rel)
    }

    pub fn mkdir(&self, rel: &str) -> PathBuf {
        let p = self.join(rel);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    pub fn write(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A free loopback port (bind 127.0.0.1:0, read the port, drop).
pub fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

/// One HTTP response.
#[derive(Debug)]
pub struct Resp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Resp {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|e| panic!("response is not JSON ({}): {}", e, self.text()))
    }

    pub fn error_code(&self) -> String {
        self.json()
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    }

    pub fn header(&self, name: &str) -> Option<String> {
        let lower = name.to_ascii_lowercase();
        self.headers
            .iter()
            .find(|(n, _)| *n == lower)
            .map(|(_, v)| v.clone())
    }
}

fn parse_response(raw: &[u8]) -> Resp {
    let text = String::from_utf8_lossy(raw);
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("").as_bytes().to_vec();
    let mut lines = head.lines();
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let mut headers = Vec::new();
    for line in lines {
        if let Some((n, v)) = line.split_once(':') {
            headers.push((n.trim().to_ascii_lowercase(), v.trim().to_string()));
        }
    }
    Resp {
        status,
        headers,
        body,
    }
}

/// A running host. Killed (SIGTERM, then SIGKILL) on drop.
pub struct RunningHost {
    pub child: Child,
    pub dir: TempDir,
    pub port: u16,
    pub socket_path: Option<PathBuf>,
    pub token: String,
    pub view_root: PathBuf,
    pub wiki_root: PathBuf,
    pub corpus_root: PathBuf,
    pub snapshot_dir: PathBuf,
    pub download_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub token_file: PathBuf,
    pub tls: bool,
}

impl RunningHost {
    /// Spawn a viewer host on a loopback port. `extra` are additional
    /// argv entries (e.g. `--behind-proxy`); `env` are extra env vars.
    pub fn viewer(extra: &[&str], env: &[(&str, &str)]) -> RunningHost {
        Self::spawn(Mode::Viewer, extra, env, false, 60, 16, None, None, true)
    }

    /// Viewer host with a custom handle-idle-secs (AT-149/AT-230 tests).
    pub fn viewer_idle(idle_secs: u64) -> RunningHost {
        Self::spawn(Mode::Viewer, &[], &[], false, idle_secs, 16, None, None, true)
    }

    /// Viewer host with a custom max-open-handles (AT-118/AT-231 tests).
    pub fn viewer_max_handles(max_handles: u64) -> RunningHost {
        Self::spawn(Mode::Viewer, &[], &[], false, 60, max_handles, None, None, true)
    }

    /// Spawn a Scholar host on a loopback port.
    pub fn scholar(extra: &[&str], env: &[(&str, &str)]) -> RunningHost {
        Self::spawn(Mode::Scholar, extra, env, false, 60, 16, None, None, true)
    }

    /// Scholar host with a custom handle-idle-secs.
    pub fn scholar_idle(idle_secs: u64) -> RunningHost {
        Self::spawn(Mode::Scholar, &[], &[], false, idle_secs, 16, None, None, true)
    }

    /// Scholar host whose temp dirs are set up by `setup` (run after the
    /// dirs are created, before the child is spawned) — for AT-187 and
    /// other pre-startup filesystem fixtures.
    pub fn scholar_with_setup(setup: impl Fn(&TempDir)) -> RunningHost {
        Self::spawn(Mode::Scholar, &[], &[], false, 60, 16, Some(&setup), None, true)
    }

    /// Spawn a Scholar host with an LRS configured (consent file +
    /// credentials). The LRS URL points at a loopback http URL that need
    /// not serve anything in Step 9 (no LRS client yet).
    pub fn scholar_with_lrs(extra: &[&str], env: &[(&str, &str)]) -> RunningHost {
        Self::spawn(Mode::ScholarLrs, extra, env, false, 60, 16, None, None, true)
    }

    /// Spawn a host on a Unix socket instead of TCP.
    pub fn viewer_unix(extra: &[&str], env: &[(&str, &str)]) -> RunningHost {
        Self::spawn(Mode::ViewerUnix, extra, env, false, 60, 16, None, None, true)
    }

    pub fn scholar_unix(extra: &[&str], env: &[(&str, &str)]) -> RunningHost {
        Self::spawn(Mode::ScholarUnix, extra, env, false, 60, 16, None, None, true)
    }

    /// Spawn a Scholar host on a non-loopback bind with TLS (uses the
    /// given cert/key files, which must already exist).
    pub fn scholar_tls(cert: &Path, key: &Path, extra: &[&str], env: &[(&str, &str)]) -> RunningHost {
        let mut full_extra: Vec<String> = vec![
            "--tls-cert".to_string(),
            cert.to_string_lossy().into_owned(),
            "--tls-key".to_string(),
            key.to_string_lossy().into_owned(),
        ];
        for e in extra {
            full_extra.push(e.to_string());
        }
        let refs: Vec<&str> = full_extra.iter().map(|s| s.as_str()).collect();
        Self::spawn(Mode::ScholarTls, &refs, env, true, 60, 16, None, None, true)
    }

    /// Scholar host from the hooks binary (native-window + test_hooks
    /// features), without --headless, with a fake DISPLAY and the
    /// window_issue_select hook armed (AT-125). `setup` runs before the
    /// child is spawned — it writes the first-candidate dossier.
    pub fn scholar_window(setup: impl Fn(&TempDir)) -> RunningHost {
        Self::spawn(
            Mode::Scholar,
            &[],
            &[
                ("DISPLAY", ":99"),
                ("S3A_KITE_TEST_HOOKS", "window_issue_select=issue"),
            ],
            false,
            60,
            16,
            Some(&setup),
            Some(host_bin_hooks()),
            false,
        )
    }

    fn spawn(
        mode: Mode,
        extra: &[&str],
        env: &[(&str, &str)],
        tls: bool,
        idle_secs: u64,
        max_handles: u64,
        setup: Option<&dyn Fn(&TempDir)>,
        bin: Option<String>,
        headless: bool,
    ) -> RunningHost {
        let bin = bin.unwrap_or_else(host_bin);
        let dir = TempDir::new(match mode {
            Mode::Viewer | Mode::ViewerUnix => "viewer",
            _ => "scholar",
        });
        let view_root = dir.mkdir("view");
        let wiki_root = dir.mkdir("wiki");
        let corpus_root = dir.mkdir("corpus");
        let snapshot_dir = dir.mkdir("snapshot");
        let download_dir = dir.mkdir("download");
        let data_dir = dir.mkdir("data");
        let log_dir = dir.mkdir("log");
        let token_file = dir.write("token.txt", format!("{TOKEN}\n").as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        if let Some(setup) = setup {
            setup(&dir);
        }
        let port = if matches!(mode, Mode::ViewerUnix | Mode::ScholarUnix) {
            0
        } else {
            free_port()
        };
        let socket_path = if matches!(mode, Mode::ViewerUnix | Mode::ScholarUnix) {
            Some(dir.join("host.sock"))
        } else {
            None
        };
        let mut args: Vec<String> = Vec::new();
        if matches!(mode, Mode::Viewer | Mode::ViewerUnix) {
            args.push("--viewer".to_string());
        }
        let bind = match (&socket_path, tls) {
            (Some(p), _) => format!("unix:{}", p.display()),
            (None, true) => "0.0.0.0".to_string(),
            (None, false) => "127.0.0.1".to_string(),
        };
        args.push("--bind".to_string());
        args.push(bind);
        if socket_path.is_none() {
            args.push("--port".to_string());
            args.push(port.to_string());
        }
        args.push("--auth-token-file".to_string());
        args.push(token_file.to_string_lossy().into_owned());
        args.push("--log-dir".to_string());
        args.push(log_dir.to_string_lossy().into_owned());
        args.push("--max-open-handles".to_string());
        args.push(max_handles.to_string());
        args.push("--max-frame-pixels".to_string());
        args.push(MAX_FRAME_PIXELS.to_string());
        args.push("--handle-idle-secs".to_string());
        args.push(idle_secs.to_string());
        args.push("--max-request-bytes".to_string());
        args.push(MAX_REQUEST_BYTES.to_string());
        args.push("--max-open-bytes".to_string());
        args.push(MAX_OPEN_BYTES.to_string());
        if headless {
            args.push("--headless".to_string());
        }
        match mode {
            Mode::Viewer | Mode::ViewerUnix => {
                args.push("--view-root".to_string());
                args.push(view_root.to_string_lossy().into_owned());
            }
            _ => {
                args.push("--wiki-root".to_string());
                args.push(wiki_root.to_string_lossy().into_owned());
                args.push("--corpus-root".to_string());
                args.push(corpus_root.to_string_lossy().into_owned());
                args.push("--working-archive".to_string());
                // DD-93: the working archive's parent is a write dir and
                // must be disjoint from every root, so the archive lives
                // in its own dedicated directory.
                let working_dir = dir.mkdir("working");
                args.push(working_dir.join("working.s3a").to_string_lossy().into_owned());
                args.push("--snapshot-dir".to_string());
                args.push(snapshot_dir.to_string_lossy().into_owned());
                args.push("--download-dir".to_string());
                args.push(download_dir.to_string_lossy().into_owned());
                args.push("--data-dir".to_string());
                args.push(data_dir.to_string_lossy().into_owned());
                args.push("--researcher-id".to_string());
                args.push("https://example.test/researcher/ashley".to_string());
                args.push("--fetch-timeout-secs".to_string());
                args.push("30".to_string());
                args.push("--max-fetch-bytes".to_string());
                args.push(MAX_OPEN_BYTES.to_string());
                args.push("--max-redirects".to_string());
                args.push("5".to_string());
                if matches!(mode, Mode::ScholarLrs) {
                    let consent = dir.join("consent.txt");
                    let creds = dir.write("lrs-creds.txt", b"lrs-user:lrs-pass");
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600))
                            .unwrap();
                    }
                    args.push("--lrs-url".to_string());
                    args.push("http://127.0.0.1:9/xapi".to_string());
                    args.push("--consent-state-file".to_string());
                    args.push(consent.to_string_lossy().into_owned());
                    args.push("--lrs-credentials-file".to_string());
                    args.push(creds.to_string_lossy().into_owned());
                    args.push("--lrs-timeout-secs".to_string());
                    args.push("5".to_string());
                }
            }
        }
        for e in extra {
            args.push(e.to_string());
        }
        let mut cmd = Command::new(&bin);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = cmd.spawn().unwrap_or_else(|e| {
            panic!("spawn {bin} {}: {e}", args.join(" "))
        });
        let mut host = RunningHost {
            child,
            dir,
            port,
            socket_path,
            token: TOKEN.to_string(),
            view_root,
            wiki_root,
            corpus_root,
            snapshot_dir,
            download_dir,
            data_dir,
            log_dir,
            token_file,
            tls,
        };
        // Wait for /health (or for the process to exit).
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(Some(status)) = host.child.try_wait() {
                panic!(
                    "host exited during startup (status {status}); log: {}",
                    host.log()
                );
            }
            if Instant::now() > deadline {
                panic!("host did not serve /health within 15s; log: {}", host.log());
            }
            // Poll with the token: /health requires it behind a proxy
            // (AT-87), and a token is accepted even when it is not
            // required. A TLS host is polled over TLS.
            let up = if host.tls {
                let req = format!(
                    "GET /health HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\n\r\n"
                );
                host.tls_roundtrip_result(req.as_bytes())
                    .map(|r| r.status == 200 && r.body == b"ok\n")
                    .unwrap_or(false)
            } else {
                host.get_raw_result(
                    "/health",
                    &[("Authorization", &format!("Bearer {TOKEN}"))],
                )
                .map(|r| r.status == 200 && r.body == b"ok\n")
                .unwrap_or(false)
            };
            if up {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        host
    }

    /// Wait for the process to exit (up to `timeout`), returning its exit
    /// code, or None on timeout.
    pub fn wait_exit(&mut self, timeout: Duration) -> Option<i32> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Ok(Some(status)) = self.child.try_wait() {
                return status.code();
            }
            if Instant::now() > deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// The base URL path prefix for requests (empty for TCP; the socket
    /// path is used instead).
    fn connect(&self) -> Box<dyn ReadWrite> {
        self.connect_result().expect("connect to host")
    }

    fn connect_result(&self) -> std::io::Result<Box<dyn ReadWrite>> {
        if let Some(path) = &self.socket_path {
            let s = UnixStream::connect(path)?;
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            s.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
            Ok(Box::new(s))
        } else {
            let s = TcpStream::connect(("127.0.0.1", self.port))?;
            s.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
            s.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
            Ok(Box::new(s))
        }
    }

    /// One raw HTTP round-trip (Connection: close). `request` is the full
    /// request bytes. Panics on a connection error.
    pub fn roundtrip(&self, request: &[u8]) -> Resp {
        self.roundtrip_result(request)
            .expect("connect to host and read the response")
    }

    /// Like `roundtrip`, but a connection error is returned so the startup
    /// wait loop can retry while the host is still binding.
    pub fn roundtrip_result(&self, request: &[u8]) -> std::io::Result<Resp> {
        let mut stream = self.connect_result()?;
        // A server that rejects an over-cap body closes the connection
        // before the client finishes writing; the write may fail with a
        // broken pipe while the 413 response is still readable, so a
        // write error is tolerated when the read succeeds.
        let write_result = stream.write_all(request);
        let mut raw = Vec::new();
        let read_result = stream.read_to_end(&mut raw);
        match (write_result, read_result) {
            (_, Ok(_)) => Ok(parse_response(&raw)),
            (Err(e), Err(_)) => Err(e),
            (Ok(()), Err(e)) => Err(e),
        }
    }

    pub fn get_raw(&self, path: &str, extra_headers: &[(&str, &str)]) -> Result<Resp, String> {
        let mut req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n");
        for (n, v) in extra_headers {
            req.push_str(&format!("{n}: {v}\r\n"));
        }
        req.push_str("Connection: close\r\n\r\n");
        Ok(self.roundtrip(req.as_bytes()))
    }

    /// Like `get_raw`, but a connection error is returned instead of
    /// panicking (the startup wait loop uses this).
    pub fn get_raw_result(
        &self,
        path: &str,
        extra_headers: &[(&str, &str)],
    ) -> std::io::Result<Resp> {
        let mut req = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n");
        for (n, v) in extra_headers {
            req.push_str(&format!("{n}: {v}\r\n"));
        }
        req.push_str("Connection: close\r\n\r\n");
        self.roundtrip_result(req.as_bytes())
    }

    pub fn get(&self, path: &str) -> Resp {
        self.get_raw(path, &[("Authorization", &format!("Bearer {}", self.token))])
            .unwrap()
    }

    pub fn post_raw(
        &self,
        path: &str,
        content_type: Option<&str>,
        body: &[u8],
    ) -> Resp {
        let mut req = format!("POST {path} HTTP/1.1\r\nHost: localhost\r\n");
        if let Some(ct) = content_type {
            req.push_str(&format!("Content-Type: {ct}\r\n"));
        }
        req.push_str(&format!(
            "Authorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            self.token,
            body.len()
        ));
        let mut bytes = req.into_bytes();
        bytes.extend_from_slice(body);
        self.roundtrip(&bytes)
    }

    pub fn post(&self, path: &str, body: &serde_json::Value) -> Resp {
        let bytes = serde_json::to_vec(body).unwrap();
        self.post_raw(path, Some("application/json"), &bytes)
    }

    /// Open a raw TCP/Unix connection for keep-alive and idle tests.
    pub fn raw_connection(&self) -> Box<dyn ReadWrite> {
        self.connect()
    }

    /// Read one HTTP response from a raw connection (used with
    /// `raw_connection` for keep-alive tests).
    pub fn read_response(stream: &mut dyn ReadWrite) -> Resp {
        let mut raw = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    if let Some(end) = find_header_end(&raw) {
                        let content_len = header_content_length(&raw[..end]);
                        if raw.len() >= end + content_len {
                            break;
                        }
                    }
                }
                Err(_) => break,
            }
        }
        parse_response(&raw)
    }

    /// TLS round-trip (accept-any verifier) for the TLS host.
    pub fn tls_roundtrip(&self, request: &[u8]) -> Resp {
        self.tls_roundtrip_result(request)
            .expect("TLS round-trip to the host")
    }

    /// Like `tls_roundtrip`, but a connection/handshake error is returned
    /// (the startup wait loop uses this).
    pub fn tls_roundtrip_result(&self, request: &[u8]) -> Result<Resp, String> {
        let raw = tls_exchange(("127.0.0.1", self.port), request)?;
        Ok(parse_response(&raw))
    }

    pub fn tls_get(&self, path: &str) -> Resp {
        let req = format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
            self.token
        );
        self.tls_roundtrip(req.as_bytes())
    }

    /// The host's log file contents.
    pub fn log(&self) -> String {
        std::fs::read_to_string(self.log_dir.join("s3a-kite.log")).unwrap_or_default()
    }
}

fn find_header_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

fn header_content_length(head: &[u8]) -> usize {
    let text = String::from_utf8_lossy(head);
    for line in text.lines() {
        if let Some((n, v)) = line.split_once(':') {
            if n.trim().eq_ignore_ascii_case("content-length") {
                return v.trim().parse().unwrap_or(0);
            }
        }
    }
    0
}

pub trait ReadWrite: Read + Write {}
impl ReadWrite for TcpStream {}
impl ReadWrite for UnixStream {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Viewer,
    ViewerUnix,
    Scholar,
    ScholarUnix,
    ScholarLrs,
    ScholarTls,
}

/// Run the host binary once and expect it to exit (startup-error tests).
/// Returns (exit code, stderr). Panics if it does not exit in time.
pub fn run_expect_exit(
    args: &[String],
    env: &[(&str, &str)],
    timeout: Duration,
) -> (i32, String) {
    let bin = host_bin();
    let mut cmd = Command::new(&bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap_or_else(|e| panic!("spawn {bin}: {e}"));
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            return (status.code().unwrap_or(-1), err);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("host did not exit within {timeout:?} (args: {})", args.join(" "));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Like `run_expect_exit` but with the hooks binary, and with env vars
/// that can be removed as well as set (AT-125: the no-DISPLAY startup
/// error needs DISPLAY and WAYLAND_DISPLAY absent).
pub fn run_expect_exit_hooks(
    args: &[String],
    env: &[(&str, &str)],
    remove_env: &[&str],
    timeout: Duration,
) -> (i32, String) {
    let bin = host_bin_hooks();
    let mut cmd = Command::new(&bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    for k in remove_env {
        cmd.env_remove(k);
    }
    let mut child = cmd.spawn().unwrap_or_else(|e| panic!("spawn {bin}: {e}"));
    let deadline = Instant::now() + timeout;
    loop {
        if let Ok(Some(status)) = child.try_wait() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            return (status.code().unwrap_or(-1), err);
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("host did not exit within {timeout:?} (args: {})", args.join(" "));
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The viewer-mode argv for a host rooted at `dir` (for startup-error
/// tests that build their own argv).
pub fn viewer_argv(dir: &TempDir, port: u16) -> Vec<String> {
    let view_root = dir.mkdir("view");
    let log_dir = dir.mkdir("log");
    let token_file = dir.write("token.txt", format!("{TOKEN}\n").as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    vec![
        "--viewer".into(),
        "--bind".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--auth-token-file".into(),
        token_file.to_string_lossy().into_owned(),
        "--log-dir".into(),
        log_dir.to_string_lossy().into_owned(),
        "--max-open-handles".into(),
        "16".into(),
        "--max-frame-pixels".into(),
        MAX_FRAME_PIXELS.to_string(),
        "--handle-idle-secs".into(),
        "60".into(),
        "--max-request-bytes".into(),
        MAX_REQUEST_BYTES.to_string(),
        "--max-open-bytes".into(),
        MAX_OPEN_BYTES.to_string(),
        "--headless".into(),
        "--view-root".into(),
        view_root.to_string_lossy().into_owned(),
    ]
}
/// Returns (cert_path, key_path). Panics if openssl is missing.
/// The paths a Scholar argv builder creates.
pub struct ScholarPaths {
    pub wiki: PathBuf,
    pub corpus: PathBuf,
    pub snapshot: PathBuf,
    pub download: PathBuf,
    pub data: PathBuf,
    pub log: PathBuf,
    pub token: PathBuf,
    pub working_archive: PathBuf,
}

/// Build a full Scholar argv rooted at `dir` (for startup-error tests
/// that vary one flag). Returns (argv, paths).
pub fn scholar_argv(dir: &TempDir, port: u16) -> (Vec<String>, ScholarPaths) {
    let wiki = dir.mkdir("wiki");
    let corpus = dir.mkdir("corpus");
    let snapshot = dir.mkdir("snap");
    let download = dir.mkdir("dl");
    let data = dir.mkdir("data");
    let log = dir.mkdir("log");
    let token = dir.write("token.txt", format!("{TOKEN}\n").as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    // DD-93: the working archive's parent is a write dir and must be
    // disjoint from every root, so the archive lives in its own dir.
    let working_dir = dir.mkdir("working");
    let working_archive = working_dir.join("working.s3a");
    let argv: Vec<String> = vec![
        "--bind".into(),
        "127.0.0.1".into(),
        "--port".into(),
        port.to_string(),
        "--auth-token-file".into(),
        token.to_string_lossy().into_owned(),
        "--log-dir".into(),
        log.to_string_lossy().into_owned(),
        "--max-open-handles".into(),
        "16".into(),
        "--max-frame-pixels".into(),
        MAX_FRAME_PIXELS.to_string(),
        "--handle-idle-secs".into(),
        "60".into(),
        "--max-request-bytes".into(),
        MAX_REQUEST_BYTES.to_string(),
        "--max-open-bytes".into(),
        MAX_OPEN_BYTES.to_string(),
        "--headless".into(),
        "--wiki-root".into(),
        wiki.to_string_lossy().into_owned(),
        "--corpus-root".into(),
        corpus.to_string_lossy().into_owned(),
        "--working-archive".into(),
        working_archive.to_string_lossy().into_owned(),
        "--snapshot-dir".into(),
        snapshot.to_string_lossy().into_owned(),
        "--download-dir".into(),
        download.to_string_lossy().into_owned(),
        "--data-dir".into(),
        data.to_string_lossy().into_owned(),
        "--researcher-id".into(),
        "https://example.test/researcher/ashley".into(),
        "--fetch-timeout-secs".into(),
        "30".into(),
        "--max-fetch-bytes".into(),
        MAX_OPEN_BYTES.to_string(),
        "--max-redirects".into(),
        "5".into(),
    ];
    (
        argv,
        ScholarPaths {
            wiki,
            corpus,
            snapshot,
            download,
            data,
            log,
            token,
            working_archive,
        },
    )
}

/// A KiteContext with dummy font bytes (the pack's paint is rect-fill;
/// no glyph rasterizer is wired, so any non-empty bytes satisfy the
/// REQUIRED_UA_FONTS check). Same approach as kite-pack-tests.
pub fn test_context() -> s3a_pack_kite::KiteContext {
    const REQUIRED_UA_FONTS: &[&str] = &[
        "Noto Sans",
        "Noto Sans Italic",
        "Noto Serif",
        "Noto Sans Mono",
        "Noto Sans Symbols",
        "Noto Sans SC",
        "Noto Emoji",
    ];
    let fonts = REQUIRED_UA_FONTS
        .iter()
        .map(|name| ((*name).to_string(), format!("{name} fixture bytes").into_bytes()));
    s3a_pack_kite::KiteContext::new(fonts).expect("complete required UA font set")
}

pub fn make_test_cert(dir: &TempDir) -> (PathBuf, PathBuf) {
    let cert = dir.join("test-cert.pem");
    let key = dir.join("test-key.pem");
    let out = Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-keyout",
            key.to_str().unwrap(),
            "-out",
            cert.to_str().unwrap(),
            "-days",
            "2",
            "-nodes",
            "-subj",
            "/CN=localhost",
        ])
        .output()
        .unwrap_or_else(|e| panic!("openssl is required for the TLS tests: {e}"));
    assert!(
        out.status.success(),
        "openssl req failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Secret files must be euid-owned mode 0600 (AT-45/AT-81/AT-142);
        // openssl creates the cert 0660 under this sandbox's umask.
        std::fs::set_permissions(&cert, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    (cert, key)
}

/// A rustls client that accepts any server certificate (test only).
/// Returns the raw response bytes, or a string error.
pub fn tls_exchange(addr: (&str, u16), request: &[u8]) -> Result<Vec<u8>, String> {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, Error as TlsError};

    #[derive(Debug)]
    struct AcceptAny;
    impl rustls::client::danger::ServerCertVerifier for AcceptAny {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, TlsError> {
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TlsError> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TlsError> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ED25519,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
            ]
        }
    }

    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(AcceptAny))
        .with_no_client_auth();
    let server_name = ServerName::IpAddress(
        rustls::pki_types::IpAddr::try_from(addr.0).map_err(|e| e.to_string())?,
    );
    let conn = ClientConnection::new(std::sync::Arc::new(config), server_name)
        .map_err(|e| e.to_string())?;
    let stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(|e| e.to_string())?;
    // StreamOwned takes the connection by value: ClientConnection derefs
    // to ConnectionCommon, which is what the bound requires.
    let mut tls = rustls::StreamOwned::new(conn, stream);
    tls.write_all(request).map_err(|e| e.to_string())?;
    tls.flush().map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    let _ = tls.read_to_end(&mut raw);
    Ok(raw)
}

/// The machine's first non-loopback IPv4 address (for AT-231: the
/// connection cap counts non-loopback peers only). Found via the UDP
/// connect trick — no packet is sent.
pub fn non_loopback_ip() -> std::net::IpAddr {
    let sock = std::net::UdpSocket::bind("0.0.0.0:0").expect("bind a UDP socket");
    sock.connect("8.8.8.8:80").expect("UDP connect");
    let ip = sock.local_addr().expect("local addr").ip();
    assert!(!ip.is_loopback(), "no non-loopback IPv4 found");
    ip
}

/// A connected rustls client stream (accept-any verifier) for holding
/// TLS connections open (AT-231 test).
pub fn tls_stream(
    addr: (&str, u16),
) -> rustls::StreamOwned<rustls::ClientConnection, std::net::TcpStream> {
    use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified};
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{ClientConfig, ClientConnection, DigitallySignedStruct, Error as TlsError};

    #[derive(Debug)]
    struct AcceptAny;
    impl rustls::client::danger::ServerCertVerifier for AcceptAny {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, TlsError> {
            Ok(ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TlsError> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dss: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, TlsError> {
            Ok(HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![
                rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
                rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
                rustls::SignatureScheme::ED25519,
                rustls::SignatureScheme::RSA_PSS_SHA256,
                rustls::SignatureScheme::RSA_PKCS1_SHA256,
            ]
        }
    }

    let provider = std::sync::Arc::new(rustls::crypto::ring::default_provider());
    let config = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS12, &rustls::version::TLS13])
        .unwrap()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(AcceptAny))
        .with_no_client_auth();
    let server_name = ServerName::IpAddress(rustls::pki_types::IpAddr::try_from(addr.0).unwrap());
    let conn = ClientConnection::new(std::sync::Arc::new(config), server_name).unwrap();
    let stream = TcpStream::connect(addr).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.set_write_timeout(Some(Duration::from_secs(10))).unwrap();
    rustls::StreamOwned::new(conn, stream)
}

impl Drop for RunningHost {
    fn drop(&mut self) {
        // SIGTERM first (the host installs a handler); SIGKILL after a
        // short grace period.
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// =====================================================================
// Step 10 additions: egress/LRS fixtures, configurable Scholar spawn,
// archive + inclusion-index inspection helpers.
// =====================================================================

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub fn host_bin_egress() -> String {
    env_bin(
        "S3A_KITE_BIN_EGRESS_TEST",
        "a built s3a-kite host binary with feature egress_test (cargo build --offline --features egress_test --target-dir target/egress)",
    )
}

pub fn host_bin_egress_hooks() -> String {
    env_bin(
        "S3A_KITE_BIN_EGRESS_TEST_HOOKS",
        "a built s3a-kite host binary with features native-window,test_hooks,egress_test (the hooks binary, target/hooks)",
    )
}

/// Which host binary a Step 10 test spawns.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum HostBin {
    Default,
    Hooks,
    Egress,
    EgressHooks,
}

/// Options for a configurable Scholar host (Step 10). Defaults match the
/// step-9 `scholar` spawn (30 s fetch timeout, 8 MiB caps, 5 redirects).
pub struct ScholarOpts {
    pub bin: HostBin,
    /// Full `--lrs-url` value (e.g. an LRS fixture URL). `None` means no LRS.
    pub lrs_url: Option<String>,
    /// POST consent `on` after spawn (requires `lrs_url`).
    pub consent_on: bool,
    pub fetch_timeout_secs: u64,
    pub max_fetch_bytes: u64,
    pub max_open_bytes: u64,
    pub max_redirects: u64,
    /// `--lrs-timeout-secs` when an LRS is configured.
    pub lrs_timeout_secs: u64,
    /// `--researcher-id` value (default: the lab fixture id).
    pub researcher_id: Option<String>,
    /// `--study-id` value (default: none — lab mode).
    pub study_id: Option<String>,
    pub egress_test_loopback_port: Option<u16>,
    pub extra: Vec<String>,
    pub env: Vec<(String, String)>,
    pub setup: Option<Box<dyn Fn(&TempDir)>>,
}

impl Default for ScholarOpts {
    fn default() -> Self {
        ScholarOpts {
            bin: HostBin::Default,
            lrs_url: None,
            consent_on: false,
            fetch_timeout_secs: 30,
            max_fetch_bytes: MAX_OPEN_BYTES,
            max_open_bytes: MAX_OPEN_BYTES,
            max_redirects: 5,
            lrs_timeout_secs: 5,
            researcher_id: None,
            study_id: None,
            egress_test_loopback_port: None,
            extra: Vec::new(),
            env: Vec::new(),
            setup: None,
        }
    }
}

impl RunningHost {
    /// Spawn a Scholar host with Step 10 options (caps, LRS fixture URL,
    /// egress-test port, binary flavour, hook env vars).
    pub fn scholar_opts(opts: ScholarOpts) -> RunningHost {
        let dir = TempDir::new("scholar10");
        Self::scholar_opts_in(dir, opts)
    }

    /// Spawn a Scholar host on an existing temp dir (restart tests reuse
    /// the first run's directories: working archive, data dir, logs).
    pub fn scholar_opts_in(dir: TempDir, opts: ScholarOpts) -> RunningHost {
        let bin = match opts.bin {
            HostBin::Default => host_bin(),
            HostBin::Hooks => host_bin_hooks(),
            HostBin::Egress => host_bin_egress(),
            HostBin::EgressHooks => host_bin_egress_hooks(),
        };
        let view_root = dir.mkdir("view");
        let wiki_root = dir.mkdir("wiki");
        let corpus_root = dir.mkdir("corpus");
        let snapshot_dir = dir.mkdir("snapshot");
        let download_dir = dir.mkdir("download");
        let data_dir = dir.mkdir("data");
        let log_dir = dir.mkdir("log");
        let token_file = dir.write("token.txt", format!("{TOKEN}\n").as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&token_file, std::fs::Permissions::from_mode(0o600))
                .unwrap();
        }
        if let Some(setup) = &opts.setup {
            setup(&dir);
        }
        let port = free_port();
        let working_dir = dir.mkdir("working");
        let working_archive = working_dir.join("working.s3a");
        let mut args: Vec<String> = vec![
            "--bind".into(),
            "127.0.0.1".into(),
            "--port".into(),
            port.to_string(),
            "--auth-token-file".into(),
            token_file.to_string_lossy().into_owned(),
            "--log-dir".into(),
            log_dir.to_string_lossy().into_owned(),
            "--max-open-handles".into(),
            "16".into(),
            "--max-frame-pixels".into(),
            MAX_FRAME_PIXELS.to_string(),
            "--handle-idle-secs".into(),
            "60".into(),
            "--max-request-bytes".into(),
            MAX_REQUEST_BYTES.to_string(),
            "--max-open-bytes".into(),
            opts.max_open_bytes.to_string(),
            "--headless".into(),
            "--wiki-root".into(),
            wiki_root.to_string_lossy().into_owned(),
            "--corpus-root".into(),
            corpus_root.to_string_lossy().into_owned(),
            "--working-archive".into(),
            working_archive.to_string_lossy().into_owned(),
            "--snapshot-dir".into(),
            snapshot_dir.to_string_lossy().into_owned(),
            "--download-dir".into(),
            download_dir.to_string_lossy().into_owned(),
            "--data-dir".into(),
            data_dir.to_string_lossy().into_owned(),
            "--researcher-id".into(),
            opts.researcher_id
                .clone()
                .unwrap_or_else(|| "https://example.test/researcher/ashley".to_string()),
            "--fetch-timeout-secs".into(),
            opts.fetch_timeout_secs.to_string(),
            "--max-fetch-bytes".into(),
            opts.max_fetch_bytes.to_string(),
            "--max-redirects".into(),
            opts.max_redirects.to_string(),
        ];
        if let Some(study) = &opts.study_id {
            args.push("--study-id".into());
            args.push(study.clone());
        }
        if let Some(p) = opts.egress_test_loopback_port {
            args.push("--egress-test-loopback-port".into());
            args.push(p.to_string());
        }
        if let Some(url) = &opts.lrs_url {
            let consent = dir.join("consent.txt");
            let creds = dir.write("lrs-creds.txt", b"lrs-user:lrs-pass");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600))
                    .unwrap();
            }
            args.push("--lrs-url".into());
            args.push(url.clone());
            args.push("--consent-state-file".into());
            args.push(consent.to_string_lossy().into_owned());
            args.push("--lrs-credentials-file".into());
            args.push(creds.to_string_lossy().into_owned());
            args.push("--lrs-timeout-secs".into());
            args.push(opts.lrs_timeout_secs.to_string());
        }
        for e in &opts.extra {
            args.push(e.clone());
        }
        let mut cmd = Command::new(&bin);
        cmd.args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in &opts.env {
            cmd.env(k, v);
        }
        let child = cmd
            .spawn()
            .unwrap_or_else(|e| panic!("spawn {bin} {}: {e}", args.join(" ")));
        let mut host = RunningHost {
            child,
            dir,
            port,
            socket_path: None,
            token: TOKEN.to_string(),
            view_root,
            wiki_root,
            corpus_root,
            snapshot_dir,
            download_dir,
            data_dir,
            log_dir,
            token_file,
            tls: false,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(Some(status)) = host.child.try_wait() {
                panic!("host exited during startup (status {status}); log: {}", host.log());
            }
            if Instant::now() > deadline {
                panic!("host did not serve /health within 15s; log: {}", host.log());
            }
            let up = host
                .get_raw_result("/health", &[("Authorization", &format!("Bearer {TOKEN}"))])
                .map(|r| r.status == 200 && r.body == b"ok\n")
                .unwrap_or(false);
            if up {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if opts.consent_on {
            assert!(
                opts.lrs_url.is_some(),
                "consent_on requires an LRS URL"
            );
            let resp = host.post("/v1/consent", &serde_json::json!({"state": "on"}));
            assert_eq!(resp.status, 200, "consent on failed: {}", resp.text());
        }
        host
    }

    /// Stop the child process (SIGTERM, 2 s grace, SIGKILL) without
    /// dropping the host struct, so a restart test can reuse its dir.
    /// After this, `Drop` finds the child already waited.
    pub fn stop(&mut self) {
        unsafe {
            libc::kill(self.child.id() as i32, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            if Instant::now() > deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Take ownership of the temp dir, leaving a fresh placeholder
    /// behind (deleted when this host drops). Restart tests spawn the
    /// next host on the returned dir with `scholar_opts_in`.
    pub fn take_dir(&mut self) -> TempDir {
        std::mem::replace(&mut self.dir, TempDir::new("scholar10-placeholder"))
    }

    /// The working archive path for this host.
    pub fn working_archive_path(&self) -> PathBuf {
        self.dir.join("working").join("working.s3a")
    }

    pub fn consent_on(&self) -> Resp {
        self.post("/v1/consent", &serde_json::json!({"state": "on"}))
    }

    pub fn consent_off(&self) -> Resp {
        self.post("/v1/consent", &serde_json::json!({"state": "off"}))
    }

    pub fn select(&self, paper_id: &str) -> Resp {
        self.post("/v1/select", &serde_json::json!({"paper_id": paper_id}))
    }

    pub fn download(&self, paper_id: &str) -> Resp {
        self.post("/v1/download", &serde_json::json!({"paper_id": paper_id}))
    }

    pub fn deselect(&self, paper_id: &str) -> Resp {
        self.post("/v1/deselect", &serde_json::json!({"paper_id": paper_id}))
    }

    pub fn skip(&self, paper_id: &str) -> Resp {
        self.post("/v1/skip", &serde_json::json!({"paper_id": paper_id}))
    }

    pub fn candidates_json(&self) -> serde_json::Value {
        let resp = self.get("/v1/candidates");
        assert_eq!(resp.status, 200, "{}", resp.text());
        resp.json()
    }
}

/// Write a `{stem}.md` dossier under `root/papers/` (wiki or corpus).
pub fn write_dossier_at(root: &Path, stem: &str, front_matter: &str, body: &str) -> PathBuf {
    let path = root.join("papers").join(format!("{stem}.md"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("---\n{front_matter}\n---\n{body}")).unwrap();
    path
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    let mut h = sha2::Sha256::new();
    h.update(bytes);
    hex_bytes(&h.finalize())
}

pub fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Count `HumanLrsRecord` (tile 10) records in an archive file (64 bytes
/// each). 0 when the file does not exist or is not a readable archive.
pub fn count_tile10_records(path: &Path) -> usize {
    let Ok(bytes) = std::fs::read(path) else {
        return 0;
    };
    let Ok(reader) = s3a_engine::MmapReader::from_bytes(&bytes) else {
        return 0;
    };
    let mut n = 0;
    for i in 0..reader.tile_count() as usize {
        if let Some((h, _payload)) = reader.get_tile(i) {
            if h.tile_type == 10 {
                n += (h.payload_bytes as usize) / 64;
            }
        }
    }
    n
}

/// Distinct tile types present in an archive file.
pub fn archive_tile_types(path: &Path) -> Vec<u32> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(reader) = s3a_engine::MmapReader::from_bytes(&bytes) else {
        return Vec::new();
    };
    let mut out: Vec<u32> = Vec::new();
    for i in 0..reader.tile_count() as usize {
        if let Some((h, _)) = reader.get_tile(i) {
            if !out.contains(&h.tile_type) {
                out.push(h.tile_type);
            }
        }
    }
    out.sort_unstable();
    out
}

/// Parse `--data-dir/inclusion-index.jsonl` into JSON values (one per
/// non-empty line). Empty vec when the file is missing.
pub fn read_index_lines(data_dir: &Path) -> Vec<serde_json::Value> {
    let Ok(text) = std::fs::read_to_string(data_dir.join("inclusion-index.jsonl")) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .collect()
}

/// Files directly inside a directory (names only, sorted).
pub fn dir_files(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = rd
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    out.sort();
    out
}

/// Files directly inside a directory excluding the sidecar `.lock` files
/// the host holds for the whole process (DD-114 lock 1).
pub fn dir_files_no_lock(dir: &Path) -> Vec<String> {
    dir_files(dir)
        .into_iter()
        .filter(|n| n != ".lock")
        .collect()
}

pub fn snapshot_files(snapshot_dir: &Path) -> Vec<String> {
    dir_files(snapshot_dir)
        .into_iter()
        .filter(|n| n.ends_with(".s3a"))
        .collect()
}

/// Reassemble the BLOB (tile 68) chunks of a snapshot file for `key`
/// (the 32-byte SHA-256 of the captured bytes). `None` when the file is
/// not a readable archive or the key is absent/incomplete.
pub fn reassemble_snapshot_blob(path: &Path, key: &[u8; 32]) -> Option<Vec<u8>> {
    let bytes = std::fs::read(path).ok()?;
    let reader = s3a_engine::MmapReader::from_bytes(&bytes).ok()?;
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for i in 0..reader.tile_count() as usize {
        if let Some((h, payload)) = reader.get_tile(i) {
            if h.tile_type == 68 {
                payloads.push(payload.to_vec());
            }
        }
    }
    let map = s3a_pack_clean::archive::unchunk(&payloads).ok()?;
    map.get(key).cloned()
}

/// Reassemble the PROVENANCE (tile 72) document of a snapshot file (the
/// single entry). `None` when absent/incomplete.
pub fn provenance_document(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let reader = s3a_engine::MmapReader::from_bytes(&bytes).ok()?;
    let mut payloads: Vec<Vec<u8>> = Vec::new();
    for i in 0..reader.tile_count() as usize {
        if let Some((h, payload)) = reader.get_tile(i) {
            if h.tile_type == 72 {
                payloads.push(payload.to_vec());
            }
        }
    }
    let map = s3a_pack_clean::archive::unchunk(&payloads).ok()?;
    let (_key, doc) = map.iter().next()?;
    String::from_utf8(doc.clone()).ok()
}

/// The BLOB chunk structure of a snapshot: (blob_key, chunk_index,
/// total_chunks, payload_len) per BLOB tile, in tile order.
pub fn snapshot_blob_chunks(path: &Path) -> Vec<([u8; 32], u32, u32, usize)> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let Ok(reader) = s3a_engine::MmapReader::from_bytes(&bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for i in 0..reader.tile_count() as usize {
        if let Some((h, payload)) = reader.get_tile(i) {
            if h.tile_type == 68 && payload.len() >= 40 {
                let mut key = [0u8; 32];
                key.copy_from_slice(&payload[0..32]);
                let index = u32::from_le_bytes(payload[32..36].try_into().unwrap());
                let total = u32::from_le_bytes(payload[36..40].try_into().unwrap());
                out.push((key, index, total, payload.len() - 40));
            }
        }
    }
    out
}

/// Create a working archive WITHOUT `ADDON_EDU` (AT-46 host B): a plain
/// `TileWriter::create` with no `enable_addon` call.
pub fn make_no_addon_archive(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    let file = std::fs::File::create(path).unwrap();
    let _writer = s3a_engine::TileWriter::create_from_file(file).unwrap();
}

/// Clear the `ADDON_EDU` bit in place (same inode, no rename) in both
/// header slots of an archive, recomputing each slot's header CRC32C
/// (AT-46 second case). The file must be a valid archive.
pub fn clear_addon_bit_in_place(path: &Path) {
    let mut bytes = std::fs::read(path).unwrap();
    assert!(bytes.len() >= 128, "archive too small for two header slots");
    for slot in [0usize, 64usize] {
        // A zeroed slot was never written; leave it alone.
        if bytes[slot..slot + 64].iter().all(|b| *b == 0) {
            continue;
        }
        // addon_mask is at offset 44..48; header_crc32 at 40..44.
        let mask = u32::from_le_bytes(bytes[slot + 44..slot + 48].try_into().unwrap());
        let cleared = mask & !1u32;
        bytes[slot + 44..slot + 48].copy_from_slice(&cleared.to_le_bytes());
        // Recompute the header CRC32C over the slot with the CRC field
        // zeroed (s3a-engine `compute_header_crc32`).
        let crc = {
            let mut c = crc32c::crc32c(&bytes[slot..slot + 40]);
            c = crc32c::crc32c_append(c, &bytes[slot + 44..slot + 64]);
            c
        };
        bytes[slot + 40..slot + 44].copy_from_slice(&crc.to_le_bytes());
    }
    std::fs::write(path, &bytes).unwrap();
}

pub fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

pub fn deflate_bytes(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    e.write_all(data).unwrap();
    e.finish().unwrap()
}

/// Wait until the fixture's GET count for `path` reaches `n` (up to
/// `timeout`); panics on timeout. Used to order a waiter behind a
/// starter's in-flight GET.
pub fn wait_for_fixture_gets(fixture: &FixtureServer, path: &str, n: u64, timeout: Duration) {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if fixture.get_count(path) >= n {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!(
        "fixture GET count for {path} did not reach {n} within {timeout:?} (got {})",
        fixture.get_count(path)
    );
}

// ---- Fixture HTTP server (egress_test loopback origin) ----

/// One programmable fixture route.
#[derive(Debug, Clone)]
pub enum FixtureRoute {
    /// A static response (status, extra headers, body) after `delay_ms`.
    Static {
        status: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
        delay_ms: u64,
    },
    /// A 302 redirect to `location`.
    Redirect { location: String },
    /// A different body on every GET: `counted-body-{n:08}\n`.
    Counter,
    /// Fails with `fail_status` until `set_flaky_succeed(true)`, then 200
    /// with `body`.
    Flaky { fail_status: u16, body: Vec<u8> },
}

struct FixtureState {
    routes: HashMap<String, FixtureRoute>,
    counts: HashMap<String, u64>,
    flaky_succeed: bool,
    connections: u64,
}

/// A tiny programmable HTTP/1.1 origin on 127.0.0.1 (the egress_test
/// fixture). One thread per connection; the accept loop polls a shutdown
/// flag so `Drop` is prompt.
pub struct FixtureServer {
    pub port: u16,
    state: Arc<Mutex<FixtureState>>,
    shutdown: Arc<AtomicBool>,
}

impl FixtureServer {
    pub fn new() -> FixtureServer {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(FixtureState {
            routes: HashMap::new(),
            counts: HashMap::new(),
            flaky_succeed: false,
            connections: 0,
        }));
        let shutdown = Arc::new(AtomicBool::new(false));
        let state2 = state.clone();
        let shutdown2 = shutdown.clone();
        std::thread::spawn(move || {
            while !shutdown2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        {
                            let mut s = state2.lock().unwrap();
                            s.connections += 1;
                        }
                        let st = state2.clone();
                        std::thread::spawn(move || serve_fixture_conn(stream, st));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        });
        FixtureServer {
            port,
            state,
            shutdown,
        }
    }

    pub fn set_route(&self, path: &str, route: FixtureRoute) {
        self.state
            .lock()
            .unwrap()
            .routes
            .insert(path.to_string(), route);
    }

    pub fn get_count(&self, path: &str) -> u64 {
        *self.state.lock().unwrap().counts.get(path).unwrap_or(&0)
    }

    pub fn total_connections(&self) -> u64 {
        self.state.lock().unwrap().connections
    }

    pub fn set_flaky_succeed(&self, v: bool) {
        self.state.lock().unwrap().flaky_succeed = v;
    }

    /// Install a redirect chain: `/c0` → `/c1` → … → `/c{hops}` →
    /// `final_path` (`hops` redirects before the final hop's target).
    pub fn set_redirect_chain(&self, hops: usize, final_path: &str) {
        for i in 0..=hops {
            let next = if i == hops {
                final_path.to_string()
            } else {
                format!("/c{}", i + 1)
            };
            self.set_route(
                &format!("/c{i}"),
                FixtureRoute::Redirect { location: next },
            );
        }
    }

    /// A static 200 route for `path` with `body`.
    pub fn set_ok(&self, path: &str, body: &[u8]) {
        self.set_route(
            path,
            FixtureRoute::Static {
                status: 200,
                headers: Vec::new(),
                body: body.to_vec(),
                delay_ms: 0,
            },
        );
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // Wake the accept loop so the thread notices promptly.
        let _ = std::net::TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        302 => "Found",
        400 => "Bad Request",
        404 => "Not Found",
        408 => "Request Timeout",
        409 => "Conflict",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

fn read_http_request(
    stream: &mut std::net::TcpStream,
) -> Option<(String, String, Vec<(String, String)>, Vec<u8>)> {
    // Read until the header block ends, then the Content-Length body.
    let mut raw: Vec<u8> = Vec::new();
    let mut buf = [0u8; 4096];
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .ok()?;
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                if raw.len() > 1_048_576 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&raw);
    let mut lines = text.lines();
    let request_line = lines.next()?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let mut content_len = 0usize;
    let mut headers: Vec<(String, String)> = Vec::new();
    for line in lines.by_ref() {
        if line.is_empty() {
            break;
        }
        if let Some((n, v)) = line.split_once(':') {
            let name = n.trim().to_string();
            let value = v.trim().to_string();
            if name.eq_ignore_ascii_case("content-length") {
                content_len = value.parse().unwrap_or(0);
            }
            headers.push((name, value));
        }
    }
    let head_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|p| p + 4)
        .unwrap_or(raw.len());
    let mut body = raw[head_end..].to_vec();
    while body.len() < content_len {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => body.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    body.truncate(content_len);
    Some((method, target, headers, body))
}

fn serve_fixture_conn(mut stream: std::net::TcpStream, state: Arc<Mutex<FixtureState>>) {
    let Some((_method, target, _headers, _body)) = read_http_request(&mut stream) else {
        return;
    };
    let path = target.split('?').next().unwrap_or("").to_string();
    let (route, count) = {
        let n = {
            let mut s = state.lock().unwrap();
            let n = s.counts.entry(path.clone()).or_insert(0);
            *n += 1;
            *n
        };
        (state.lock().unwrap().routes.get(&path).cloned(), n)
    };
    let (status, headers, body, delay_ms) = match route {
        Some(FixtureRoute::Static {
            status,
            headers,
            body,
            delay_ms,
        }) => (status, headers, body, delay_ms),
        Some(FixtureRoute::Redirect { location }) => (
            302,
            vec![("Location".to_string(), location)],
            Vec::new(),
            0,
        ),
        Some(FixtureRoute::Counter) => (
            200,
            Vec::new(),
            format!("counted-body-{count:08}\n").into_bytes(),
            500,
        ),
        Some(FixtureRoute::Flaky { fail_status, body }) => {
            let succeed = state.lock().unwrap().flaky_succeed;
            if succeed {
                (200, Vec::new(), body, 0)
            } else {
                (fail_status, Vec::new(), b"flaky failure\n".to_vec(), 0)
            }
        }
        None => (
            404,
            Vec::new(),
            b"fixture: no such route\n".to_vec(),
            0,
        ),
    };
    if delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(delay_ms));
    }
    let mut resp = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        status,
        reason_phrase(status),
        body.len()
    );
    for (n, v) in &headers {
        resp.push_str(&format!("{n}: {v}\r\n"));
    }
    resp.push_str("\r\n");
    let mut bytes = resp.into_bytes();
    bytes.extend_from_slice(&body);
    let _ = stream.write_all(&bytes);
    let _ = stream.flush();
}

// ---- LRS fixture server ----

struct LrsState {
    status: u16,
    statements: Vec<serde_json::Value>,
    auth: Vec<String>,
    api_versions: Vec<String>,
    paths: Vec<String>,
    posts: u64,
    /// Sleep before writing the response (AT-156 slow-LRS case).
    delay: Duration,
    /// Accept + read the request, then never respond (AT-192 timeout).
    hang: bool,
    /// Exact bytes to send instead of the canned response (AT-191
    /// oversized-remainder cases). `None` means the canned response.
    raw: Option<Vec<u8>>,
}

/// A programmable xAPI LRS fixture: records every POSTed statement, its
/// Authorization header, and its X-Experience-API-Version, and answers
/// with a configurable status. Binds on the given IP (127.0.0.1 or ::1).
pub struct LrsServer {
    pub port: u16,
    pub ip: String,
    state: Arc<Mutex<LrsState>>,
    shutdown: Arc<AtomicBool>,
}

impl LrsServer {
    pub fn new(ip: &str) -> LrsServer {
        // An IPv6 literal needs brackets in a SocketAddr string.
        let addr: std::net::SocketAddr = if ip.contains(':') {
            format!("[{ip}]:0").parse().unwrap()
        } else {
            format!("{ip}:0").parse().unwrap()
        };
        let listener = std::net::TcpListener::bind(addr).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let state = Arc::new(Mutex::new(LrsState {
            status: 200,
            statements: Vec::new(),
            auth: Vec::new(),
            api_versions: Vec::new(),
            paths: Vec::new(),
            posts: 0,
            delay: Duration::from_millis(0),
            hang: false,
            raw: None,
        }));
        let shutdown = Arc::new(AtomicBool::new(false));
        let state2 = state.clone();
        let shutdown2 = shutdown.clone();
        std::thread::spawn(move || {
            while !shutdown2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let st = state2.clone();
                        std::thread::spawn(move || serve_lrs_conn(stream, st));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
            }
        });
        LrsServer {
            port,
            ip: ip.to_string(),
            state,
            shutdown,
        }
    }

    /// The `--lrs-url` value for this fixture (`{scheme}://{host}:{port}/xapi`).
    pub fn url(&self) -> String {
        if self.ip.contains(':') {
            format!("http://[{}]:{}/xapi", self.ip, self.port)
        } else {
            format!("http://{}:{}/xapi", self.ip, self.port)
        }
    }

    pub fn set_status(&self, status: u16) {
        self.state.lock().unwrap().status = status;
    }

    /// Sleep `delay` before writing each response.
    pub fn set_delay(&self, delay: Duration) {
        self.state.lock().unwrap().delay = delay;
    }

    /// Accept and read the request, then never respond.
    pub fn set_hang(&self, hang: bool) {
        self.state.lock().unwrap().hang = hang;
    }

    /// Send these exact bytes as the response (recorded the same way).
    /// `None` restores the canned response.
    pub fn set_raw_response(&self, raw: Option<Vec<u8>>) {
        self.state.lock().unwrap().raw = raw;
    }

    pub fn statements(&self) -> Vec<serde_json::Value> {
        self.state.lock().unwrap().statements.clone()
    }

    pub fn statement_count(&self) -> usize {
        self.state.lock().unwrap().statements.len()
    }

    pub fn post_count(&self) -> u64 {
        self.state.lock().unwrap().posts
    }

    pub fn verbs(&self) -> Vec<String> {
        self.state
            .lock()
            .unwrap()
            .statements
            .iter()
            .filter_map(|s| {
                s.get("verb")?
                    .get("id")?
                    .as_str()
                    .map(|v| v.to_string())
            })
            .collect()
    }

    pub fn auth_headers(&self) -> Vec<String> {
        self.state.lock().unwrap().auth.clone()
    }

    pub fn api_versions(&self) -> Vec<String> {
        self.state.lock().unwrap().api_versions.clone()
    }

    pub fn paths(&self) -> Vec<String> {
        self.state.lock().unwrap().paths.clone()
    }
}

impl Drop for LrsServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        // An IPv6 literal needs brackets in a SocketAddr string.
        let addr: std::net::SocketAddr = if self.ip.contains(':') {
            format!("[{}]:{}", self.ip, self.port).parse().unwrap()
        } else {
            format!("{}:{}", self.ip, self.port).parse().unwrap()
        };
        let _ = std::net::TcpStream::connect(addr);
    }
}

fn serve_lrs_conn(mut stream: std::net::TcpStream, state: Arc<Mutex<LrsState>>) {
    let Some((_method, target, headers, body)) = read_http_request(&mut stream) else {
        return;
    };
    let header = |name: &str| {
        headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    let (status, delay, hang, raw) = {
        let mut s = state.lock().unwrap();
        s.posts += 1;
        s.paths.push(target.clone());
        s.auth.push(header("authorization").unwrap_or_default());
        s.api_versions
            .push(header("x-experience-api-version").unwrap_or_default());
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&body) {
            s.statements.push(v);
        }
        (s.status, s.delay, s.hang, s.raw.clone())
    };
    if hang {
        // Never respond; park until the test drops the server (the
        // accept loop's shutdown flag is not visible here, so sleep in
        // slices and stop when the socket errors or the process ends).
        loop {
            std::thread::sleep(Duration::from_millis(100));
            if stream.set_nonblocking(true).is_err() {
                return;
            }
            let mut b = [0u8; 1];
            match stream.read(&mut b) {
                Ok(0) => return,
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(_) => return,
                Ok(_) => continue,
            }
        }
    }
    if delay.as_millis() > 0 {
        std::thread::sleep(delay);
    }
    if let Some(raw) = raw {
        let _ = stream.write_all(&raw);
        let _ = stream.flush();
        return;
    }
    let resp = format!(
        "HTTP/1.1 {status} {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        reason_phrase(status)
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}
