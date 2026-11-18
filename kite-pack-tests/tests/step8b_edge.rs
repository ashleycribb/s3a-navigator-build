//! Step 8b — Edge pinned keys and withholding (SPEC §7, DD-145, DD-150).
//! AT-23, AT-35 (Edge), AT-67, AT-68, AT-85, AT-86, AT-134, AT-154 (Edge),
//! AT-155, AT-177, AT-194, AT-195, AT-201, AT-210, AT-220, AT-232.
//!
//! The withheld unique string for an event-log record is its operator id
//! hex (16 lowercase hex chars) as rendered by the pack
//! (`event ... operator <hex>`). Identity floors are exempt from
//! withholding (DD-53) and stay visible under denial.

mod fixture8b;
mod support;

use s3a_edge::{SigningKey, PURPOSE_RESEARCH_ANALYTICS};
use s3a_pack_kite::{
    hit_test, open, proceed_to_warehouse, render_floor_detail, render_manifest,
    render_warehouse, verification_report, Hit,
};

use fixture8b::*;
use support::*;

const NOW: u64 = 1_700_000_000;

fn open_edge(
    ctx: &s3a_pack_kite::KiteContext,
    bytes: &[u8],
    pins: Vec<[u8; 32]>,
) -> s3a_pack_kite::Handle {
    open(ctx, bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, pins).expect("open")
}

fn header_text(h: &s3a_pack_kite::Handle) -> String {
    text_layer(&render_manifest(h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).expect("header sheet"))
}

/// A signed file: identity(0) + `data` tiles, each followed by a seal
/// container for it. Returns (bytes, public_key). The signed flag is set.
fn signed_file<F: FnOnce(&mut EdgeBuilder)>(f: F) -> (Vec<u8>, [u8; 32]) {
    let mut b = EdgeBuilder::new();
    f(&mut b);
    let pk = b.public_key();
    (b.finish_with_signed_flag(), pk)
}

/// AT-23: Edge denial — per-file and per-floor withheld counts, and the
/// derived header sheet + identity floors stay visible under denial.
#[test]
fn at23_edge_denial_counts_and_visible_identity() {
    let op1 = operator(0x11);
    let op2 = operator(0x22);
    let (bytes, pk) = signed_file(|b| {
        let ev = b.append_events(vec![
            event(NOW, 0x0001, 2, 0x11),
            event(NOW + 10, 0x0004, 5, 0x22),
        ]);
        let cons = b.append_consent(edge_consent(0, NOW, 0));
        b.append_seal_for(0);
        b.append_seal_for(ev);
        b.append_seal_for(cons);
    });
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "verified", "signed fixture must verify");
    assert_eq!(rep.seal_status, "sealed");
    let head = header_text(&h);
    assert!(head.contains("withheld_count: 2"), "header: {head}");
    assert!(head.contains("device_id 11111111111111111111111111111111"), "header: {head}");
    assert!(head.contains("hardware_model test-model"), "header: {head}");
    assert!(!head.contains(&hex(&op1)), "denied operator must not leak via header");

    proceed_to_warehouse(&h).expect("proceed");
    let ev_floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("event floor");
    assert!(text_layer(&ev_floor).contains("withheld_count: 2"));
    assert_absent_from_frame(&ev_floor, &hex(&op1));
    assert_absent_from_frame(&ev_floor, &hex(&op2));
    // Identity floor stays visible under denial (DD-53 exemption).
    let id_floor = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("identity floor");
    assert_present_in_frame(&id_floor, "11111111111111111111111111111111");
    assert_present_in_frame(&id_floor, "test-model");
    // Hit-test targets: event floor carries no targets under denial.
    assert_eq!(
        hit_test(&h, "floor-detail", Some(1), 100.0, 100.0).expect("hit test"),
        Hit::None
    );
}

/// AT-35 (Edge): a committed CRC failure quarantines the file; floors end
/// at the failed tile; seal_status is seal_failed on an active chain. The
/// reason is chain_break, not crc: the CRC failure truncates the seal
/// chain, so the seal record for the failed tile has hash_ok=false, and
/// chain_break outranks crc in the DD-150 priority (logged in the step-8b
/// log as the literal DD-150 reading).
#[test]
fn at35_edge_crc_quarantines() {
    let (mut bytes, pk) = signed_file(|b| {
        let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        let ev2 = b.append_events(vec![event(NOW + 10, 0x0002, 1, 0x22)]);
        b.append_seal_for(0);
        b.append_seal_for(ev);
        b.append_seal_for(ev2);
    });
    flip_edge_payload_byte(&mut bytes, 1, 0);
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(rep.seal_status, "seal_failed");
    assert_eq!(rep.tiles_crc_failed, vec![1]);
    assert_eq!(rep.break_index, Some(1));
    assert!(rep.invalid_tile_indices.contains(&1));
    assert!(rep.invalid_tile_indices.contains(&2));
    // The identity floor (before the break) is still a valid floor.
    proceed_to_warehouse(&h).expect("proceed");
    let id_floor = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("identity floor");
    assert_present_in_frame(&id_floor, "test-model");
    // The CRC-failed floor is not a valid floor.
    assert!(render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).is_err());
    let head = header_text(&h);
    assert!(head.contains("quarantine: chain_break"), "header: {head}");
    assert!(head.contains("invalid tiles:"), "header: {head}");
}

/// AT-67: an unknown-layout type-64 tile (record size neither 32 nor 96)
/// shows metadata only, never decoded payload fields.
#[test]
fn at67_unknown_layout_type64_metadata_only() {
    let mut payload = vec![0u8; 48];
    payload[..16].copy_from_slice(b"unknown-layout-!");
    let (bytes, pk) = signed_file(|b| {
        b.append_raw(64, &payload);
        b.append_seal_for(0);
        b.append_seal_for(1);
    });
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("unknown-layout floor");
    let text = text_layer(&floor);
    assert!(text.contains("unknown layout"), "floor: {text}");
    assert!(text.contains("record size 48"), "floor: {text}");
    assert!(!text.contains("unknown-layout-!"), "raw payload must not leak");
}

/// AT-68: (a) no identity record -> the header sheet says so; (b) two
/// identity records -> the header sheet uses the first record only.
#[test]
fn at68_identity_absent_and_two_identities() {
    // (a) No identity record.
    let mut b = EdgeBuilder::new_bare();
    b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    let bytes = b.finish();
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![]);
    let head = header_text(&h);
    assert!(head.contains("device identity record absent"), "header: {head}");
    assert_eq!(
        verification_report(&h).expect("report").status,
        "unsigned",
        "no identity, no signatures -> unsigned"
    );

    // (b) Two identity records: the second device id must not appear in
    // the header sheet (first record supplies device fields).
    let mut b2 = EdgeBuilder::new();
    let second = s3a_edge::DeviceIdentityRecord::new(
        [0x22u8; 16],
        "second-model",
        [2, 0, 0, 0],
        b2.public_key(),
        NOW,
        2,
    );
    b2.append_identity(second);
    let bytes2 = b2.finish();
    let h2 = open_edge(&ctx, &bytes2, vec![]);
    let head2 = header_text(&h2);
    assert!(head2.contains("11111111111111111111111111111111"), "first id: {head2}");
    assert!(!head2.contains("22222222222222222222222222222222"), "second id: {head2}");
    assert!(!head2.contains("second-model"), "second model: {head2}");
}

/// AT-85: an Edge file with no consent records of any kind is unrestricted:
/// event payloads are shown.
#[test]
fn at85_edge_unrestricted_shows_events() {
    let op = operator(0x33);
    let mut b = EdgeBuilder::new();
    b.append_events(vec![event(NOW, 0x0001, 2, 0x33)]);
    let bytes = b.finish();
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![]);
    assert!(header_text(&h).contains("withheld_count: 0"));
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("event floor");
    assert_present_in_frame(&floor, &hex(&op));
}

/// AT-86: under denial the derived header sheet and the identity floors
/// stay visible (the denial withholds event-log payloads only).
#[test]
fn at86_denial_keeps_header_and_identity_visible() {
    let (bytes, pk) = signed_file(|b| {
        b.append_events(vec![event(NOW, 0x0004, 5, 0x44)]);
        let cons = b.append_consent(edge_consent(0, NOW, 0));
        b.append_seal_for(0);
        b.append_seal_for(1);
        b.append_seal_for(cons);
    });
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let head = header_text(&h);
    assert!(head.contains("derived header sheet"), "header: {head}");
    assert!(head.contains("device_id 11111111111111111111111111111111"), "header: {head}");
    assert!(head.contains("withheld_count: 1"), "header: {head}");
    proceed_to_warehouse(&h).expect("proceed");
    let id_floor = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("identity floor");
    assert_present_in_frame(&id_floor, "11111111111111111111111111111111");
}

/// AT-134: flag set but no type-70 records -> missing_signature, no valid
/// floors, and the banner count is the event-log record count (no-valid-
/// floors counts parseable records anywhere).
#[test]
fn at134_missing_signature_no_valid_floors() {
    let mut b = EdgeBuilder::new();
    b.append_events(vec![
        event(NOW, 0x0001, 2, 0x11),
        event(NOW + 10, 0x0002, 1, 0x22),
    ]);
    let pk = b.public_key();
    let bytes = b.finish_with_signed_flag();
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("missing_signature"));
    assert_eq!(rep.invalid_tile_indices, vec![0, 1]);
    let head = header_text(&h);
    assert!(head.contains("quarantine: missing_signature"), "header: {head}");
    assert!(head.contains("withheld_count: 2"), "header: {head}");
}

/// AT-154 (Edge): flag clear and no signatures -> Unsigned, and not
/// unauthenticated as a whole (events are shown).
#[test]
fn at154_edge_flag_clear_no_signatures_unsigned() {
    let op = operator(0x55);
    let mut b = EdgeBuilder::new();
    b.append_events(vec![event(NOW, 0x0001, 2, 0x55)]);
    let pk = b.public_key();
    let bytes = b.finish();
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "unsigned");
    assert!(rep.quarantine_reason.is_none());
    assert!(header_text(&h).contains("withheld_count: 0"));
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("event floor");
    assert_present_in_frame(&floor, &hex(&op));
}

/// AT-155: (a) an unpinned signer quarantines the whole file (no valid
/// floors); (b) with empty pins, a later record that verifies under no
/// known key after an authentic one is signature_known_signer.
#[test]
fn at155_unpinned_signer_and_known_signer() {
    // (a) Seal records signed with sk2; identity carries pk2 (embedded);
    // pins carry pk1 only -> verifies embedded, not pinned.
    let sk2 = SigningKey::from_bytes(&[0x77; 32]);
    let pk1 = SigningKey::from_bytes(&[0x42; 32]).verifying_key().to_bytes();
    let mut b = EdgeBuilder::new_with_seed(0x77);
    let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b.append_seal_for_with_key(0, &sk2);
    b.append_seal_for_with_key(ev, &sk2);
    let bytes = b.finish_with_signed_flag();
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk1]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("signer_not_pinned"));
    assert_eq!(rep.invalid_tile_indices, vec![0, 1, 2, 3]);

    // (b) Empty pins: tile 0 authentic (sk1, embedded pk1), tile 1 sealed
    // with sk2 (unknown) -> signature_known_signer, break_index 1.
    let mut b2 = EdgeBuilder::new();
    let ev2 = b2.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b2.append_seal_for(0);
    b2.append_seal_for_with_key(ev2, &sk2);
    let bytes2 = b2.finish_with_signed_flag();
    let h2 = open_edge(&ctx, &bytes2, vec![]);
    let rep2 = verification_report(&h2).expect("report");
    assert_eq!(rep2.status, "quarantined");
    assert_eq!(
        rep2.quarantine_reason.as_deref(),
        Some("signature_known_signer")
    );
    assert_eq!(rep2.break_index, Some(1));
    assert!(rep2.invalid_tile_indices.contains(&1));
    assert!(!rep2.invalid_tile_indices.contains(&0));
}

/// AT-177: a denial on a CRC-failed type-71 tile can only restrict — an
/// untrusted denial still denies, and an untrusted grant never releases.
#[test]
fn at177_crc_failed_denial_can_only_restrict() {
    let ctx = test_context();
    // (a) Trusted grant + later untrusted denial (CRC-failed tile): denied.
    let mut b2 = EdgeBuilder::new();
    b2.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b2.append_consent(edge_consent(PURPOSE_RESEARCH_ANALYTICS, NOW, 0));
    let denial2 = b2.append_consent(edge_consent(0, NOW, 0));
    let mut bytes2 = b2.finish();
    flip_edge_payload_byte(&mut bytes2, denial2 as usize, 0);
    let h2 = open_edge(&ctx, &bytes2, vec![]);
    let head2 = header_text(&h2);
    assert!(head2.contains("withheld_count: 1"), "denied events: {head2}");
    proceed_to_warehouse(&h2).expect("proceed");
    let floor = render_floor_detail(&h2, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("event floor");
    assert_absent_from_frame(&floor, &hex(&operator(0x11)));

    // (b) Trusted denial + later untrusted grant (CRC-failed tile): still denied.
    let mut b3 = EdgeBuilder::new();
    b3.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b3.append_consent(edge_consent(0, NOW, 0));
    let grant3 = b3.append_consent(edge_consent(PURPOSE_RESEARCH_ANALYTICS, NOW, 0));
    let mut bytes3 = b3.finish();
    flip_edge_payload_byte(&mut bytes3, grant3 as usize, 0);
    let h3 = open_edge(&ctx, &bytes3, vec![]);
    assert!(header_text(&h3).contains("withheld_count: 1"));
}

/// AT-194: a pre-break grant releases, and post-break payloads are not
/// counted (they are invalid floors).
#[test]
fn at194_pre_break_grant_releases_post_break_not_counted() {
    let op_pre = operator(0x11);
    let op_post = operator(0x22);
    let (mut bytes, pk) = signed_file(|b| {
        let grant = b.append_consent(edge_consent(PURPOSE_RESEARCH_ANALYTICS, NOW, 0));
        let ev_pre = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        let ev_post = b.append_events(vec![event(NOW + 10, 0x0002, 1, 0x22)]);
        b.append_seal_for(0);
        b.append_seal_for(grant);
        b.append_seal_for(ev_pre);
        b.append_seal_for(ev_post);
    });
    // Break the chain at the post-break event tile (tile 3): edit + recompute.
    edit_edge_tile_payload_recompute_crc(&mut bytes, 3, 0);
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(rep.break_index, Some(3));
    assert!(header_text(&h).contains("withheld_count: 0"));
    proceed_to_warehouse(&h).expect("proceed");
    let pre = render_floor_detail(&h, 2, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("pre-break event floor");
    assert_present_in_frame(&pre, &hex(&op_pre));
    assert!(render_floor_detail(&h, 3, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).is_err());
    let wh = render_warehouse(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR).expect("warehouse");
    assert!(!text_layer(&wh).contains(&hex(&op_post)));
}

/// AT-195: a signature-stripped file has no valid floors, and the banner
/// count is the event-log record count.
#[test]
fn at195_signature_stripped_no_valid_floors() {
    let (mut bytes, pk) = signed_file(|b| {
        let ev = b.append_events(vec![
            event(NOW, 0x0001, 2, 0x11),
            event(NOW + 10, 0x0002, 1, 0x22),
        ]);
        b.append_seal_for(0);
        b.append_seal_for(ev);
    });
    strip_edge_signatures(&mut bytes);
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("signer_not_pinned"));
    assert_eq!(rep.invalid_tile_indices, vec![0, 1, 2, 3]);
    let head = header_text(&h);
    assert!(head.contains("quarantine: signer_not_pinned"), "header: {head}");
    assert!(head.contains("withheld_count: 2"), "header: {head}");
}

/// AT-201: ten DD-145 attribution cases (see the step-8b log for the table).
#[test]
fn at201_attribution_cases() {
    let ctx = test_context();
    let sk1 = SigningKey::from_bytes(&[0x42; 32]);
    let pk1 = sk1.verifying_key().to_bytes();
    let sk2 = SigningKey::from_bytes(&[0x77; 32]);

    // 1. Pinned key, authentic record -> verified.
    let (b1, _) = signed_file(|b| {
        let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        b.append_seal_for(0);
        b.append_seal_for(ev);
    });
    let r1 = verification_report(&open_edge(&ctx, &b1, vec![pk1])).expect("report");
    assert_eq!((r1.status.as_str(), r1.quarantine_reason.as_deref()), ("verified", None));

    // 2. Empty pins, authentic record (embedded key) -> self_signed.
    let r2 = verification_report(&open_edge(&ctx, &b1, vec![])).expect("report");
    assert_eq!((r2.status.as_str(), r2.quarantine_reason.as_deref()), ("self_signed", None));

    // 3. Unpinned signer (verifies embedded only) -> signer_not_pinned, no floors.
    let mut b3 = EdgeBuilder::new_with_seed(0x77);
    let ev3 = b3.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b3.append_seal_for(0);
    b3.append_seal_for(ev3);
    let bytes3 = b3.finish_with_signed_flag();
    let r3 = verification_report(&open_edge(&ctx, &bytes3, vec![pk1])).expect("report");
    assert_eq!(r3.status, "quarantined");
    assert_eq!(r3.quarantine_reason.as_deref(), Some("signer_not_pinned"));
    assert_eq!(r3.invalid_tile_indices, vec![0, 1, 2, 3]);

    // 4. Chain break (verifies pinned, hash mismatch), preceding authentic 0
    // -> chain_break, break_index 1.
    let (mut b4, _) = signed_file(|b| {
        let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        b.append_seal_for(0);
        b.append_seal_for(ev);
    });
    edit_edge_tile_payload_recompute_crc(&mut b4, 1, 0);
    let r4 = verification_report(&open_edge(&ctx, &b4, vec![pk1])).expect("report");
    assert_eq!(r4.status, "quarantined");
    assert_eq!(r4.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(r4.break_index, Some(1));

    // 5. Signature known signer (verifies under no key, preceding authentic)
    // -> signature_known_signer, break_index 1.
    let mut b5 = EdgeBuilder::new();
    let ev5 = b5.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b5.append_seal_for(0);
    b5.append_seal_for_with_key(ev5, &sk2);
    let bytes5 = b5.finish_with_signed_flag();
    let r5 = verification_report(&open_edge(&ctx, &bytes5, vec![pk1])).expect("report");
    assert_eq!(r5.quarantine_reason.as_deref(), Some("signature_known_signer"));
    assert_eq!(r5.break_index, Some(1));

    // 6. Flag set, no type-70 records -> missing_signature, no valid floors.
    let mut b6 = EdgeBuilder::new();
    b6.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    let bytes6 = b6.finish_with_signed_flag();
    let r6 = verification_report(&open_edge(&ctx, &bytes6, vec![pk1])).expect("report");
    assert_eq!(r6.quarantine_reason.as_deref(), Some("missing_signature"));
    assert_eq!(r6.invalid_tile_indices, vec![0, 1]);

    // 7. Seal record present but no identity record and no pins -> no
    // candidate keys -> missing_signature.
    let mut b7 = EdgeBuilder::new_bare();
    let ev7 = b7.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b7.append_seal_for(ev7);
    let bytes7 = b7.finish_with_signed_flag();
    let r7 = verification_report(&open_edge(&ctx, &bytes7, vec![])).expect("report");
    assert_eq!(r7.quarantine_reason.as_deref(), Some("missing_signature"));
    assert_eq!(r7.invalid_tile_indices, vec![0, 1]);

    // 8. Chain break on the first record (no preceding authentic)
    // -> chain_break, break_index 0.
    let (mut b8, _) = signed_file(|b| {
        let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        b.append_seal_for(0);
        b.append_seal_for(ev);
    });
    edit_edge_tile_payload_recompute_crc(&mut b8, 0, 0);
    let r8 = verification_report(&open_edge(&ctx, &b8, vec![pk1])).expect("report");
    assert_eq!(r8.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(r8.break_index, Some(0));

    // 9. Seal record for a tile_index beyond the file -> chain_break,
    // break_index = preceding + 1.
    let mut b9 = EdgeBuilder::new();
    let ev9 = b9.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b9.append_seal_for(0);
    b9.append_seal_for(ev9);
    // Extra container holding a record for tile 99 (signed, zero hash).
    let rec99 = s3a_edge::DeviceSealRecord::new(
        99,
        b9.w_generation(),
        [0u8; 32],
        b9.seal_hash(1),
        s3a_edge::sign_seal_hash(&sk1, &[0u8; 32]),
    );
    b9.append_raw_seal_record(&rec99);
    let bytes9 = b9.finish_with_signed_flag();
    let r9 = verification_report(&open_edge(&ctx, &bytes9, vec![pk1])).expect("report");
    assert_eq!(r9.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(r9.break_index, Some(2));

    // 10. Chain truncated by a CRC failure: the seal record past the stop
    // has hash_ok=false -> chain_break, break_index = the CRC tile.
    let (mut b10, _) = signed_file(|b| {
        let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        b.append_seal_for(0);
        b.append_seal_for(ev);
    });
    flip_edge_payload_byte(&mut b10, 1, 0);
    let r10 = verification_report(&open_edge(&ctx, &b10, vec![pk1])).expect("report");
    assert_eq!(r10.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(r10.break_index, Some(1));
    assert_eq!(r10.tiles_crc_failed, vec![1]);
}

/// AT-210: unsigned tail — tiles after the highest authentic tile are
/// invalid, except a type-70 container whose records all verify; and a
/// container that authenticates a later tile extends coverage (no tail).
#[test]
fn at210_unsigned_tail_and_container_exemption() {
    // (a) Unsigned event tile after the last authentic tile -> tail.
    let mut b = EdgeBuilder::new();
    let ev = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b.append_seal_for(0);
    b.append_seal_for(ev);
    let tail_tile = b.append_events(vec![event(NOW + 10, 0x0002, 1, 0x22)]);
    assert_eq!(tail_tile, 4);
    let pk = b.public_key();
    let bytes = b.finish_with_signed_flag();
    let ctx = test_context();
    let h = open_edge(&ctx, &bytes, vec![pk]);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "verified");
    assert_eq!(rep.invalid_tile_indices, vec![4]);
    assert!(header_text(&h).contains("unsigned tail: 4"));

    // (b) A later type-70 container that authenticates the tail tile
    // extends coverage: no tail, all floors valid.
    let mut b2 = EdgeBuilder::new();
    let ev2 = b2.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
    b2.append_seal_for(0);
    b2.append_seal_for(ev2);
    let tail2 = b2.append_events(vec![event(NOW + 10, 0x0002, 1, 0x22)]);
    b2.append_seal_for(tail2);
    let pk2 = b2.public_key();
    let bytes2 = b2.finish_with_signed_flag();
    let h2 = open_edge(&ctx, &bytes2, vec![pk2]);
    let rep2 = verification_report(&h2).expect("report");
    assert_eq!(rep2.status, "verified");
    assert!(rep2.invalid_tile_indices.is_empty(), "rep: {rep2:?}");
    assert!(!header_text(&h2).contains("unsigned tail"));
}

/// AT-220: chain-inactive seal-hash mismatch — sparse and dense. The
/// reason is chain_break, but seal_status is unsealed (the chain is
/// inactive, so nothing is seal_failed).
#[test]
fn at220_chain_inactive_seal_hash_mismatch() {
    let ctx = test_context();
    // Sparse: seal records for tiles 0 and 2 only; tile 2 edited.
    let (mut sparse, pk) = signed_file(|b| {
        let ev1 = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        let ev2 = b.append_events(vec![event(NOW + 10, 0x0002, 1, 0x22)]);
        b.append_seal_for(0);
        b.append_seal_for(ev2);
        let _ = ev1;
    });
    edit_edge_tile_payload_recompute_crc(&mut sparse, 2, 0);
    set_edge_chain_inactive(&mut sparse);
    let rs = verification_report(&open_edge(&ctx, &sparse, vec![pk])).expect("report");
    assert_eq!(rs.status, "quarantined");
    assert_eq!(rs.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(rs.break_index, Some(1));
    assert_eq!(rs.seal_status, "unsealed");

    // Dense: seal records for tiles 0, 1, 2; tile 1 edited.
    let (mut dense, _) = signed_file(|b| {
        let ev1 = b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        let ev2 = b.append_events(vec![event(NOW + 10, 0x0002, 1, 0x22)]);
        b.append_seal_for(0);
        b.append_seal_for(ev1);
        b.append_seal_for(ev2);
    });
    edit_edge_tile_payload_recompute_crc(&mut dense, 1, 0);
    set_edge_chain_inactive(&mut dense);
    let rd = verification_report(&open_edge(&ctx, &dense, vec![pk])).expect("report");
    assert_eq!(rd.status, "quarantined");
    assert_eq!(rd.quarantine_reason.as_deref(), Some("chain_break"));
    assert_eq!(rd.break_index, Some(1));
    assert_eq!(rd.seal_status, "unsealed");
}

/// AT-232: Edge floors of other types (here type 75, time-series) show
/// metadata only, under a grant and under a denial.
#[test]
fn at232_other_type_floors_metadata_only() {
    let payload = b"time-series-payload-secret".to_vec();
    for allowed in [PURPOSE_RESEARCH_ANALYTICS, 0u32] {
        let mut b = EdgeBuilder::new();
        b.append_events(vec![event(NOW, 0x0001, 2, 0x11)]);
        let ts = b.append_raw(75, &payload);
        b.append_consent(edge_consent(allowed, NOW, 0));
        let bytes = b.finish();
        let ctx = test_context();
        let h = open_edge(&ctx, &bytes, vec![]);
        proceed_to_warehouse(&h).expect("proceed");
        let floor = render_floor_detail(&h, ts, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
            .expect("type-75 floor");
        let text = text_layer(&floor);
        assert!(text.contains("type 75"), "floor: {text}");
        assert!(!text.contains("time-series-payload-secret"), "raw payload must not leak");
    }
}
