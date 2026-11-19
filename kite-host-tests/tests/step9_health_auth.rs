//! Step 9 — health, token/auth, and startup errors (AT-04–AT-07, AT-45,
//! AT-50–AT-53, AT-125 headless half, AT-190, AT-223).

mod support;

use std::time::Duration;

use support::*;

#[test]
fn health_ok_tokenless() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.get_raw("/health", &[]).unwrap();
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"ok\n");
    assert_eq!(
        resp.header("content-type").as_deref(),
        Some("text/plain")
    );
}

#[test]
fn health_wrong_method_is_405() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.post_raw("/health", Some("application/json"), b"{}");
    assert_eq!(resp.status, 405);
    assert_eq!(resp.error_code(), "method_not_allowed");
}

#[test]
fn health_behind_proxy_requires_token() {
    let host = RunningHost::viewer(&["--behind-proxy"], &[]);
    let no_token = host.get_raw("/health", &[]).unwrap();
    assert_eq!(no_token.status, 401);
    assert_eq!(no_token.error_code(), "unauthenticated");
    let with_token = host.get("/health");
    assert_eq!(with_token.status, 200);
    assert_eq!(with_token.body, b"ok\n");
}

#[test]
fn unknown_path_is_404_after_auth() {
    let host = RunningHost::viewer(&[], &[]);
    let no_token = host.get_raw("/v1/nope", &[]).unwrap();
    assert_eq!(no_token.status, 401);
    assert_eq!(no_token.error_code(), "unauthenticated");
    let resp = host.get("/v1/nope");
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_path");
}

#[test]
fn missing_token_is_401() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host.get_raw("/v1/candidates", &[]).unwrap();
    assert_eq!(resp.status, 401);
    assert_eq!(resp.error_code(), "unauthenticated");
    let body = resp.json();
    assert!(body.get("message").is_some(), "error body carries a message");
}

#[test]
fn wrong_token_is_401() {
    let host = RunningHost::viewer(&[], &[]);
    let resp = host
        .get_raw("/v1/candidates", &[("Authorization", "Bearer wrong-token-0123456789abcdef")])
        .unwrap();
    assert_eq!(resp.status, 401);
    assert_eq!(resp.error_code(), "unauthenticated");
}

#[test]
fn malformed_authorization_is_401() {
    let host = RunningHost::viewer(&[], &[]);
    for header in [
        "Bearer",
        "Bearer ",
        "Basic dGVzdA==",
        "bearer test-token-0123456789abcdefghij0123456789",
        "Bearer test-token-0123456789abcdefghij0123456789 extra",
    ] {
        let resp = host
            .get_raw("/v1/candidates", &[("Authorization", header)])
            .unwrap();
        assert_eq!(resp.status, 401, "header {header:?}");
        assert_eq!(resp.error_code(), "unauthenticated", "header {header:?}");
    }
}

fn startup_exit(argv: Vec<String>) -> (i32, String) {
    run_expect_exit(&argv, &[], Duration::from_secs(10))
}

#[test]
fn token_file_short_is_startup_error() {
    let dir = TempDir::new("token-short");
    let port = free_port();
    let argv = viewer_argv(&dir, port);
    // Replace the token file with a 10-char token.
    let token_file = dir.join("token.txt");
    std::fs::write(&token_file, "short-token").unwrap();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--auth-token-file"), "stderr: {err}");
}

#[test]
fn token_file_bad_charset_is_startup_error() {
    let dir = TempDir::new("token-charset");
    let port = free_port();
    let argv = viewer_argv(&dir, port);
    let token_file = dir.join("token.txt");
    std::fs::write(&token_file, "has a space in it 0123456789abcdef\n").unwrap();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--auth-token-file"), "stderr: {err}");
}

#[test]
fn missing_bind_is_startup_error() {
    let dir = TempDir::new("no-bind");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    // Remove --bind and its value.
    let i = argv.iter().position(|a| a == "--bind").unwrap();
    argv.drain(i..i + 2);
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--bind"), "stderr: {err}");
}

#[test]
fn unknown_flag_is_startup_error() {
    let dir = TempDir::new("unknown-flag");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    argv.push("--bogus-flag".to_string());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--bogus-flag"), "stderr: {err}");
}

#[test]
fn duplicate_flag_is_startup_error() {
    let dir = TempDir::new("dup-flag");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    argv.push("--port".to_string());
    argv.push(port.to_string());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--port"), "stderr: {err}");
}

#[test]
fn removed_flag_is_startup_error() {
    let dir = TempDir::new("removed-flag");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    argv.push("--lrs-actor-homepage".to_string());
    argv.push("https://example.test/".to_string());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--lrs-actor-homepage"), "stderr: {err}");
}

#[test]
fn viewer_with_scholar_flag_is_startup_error() {
    let dir = TempDir::new("viewer-scholar-flag");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    argv.push("--wiki-root".to_string());
    argv.push(dir.join("wiki").to_string_lossy().into_owned());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--wiki-root"), "stderr: {err}");
}

#[test]
fn viewer_without_view_root_is_startup_error() {
    let dir = TempDir::new("viewer-no-view-root");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    let i = argv.iter().position(|a| a == "--view-root").unwrap();
    argv.drain(i..i + 2);
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--view-root"), "stderr: {err}");
}

#[test]
fn port_out_of_range_is_startup_error() {
    for bad in ["0", "70000", "abc"] {
        let dir = TempDir::new("bad-port");
        let port = free_port();
        let mut argv = viewer_argv(&dir, port);
        let i = argv.iter().position(|a| a == "--port").unwrap();
        argv[i + 1] = bad.to_string();
        let (code, err) = startup_exit(argv);
        assert_eq!(code, 2, "port {bad}");
        assert!(err.contains("--port"), "port {bad}: stderr: {err}");
    }
}

#[test]
fn max_open_handles_zero_is_startup_error() {
    let dir = TempDir::new("max-handles-0");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    let i = argv.iter().position(|a| a == "--max-open-handles").unwrap();
    argv[i + 1] = "0".to_string();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--max-open-handles"), "stderr: {err}");
}

#[test]
fn headless_missing_is_startup_error_on_default_build() {
    // The default build has no native-window feature, so --headless is
    // required (AT-125).
    let dir = TempDir::new("no-headless");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    let i = argv.iter().position(|a| a == "--headless").unwrap();
    argv.remove(i);
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--headless"), "stderr: {err}");
}

#[test]
fn tls_flags_on_loopback_are_startup_error() {
    // AT-190: passing TLS flags on a loopback bind is a startup error
    // naming the flag.
    let dir = TempDir::new("tls-loopback");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    argv.push("--tls-cert".to_string());
    argv.push(dir.join("cert.pem").to_string_lossy().into_owned());
    argv.push("--tls-key".to_string());
    argv.push(dir.join("key.pem").to_string_lossy().into_owned());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--tls-cert"), "stderr: {err}");
}

#[test]
fn non_loopback_without_tls_is_startup_error() {
    // AT-190: a non-loopback bind without TLS is a startup error naming
    // --tls-cert.
    let dir = TempDir::new("nonloopback-no-tls");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    let i = argv.iter().position(|a| a == "--bind").unwrap();
    argv[i + 1] = "0.0.0.0".to_string();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--tls-cert"), "stderr: {err}");
}

#[test]
fn fetch_gt_open_is_startup_error_naming_both() {
    // AT-223: --max-fetch-bytes > --max-open-bytes names both flags.
    let host_dir = TempDir::new("fetch-gt-open");
    let dir = TempDir::new("fetch-gt-open-argv");
    // Build a Scholar argv by hand (viewer_argv has no fetch flags).
    let port = free_port();
    let wiki = dir.mkdir("wiki");
    let corpus = dir.mkdir("corpus");
    let snap = dir.mkdir("snap");
    let dl = dir.mkdir("dl");
    let data = dir.mkdir("data");
    let log = dir.mkdir("log");
    let token = dir.write("token.txt", format!("{TOKEN}\n").as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&token, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let argv: Vec<String> = vec![
        "--bind".into(), "127.0.0.1".into(),
        "--port".into(), port.to_string(),
        "--auth-token-file".into(), token.to_string_lossy().into_owned(),
        "--log-dir".into(), log.to_string_lossy().into_owned(),
        "--max-open-handles".into(), "16".into(),
        "--max-frame-pixels".into(), MAX_FRAME_PIXELS.to_string(),
        "--handle-idle-secs".into(), "60".into(),
        "--max-request-bytes".into(), MAX_REQUEST_BYTES.to_string(),
        "--max-open-bytes".into(), "1000".into(),
        "--headless".into(),
        "--wiki-root".into(), wiki.to_string_lossy().into_owned(),
        "--corpus-root".into(), corpus.to_string_lossy().into_owned(),
        "--working-archive".into(), data.join("working.s3a").to_string_lossy().into_owned(),
        "--snapshot-dir".into(), snap.to_string_lossy().into_owned(),
        "--download-dir".into(), dl.to_string_lossy().into_owned(),
        "--data-dir".into(), data.to_string_lossy().into_owned(),
        "--researcher-id".into(), "https://example.test/researcher/ashley".into(),
        "--fetch-timeout-secs".into(), "30".into(),
        "--max-fetch-bytes".into(), "2000".into(),
        "--max-redirects".into(), "5".into(),
    ];
    let _ = host_dir;
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--max-fetch-bytes"), "stderr: {err}");
    assert!(err.contains("--max-open-bytes"), "stderr: {err}");
}
