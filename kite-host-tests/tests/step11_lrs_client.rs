//! Step 11 — LRS client, retries, identity, actor homepage (§5.5,
//! DD-51/DD-65/DD-134/DD-144/DD-155/DD-156/DD-157, Q-02/Q-08/Q-09/Q-13).
//! Fail-first acceptance tests for the step-11 ATs. Step 10 landed the
//! step-3 emit (statement build, POST, classification, retry-file
//! write); step 11 adds the latch + cancellation token, the revoke
//! persist sequence, startup queue handling, the background retry
//! worker, the failure record, and study-mode identity validation.

mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use base64::Engine as _;
use sha2::Digest;
use support::*;

const EXT_PAPER_ID: &str = "https://ashecribb.dev/xapi/ext/paper-id";
const EXT_SOURCE_URL: &str = "https://ashecribb.dev/xapi/ext/source-url";
const EXT_CONTENT_SHA256: &str = "https://ashecribb.dev/xapi/ext/content-sha256";
const LAB_HOMEPAGE: &str = "https://34.74.163.88.sslip.io/scholar/lab";

/// A Scholar host with an LRS fixture on `::1` (so the LRS host and the
/// fetch fixture on 127.0.0.1 never collide under the LRS-host deny rule,
/// AT-54 second case) and an egress-test fixture on 127.0.0.1.
fn scholar_with_lrs(fixture: &FixtureServer, consent: bool) -> (RunningHost, LrsServer) {
    let lrs = LrsServer::new("::1");
    let opts = ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: consent,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    };
    (RunningHost::scholar_opts(opts), lrs)
}

/// Write a dossier under `host.wiki_root/papers/<stem>.md`.
fn dossier(host: &RunningHost, stem: &str, front_matter: &str, body: &str) -> std::path::PathBuf {
    write_dossier_at(&host.wiki_root, stem, front_matter, body)
}

fn sha256_hex(data: &[u8]) -> String {
    let mut h = sha2::Sha256::new();
    h.update(data);
    format!("{:x}", h.finalize())
}

/// Poll `cond` until true or `timeout` elapses; panic naming `what`.
fn wait_until(timeout: Duration, what: &str, mut cond: impl FnMut() -> bool) {
    let deadline = Instant::now() + timeout;
    loop {
        if cond() {
            return;
        }
        if Instant::now() > deadline {
            panic!("timed out waiting for {what}");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// `{id}.json` retry files under `--data-dir/lrs-retry/`.
fn retry_names(data_dir: &std::path::Path) -> Vec<String> {
    dir_files(&data_dir.join("lrs-retry"))
        .into_iter()
        .filter(|n| n.ends_with(".json"))
        .collect()
}

/// `{id}.permanent` files under `--data-dir/lrs-retry/`.
fn permanent_names(data_dir: &std::path::Path) -> Vec<String> {
    dir_files(&data_dir.join("lrs-retry"))
        .into_iter()
        .filter(|n| n.ends_with(".permanent"))
        .collect()
}

/// The `--data-dir/lrs-failures.jsonl` records (DD-156, AT-236).
fn failure_records(data_dir: &std::path::Path) -> Vec<serde_json::Value> {
    let Ok(text) = std::fs::read_to_string(data_dir.join("lrs-failures.jsonl")) else {
        return Vec::new();
    };
    text.lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .collect()
}

/// Stop `host` and spawn a new host on the same directories.
fn restart(mut host: RunningHost, opts: ScholarOpts) -> RunningHost {
    host.stop();
    let dir = host.take_dir();
    drop(host);
    RunningHost::scholar_opts_in(dir, opts)
}

/// Send a POST and return the open connection without reading the
/// response (for requests the test revokes or parks mid-flight).
fn send_post_raw(host: &RunningHost, path: &str, body: &[u8]) -> TcpStream {
    let mut stream = TcpStream::connect(("127.0.0.1", host.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        host.token,
        body.len()
    );
    let mut bytes = req.into_bytes();
    bytes.extend_from_slice(body);
    stream.write_all(&bytes).unwrap();
    stream.flush().unwrap();
    stream
}

/// Read one HTTP response from an open connection (mirrors
/// `RunningHost::read_response` for a plain `TcpStream`).
fn read_resp(stream: &mut TcpStream) -> Resp {
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                raw.extend_from_slice(&buf[..n]);
                if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                    let head_end = raw
                        .windows(4)
                        .position(|w| w == b"\r\n\r\n")
                        .map(|p| p + 4)
                        .unwrap();
                    let head = String::from_utf8_lossy(&raw[..head_end]).into_owned();
                    let content_len: usize = head
                        .lines()
                        .find_map(|l| {
                            let (n, v) = l.split_once(':')?;
                            if n.trim().eq_ignore_ascii_case("content-length") {
                                v.trim().parse().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                    if raw.len() >= head_end + content_len {
                        break;
                    }
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&raw).into_owned();
    let mut parts = text.splitn(2, "\r\n\r\n");
    let head = parts.next().unwrap_or("");
    let body = parts.next().unwrap_or("").as_bytes().to_vec();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .unwrap_or("")
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

/// Send a consent POST over a raw socket and release the parked
/// `pause_after_consent_receipt` hook for it (the hook fires on every
/// consent receipt, so a synchronous `host.consent_on()` would park and
/// time out). Returns the response. Only for a single consent POST
/// with no concurrency (the off/on race tests park both first, then
/// release both).
fn consent_raw_release(host: &RunningHost, dir: &TempDir, state: &str) -> Resp {
    let ready = dir.join("hook-ready");
    let _ = std::fs::remove_file(&ready);
    let body = format!("{{\"state\":\"{state}\"}}");
    let mut conn = send_post_raw(host, "/v1/consent", body.as_bytes());
    wait_until(Duration::from_secs(10), "consent parked", || ready.exists());
    std::fs::write(dir.join("hook-continue"), b"go\n").unwrap();
    read_resp(&mut conn)
}

/// Is this a v4 UUID string (`xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`)?
fn is_uuid_v4(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b[8] == b'-'
        && b[13] == b'-'
        && b[18] == b'-'
        && b[23] == b'-'
        && b[14] == b'4'
        && s.chars()
            .enumerate()
            .all(|(i, c)| [8, 13, 18, 23].contains(&i) || c.is_ascii_hexdigit())
}

fn basic_auth(user: &str, pass: &str) -> String {
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"))
    )
}

fn startup_exit(argv: Vec<String>) -> (i32, String) {
    run_expect_exit(&argv, &[], Duration::from_secs(10))
}

fn startup_exit_hooks(argv: Vec<String>, env: &[(&str, &str)]) -> (i32, String) {
    run_expect_exit_hooks(&argv, env, &[], Duration::from_secs(10))
}

/// A Scholar argv (no LRS) rooted at `dir`, plus the LRS flags for the
/// given fixture URL. Returns the argv and the consent/creds paths.
fn lrs_argv(
    dir: &TempDir,
    port: u16,
    lrs_url: &str,
) -> (Vec<String>, std::path::PathBuf, std::path::PathBuf) {
    let (mut argv, _paths) = scholar_argv(dir, port);
    let consent = dir.join("consent.txt");
    let creds = dir.write("lrs-creds.txt", b"lrs-user:lrs-pass");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    argv.push("--lrs-url".into());
    argv.push(lrs_url.to_string());
    argv.push("--consent-state-file".into());
    argv.push(consent.to_string_lossy().into_owned());
    argv.push("--lrs-credentials-file".into());
    argv.push(creds.to_string_lossy().into_owned());
    argv.push("--lrs-timeout-secs".into());
    argv.push("5".into());
    (argv, consent, creds)
}

/// Replace the value after `flag` in `argv` (the flag is already
/// present exactly once), or append the pair if it is absent.
fn set_argv_value(argv: &mut Vec<String>, flag: &str, value: &str) {
    if let Some(i) = argv.iter().position(|a| a == flag) {
        argv[i + 1] = value.to_string();
    } else {
        argv.push(flag.to_string());
        argv.push(value.to_string());
    }
}

/// Env vars for a hooks-binary host: S3A_KITE_TEST_HOOKS plus the
/// READY/CONTINUE signal paths under `dir` (required when a pause
/// hook is configured; harmless otherwise).
fn hook_env_vars(dir: &TempDir, hooks: &str) -> Vec<(String, String)> {
    vec![
        ("S3A_KITE_TEST_HOOKS".to_string(), hooks.to_string()),
        (
            "S3A_KITE_TEST_HOOK_READY".to_string(),
            dir.join("hook-ready").to_string_lossy().into_owned(),
        ),
        (
            "S3A_KITE_TEST_HOOK_CONTINUE".to_string(),
            dir.join("hook-continue").to_string_lossy().into_owned(),
        ),
    ]
}

// =====================================================================
// AT-110 — the statement shape (§5.5, DD-65)
// =====================================================================

#[test]
fn lrs_statement_shape_full() {
    // AT-110: with consent on, the LRS request is POST
    // {lrs-url}/xapi/statements with Basic auth from the credentials
    // file, X-Experience-API-Version 1.0.3, and a statement whose id,
    // actor.account, verb.id, object.id, timestamp, and EXT_PAPER_ID
    // match §5.5. When the candidate has a source_url that redirected,
    // object.id and EXT_SOURCE_URL are the candidate's listed
    // pre-redirect source_url, not the final redirected URL.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"redirected paper body\n");
    fixture.set_redirect_chain(1, "/paper-1");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/c0", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    // A download fetches (following the redirect) and emits the saved
    // statement; the statement still names the pre-redirect URL.
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(fixture.get_count("/c0"), 1, "the fetch ran once");
    assert_eq!(lrs.post_count(), 1, "one LRS POST");
    assert_eq!(lrs.paths(), vec!["/xapi/statements"]);
    assert_eq!(lrs.auth_headers(), vec![basic_auth("lrs-user", "lrs-pass")]);
    assert_eq!(lrs.api_versions(), vec!["1.0.3"]);
    let stmts = lrs.statements();
    assert_eq!(stmts.len(), 1);
    let st = &stmts[0];
    let id = st.get("id").and_then(|v| v.as_str()).unwrap_or("");
    assert!(is_uuid_v4(id), "statement id is a v4 UUID: {id}");
    assert_eq!(st.get("actor").and_then(|v| v.get("objectType")).and_then(|v| v.as_str()), Some("Agent"));
    assert_eq!(
        st.get("actor").and_then(|v| v.get("account")).and_then(|v| v.get("homePage")).and_then(|v| v.as_str()),
        Some(LAB_HOMEPAGE)
    );
    assert_eq!(
        st.get("actor").and_then(|v| v.get("account")).and_then(|v| v.get("name")).and_then(|v| v.as_str()),
        Some("https://example.test/researcher/ashley")
    );
    assert_eq!(
        st.get("verb").and_then(|v| v.get("id")).and_then(|v| v.as_str()),
        Some("https://ashecribb.dev/xapi/verbs/saved")
    );
    assert_eq!(
        st.get("object").and_then(|v| v.get("id")).and_then(|v| v.as_str()),
        Some(url.as_str()),
        "object.id is the pre-redirect source_url"
    );
    let ext = st
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap_or_else(|| panic!("no extensions: {st}"));
    assert_eq!(ext.get(EXT_PAPER_ID).and_then(|v| v.as_str()), Some("SRC-0001"));
    assert_eq!(
        ext.get(EXT_SOURCE_URL).and_then(|v| v.as_str()),
        Some(url.as_str()),
        "EXT_SOURCE_URL is the pre-redirect source_url"
    );
    let ts = st.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
    assert!(ts.ends_with('Z') && ts.contains('T') && ts.len() == 20, "RFC3339 seconds UTC: {ts}");
}

#[test]
fn lrs_statement_shape_fallback_object_id() {
    // AT-110: when the candidate has no source_url, object.id is the
    // fallback IRI (activities/paper/ + percent-encoded identity) and
    // EXT_SOURCE_URL is absent.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    dossier(&host, "SRC-0007", "title: No Source", "# No Source\n");
    let resp = host.select("SRC-0007");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(lrs.post_count(), 1);
    let st = &lrs.statements()[0];
    assert_eq!(
        st.get("object").and_then(|v| v.get("id")).and_then(|v| v.as_str()),
        Some("https://ashecribb.dev/xapi/activities/paper/SRC-0007")
    );
    let ext = st
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap();
    assert!(ext.get(EXT_SOURCE_URL).is_none(), "no EXT_SOURCE_URL: {ext}");
    assert_eq!(ext.get(EXT_PAPER_ID).and_then(|v| v.as_str()), Some("SRC-0007"));
}

#[test]
fn lrs_statement_shape_content_sha256() {
    // AT-110 + DD-137: EXT_CONTENT_SHA256 is present iff, at step 3, the
    // inclusion index has a non-null snapshot_sha256 for the key. An
    // on-disk-only download omits it; a snapshotted select includes it.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"snapshot body\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    // On-disk-only download: no snapshot, no EXT_CONTENT_SHA256.
    let data = b"on-disk paper bytes\n";
    let data_dir = host.wiki_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("SRC-0001.txt"), data).unwrap();
    dossier(
        &host,
        "SRC-0001",
        "title: On Disk\ndrive_path: ../data/SRC-0001.txt",
        "# On Disk\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    let st = &lrs.statements()[0];
    let ext = st
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap();
    assert!(
        ext.get(EXT_CONTENT_SHA256).is_none(),
        "on-disk-only download omits EXT_CONTENT_SHA256: {ext}"
    );
    // Snapshotted download: the statement carries the recorded sha.
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0002",
        &format!("title: Fetched\nsource_url: {url}"),
        "# Fetched\n",
    );
    let resp = host.download("SRC-0002");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    let st = &lrs.statements()[1];
    let ext = st
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap();
    let sha = sha256_hex(b"snapshot body\n");
    assert_eq!(
        ext.get(EXT_CONTENT_SHA256).and_then(|v| v.as_str()),
        Some(sha.as_str()),
        "snapshotted download carries the snapshot sha"
    );
    // A snapshotted select includes it too (DD-137).
    let resp = host.select("SRC-0002");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let st = &lrs.statements()[2];
    let ext = st
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap();
    assert_eq!(
        ext.get(EXT_CONTENT_SHA256).and_then(|v| v.as_str()),
        Some(sha.as_str())
    );
}

// =====================================================================
// AT-191 — LRS HTTP classification (DD-134)
// =====================================================================

#[test]
fn lrs_409_is_success_first_emit() {
    // AT-191: a fixture that returns 409 is success on a first emit
    // (lrs_status "sent"; the statement is not retried).
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(409);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(lrs.post_count(), 1);
    assert!(retry_names(&host.data_dir).is_empty(), "no retry file");
    assert!(permanent_names(&host.data_dir).is_empty(), "no permanent file");
    std::thread::sleep(Duration::from_secs(2));
    assert_eq!(lrs.post_count(), 1, "no retry POST");
}

#[test]
fn lrs_400_is_permanent_no_retry() {
    // AT-191: a fixture that returns 400 is a permanent failure
    // (lrs_status "failed", a {id}.permanent file is kept, and a later
    // retry pass does not POST that statement again).
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(400);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("failed")
    );
    assert_eq!(lrs.post_count(), 1);
    assert!(retry_names(&host.data_dir).is_empty(), "no sendable retry file");
    assert_eq!(permanent_names(&host.data_dir).len(), 1, "one .permanent file");
    // A full retry interval passes with no further POST.
    std::thread::sleep(Duration::from_secs(12));
    assert_eq!(lrs.post_count(), 1, "no retry POST after 12 s");
    assert_eq!(permanent_names(&host.data_dir).len(), 1);
}

#[test]
fn lrs_408_queued_and_retried() {
    // AT-191: a fixture that returns 408 is queued as {id}.json and
    // retried (the retry resends the same statement id).
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(408);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    assert_eq!(retry_names(&host.data_dir).len(), 1);
    let first_id = lrs.statements()[0]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    lrs.set_status(200);
    wait_until(Duration::from_secs(25), "retry POST", || lrs.post_count() >= 2);
    assert_eq!(lrs.post_count(), 2);
    assert_eq!(
        lrs.statements()[1].get("id").and_then(|v| v.as_str()),
        Some(first_id.as_str()),
        "the retry resends the same statement id"
    );
    wait_until(Duration::from_secs(5), "queue file gone", || {
        retry_names(&host.data_dir).is_empty()
    });
}

#[test]
fn lrs_429_queued_and_retried() {
    // AT-191: a fixture that returns 429 is queued as {id}.json and
    // retried.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(429);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    assert_eq!(retry_names(&host.data_dir).len(), 1);
    lrs.set_status(200);
    wait_until(Duration::from_secs(25), "retry POST", || lrs.post_count() >= 2);
    wait_until(Duration::from_secs(5), "queue file gone", || {
        retry_names(&host.data_dir).is_empty()
    });
}

#[test]
fn lrs_retry_resends_same_id_after_503() {
    // AT-110: a retry after an LRS 5xx resends the same id.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(503);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    let first_id = lrs.statements()[0]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    lrs.set_status(200);
    wait_until(Duration::from_secs(25), "retry POST", || lrs.post_count() >= 2);
    assert_eq!(
        lrs.statements()[1].get("id").and_then(|v| v.as_str()),
        Some(first_id.as_str())
    );
    wait_until(Duration::from_secs(5), "queue file gone", || {
        retry_names(&host.data_dir).is_empty()
    });
}

#[test]
fn lrs_oversized_remainder_follows_status() {
    // AT-191: a fixture that returns a complete status line and then an
    // oversized remainder (header or body exceeding --max-open-bytes)
    // is decided by the status class; the host does not read further.
    // 2xx → sent; 503 → queued; other 4xx → permanent. A header block
    // that exceeds the cap before a status line is read is permanent.
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    // 200 + oversized remainder → sent.
    let mut raw200 = b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec();
    raw200.extend_from_slice(&vec![b'X'; 8192]);
    lrs.set_raw_response(Some(raw200));
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        max_open_bytes: 1024,
        max_fetch_bytes: 1024,
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent"),
        "200 + oversized remainder is sent"
    );
    assert!(retry_names(&host.data_dir).is_empty());
    drop(host);
    // 503 + oversized remainder → queued.
    let mut raw503 = b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n".to_vec();
    raw503.extend_from_slice(&vec![b'X'; 8192]);
    lrs.set_raw_response(Some(raw503));
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        max_open_bytes: 1024,
        max_fetch_bytes: 1024,
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued"),
        "503 + oversized remainder is queued"
    );
    assert_eq!(retry_names(&host.data_dir).len(), 1);
    drop(host);
    // 400 + oversized remainder → permanent.
    let mut raw400 = b"HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n".to_vec();
    raw400.extend_from_slice(&vec![b'X'; 8192]);
    lrs.set_raw_response(Some(raw400));
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        max_open_bytes: 1024,
        max_fetch_bytes: 1024,
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("failed"),
        "400 + oversized remainder is permanent"
    );
    assert_eq!(permanent_names(&host.data_dir).len(), 1);
    drop(host);
    // No status line before the cap → permanent.
    lrs.set_raw_response(Some(vec![b'X'; 8192]));
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        max_open_bytes: 1024,
        max_fetch_bytes: 1024,
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("failed"),
        "header block over the cap before a status line is permanent"
    );
    assert_eq!(permanent_names(&host.data_dir).len(), 1);
    // The permanent statement is never re-POSTed.
    let posts = lrs.post_count();
    std::thread::sleep(Duration::from_secs(12));
    assert_eq!(lrs.post_count(), posts, "no retry POST for a permanent failure");
}

// =====================================================================
// AT-192 — no response / TLS failure
// =====================================================================

#[test]
fn lrs_timeout_and_refused_queued() {
    // AT-192: an LRS fixture that accepts the TCP connection but never
    // returns a status, for longer than --lrs-timeout-secs, yields
    // lrs_status "queued" and a sendable {id}.json. A fixture that
    // refuses the connection (nothing listening) is also queued.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_hang(true);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let started = Instant::now();
    let resp = host.select("SRC-0001");
    let elapsed = started.elapsed();
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued"),
        "a hanging LRS is queued after the timeout"
    );
    assert!(elapsed >= Duration::from_secs(4), "waited for the LRS timeout: {elapsed:?}");
    assert!(elapsed < Duration::from_secs(10), "bounded by the timeout: {elapsed:?}");
    assert_eq!(retry_names(&host.data_dir).len(), 1);
    drop(host);
    // Connection refused: nothing listens on the configured LRS address.
    let dead_port = free_port();
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(format!("http://127.0.0.1:{dead_port}/xapi")),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued"),
        "a refused LRS connection is queued"
    );
    assert_eq!(retry_names(&host.data_dir).len(), 1);
}

#[test]
fn lrs_tls_cert_failure_is_permanent() {
    // AT-192: a fixture whose TLS certificate does not validate (name
    // or chain mismatch) is a permanent failure (lrs_status "failed",
    // a {id}.permanent file, no retry POST).
    let dir = TempDir::new("lrs-tls");
    let (cert, key) = make_test_cert(&dir);
    let port = free_port();
    let mut s_server = std::process::Command::new("openssl")
        .args([
            "s_server",
            "-accept",
            &format!("127.0.0.1:{port}"),
            "-cert",
            cert.to_str().unwrap(),
            "-key",
            key.to_str().unwrap(),
            "-quiet",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap_or_else(|e| panic!("openssl is required for the TLS LRS test: {e}"));
    wait_until(Duration::from_secs(5), "s_server listening", || {
        TcpStream::connect(("127.0.0.1", port)).is_ok()
    });
    let fixture = FixtureServer::new();
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(format!("https://127.0.0.1:{port}")),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("failed"),
        "a TLS certificate failure is permanent"
    );
    assert!(retry_names(&host.data_dir).is_empty(), "no sendable retry file");
    assert_eq!(permanent_names(&host.data_dir).len(), 1, "one .permanent file");
    let _ = s_server.kill();
    let _ = s_server.wait();
}

// =====================================================================
// AT-236 — background retries: 3 tries, 10 s apart (Q-09, DD-51/DD-156)
// =====================================================================

#[test]
fn lrs_background_retries_three_tries() {
    // AT-236: with consent on and an LRS fixture that fails transiently
    // on every POST, a POST /v1/download returns with lrs_status
    // "queued" without waiting the backoff interval (the handler is not
    // blocked for 10 seconds). The host then retries in the background:
    // three tries in total, 10 seconds apart, still re-checking consent
    // before each send. After the third failure, a local failure record
    // names the affected entry ids and the LRS error. A diagnosis export
    // contains only the error code and message — never the xAPI
    // statement body, --researcher-id, paper identity, or source URL.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"retry paper body\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(503);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let started = Instant::now();
    let resp = host.download("SRC-0001");
    let elapsed = started.elapsed();
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the handler did not wait the backoff: {elapsed:?}"
    );
    let statement_id = lrs.statements()[0]
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    // Three tries in total, 10 s apart: the third POST lands ~20 s
    // after the first.
    wait_until(Duration::from_secs(40), "three LRS tries", || {
        lrs.post_count() >= 3
    });
    assert_eq!(lrs.post_count(), 3, "exactly three tries");
    // The failure record names the entry id and the LRS error, and
    // contains no researcher data.
    wait_until(Duration::from_secs(5), "failure record", || {
        !failure_records(&host.data_dir).is_empty()
    });
    let records = failure_records(&host.data_dir);
    assert_eq!(records.len(), 1, "one failure record: {records:?}");
    let rec = &records[0];
    assert_eq!(
        rec.get("statement_id").and_then(|v| v.as_str()),
        Some(statement_id.as_str())
    );
    assert_eq!(rec.get("attempts").and_then(|v| v.as_u64()), Some(3));
    let rec_text = serde_json::to_string(rec).unwrap();
    assert!(
        rec_text.contains("503"),
        "the record names the LRS error: {rec_text}"
    );
    for forbidden in [
        "ashley",
        "SRC-0001",
        &url,
        "ashecribb.dev/xapi/verbs",
        "actor",
        "object",
    ] {
        assert!(
            !rec_text.contains(forbidden),
            "diagnosis record must not contain {forbidden:?}: {rec_text}"
        );
    }
    // The sendable queue file is kept (not dropped) after the third
    // failure, for a later replay.
    assert_eq!(retry_names(&host.data_dir).len(), 1, "queue file kept");
    // No fourth try in this episode.
    std::thread::sleep(Duration::from_secs(12));
    assert_eq!(lrs.post_count(), 3, "no fourth try");
}

// =====================================================================
// AT-156 — revoke during an in-flight LRS POST or a slow fetch
// =====================================================================

#[test]
fn lrs_revoke_during_slow_post_aborts() {
    // AT-156: the LRS fixture delays the response by 3 s. The client
    // sends /v1/select over a raw socket and then revokes consent while
    // the POST is in flight. The host aborts the in-flight POST, the
    // handler returns quickly with lrs_status "skipped" (consent now
    // off), and no {id}.json retry file is written. The fixture has
    // exactly one POST (the aborted one).
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_delay(Duration::from_millis(3000));
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let mut conn = send_post_raw(
        &host,
        "/v1/select",
        br#"{"paper_id":"SRC-0001"}"#,
    );
    // Wait until the host's LRS POST has arrived at the fixture, so the
    // revoke lands while it is still in flight.
    wait_until(Duration::from_secs(5), "LRS POST in flight", || {
        lrs.post_count() >= 1
    });
    let revoke = host.consent_off();
    assert_eq!(revoke.status, 200, "{}", revoke.text());
    let started = Instant::now();
    let resp = read_resp(&mut conn);
    let elapsed = started.elapsed();
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped"),
        "a revoke aborts the in-flight POST and skips the emit: {}",
        json
    );
    assert!(
        elapsed < Duration::from_secs(2),
        "the aborted POST returns promptly: {elapsed:?}"
    );
    assert!(retry_names(&host.data_dir).is_empty(), "no retry file");
    assert_eq!(lrs.post_count(), 1, "no second POST");
}

#[test]
fn lrs_revoke_during_slow_fetch_no_emit() {
    // AT-156: the fetch fixture delays the body by 2 s. The client
    // sends /v1/download over a raw socket and then revokes consent
    // while the fetch is still in flight. The host finishes the download
    // but skips the LRS emit: zero LRS requests and no {id}.json.
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/paper-1",
        FixtureRoute::Static {
            status: 200,
            headers: Vec::new(),
            body: b"slow body\n".to_vec(),
            delay_ms: 2000,
        },
    );
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let mut conn = send_post_raw(
        &host,
        "/v1/download",
        br#"{"paper_id":"SRC-0001"}"#,
    );
    std::thread::sleep(Duration::from_millis(500));
    let revoke = host.consent_off();
    assert_eq!(revoke.status, 200, "{}", revoke.text());
    let resp = read_resp(&mut conn);
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped"),
        "the download finishes but the emit is skipped"
    );
    assert_eq!(lrs.post_count(), 0, "zero LRS requests");
    assert!(retry_names(&host.data_dir).is_empty(), "no retry file");
}

#[test]
fn lrs_revoke_during_retry_write_is_skipped() {
    // AT-156: with the pause_before_lrs_retry_write hook parked inside
    // the write mutex, the client revokes consent. The host MUST see
    // consent off in the same mutex hold and skip the write. The
    // /v1/select handler returns lrs_status "skipped"; the queue dir
    // is deleted by the revoke, and no .json file appears.
    let dir = TempDir::new("lrs-revoke-write");
    let hooks = hook_env_vars(&dir, "pause_before_lrs_retry_write=pause_until_signal");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    lrs.set_status(503);
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let mut conn = send_post_raw(
        &host,
        "/v1/select",
        br#"{"paper_id":"SRC-0001"}"#,
    );
    // Wait for the hook to park (READY appears) — the handler is now
    // inside the write mutex, just before the retry-file write.
    let ready = dir.join("hook-ready");
    wait_until(Duration::from_secs(10), "hook parked", || ready.exists());
    // The revoke must go over a raw socket: the parked handler holds
    // the write mutex, so a synchronous consent_off() would block on
    // it (10 s read timeout) and the test could never release the
    // hook. Give the server a moment to receive the revoke (its
    // receipt-time latch actions run before the mutex) before the
    // release, so the handler's re-check sees the inhibit active.
    let mut revoke_conn = send_post_raw(&host, "/v1/consent", br#"{"state":"off"}"#);
    std::thread::sleep(Duration::from_millis(500));
    // Release the hook; the handler must now see consent off and skip.
    let continue_path = dir.join("hook-continue");
    std::fs::write(&continue_path, b"go\n").unwrap();
    let resp = read_resp(&mut conn);
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped"),
        "the retry-file write is skipped when consent is off in the same hold: {}",
        resp.json()
    );
    let revoke = read_resp(&mut revoke_conn);
    assert_eq!(revoke.status, 200, "{}", revoke.text());
    assert!(
        !host.data_dir.join("lrs-retry").exists(),
        "the revoke deleted the queue"
    );
    assert!(retry_names(&host.data_dir).is_empty(), "no retry file");
}

// =====================================================================
// AT-124 — revoke delete failure is fail-closed (DD-113/DD-138)
// =====================================================================

#[test]
fn lrs_revoke_delete_failure_is_fail_closed() {
    // AT-124: with the fail_lrs_retry_delete hook failing the queue
    // deletion, POST {"state":"off"} returns 500 revoke_delete_failed
    // (the in-flight consent state is already off), the process exits
    // non-zero, revoke-pending is left in place, and the queue is
    // still there for the startup retry.
    let dir = TempDir::new("lrs-revoke-delete");
    let hooks = hook_env_vars(&dir, "fail_lrs_retry_delete=fail_with_errno=5");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let mut host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    // Plant a retry file so the queue deletion is non-trivial.
    std::fs::create_dir_all(host.data_dir.join("lrs-retry")).unwrap();
    std::fs::write(
        host.data_dir.join("lrs-retry").join("deadbeef.json"),
        br#"{"id":"deadbeef"}"#,
    )
    .unwrap();
    let data_dir = host.data_dir.clone();
    let resp = host.consent_off();
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(
        resp.json().get("error").and_then(|v| v.as_str()),
        Some("revoke_delete_failed"),
        "500 revoke_delete_failed: {}",
        resp.json()
    );
    assert_eq!(
        host.wait_exit(Duration::from_secs(5)),
        Some(1),
        "fail-closed exit is non-zero"
    );
    assert!(data_dir.join("revoke-pending").exists(), "revoke-pending left");
    assert!(
        data_dir.join("lrs-retry").join("deadbeef.json").exists(),
        "the queue survives for the startup retry"
    );
}

#[test]
fn lrs_revoke_pending_write_failure_is_fail_closed() {
    // AT-189: with the fail_revoke_pending_write hook failing the
    // revoke-pending write, POST {"state":"off"} still best-effort
    // writes consent off and deletes the queue, then fails closed
    // (500 consent_write_failed, exit non-zero). No revoke-pending
    // file is left behind (it was never written).
    let dir = TempDir::new("lrs-revoke-pending-write");
    let hooks = hook_env_vars(&dir, "fail_revoke_pending_write=fail_with_errno=5");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let mut host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    // Plant a retry file so the queue deletion is observable.
    std::fs::create_dir_all(host.data_dir.join("lrs-retry")).unwrap();
    std::fs::write(
        host.data_dir.join("lrs-retry").join("deadbeef.json"),
        b"{}",
    )
    .unwrap();
    let data_dir = host.data_dir.clone();
    let resp = host.consent_off();
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(
        resp.json().get("error").and_then(|v| v.as_str()),
        Some("consent_write_failed"),
        "500 consent_write_failed: {}",
        resp.json()
    );
    assert_eq!(
        host.wait_exit(Duration::from_secs(5)),
        Some(1),
        "fail-closed exit is non-zero"
    );
    let consent_text = std::fs::read_to_string(host.dir.join("consent.txt")).unwrap();
    assert!(consent_text.starts_with("off"), "consent written off: {consent_text}");
    assert!(
        !data_dir.join("lrs-retry").exists(),
        "the queue is deleted by the best-effort path"
    );
    assert!(
        !data_dir.join("revoke-pending").exists(),
        "no revoke-pending was written"
    );
}

#[test]
fn lrs_revoke_delete_failure_startup_retry() {
    // AT-124, startup half: when a revoke-pending file is present at
    // startup, the host retries the consent-file write and queue
    // deletion. With the delete hook failing, the startup is an error
    // naming lrs-retry.
    let dir = TempDir::new("lrs-revoke-startup");
    let port = free_port();
    let (argv, consent, _creds) = lrs_argv(&dir, port, "http://127.0.0.1:1/xapi");
    std::fs::write(&consent, b"off\n2026-11-19T00:00:00Z\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&consent, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    // Plant a queue file and a revoke-pending file (flag was on).
    std::fs::create_dir_all(dir.join("data/lrs-retry")).unwrap();
    std::fs::write(dir.join("data/lrs-retry/deadbeef.json"), b"{}").unwrap();
    std::fs::write(dir.join("data/revoke-pending"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
    let hook_dir = TempDir::new("lrs-revoke-startup-hooks");
    let hooks = hook_env_vars(&hook_dir, "fail_lrs_retry_delete=fail_with_errno=5");
    let env: Vec<(&str, &str)> = hooks.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (code, stderr) = startup_exit_hooks(argv, &env);
    assert_eq!(code, 2, "startup error: {stderr}");
    assert!(stderr.contains("lrs-retry"), "names lrs-retry: {stderr}");
}

// =====================================================================
// AT-198 — consent races (DD-144)
// =====================================================================

#[test]
fn lrs_consent_on_before_revoke_hold_clears_inhibit() {
    // AT-198 (on-before-revoke-hold): with the pause_after_consent_
    // receipt hook parked after an off is received, an on that arrives
    // while the revoke is still in its mutex hold is NOT qualifying —
    // the revoke was received before it. After the off's hold ends,
    // consent is off (the on is not replayed). A later on enables
    // consent again.
    let dir = TempDir::new("lrs-on-before-hold");
    let hooks = hook_env_vars(&dir, "pause_after_consent_receipt=pause_until_signal");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        // Consent starts on from the FILE (not a POST): the pause hook
        // fires on every consent receipt, so a synchronous consent-on
        // POST after spawn would park and time out.
        consent_on: false,
        setup: Some(Box::new(|d| {
            std::fs::write(d.join("consent.txt"), b"on\n2026-11-19T00:00:00Z\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    d.join("consent.txt"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
        })),
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    // Send the off over a raw socket and wait for the hook to park
    // (the off is received and latched, but the hold has not started).
    let mut off_conn = send_post_raw(&host, "/v1/consent", br#"{"state":"off"}"#);
    let ready = dir.join("hook-ready");
    wait_until(Duration::from_secs(10), "off parked", || ready.exists());
    // The on arrives now — while the revoke is still in its mutex hold.
    // It is sent over a raw socket too: the hook fires on every consent
    // receipt, so the on also parks (its receipt observation was already
    // recorded, while the revoke was in flight).
    let mut on_conn = send_post_raw(&host, "/v1/consent", br#"{"state":"on"}"#);
    std::thread::sleep(Duration::from_millis(500));
    // Release both parked requests (the hook consumes CONTINUE once per
    // waiter; a second write releases the other).
    for _ in 0..2 {
        std::fs::write(dir.join("hook-continue"), b"go\n").unwrap();
        std::thread::sleep(Duration::from_millis(300));
    }
    let off_resp = read_resp(&mut off_conn);
    assert_eq!(off_resp.status, 200, "{}", off_resp.text());
    let on_resp = read_resp(&mut on_conn);
    assert_eq!(on_resp.status, 200, "{}", on_resp.text());
    // Consent is off: the on did not clear the inhibit.
    let consent_file = host.dir.join("consent.txt");
    let content = std::fs::read_to_string(&consent_file).unwrap();
    assert!(content.starts_with("off"), "consent is off after the race: {content}");
    // A later on enables consent again (sent raw: the pause hook is
    // still armed, so a synchronous consent_on() would park).
    let on2 = consent_raw_release(&host, &dir, "on");
    assert_eq!(on2.status, 200, "{}", on2.text());
    let content = std::fs::read_to_string(&consent_file).unwrap();
    assert!(content.starts_with("on"), "a later on enables consent: {content}");
}

#[test]
fn lrs_consent_on_after_receipt_with_revokes_in_flight() {
    // AT-198 (on-after-receipt with revokes in flight): with the
    // pause_after_consent_receipt hook parked after an off is received,
    // an on sent while the revoke is still in flight is not qualifying.
    // After the off completes, a select is skipped (consent off).
    let dir = TempDir::new("lrs-on-after-receipt");
    let hooks = hook_env_vars(&dir, "pause_after_consent_receipt=pause_until_signal");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        // Consent starts on from the FILE (not a POST): the pause hook
        // fires on every consent receipt, so a synchronous consent-on
        // POST after spawn would park and time out.
        consent_on: false,
        setup: Some(Box::new(|d| {
            std::fs::write(d.join("consent.txt"), b"on\n2026-11-19T00:00:00Z\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    d.join("consent.txt"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
        })),
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    let mut off_conn = send_post_raw(&host, "/v1/consent", br#"{"state":"off"}"#);
    let ready = dir.join("hook-ready");
    wait_until(Duration::from_secs(10), "off parked", || ready.exists());
    let mut on_conn = send_post_raw(&host, "/v1/consent", br#"{"state":"on"}"#);
    std::thread::sleep(Duration::from_millis(500));
    for _ in 0..2 {
        std::fs::write(dir.join("hook-continue"), b"go\n").unwrap();
        std::thread::sleep(Duration::from_millis(300));
    }
    let off_resp = read_resp(&mut off_conn);
    assert_eq!(off_resp.status, 200, "{}", off_resp.text());
    let on_resp = read_resp(&mut on_conn);
    assert_eq!(on_resp.status, 200, "{}", on_resp.text());
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped"),
        "the on did not clear the inhibit while a revoke was in flight"
    );
    assert_eq!(lrs.post_count(), 0);
}

#[test]
fn lrs_disconnect_mid_revoke_revoke_still_persists() {
    // AT-198 (disconnect mid-revoke): the client sends /v1/consent off
    // over a raw socket and disconnects before the response. The revoke
    // is still persisted (consent file off, revoke-pending removed
    // after the queue delete, inhibit active) — a later select is
    // skipped.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let conn = send_post_raw(&host, "/v1/consent", br#"{"state":"off"}"#);
    drop(conn);
    // The revoke is asynchronous after the disconnect; poll for the
    // consent file to flip off.
    let consent_file = host.dir.join("consent.txt");
    wait_until(Duration::from_secs(5), "consent file off", || {
        std::fs::read_to_string(&consent_file)
            .map(|c| c.starts_with("off"))
            .unwrap_or(false)
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped"),
        "the disconnect-mid-revoke revoke persisted"
    );
    assert_eq!(lrs.post_count(), 0);
}

// =====================================================================
// AT-229 — revoke-pending removal failure (DD-113)
// =====================================================================

#[test]
fn lrs_revoke_pending_remove_failure_is_tolerated() {
    // AT-229: when revoke-pending exists at the moment of a revoke,
    // its removal must not fail closed if the unlink fails for the
    // first time; the revoke returns 200. After the hook's one-shot
    // failure is consumed, the next consent on succeeds.
    let dir = TempDir::new("lrs-revoke-pending-once");
    let hooks = hook_env_vars(&dir, "fail_revoke_pending_remove=fail_once_with_errno=5");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    // Plant a revoke-pending file before the off, so step 4 (remove
    // revoke-pending) actually runs and the one-shot hook fires.
    std::fs::write(host.data_dir.join("revoke-pending"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
    let resp = host.consent_off();
    assert_eq!(resp.status, 200, "{}", resp.text());
    // The file is still there (the one-shot unlink failed), but the
    // revoke returned 200 and the host is up.
    assert!(host.data_dir.join("revoke-pending").exists(), "revoke-pending left");
    let on = host.consent_on();
    assert_eq!(on.status, 200, "{}", on.text());
    // The on unlinked the leftover revoke-pending.
    assert!(
        !host.data_dir.join("revoke-pending").exists(),
        "the later on removed the leftover revoke-pending"
    );
}

#[test]
fn lrs_revoke_pending_remove_lifetime_failure_blocks_on() {
    // AT-229, lifetime case: with revoke-pending's removal failing for
    // the lifetime of the process, an on after the off fails
    // fail-closed (the on cannot remove revoke-pending before writing
    // consent on) and the process exits non-zero.
    let dir = TempDir::new("lrs-revoke-pending-lifetime");
    let hooks = hook_env_vars(&dir, "fail_revoke_pending_remove=fail_with_errno=5");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let mut host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    std::fs::write(host.data_dir.join("revoke-pending"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
    let off = host.consent_off();
    assert_eq!(off.status, 200, "{}", off.text());
    let on = host.consent_on();
    assert_eq!(on.status, 500, "{}", on.text());
    assert_eq!(
        host.wait_exit(Duration::from_secs(5)),
        Some(1),
        "fail-closed exit is non-zero"
    );
}

// =====================================================================
// AT-191 (hook case) — retry-file write failure is not fail-closed
// =====================================================================

#[test]
fn lrs_retry_write_failure_is_not_fail_closed() {
    // AT-191: with the fail_lrs_retry_write hook failing the retry-file
    // write, /v1/select returns lrs_status "failed" and the process
    // stays up (not DD-138 fail-closed — the inclusion write itself
    // succeeded).
    let dir = TempDir::new("lrs-retry-write-fail");
    let hooks = hook_env_vars(&dir, "fail_lrs_retry_write=fail_with_errno=5");
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    lrs.set_status(503);
    let mut host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: hooks,
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("failed"),
        "a failed retry-file write reports failed: {}",
        resp.json()
    );
    assert_eq!(
        host.wait_exit(Duration::from_secs(2)),
        None,
        "the process stays up"
    );
    assert!(retry_names(&host.data_dir).is_empty(), "no retry file");
}

// =====================================================================
// AT-91 / AT-111 — queue survival and consent-off at restart
// =====================================================================

#[test]
fn lrs_queue_survives_restart_and_replays() {
    // AT-91: queued LRS statements survive an LRS restart; a leftover
    // .claimed file is renamed to .json at Scholar start with consent
    // on and replayed in the background.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(503);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    let queued = retry_names(&host.data_dir);
    assert_eq!(queued.len(), 1);
    let stmt_id = queued[0].trim_end_matches(".json").to_string();
    // Simulate an interrupted replay: rename the queued file to
    // .claimed, then restart with consent on.
    std::fs::rename(
        host.data_dir.join("lrs-retry").join(&queued[0]),
        host.data_dir.join("lrs-retry").join(format!("{stmt_id}.claimed")),
    )
    .unwrap();
    let data_dir = host.data_dir.clone();
    let opts = ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    };
    lrs.set_status(200);
    let host2 = restart(host, opts);
    // The leftover .claimed was renamed to .json and replayed.
    wait_until(Duration::from_secs(25), "replay POST after restart", || {
        lrs.post_count() >= 2
    });
    assert_eq!(
        lrs.statements()[1].get("id").and_then(|v| v.as_str()),
        Some(stmt_id.as_str()),
        "the replayed statement keeps its id"
    );
    wait_until(Duration::from_secs(5), "queue file gone", || {
        retry_names(&host2.data_dir).is_empty()
    });
    assert!(data_dir.join("lrs-retry").exists(), "the queue dir survives");
}

#[test]
fn lrs_consent_off_at_restart_deletes_queue() {
    // AT-111: a Scholar start with consent off deletes any
    // --data-dir/lrs-retry queue and makes zero LRS requests.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    lrs.set_status(503);
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    assert_eq!(retry_names(&host.data_dir).len(), 1);
    let posts = lrs.post_count();
    // Restart with consent off: write the consent file off first.
    // (`scholar_opts` with consent_on: false does NOT write the file;
    // it only skips the consent-on POST, so the file must say off.)
    // The file already exists with mode 0600 (written by the first
    // host's consent-on POST), so a plain overwrite keeps the mode.
    std::fs::write(host.dir.join("consent.txt"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
    let opts = ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: false,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    };
    let host2 = restart(host, opts);
    assert!(
        !host2.data_dir.join("lrs-retry").exists(),
        "the queue is deleted at a consent-off start"
    );
    assert_eq!(lrs.post_count(), posts, "zero LRS requests after restart");
    // A select with consent off is skipped, with no LRS traffic.
    dossier(&host2, "SRC-0002", "title: Second", "# Second\n");
    let resp = host2.select("SRC-0002");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped")
    );
    assert_eq!(lrs.post_count(), posts, "still zero LRS requests");
}

// =====================================================================
// AT-181 — startup with revoke-pending present (DD-113)
// =====================================================================

#[test]
fn lrs_startup_revoke_pending_retry() {
    // AT-181: when a revoke-pending file is present at startup, the
    // host treats consent as off, retries the consent-file write and
    // queue deletion, removes revoke-pending, and sends no LRS
    // requests.
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: false,
        setup: Some(Box::new(|dir| {
            // The consent file says on (the revoke never finished
            // writing it off), the queue has a file, and revoke-pending
            // is present.
            std::fs::write(dir.join("consent.txt"), b"on\n2026-11-19T00:00:00Z\n").unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    dir.join("consent.txt"),
                    std::fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
            std::fs::create_dir_all(dir.join("data/lrs-retry")).unwrap();
            std::fs::write(dir.join("data/lrs-retry/deadbeef.json"), b"{}").unwrap();
            std::fs::write(dir.join("data/revoke-pending"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
        })),
        ..ScholarOpts::default()
    });
    assert!(
        !host.data_dir.join("revoke-pending").exists(),
        "revoke-pending removed"
    );
    assert!(
        !host.data_dir.join("lrs-retry").exists(),
        "the queue is deleted"
    );
    let consent_text = std::fs::read_to_string(host.dir.join("consent.txt")).unwrap();
    assert!(consent_text.starts_with("off"), "consent is off: {consent_text}");
    assert_eq!(lrs.post_count(), 0, "no LRS requests");
}

#[test]
fn lrs_startup_revoke_pending_remove_failure() {
    // AT-181: if revoke-pending cannot be removed after the consent
    // write and queue deletion succeed, startup is an error naming
    // revoke-pending.
    let dir = TempDir::new("lrs-startup-revoke-rm");
    let port = free_port();
    let (argv, consent, _creds) = lrs_argv(&dir, port, "http://127.0.0.1:1/xapi");
    std::fs::write(&consent, b"on\n2026-11-19T00:00:00Z\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&consent, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    std::fs::create_dir_all(dir.join("data/lrs-retry")).unwrap();
    std::fs::write(dir.join("data/lrs-retry/deadbeef.json"), b"{}").unwrap();
    std::fs::write(dir.join("data/revoke-pending"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
    let hook_dir = TempDir::new("lrs-startup-revoke-rm-hooks");
    let hooks = hook_env_vars(&hook_dir, "fail_revoke_pending_remove=fail_with_errno=5");
    let env: Vec<(&str, &str)> = hooks.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (code, stderr) = startup_exit_hooks(argv, &env);
    assert_eq!(code, 2, "startup error: {stderr}");
    assert!(stderr.contains("revoke-pending"), "names revoke-pending: {stderr}");
}

// =====================================================================
// AT-199 — Scholar start without --lrs-url
// =====================================================================

#[test]
fn lrs_no_lrs_start_deletes_queue_and_revoke_pending() {
    // AT-199: a Scholar start WITHOUT --lrs-url takes sidecar locks
    // first, then deletes --data-dir/lrs-retry/ and
    // --data-dir/revoke-pending if present, then continues with
    // tracing unconfigured.
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: None,
        consent_on: false,
        setup: Some(Box::new(|dir| {
            std::fs::create_dir_all(dir.join("data/lrs-retry")).unwrap();
            std::fs::write(dir.join("data/lrs-retry/deadbeef.json"), b"{}").unwrap();
            std::fs::write(dir.join("data/revoke-pending"), b"off\n2026-11-19T00:00:00Z\n").unwrap();
        })),
        ..ScholarOpts::default()
    });
    assert!(
        !host.data_dir.join("lrs-retry").exists(),
        "the queue is deleted"
    );
    assert!(
        !host.data_dir.join("revoke-pending").exists(),
        "revoke-pending is deleted"
    );
}

#[test]
fn lrs_no_lrs_start_delete_failure_is_startup_error() {
    // AT-199: failure to delete either is a startup error naming the
    // path.
    let dir = TempDir::new("lrs-no-lrs-start-fail");
    let port = free_port();
    let (argv, _paths) = scholar_argv(&dir, port);
    std::fs::create_dir_all(dir.join("data/lrs-retry")).unwrap();
    std::fs::write(dir.join("data/lrs-retry/deadbeef.json"), b"{}").unwrap();
    // (scholar_argv already passes --data-dir; no second copy.)
    let hook_dir = TempDir::new("lrs-no-lrs-start-fail-hooks");
    let hooks = hook_env_vars(&hook_dir, "fail_lrs_retry_delete=fail_with_errno=5");
    let env: Vec<(&str, &str)> = hooks.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let (code, stderr) = startup_exit_hooks(argv, &env);
    assert_eq!(code, 2, "startup error: {stderr}");
    assert!(stderr.contains("lrs-retry"), "names lrs-retry: {stderr}");
}

// =====================================================================
// AT-237 — lab vs study identity (DD-155/DD-157, Q-08/Q-13)
// =====================================================================

#[test]
fn lrs_lab_homepage_and_researcher_id() {
    // AT-237 (lab): with --researcher-id ashley.cribb and no
    // --study-id, the actor homepage is the lab homepage and the actor
    // account name is the researcher id.
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        researcher_id: Some("ashley.cribb".to_string()),
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(lrs.post_count(), 1);
    let st = &lrs.statements()[0];
    assert_eq!(
        st.get("actor").and_then(|v| v.get("account")).and_then(|v| v.get("homePage")).and_then(|v| v.as_str()),
        Some(LAB_HOMEPAGE)
    );
    assert_eq!(
        st.get("actor").and_then(|v| v.get("account")).and_then(|v| v.get("name")).and_then(|v| v.as_str()),
        Some("ashley.cribb")
    );
}

#[test]
fn lrs_study_homepage_and_participant_code() {
    // AT-237 (study): with --study-id PROJ-42 and --researcher-id
    // P-017, the actor homepage is the study homepage and the account
    // name is the participant code.
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        researcher_id: Some("P-017".to_string()),
        study_id: Some("PROJ-42".to_string()),
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(lrs.post_count(), 1);
    let st = &lrs.statements()[0];
    assert_eq!(
        st.get("actor").and_then(|v| v.get("account")).and_then(|v| v.get("homePage")).and_then(|v| v.as_str()),
        Some("https://34.74.163.88.sslip.io/scholar/study/PROJ-42")
    );
    assert_eq!(
        st.get("actor").and_then(|v| v.get("account")).and_then(|v| v.get("name")).and_then(|v| v.as_str()),
        Some("P-017")
    );
}

#[test]
fn lrs_study_mode_rejects_name_or_email_researcher_id() {
    // AT-237 (study validation): with --study-id set, a
    // --researcher-id that looks like a name or an email is a startup
    // error naming --researcher-id.
    for bad in ["Ashley Cribb", "ashley@example.test", "Ashley"] {
        let dir = TempDir::new("lrs-study-bad-id");
        let port = free_port();
        let (mut argv, _consent, _creds) =
            lrs_argv(&dir, port, "http://127.0.0.1:1/xapi");
        set_argv_value(&mut argv, "--researcher-id", bad);
        argv.push("--study-id".into());
        argv.push("PROJ-42".to_string());
        let (code, stderr) = startup_exit(argv);
        assert_eq!(code, 2, "startup error for {bad:?}: {stderr}");
        assert!(
            stderr.contains("--researcher-id"),
            "names --researcher-id for {bad:?}: {stderr}"
        );
    }
}

#[test]
fn lrs_actor_homepage_flag_is_startup_error() {
    // AT-237 (removed flag): passing --lrs-actor-homepage is a startup
    // error naming the removed flag.
    let dir = TempDir::new("lrs-homepage-flag");
    let port = free_port();
    let (mut argv, _consent, _creds) = lrs_argv(&dir, port, "http://127.0.0.1:1/xapi");
    argv.push("--lrs-actor-homepage".into());
    argv.push("https://example.test/scholar".into());
    let (code, stderr) = startup_exit(argv);
    assert_eq!(code, 2, "startup error: {stderr}");
    assert!(
        stderr.contains("--lrs-actor-homepage"),
        "names the removed flag: {stderr}"
    );
}

#[test]
fn lrs_study_id_without_researcher_id_is_startup_error() {
    // AT-237: --study-id without --researcher-id is a startup error
    // naming --researcher-id (a study emit needs a participant code).
    let dir = TempDir::new("lrs-study-no-id");
    let port = free_port();
    let (mut argv, _consent, _creds) = lrs_argv(&dir, port, "http://127.0.0.1:1/xapi");
    argv.push("--study-id".into());
    argv.push("PROJ-42".to_string());
    let (code, stderr) = startup_exit(argv);
    assert_eq!(code, 2, "startup error: {stderr}");
    assert!(
        stderr.contains("--researcher-id"),
        "names --researcher-id: {stderr}"
    );
}

// =====================================================================
// AT-182 — credentials file trailing fields
// =====================================================================

#[test]
fn lrs_credentials_trailing_fields_accepted() {
    // AT-182: the trailing fields after the password are optional and
    // accepted, and the auth header still proves the password is the
    // second colon-separated field (not a later field). The LRS flags
    // are passed via `extra` (ScholarOpts with lrs_url set would pass
    // its own --lrs-credentials-file, and duplicate flags are a
    // startup error), and consent is turned on by hand after spawn.
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let dir = TempDir::new("lrs-creds-trailing");
    let creds = dir.write(
        "my-creds.txt",
        b"lrs-user:lrs-pass:read,write:archive-1",
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    let consent = dir.join("consent.txt");
    let host = RunningHost::scholar_opts_in(
        dir,
        ScholarOpts {
            bin: HostBin::Egress,
            lrs_url: None,
            consent_on: false,
            egress_test_loopback_port: Some(fixture.port),
            extra: vec![
                "--lrs-url".to_string(),
                lrs.url(),
                "--consent-state-file".to_string(),
                consent.to_string_lossy().into_owned(),
                "--lrs-credentials-file".to_string(),
                creds.to_string_lossy().into_owned(),
                "--lrs-timeout-secs".to_string(),
                "5".to_string(),
            ],
            ..ScholarOpts::default()
        },
    );
    let on = host.consent_on();
    assert_eq!(on.status, 200, "{}", on.text());
    dossier(&host, "SRC-0001", "title: First", "# First\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(lrs.post_count(), 1);
    assert_eq!(lrs.auth_headers(), vec![basic_auth("lrs-user", "lrs-pass")]);
}

// =====================================================================
// Consent copy is NOT served (Q-02, DD-158)
// =====================================================================

#[test]
fn lrs_consent_copy_not_served() {
    // The §5.2.3 consent copy is a draft pending Ashley's review and
    // IRB approval. No host route serves it, and the text is not
    // embedded in any response.
    let fixture = FixtureServer::new();
    let (host, _lrs) = scholar_with_lrs(&fixture, true);
    let _ = _lrs;
    let resp = host.get("/v1/consent-copy");
    assert_eq!(resp.status, 404, "no consent-copy route: {}", resp.text());
    let health = host.get("/health");
    assert_eq!(health.status, 200);
    assert!(
        !health.text().contains("consent"),
        "the health response carries no consent copy"
    );
}

// =====================================================================
// DD-65 — LRS URL path forms
// =====================================================================

#[test]
fn lrs_url_path_forms_accepted() {
    // DD-65: --lrs-url accepts the empty path, /, /xapi, and /xapi/,
    // and the statements target is /xapi/statements in every case
    // (no doubled /xapi). Other paths, userinfo, query, fragment, and
    // http with a hostname are startup errors.
    for path in ["", "/", "/xapi", "/xapi/"] {
        let fixture = FixtureServer::new();
        let lrs = LrsServer::new("::1");
        let url = format!("http://[::1]:{}{}", lrs.port, path);
        let host = RunningHost::scholar_opts(ScholarOpts {
            bin: HostBin::Egress,
            lrs_url: Some(url),
            consent_on: true,
            egress_test_loopback_port: Some(fixture.port),
            ..ScholarOpts::default()
        });
        dossier(&host, "SRC-0001", "title: First", "# First\n");
        let resp = host.select("SRC-0001");
        assert_eq!(resp.status, 200, "{} (path {path:?})", resp.text());
        assert_eq!(
            resp.json().get("lrs_status").and_then(|v| v.as_str()),
            Some("sent"),
            "path {path:?}"
        );
        assert_eq!(lrs.paths(), vec!["/xapi/statements"], "path {path:?}");
    }
}

#[test]
fn lrs_url_invalid_forms_are_startup_errors() {
    // DD-65: a path other than empty, /, /xapi, or /xapi/; userinfo;
    // query; fragment; or http with a hostname are startup errors
    // naming --lrs-url.
    for url in [
        "http://127.0.0.1:1/xapi/v2",
        "http://127.0.0.1:1/statements",
        "http://user:pass@127.0.0.1:1/xapi",
        "http://127.0.0.1:1/xapi?token=abc",
        "http://127.0.0.1:1/xapi#frag",
        "http://localhost:1/xapi",
    ] {
        let dir = TempDir::new("lrs-url-bad");
        let port = free_port();
        let (argv, _consent, _creds) = lrs_argv(&dir, port, url);
        let (code, stderr) = startup_exit(argv);
        assert_eq!(code, 2, "startup error for {url}: {stderr}");
        assert!(stderr.contains("--lrs-url"), "names --lrs-url for {url}: {stderr}");
    }
}
