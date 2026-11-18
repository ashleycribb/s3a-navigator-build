//! Step 8b — hyper-tile withheld count (SPEC §7, §5.3 item 4).
//! AT-11, AT-12, AT-107, AT-135, AT-136, AT-176, AT-184, AT-185, AT-200,
//! AT-204, AT-205.
//!
//! The withheld unique string is the subject_id hex (32 lowercase hex
//! chars) as rendered by the pack (`consent subject_id <hex> ...`). Absence
//! is asserted in text runs, semantic labels, hit-test targets, and the
//! §5.8 pixel oracle (8b form: the pixmap paints no text, so it is all
//! zeros and byte-identical to any control pixmap; see fixture8b).

mod fixture8b;
mod support;

use s3a_core::consent::ConsentPurpose;
use s3a_core::{MarkRecord, MARK_CONSENT_WITHDRAWN, MARK_SET};
use s3a_pack_kite::{
    hit_test, open, proceed_to_warehouse, render_floor_detail, render_floor_transition,
    render_manifest, render_warehouse, verification_report, Hit,
};

use fixture8b::*;
use support::*;

const NOW: u64 = 1_700_000_000;

fn open_bytes(ctx: &s3a_pack_kite::KiteContext, bytes: &[u8]) -> s3a_pack_kite::Handle {
    open(ctx, bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open")
}

fn manifest_text(h: &s3a_pack_kite::Handle) -> String {
    text_layer(&render_manifest(h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("render manifest"))
}

/// AT-11: withdrawal withholds one subject; the other stays shown, and the
/// withheld subject_id hex is absent from the manifest, the warehouse, the
/// consent floor, hit-test targets, and the transition frames.
#[test]
fn at11_withdrawal_withholds_subject() {
    let x = subject(0x11);
    let y = subject(0x22);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at11 manifest".into()),
            HyperTile::Consent(vec![
                grant(x, NOW, 0),
                grant(y, NOW, 0),
                withdrawal(x, NOW + 100),
            ]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let rep = verification_report(&h).expect("report");
    // A sealed-OK hyper-tile has no device signer: Unsigned, seal sealed
    // (AT-154), not Verified.
    assert_eq!(rep.status, "unsigned", "sealed hyper-tile is unsigned");
    assert_eq!(rep.seal_status, "sealed");
    // The count is per record (SPEC §5.3 item 4): X's grant and X's
    // withdrawal are both withheld records.
    assert!(manifest_text(&h).contains("withheld_count: 2"));

    proceed_to_warehouse(&h).expect("proceed");
    let wh = render_warehouse(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR).expect("warehouse");
    // The warehouse lists floor directory lines only; a withheld subject's
    // hex must not appear anywhere in it.
    assert_absent_from_frame(&wh, &hex(&x));

    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert!(text_layer(&floor).contains("withheld_count: 2"));
    assert_absent_from_frame(&floor, &hex(&x));
    assert_present_in_frame(&floor, &hex(&y));

    // Hit-test targets: floor-detail frames carry no targets.
    assert_eq!(
        hit_test(&h, "floor-detail", Some(1), 100.0, 100.0).expect("hit test"),
        Hit::None
    );
    // Transition frames show the same floor-detail text layer.
    for t in [0.0, 0.225, 0.45] {
        let fr = render_floor_transition(&h, 1, t, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, false, DPR)
            .expect("transition frame");
        assert_absent_from_frame(&fr, &hex(&x));
        assert_present_in_frame(&fr, &hex(&y));
    }
}

/// AT-11: a type-82 tile holding consent-shaped bytes is NOT consent
/// (consent.rs: type 82 is never consent). The file is unrestricted,
/// withheld_count is 0, and the type-82 payload is metadata only.
#[test]
fn at11_type82_is_not_consent() {
    let z = subject(0x33);
    let rec = grant(z, NOW, 0);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at11 type82".into()),
            HyperTile::Raw(82, bytemuck::bytes_of(&rec).to_vec(), 1),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 0"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("type-82 floor");
    assert!(text_layer(&floor).contains("withheld_count: 0"));
    assert_absent_from_frame(&floor, &hex(&z));
    // Metadata only: the floor shows the type/size/hash line, never the payload.
    assert!(text_layer(&floor).contains("type 82"));
}

/// AT-11: a MARK_CONSENT_WITHDRAWN on an otherwise unrestricted file
/// withholds only the marked record (DD-111: marks from parseable tiles
/// can restrict).
#[test]
fn at11_mark_only_withholds_marked_record() {
    let x = subject(0x44);
    let y = subject(0x55);
    // Build once to learn the consent tile_id (writer: tile_id = index + 1).
    let probe = build_hyper(
        vec![
            HyperTile::Manifest("probe".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0), grant(y, NOW, 0)]),
        ],
        false,
    );
    assert_eq!(probe.tile_ids[1], 2, "consent tile_id is index + 1");
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at11 mark".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0), grant(y, NOW, 0)]),
            HyperTile::Marks(vec![MarkRecord::new(
                71,
                probe.tile_ids[1],
                0,
                1,
                MARK_CONSENT_WITHDRAWN,
                MARK_SET,
                NOW,
                0,
            )]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 1"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert!(text_layer(&floor).contains("withheld_count: 1"));
    assert_absent_from_frame(&floor, &hex(&x));
    assert_present_in_frame(&floor, &hex(&y));
}

/// AT-12: script / angle-bracket text in the manifest is painted as text
/// (no JS execution surface): the literal text appears in text runs, and
/// no semantic node carries an href or a script role.
#[test]
fn at12_script_text_is_painted_as_text() {
    let fx = build_hyper(
        vec![HyperTile::Manifest(
            "at12 <script>alert(1)</script> <b>bold</b> & \"quotes\"".into(),
        )],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let frame = render_manifest(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("manifest");
    let text = text_layer(&frame);
    assert!(text.contains("<script>alert(1)</script>"), "script text: {text}");
    assert!(text.contains("<b>bold</b>"), "angle text: {text}");
    for node in &frame.semantics {
        assert!(node.href.is_none(), "no href on node {:?}", node.label);
        assert_ne!(node.role, "script", "no script role");
    }
}

/// AT-107: a BLOB+PROVENANCE snapshot is unrestricted and metadata only:
/// the captured body (itself an S3A archive) is never shown, and the
/// provenance document text is shown. The inner archive is split across
/// two BLOB chunk tiles: one hyper tile cannot hold a whole archive
/// (a sealed 1-tile archive is ~131,856 bytes; hyper-tile capacity is
/// 130,560).
#[test]
fn at107_blob_provenance_snapshot_unrestricted() {
    // Inner archive whose manifest text must never leak into the outer view.
    let inner = build_hyper(
        vec![HyperTile::Manifest("inner-secret-manifest".into())],
        true,
    );
    let (chunk0, chunk1) = inner.bytes.split_at(inner.bytes.len() / 2);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at107 outer".into()),
            HyperTile::Raw(68, chunk0.to_vec(), 1),
            HyperTile::Raw(68, chunk1.to_vec(), 1),
            HyperTile::Provenance("provenance-doc-line-1\nprovenance-doc-line-2".into()),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let m = manifest_text(&h);
    assert!(m.contains("withheld_count: 0"), "outer unrestricted: {m}");
    assert!(!m.contains("inner-secret-manifest"));

    proceed_to_warehouse(&h).expect("proceed");
    for blob_floor in [1, 2] {
        let blob = render_floor_detail(&h, blob_floor, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
            .expect("blob floor");
        assert_absent_from_frame(&blob, "inner-secret-manifest");
        assert!(text_layer(&blob).contains("type 68"));
    }
    let prov = render_floor_detail(&h, 3, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("provenance floor");
    assert!(text_layer(&prov).contains("provenance-doc-line-1"));
    assert_absent_from_frame(&prov, "inner-secret-manifest");
}

/// AT-135: only non-research consent records -> the file is evaluated, not
/// unrestricted: the subject has no research grant, so it is withheld.
#[test]
fn at135_non_research_consent_is_evaluated() {
    let x = subject(0x66);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at135".into()),
            HyperTile::Consent(vec![grant_purpose(
                x,
                ConsentPurpose::Analytics,
                NOW,
                0,
            )]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 1"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert!(text_layer(&floor).contains("withheld_count: 1"));
    assert_absent_from_frame(&floor, &hex(&x));
}

/// AT-136: a later withdrawal (later tile) withholds a subject granted
/// earlier; the other subject stays shown.
#[test]
fn at136_later_withdrawal_withholds() {
    let x = subject(0x77);
    let y = subject(0x88);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at136".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0), grant(y, NOW, 0)]),
            HyperTile::Consent(vec![withdrawal(x, NOW + 100)]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    // Per-record count: X's grant (tile 1) and X's withdrawal (tile 2).
    assert!(manifest_text(&h).contains("withheld_count: 2"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("grant floor");
    assert!(text_layer(&floor).contains("withheld_count: 1"));
    assert_absent_from_frame(&floor, &hex(&x));
    assert_present_in_frame(&floor, &hex(&y));
    let floor2 = render_floor_detail(&h, 2, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("withdrawal floor");
    assert!(text_layer(&floor2).contains("withheld_count: 1"));
    assert_absent_from_frame(&floor2, &hex(&x));
}

/// AT-176: a grant on a CRC-failed tile can only restrict — an untrusted
/// grant never releases, and a later untrusted withdrawal still restricts.
/// The CRC flip targets offset 80 (inside the first record's
/// `prev_consent_hash`), so it breaks the tile CRC without changing any
/// subject_id or decision field.
#[test]
fn at176_crc_failed_grant_can_only_restrict() {
    let x = subject(0x99);
    let y = subject(0xaa);
    // (a) Withdrawal on a valid tile + grant on a CRC-failed tile: the
    // untrusted grant cannot release X, so X stays withheld (count 1).
    let mut fx = build_hyper(
        vec![
            HyperTile::Manifest("at176a".into()),
            HyperTile::Consent(vec![withdrawal(x, NOW + 100)]),
            HyperTile::Consent(vec![grant(x, NOW, 0)]),
        ],
        false,
    );
    flip_hyper_payload_byte(&mut fx.bytes, 2, 80);
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let rep = verification_report(&h).expect("report");
    assert!(rep.tiles_crc_failed.contains(&2), "tile 2 must fail CRC");
    assert!(manifest_text(&h).contains("withheld_count: 1"));

    // (b) Trusted grant + later untrusted withdrawal: the withdrawal on the
    // CRC-failed tile still restricts Y.
    let mut fx2 = build_hyper(
        vec![
            HyperTile::Manifest("at176b".into()),
            HyperTile::Consent(vec![grant(y, NOW, 0)]),
            HyperTile::Consent(vec![withdrawal(y, NOW + 100)]),
        ],
        false,
    );
    flip_hyper_payload_byte(&mut fx2.bytes, 2, 80);
    let h2 = open_bytes(&ctx, &fx2.bytes);
    assert!(manifest_text(&h2).contains("withheld_count: 1"));
    proceed_to_warehouse(&h2).expect("proceed");
    let floor = render_floor_detail(&h2, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("grant floor");
    assert_absent_from_frame(&floor, &hex(&y));
}

/// AT-184: a withdrawal on an invalid (CRC-failed) tile withholds X and Y:
/// parseable invalid-tile records restrict (DD-111). The failed tile's own
/// records are not counted (valid floors only), so the file count is 2.
#[test]
fn at184_invalid_tile_withdrawal_withholds() {
    let x = subject(0xbb);
    let y = subject(0xcc);
    let mut fx = build_hyper(
        vec![
            HyperTile::Manifest("at184".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0), grant(y, NOW, 0)]),
            HyperTile::Consent(vec![withdrawal(x, NOW + 100), withdrawal(y, NOW + 100)]),
        ],
        false,
    );
    flip_hyper_payload_byte(&mut fx.bytes, 2, 80);
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 2"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("grant floor");
    assert!(text_layer(&floor).contains("withheld_count: 2"));
    assert_absent_from_frame(&floor, &hex(&x));
    assert_absent_from_frame(&floor, &hex(&y));
}

/// AT-185: seal tamper (endcap index_hash flipped, magic kept) -> the file
/// is seal_failed, unauthenticated as a whole, and the grants stay
/// untrusted: X and Y stay withheld.
#[test]
fn at185_seal_tamper_grants_untrusted() {
    let x = subject(0xdd);
    let y = subject(0xee);
    let mut fx = build_hyper(
        vec![
            HyperTile::Manifest("at185".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0), grant(y, NOW, 0)]),
        ],
        true,
    );
    tamper_seal_trailer(&mut fx.bytes);
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.seal_status, "seal_failed", "tampered seal must fail");
    assert!(rep.quarantine_reason.is_some(), "seal_failed quarantines");
    assert!(manifest_text(&h).contains("withheld_count: 2"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert_absent_from_frame(&floor, &hex(&x));
    assert_absent_from_frame(&floor, &hex(&y));
}

/// AT-200: the same grant-after-withdrawal releases X in the unsealed
/// control, but stays withheld when the seal is tampered (AT-185 shape).
#[test]
fn at200_unsealed_control_shows_later_grant() {
    let x = subject(0x12);
    let tiles = vec![
        HyperTile::Manifest("at200".into()),
        HyperTile::Consent(vec![
            grant(x, NOW, 0),
            withdrawal(x, NOW + 100),
            grant(x, NOW + 200, 0),
        ]),
    ];
    // Control: unsealed -> the later grant is trusted and releases X.
    let fx_control = build_hyper(tiles, false);
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx_control.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 0"));
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert_present_in_frame(&floor, &hex(&x));

    // Tampered seal -> unauthenticated as a whole -> all three X records
    // (grant, withdrawal, later grant) stay withheld.
    let mut fx = build_hyper(
        vec![
            HyperTile::Manifest("at200".into()),
            HyperTile::Consent(vec![
                grant(x, NOW, 0),
                withdrawal(x, NOW + 100),
                grant(x, NOW + 200, 0),
            ]),
        ],
        true,
    );
    tamper_seal_trailer(&mut fx.bytes);
    let h2 = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h2).contains("withheld_count: 3"));
}

/// AT-204: a withdrawal-only file withholds X and Y (no grant exists, so
/// there is no trusted grant to release them).
#[test]
fn at204_withdrawal_only_withholds() {
    let x = subject(0x34);
    let y = subject(0x56);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at204".into()),
            HyperTile::Consent(vec![withdrawal(x, NOW + 100), withdrawal(y, NOW + 100)]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 2"));

    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert_absent_from_frame(&floor, &hex(&x));
    assert_absent_from_frame(&floor, &hex(&y));
}

/// AT-205: an injected clock drives expiry, and re-evaluation after the
/// clock advances withholds the expired subject without re-opening.
#[test]
fn at205_injected_clock_expiry_reevaluates() {
    let x = subject(0x78);
    let y = subject(0x9a);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at205".into()),
            HyperTile::Consent(vec![
                grant(x, NOW, NOW + 1000),
                grant(y, NOW, 0),
            ]),
        ],
        true,
    );
    let (ctx, clock) = test_context_with_clock(NOW as i64 + 500);
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 0"));
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor");
    assert_present_in_frame(&floor, &hex(&x));
    assert_present_in_frame(&floor, &hex(&y));

    // Advance past X's expiry: the same handle re-evaluates on render.
    clock.advance(600);
    let m2 = manifest_text(&h);
    assert!(m2.contains("withheld_count: 1"), "after expiry: {m2}");
    let floor2 = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor after expiry");
    assert_absent_from_frame(&floor2, &hex(&x));
    assert_present_in_frame(&floor2, &hex(&y));
}
