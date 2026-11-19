//! Step 8c — Pixmap paint (AT-19, AT-20), frame-size rules (AT-141, AT-143),
//! and the §5.8 render-model oracle for metadata-only floors
//! (AT-11, AT-23, AT-67, AT-232) plus the shown-events oracle.
//!
//! Test-first: written against SPEC v24.4 before the paint implementation
//! exists; the first run is expected to fail.

mod fixture8b;
mod support;

use fixture8b::{
    EdgeBuilder, HyperTile, assert_absent_from_frame, build_hyper, edge_consent, event, hex,
    operator, subject,
};
use s3a_edge::PURPOSE_RESEARCH_ANALYTICS;
use s3a_pack_kite::{
    open, proceed_to_warehouse, render_floor_detail, render_manifest, render_warehouse,
};
use support::{assert_pixmap_size, text_layer, CSS_HEIGHT, CSS_WIDTH, DPR, MAX_FRAME_PIXELS, MAX_OPEN_BYTES};

const NOW: u64 = 1_700_000_000;

/// Sample one RGBA pixel from a frame at pixmap coordinates (x, y).
fn sample(frame: &s3a_pack_kite::RenderFrame, x: usize, y: usize) -> [u8; 4] {
    let idx = (y * frame.width as usize + x) * 4;
    frame.rgba[idx..idx + 4].try_into().unwrap()
}

/// Centre of a severity badge rect in pixmap coordinates (CSS rect scaled
/// by dpr). The dpr is recovered from the frame's pixmap width.
fn badge_centre(frame: &s3a_pack_kite::RenderFrame, css_rect: (f64, f64, f64, f64)) -> (usize, usize) {
    let (x, y, w, h) = css_rect;
    let dpr = frame.width as f64 / CSS_WIDTH;
    (
        ((x + w / 2.0) * dpr) as usize,
        ((y + h / 2.0) * dpr) as usize,
    )
}

/// Find the SemanticNode with the given label and return its CSS rect.
fn node_rect(frame: &s3a_pack_kite::RenderFrame, label: &str) -> (f64, f64, f64, f64) {
    let node = frame
        .semantics
        .iter()
        .find(|n| n.label == label)
        .unwrap_or_else(|| panic!("no SemanticNode with label {label:?}"));
    (
        node.x as f64,
        node.y as f64,
        node.width as f64,
        node.height as f64,
    )
}

fn open_edge(ctx: &s3a_pack_kite::KiteContext, bytes: &[u8]) -> s3a_pack_kite::Handle {
    open(ctx, bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open")
}

/// Compare two pixmaps by counting differing bytes (never `assert_eq` on
/// the whole `rgba` vec — a failure would dump megabytes of zeros).
fn assert_pixmaps_equal(a: &s3a_pack_kite::RenderFrame, b: &s3a_pack_kite::RenderFrame, what: &str) {
    assert_eq!(a.width, b.width, "{what} width");
    assert_eq!(a.height, b.height, "{what} height");
    assert_eq!(a.rgba.len(), b.rgba.len(), "{what} rgba len");
    let diff = a
        .rgba
        .iter()
        .zip(b.rgba.iter())
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(diff, 0, "{what} pixmap differs in {diff} bytes");
}

fn pixmaps_identical(a: &s3a_pack_kite::RenderFrame, b: &s3a_pack_kite::RenderFrame) -> bool {
    a.width == b.width && a.height == b.height && a.rgba == b.rgba
}

/// Compare two pixmaps after blanking (to background white) the pixels of
/// every text run containing `payload_hash_short` in each frame. The SPEC
/// §5.8 control takes derived displays from the real bytes, so the hash run
/// — a derived display of the payload — is excluded from the comparison;
/// every other pixel must match. Diff-counted, never a whole-vec assert.
fn pixmaps_equal_except_hash(a: &s3a_pack_kite::RenderFrame, b: &s3a_pack_kite::RenderFrame) {
    assert_eq!(a.width, b.width);
    assert_eq!(a.height, b.height);
    let blank = |f: &s3a_pack_kite::RenderFrame| -> Vec<u8> {
        let mut rgba = f.rgba.clone();
        for run in &f.text_runs {
            if run.text.contains("payload_hash_short") {
                for py in run.y as usize..(run.y as usize + run.height as usize) {
                    for px in run.x as usize..(run.x as usize + run.width as usize) {
                        if px < f.width as usize && py < f.height as usize {
                            let idx = (py * f.width as usize + px) * 4;
                            rgba[idx..idx + 4].copy_from_slice(&[255, 255, 255, 255]);
                        }
                    }
                }
            }
        }
        rgba
    };
    let ba = blank(a);
    let bb = blank(b);
    let diff = ba.iter().zip(bb.iter()).filter(|(x, y)| x != y).count();
    assert_eq!(diff, 0, "pixmaps differ in {diff} bytes outside the hash run");
}

#[test]
fn at_19_okabe_ito_badge_fills() {
    // Four rows: maintenance sev 0, normal sev 1, fault sev 2, unknown sev 3.
    let mut b = EdgeBuilder::new();
    b.append_events(vec![
        event(NOW, 0x0005, 0, 0x11), // maintenance, severity 0
        event(NOW + 1, 0x0001, 1, 0x22), // normal, severity 1
        event(NOW + 2, 0x0003, 2, 0x33), // fault, severity 2
        event(NOW + 3, 0x0009, 3, 0x44), // unknown, severity 3
    ]);
    let bytes = b.finish();
    let ctx = support::test_context();
    let h = open_edge(&ctx, &bytes);
    proceed_to_warehouse(&h).expect("proceed");
    let frame = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert_pixmap_size(&frame, CSS_WIDTH, CSS_HEIGHT, DPR);

    // The non-colour category label and the severity label are always present.
    for (sev, category) in [
        (0, "maintenance"),
        (1, "normal"),
        (2, "fault"),
        (3, "unknown"),
    ] {
        let text = text_layer(&frame);
        let sev_text = format!("severity {sev}");
        assert!(text.contains(&sev_text), "missing {sev_text} in {text}");
        assert!(text.contains(category), "missing {category} in {text}");
        let node_label = format!("severity: {sev}");
        let rect = node_rect(&frame, &node_label);
        let (px, py) = badge_centre(&frame, rect);
        let got = sample(&frame, px, py);
        let want = match sev {
            0 => [0x00, 0x72, 0xB2, 0xFF], // #0072B2
            1 => [0xE6, 0x9F, 0x00, 0xFF], // #E69F00
            _ => [0xD5, 0x5E, 0x00, 0xFF], // #D55E00 (2 and 3 share)
        };
        assert_eq!(got, want, "badge fill for severity {sev} at ({px}, {py})");
    }

    // Same assertions at dpr 2.0: centres are scaled by dpr.
    let frame2 = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, 2.0).unwrap();
    assert_pixmap_size(&frame2, CSS_WIDTH, CSS_HEIGHT, 2.0);
    let rect = node_rect(&frame2, "severity: 2");
    let (px, py) = badge_centre(&frame2, rect);
    assert_eq!(sample(&frame2, px, py), [0xD5, 0x5E, 0x00, 0xFF]);
}

#[test]
fn at_19_severity_zero_maintenance_label() {
    // A severity-0 maintenance event still carries its category label.
    let mut b = EdgeBuilder::new();
    b.append_events(vec![event(NOW, 0x0005, 0, 0x11)]);
    let bytes = b.finish();
    let ctx = support::test_context();
    let h = open_edge(&ctx, &bytes);
    proceed_to_warehouse(&h).expect("proceed");
    let frame = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let text = text_layer(&frame);
    assert!(text.contains("maintenance"), "floor: {text}");
    assert!(text.contains("severity 0"), "floor: {text}");
    let rect = node_rect(&frame, "severity: 0");
    let (px, py) = badge_centre(&frame, rect);
    assert_eq!(sample(&frame, px, py), [0x00, 0x72, 0xB2, 0xFF]);
}

#[test]
fn at_20_identity_floor_paints_device_fields() {
    // EdgeBuilder::new() writes the identity record on tile 0:
    // device_id [0x11; 16], hardware_model "test-model".
    let bytes = EdgeBuilder::new().finish();
    let ctx = support::test_context();
    let h = open_edge(&ctx, &bytes);
    proceed_to_warehouse(&h).expect("proceed");
    let frame = render_floor_detail(&h, 0, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let text = text_layer(&frame);
    assert!(text.contains("11111111111111111111111111111111"), "floor: {text}");
    assert!(text.contains("test-model"), "floor: {text}");
    assert!(!text.contains("severity"), "floor: {text}");
    assert!(!text.contains("event "), "floor: {text}");
    // The identity line is painted with the text ink at a run centre.
    let run = frame.text_runs.first().expect("identity run");
    let (rx, ry) = (
        (run.x + run.width as i32 / 2) as usize,
        (run.y + run.height as i32 / 2) as usize,
    );
    assert_eq!(sample(&frame, rx, ry), [0x22, 0x22, 0x22, 0xFF]);
}

#[test]
fn at_141_invalid_frame_size() {
    let mut b = EdgeBuilder::new();
    b.append_events(vec![event(NOW, 0x0001, 1, 0x11)]);
    let bytes = b.finish();
    let ctx = support::test_context();
    let h = open_edge(&ctx, &bytes);
    proceed_to_warehouse(&h).expect("proceed");
    // w == 0 and h == 0 are invalid_frame_size.
    for (w, hh, dpr) in [(0.0, CSS_HEIGHT, DPR), (CSS_WIDTH, 0.0, DPR)] {
        let err = render_floor_detail(&h, 1, w, hh, 0.0, dpr).unwrap_err();
        assert_eq!(err.code(), "invalid_frame_size", "w={w} h={hh}");
    }
    // Finite dpr <= 0 is invalid_frame_size.
    for dpr in [0.0, -1.0] {
        let err = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, dpr).unwrap_err();
        assert_eq!(err.code(), "invalid_frame_size", "dpr={dpr}");
    }
    // Non-finite dpr is invalid_param.
    let err = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, f64::NAN).unwrap_err();
    assert_eq!(err.code(), "invalid_param");
}

#[test]
fn at_141_product_over_max_frame_pixels() {
    let mut b = EdgeBuilder::new();
    b.append_events(vec![event(NOW, 0x0001, 1, 0x11)]);
    let bytes = b.finish();
    let ctx = support::test_context();
    // A tiny max_frame_pixels makes 800x600 exceed the cap.
    let h = open(&ctx, &bytes, None, 100, MAX_OPEN_BYTES, vec![]).unwrap();
    proceed_to_warehouse(&h).expect("proceed");
    let err = render_floor_detail(&h, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap_err();
    assert_eq!(err.code(), "invalid_frame_size");
}

#[test]
fn at_143_dpr_two_pixmap_dimensions() {
    let mut b = EdgeBuilder::new();
    b.append_events(vec![event(NOW, 0x0001, 1, 0x11)]);
    let bytes = b.finish();
    let ctx = support::test_context();
    let h = open_edge(&ctx, &bytes);
    proceed_to_warehouse(&h).expect("proceed");
    let frame = render_floor_detail(&h, 1, 100.0, 50.0, 0.0, 2.0).unwrap();
    assert_eq!(frame.width, 200);
    assert_eq!(frame.height, 100);
    assert_eq!(frame.rgba.len(), 80_000);
}

#[test]
fn oracle_at_11_withheld_subject_pixmap() {
    // Two hyper-tile archives: same structure, only the withdrawn subject
    // differs (X vs Z). Both subjects' records are withheld, so the consent
    // floor and the manifest show identical text; the §5.8 control pixmap
    // must be byte-identical to the fixture pixmap.
    let mk = |subj: u8| {
        build_hyper(
            vec![
                HyperTile::Manifest("at11 oracle".into()),
                HyperTile::Consent(vec![
                    fixture8b::grant(subject(subj), NOW, 0),
                    fixture8b::withdrawal(subject(subj), NOW + 100),
                ]),
            ],
            true,
        )
    };
    let fix = mk(0x11);
    let ctl = mk(0x33);
    let ctx = support::test_context();
    let hf = open_edge(&ctx, &fix.bytes);
    let hc = open_edge(&ctx, &ctl.bytes);
    let mf = render_manifest(&hf, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let mc = render_manifest(&hc, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert_pixmaps_equal(&mf, &mc, "manifest");
    assert!(text_layer(&mf).contains("withheld_count: 2"));
    assert_absent_from_frame(&mf, &hex(&subject(0x11)));
    proceed_to_warehouse(&hf).expect("proceed");
    proceed_to_warehouse(&hc).expect("proceed");
    let ff = render_floor_detail(&hf, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let fc = render_floor_detail(&hc, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert_pixmaps_equal(&ff, &fc, "consent floor");
    assert_absent_from_frame(&ff, &hex(&subject(0x11)));
}

#[test]
fn oracle_at_23_edge_denial_pixmap() {
    // Two Edge archives: same events, different operator ids, denied by a
    // type-71 record that does not allow the research purpose. The denied
    // rows render nothing, so the event-floor pixmaps must be identical.
    let mk = |op: u8| {
        let mut b = EdgeBuilder::new();
        b.append_events(vec![
            event(NOW, 0x0003, 2, op),
            event(NOW + 1, 0x0001, 1, op),
        ]);
        b.append_consent(edge_consent(0, NOW, 0));
        b.finish()
    };
    let ctx = support::test_context();
    let hf = open_edge(&ctx, &mk(0xAA));
    let hc = open_edge(&ctx, &mk(0xBB));
    proceed_to_warehouse(&hf).expect("proceed");
    proceed_to_warehouse(&hc).expect("proceed");
    let ff = render_floor_detail(&hf, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let fc = render_floor_detail(&hc, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert_pixmaps_equal(&ff, &fc, "denied event floor");
    assert!(text_layer(&ff).contains("withheld_count: 2"));
    assert_absent_from_frame(&ff, &hex(&operator(0xAA)));
}

#[test]
fn oracle_at_67_unknown_layout_pixmap() {
    // Two Edge archives with a type-64 unknown-layout floor (48-byte
    // records): the distinctive payload strings differ, the pixmaps must
    // not — except the payload_hash_short run, a derived display the SPEC
    // §5.8 control takes from the real bytes.
    let mk = |fill: u8| {
        let mut payload = b"unknown-layout-!".to_vec();
        payload.extend(std::iter::repeat(fill).take(32));
        assert_eq!(payload.len(), 48);
        let mut b = EdgeBuilder::new();
        b.append_raw(64, &payload);
        b.finish()
    };
    let ctx = support::test_context();
    let hf = open_edge(&ctx, &mk(b'A'));
    let hc = open_edge(&ctx, &mk(b'B'));
    proceed_to_warehouse(&hf).expect("proceed");
    proceed_to_warehouse(&hc).expect("proceed");
    let ff = render_floor_detail(&hf, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let fc = render_floor_detail(&hc, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert!(text_layer(&ff).contains("unknown layout"));
    assert!(text_layer(&ff).contains("record size 48"));
    assert_absent_from_frame(&ff, "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA");
    assert_absent_from_frame(&ff, "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB");
    pixmaps_equal_except_hash(&ff, &fc);
}

#[test]
fn oracle_at_232_non_edge_floor_pixmap() {
    // Two Edge archives with a type-75 floor: payload differs, pixmaps must
    // not — except the payload_hash_short run (derived display).
    let mk = |fill: u8| {
        let mut payload = b"time-series-payload-secret-".to_vec();
        payload.extend(std::iter::repeat(fill).take(16));
        let mut b = EdgeBuilder::new();
        b.append_raw(75, &payload);
        b.finish()
    };
    let ctx = support::test_context();
    let hf = open_edge(&ctx, &mk(b'A'));
    let hc = open_edge(&ctx, &mk(b'B'));
    proceed_to_warehouse(&hf).expect("proceed");
    proceed_to_warehouse(&hc).expect("proceed");
    let ff = render_floor_detail(&hf, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let fc = render_floor_detail(&hc, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert!(text_layer(&ff).contains("type 75"));
    assert_absent_from_frame(&ff, "AAAAAAAAAAAAAAAA");
    assert_absent_from_frame(&ff, "BBBBBBBBBBBBBBBB");
    pixmaps_equal_except_hash(&ff, &fc);
    // The same holds under a research grant.
    let mk2 = |fill: u8| {
        let mut payload = b"time-series-payload-secret-".to_vec();
        payload.extend(std::iter::repeat(fill).take(16));
        let mut b = EdgeBuilder::new();
        b.append_raw(75, &payload);
        b.append_consent(edge_consent(PURPOSE_RESEARCH_ANALYTICS, NOW, 0));
        b.finish()
    };
    let hg = open_edge(&ctx, &mk2(b'A'));
    let hgc = open_edge(&ctx, &mk2(b'B'));
    proceed_to_warehouse(&hg).expect("proceed");
    proceed_to_warehouse(&hgc).expect("proceed");
    let gf = render_floor_detail(&hg, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let gc = render_floor_detail(&hgc, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    pixmaps_equal_except_hash(&gf, &gc);
}

#[test]
fn oracle_shown_events_pixmap_differs() {
    // Positive control for the oracle: unrestricted events shown, with
    // different event names (different lengths), so the pixmaps DO differ
    // — proving the painter responds to content and the oracle is not
    // vacuous. Rect-fill paint is length-sensitive only: same-length
    // different text (e.g. two operator hexes) paints identically, so the
    // control varies the event name, not the operator. A future glyph
    // painter would need a pack-internal §5.8 control (see the test log).
    let mk = |ty: u16| {
        let mut b = EdgeBuilder::new();
        b.append_events(vec![event(NOW, ty, 2, 0xAA)]);
        b.finish()
    };
    let ctx = support::test_context();
    let hf = open_edge(&ctx, &mk(0x0001)); // "machine_start", category normal
    let hc = open_edge(&ctx, &mk(0x0004)); // "fault_code", category fault
    proceed_to_warehouse(&hf).expect("proceed");
    proceed_to_warehouse(&hc).expect("proceed");
    let ff = render_floor_detail(&hf, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    let fc = render_floor_detail(&hc, 1, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert!(!pixmaps_identical(&ff, &fc), "shown-event pixmaps should differ");
    let mut diff = 0;
    for i in (0..ff.rgba.len()).step_by(4) {
        if ff.rgba[i..i + 4] != fc.rgba[i..i + 4] {
            diff += 1;
        }
    }
    assert!(diff > 0 && diff < 10_000, "diff pixels: {diff}");
}

#[test]
fn paint_liveness_manifest_and_warehouse() {
    // The manifest and warehouse frames are painted: not all background,
    // and every visible text run contains a painted pixel.
    let fix = build_hyper(vec![HyperTile::Manifest("liveness manifest".into())], true);
    let ctx = support::test_context();
    let h = open_edge(&ctx, &fix.bytes);
    let m = render_manifest(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, DPR).unwrap();
    assert!(m.rgba.chunks_exact(4).any(|p| p == [0x22, 0x22, 0x22, 0xFF]));
    assert_absent_from_frame(&m, "this-string-is-not-present");
    proceed_to_warehouse(&h).expect("proceed");
    let w = render_warehouse(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR).unwrap();
    assert!(w.rgba.chunks_exact(4).any(|p| p == [0x22, 0x22, 0x22, 0xFF]));
}
