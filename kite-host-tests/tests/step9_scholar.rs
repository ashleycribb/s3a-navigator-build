//! Step 9 — Scholar routes: /v1/candidates, /v1/select, /v1/skip,
//! /v1/deselect, /v1/consent (AT-116, AT-144, AT-225), handle idle
//! (AT-149), keep-alive / idle connections (AT-230), non-loopback
//! connection cap (AT-231).

mod fixture8b;
mod support;

use std::io::{Read, Write};
use std::time::{Duration, Instant};

use fixture8b::*;
use s3a_core::{ADDON_EDU, HyperTileHeader};
use s3a_pack_scholar::records::AcademicPaperRecord;
use s3a_pack_scholar::{ScholarPaperBlobHeader, ScholarTileType};
use sha2::{Digest, Sha256};
use support::*;

fn paper_record(paper_id: u64) -> AcademicPaperRecord {
    AcademicPaperRecord::new(paper_id, 1_700_000_000, 0, 0.0, 0.0, 0.0, 0, 2024, 0, 0, 0, 0)
}

fn type12_v1_tile(records: &[AcademicPaperRecord]) -> HyperTile {
    let mut payload = Vec::new();
    for r in records {
        payload.extend_from_slice(bytemuck::bytes_of(r));
    }
    HyperTile::Raw(ScholarTileType::ACADEMIC_PAPERS, payload, records.len() as u32)
}

fn write_dossier(host: &RunningHost, stem: &str, front_matter: &str, body: &str) {
    let path = host.wiki_root.join("papers").join(format!("{stem}.md"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, format!("---\n{front_matter}\n---\n{body}")).unwrap();
}

fn candidates(host: &RunningHost) -> serde_json::Value {
    let resp = host.get("/v1/candidates");
    assert_eq!(resp.status, 200, "{}", resp.text());
    resp.json()
}

#[test]
fn candidates_empty() {
    let host = RunningHost::scholar(&[], &[]);
    let body = candidates(&host);
    assert_eq!(body.get("papers").and_then(|v| v.as_array()).unwrap().len(), 0);
}

#[test]
fn candidates_dossier() {
    let host = RunningHost::scholar(&[], &[]);
    write_dossier(
        &host,
        "SRC-0001",
        "title: First Paper\nsource_url: https://example.test/paper-1",
        "# First Paper\n\nBody.\n",
    );
    let body = candidates(&host);
    let papers = body.get("papers").and_then(|v| v.as_array()).unwrap();
    assert_eq!(papers.len(), 1);
    let p = &papers[0];
    assert_eq!(p.get("id").and_then(|v| v.as_str()), Some("SRC-0001"));
    assert_eq!(p.get("title").and_then(|v| v.as_str()), Some("First Paper"));
    assert_eq!(
        p.get("source_url").and_then(|v| v.as_str()),
        Some("https://example.test/paper-1")
    );
    assert!(p.get("page_path").and_then(|v| v.as_str()).is_some());
    assert_eq!(p.get("archive_path"), Some(&serde_json::Value::Null));
}

#[test]
fn candidates_archive_paper() {
    // AT-225 item 2: a live type-12 paper record in a corpus archive.
    let host = RunningHost::scholar(&[], &[]);
    let fx = build_hyper(
        vec![
            HyperTile::Addon(ADDON_EDU),
            type12_v1_tile(&[paper_record(111)]),
        ],
        false,
    );
    let path = host.corpus_root.join("papers.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = candidates(&host);
    let papers = body.get("papers").and_then(|v| v.as_array()).unwrap();
    assert_eq!(papers.len(), 1);
    let p = &papers[0];
    assert_eq!(p.get("id").and_then(|v| v.as_str()), Some("111"));
    assert_eq!(p.get("title").and_then(|v| v.as_str()), Some(""));
    assert!(p.get("archive_path").and_then(|v| v.as_str()).is_some());
    assert_eq!(p.get("page_path"), Some(&serde_json::Value::Null));
}

#[test]
fn candidates_viewer_only() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.get("/v1/candidates");
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "viewer_only");
}

#[test]
fn select_no_lrs() {
    let host = RunningHost::scholar(&[], &[]);
    write_dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.post("/v1/select", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert_eq!(body.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        body.get("lrs_status").and_then(|v| v.as_str()),
        Some("not_configured")
    );
}

#[test]
fn select_unknown_paper_is_404() {
    let host = RunningHost::scholar(&[], &[]);
    let resp = host.post("/v1/select", &serde_json::json!({"paper_id": "SRC-9999"}));
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_paper");
}

#[test]
fn select_viewer_only() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post("/v1/select", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "viewer_only");
}

#[test]
fn skip_writes_nothing() {
    // AT-125/§5.2.1: skip writes nothing and does not select — a later
    // deselect is 404 not_selected.
    let host = RunningHost::scholar(&[], &[]);
    write_dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.post("/v1/skip", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json().get("ok").and_then(|v| v.as_bool()), Some(true));
    let resp = host.post("/v1/deselect", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "not_selected");
}

#[test]
fn deselect_not_selected_is_404() {
    let host = RunningHost::scholar(&[], &[]);
    write_dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.post("/v1/deselect", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "not_selected");
}

#[test]
fn deselect_reason_is_400() {
    let host = RunningHost::scholar(&[], &[]);
    write_dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.post(
        "/v1/deselect",
        &serde_json::json!({"paper_id": "SRC-0001", "reason": "no longer needed"}),
    );
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_request");
}

#[test]
fn deselect_after_select() {
    let host = RunningHost::scholar(&[], &[]);
    write_dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.post("/v1/select", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.post("/v1/deselect", &serde_json::json!({"paper_id": "SRC-0001"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json().get("ok").and_then(|v| v.as_bool()), Some(true));
}

#[test]
fn consent_get_no_lrs() {
    let host = RunningHost::scholar(&[], &[]);
    let resp = host.get("/v1/consent");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert_eq!(body.get("state").and_then(|v| v.as_str()), Some("off"));
    assert_eq!(body.get("timestamp"), Some(&serde_json::Value::Null));
}

#[test]
fn consent_post_on_without_lrs_is_409() {
    // AT-116: consent on without --lrs-url is 409 lrs_not_configured,
    // and no consent file is created.
    let host = RunningHost::scholar(&[], &[]);
    let resp = host.post("/v1/consent", &serde_json::json!({"state": "on"}));
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "lrs_not_configured");
    assert!(!host.dir.join("consent.txt").exists());
}

#[test]
fn consent_post_off_without_lrs_is_noop() {
    let host = RunningHost::scholar(&[], &[]);
    let resp = host.post("/v1/consent", &serde_json::json!({"state": "off"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert_eq!(body.get("state").and_then(|v| v.as_str()), Some("off"));
    assert_eq!(body.get("timestamp"), Some(&serde_json::Value::Null));
}

#[test]
fn consent_post_bad_state_is_400() {
    let host = RunningHost::scholar(&[], &[]);
    let resp = host.post("/v1/consent", &serde_json::json!({"state": "maybe"}));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_param");
}

#[test]
fn consent_with_lrs_roundtrip() {
    let host = RunningHost::scholar_with_lrs(&[], &[]);
    let consent = host.dir.join("consent.txt");
    // Bootstrapped off with a timestamp.
    let resp = host.get("/v1/consent");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert_eq!(body.get("state").and_then(|v| v.as_str()), Some("off"));
    assert!(body.get("timestamp").and_then(|v| v.as_str()).is_some());
    // On.
    let resp = host.post("/v1/consent", &serde_json::json!({"state": "on"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json().get("state").and_then(|v| v.as_str()), Some("on"));
    let resp = host.get("/v1/consent");
    assert_eq!(resp.json().get("state").and_then(|v| v.as_str()), Some("on"));
    assert!(
        std::fs::read_to_string(&consent).unwrap().starts_with("on\n"),
        "consent file on disk matches"
    );
    // Off again.
    let resp = host.post("/v1/consent", &serde_json::json!({"state": "off"}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json().get("state").and_then(|v| v.as_str()), Some("off"));
    assert!(
        std::fs::read_to_string(&consent).unwrap().starts_with("off\n"),
        "consent file on disk matches"
    );
}

#[test]
fn consent_viewer_only() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.get("/v1/consent");
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "viewer_only");
}

#[test]
fn handle_idle_expires() {
    // AT-149: an idle handle is 404 unknown_handle after --handle-idle-secs.
    let host = RunningHost::scholar_idle(1);
    let fx = build_hyper(vec![HyperTile::Manifest("idle manifest".into())], false);
    let path = host.corpus_root.join("i.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let resp = host.post("/v1/open", &serde_json::json!({"path": path.to_string_lossy()}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let handle = resp.json().get("handle").unwrap().as_str().unwrap().to_string();
    std::thread::sleep(Duration::from_secs(2));
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=manifest&w=800&h=600"));
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_handle");
}

#[test]
fn select_control_scholar_vs_viewer() {
    // AT-144: the page view paints the Select control only in Scholar
    // mode (select_enabled), and only for a dossier with a paper identity.
    let scholar = RunningHost::scholar(&[], &[]);
    write_dossier(
        &scholar,
        "SRC-0001",
        "title: First Paper",
        "# First Paper\n\nBody.\n",
    );
    let path = scholar.wiki_root.join("papers").join("SRC-0001.md");
    let resp = scholar.post("/v1/open", &serde_json::json!({"path": path.to_string_lossy()}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let handle = resp.json().get("handle").unwrap().as_str().unwrap().to_string();
    let resp = scholar.get(&format!("/v1/frame?handle={handle}&view=page&w=800&h=600"));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let semantics = resp.json().get("semantics").and_then(|v| v.as_array()).unwrap().clone();
    assert!(
        semantics.iter().any(|n| n.get("role").and_then(|r| r.as_str()) == Some("select")),
        "Scholar page frame carries a Select control: {semantics:?}"
    );

    let viewer = RunningHost::viewer(&[], &[]);
    let md = viewer.view_root.join("page.md");
    std::fs::write(&md, "# Title\n\nBody.\n").unwrap();
    let resp = viewer.post("/v1/open", &serde_json::json!({"path": md.to_string_lossy()}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let handle = resp.json().get("handle").unwrap().as_str().unwrap().to_string();
    let resp = viewer.get(&format!("/v1/frame?handle={handle}&view=page&w=800&h=600"));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let semantics = resp.json().get("semantics").and_then(|v| v.as_array()).unwrap().clone();
    assert!(
        !semantics.iter().any(|n| n.get("role").and_then(|r| r.as_str()) == Some("select")),
        "viewer page frame must not carry a Select control: {semantics:?}"
    );
}

#[test]
fn keep_alive_two_requests_one_connection() {
    // AT-230: keep-alive serves a second request on the same connection.
    let host = RunningHost::viewer(&[], &[]);
    let mut stream = host.raw_connection();
    let req = b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";
    stream.write_all(req).unwrap();
    let resp = RunningHost::read_response(&mut *stream);
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"ok\n");
    stream.write_all(req).unwrap();
    let resp = RunningHost::read_response(&mut *stream);
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"ok\n");
}

#[test]
fn idle_connection_closes() {
    // AT-230: after --handle-idle-secs of inactivity the server closes
    // the keep-alive connection.
    let host = RunningHost::viewer_idle(1);
    let mut stream = host.raw_connection();
    let req = b"GET /health HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";
    stream.write_all(req).unwrap();
    let resp = RunningHost::read_response(&mut *stream);
    assert_eq!(resp.status, 200);
    std::thread::sleep(Duration::from_secs(2));
    let mut one = [0u8; 1];
    let n = stream.read(&mut one).unwrap();
    assert_eq!(n, 0, "the idle connection should be closed (EOF)");
}

/// AT-125: no --headless + no DISPLAY/WAYLAND_DISPLAY (native-window
/// build) is a startup error naming DISPLAY.
#[test]
fn window_no_display_is_startup_error() {
    let dir = TempDir::new("window-no-display");
    let port = free_port();
    let (mut argv, _paths) = scholar_argv(&dir, port);
    // Drop --headless so the native-window path is taken.
    argv.retain(|a| a != "--headless");
    let (code, err) = run_expect_exit_hooks(
        &argv,
        &[],
        &["DISPLAY", "WAYLAND_DISPLAY"],
        Duration::from_secs(10),
    );
    assert_eq!(code, 2, "stderr: {err}");
    assert!(err.contains("DISPLAY"), "stderr should name DISPLAY: {err}");
}

/// AT-125: with a DISPLAY and the window_issue_select hook armed, the
/// native-window client issues GET /health, then GET /v1/candidates,
/// then POST /v1/select for the first listed candidate — all real Host
/// HTTP API requests with the Bearer token. The select is observable:
/// a later /v1/deselect for that paper succeeds (200) instead of
/// 404 not_selected.
#[test]
fn window_issue_select_selects_first_candidate() {
    let host = RunningHost::scholar_window(|dir| {
        let path = dir.join("wiki").join("papers").join("SRC-0001.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "---\ntitle: First Paper\n---\n# First Paper\n").unwrap();
    });
    // The dossier must be the first (only) candidate, or the window
    // client has nothing to select.
    let body = candidates(&host);
    let papers = body.get("papers").and_then(|v| v.as_array()).unwrap();
    assert_eq!(papers.len(), 1);
    assert_eq!(papers[0].get("id").and_then(|v| v.as_str()), Some("SRC-0001"));
    // The window client polls /health, then candidates, then selects.
    // Poll for the selection: a /v1/deselect succeeds (200) only once
    // the window client has selected SRC-0001.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut selected = false;
    while Instant::now() < deadline {
        let resp = host.post(
            "/v1/deselect",
            &serde_json::json!({"paper_id": "SRC-0001"}),
        );
        if resp.status == 200 {
            selected = true;
            break;
        }
        assert_eq!(resp.status, 404, "unexpected deselect status: {}", resp.text());
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(selected, "the window client did not select SRC-0001 within 5s");
}

/// AT-231 — IGNORED in this sandbox: the cap counts non-loopback peers,
/// and this sandbox permits loopback peers only. A connection to the
/// machine's non-loopback IPv4 is NAT-redirected to the gateway (the
/// peer answers with plaintext, so the TLS handshake fails), IPv6 is
/// blocked outright ("Operation not permitted"), and binding the client
/// socket to the non-loopback source IP times out (reverse-path
/// filtering). The server code (serve_tcp's 8-connection cap, 9th
/// accepted then closed without handshake/response) is in place and was
/// reviewed; this test runs where a real non-loopback peer exists.
#[ignore]
#[test]
fn non_loopback_connection_cap() {
    // AT-231: non-loopback concurrent connections are capped at 8; the
    // 9th is accepted then closed immediately, without a handshake.
    let dir = TempDir::new("conn-cap");
    let (cert, key) = make_test_cert(&dir);
    let host = RunningHost::scholar_tls(&cert, &key, &[], &[]);
    // The cap counts non-loopback peers only, so the connections go to
    // the machine's non-loopback IP (the 0.0.0.0 listener binds it).
    let ip = non_loopback_ip();
    let mut held = Vec::new();
    for _ in 0..8 {
        let mut s = tls_stream((&ip.to_string(), host.port));
        let req = format!(
            "GET /health HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {}\r\nConnection: keep-alive\r\n\r\n",
            host.token
        );
        s.write_all(req.as_bytes()).unwrap();
        s.flush().unwrap();
        // Read the response (framed: headers + Content-Length body).
        // The first read may race the server's reply, so WouldBlock is
        // retried briefly.
        let mut raw = Vec::new();
        let mut buf = [0u8; 8192];
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            match s.read(&mut buf) {
                Ok(n) => {
                    raw.extend_from_slice(&buf[..n]);
                    if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() > deadline {
                        panic!("no response from the held connection within 5s");
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(e) => panic!("held connection read: {e}"),
            }
        }
        assert!(raw.starts_with(b"HTTP/1.1 200"), "held conn: {:?}", String::from_utf8_lossy(&raw));
        held.push(s);
    }
    // The 9th connection is closed immediately: a plain TCP read sees EOF.
    let mut ninth = std::net::TcpStream::connect((ip, host.port)).unwrap();
    ninth.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let mut one = [0u8; 1];
    let n = ninth.read(&mut one).unwrap();
    assert_eq!(n, 0, "the 9th connection should be closed immediately (EOF)");
    drop(held);
}
