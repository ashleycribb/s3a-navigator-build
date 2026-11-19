//! Step 9 — /v1/open and /v1/frame (AT-100, AT-113–AT-117, AT-141, AT-143,
//! AT-171, AT-196) plus the HTTP halves of the 8c ATs (AT-30/AT-33).

mod fixture8b;
mod support;

use fixture8b::*;
use support::*;

fn hyper_fixture() -> HyperFixture {
    build_hyper(
        vec![
            HyperTile::Manifest("step9 manifest".into()),
            HyperTile::Raw(68, b"blob-bytes".to_vec(), 1),
        ],
        true,
    )
}

fn open_archive(host: &RunningHost, path: &std::path::Path) -> serde_json::Value {
    let resp = host.post(
        "/v1/open",
        &serde_json::json!({"path": path.to_string_lossy()}),
    );
    assert_eq!(resp.status, 200, "open {}: {}", path.display(), resp.text());
    resp.json()
}

#[test]
fn open_hyper_tile_kind_and_first_view() {
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("a.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    assert_eq!(body.get("kind").and_then(|v| v.as_str()), Some("hyper-tile"));
    assert_eq!(
        body.get("first_view").and_then(|v| v.as_str()),
        Some("manifest")
    );
    assert!(body.get("handle").and_then(|v| v.as_str()).is_some());
}

#[test]
fn open_kind_by_extension() {
    let host = RunningHost::viewer(&[], &[]);
    let md = host.view_root.join("page.md");
    std::fs::write(&md, "# Hello\n\nSome text.\n").unwrap();
    let body = open_archive(&host, &md);
    assert_eq!(body.get("kind").and_then(|v| v.as_str()), Some("wiki"));
    assert_eq!(body.get("first_view").and_then(|v| v.as_str()), Some("page"));

    let html = host.view_root.join("page.html");
    std::fs::write(&html, "<h1>Hi</h1><p>Text.</p>").unwrap();
    let body = open_archive(&host, &html);
    assert_eq!(body.get("kind").and_then(|v| v.as_str()), Some("html"));
    assert_eq!(body.get("first_view").and_then(|v| v.as_str()), Some("page"));

    let txt = host.view_root.join("notes.txt");
    std::fs::write(&txt, "plain text").unwrap();
    let body = open_archive(&host, &txt);
    assert_eq!(body.get("kind").and_then(|v| v.as_str()), Some("other"));
    assert_eq!(body.get("first_view").and_then(|v| v.as_str()), Some("page"));
}

#[test]
fn open_relative_path_is_400() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post("/v1/open", &serde_json::json!({"path": "relative/path.s3a"}));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_param");
    let resp = host.post("/v1/open", &serde_json::json!({"path": ""}));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_param");
}

#[test]
fn open_missing_is_404_not_found() {
    let host = RunningHost::viewer(&[], &[]);
    let missing = host.view_root.join("nope.s3a");
    let resp = host.post(
        "/v1/open",
        &serde_json::json!({"path": missing.to_string_lossy()}),
    );
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "not_found");
}

#[test]
fn open_directory_is_409_not_regular_file() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post(
        "/v1/open",
        &serde_json::json!({"path": host.view_root.to_string_lossy()}),
    );
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "not_regular_file");
}

#[test]
fn open_outside_root_is_409_path_escape() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post("/v1/open", &serde_json::json!({"path": "/etc/passwd"}));
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "path_escape");
}

#[test]
fn open_file_url_accepted_and_other_host_escapes() {
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("u.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let url = format!("file://{}", path.display());
    let resp = host.post("/v1/open", &serde_json::json!({"path": url}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let url = format!("file://localhost{}", path.display());
    let resp = host.post("/v1/open", &serde_json::json!({"path": url}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.post(
        "/v1/open",
        &serde_json::json!({"path": "file://otherhost/etc/passwd"}),
    );
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "path_escape");
}

#[test]
fn open_wrong_content_type_is_415() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post_raw(
        "/v1/open",
        Some("text/plain"),
        b"{\"path\": \"/x\"}",
    );
    assert_eq!(resp.status, 415);
    assert_eq!(resp.error_code(), "unsupported_media_type");
}

#[test]
fn open_malformed_json_is_400() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post_raw("/v1/open", Some("application/json"), b"{not json");
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "malformed_json");
    let resp = host.post("/v1/open", &serde_json::json!({}));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "missing_param");
}

#[test]
fn open_too_many_handles_is_429() {
    let host = RunningHost::viewer_max_handles(1);
    let fx = hyper_fixture();
    let a = host.view_root.join("a.s3a");
    let b = host.view_root.join("b.s3a");
    std::fs::write(&a, &fx.bytes).unwrap();
    std::fs::write(&b, &fx.bytes).unwrap();
    let _ = open_archive(&host, &a);
    let resp = host.post("/v1/open", &serde_json::json!({"path": b.to_string_lossy()}));
    assert_eq!(resp.status, 429);
    assert_eq!(resp.error_code(), "too_many_handles");
}

#[test]
fn frame_manifest_returns_png() {
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("f.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.get(&format!(
        "/v1/frame?handle={handle}&view=manifest&w=800&h=600"
    ));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert_eq!(body.get("width").and_then(|v| v.as_u64()), Some(800));
    assert_eq!(body.get("height").and_then(|v| v.as_u64()), Some(600));
    let png_b64 = body.get("png_base64").and_then(|v| v.as_str()).unwrap();
    let png = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, png_b64)
        .unwrap();
    assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "PNG signature");
    assert!(body.get("text_runs").and_then(|v| v.as_array()).is_some());
    assert!(body.get("semantics").and_then(|v| v.as_array()).is_some());
}

#[test]
fn frame_banner_renders_manifest() {
    // DD-81: view=banner calls render_manifest.
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("b.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=banner&w=800&h=600"));
    assert_eq!(resp.status, 200, "{}", resp.text());
}

#[test]
fn frame_unknown_view_beats_unknown_handle() {
    // AT-100/AT-196: an unknown view beats a missing/mistyped handle.
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.get("/v1/frame?handle=h-deadbeef&view=bogus&w=800&h=600");
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_view");
}

#[test]
fn frame_unknown_handle_is_404() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.get("/v1/frame?handle=h-deadbeef&view=manifest&w=800&h=600");
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_handle");
}

#[test]
fn frame_param_errors() {
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("p.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    // w=0 -> invalid_frame_size.
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=manifest&w=0&h=600"));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_frame_size");
    // Missing w -> missing_param.
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=manifest&h=600"));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "missing_param");
    // Missing view -> missing_param.
    let resp = host.get(&format!("/v1/frame?handle={handle}&w=800&h=600"));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "missing_param");
    // floor on a non-floor view -> invalid_param.
    let resp = host.get(&format!(
        "/v1/frame?handle={handle}&view=manifest&w=800&h=600&floor=1"
    ));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_param");
    // floor-detail without floor -> missing_param.
    let resp = host.get(&format!(
        "/v1/frame?handle={handle}&view=floor-detail&w=800&h=600"
    ));
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "missing_param");
    // floor-detail with an out-of-range floor -> invalid_floor. The
    // view matrix is checked before the floor range, so proceed first.
    let resp = host.post("/v1/proceed", &serde_json::json!({"handle": handle}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.get(&format!(
        "/v1/frame?handle={handle}&view=floor-detail&floor=99&w=800&h=600"
    ));
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "invalid_floor");
}

#[test]
fn frame_page_renders_markdown() {
    // AT-141/AT-143 HTTP half: the page view renders Markdown.
    let host = RunningHost::viewer(&[], &[]);
    let md = host.view_root.join("doc.md");
    std::fs::write(&md, "# Title\n\nBody text here.\n").unwrap();
    let body = open_archive(&host, &md);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=page&w=800&h=600"));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    let runs = body.get("text_runs").and_then(|v| v.as_array()).unwrap();
    let joined: String = runs
        .iter()
        .filter_map(|r| r.get("text").and_then(|t| t.as_str()))
        .collect();
    assert!(joined.contains("Title"), "text runs: {joined}");
    assert!(joined.contains("Body text here"), "text runs: {joined}");
}

#[test]
fn hit_test_needs_a_prior_frame() {
    // AT-118: hit-test with no prior frame for (handle, view, floor) is
    // 409 no_frame.
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("h.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.post(
        "/v1/hit-test",
        &serde_json::json!({"handle": handle, "view": "manifest", "x": 10.0, "y": 10.0}),
    );
    assert_eq!(resp.status, 409);
    assert_eq!(resp.error_code(), "no_frame");
}

#[test]
fn hit_test_after_frame_and_last_frame_cache() {
    // AT-118/AT-196: after a frame, hit-test works; framing a different
    // view keeps the manifest frame cached (keyed by (handle, view, floor)).
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("h2.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=manifest&w=800&h=600"));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.post(
        "/v1/hit-test",
        &serde_json::json!({"handle": handle, "view": "manifest", "x": 10.0, "y": 10.0}),
    );
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert!(body.get("hit").and_then(|v| v.as_str()).is_some());
    // Frame a different view, then hit-test the manifest view again: the
    // manifest frame is still cached. Warehouse needs proceed first.
    let resp = host.post("/v1/proceed", &serde_json::json!({"handle": handle}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=warehouse&w=800&h=600"));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.post(
        "/v1/hit-test",
        &serde_json::json!({"handle": handle, "view": "manifest", "x": 10.0, "y": 10.0}),
    );
    assert_eq!(resp.status, 200, "{}", resp.text());
}

#[test]
fn hit_test_unknown_view_beats_handle() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post(
        "/v1/hit-test",
        &serde_json::json!({"handle": "h-deadbeef", "view": "bogus", "x": 1.0, "y": 1.0}),
    );
    assert_eq!(resp.status, 400);
    assert_eq!(resp.error_code(), "invalid_view");
}

#[test]
fn proceed_and_close() {
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("c.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.post("/v1/proceed", &serde_json::json!({"handle": handle}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.json().get("ok").and_then(|v| v.as_bool()), Some(true));
    let resp = host.post("/v1/close", &serde_json::json!({"handle": handle}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.get(&format!("/v1/frame?handle={handle}&view=manifest&w=800&h=600"));
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_handle");
}

#[test]
fn close_unknown_handle_is_404() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post("/v1/close", &serde_json::json!({"handle": "h-deadbeef"}));
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_handle");
}

#[test]
fn frame_t_is_clamped_not_an_error() {
    // AT-196: t outside [0, 0.45] is clamped, not an error.
    let host = RunningHost::viewer(&[], &[]);
    let fx = hyper_fixture();
    let path = host.view_root.join("t.s3a");
    std::fs::write(&path, &fx.bytes).unwrap();
    let body = open_archive(&host, &path);
    let handle = body.get("handle").unwrap().as_str().unwrap();
    let resp = host.post("/v1/proceed", &serde_json::json!({"handle": handle}));
    assert_eq!(resp.status, 200, "{}", resp.text());
    let resp = host.get(&format!(
        "/v1/frame?handle={handle}&view=floor-transition&floor=0&t=0.9&w=800&h=600"
    ));
    assert_eq!(resp.status, 200, "{}", resp.text());
}

#[test]
fn body_over_cap_is_413_after_auth() {
    // AT-162/AT-171: a body over --max-request-bytes is 413 *after* auth.
    let host = RunningHost::viewer(&[], &[]);
    let big = vec![b'x'; (MAX_REQUEST_BYTES + 100) as usize];
    let resp = host.post_raw("/v1/open", Some("application/json"), &big);
    assert_eq!(resp.status, 413);
    assert_eq!(resp.error_code(), "request_too_large");
    // Without a token, auth (401) beats the body cap.
    let mut req = format!(
        "POST /v1/open HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        big.len()
    );
    let mut bytes = req.into_bytes();
    bytes.extend_from_slice(&big);
    let resp = host.roundtrip(&bytes);
    assert_eq!(resp.status, 401);
    assert_eq!(resp.error_code(), "unauthenticated");
}

#[test]
fn header_block_over_cap_is_413_before_auth() {
    // AT-162: a header block over --max-request-bytes is 413 *before* auth.
    let host = RunningHost::viewer(&[], &[]);
    let big_value = "x".repeat((MAX_REQUEST_BYTES + 100) as usize);
    let req = format!(
        "GET /health HTTP/1.1\r\nHost: localhost\r\nX-Big: {big_value}\r\nConnection: close\r\n\r\n"
    );
    let resp = host.roundtrip(req.as_bytes());
    assert_eq!(resp.status, 413);
    assert_eq!(resp.error_code(), "request_too_large");
}
