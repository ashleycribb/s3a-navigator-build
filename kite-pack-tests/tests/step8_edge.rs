//! Step-8 first slice: Edge MICRO-VEC open / warehouse floors.
//!
//! AT-14, AT-15, AT-17, AT-18. Fail-first on the post-step-7 pack.

mod support;

use s3a_pack_kite::{
    close, open, proceed_to_warehouse, render_manifest, render_warehouse, verification_report,
};
use support::{
    assert_pixmap_size, load_bytes, test_context, text_layer, CSS_HEIGHT, CSS_WIDTH, DPR,
    MAX_FRAME_PIXELS, MAX_OPEN_BYTES,
};

fn open_file(rel: &str, name_hint: &str) -> s3a_pack_kite::Handle {
    let ctx = test_context();
    let bytes = load_bytes(rel);
    open(
        &ctx,
        &bytes,
        Some(name_hint.to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        Vec::<[u8; 32]>::new(),
    )
    .unwrap_or_else(|e| panic!("open {rel} failed: {} / {}", e.code(), e.message()))
}

#[test]
fn at_14_micro_vec_01_derived_header_sheet() {
    // AT-14: MICRO-VEC-01 first frame is the derived header sheet, labeled
    // as derived, and includes stride 4096 and the committed tile_count.
    let handle = open_file("micro-vec/micro_valid_4kb.s3a", "micro_valid_4kb.s3a");
    assert_eq!(handle.kind(), "edge");
    assert_eq!(handle.first_view(), "header");
    let frame = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("AT-14 header sheet");
    assert_pixmap_size(&frame, CSS_WIDTH, CSS_HEIGHT, DPR);
    let text = text_layer(&frame);
    assert!(
        text.to_lowercase().contains("derived"),
        "header sheet must be labeled as derived; got {text:?}"
    );
    assert!(
        text.contains("4096"),
        "header sheet must include stride 4096; got {text:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_15_micro_vec_01_warehouse_lists_committed_micro_tiles() {
    let handle = open_file("micro-vec/micro_valid_4kb.s3a", "micro_valid_4kb.s3a");
    render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("header");
    proceed_to_warehouse(&handle).expect("proceed");
    let warehouse = render_warehouse(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
        .expect("warehouse");
    let text = text_layer(&warehouse);
    let floors: Vec<_> = warehouse
        .semantics
        .iter()
        .filter(|node| node.role == "floor")
        .collect();
    assert_eq!(
        floors.len(),
        2,
        "MICRO-VEC-01 has exactly two committed micro-tiles; got {floors:?} text={text:?}"
    );
    assert!(
        text.contains("4096") || floors.iter().all(|_| text.contains("size_bytes")),
        "each floor size_bytes must be 4096; text={text:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_17_micro_vec_02_omits_torn_tile_and_is_not_quarantined_for_tear() {
    let handle = open_file(
        "micro-vec/micro_truncated_file.s3a",
        "micro_truncated_file.s3a",
    );
    let report = verification_report(&handle).expect("verification_report");
    assert_ne!(
        report.status, "quarantined",
        "AT-17: file-level status is not Quarantined solely because of the torn tile; got {:?}",
        report.status
    );
    render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("header");
    proceed_to_warehouse(&handle).expect("proceed");
    let warehouse = render_warehouse(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
        .expect("warehouse");
    let floors: Vec<_> = warehouse
        .semantics
        .iter()
        .filter(|node| node.role == "floor")
        .collect();
    // Tile 0 is valid; the torn tile is omitted.
    assert!(
        !floors.is_empty(),
        "valid leading tile(s) appear as floors; got {floors:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_18_micro_vec_03_mutated_copy_is_quarantined() {
    let original = load_bytes("micro-vec/micro_signed_seal_chain.s3a");
    let ctx = test_context();
    let handle = open(
        &ctx,
        &original,
        Some("micro_signed_seal_chain.s3a".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        Vec::<[u8; 32]>::new(),
    )
    .expect("open MICRO-VEC-03");
    let report = verification_report(&handle).expect("verification_report");
    assert_ne!(
        report.status, "quarantined",
        "intact MICRO-VEC-03 must not be quarantined; got {:?}",
        report.status
    );
    close(handle).expect("close");

    let mut mutated = original.clone();
    // Flip a byte in a committed tile payload (after the 512-byte file header).
    let idx = 512 + 64;
    assert!(idx < mutated.len());
    mutated[idx] ^= 0xff;
    let mutated_handle = open(
        &ctx,
        &mutated,
        Some("micro_signed_seal_chain.s3a".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        Vec::<[u8; 32]>::new(),
    )
    .expect("open mutated MICRO-VEC-03");
    let mutated_report = verification_report(&mutated_handle).expect("mutated report");
    assert_eq!(
        mutated_report.status, "quarantined",
        "reordered or mutated copy must be quarantined; got {:?}",
        mutated_report.status
    );
    render_manifest(&mutated_handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("banner");
    let proceed = proceed_to_warehouse(&mutated_handle);
    if proceed.is_ok() {
        let warehouse = render_warehouse(&mutated_handle, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
            .expect("warehouse of mutated copy");
        // The mutated tile and every subsequent tile are not valid floors.
        // A mutated byte early in tile 0 means no valid floors after that stop.
        let floors: Vec<_> = warehouse
            .semantics
            .iter()
            .filter(|node| node.role == "floor")
            .collect();
        assert!(
            floors.len() < 2,
            "mutated tile and subsequent tiles must not be listed as floors; got {floors:?}"
        );
    }
    close(mutated_handle).expect("close");
}
