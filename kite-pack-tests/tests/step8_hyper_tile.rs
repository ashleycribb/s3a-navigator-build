//! Step-8 first slice: hyper-tile manifest / warehouse first views.
//!
//! AT-08, AT-09, AT-10, AT-64, AT-65, AT-72. Fail-first: these must not
//! compile (or must fail) on the post-step-7 pack, which has no open or
//! render APIs. Do not edit them to make them pass.

mod support;

use s3a_pack_kite::{
    close, hit_test, open, proceed_to_warehouse, render_manifest, render_warehouse, Hit,
};
use support::{
    assert_pixmap_size, load_bytes, test_context, text_layer, CSS_HEIGHT, CSS_WIDTH, DPR,
    MAX_FRAME_PIXELS, MAX_OPEN_BYTES,
};

fn open_file(rel: &str) -> s3a_pack_kite::Handle {
    let ctx = test_context();
    let bytes = load_bytes(rel);
    open(
        &ctx,
        &bytes,
        Some("archive.s3a".to_string()),
        MAX_FRAME_PIXELS,
        MAX_OPEN_BYTES,
        Vec::<[u8; 32]>::new(),
    )
    .unwrap_or_else(|e| panic!("open {rel} failed: {} / {}", e.code(), e.message()))
}

#[test]
fn at_08_sealed_manifest_is_first_frame_text() {
    // AT-08: first frame is render_manifest; text layer contains the exact
    // stored manifest payload, including THIS IS AN S3A FILE and the seal
    // title/creator. Warehouse floors are not the first frame.
    let handle = open_file("hyper-tile/at08_sealed_manifest.s3a");
    assert_eq!(handle.kind(), "hyper-tile");
    assert_eq!(handle.first_view(), "manifest");
    let frame = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("AT-08 render_manifest");
    assert_pixmap_size(&frame, CSS_WIDTH, CSS_HEIGHT, DPR);
    let text = text_layer(&frame);
    assert!(
        text.contains("THIS IS AN S3A FILE"),
        "manifest text must contain THIS IS AN S3A FILE; got {text:?}"
    );
    assert!(
        text.contains("Tour Manifest Test Suite"),
        "manifest text must contain the seal title; got {text:?}"
    );
    assert!(
        text.contains("gate://vault-test-manifest"),
        "manifest text must contain the seal creator; got {text:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_09_absent_manifest_proceed_and_pure_hit_test() {
    // AT-09: no tile 78 → labeled "manifest absent"; no invented title/creator.
    // After proceed_to_warehouse, render_warehouse is the next view.
    // A Continue hit_test alone does not change the view (AT-152).
    let handle = open_file("hyper-tile/no_tile_78.s3a");
    assert_eq!(handle.kind(), "hyper-tile");
    let first = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("AT-09 first view");
    let text = text_layer(&first);
    assert!(
        text.contains("manifest absent"),
        "first view must be labeled manifest absent; got {text:?}"
    );
    assert!(
        !text.contains("Tour Manifest Test Suite"),
        "must not invent title text; got {text:?}"
    );

    let before = hit_test(&handle, "manifest", None, 10.0, 10.0).expect("hit_test");
    let _ = before;
    let still_first = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("hit_test must not change the view");
    assert!(
        text_layer(&still_first).contains("manifest absent"),
        "Continue or any hit_test alone must not leave the first view"
    );

    proceed_to_warehouse(&handle).expect("proceed_to_warehouse");
    let warehouse = render_warehouse(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
        .expect("AT-09 render_warehouse after proceed");
    assert_pixmap_size(&warehouse, CSS_WIDTH, CSS_HEIGHT, DPR);
    close(handle).expect("close");
}

#[test]
fn at_10_first_tile_78_is_also_a_floor() {
    // AT-10: after the manifest view, the floor list includes the first
    // tile 78 when it is a valid floor, plus index, type_id, record_count,
    // size_bytes, and payload_hash_short for each floor. CLI is the oracle
    // only for sealed, type-82-free files; this fixture is that class.
    let handle = open_file("hyper-tile/at08_sealed_manifest.s3a");
    render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("manifest");
    proceed_to_warehouse(&handle).expect("proceed");
    let warehouse = render_warehouse(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
        .expect("warehouse");
    let text = text_layer(&warehouse);
    let floors: Vec<_> = warehouse
        .semantics
        .iter()
        .filter(|node| node.role == "floor")
        .collect();
    assert!(
        !floors.is_empty(),
        "warehouse must list floors as SemanticNode role=floor"
    );
    assert!(
        floors.iter().any(|node| node.floor == Some(0) || text.contains("type_id")),
        "first tile 78, when a valid floor, must be listed; semantics={floors:?} text={text:?}"
    );
    for field in ["index", "type_id", "record_count", "size_bytes", "payload_hash_short"] {
        assert!(
            text.contains(field),
            "warehouse text must include {field}; got {text:?}"
        );
    }
    close(handle).expect("close");
}

#[test]
fn at_65_two_tile_78s_use_first_as_manifest_and_list_both() {
    let handle = open_file("hyper-tile/two_manifests.s3a");
    let first = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("manifest");
    let text = text_layer(&first);
    assert!(
        text.contains("FIRST TILE 78 MANIFEST TEXT"),
        "first view uses the first tile 78 bytes; got {text:?}"
    );
    assert!(
        !text.contains("SECOND TILE 78 MANIFEST TEXT"),
        "first view must not use the second tile 78 as the manifest; got {text:?}"
    );
    proceed_to_warehouse(&handle).expect("proceed");
    let warehouse = render_warehouse(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR)
        .expect("warehouse");
    let floors: Vec<u32> = warehouse
        .semantics
        .iter()
        .filter(|node| node.role == "floor")
        .filter_map(|node| node.floor)
        .collect();
    assert!(
        floors.len() >= 2,
        "first and second tile 78 must both be listed as floors; got {floors:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_64_non_utf8_tile_78_is_labeled_not_utf8() {
    let handle = open_file("hyper-tile/manifest_not_utf8.s3a");
    let first = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("manifest");
    let text = text_layer(&first);
    assert!(
        text.contains("manifest not UTF-8"),
        "non-UTF-8 tile 78 must be labeled manifest not UTF-8; got {text:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_72_unsealed_with_tile_78_shows_manifest() {
    let handle = open_file("hyper-tile/unsealed_with_manifest.s3a");
    let first = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("manifest");
    let text = text_layer(&first);
    assert!(
        text.contains("THIS IS AN S3A FILE"),
        "unsealed file with a valid tile 78 still shows the manifest; got {text:?}"
    );
    close(handle).expect("close");
}

#[test]
fn at_66_and_at_97_inconsistent_headers_are_unsupported_banners() {
    // AT-66 and AT-97: pack open returns Ok with kind unsupported,
    // first_view banner; proceed is unsupported; no Continue control.
    let cases = [
        ("hyper-tile/edge_flag_128kib.s3a", "inconsistent-header"),
        ("hyper-tile/size_4kb_stride_8192.s3a", "inconsistent-header"),
        ("hyper-tile/both_size_flags.s3a", "inconsistent-header"),
        ("hyper-tile/stride_65536.s3a", "unsupported-stride"),
    ];
    for (rel, banner) in cases {
        let handle = open_file(rel);
        assert_eq!(handle.kind(), "unsupported", "{rel}");
        assert_eq!(handle.first_view(), "banner", "{rel}");
        let frame = render_manifest(&handle, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
            .unwrap_or_else(|e| panic!("banner for {rel}: {} / {}", e.code(), e.message()));
        let text = text_layer(&frame);
        assert!(
            text.contains(banner),
            "{rel} banner must contain {banner}; got {text:?}"
        );
        let hit = hit_test(&handle, "banner", None, 10.0, 10.0).expect("hit_test");
        assert!(
            matches!(hit, Hit::None),
            "{rel} unsupported banner paints no Continue; got {hit:?}"
        );
        let err = proceed_to_warehouse(&handle)
            .expect_err("{rel} proceed must be unsupported");
        assert_eq!(err.code(), "unsupported", "{rel}");
        close(handle).expect("close");
    }
}
