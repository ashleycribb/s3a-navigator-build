//! Step 9 — pack `open_path` (AT-96, AT-101) and Scholar paper enumeration
//! (`scholar_paper_records`, AT-225 item 2 pack half).
//!
//! Test-first: written against SPEC v24.4 before `open_path` exists; the
//! first run is expected to fail (the symbols are not yet exported).

mod fixture8b;
mod support;

use std::fs::File;
use std::os::unix::io::RawFd;
use std::path::{Path, PathBuf};

use s3a_core::{HyperTileHeader, MarkRecord, ADDON_EDU, MARK_CONSENT_WITHDRAWN, MARK_SET};
use s3a_pack_kite::{
    open, open_path, render_page, scholar_paper_records, ScholarRoots,
};
use s3a_pack_scholar::records::AcademicPaperRecord;
use s3a_pack_scholar::{ScholarPaperBlobHeader, ScholarTileType};
use sha2::{Digest, Sha256};

use fixture8b::*;
use support::*;

const NOW: u64 = 1_700_000_000;

fn dossier_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "kite9_{}_{}_{}",
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

fn open_fd(path: &Path, root: &Path, ctx: &s3a_pack_kite::KiteContext, roots: Option<&ScholarRoots>) -> s3a_pack_kite::Handle {
    let file = File::open(path).expect("open fd");
    let fd: RawFd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let canonical = std::fs::canonicalize(path).expect("canonicalize");
    // The pack must not close the fd; keep `file` alive for the call.
    let handle = open_path(
        ctx,
        fd,
        canonical.to_str().expect("utf8 path"),
        root,
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
        roots,
        Some(path.file_name().unwrap().to_str().unwrap().to_string()),
    );
    std::mem::forget(file);
    handle.unwrap_or_else(|e| panic!("open_path {}: {}", path.display(), e.code()))
}

fn select_node(h: &s3a_pack_kite::Handle) -> Option<String> {
    let frame = render_page(h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, true).expect("render_page");
    frame
        .semantics
        .iter()
        .find(|n| n.role == "select")
        .and_then(|n| n.paper_id.clone())
}

const DOSSIER_A: &str = "---\ntitle: First Paper\nsource_id: SRC-0007\n---\n\n# First Paper\n\nBody A.\n";

#[test]
fn at_96_open_of_bytes_has_no_paper_identity() {
    let ctx = test_context();
    let h = open(
        &ctx,
        DOSSIER_A.as_bytes(),
        Some("SRC-0007.md".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
    )
    .expect("open bytes");
    assert_eq!(h.kind(), "wiki");
    // open of bytes never carries a paper identity (AT-96): even with
    // select_enabled, no Select control is painted.
    assert_eq!(select_node(&h), None);
}

#[test]
fn at_101_open_path_dossier_identity() {
    let wiki = dossier_root("wiki");
    let corpus = dossier_root("corpus");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let h = open_fd(&path, &wiki, &ctx, Some(&r));
    assert_eq!(h.kind(), "wiki");
    assert_eq!(h.first_view(), "page");
    assert_eq!(h.paper_identity(), Some("SRC-0007"));
    assert_eq!(select_node(&h), Some("SRC-0007".to_string()));
}

#[test]
fn at_101_open_path_no_identity_without_scholar_roots() {
    let wiki = dossier_root("wiki2");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let h = open_fd(&path, &wiki, &ctx, None);
    assert_eq!(h.paper_identity(), None);
    assert_eq!(select_node(&h), None);
}

#[test]
fn at_101_open_path_no_identity_when_shadowed() {
    let wiki = dossier_root("wiki3");
    let corpus = dossier_root("corpus3");
    let path = write_dossier(&corpus, "papers/SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, true);
    let h = open_fd(&path, &corpus, &ctx, Some(&r));
    assert_eq!(h.paper_identity(), None);
    assert_eq!(select_node(&h), None);
}

#[test]
fn at_101_open_path_no_identity_when_source_id_mismatch() {
    let wiki = dossier_root("wiki4");
    let corpus = dossier_root("corpus4");
    let bad = "---\ntitle: Wrong\nsource_id: SRC-9999\n---\n\n# Wrong\n";
    let path = write_dossier(&wiki, "papers/SRC-0007.md", bad);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let h = open_fd(&path, &wiki, &ctx, Some(&r));
    assert_eq!(h.paper_identity(), None);
    assert_eq!(select_node(&h), None);
}

#[test]
fn at_101_open_path_no_identity_when_front_matter_unparseable() {
    let wiki = dossier_root("wiki5");
    let corpus = dossier_root("corpus5");
    let bad = "---\ntitle: *alias\n---\n\n# Aliased\n";
    let path = write_dossier(&wiki, "papers/SRC-0007.md", bad);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let h = open_fd(&path, &wiki, &ctx, Some(&r));
    // The page still opens and renders; only the identity is absent.
    assert_eq!(h.kind(), "wiki");
    assert_eq!(h.paper_identity(), None);
    assert_eq!(select_node(&h), None);
}

#[test]
fn at_101_open_path_no_identity_outside_papers_dir() {
    let wiki = dossier_root("wiki6");
    let corpus = dossier_root("corpus6");
    let path = write_dossier(&wiki, "SRC-0007.md", DOSSIER_A);
    let ctx = test_context();
    let r = roots(&wiki, &corpus, false);
    let h = open_fd(&path, &wiki, &ctx, Some(&r));
    assert_eq!(h.paper_identity(), None);
    assert_eq!(select_node(&h), None);
}

#[test]
fn at_101_open_path_reads_fd_not_name() {
    // AT-101 (SPEC §5.2.7): open a host-validated fd, swap an intermediate
    // path component for a symlink, then call open_path on that fd; the pack
    // must not follow the symlink or reopen the path by name.
    let wiki = dossier_root("wiki7");
    let other = dossier_root("other7");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let other_dossier = "---\ntitle: Other Paper\nsource_id: SRC-9999\n---\n\n# Other Paper\n";
    write_dossier(&other, "papers/SRC-9999.md", other_dossier);
    let file = File::open(&path).expect("open fd");
    let fd: RawFd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let canonical = std::fs::canonicalize(&path).expect("canonicalize");
    // Swap the intermediate `papers` component for a symlink into `other`.
    std::fs::rename(wiki.join("papers"), wiki.join("papers-orig")).expect("rename papers");
    std::os::unix::fs::symlink(other.join("papers"), wiki.join("papers")).expect("symlink");
    let ctx = test_context();
    let r = roots(&wiki, &other, false);
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
    .unwrap_or_else(|e| panic!("open_path after swap: {}", e.code()));
    std::mem::forget(file);
    // The fd still reads dossier A, not the symlink target's dossier B.
    let frame = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    assert!(text_layer(&frame).contains("First Paper"), "page text: {}", text_layer(&frame));
    assert_eq!(h.paper_identity(), Some("SRC-0007"));
    assert_eq!(select_node(&h), Some("SRC-0007".to_string()));
}

#[test]
fn at_101_open_path_file_too_large() {
    let wiki = dossier_root("wiki8");
    let path = write_dossier(&wiki, "papers/SRC-0007.md", DOSSIER_A);
    let file = File::open(&path).expect("open fd");
    let fd: RawFd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let canonical = std::fs::canonicalize(&path).expect("canonicalize");
    let ctx = test_context();
    let err = match open_path(
        &ctx,
        fd,
        canonical.to_str().expect("utf8 path"),
        &wiki,
        MAX_FRAME_PIXELS,
        10,
        vec![],
        None,
        Some("SRC-0007.md".to_string()),
    ) {
        Ok(_) => panic!("oversize must fail"),
        Err(e) => e,
    };
    assert_eq!(err.code(), "file_too_large");
}

#[test]
fn at_101_open_path_bad_fd_is_error() {
    let ctx = test_context();
    let err = match open_path(
        &ctx,
        -1,
        "/tmp/does-not-matter.md",
        Path::new("/tmp"),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
        None,
        Some("x.md".to_string()),
    ) {
        Ok(_) => panic!("bad fd must fail"),
        Err(e) => e,
    };
    assert_eq!(err.code(), "invalid_param");
}

fn paper_record(paper_id: u64, tombstone: bool) -> AcademicPaperRecord {
    let mut r = AcademicPaperRecord::new(paper_id, 1_700_000_000, 0, 0.0, 0.0, 0.0, 0, 2024, 0, 0, 0, 0);
    if tombstone {
        r.mark_tombstone();
    }
    r
}

fn type12_v1_tile(records: &[AcademicPaperRecord]) -> HyperTile {
    let mut payload = Vec::new();
    for r in records {
        payload.extend_from_slice(bytemuck::bytes_of(r));
    }
    HyperTile::Raw(ScholarTileType::ACADEMIC_PAPERS, payload, records.len() as u32)
}

fn type12_v2_tile(json: &[u8]) -> HyperTile {
    let mut blob = ScholarPaperBlobHeader::default();
    blob.sha256_hash = Sha256::digest(json).into();
    blob.total_bytes = json.len() as u32;
    blob.chunk_index = 0;
    blob.total_chunks = 1;
    blob.chunk_bytes = json.len() as u32;
    blob.chunk_crc32 = crc32c::crc32c(json);
    blob.schema_version = 2;
    let mut payload = Vec::new();
    payload.extend_from_slice(bytemuck::bytes_of(&blob));
    payload.extend_from_slice(json);
    let mut h = HyperTileHeader::new(0, ScholarTileType::ACADEMIC_PAPERS);
    h.record_count = 1;
    h.record_layout_version = 2;
    // Raw carries its own header; rebuild with the v2 layout version.
    HyperTile::RawWithHeader(h, payload)
}

#[test]
fn scholar_records_v1_live_and_tombstone() {
    let fx = build_hyper(
        vec![
            HyperTile::Addon(ADDON_EDU),
            type12_v1_tile(&[
                paper_record(111, false),
                paper_record(222, false),
                paper_record(333, true),
            ]),
        ],
        false,
    );
    let ctx = test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open");
    let ids = scholar_paper_records(&h).expect("scholar_paper_records");
    assert_eq!(ids, vec!["111".to_string(), "222".to_string()], "tombstone 333 excluded, got {ids:?}");
}

#[test]
fn scholar_records_v1_mark_withheld() {
    let probe = build_hyper(
        vec![
            HyperTile::Addon(ADDON_EDU),
            type12_v1_tile(&[paper_record(111, false), paper_record(222, false)]),
        ],
        false,
    );
    let tile_id = probe.tile_ids[0];
    let fx = build_hyper(
        vec![
            HyperTile::Addon(ADDON_EDU),
            type12_v1_tile(&[paper_record(111, false), paper_record(222, false)]),
            HyperTile::Marks(vec![MarkRecord::new(
                ScholarTileType::ACADEMIC_PAPERS,
                tile_id,
                1,
                1,
                MARK_CONSENT_WITHDRAWN,
                MARK_SET,
                NOW,
                0,
            )]),
        ],
        false,
    );
    let ctx = test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open");
    let ids = scholar_paper_records(&h).expect("scholar_paper_records");
    assert_eq!(ids, vec!["111".to_string()], "mark-withheld 222 excluded, got {ids:?}");
}

#[test]
fn scholar_records_v2_key() {
    let json = br#"{"key":"SRC-0007","title":"V2 Paper"}"#;
    let fx = build_hyper(vec![HyperTile::Addon(ADDON_EDU), type12_v2_tile(json)], false);
    let ctx = test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open");
    let ids = scholar_paper_records(&h).expect("scholar_paper_records");
    assert_eq!(ids, vec!["SRC-0007".to_string()], "v2 key, got {ids:?}");
}

#[test]
fn scholar_records_quarantined_returns_empty() {
    let mut fx = build_hyper(
        vec![
            HyperTile::Addon(ADDON_EDU),
            type12_v1_tile(&[paper_record(111, false)]),
        ],
        false,
    );
    flip_hyper_payload_byte(&mut fx.bytes, 0, 0);
    let ctx = test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open");
    let rep = s3a_pack_kite::verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined", "flipped tile quarantines the file");
    let recs = scholar_paper_records(&h).expect("scholar_paper_records");
    assert!(recs.is_empty(), "quarantined file contributes no candidates");
}

/// Step 9 fix (AT-118): hit-test on a manifest view with no prior frame
/// is 409 no_frame — it used to return Hit::None silently.
#[test]
fn hit_test_manifest_without_frame_is_no_frame() {
    use s3a_pack_kite::hit_test;
    let fx = build_hyper(
        vec![HyperTile::Manifest("hit-test manifest".into())],
        false,
    );
    let ctx = test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    let err = hit_test(&h, "manifest", None, 10.0, 10.0).unwrap_err();
    assert_eq!(err.code(), "no_frame", "got {}", err.message());
}
