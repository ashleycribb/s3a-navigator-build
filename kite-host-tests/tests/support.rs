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
