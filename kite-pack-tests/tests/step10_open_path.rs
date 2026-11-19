//! Step 10 — pack `name_hint` retention (8c open item) and the AT-101
//! mid-call swap test (the pack half; the host half is the Step 9
//! `open_path` + `paper_identity` work).

mod fixture8b;
mod support;

use std::fs::File;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};

use s3a_pack_kite::{open, open_path, open_verified, render_page, ScholarRoots};

use fixture8b::*;
use support::*;

fn dossier_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kite10_{}_{}_{}",
        std::process::id(),
        name,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("papers")).expect("create papers dir");
    dir
}

fn write_dossier(root: &Path, rel: &str, contents: &str) -> PathBuf {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("create parent");
    }
    std::fs::write(&path, contents).expect("write dossier");
    path
}

fn roots(wiki: &Path, corpus: &Path, shadowed: bool) -> ScholarRoots {
    ScholarRoots {
        wiki_root: wiki.to_path_buf(),
        corpus_root: corpus.to_path_buf(),
        shadowed_by_wiki: shadowed,
    }
}

const DOSSIER_A: &str = "---\ntitle: First Paper\nsource_id: SRC-0007\n---\n\n# First Paper\n\nBody A.\n";
const DOSSIER_B: &str = "---\ntitle: Second Paper\nsource_id: SRC-0008\n---\n\n# Second Paper\n\nBody B.\n";

#[test]
fn name_hint_retained_from_open_path() {
    // 8c open item: open_path retains the effective name hint on the
    // handle — the /v1/open `name` when given, else the basename.
    let wiki = dossier_root("wiki");
    let corpus = dossier_root("corpus");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let file = File::open(&path).expect("open fd");
    let fd: RawFd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let canonical = std::fs::canonicalize(&path).expect("canonicalize");
    let h = open_path(
        &ctx,
        fd,
        canonical.to_str().expect("utf8 path"),
        &wiki,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
        Some(&r),
        Some("custom-name.md".to_string()),
    )
    .expect("open_path with an explicit hint");
    std::mem::forget(file);
    assert_eq!(h.name_hint(), Some("custom-name.md"));
    assert_eq!(h.paper_identity(), Some("SRC-0007"));
}

#[test]
fn name_hint_falls_back_to_basename() {
    // open_path with no hint retains the basename of the opened path.
    let wiki = dossier_root("wiki");
    let corpus = dossier_root("corpus");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let file = File::open(&path).expect("open fd");
    let fd: RawFd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let canonical = std::fs::canonicalize(&path).expect("canonicalize");
    let h = open_path(
        &ctx,
        fd,
        canonical.to_str().expect("utf8 path"),
        &wiki,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
        Some(&r),
        None,
    )
    .expect("open_path without a hint");
    std::mem::forget(file);
    assert_eq!(h.name_hint(), Some("SRC-0007.md"));
    assert_eq!(h.paper_identity(), Some("SRC-0007"));
}

#[test]
fn name_hint_retained_from_open_bytes() {
    // open() retains the caller's hint; None stays None.
    let ctx = test_context();
    let h = open(
        &ctx,
        DOSSIER_A.as_bytes(),
        Some("SRC-0007.md".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
    )
    .expect("open bytes with a hint");
    assert_eq!(h.name_hint(), Some("SRC-0007.md"));
    assert_eq!(h.paper_identity(), None);
    let h2 = open(
        &ctx,
        DOSSIER_A.as_bytes(),
        None,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
    )
    .expect("open bytes without a hint");
    assert_eq!(h2.name_hint(), None);
}

#[test]
fn at_101_mid_call_swap_reads_the_fd() {
    // AT-101 pack half: open_path reads from the already-open fd only.
    // The path is replaced (new inode) after the fd is opened and before
    // open_path runs; the handle still carries the ORIGINAL bytes'
    // identity, never the replacement's.
    let wiki = dossier_root("wiki");
    let corpus = dossier_root("corpus");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let file = File::open(&path).expect("open fd");
    let fd: RawFd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    // Replace the path with a different dossier (atomic rename = new inode).
    let tmp = write_dossier(&wiki, "papers/.swap-tmp.md", DOSSIER_B);
    std::fs::rename(&tmp, &path).expect("atomic swap");
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        DOSSIER_B,
        "the path now holds the replacement"
    );
    let canonical = std::fs::canonicalize(&path).expect("canonicalize");
    let h = open_path(
        &ctx,
        fd,
        canonical.to_str().expect("utf8 path"),
        &wiki,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
        Some(&r),
        Some("SRC-0007.md".to_string()),
    )
    .expect("open_path after the swap");
    std::mem::forget(file);
    // The fd still refers to the original inode's bytes.
    assert_eq!(h.paper_identity(), Some("SRC-0007"));
    assert_eq!(h.name_hint(), Some("SRC-0007.md"));
    // The Select control is for the original identity.
    let frame = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, true).expect("render_page");
    let select = frame
        .semantics
        .iter()
        .find(|n| n.role == "select")
        .and_then(|n| n.paper_id.clone());
    assert_eq!(select, Some("SRC-0007".to_string()));
}

#[test]
fn name_hint_retained_from_open_verified() {
    // open_verified retains the caller's hint as well.
    let ctx = test_context();
    // A report comes from opening the same bytes first (the
    // unsigned-unsealed constructor is crate-private).
    let fx = build_hyper(vec![], false);
    let opened = open(
        &ctx,
        &fx.bytes,
        None,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
    )
    .expect("open the fixture archive");
    let report = s3a_pack_kite::verification_report(&opened).expect("report");
    let h = open_verified(
        &ctx,
        &fx.bytes,
        Some("archive.s3a".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        report,
    )
    .expect("open_verified");
    assert_eq!(h.name_hint(), Some("archive.s3a"));
}
