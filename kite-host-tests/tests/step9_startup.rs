//! Step 9 — startup: sidecar locks (AT-164/AT-208), Unix socket
//! (AT-174/AT-163), TLS (AT-163/AT-190), consent bootstrap (AT-151/AT-114),
//! working archive (AT-78/AT-79/AT-203), temp cleanup (AT-187), inclusion
//! index (AT-224).

mod support;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use support::*;

fn startup_exit(argv: Vec<String>) -> (i32, String) {
    run_expect_exit(&argv, &[], Duration::from_secs(10))
}

#[test]
fn second_host_same_data_dir_fails() {
    // AT-164/AT-208: a second host with the same --data-dir is a startup
    // error naming --data-dir, binds nothing, mutates nothing.
    let a = RunningHost::scholar(&[], &[]);
    let dir = TempDir::new("second-host");
    let port = free_port();
    let (mut argv, paths) = scholar_argv(&dir, port);
    // Same --data-dir as A, everything else fresh.
    let i = argv.iter().position(|x| x == "--data-dir").unwrap();
    argv[i + 1] = a.data_dir.to_string_lossy().into_owned();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--data-dir"), "stderr: {err}");
    let _ = paths;
}

#[test]
fn lock_files_created() {
    // AT-164: the sidecar lock files exist while a Scholar host runs.
    let host = RunningHost::scholar(&[], &[]);
    for lock in [
        host.data_dir.join(".lock"),
        host.dir.join("working").join("working.s3a.lock"),
        host.snapshot_dir.join(".lock"),
        host.download_dir.join(".lock"),
    ] {
        assert!(lock.is_file(), "lock file missing: {}", lock.display());
    }
}

#[test]
fn unix_socket_health_and_mode() {
    // AT-174: a Unix-socket host serves /health over the socket, and the
    // socket is created mode 0600.
    let host = RunningHost::viewer_unix(&[], &[]);
    let sock = host.socket_path.as_ref().unwrap();
    let mode = std::fs::metadata(sock).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "socket mode {mode:o}");
    let resp = host.get("/health");
    assert_eq!(resp.status, 200);
    assert_eq!(resp.body, b"ok\n");
}

#[test]
fn unix_socket_existing_file_is_startup_error() {
    // AT-163: an existing file at the socket path is a startup error, and
    // the file is not unlinked.
    let dir = TempDir::new("unix-exists");
    let sock = dir.join("host.sock");
    std::fs::write(&sock, b"not a socket").unwrap();
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    let i = argv.iter().position(|a| a == "--bind").unwrap();
    argv[i + 1] = format!("unix:{}", sock.display());
    let j = argv.iter().position(|a| a == "--port").unwrap();
    argv.drain(j..j + 2);
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--bind"), "stderr: {err}");
    assert_eq!(
        std::fs::read(&sock).unwrap(),
        b"not a socket",
        "the existing file was not unlinked"
    );
}

#[test]
fn unix_socket_bad_parent_is_startup_error() {
    let dir = TempDir::new("unix-bad-parent");
    let port = free_port();
    let mut argv = viewer_argv(&dir, port);
    let i = argv.iter().position(|a| a == "--bind").unwrap();
    argv[i + 1] = "unix:/nonexistent-dir-xyz/host.sock".to_string();
    let j = argv.iter().position(|a| a == "--port").unwrap();
    argv.drain(j..j + 2);
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--bind"), "stderr: {err}");
}

#[test]
fn tls_health_over_rustls_client() {
    // AT-163: a non-loopback TLS host serves /health over TLS 1.2+.
    let dir = TempDir::new("tls-health");
    let (cert, key) = make_test_cert(&dir);
    let host = RunningHost::scholar_tls(&cert, &key, &[], &[]);
    let resp = host.tls_get("/health");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(resp.body, b"ok\n");
    // The plaintext port is not serving HTTP (it is the TLS port).
    let plain = host.get_raw_result("/health", &[]);
    assert!(plain.is_err() || plain.map(|r| r.status).unwrap_or(0) != 200);
}

#[test]
fn tls_11_is_refused() {
    // AT-163: TLS 1.1 is refused. Uses the openssl CLI.
    let dir = TempDir::new("tls11");
    let (cert, key) = make_test_cert(&dir);
    let host = RunningHost::scholar_tls(&cert, &key, &[], &[]);
    let out = std::process::Command::new("openssl")
        .args([
            "s_client",
            "-connect",
            &format!("127.0.0.1:{}", host.port),
            "-tls1_1",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap_or_else(|e| panic!("openssl is required for the TLS tests: {e}"));
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success() || combined.contains("alert") || combined.contains("handshake failure"),
        "TLS 1.1 was accepted: {combined}"
    );
}

#[test]
fn consent_file_bootstrapped_off() {
    // AT-151: with --lrs-url, the consent file is created as off + an
    // RFC3339-seconds-UTC timestamp, mode 0600.
    let host = RunningHost::scholar_with_lrs(&[], &[]);
    let consent = host.dir.join("consent.txt");
    let mode = std::fs::metadata(&consent).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "consent file mode {mode:o}");
    let text = std::fs::read_to_string(&consent).unwrap();
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("off"));
    let ts = lines.next().expect("a timestamp line");
    assert!(
        ts.len() == 20 && ts.ends_with('Z') && ts.as_bytes()[10] == b'T',
        "timestamp is RFC3339 seconds UTC: {ts}"
    );
    assert_eq!(lines.next(), None);
}

#[test]
fn consent_file_malformed_is_startup_error() {
    let dir = TempDir::new("consent-malformed");
    let port = free_port();
    let (mut argv, paths) = scholar_argv(&dir, port);
    let consent = dir.join("consent.txt");
    std::fs::write(&consent, "garbage\n").unwrap();
    let creds = dir.write("lrs-creds.txt", b"u:p");
    std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
    argv.push("--lrs-url".into());
    argv.push("http://127.0.0.1:9/xapi".into());
    argv.push("--consent-state-file".into());
    argv.push(consent.to_string_lossy().into_owned());
    argv.push("--lrs-credentials-file".into());
    argv.push(creds.to_string_lossy().into_owned());
    argv.push("--lrs-timeout-secs".into());
    argv.push("5".into());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--consent-state-file"), "stderr: {err}");
    let _ = paths;
}

#[test]
fn consent_without_lrs_is_startup_error() {
    // AT-114: --consent-state-file without --lrs-url is a startup error.
    let dir = TempDir::new("consent-no-lrs");
    let port = free_port();
    let (mut argv, _paths) = scholar_argv(&dir, port);
    argv.push("--consent-state-file".into());
    argv.push(dir.join("consent.txt").to_string_lossy().into_owned());
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--consent-state-file"), "stderr: {err}");
}

#[test]
fn working_archive_created_and_valid() {
    // AT-78/AT-79: a missing working archive is created at startup, and
    // it is a valid hyper-tile archive (the pack opens it).
    let host = RunningHost::scholar(&[], &[]);
    let wa = host.dir.join("working").join("working.s3a");
    assert!(wa.is_file(), "working archive was not created");
    let bytes = std::fs::read(&wa).unwrap();
    assert!(!bytes.is_empty());
    let ctx = test_context();
    let h = s3a_pack_kite::open(
        &ctx,
        &bytes,
        None,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
    )
    .expect("the working archive opens in the pack");
    assert_eq!(h.kind(), "hyper-tile");
}

#[test]
fn working_archive_symlink_is_startup_error() {
    // AT-79: a symlink as the working archive's final component is a
    // startup error naming --working-archive.
    let dir = TempDir::new("wa-symlink");
    let port = free_port();
    let (mut argv, paths) = scholar_argv(&dir, port);
    let target = dir.join("real.s3a");
    std::fs::write(&target, b"").unwrap();
    let link = dir.join("link.s3a");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let i = argv.iter().position(|a| a == "--working-archive").unwrap();
    argv[i + 1] = link.to_string_lossy().into_owned();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--working-archive"), "stderr: {err}");
    let _ = paths;
}

#[test]
fn temp_cleanup_removes_stale_with_marker() {
    // AT-187: a stale s3a-kite-tmp-* dir in --snapshot-dir with a
    // same-euid flockable marker is removed at startup; one without a
    // marker is kept.
    let host = RunningHost::scholar_with_setup(|dir| {
        let snap = dir.join("snapshot");
        let stale = snap.join("s3a-kite-tmp-99999-abc");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("marker"), b"").unwrap();
        let kept = snap.join("s3a-kite-tmp-99999-no-marker");
        std::fs::create_dir_all(&kept).unwrap();
    });
    assert!(
        !host.snapshot_dir.join("s3a-kite-tmp-99999-abc").exists(),
        "the stale dir with a marker was not removed"
    );
    assert!(
        host.snapshot_dir
            .join("s3a-kite-tmp-99999-no-marker")
            .exists(),
        "the dir without a marker was removed"
    );
}

#[test]
fn inclusion_index_symlink_is_startup_error() {
    // AT-224: a symlink inclusion-index is a startup error naming the path.
    let dir = TempDir::new("index-symlink");
    let port = free_port();
    let (argv, paths) = scholar_argv(&dir, port);
    let target = dir.join("real-index.jsonl");
    std::fs::write(&target, "{}\n").unwrap();
    std::os::unix::fs::symlink(&target, paths.data.join("inclusion-index.jsonl")).unwrap();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--data-dir"), "stderr: {err}");
}

#[test]
fn inclusion_index_malformed_line_is_startup_error() {
    let dir = TempDir::new("index-malformed");
    let port = free_port();
    let (argv, paths) = scholar_argv(&dir, port);
    std::fs::write(paths.data.join("inclusion-index.jsonl"), "not json\n").unwrap();
    let (code, err) = startup_exit(argv);
    assert_eq!(code, 2);
    assert!(err.contains("--data-dir"), "stderr: {err}");
}
