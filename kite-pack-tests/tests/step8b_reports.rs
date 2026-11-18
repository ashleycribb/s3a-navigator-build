//! Step 8b — verification reports and `open_verified` (SPEC §5.1).
//! AT-35 (hyper-tile), AT-90, AT-154 (hyper-tile), AT-211, AT-112.
//!
//! AT-35's Edge signature_known_signer half is covered by AT-201 case 5
//! in step8b_edge.rs.

mod fixture8b;
mod support;

use s3a_pack_kite::{
    open, open_verified, proceed_to_warehouse, render_floor_detail, render_manifest,
    verification_report, KiteError, KiteVerificationReport,
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

/// The error code of an `open_verified` result, panicking if it opened.
fn err_code_of(label: &str, r: Result<s3a_pack_kite::Handle, KiteError>) -> String {
    match r {
        Ok(_) => panic!("expected an open_verified error, got Ok ({label})"),
        Err(e) => e.code().to_string(),
    }
}

/// AT-35 (hyper-tile): a sealed file with a committed CRC failure is
/// Quarantined; the seal-trailer reason outranks crc, but the CRC rule
/// decides the valid floors (the failed tile and everything after it are
/// not floors).
#[test]
fn at35_hyper_crc_quarantines() {
    let x = subject(0x11);
    let y = subject(0x22);
    let mut fx = build_hyper(
        vec![
            HyperTile::Manifest("at35".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0)]),
            HyperTile::Consent(vec![grant(y, NOW, 0)]),
        ],
        true,
    );
    flip_hyper_payload_byte(&mut fx.bytes, 1, 80);
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("seal_trailer"));
    assert_eq!(rep.seal_status, "seal_failed");
    assert_eq!(rep.tiles_crc_failed, vec![1]);
    assert_eq!(rep.break_index, Some(1));
    assert!(rep.invalid_tile_indices.contains(&1));
    assert!(rep.invalid_tile_indices.contains(&2));
    // Unauthenticated as a whole, so the grants are withheld — but the
    // count is 0: the consent records sit on invalid floors (tiles 1, 2),
    // and the counting rule adds parseable payloads on invalid tiles only
    // when the file has no valid floors (AT-194). Contrast AT-90, where
    // the CRC-valid tiles stay display-only floors and the count is 2.
    let m = manifest_text(&h);
    assert!(m.contains("quarantine: seal_trailer"), "manifest: {m}");
    assert!(m.contains("withheld_count: 0"), "manifest: {m}");
    // The manifest floor (before the break) is still a valid floor.
    proceed_to_warehouse(&h).expect("proceed");
    assert!(render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).is_ok());
    assert!(render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).is_err());
}

/// AT-90: a seal-failed hyper-tile with no CRC failure lists every
/// CRC-valid committed tile as a valid floor (display only), while the
/// file is unauthenticated as a whole for consent.
#[test]
fn at90_seal_failed_display_only_floors() {
    let x = subject(0x33);
    let y = subject(0x44);
    let mut fx = build_hyper(
        vec![
            HyperTile::Manifest("at90".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0), grant(y, NOW, 0)]),
        ],
        true,
    );
    tamper_seal_trailer(&mut fx.bytes);
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "quarantined");
    assert_eq!(rep.quarantine_reason.as_deref(), Some("seal_trailer"));
    assert_eq!(rep.seal_status, "seal_failed");
    assert!(rep.tiles_crc_failed.is_empty());
    assert!(rep.invalid_tile_indices.is_empty(), "all CRC-valid tiles are floors");
    let m = manifest_text(&h);
    assert!(m.contains("quarantine: seal_trailer"), "manifest: {m}");
    assert!(m.contains("status: quarantined seal: seal_failed"), "manifest: {m}");
    assert!(m.contains("withheld_count: 2"), "manifest: {m}");
    // The consent floor renders (display only) but shows no records.
    proceed_to_warehouse(&h).expect("proceed");
    let floor = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR)
        .expect("consent floor is a valid display-only floor");
    assert!(text_layer(&floor).contains("withheld_count: 2"));
    assert_absent_from_frame(&floor, &hex(&x));
    assert_absent_from_frame(&floor, &hex(&y));
}

/// AT-154 (hyper-tile): a sealed-OK hyper-tile (trailer verifies, no
/// device signer) is Unsigned with seal_status sealed, not Verified.
#[test]
fn at154_hyper_sealed_ok_unsigned() {
    let x = subject(0x55);
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at154".into()),
            HyperTile::Consent(vec![grant(x, NOW, 0)]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    let rep = verification_report(&h).expect("report");
    assert_eq!(rep.status, "unsigned");
    assert_eq!(rep.seal_status, "sealed");
    assert!(rep.quarantine_reason.is_none());
    assert!(manifest_text(&h).contains("withheld_count: 0"));
}

/// AT-112: a hyper-tile file with no consent records of any kind is
/// unrestricted: nothing is withheld.
#[test]
fn at112_hyper_unrestricted() {
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at112".into()),
            HyperTile::Raw(68, b"blob-bytes".to_vec(), 1),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    assert!(manifest_text(&h).contains("withheld_count: 0"));
}

/// AT-211: `open_verified` report validation. Each bad report is
/// `invalid_report`; `status: unverified` is the distinct code `unverified`;
/// a valid report opens and is returned as supplied (AT-137); unsupported
/// bytes are `not_archive` (AT-97).
#[test]
fn at211_open_verified_validation() {
    let ctx = test_context();
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at211".into()),
            HyperTile::Consent(vec![grant(subject(0x66), NOW, 0)]),
        ],
        true,
    );
    let h = open_bytes(&ctx, &fx.bytes);
    let good = verification_report(&h).expect("report");
    assert_eq!(good.status, "unsigned");
    assert_eq!(good.invalid_tile_indices, Vec::<u32>::new());

    let open_v = |rep: KiteVerificationReport| {
        open_verified(
            &ctx,
            &fx.bytes,
            None,
            MAX_FRAME_PIXELS,
            MAX_OPEN_BYTES,
            rep,
        )
    };

    // A valid report opens, and the handle carries the supplied report.
    let hv = open_v(good.clone()).expect("valid report must open");
    assert_eq!(
        verification_report(&hv).expect("report"),
        good,
        "open_verified returns the supplied report (AT-137)"
    );

    // status unverified -> distinct code `unverified` (AT-154).
    let mut bad = good.clone();
    bad.status = "unverified".into();
    assert_eq!(err_code_of("case1", open_v(bad)), "unverified");

    // invalid_tile_indices out of range.
    let mut bad = good.clone();
    bad.invalid_tile_indices = vec![99];
    assert_eq!(err_code_of("case2", open_v(bad)), "invalid_report");

    // torn not a subset of invalid.
    let mut bad = good.clone();
    bad.torn_tile_indices = vec![0];
    assert_eq!(err_code_of("case3", open_v(bad)), "invalid_report");

    // break_index present but an index at/after it is not invalid.
    let mut bad = good.clone();
    bad.break_index = Some(1);
    assert_eq!(err_code_of("case4", open_v(bad)), "invalid_report");

    // signer_not_pinned without the full invalid range.
    let mut bad = good.clone();
    bad.status = "quarantined".into();
    bad.quarantine_reason = Some("signer_not_pinned".into());
    bad.invalid_tile_indices = vec![0];
    assert_eq!(err_code_of("case5", open_v(bad)), "invalid_report");

    // quarantined without a reason.
    let mut bad = good.clone();
    bad.status = "quarantined".into();
    assert_eq!(err_code_of("case6", open_v(bad)), "invalid_report");

    // a reason without quarantined status.
    let mut bad = good.clone();
    bad.quarantine_reason = Some("crc".into());
    bad.tiles_crc_failed = vec![1];
    bad.break_index = Some(1);
    bad.invalid_tile_indices = vec![1];
    assert_eq!(err_code_of("case7", open_v(bad)), "invalid_report");

    // a hyper-tile report must not use signature_known_signer.
    let mut bad = good.clone();
    bad.status = "quarantined".into();
    bad.quarantine_reason = Some("signature_known_signer".into());
    bad.break_index = Some(1);
    bad.invalid_tile_indices = vec![1];
    assert_eq!(err_code_of("case8", open_v(bad)), "invalid_report");

    // a hyper-tile report must not use status verified.
    let mut bad = good.clone();
    bad.status = "verified".into();
    assert_eq!(err_code_of("case9", open_v(bad)), "invalid_report");

    // seal_failed without seal_trailer reason.
    let mut bad = good.clone();
    bad.status = "quarantined".into();
    bad.seal_status = "seal_failed".into();
    assert_eq!(err_code_of("case10", open_v(bad)), "invalid_report");

    // crc reason with empty tiles_crc_failed.
    let mut bad = good.clone();
    bad.status = "quarantined".into();
    bad.quarantine_reason = Some("crc".into());
    bad.break_index = Some(1);
    bad.invalid_tile_indices = vec![1];
    assert_eq!(err_code_of("case11", open_v(bad)), "invalid_report");

    // tiles_crc_failed not a subset of invalid.
    let mut bad = good.clone();
    bad.status = "quarantined".into();
    bad.quarantine_reason = Some("crc".into());
    bad.tiles_crc_failed = vec![0];
    bad.break_index = Some(0);
    bad.invalid_tile_indices = vec![1];
    assert_eq!(err_code_of("case12", open_v(bad)), "invalid_report");

    // Unsupported bytes (inconsistent header) -> not_archive, not
    // invalid_report (AT-97).
    let mut unsupported = fx.bytes.clone();
    set_inconsistent_header_flags(&mut unsupported);
    let err = err_code_of(
        "unsupported-bytes",
        open_verified(
            &ctx,
            &unsupported,
            None,
            MAX_FRAME_PIXELS,
            MAX_OPEN_BYTES,
            good.clone(),
        ),
    );
    assert_eq!(err, "not_archive");
}
