//! Step 8c — render_page (AT-100, AT-30, AT-33, AT-32, AT-144, AT-148),
//! kind detection (§5.2.5), the §5.2.6 view matrix for page kinds, and
//! F-14 nesting enforcement for render_page.
//!
//! Test-first: written against SPEC v24.4 before render_page exists; the
//! first run is expected to fail (render_page is not yet exported).

mod fixture8b;
mod support;

use fixture8b::{build_hyper, HyperTile};
use s3a_pack_kite::{
    hit_test, open, open_verified, proceed_to_warehouse, render_floor_detail, render_manifest,
    render_page, render_warehouse, verification_report, Hit,
};
use support::{text_layer, CSS_HEIGHT, CSS_WIDTH, DPR, MAX_FRAME_PIXELS, MAX_OPEN_BYTES};

fn open_page(
    ctx: &s3a_pack_kite::KiteContext,
    bytes: &[u8],
    name: &str,
) -> s3a_pack_kite::Handle {
    open(
        ctx,
        bytes,
        Some(name.to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        vec![],
    )
    .unwrap_or_else(|e| panic!("open {name}: {}", e.code()))
}

fn page_text(h: &s3a_pack_kite::Handle, select_enabled: bool) -> String {
    let frame = render_page(h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, select_enabled)
        .expect("render_page");
    text_layer(&frame)
}

#[test]
fn at_100_render_page_exists_and_first_view_page() {
    let ctx = support::test_context();
    let md = b"# Scholar Explorer\n\nA heading and a paragraph.\n";
    let h = open_page(&ctx, md, "index.md");
    assert_eq!(h.kind(), "wiki");
    assert_eq!(h.first_view(), "page");
    let text = page_text(&h, false);
    assert!(text.contains("Scholar Explorer"), "page text: {text}");

    let html = b"<html><body><h1>Static Heading</h1><p>Body.</p></body></html>";
    let hh = open_page(&ctx, html, "page.html");
    assert_eq!(hh.kind(), "html");
    assert_eq!(hh.first_view(), "page");
    let htext = page_text(&hh, false);
    assert!(htext.contains("Static Heading"), "page text: {htext}");
}

#[test]
fn at_30_wiki_dossier_heading_contiguous_and_front_matter_stripped() {
    let ctx = support::test_context();
    let md = b"---\ntitle: Scholar Explorer\nauthor: Ashley Cribb\n---\n\n# Scholar Explorer\n\nThe first paragraph of the dossier.\n";
    let h = open_page(&ctx, md, "SRC-0001.md");
    assert_eq!(h.kind(), "wiki");
    assert_eq!(h.first_view(), "page");
    let frame = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    let text = text_layer(&frame);
    // The heading is a contiguous substring of the text layer.
    assert!(text.contains("Scholar Explorer"), "page text: {text}");
    // The front-matter keys are not page text.
    assert!(!text.contains("title:"), "front matter leaked: {text}");
    assert!(!text.contains("author:"), "front matter leaked: {text}");
    // The heading run itself is exactly the heading text.
    assert!(
        frame.text_runs.iter().any(|r| r.text == "Scholar Explorer"),
        "no exact heading run in {:?}",
        frame.text_runs.iter().map(|r| &r.text).collect::<Vec<_>>()
    );
}

#[test]
fn at_33_html_script_does_not_run() {
    let ctx = support::test_context();
    let html = b"<html><head><script>document.title = \"Scripted Title\";</script></head>\
        <body><h1>Static Heading</h1><p>Body text.</p></body></html>";
    let h = open_page(&ctx, html, "fixture.html");
    assert_eq!(h.kind(), "html");
    let text = page_text(&h, false);
    assert!(text.contains("Static Heading"), "page text: {text}");
    assert!(!text.contains("Scripted Title"), "script ran: {text}");
    assert!(!text.contains("document.title"), "script text leaked: {text}");
    assert!(!text.contains("<script>"), "raw tag leaked: {text}");
}

#[test]
fn at_56_markdown_script_and_remote_image_inert() {
    let ctx = support::test_context();
    let md = b"# Heading\n\n<script>document.title = \"Scripted\";</script>\n\n![alt text](http://example.com/img.png)\n";
    let h = open_page(&ctx, md, "page.md");
    let text = page_text(&h, false);
    assert!(text.contains("Heading"), "page text: {text}");
    assert!(text.contains("alt text"), "page text: {text}");
    assert!(!text.contains("Scripted"), "script ran: {text}");
    assert!(!text.contains("http://example.com/img.png"), "image fetched or leaked: {text}");
}

#[test]
fn at_144_no_select_control_when_select_disabled() {
    let ctx = support::test_context();
    let md = b"# Page\n\n[link text](other.md)\n";
    let h = open_page(&ctx, md, "page.md");
    // Viewer-only mode: select_enabled=false.
    let frame = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    assert!(
        !frame.semantics.iter().any(|n| n.role == "select"),
        "Select node present with select_enabled=false"
    );
    // hit_test at the usual Select location (top-right, x=width-136, y=16,
    // w=120, h=32) returns None, not Select.
    let hit = hit_test(&h, "page", None, 724.0, 32.0).expect("hit_test");
    assert_eq!(hit, Hit::None);
    // select_enabled=true with no paper_identity also paints no Select
    // control (open of bytes yields paper_identity None).
    let frame2 = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, true).expect("render_page");
    assert!(
        !frame2.semantics.iter().any(|n| n.role == "select"),
        "Select node present without paper_identity"
    );
}

#[test]
fn at_144_page_link_hit_and_no_frame() {
    let ctx = support::test_context();
    let md = b"# Page\n\n[link text](other.md)\n";
    let h = open_page(&ctx, md, "page.md");
    // No prior page frame: hit_test on the page view is no_frame.
    let err = hit_test(&h, "page", None, 100.0, 100.0).unwrap_err();
    assert_eq!(err.code(), "no_frame");
    let frame = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    // The link is a semantic node; hitting it returns Link with the raw
    // href (fragment stripped, bytes-opened handle: no resolution, AT-206).
    let link = frame
        .semantics
        .iter()
        .find(|n| n.role == "link")
        .expect("link node");
    assert_eq!(link.href.as_deref(), Some("other.md"));
    let hit = hit_test(
        &h,
        "page",
        None,
        link.x as f64 + link.width as f64 / 2.0,
        link.y as f64 + link.height as f64 / 2.0,
    )
    .expect("hit_test");
    assert_eq!(hit, Hit::Link { href: "other.md".to_string() });
    // A point on empty page space is None.
    assert_eq!(
        hit_test(&h, "page", None, 700.0, 550.0).expect("hit_test"),
        Hit::None
    );
}

#[test]
fn at_148_other_kind_label_exact() {
    let ctx = support::test_context();
    // A PDF (not S3A magic) under a .pdf name.
    let pdf = b"%PDF-1.4\n% fake pdf bytes\n1 0 obj\n";
    let h = open_page(&ctx, pdf, "paper.pdf");
    assert_eq!(h.kind(), "other");
    assert_eq!(h.first_view(), "page");
    let frame = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    assert_eq!(text_layer(&frame), "no text conversion in archive");
    // Extensionless bytes with no hint are also kind other.
    let h2 = open(&ctx, b"just some bytes", None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open extensionless");
    assert_eq!(h2.kind(), "other");
    assert_eq!(h2.first_view(), "page");
    let f2 = render_page(&h2, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    assert_eq!(text_layer(&f2), "no text conversion in archive");
    // A .s3a-named file whose bytes do not begin with S3A_MAGIC is other.
    let h3 = open_page(&ctx, b"not an archive", "archive.s3a");
    assert_eq!(h3.kind(), "other");
    let f3 = render_page(&h3, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).expect("render_page");
    assert_eq!(text_layer(&f3), "no text conversion in archive");
}

#[test]
fn kind_detection_case_insensitive_extensions() {
    let ctx = support::test_context();
    let h = open_page(&ctx, b"# Title\n", "PAGE.MD");
    assert_eq!(h.kind(), "wiki");
    let h2 = open_page(&ctx, b"<h1>T</h1>", "PAGE.HTM");
    assert_eq!(h2.kind(), "html");
    let h3 = open_page(&ctx, b"<h1>T</h1>", "page.HTML");
    assert_eq!(h3.kind(), "html");
}

/// Build one PROVENANCE chunk tile payload: 40-byte prefix (key, chunk
/// index, total chunks, all LE) then the chunk data.
fn provenance_chunk(key: [u8; 32], index: u32, total: u32, data: &[u8]) -> Vec<u8> {
    let mut p = key.to_vec();
    p.extend_from_slice(&index.to_le_bytes());
    p.extend_from_slice(&total.to_le_bytes());
    p.extend_from_slice(data);
    p
}

const PROV_DOC: &[u8] = b"sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\nurl=https://example.com/paper.pdf\ncaptured=2026-11-18T00:00:00Z\n";

/// SHA-256 of a document: the AT-32 blob_key rule (the `sha256=` line inside
/// the document is the captured body's hash, a different value — the
/// document is not its own hash).
fn sha256(data: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(data).into()
}

fn hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn at_32_provenance_reassembly_single_chunk() {
    let key = sha256(PROV_DOC);
    let fx = build_hyper(
        vec![HyperTile::Raw(
            72,
            provenance_chunk(key, 0, 1, PROV_DOC),
            1,
        )],
        false,
    );
    let ctx = support::test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("floor");
    let text = text_layer(&floor);
    assert!(text.contains("chunk 0/1"), "floor: {text}");
    assert!(text.contains(&hex_bytes(&key)), "blob_key hex missing: {text}");
    assert!(
        text.contains("sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "doc line 1: {text}"
    );
    assert!(text.contains("url=https://example.com/paper.pdf"), "doc line 2: {text}");
    assert!(text.contains("captured=2026-11-18T00:00:00Z"), "doc line 3: {text}");
}

#[test]
fn at_32_provenance_reassembly_two_chunks() {
    let key = sha256(PROV_DOC);
    let half = PROV_DOC.len() / 2;
    let fx = build_hyper(
        vec![
            HyperTile::Raw(72, provenance_chunk(key, 0, 2, &PROV_DOC[..half]), 1),
            HyperTile::Raw(72, provenance_chunk(key, 1, 2, &PROV_DOC[half..]), 1),
        ],
        false,
    );
    let ctx = support::test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    proceed_to_warehouse(&h).expect("proceed");
    // Both chunk floors show the reassembled document for the key.
    for floor_index in [0, 1] {
        let floor =
            render_floor_detail(&h, floor_index, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("floor");
        let text = text_layer(&floor);
        assert!(
            text.contains("sha256=aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
            "floor {floor_index}: {text}"
        );
        assert!(text.contains("url=https://example.com/paper.pdf"), "floor {floor_index}: {text}");
        assert!(text.contains("captured=2026-11-18T00:00:00Z"), "floor {floor_index}: {text}");
    }
}

#[test]
fn at_107_provenance_without_prefix_still_shows_text() {
    // Regression guard: a PROVENANCE tile with no chunk prefix (the 8b
    // AT-107 shape) still shows its document text.
    let fx = build_hyper(
        vec![HyperTile::Provenance(
            "sha256=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\nurl=https://example.com/other.pdf\ncaptured=2026-11-18T00:00:00Z\n".to_string(),
        )],
        false,
    );
    let ctx = support::test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("floor");
    let text = text_layer(&floor);
    assert!(text.contains("url=https://example.com/other.pdf"), "floor: {text}");
}

#[test]
fn blob_chunk_metadata_only_when_credible() {
    // A genuine two-chunk BLOB (key = SHA-256 of the content) shows its
    // chunk index/total and key as metadata on both floors — never the
    // chunk bytes.
    let content = b"blob-content-for-chunk-test-0123456789abcdef";
    let key = sha256(content);
    let half = content.len() / 2;
    let fx = build_hyper(
        vec![
            HyperTile::Raw(68, provenance_chunk(key, 0, 2, &content[..half]), 1),
            HyperTile::Raw(68, provenance_chunk(key, 1, 2, &content[half..]), 1),
        ],
        false,
    );
    let ctx = support::test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    proceed_to_warehouse(&h).expect("proceed");
    for (floor_index, want) in [(0, "chunk 0/2"), (1, "chunk 1/2")] {
        let floor =
            render_floor_detail(&h, floor_index, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("floor");
        let text = text_layer(&floor);
        assert!(text.contains(want), "floor {floor_index}: {text}");
        assert!(text.contains(&hex_bytes(&key)), "floor {floor_index}: {text}");
        assert!(!text.contains("blob-content-for-chunk"), "chunk bytes leaked: {text}");
    }
}

#[test]
fn blob_chunk_prefix_without_matching_hash_is_not_a_chunk() {
    // A 40+ byte BLOB payload whose bytes 32..40 form a plausible
    // index/total — but whose content does not hash to the key — is not a
    // chunk: no chunk line, metadata only.
    let mut payload = [0x42u8; 32].to_vec();
    payload.extend_from_slice(&0u32.to_le_bytes());
    payload.extend_from_slice(&1u32.to_le_bytes());
    payload.extend_from_slice(b"12345678");
    let fx = build_hyper(vec![HyperTile::Raw(68, payload, 1)], false);
    let ctx = support::test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("floor");
    let text = text_layer(&floor);
    assert!(text.contains("type 68"), "floor: {text}");
    assert!(!text.contains("chunk "), "spurious chunk line: {text}");
}

#[test]
fn view_matrix_page_kinds() {
    let ctx = support::test_context();
    let h = open_page(&ctx, b"# Page\n", "page.md");
    // Manifest on a page kind is view_mismatch; warehouse and floor views
    // are not_archive; verification_report and proceed are not_archive.
    assert_eq!(
        render_manifest(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
            .unwrap_err()
            .code(),
        "view_mismatch"
    );
    assert_eq!(
        render_warehouse(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
            .unwrap_err()
            .code(),
        "not_archive"
    );
    assert_eq!(
        render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
            .unwrap_err()
            .code(),
        "not_archive"
    );
    assert_eq!(
        verification_report(&h).unwrap_err().code(),
        "not_archive"
    );
    assert_eq!(
        proceed_to_warehouse(&h).unwrap_err().code(),
        "not_archive"
    );
    // render_page on an archive kind is view_mismatch.
    let fx = build_hyper(vec![HyperTile::Manifest("m".into())], false);
    let ha = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open hyper");
    assert_eq!(
        render_page(&ha, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false)
            .unwrap_err()
            .code(),
        "view_mismatch"
    );
    // hit_test page on an archive kind is view_mismatch.
    assert_eq!(
        hit_test(&ha, "page", None, 100.0, 100.0).unwrap_err().code(),
        "view_mismatch"
    );
    // An unknown view is invalid_view.
    assert_eq!(
        hit_test(&h, "bogus", None, 100.0, 100.0).unwrap_err().code(),
        "invalid_view"
    );
}

#[test]
fn open_verified_on_page_bytes_is_invalid_report() {
    // A real report from a hyper-tile archive, supplied for wiki bytes:
    // the bytes are not an archive kind, so invalid_report (AT-211).
    let ctx = support::test_context();
    let fx = build_hyper(vec![HyperTile::Manifest("m".into())], false);
    let ha = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open hyper");
    let report = verification_report(&ha).expect("report");
    let err = match open_verified(
        &ctx,
        b"# Page\n",
        Some("page.md".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        report,
    ) {
        Ok(_) => panic!("open_verified of wiki bytes unexpectedly succeeded"),
        Err(e) => e,
    };
    assert_eq!(err.code(), "invalid_report");
}

#[test]
fn f14_nesting_markdown_seed_is_page_error() {
    let path = support::repo_root().join("../fuzz/corpus/open_and_render/deeply_nested_markdown.md");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let ctx = support::test_context();
    let h = open_page(&ctx, &bytes, "page.md");
    let err = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).unwrap_err();
    assert_eq!(err.code(), "nesting_too_deep");
}

#[test]
fn f14_nesting_html_seed_is_page_error() {
    let path = support::repo_root().join("../fuzz/corpus/open_and_render/deeply_nested_html.html");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let ctx = support::test_context();
    let h = open_page(&ctx, &bytes, "page.html");
    let err = render_page(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR, false).unwrap_err();
    assert_eq!(err.code(), "nesting_too_deep");
}

#[test]
fn f14_moderate_nesting_renders() {
    let ctx = support::test_context();
    // Moderate Markdown nesting (3-level blockquote) renders.
    let md = b"> > > deeply quoted\n\nplain paragraph\n";
    let h = open_page(&ctx, md, "page.md");
    let text = page_text(&h, false);
    assert!(text.contains("deeply quoted"), "page text: {text}");
    assert!(text.contains("plain paragraph"), "page text: {text}");
    // Moderate HTML nesting (3 divs) renders.
    let html = b"<div><div><div>nested</div></div></div>";
    let hh = open_page(&ctx, html, "page.html");
    let htext = page_text(&hh, false);
    assert!(htext.contains("nested"), "page text: {htext}");
}
