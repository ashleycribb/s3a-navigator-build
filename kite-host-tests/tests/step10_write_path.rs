//! Step 10 — Scholar write path: select / download / deselect through
//! the host (§5.2.1, §5.5, §5.6). Fail-first acceptance tests for the
//! step-10 ATs; the step-9 host returned 500 `internal` for every
//! Scholar-mode write, so these fail until the write path lands.

mod fixture8b;
mod support;

use std::io::Write;
use std::time::Duration;

use fixture8b::*;
use s3a_core::ADDON_EDU;
use sha2::Digest;
use support::*;

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

fn scholar_egress(fixture: &FixtureServer) -> RunningHost {
    RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    })
}

/// Write a dossier under `host.wiki_root/papers/<stem>.md` with the given
/// front matter and body. Returns the dossier path.
fn dossier(host: &RunningHost, stem: &str, front_matter: &str, body: &str) -> std::path::PathBuf {
    write_dossier_at(&host.wiki_root, stem, front_matter, body)
}

/// Write a dossier under the corpus root (used for on-disk drive_path
/// cases, where the data file lives next to the dossier tree).
fn corpus_dossier(
    host: &RunningHost,
    stem: &str,
    front_matter: &str,
    body: &str,
) -> std::path::PathBuf {
    write_dossier_at(&host.corpus_root, stem, front_matter, body)
}

fn expect_inclusion(
    host: &RunningHost,
    identity: &str,
    verb_id: u16,
    snapshot_sha256: Option<&str>,
) {
    let lines = read_index_lines(&host.data_dir);
    // DD-104: the LAST matching line wins (step 1 appends a null-sha
    // line; a successful step 2 appends the sha line after it).
    let line = lines
        .iter()
        .rev()
        .find(|l| {
            l.get("paper_identity").and_then(|v| v.as_str()) == Some(identity)
                && l.get("verb_id").and_then(|v| v.as_u64()) == Some(verb_id as u64)
        })
        .unwrap_or_else(|| panic!("no index line for {identity} verb {verb_id}: {lines:?}"));
    assert_eq!(
        line.get("object_hash").and_then(|v| v.as_u64()),
        Some(crc32c::crc32c(identity.as_bytes()) as u64),
        "object_hash is the CRC32C of the UTF-8 paper identity"
    );
    assert!(
        line.get("timestamp_sec").and_then(|v| v.as_u64()).unwrap_or(0) > 0,
        "timestamp_sec is set"
    );
    assert!(
        line.get("working_archive")
            .and_then(|v| v.as_str())
            .map(|s| s.ends_with("working.s3a"))
            .unwrap_or(false),
        "working_archive names the working archive: {line:?}"
    );
    assert_eq!(
        line.get("file_uuid").and_then(|v| v.as_str()).map(|s| s.len()),
        Some(32),
        "file_uuid is 32 lowercase hex chars: {line:?}"
    );
    match snapshot_sha256 {
        Some(sha) => assert_eq!(
            line.get("snapshot_sha256").and_then(|v| v.as_str()),
            Some(sha),
            "snapshot_sha256 recorded"
        ),
        None => assert_eq!(
            line.get("snapshot_sha256"),
            Some(&serde_json::Value::Null),
            "snapshot_sha256 is null when no snapshot: {line:?}"
        ),
    }
}

// =====================================================================
// Select (AT-26, AT-27, AT-31, AT-54, AT-82, AT-89, AT-102, AT-234)
// =====================================================================

#[test]
fn select_writes_nothing_durable() {
    // AT-26 + §5.2.1 select row + DD-154: a select is a closer-read event
    // only. It writes NO tile-10 inclusion record, NO snapshot, NO
    // fingerprint, NO inclusion-index line, and no download file. The
    // working archive still holds only its startup create-time ConsentRecord
    // (tile 71).
    let host = RunningHost::scholar_opts(ScholarOpts::default());
    assert!(host.working_archive_path().exists(), "startup creates it");
    assert_eq!(archive_tile_types(&host.working_archive_path()), vec![71]);
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let body = resp.json();
    assert_eq!(body.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        body.get("lrs_status").and_then(|v| v.as_str()),
        Some("not_configured")
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(archive_tile_types(&host.working_archive_path()), vec![71]);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    assert!(dir_files_no_lock(&host.download_dir).is_empty());
}

#[test]
fn select_sends_lrs_opened_statement() {
    // AT-54 (first case): LRS on + consent on → the select emits one
    // `opened` statement and reports `sent`.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    // No durable write: no tile-10, no index line.
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
    assert_eq!(lrs.statement_count(), 1);
    let stmt = &lrs.statements()[0];
    assert_eq!(
        stmt.get("verb").and_then(|v| v.get("id")).and_then(|v| v.as_str()),
        Some("https://ashecribb.dev/xapi/verbs/opened")
    );
    // No source_url on this dossier: object.id is the fallback paper IRI,
    // and there is no EXT_SOURCE_URL or EXT_CONTENT_SHA256 extension.
    assert_eq!(
        stmt.get("object").and_then(|v| v.get("id")).and_then(|v| v.as_str()),
        Some("https://ashecribb.dev/xapi/activities/paper/SRC-0001")
    );
    assert_eq!(
        stmt.get("actor")
            .and_then(|v| v.get("account"))
            .and_then(|v| v.get("homePage"))
            .and_then(|v| v.as_str()),
        Some("https://34.74.163.88.sslip.io/scholar/lab")
    );
    assert_eq!(
        stmt.get("actor")
            .and_then(|v| v.get("account"))
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str()),
        Some("https://example.test/researcher/ashley")
    );
    let ext = stmt
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .expect("object.definition.extensions");
    assert_eq!(
        ext.get("https://ashecribb.dev/xapi/ext/paper-id")
            .and_then(|v| v.as_str()),
        Some("SRC-0001")
    );
    // Basic auth from the credentials file, xAPI 1.0.3 version header.
    let expected_auth = format!(
        "Basic {}",
        base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            b"lrs-user:lrs-pass"
        )
    );
    assert_eq!(lrs.auth_headers(), vec![expected_auth]);
    assert_eq!(lrs.api_versions(), vec!["1.0.3"]);
    assert_eq!(lrs.paths(), vec!["/xapi/statements"]);
}

#[test]
fn select_repeat_is_idempotent() {
    // AT-234 + §5.2.1: a repeat select writes no second record and emits
    // no second LRS statement; it reports `skipped`.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let first = host.select("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    let second = host.select("SRC-0001");
    assert_eq!(second.status, 200, "{}", second.text());
    assert_eq!(
        second.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped")
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
    assert_eq!(lrs.statement_count(), 1);
}

#[test]
fn select_consent_off_skips_lrs() {
    // Consent off: the select still succeeds, but the LRS step is
    // `skipped` and no statement is sent; nothing durable is written.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, false);
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped")
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
    assert_eq!(lrs.statement_count(), 0);
}

#[test]
fn select_unknown_paper_writes_nothing() {
    // AT-27: an unknown paper_id is 404 `unknown_paper` with no inclusion
    // record and no index line (the working archive was created at startup
    // and still holds only its create-time ConsentRecord).
    let host = RunningHost::scholar_opts(ScholarOpts::default());
    let resp = host.select("SRC-9999");
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "unknown_paper");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
}

// =====================================================================
// AT-46 addon_missing hold
// =====================================================================

#[test]
fn select_addon_missing_is_409() {
    // AT-46 host A/B: after a successful control select on a valid
    // ADDON_EDU archive in host A, a second host process B (distinct
    // --data-dir, --working-archive, --snapshot-dir, --download-dir, own
    // port) pointed at a file whose addon_mask lacks ADDON_EDU starts
    // fine, then its select fails with 409 `addon_missing` in the
    // step-0/1 hold, writes nothing, has no `lrs_status`, and leaves that
    // second file byte-identical.
    let host_a = RunningHost::scholar_opts(ScholarOpts::default());
    dossier(&host_a, "SRC-0001", "title: First Paper", "# First Paper\n");
    let control = host_a.select("SRC-0001");
    assert_eq!(control.status, 200, "{}", control.text());

    let fx = build_hyper(vec![], false);
    let host_b = RunningHost::scholar_opts(ScholarOpts {
        setup: Some(Box::new(move |dir| {
            let path = dir.join("working").join("working.s3a");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &fx.bytes).unwrap();
        })),
        ..ScholarOpts::default()
    });
    dossier(&host_b, "SRC-0001", "title: First Paper", "# First Paper\n");
    let before = std::fs::read(host_b.working_archive_path()).unwrap();
    let resp = host_b.select("SRC-0001");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "addon_missing");
    assert!(
        resp.json().get("lrs_status").is_none(),
        "no lrs_status on addon_missing: {}",
        resp.text()
    );
    assert_eq!(count_tile10_records(&host_b.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host_b.data_dir).len(), 0);
    assert_eq!(
        std::fs::read(host_b.working_archive_path()).unwrap(),
        before,
        "the second file is byte-identical"
    );
}

#[test]
fn download_addon_bit_cleared_after_inclusion_is_409() {
    // AT-46 second case: a host whose working archive has ADDON_EDU and
    // whose step 1 is skipped because a last matching inclusion-index
    // line is already present, whose bit is then cleared mid-process in
    // place on the same (dev, ino), returns the same 409 `addon_missing`
    // on the next download — the check runs even when step 1 is skipped.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"addon body\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    clear_addon_bit_in_place(&host.working_archive_path());
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "addon_missing");
    assert!(resp.json().get("lrs_status").is_none(), "no lrs_status");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // §5.5: step 1 appends a null-sha line and step 2 appends the sha
    // line, so one successful download leaves two index lines.
    assert_eq!(read_index_lines(&host.data_dir).len(), 2);
    assert_eq!(lrs.statement_count(), 1, "no new statement");
    assert_eq!(fixture.get_count("/paper-1"), 1, "no second fetch");
}

#[test]
fn select_addon_bit_cleared_is_409() {
    // AT-46 second case, select variant: a successful select, then the
    // bit cleared in place on the same (dev, ino) → the next select is
    // 409 `addon_missing`.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let first = host.select("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    clear_addon_bit_in_place(&host.working_archive_path());
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "addon_missing");
    assert!(resp.json().get("lrs_status").is_none(), "no lrs_status");
    assert_eq!(lrs.statement_count(), 1, "no new statement");
}

#[test]
fn select_deleted_working_archive_is_500() {
    // AT-74 / §5.2.2: the archive is created at startup; deleted
    // mid-process, a select is 500 `write_failed` with no `lrs_status`,
    // and the host does not create a replacement until the next start.
    let host = RunningHost::scholar_opts(ScholarOpts::default());
    assert!(host.working_archive_path().exists());
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    std::fs::remove_file(host.working_archive_path()).unwrap();
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    assert!(resp.json().get("lrs_status").is_none(), "no lrs_status");
    assert!(!host.working_archive_path().exists(), "no mid-process create");
}

#[test]
fn select_replaced_working_archive_is_500() {
    // AT-46 first case (replaced path): the working archive is replaced
    // between startup and the select (a different file_uuid) → 500
    // `write_failed`, no inclusion record.
    let fx = build_hyper(vec![HyperTile::Addon(ADDON_EDU)], false);
    let host = RunningHost::scholar_opts(ScholarOpts {
        setup: Some(Box::new(move |dir| {
            let path = dir.join("working").join("working.s3a");
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &fx.bytes).unwrap();
        })),
        ..ScholarOpts::default()
    });
    // Replace the archive with a different one (different file_uuid)
    // via rewrite-and-rename: write the new bytes to a temp file in
    // the same directory, then rename over the archive path, so the
    // path now names a NEW inode. (A truncate-in-place `fs::write`
    // keeps the same (dev, ino) and is not caught by the name-binding
    // check — see the test log; SPEC's "rewrite-and-rename" is the
    // 500 `write_failed` case, AT-46 line 1006.)
    let fx2 = build_hyper(vec![HyperTile::Addon(ADDON_EDU)], false);
    let tmp_path = host
        .working_archive_path()
        .with_extension("replace-tmp");
    std::fs::write(&tmp_path, &fx2.bytes).unwrap();
    std::fs::rename(&tmp_path, host.working_archive_path()).unwrap();
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    assert!(resp.json().get("lrs_status").is_none(), "no lrs_status");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
}

// =====================================================================
// Download (AT-31, AT-54, AT-82, AT-88, AT-102, AT-103, AT-106, AT-126,
// AT-157, AT-180, AT-186, AT-202, AT-203, AT-209, AT-212, AT-213, AT-214,
// AT-215, AT-216, AT-219, AT-221, AT-223, AT-227)
// =====================================================================

#[test]
fn download_fetch_snapshot_and_delivery() {
    // AT-31/AT-54/AT-82/AT-88/AT-102/AT-103: source_url only → fetch,
    // snapshot, inclusion record, delivery file named by the SHA-256 of
    // the delivered bytes, LRS `saved` statement.
    let fixture = FixtureServer::new();
    let body = b"hello paper body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(json.get("ok").and_then(|v| v.as_bool()), Some(true));
    let sha = sha256_hex(body);
    assert_eq!(
        json.get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(json.get("snapshot_error"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("fetch_error"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("download_error"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    // Delivery file: --download-dir/{sha}, no extension, exact bytes.
    let delivered = host.download_dir.join(&sha);
    assert_eq!(std::fs::read(&delivered).unwrap(), body);
    assert_eq!(
        json.get("path").and_then(|v| v.as_str()),
        Some(delivered.to_string_lossy().as_ref())
    );
    // Snapshot: one {sha}.s3a; its BLOB reassembles to the body and its
    // PROVENANCE is the captured document (sha256/url/captured).
    let snaps = snapshot_files(&host.snapshot_dir);
    assert_eq!(snaps, vec![format!("{sha}.s3a")]);
    let snap_path = host.snapshot_dir.join(&snaps[0]);
    let mut key = [0u8; 32];
    key.copy_from_slice(&sha2::Sha256::digest(body));
    assert_eq!(reassemble_snapshot_blob(&snap_path, &key).as_deref(), Some(body as &[u8]));
    let prov = provenance_document(&snap_path).expect("provenance document");
    assert!(prov.starts_with(&format!("sha256={sha}\nurl={url}\ncaptured=")), "{prov}");
    assert!(prov.ends_with('\n'));
    // Inclusion record + index line (verb saved = 13) with the snapshot sha.
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    expect_inclusion(&host, "SRC-0001", 13, Some(&sha));
    // Exactly one GET, one LRS statement, verb saved.
    assert_eq!(fixture.get_count("/paper-1"), 1);
    assert_eq!(lrs.statement_count(), 1);
    assert_eq!(lrs.verbs(), vec!["https://ashecribb.dev/xapi/verbs/saved"]);
    // The statement carries the content SHA-256 extension (DD-137).
    let stmt = &lrs.statements()[0];
    let ext = stmt
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap();
    assert_eq!(
        ext.get("https://ashecribb.dev/xapi/ext/content-sha256")
            .and_then(|v| v.as_str()),
        Some(sha.as_str())
    );
    assert_eq!(
        ext.get("https://ashecribb.dev/xapi/ext/source-url")
            .and_then(|v| v.as_str()),
        Some(url.as_str())
    );
}

#[test]
fn download_on_disk_copy_no_snapshot() {
    // AT-31/AT-54/AT-82 (on-disk row): drive_path only → copy, no fetch,
    // no snapshot; the delivery is named by the hash of the file bytes.
    let fixture = FixtureServer::new();
    let host = scholar_egress(&fixture);
    let data = b"on-disk paper bytes\n";
    let data_dir = host.corpus_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("SRC-0001.txt"), data).unwrap();
    corpus_dossier(
        &host,
        "SRC-0001",
        "title: First Paper\ndrive_path: ../data/SRC-0001.txt",
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    let sha = sha256_hex(data);
    assert_eq!(json.get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("fetch_error"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("download_error"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("path").and_then(|v| v.as_str()),
        Some(host.download_dir.join(&sha).to_string_lossy().as_ref())
    );
    assert_eq!(std::fs::read(host.download_dir.join(&sha)).unwrap(), data);
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    assert_eq!(fixture.total_connections(), 0, "no fetch happened");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    expect_inclusion(&host, "SRC-0001", 13, None);
}

#[test]
fn download_on_disk_preferred_over_fetch() {
    // AT-31/AT-54/AT-82 (both row): source_url AND a usable drive_path →
    // the on-disk copy wins; no fetch, no snapshot.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"fetched body\n");
    let host = scholar_egress(&fixture);
    let data = b"on-disk wins\n";
    let data_dir = host.wiki_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("SRC-0001.txt"), data).unwrap();
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}\ndrive_path: ../data/SRC-0001.txt"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(fixture.get_count("/paper-1"), 0, "no fetch happened");
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    let sha = sha256_hex(data);
    assert_eq!(std::fs::read(host.download_dir.join(&sha)).unwrap(), data);
    expect_inclusion(&host, "SRC-0001", 13, None);
}

#[test]
fn download_no_source_writes_inclusion_only() {
    // AT-128 / §5.5 (neither row): select then download of a candidate
    // with neither a usable drive_path nor a source_url → the download
    // writes the inclusion record (snapshot_sha256 null) and returns 200
    // with download_error `no_source`, path null, no snapshot, no
    // delivery; --download-dir and --snapshot-dir are unchanged.
    let host = RunningHost::scholar_opts(ScholarOpts::default());
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(json.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("no_source")
    );
    assert_eq!(json.get("path"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    expect_inclusion(&host, "SRC-0001", 13, None);
}

#[test]
fn download_repeat_is_success_noop() {
    // AT-106 / AT-83: a second download of the same fixture, after the
    // `{sha256_hex}` file already exists with identical bytes, returns
    // 200, returns that same path, and leaves the file byte-identical
    // (success no-op) — no second fetch, no second record, no second
    // LRS statement.
    let fixture = FixtureServer::new();
    let body = b"repeat body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    let first_path = first
        .json()
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap()
        .to_string();
    let second = host.download("SRC-0001");
    assert_eq!(second.status, 200, "{}", second.text());
    let json = second.json();
    assert_eq!(
        json.get("path").and_then(|v| v.as_str()),
        Some(first_path.as_str()),
        "same path"
    );
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped")
    );
    assert_eq!(std::fs::read(&first_path).unwrap(), body);
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // §5.5: the first download leaves a null-sha step-1 line and a sha
    // step-2 line; the repeat adds neither.
    assert_eq!(read_index_lines(&host.data_dir).len(), 2);
    assert_eq!(fixture.get_count("/paper-1"), 1);
    assert_eq!(lrs.statement_count(), 1);
}

#[test]
fn select_then_download_is_two_events_one_inclusion() {
    // AT-83: select then download of the same identity uses two verb IRIs
    // (`opened` then `saved`); the select writes nothing, and the
    // download leaves a single inclusion record and a single snapshot.
    let fixture = FixtureServer::new();
    let body = b"at83 body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
    let dl = host.download("SRC-0001");
    assert_eq!(dl.status, 200, "{}", dl.text());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // §5.5: the download leaves a null-sha step-1 line and a sha
    // step-2 line.
    assert_eq!(read_index_lines(&host.data_dir).len(), 2);
    assert_eq!(snapshot_files(&host.snapshot_dir).len(), 1);
    assert_eq!(
        lrs.verbs(),
        vec![
            "https://ashecribb.dev/xapi/verbs/opened",
            "https://ashecribb.dev/xapi/verbs/saved"
        ]
    );
}

#[test]
fn select_consent_off_after_download_keeps_inclusion() {
    // AT-27: the select of the AT-31 remote-fetched fixture, with consent
    // off: zero HTTP requests to the LRS for the select, and the working
    // archive still contains the inclusion record the download wrote.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"at27 body\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let dl = host.download("SRC-0001");
    assert_eq!(dl.status, 200, "{}", dl.text());
    assert_eq!(lrs.statement_count(), 1);
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    let off = host.consent_off();
    assert_eq!(off.status, 200, "{}", off.text());
    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    assert_eq!(lrs.statement_count(), 1, "zero new LRS requests");
    assert_eq!(
        count_tile10_records(&host.working_archive_path()),
        1,
        "inclusion record still there"
    );
}

#[test]
fn download_without_prior_select() {
    // A download without a prior select writes its own inclusion record
    // (the download is the inclusion write); the select set is not a
    // prerequisite for download.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"direct download\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(lrs.statement_count(), 1);
}

// =====================================================================
// Deselect (AT-88, AT-89, AT-102, AT-127, AT-128, AT-129, AT-228, AT-235)
// =====================================================================

#[test]
fn deselect_after_select_logs_removed_only() {
    // AT-88/AT-89/AT-102/AT-127/AT-128/AT-129: a deselect after a select
    // writes NO tile-10 record, NO snapshot, NO inclusion record and NO
    // index line — it removes the in-memory selection and emits the LRS
    // `removed` statement.
    let fixture = FixtureServer::new();
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    let resp = host.deselect("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(json.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 0);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0);
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    assert_eq!(lrs.statement_count(), 2);
    assert_eq!(
        lrs.verbs(),
        vec![
            "https://ashecribb.dev/xapi/verbs/opened",
            "https://ashecribb.dev/xapi/verbs/removed"
        ]
    );
    // A second deselect is 404 `not_selected`.
    let again = host.deselect("SRC-0001");
    assert_eq!(again.status, 404);
    assert_eq!(again.error_code(), "not_selected");
}

#[test]
fn deselect_after_download_is_already_included() {
    // AT-128: a deselect after a download (included) is 409
    // `already_included` — the inclusion cannot be undone by deselect.
    // The download happened WITHOUT a prior select, so the
    // `already_included` (index) check runs before the `not_selected`
    // (in-memory set) check.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"included body\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let dl = host.download("SRC-0001");
    assert_eq!(dl.status, 200, "{}", dl.text());
    let resp = host.deselect("SRC-0001");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "already_included");
    assert_eq!(lrs.statement_count(), 1, "no removed statement");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

#[test]
fn deselect_without_select_is_not_selected() {
    // A deselect of a paper that was never selected and never downloaded
    // is 404 `not_selected`.
    let host = RunningHost::scholar_opts(ScholarOpts::default());
    dossier(&host, "SRC-0002", "title: Second Paper", "# Second\n");
    let resp = host.deselect("SRC-0002");
    assert_eq!(resp.status, 404);
    assert_eq!(resp.error_code(), "not_selected");
}

// =====================================================================
// Fetch errors (AT-54, AT-103, AT-122, AT-139, AT-140, AT-186, AT-202,
// AT-203, AT-227)
// =====================================================================

#[test]
fn fetch_404_is_http_status() {
    // AT-139/AT-140: a source_url whose final HTTP status is 404 yields
    // fetch_error `http_status`, writes no snapshot, does not store the
    // error-page body. The inclusion record from step 1 is kept, and step
    // 3 still runs (the statement carries no content-sha256 extension).
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/paper-1",
        FixtureRoute::Static {
            status: 404,
            headers: Vec::new(),
            body: b"<html>not found, a long error page</html>\n".to_vec(),
            delay_ms: 0,
        },
    );
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(
        json.get("fetch_error").and_then(|v| v.as_str()),
        Some("http_status")
    );
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("fetch_failed")
    );
    assert_eq!(json.get("path"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    assert!(dir_files_no_lock(&host.download_dir).is_empty());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    expect_inclusion(&host, "SRC-0001", 13, None);
    assert_eq!(lrs.statement_count(), 1);
    let stmts = lrs.statements();
    let ext = stmts[0]
        .get("object")
        .and_then(|v| v.get("definition"))
        .and_then(|v| v.get("extensions"))
        .unwrap();
    assert!(
        ext.get("https://ashecribb.dev/xapi/ext/content-sha256").is_none(),
        "no content sha without a snapshot"
    );
}

#[test]
fn fetch_scheme_not_allowed() {
    // AT-202/AT-203: a redirect to a non-http(s) scheme is fetch_error
    // `scheme_not_allowed`; the inclusion is kept, step 3 runs. (A
    // dossier `source_url` with a non-http(s) scheme is nulled at listing
    // time per §5.5, so it can only reach the fetch client via a
    // redirect.)
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/paper-1",
        FixtureRoute::Redirect {
            location: "ftp://example.test/paper-1".to_string(),
        },
    );
    let host = scholar_egress(&fixture);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(
        json.get("fetch_error").and_then(|v| v.as_str()),
        Some("scheme_not_allowed")
    );
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("fetch_failed")
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

#[test]
fn fetch_loopback_denied_without_egress_test() {
    // AT-122/AT-54: a default (non-egress_test) build denies loopback
    // fetch origins — `egress_denied`, no connection to the fixture.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"never fetched\n");
    let host = RunningHost::scholar_opts(ScholarOpts::default());
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("egress_denied")
    );
    assert_eq!(fixture.get_count("/paper-1"), 0, "no connection made");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

#[test]
fn fetch_lrs_host_is_denied() {
    // AT-54 second case: LRS on 127.0.0.1 + --egress-test-loopback-port
    // set, a source_url of http://127.0.0.1:<egress-port>/ is denied
    // (`egress_denied`) — the LRS-host deny beats the egress_test
    // exception.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"never fetched\n");
    let lrs = LrsServer::new("127.0.0.1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    });
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("egress_denied")
    );
    assert_eq!(fixture.get_count("/paper-1"), 0);
}

#[test]
fn fetch_too_many_redirects() {
    // AT-103: more redirects than --max-redirects → fetch_error
    // `too_many_redirects`; the final hop is never fetched.
    let fixture = FixtureServer::new();
    fixture.set_redirect_chain(5, "/ok");
    fixture.set_ok("/ok", b"final body\n");
    let host = scholar_egress(&fixture);
    let url = format!("http://127.0.0.1:{}/c0", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("too_many_redirects")
    );
    assert_eq!(fixture.get_count("/ok"), 0, "the cap was respected");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

#[test]
fn fetch_timeout() {
    // AT-103: a fixture slower than --fetch-timeout-secs → fetch_error
    // `timeout`; the inclusion is kept.
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/slow",
        FixtureRoute::Static {
            status: 200,
            headers: Vec::new(),
            body: b"slow body\n".to_vec(),
            delay_ms: 3000,
        },
    );
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        egress_test_loopback_port: Some(fixture.port),
        fetch_timeout_secs: 2,
        ..ScholarOpts::default()
    });
    let url = format!("http://127.0.0.1:{}/slow", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let start = std::time::Instant::now();
    let resp = host.download("SRC-0001");
    let elapsed = start.elapsed();
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("timeout")
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "the timeout fired promptly ({elapsed:?})"
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

#[test]
fn fetch_gzip_and_deflate_snapshot_decoded_body() {
    // AT-140: gzip/deflate bodies are snapshotted as the DECODED body.
    let fixture = FixtureServer::new();
    let body = b"decoded paper body, repeated many times to compress well. ".repeat(20);
    fixture.set_route(
        "/gz",
        FixtureRoute::Static {
            status: 200,
            headers: vec![(
                "Content-Encoding".to_string(),
                "gzip".to_string(),
            )],
            body: gzip_bytes(&body),
            delay_ms: 0,
        },
    );
    fixture.set_route(
        "/df",
        FixtureRoute::Static {
            status: 200,
            headers: vec![(
                "Content-Encoding".to_string(),
                "deflate".to_string(),
            )],
            body: deflate_bytes(&body),
            delay_ms: 0,
        },
    );
    let host = scholar_egress(&fixture);
    for (stem, path) in [("SRC-0001", "/gz"), ("SRC-0002", "/df")] {
        let url = format!("http://127.0.0.1:{}{path}", fixture.port);
        dossier(
            &host,
            stem,
            &format!("title: {stem}\nsource_url: {url}"),
            "# Paper\n",
        );
        let resp = host.download(stem);
        assert_eq!(resp.status, 200, "{}: {}", stem, resp.text());
        let sha = sha256_hex(&body);
        assert_eq!(
            resp.json().get("snapshot").and_then(|v| v.as_str()),
            Some(
                host.snapshot_dir
                    .join(format!("{sha}.s3a"))
                    .to_string_lossy()
                    .as_ref()
            ),
            "{stem}: the snapshot names the decoded body's hash"
        );
        assert_eq!(std::fs::read(host.download_dir.join(&sha)).unwrap(), body);
    }
}

#[test]
fn fetch_oversize_decoded_body() {
    // AT-140: a decoded body over --max-fetch-bytes → fetch_error
    // `body_too_large` (the raw gzip is small; the decoded body is what
    // is capped).
    let fixture = FixtureServer::new();
    let body = vec![b'x'; 2048];
    fixture.set_route(
        "/big",
        FixtureRoute::Static {
            status: 200,
            headers: vec![(
                "Content-Encoding".to_string(),
                "gzip".to_string(),
            )],
            body: gzip_bytes(&body),
            delay_ms: 0,
        },
    );
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        egress_test_loopback_port: Some(fixture.port),
        max_fetch_bytes: 1024,
        ..ScholarOpts::default()
    });
    let url = format!("http://127.0.0.1:{}/big", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("body_too_large")
    );
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

#[test]
fn fetch_unknown_encoding() {
    // AT-140: an unlisted Content-Encoding → fetch_error
    // `unknown_encoding`.
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/br",
        FixtureRoute::Static {
            status: 200,
            headers: vec![("Content-Encoding".to_string(), "br".to_string())],
            body: b"brotli-ish\n".to_vec(),
            delay_ms: 0,
        },
    );
    let host = scholar_egress(&fixture);
    let url = format!("http://127.0.0.1:{}/br", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("unknown_encoding")
    );
}

#[test]
fn fetch_headers_too_large() {
    // AT-140: a header block over --max-fetch-bytes → fetch_error
    // `headers_too_large`.
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/hdr",
        FixtureRoute::Static {
            status: 200,
            headers: vec![("X-Pad".to_string(), "x".repeat(2048))],
            body: b"body\n".to_vec(),
            delay_ms: 0,
        },
    );
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        egress_test_loopback_port: Some(fixture.port),
        max_fetch_bytes: 1024,
        ..ScholarOpts::default()
    });
    let url = format!("http://127.0.0.1:{}/hdr", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("headers_too_large")
    );
}

#[test]
fn fetch_retry_after_failure() {
    // AT-186 (read under the v24.1 rule as downloads): a first download
    // fails the GET (fixture 500); the fixture is then made to succeed; a
    // repeat download issues exactly one GET, writes no second inclusion
    // record and no second LRS statement, and returns a non-null snapshot;
    // a third download issues zero GETs.
    let fixture = FixtureServer::new();
    let body = b"at186 body\n";
    fixture.set_route("/paper-1", FixtureRoute::Flaky {
        fail_status: 500,
        body: body.to_vec(),
    });
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First Paper\nsource_url: {url}"),
        "# First Paper\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    assert_eq!(
        first.json().get("fetch_error").and_then(|v| v.as_str()),
        Some("http_status")
    );
    assert_eq!(first.json().get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(fixture.get_count("/paper-1"), 1);

    fixture.set_flaky_succeed(true);
    let second = host.download("SRC-0001");
    assert_eq!(second.status, 200, "{}", second.text());
    let sha = sha256_hex(body);
    assert_eq!(
        second.json().get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(fixture.get_count("/paper-1"), 2, "exactly one new GET");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(lrs.statement_count(), 1, "no second LRS statement");
    // The second download appends a new index line recording the sha
    // (the first line had a null sha); no second tile-10.
    let lines = read_index_lines(&host.data_dir);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[1].get("snapshot_sha256").and_then(|v| v.as_str()),
        Some(sha.as_str())
    );

    let third = host.download("SRC-0001");
    assert_eq!(third.status, 200, "{}", third.text());
    assert_eq!(fixture.get_count("/paper-1"), 2, "zero new GETs");
    assert_eq!(read_index_lines(&host.data_dir).len(), 2);
}

// =====================================================================
// Snapshot reuse / conflict / invalid (AT-83, AT-157, AT-180, AT-209,
// AT-215, AT-227, AT-228)
// =====================================================================

fn corrupt_snapshot_at(path: &std::path::Path, offset: usize) {
    let mut bytes = std::fs::read(path).unwrap();
    bytes[offset] ^= 0xff;
    std::fs::write(path, &bytes).unwrap();
}

#[test]
fn snapshot_reuse_same_bytes_one_file() {
    // AT-157: two candidates, different identities + source_urls,
    // identical bytes → one `{sha}.s3a`; PROVENANCE is the first
    // capture's url + captured time; both index lines store the same
    // `snapshot_sha256`; the first snapshot file is byte-identical.
    let fixture = FixtureServer::new();
    let body = b"shared body for two papers\n";
    fixture.set_ok("/a", body);
    fixture.set_ok("/b", body);
    let host = scholar_egress(&fixture);
    let url_a = format!("http://127.0.0.1:{}/a", fixture.port);
    let url_b = format!("http://127.0.0.1:{}/b", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url_a}"),
        "# First\n",
    );
    dossier(
        &host,
        "SRC-0002",
        &format!("title: Second\nsource_url: {url_b}"),
        "# Second\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    let sha = sha256_hex(body);
    let snap_path = host.snapshot_dir.join(format!("{sha}.s3a"));
    let first_bytes = std::fs::read(&snap_path).unwrap();
    let first_prov = provenance_document(&snap_path).expect("provenance");
    let second = host.download("SRC-0002");
    assert_eq!(second.status, 200, "{}", second.text());
    assert_eq!(
        second.json().get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(snapshot_files(&host.snapshot_dir), vec![format!("{sha}.s3a")]);
    assert_eq!(
        std::fs::read(&snap_path).unwrap(),
        first_bytes,
        "first snapshot file byte-identical"
    );
    assert_eq!(
        provenance_document(&snap_path).as_deref(),
        Some(first_prov.as_str()),
        "PROVENANCE is the first capture's"
    );
    assert!(first_prov.contains(&format!("url={url_a}\n")));
    let lines = read_index_lines(&host.data_dir);
    // §5.5: step 1 appends a null-sha line and step 2 appends the sha
    // line, so two downloads leave four lines; the two sha lines (one
    // per identity) store the same snapshot_sha256 (AT-157).
    assert_eq!(lines.len(), 4);
    assert_eq!(
        lines[0].get("snapshot_sha256"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(
        lines[1]
            .get("snapshot_sha256")
            .and_then(|v| v.as_str()),
        Some(sha.as_str())
    );
    assert_eq!(
        lines[2].get("snapshot_sha256"),
        Some(&serde_json::Value::Null)
    );
    assert_eq!(
        lines[3]
            .get("snapshot_sha256")
            .and_then(|v| v.as_str()),
        Some(sha.as_str())
    );
    assert_eq!(fixture.get_count("/a"), 1);
    assert_eq!(fixture.get_count("/b"), 1);
}

#[test]
fn snapshot_conflict_planted_corrupt() {
    // AT-180/AT-215: a newly fetched capture's `{sha}.s3a` already exists
    // and fails the DD-103 reuse checks → 409 `snapshot_conflict` with
    // `included: true` + `lrs_status`; the inclusion is kept and the file
    // is not overwritten. (A second identity with the same source_url has
    // no recorded sha, so this is the "newly fetched" case, not AT-209.)
    let fixture = FixtureServer::new();
    let body = b"conflict body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    dossier(
        &host,
        "SRC-0002",
        &format!("title: Second\nsource_url: {url}"),
        "# Second\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    let sha = sha256_hex(body);
    let snap_path = host.snapshot_dir.join(format!("{sha}.s3a"));
    // Corrupt real payload, not padding: the snapshot is a 512-byte
    // file header + hyper tiles (512-byte tile header each), so the
    // first BLOB chunk's 40-byte prefix starts at offset 1024 and its
    // blob_key spans 1024-1055. Offset 2048 is unused tile padding —
    // the seal does not cover it (cf. the Nov 18 S3A backup test), so
    // a flip there passes DD-103. Offset 1030 is inside the blob_key.
    corrupt_snapshot_at(&snap_path, 1030);
    let corrupt_bytes = std::fs::read(&snap_path).unwrap();
    let resp = host.download("SRC-0002");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "snapshot_conflict");
    let json = resp.json();
    assert_eq!(json.get("included").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(
        std::fs::read(&snap_path).unwrap(),
        corrupt_bytes,
        "the file is not overwritten"
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 2);
    assert_eq!(lrs.statement_count(), 2);
}

#[test]
fn snapshot_invalid_recorded_corrupt() {
    // AT-209: the last recorded sha's file is present but fails DD-103 →
    // a repeat download returns 200 with `snapshot: null`,
    // `snapshot_error: "snapshot_invalid"`, `download_error:
    // "snapshot_invalid"`, `path: null`; keeps the inclusion, starts no
    // fetch, does not fall back to a usable drive_path. A repeat select
    // of the same identity returns the same 200 `snapshot_invalid`.
    let fixture = FixtureServer::new();
    let body = b"invalid body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    let sha = sha256_hex(body);
    let snap_path = host.snapshot_dir.join(format!("{sha}.s3a"));
    // Offset 1030 is inside the first BLOB chunk's blob_key (see the
    // layout note in snapshot_conflict_planted_corrupt); 2048 would
    // be unused tile padding and pass DD-103.
    corrupt_snapshot_at(&snap_path, 1030);
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(json.get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("snapshot_error").and_then(|v| v.as_str()),
        Some("snapshot_invalid")
    );
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("snapshot_invalid")
    );
    assert_eq!(json.get("path"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped")
    );
    assert_eq!(fixture.get_count("/paper-1"), 1, "no second fetch");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(lrs.statement_count(), 1);

    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    assert_eq!(sel.json().get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(
        sel.json().get("snapshot_error").and_then(|v| v.as_str()),
        Some("snapshot_invalid")
    );
}

#[test]
fn snapshot_missing_refetches() {
    // AT-228 case 1: the recorded `{sha}.s3a` is deleted (missing, not
    // failing). A repeat download starts a fetch, publishes `{sha}.s3a`
    // again (a create, not an overwrite), returns 200 with that snapshot
    // path, does not return `snapshot_invalid`, and writes no second
    // tile-10. Case 2: the fixture returns different bytes → the host
    // publishes `{new_sha}.s3a` and appends a new index line with
    // `snapshot_sha256 = new_sha` (no second tile-10).
    let fixture = FixtureServer::new();
    let body = b"refetch body\n";
    fixture.set_route("/paper-1", FixtureRoute::Flaky {
        fail_status: 500,
        body: body.to_vec(),
    });
    // The flaky route fails until told otherwise; succeed from the start
    // for the first download, then change the body for case 2.
    fixture.set_flaky_succeed(true);
    let (host, _lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let first = host.download("SRC-0001");
    assert_eq!(first.status, 200, "{}", first.text());
    let sha = sha256_hex(body);
    let snap_path = host.snapshot_dir.join(format!("{sha}.s3a"));
    assert!(snap_path.exists());
    std::fs::remove_file(&snap_path).unwrap();

    let second = host.download("SRC-0001");
    assert_eq!(second.status, 200, "{}", second.text());
    assert_eq!(
        second.json().get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
    assert!(snap_path.exists(), "re-published");
    assert_eq!(fixture.get_count("/paper-1"), 2, "a fetch ran");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // §5.5: the first download left a null-sha step-1 line and a sha
    // step-2 line; a re-fetch hashing to the recorded sha adds no line.
    assert_eq!(read_index_lines(&host.data_dir).len(), 2, "same sha: no new line");

    // Case 2: different bytes now.
    let body2 = b"different refetch body\n";
    fixture.set_route("/paper-1", FixtureRoute::Static {
        status: 200,
        headers: Vec::new(),
        body: body2.to_vec(),
        delay_ms: 0,
    });
    std::fs::remove_file(&snap_path).unwrap();
    let third = host.download("SRC-0001");
    assert_eq!(third.status, 200, "{}", third.text());
    let sha2 = sha256_hex(body2);
    assert_eq!(
        third.json().get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha2}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
    assert!(host.snapshot_dir.join(format!("{sha2}.s3a")).exists());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    let lines = read_index_lines(&host.data_dir);
    assert_eq!(lines.len(), 3, "new sha appends a new line");
    assert_eq!(
        lines[2].get("snapshot_sha256").and_then(|v| v.as_str()),
        Some(sha2.as_str())
    );
    assert_eq!(
        lines[2].get("verb_id").and_then(|v| v.as_u64()),
        Some(13),
        "the new line is under this request's verb"
    );
}

#[test]
fn select_repeat_with_usable_snapshot_returns_snapshot() {
    // AT-83: an idempotent second select of an identity that already has
    // a usable snapshot returns 200 with `ok: true` and `snapshot` set
    // to `--snapshot-dir/{snapshot_sha256}.s3a`.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"at83 snap body\n");
    let (host, _lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let sel1 = host.select("SRC-0001");
    assert_eq!(sel1.status, 200, "{}", sel1.text());
    assert_eq!(sel1.json().get("snapshot"), Some(&serde_json::Value::Null));
    let dl = host.download("SRC-0001");
    assert_eq!(dl.status, 200, "{}", dl.text());
    let sha = sha256_hex(b"at83 snap body\n");
    let sel2 = host.select("SRC-0001");
    assert_eq!(sel2.status, 200, "{}", sel2.text());
    let json = sel2.json();
    assert_eq!(json.get("ok").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        json.get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
    assert_eq!(json.get("snapshot_error"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("skipped")
    );
}

#[test]
fn snapshot_too_large() {
    // AT-227: a sealed snapshot larger than --max-open-bytes is not
    // published; the request returns 200 with fetch_error
    // `snapshot_too_large`, a download also has download_error
    // `fetch_failed`, path/snapshot null, 2b skipped; the inclusion is
    // kept, step 3 runs, and the registry entry is removed (a later
    // request may retry). A snapshot for a small body is ~262,656 bytes
    // + seal overhead, so --max-open-bytes 200,000 trips it.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"small body, large snapshot\n");
    let lrs2 = LrsServer::new("::1");
    let host2 = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs2.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        max_open_bytes: 200_000,
        max_fetch_bytes: 200_000,
        ..ScholarOpts::default()
    });
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host2,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let resp = host2.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(
        json.get("fetch_error").and_then(|v| v.as_str()),
        Some("snapshot_too_large")
    );
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("fetch_failed")
    );
    assert_eq!(json.get("path"), Some(&serde_json::Value::Null));
    assert_eq!(json.get("snapshot"), Some(&serde_json::Value::Null));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert!(snapshot_files(&host2.snapshot_dir).is_empty());
    assert!(dir_files_no_lock(&host2.download_dir).is_empty());
    assert_eq!(count_tile10_records(&host2.working_archive_path()), 1);
    expect_inclusion(&host2, "SRC-0001", 13, None);
    assert_eq!(lrs2.statement_count(), 1);
}

// =====================================================================
// Download-name conflicts (AT-94, AT-216, AT-223)
// =====================================================================

#[test]
fn download_conflict_wrong_content() {
    // AT-94: --download-dir already contains a file at {sha256_hex}
    // whose contents do NOT hash to that name → 409 `download_conflict`
    // with `included: true` + `lrs_status`; the file is not overwritten
    // (byte-identical) and the inclusion write is kept.
    let fixture = FixtureServer::new();
    let body = b"conflict delivery body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let sha = sha256_hex(body);
    let planted = host.download_dir.join(&sha);
    std::fs::write(&planted, b"wrong contents, not the paper\n").unwrap();
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "download_conflict");
    let json = resp.json();
    assert_eq!(json.get("included").and_then(|v| v.as_bool()), Some(true));
    assert_eq!(
        json.get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(
        std::fs::read(&planted).unwrap(),
        b"wrong contents, not the paper\n"
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(lrs.statement_count(), 1);
    // The snapshot was still published (step 2 ran before 2b).
    assert_eq!(snapshot_files(&host.snapshot_dir), vec![format!("{sha}.s3a")]);
}

#[test]
fn download_conflict_symlink_and_fifo() {
    // AT-216: a planted symlink or FIFO at --download-dir/{sha} → 409
    // `download_conflict`; the host does not follow the symlink, does not
    // hang on the FIFO, and does not overwrite the planted path.
    let fixture = FixtureServer::new();
    let body_a = b"symlink case body\n";
    let body_b = b"fifo case body\n";
    fixture.set_ok("/a", body_a);
    fixture.set_ok("/b", body_b);
    let (host, _lrs) = scholar_with_lrs(&fixture, true);
    let url_a = format!("http://127.0.0.1:{}/a", fixture.port);
    let url_b = format!("http://127.0.0.1:{}/b", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url_a}"),
        "# First\n",
    );
    dossier(
        &host,
        "SRC-0002",
        &format!("title: Second\nsource_url: {url_b}"),
        "# Second\n",
    );
    // Symlink case: the link points at a sentinel file; it must stay
    // byte-identical.
    let sha_a = sha256_hex(body_a);
    let sentinel = host.data_dir.join("sentinel.txt");
    std::fs::write(&sentinel, b"sentinel bytes\n").unwrap();
    std::os::unix::fs::symlink(&sentinel, host.download_dir.join(&sha_a)).unwrap();
    let resp_a = host.download("SRC-0001");
    assert_eq!(resp_a.status, 409, "{}", resp_a.text());
    assert_eq!(resp_a.error_code(), "download_conflict");
    assert_eq!(
        std::fs::read(&sentinel).unwrap(),
        b"sentinel bytes\n",
        "the symlink target is untouched"
    );
    // FIFO case: the host must not block opening it.
    let sha_b = sha256_hex(body_b);
    let fifo = host.download_dir.join(&sha_b);
    let cname = std::ffi::CString::new(fifo.to_string_lossy().into_owned()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cname.as_ptr(), 0o600) }, 0);
    let start = std::time::Instant::now();
    let resp_b = host.download("SRC-0002");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "no hang on the FIFO ({:?})",
        start.elapsed()
    );
    assert_eq!(resp_b.status, 409, "{}", resp_b.text());
    assert_eq!(resp_b.error_code(), "download_conflict");
}

#[test]
fn download_conflict_oversize_existing_file() {
    // AT-223 second half: a regular file at the destination larger than
    // --max-open-bytes → 409 `download_conflict`, no overwrite, inclusion
    // kept. (On-disk source, so no snapshot is involved.)
    let host = RunningHost::scholar_opts(ScholarOpts {
        max_open_bytes: 8192,
        max_fetch_bytes: 8192,
        ..ScholarOpts::default()
    });
    let data = b"small on-disk bytes\n";
    let data_dir = host.corpus_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("SRC-0001.txt"), data).unwrap();
    corpus_dossier(
        &host,
        "SRC-0001",
        "title: First Paper\ndrive_path: ../data/SRC-0001.txt",
        "# First Paper\n",
    );
    let sha = sha256_hex(data);
    let planted = host.download_dir.join(&sha);
    std::fs::write(&planted, vec![b'y'; 9000]).unwrap();
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 409, "{}", resp.text());
    assert_eq!(resp.error_code(), "download_conflict");
    assert_eq!(
        resp.json().get("included").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(std::fs::read(&planted).unwrap().len(), 9000, "not overwritten");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

// =====================================================================
// Concurrency (AT-126, AT-221)
// =====================================================================

#[test]
fn concurrent_downloads_one_get() {
    // AT-126: N concurrent downloads of the same identity, fixture
    // returning different bytes per GET → exactly one tile-10 record, at
    // most one snapshot file (same snapshot_sha256), exactly one GET.
    let fixture = std::sync::Arc::new(FixtureServer::new());
    fixture.set_route("/counter", FixtureRoute::Counter);
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let host = std::sync::Arc::new(host);
    let url = format!("http://127.0.0.1:{}/counter", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    let mut handles = Vec::new();
    for _ in 0..4 {
        let h = host.clone();
        handles.push(std::thread::spawn(move || h.download("SRC-0001")));
    }
    let mut statuses = Vec::new();
    for h in handles {
        let r = h.join().unwrap();
        statuses.push(r.status);
    }
    assert!(
        statuses.iter().all(|s| *s == 200),
        "all four downloads return 200: {statuses:?}"
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // §5.5: the starter leaves a null-sha step-1 line and a sha step-2
    // line; the waiters append nothing.
    assert_eq!(read_index_lines(&host.data_dir).len(), 2);
    assert_eq!(snapshot_files(&host.snapshot_dir).len(), 1);
    assert_eq!(fixture.get_count("/counter"), 1, "exactly one GET");
    assert_eq!(lrs.statement_count(), 1, "exactly one LRS statement");
}

#[test]
fn download_runs_to_completion_after_disconnect() {
    // AT-221: a download that passed membership runs steps 0–3 to
    // completion even if the client disconnects; exactly one LRS
    // statement for that identity.
    let fixture = FixtureServer::new();
    fixture.set_route(
        "/slow",
        FixtureRoute::Static {
            status: 200,
            headers: Vec::new(),
            body: b"disconnect body\n".to_vec(),
            delay_ms: 1500,
        },
    );
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/slow", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First\n",
    );
    // Write the POST bytes and drop the connection immediately.
    let body = serde_json::to_vec(&serde_json::json!({"paper_id": "SRC-0001"})).unwrap();
    let req = format!(
        "POST /v1/download HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        host.token,
        body.len()
    );
    {
        let mut conn = host.raw_connection();
        conn.write_all(req.as_bytes()).unwrap();
        conn.write_all(&body).unwrap();
        conn.flush().unwrap();
        // Drop without reading the response.
    }
    // Poll for the LRS statement (the fetch alone takes ~1.5 s).
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while lrs.statement_count() == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(lrs.statement_count(), 1, "exactly one LRS statement");
    assert_eq!(
        lrs.verbs(),
        vec!["https://ashecribb.dev/xapi/verbs/saved"]
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // §5.5: the download leaves a null-sha step-1 line and a sha
    // step-2 line.
    assert_eq!(read_index_lines(&host.data_dir).len(), 2);
    assert_eq!(fixture.get_count("/slow"), 1);
    let sha = sha256_hex(b"disconnect body\n");
    assert!(host.download_dir.join(&sha).exists(), "delivery written");
}

// =====================================================================
// drive_path safety (AT-202) and file:// source_url (AT-55)
// =====================================================================

#[test]
fn download_drive_path_symlink_swap_is_not_path_escape() {
    // AT-202: after candidates reports on_disk: true for a drive_path,
    // that path is replaced with a symlink whose target is
    // --auth-token-file, then POST /v1/download is issued. The download
    // does not write those token bytes into --download-dir, does not hang
    // holding the write mutex, and is not 409 path_escape; the path is
    // not a usable on-disk source (the fetch runs instead).
    let fixture = FixtureServer::new();
    let fetched = b"fetched after the swap\n";
    fixture.set_ok("/paper-1", fetched);
    let host = scholar_egress(&fixture);
    let data_dir = host.wiki_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let data_path = data_dir.join("SRC-0001.txt");
    std::fs::write(&data_path, b"original on-disk bytes\n").unwrap();
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}\ndrive_path: ../data/SRC-0001.txt"),
        "# First Paper\n",
    );
    let cands = host.candidates_json();
    let paper = cands
        .get("papers")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some("SRC-0001"))
        .expect("candidate listed");
    assert_eq!(
        paper.get("on_disk").and_then(|v| v.as_bool()),
        Some(true),
        "on_disk: true before the swap"
    );
    std::fs::remove_file(&data_path).unwrap();
    std::os::unix::fs::symlink(&host.token_file, &data_path).unwrap();
    let start = std::time::Instant::now();
    let resp = host.download("SRC-0001");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "no hang ({:?})",
        start.elapsed()
    );
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_ne!(resp.error_code(), "path_escape");
    assert_eq!(fixture.get_count("/paper-1"), 1, "fell through to the fetch");
    let sha = sha256_hex(fetched);
    assert_eq!(std::fs::read(host.download_dir.join(&sha)).unwrap(), fetched);
    let token = std::fs::read(&host.token_file).unwrap();
    assert_ne!(fetched, token.as_slice(), "sanity: token != fetched body");
}

#[test]
fn download_drive_path_fifo_swap_does_not_hang() {
    // AT-202/AT-197: the same replacement with a FIFO does not hang the
    // write mutex; the path is not a usable on-disk source.
    let fixture = FixtureServer::new();
    let fetched = b"fetched after the fifo swap\n";
    fixture.set_ok("/paper-1", fetched);
    let host = scholar_egress(&fixture);
    let data_dir = host.wiki_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    let data_path = data_dir.join("SRC-0001.txt");
    std::fs::write(&data_path, b"original on-disk bytes\n").unwrap();
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}\ndrive_path: ../data/SRC-0001.txt"),
        "# First Paper\n",
    );
    std::fs::remove_file(&data_path).unwrap();
    let cname = std::ffi::CString::new(data_path.to_string_lossy().into_owned()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(cname.as_ptr(), 0o600) }, 0);
    let start = std::time::Instant::now();
    let resp = host.download("SRC-0001");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "no hang on the FIFO ({:?})",
        start.elapsed()
    );
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(fixture.get_count("/paper-1"), 1);
}

#[test]
fn file_url_source_is_nulled_and_never_read() {
    // AT-55: a candidate whose YAML source_url is a file:// URL pointing
    // at the token file is listed with source_url: null. Select and
    // download of that identity perform no HTTP request and no filesystem
    // read of the file:// target; with no usable drive_path, download
    // returns 200 with download_error `no_source` and path null.
    let fixture = FixtureServer::new();
    let host = scholar_egress(&fixture);
    let token_url = format!("file://{}", host.token_file.to_string_lossy());
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {token_url}"),
        "# First Paper\n",
    );
    let cands = host.candidates_json();
    let paper = cands
        .get("papers")
        .and_then(|v| v.as_array())
        .unwrap()
        .iter()
        .find(|p| p.get("id").and_then(|v| v.as_str()) == Some("SRC-0001"))
        .expect("candidate listed");
    assert_eq!(
        paper.get("source_url"),
        Some(&serde_json::Value::Null),
        "a file:// source_url is listed as null"
    );
    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("no_source")
    );
    assert_eq!(json.get("path"), Some(&serde_json::Value::Null));
    assert_eq!(fixture.total_connections(), 0, "no HTTP request");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
}

// =====================================================================
// Startup errors (AT-158, AT-182, AT-223)
// =====================================================================

#[test]
fn startup_malformed_index_is_error() {
    // AT-158: a present inclusion-index.jsonl with a truncated last line,
    // a non-JSON line, or a line missing a required key is a startup
    // error that names the file and binds nothing.
    for (name, content) in [
        ("truncated", "{\"paper_identity\": \"SRC-0001\", \"verb_id\": 13, \"object_hash\": 1, \"timestamp_sec\": 1, \"working_archive\": \"x\", \"file_uuid\": \"00\""),
        ("non-json", "this is not json\n"),
        ("missing-key", "{\"paper_identity\": \"SRC-0001\", \"verb_id\": 13}\n"),
    ] {
        let dir = TempDir::new("idxbad");
        let (mut argv, paths) = scholar_argv(&dir, free_port());
        std::fs::write(paths.data.join("inclusion-index.jsonl"), content).unwrap();
        // scholar_argv already created the data dir; the index file is
        // planted before the host starts.
        let (code, stderr) = run_expect_exit(&argv, &[], Duration::from_secs(10));
        assert_ne!(code, 0, "{name}: expected a startup failure");
        assert!(
            stderr.contains("inclusion-index.jsonl"),
            "{name}: the error names the file: {stderr}"
        );
        let _ = &mut argv;
    }
}

#[test]
fn startup_max_fetch_gt_max_open_is_error() {
    // AT-223 first half: --max-fetch-bytes greater than --max-open-bytes
    // is a startup error that names both flags, binds nothing, and
    // creates no sidecar lock file.
    let dir = TempDir::new("capbad");
    let (mut argv, paths) = scholar_argv(&dir, free_port());
    for i in 0..argv.len() {
        if argv[i] == "--max-fetch-bytes" {
            argv[i + 1] = (MAX_OPEN_BYTES * 2).to_string();
        }
    }
    let (code, stderr) = run_expect_exit(&argv, &[], Duration::from_secs(10));
    assert_ne!(code, 0, "expected a startup failure");
    assert!(stderr.contains("--max-fetch-bytes"), "names the flag: {stderr}");
    assert!(stderr.contains("--max-open-bytes"), "names the flag: {stderr}");
    assert!(
        !paths.data.join(".lock").exists(),
        "no sidecar lock file was created"
    );
    let _ = &mut argv;
}

#[test]
fn startup_lrs_retry_not_a_dir_is_error() {
    // §5.5 retry queue: if --data-dir/lrs-retry already exists at
    // startup, lstat must show a directory; a regular file is a startup
    // error that names lrs-retry and binds nothing.
    let dir = TempDir::new("retrybad");
    let (argv, paths) = scholar_argv(&dir, free_port());
    std::fs::write(paths.data.join("lrs-retry"), b"not a directory\n").unwrap();
    let (code, stderr) = run_expect_exit(&argv, &[], Duration::from_secs(10));
    assert_ne!(code, 0, "expected a startup failure");
    assert!(stderr.contains("lrs-retry"), "names lrs-retry: {stderr}");
    let _ = &argv;
}

#[test]
fn startup_bad_lrs_credentials_is_error() {
    // AT-182: a credentials file with no valid credential line (a line
    // with fewer than two fields, an empty username or password, or only
    // comments/blank lines) is a startup error that names the file.
    for (name, content) in [
        ("one-field", b"onlyuser\n".as_slice()),
        ("empty-password", b"user:\n".as_slice()),
        ("empty-user", b":pass\n".as_slice()),
        ("comments-only", b"# nothing here\n\n".as_slice()),
    ] {
        let dir = TempDir::new("credsbad");
        let (mut argv, _paths) = scholar_argv(&dir, free_port());
        let creds = dir.write("lrs-creds.txt", content);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let consent = dir.join("consent.txt");
        argv.push("--lrs-url".into());
        argv.push("http://127.0.0.1:9/xapi".into());
        argv.push("--consent-state-file".into());
        argv.push(consent.to_string_lossy().into_owned());
        argv.push("--lrs-credentials-file".into());
        argv.push(creds.to_string_lossy().into_owned());
        argv.push("--lrs-timeout-secs".into());
        argv.push("5".into());
        let (code, stderr) = run_expect_exit(&argv, &[], Duration::from_secs(10));
        assert_ne!(code, 0, "{name}: expected a startup failure");
        assert!(
            stderr.contains("lrs-creds.txt"),
            "{name}: the error names the file: {stderr}"
        );
        let _ = &mut argv;
    }
}

// =====================================================================
// LRS outcomes (AT-55 is above; AT-61, AT-139 is above, AT-182 creds)
// =====================================================================

#[test]
fn consent_off_select_leaves_no_trace_in_logs_or_data() {
    // AT-61: after a consent-off select, --log-dir contains no paper ids
    // or source URLs, and --data-dir contains none other than in
    // inclusion-index.jsonl. The working archive still has the inclusion
    // record (from the earlier download). --data-dir/lrs-retry/ is empty
    // or absent.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"at61 body\n");
    let (host, lrs) = scholar_with_lrs(&fixture, true);
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First Paper\n",
    );
    let dl = host.download("SRC-0001");
    assert_eq!(dl.status, 200, "{}", dl.text());
    let off = host.consent_off();
    assert_eq!(off.status, 200, "{}", off.text());
    let sel = host.select("SRC-0001");
    assert_eq!(sel.status, 200, "{}", sel.text());
    assert_eq!(lrs.statement_count(), 1, "the select sent nothing");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    // Log dir: no paper ids or source URLs anywhere.
    for entry in std::fs::read_dir(&host.log_dir).unwrap().flatten() {
        let bytes = std::fs::read(entry.path()).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("SRC-0001"),
            "paper id leaked into log {}",
            entry.path().display()
        );
        assert!(
            !text.contains(&url),
            "source URL leaked into log {}",
            entry.path().display()
        );
    }
    // Data dir: no paper ids or source URLs except in the index file.
    for entry in std::fs::read_dir(&host.data_dir).unwrap().flatten() {
        if entry.file_name() == "inclusion-index.jsonl" {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            for inner in std::fs::read_dir(&path).unwrap().flatten() {
                let bytes = std::fs::read(inner.path()).unwrap_or_default();
                let text = String::from_utf8_lossy(&bytes);
                assert!(
                    !text.contains("SRC-0001") && !text.contains(&url),
                    "leak in {}",
                    inner.path().display()
                );
            }
            continue;
        }
        let bytes = std::fs::read(&path).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains("SRC-0001") && !text.contains(&url),
            "leak in {}",
            path.display()
        );
    }
    let retry = host.data_dir.join("lrs-retry");
    assert!(
        !retry.exists()
            || std::fs::read_dir(&retry).map(|mut d| d.next().is_none()).unwrap_or(false),
        "lrs-retry/ is empty or absent"
    );
}

#[test]
fn lrs_queued_on_503_and_failed_on_400() {
    // DD-134: an LRS 503 (or 408/429/5xx) queues the statement
    // (`lrs_status: "queued"`, a `{id}.json` retry file is kept); a 400
    // (other 4xx) is permanent (`lrs_status: "failed"`, a `{id}.permanent`
    // file). Step 11 owns the retry; Step 10 owns the classification.
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    lrs.set_status(503);
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp = host.select("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("queued")
    );
    let retry_dir = host.data_dir.join("lrs-retry");
    let queued: Vec<String> = dir_files(&retry_dir)
        .into_iter()
        .filter(|n| n.ends_with(".json"))
        .collect();
    assert_eq!(queued.len(), 1, "one retry file kept: {queued:?}");
    assert_eq!(lrs.statement_count(), 1, "the POST was attempted");

    // 400 → permanent.
    let lrs2 = LrsServer::new("::1");
    lrs2.set_status(400);
    let host2 = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::Egress,
        lrs_url: Some(lrs2.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        ..ScholarOpts::default()
    });
    dossier(&host2, "SRC-0001", "title: First Paper", "# First Paper\n");
    let resp2 = host2.select("SRC-0001");
    assert_eq!(resp2.status, 200, "{}", resp2.text());
    assert_eq!(
        resp2.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("failed")
    );
    let perm: Vec<String> = dir_files(&host2.data_dir.join("lrs-retry"))
        .into_iter()
        .filter(|n| n.ends_with(".permanent"))
        .collect();
    assert_eq!(perm.len(), 1, "one permanent file: {perm:?}");
}

// =====================================================================
// Hook-injected failures (AT-158, AT-202 grow, AT-212, AT-213, AT-214,
// AT-219) — hooks binary only.
// =====================================================================

fn hooks_host(fixture: &FixtureServer, hooks: &str) -> (RunningHost, LrsServer) {
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: vec![("S3A_KITE_TEST_HOOKS".to_string(), hooks.to_string())],
        ..ScholarOpts::default()
    });
    (host, lrs)
}

#[test]
fn hook_fail_index_append_after_tile10() {
    // AT-158: after a successful step-1 tile-10 write, a forced
    // index-append failure (fail_index_append) returns 500 `write_failed`
    // with `included: true` and no `lrs_status` (steps 2, 2b, and 3 are
    // skipped), and leaves the tile-10 record in the working archive.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"hook body\n");
    let (host, lrs) = hooks_host(&fixture, "fail_index_append=fail_with_errno=5");
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    let json = resp.json();
    assert_eq!(json.get("included").and_then(|v| v.as_bool()), Some(true));
    assert!(json.get("lrs_status").is_none(), "no lrs_status: {}", resp.text());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(read_index_lines(&host.data_dir).len(), 0, "no index line");
    assert_eq!(fixture.get_count("/paper-1"), 0, "step 2 never ran");
    assert_eq!(lrs.statement_count(), 0, "step 3 never ran");
}

#[test]
fn hook_fail_snapshot_write_temp() {
    // AT-212: after step 1, fail_snapshot_write_temp (one-shot) fails the
    // step-2 snapshot temp write → 500 `write_failed` with `included:
    // true` + `lrs_status`; the inclusion is kept, no snapshot is
    // published, the temp is deleted, and the registry entry is gone (a
    // later download retries the fetch).
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"hook snap body\n");
    let (host, lrs) = hooks_host(&fixture, "fail_snapshot_write_temp=fail_once_with_errno=5");
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    assert_eq!(
        resp.json().get("included").and_then(|v| v.as_bool()),
        Some(true)
    );
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert!(snapshot_files(&host.snapshot_dir).is_empty());
    assert!(dir_files(&host.snapshot_dir)
        .iter()
        .all(|n| !n.starts_with("s3a-kite-tmp-")), "temp deleted");
    assert_eq!(lrs.statement_count(), 1);
    // A later download retries the fetch (the hook fired once).
    let retry = host.download("SRC-0001");
    assert_eq!(retry.status, 200, "{}", retry.text());
    assert_eq!(fixture.get_count("/paper-1"), 2);
    assert_eq!(snapshot_files(&host.snapshot_dir).len(), 1);
}

#[test]
fn hook_fail_snapshot_index_append() {
    // AT-213: the snapshot temp is written, fsync'd, and renamed, then
    // fail_snapshot_index_append (one-shot) fails the index-line append →
    // 500 `write_failed` with `included: true` + `lrs_status`; the
    // published `{sha}.s3a` is left for DD-103 reuse; no `snapshot_sha256`
    // is recorded. A later download reuses the left file.
    let fixture = FixtureServer::new();
    let body = b"hook idx body\n";
    fixture.set_ok("/paper-1", body);
    let (host, lrs) = hooks_host(&fixture, "fail_snapshot_index_append=fail_once_with_errno=5");
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    let sha = sha256_hex(body);
    assert_eq!(snapshot_files(&host.snapshot_dir), vec![format!("{sha}.s3a")]);
    let lines = read_index_lines(&host.data_dir);
    assert_eq!(lines.len(), 1);
    assert_eq!(
        lines[0].get("snapshot_sha256"),
        Some(&serde_json::Value::Null),
        "no snapshot_sha256 recorded"
    );
    assert_eq!(lrs.statement_count(), 1);
    // A later download fetches again (SPEC line 1217: it has "captured
    // bytes" hashing to that sha) and reuses the left file at publish
    // (DD-103: the existing file is not overwritten). The hook is
    // one-shot, so this request's index append (recording the sha)
    // succeeds.
    let retry = host.download("SRC-0001");
    assert_eq!(retry.status, 200, "{}", retry.text());
    assert_eq!(fixture.get_count("/paper-1"), 2, "later download fetches again");
    assert_eq!(
        snapshot_files(&host.snapshot_dir),
        vec![format!("{sha}.s3a")],
        "one file, not overwritten"
    );
    let lines = read_index_lines(&host.data_dir);
    assert_eq!(lines.len(), 2, "step-1 null + step-2 sha");
    assert_eq!(
        lines[1].get("snapshot_sha256").and_then(|v| v.as_str()),
        Some(sha.as_str()),
        "the sha is now recorded"
    );
    assert_eq!(
        retry.json().get("snapshot").and_then(|v| v.as_str()),
        Some(
            host.snapshot_dir
                .join(format!("{sha}.s3a"))
                .to_string_lossy()
                .as_ref()
        )
    );
}

#[test]
fn hook_fail_download_write() {
    // AT-214: fail_download_write fails every step-2b download temp
    // write/fsync/rename → 500 `write_failed` with `included: true` +
    // `lrs_status`; no published file appears, the temp is deleted, and
    // the inclusion is kept.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"hook dl body\n");
    let (host, lrs) = hooks_host(&fixture, "fail_download_write=fail_with_errno=5");
    let url = format!("http://127.0.0.1:{}/paper-1", fixture.port);
    dossier(
        &host,
        "SRC-0001",
        &format!("title: First\nsource_url: {url}"),
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    assert_eq!(
        resp.json().get("lrs_status").and_then(|v| v.as_str()),
        Some("sent")
    );
    assert!(dir_files_no_lock(&host.download_dir).is_empty(), "no published file");
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(lrs.statement_count(), 1);
    // The snapshot was published (step 2 ran); only 2b failed.
    assert_eq!(snapshot_files(&host.snapshot_dir).len(), 1);
}

#[test]
fn hook_grow_drive_path_during_copy() {
    // AT-202 further case: after the pre-step-0 open has accepted a
    // drive_path fd under the cap, grow_drive_path_during_copy (grow)
    // makes the file grow so more than --max-open-bytes arrive during the
    // 2b copy → the host deletes the temp, writes no published download
    // file, keeps the inclusion, does not start a mid-request fetch, and
    // returns 200 with download_error `source_changed`.
    let fixture = FixtureServer::new();
    fixture.set_ok("/paper-1", b"must not be fetched\n");
    let lrs = LrsServer::new("::1");
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        max_open_bytes: 1024,
        max_fetch_bytes: 1024,
        env: vec![(
            "S3A_KITE_TEST_HOOKS".to_string(),
            "grow_drive_path_during_copy=grow".to_string(),
        )],
        ..ScholarOpts::default()
    });
    let data_dir = host.corpus_root.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    std::fs::write(data_dir.join("SRC-0001.txt"), b"small start\n").unwrap();
    corpus_dossier(
        &host,
        "SRC-0001",
        "title: First Paper\ndrive_path: ../data/SRC-0001.txt\nsource_url: http://127.0.0.1:1/paper-1",
        "# First Paper\n",
    );
    let resp = host.download("SRC-0001");
    assert_eq!(resp.status, 200, "{}", resp.text());
    let json = resp.json();
    assert_eq!(
        json.get("download_error").and_then(|v| v.as_str()),
        Some("source_changed")
    );
    assert_eq!(json.get("path"), Some(&serde_json::Value::Null));
    assert!(dir_files_no_lock(&host.download_dir).is_empty());
    assert_eq!(count_tile10_records(&host.working_archive_path()), 1);
    assert_eq!(fixture.total_connections(), 0, "no mid-request fetch");
}

#[test]
fn hook_pause_before_name_binding_check() {
    // AT-219: pause_before_name_binding_check pauses the host after the
    // writer is attached and immediately before the name-binding fstatat
    // re-check. The test replaces the final component with a symlink into
    // --corpus-root and writes CONTINUE. The select returns 500
    // `write_failed`, writes no tile-10, does not follow/append/truncate
    // the symlink target, and leaves the target byte-identical.
    let fixture = FixtureServer::new();
    let lrs = LrsServer::new("::1");
    let ready = std::env::temp_dir().join(format!(
        "kite10_ready_{}_{}",
        std::process::id(),
        "at219"
    ));
    let cont = std::env::temp_dir().join(format!(
        "kite10_cont_{}_{}",
        std::process::id(),
        "at219"
    ));
    let _ = std::fs::remove_file(&ready);
    let _ = std::fs::remove_file(&cont);
    let host = RunningHost::scholar_opts(ScholarOpts {
        bin: HostBin::EgressHooks,
        lrs_url: Some(lrs.url()),
        consent_on: true,
        egress_test_loopback_port: Some(fixture.port),
        env: vec![
            (
                "S3A_KITE_TEST_HOOKS".to_string(),
                "pause_before_name_binding_check=pause_until_signal".to_string(),
            ),
            (
                "S3A_KITE_TEST_HOOK_READY".to_string(),
                ready.to_string_lossy().into_owned(),
            ),
            (
                "S3A_KITE_TEST_HOOK_CONTINUE".to_string(),
                cont.to_string_lossy().into_owned(),
            ),
        ],
        ..ScholarOpts::default()
    });
    dossier(&host, "SRC-0001", "title: First Paper", "# First Paper\n");
    // The symlink target inside --corpus-root.
    let target = host.corpus_root.join("target.s3a");
    std::fs::write(&target, b"target bytes, must stay identical\n").unwrap();
    let host = std::sync::Arc::new(host);
    let h2 = host.clone();
    let handle = std::thread::spawn(move || h2.select("SRC-0001"));
    // Wait for the pause (READY contains the point name).
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(text) = std::fs::read_to_string(&ready) {
            if text.contains("pause_before_name_binding_check") {
                break;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the host never paused at the name-binding check"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    // Swap the final component for a symlink into --corpus-root.
    let wa = host.working_archive_path();
    std::fs::remove_file(&wa).unwrap();
    std::os::unix::fs::symlink(&target, &wa).unwrap();
    std::fs::write(&cont, b"go\n").unwrap();
    let resp = handle.join().unwrap();
    assert_eq!(resp.status, 500, "{}", resp.text());
    assert_eq!(resp.error_code(), "write_failed");
    assert_eq!(
        std::fs::read(&target).unwrap(),
        b"target bytes, must stay identical\n",
        "the symlink target is byte-identical"
    );
    assert_eq!(count_tile10_records(&wa), 0, "no tile-10 written");
    let _ = std::fs::remove_file(&ready);
    let _ = std::fs::remove_file(&cont);
}

// =====================================================================
// Startup temp cleanup (AT-187 further case, §5.6)
// =====================================================================

#[test]
fn startup_temp_cleanup_deletes_stale_only() {
    // §5.6 / AT-187 further case: a lockable stale `s3a-kite-tmp-*` file
    // and a separately LOCK_EX-locked (live-looking) `s3a-kite-tmp-*` file
    // are planted in the host's own --snapshot-dir before it starts;
    // after that host starts (startup cleanup runs before /health), only
    // the unlocked stale file is deleted and the locked one remains.
    let host = RunningHost::scholar_opts(ScholarOpts {
        setup: Some(Box::new(move |dir| {
            let snap = dir.join("snapshot");
            std::fs::write(snap.join("s3a-kite-tmp-999-stale"), b"stale\n").unwrap();
            let live_path = snap.join("s3a-kite-tmp-999-live");
            let f = std::fs::File::create(&live_path).unwrap();
            use std::os::unix::io::AsRawFd;
            assert_eq!(
                unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                0,
                "lock the live temp"
            );
            // Leak the File so the LOCK_EX survives the setup closure and
            // the host's startup cleanup (this test process holds it).
            Box::leak(Box::new(f));
        })),
        ..ScholarOpts::default()
    });
    assert!(
        !host.snapshot_dir.join("s3a-kite-tmp-999-stale").exists(),
        "the stale temp was deleted at startup"
    );
    assert!(
        host.snapshot_dir.join("s3a-kite-tmp-999-live").exists(),
        "the locked temp remains"
    );
}
