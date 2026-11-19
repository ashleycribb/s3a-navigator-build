//! The native-window client (§5.2.2, AT-125). Only in a `native-window`
//! build, and only when `--headless` was not given.
//!
//! There is no GUI toolkit in this tree, so the window itself is a stub:
//! this client exists to perform the §5.2.7 `window_issue_select` hook
//! action — after `GET /health` succeeds, issue `GET /v1/candidates`
//! and then `POST /v1/select` for the first listed candidate, both as
//! real Host HTTP API requests carrying the Bearer token. The window
//! rendering itself is not implemented (logged as a known gap).

use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::flags::{Bind, Config};

/// Start the window client thread. Does nothing when the
/// `window_issue_select` hook is not configured.
pub fn start_window_client(
    config: &Config,
    token: &str,
    window_issue_select: bool,
    shutdown: Arc<AtomicBool>,
) {
    if !window_issue_select {
        return;
    }
    let bind = config.bind.clone();
    let port = config.port;
    let token = token.to_string();
    std::thread::spawn(move || {
        run_window_hook(&bind, port, &token, &shutdown);
    });
}

fn run_window_hook(bind: &Bind, port: Option<u16>, token: &str, shutdown: &Arc<AtomicBool>) {
    // Wait for GET /health to succeed (poll, bounded by shutdown).
    for _ in 0..300 {
        if shutdown.load(Ordering::Relaxed) {
            return;
        }
        match http_get(bind, port, "/health", token) {
            Ok((200, _)) => break,
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    if shutdown.load(Ordering::Relaxed) {
        return;
    }
    let Ok((200, body)) = http_get(bind, port, "/v1/candidates", token) else {
        return;
    };
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(&body) else {
        return;
    };
    let Some(first) = json
        .get("papers")
        .and_then(|p| p.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("id"))
        .and_then(|id| id.as_str())
        .map(|s| s.to_string())
    else {
        return;
    };
    let body = serde_json::to_vec(&serde_json::json!({"paper_id": first})).unwrap();
    let _ = http_post(bind, port, "/v1/select", token, &body);
}

/// Minimal HTTP/1.1 client over the host's own bind (plaintext loopback
/// TCP or a Unix socket). The window client only runs on loopback/unix
/// binds (a non-loopback bind requires --headless), so no TLS is needed.
fn http_get(bind: &Bind, port: Option<u16>, path: &str, token: &str) -> Result<(u16, Vec<u8>), String> {
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nConnection: close\r\n\r\n"
    );
    let raw = http_roundtrip(bind, port, request.as_bytes())?;
    parse_response(&raw)
}

fn http_post(bind: &Bind, port: Option<u16>, path: &str, token: &str, body: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let mut bytes = request.into_bytes();
    bytes.extend_from_slice(body);
    let raw = http_roundtrip(bind, port, &bytes)?;
    parse_response(&raw)
}

fn http_roundtrip(bind: &Bind, port: Option<u16>, request: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    match bind {
        Bind::Tcp(ip) => {
            let port = port.unwrap_or(0);
            let mut stream = TcpStream::connect((*ip, port))
                .map_err(|e| format!("window client connect failed: {e}"))?;
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .map_err(|e| e.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_secs(10)))
                .map_err(|e| e.to_string())?;
            stream
                .write_all(request)
                .map_err(|e| format!("window client write failed: {e}"))?;
            stream
                .read_to_end(&mut out)
                .map_err(|e| format!("window client read failed: {e}"))?;
        }
        Bind::Unix(path) => {
            let mut stream = UnixStream::connect(path)
                .map_err(|e| format!("window client connect failed: {e}"))?;
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .map_err(|e| e.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_secs(10)))
                .map_err(|e| e.to_string())?;
            stream
                .write_all(request)
                .map_err(|e| format!("window client write failed: {e}"))?;
            stream
                .read_to_end(&mut out)
                .map_err(|e| format!("window client read failed: {e}"))?;
        }
    }
    Ok(out)
}

fn parse_response(raw: &[u8]) -> Result<(u16, Vec<u8>), String> {
    let text = std::str::from_utf8(raw).map_err(|_| "non-UTF-8 response".to_string())?;
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().ok_or("empty response")?;
    let body = parts.next().unwrap_or("").as_bytes().to_vec();
    let status_line = head.lines().next().ok_or("empty response")?;
    let status: u16 = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or("malformed status line")?;
    Ok((status, body))
}
