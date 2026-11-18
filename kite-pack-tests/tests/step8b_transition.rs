//! Step 8b — floor transition sampling (SPEC §5.8).
//! AT-13, AT-62, AT-93.
//!
//! §5.8: start rect is the warehouse floor node rect rendered with the
//! same params; end rect is (0.06w, 0.08h, 0.88w, 0.84h); opacity 0.2 -> 1
//! along CSS cubic-bezier(.2,.8,.2,1) at t/0.45; t is clamped to [0, 0.45];
//! instant=true returns only the end frame. Warehouse bays are square at
//! reference scale: side 32 (4 KiB), 45 (8 KiB), 181 (128 KiB).

mod fixture8b;
mod support;

use s3a_pack_kite::{
    open, proceed_to_warehouse, render_floor_transition, render_warehouse,
};

use fixture8b::*;
use support::*;

const NOW: u64 = 1_700_000_000;

fn open_bytes(ctx: &s3a_pack_kite::KiteContext, bytes: &[u8]) -> s3a_pack_kite::Handle {
    open(ctx, bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![]).expect("open")
}

/// The transition card node for `floor` in a frame.
fn card(frame: &s3a_pack_kite::RenderFrame) -> &s3a_pack_kite::SemanticNode {
    frame
        .semantics
        .iter()
        .find(|n| n.role == "card")
        .expect("transition frame must carry a card node")
}

/// AT-13 + AT-62: transition sampling at t=0, t=0.225, t=0.45. The card
/// interpolates from the warehouse bay rect to the end rect, and opacity
/// follows cubic-bezier(.2,.8,.2,1) at t/0.45: 0.2 at t=0, ~0.957 at
/// t=0.225, 1.0 at t=0.45 (verified by hand in the step-8b log).
#[test]
fn at13_transition_sampling() {
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at13".into()),
            HyperTile::Consent(vec![grant(subject(0x11), NOW, 0)]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    proceed_to_warehouse(&h).expect("proceed");
    let wh = render_warehouse(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR).expect("warehouse");
    let bay = wh
        .semantics
        .iter()
        .find(|n| n.floor == Some(1))
        .expect("warehouse floor node for tile 1");
    let (bx, by, bw, bh) = (bay.x, bay.y, bay.width, bay.height);
    assert_eq!((bw, bh), (181, 181), "128 KiB bay is square, side 181");

    let end = (
        (0.06 * CSS_WIDTH).round() as i32,
        (0.08 * CSS_HEIGHT).round() as i32,
        (0.88 * CSS_WIDTH).round() as u32,
        (0.84 * CSS_HEIGHT).round() as u32,
    );

    // t = 0: the card is exactly the warehouse bay rect, opacity 0.2.
    let f0 = render_floor_transition(&h, 1, 0.0, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, false, DPR)
        .expect("t=0 frame");
    let c0 = card(&f0);
    assert_eq!((c0.x, c0.y, c0.width, c0.height), (bx, by, bw, bh));
    assert!((c0.opacity.unwrap() - 0.2).abs() < 1e-9, "opacity at t=0");

    // t = 0.225: halfway in time, ~0.946 through the bezier. The card is
    // strictly between the bay rect and the end rect (here it moves left
    // in x, down in y, and grows in width/height).
    let f1 = render_floor_transition(&h, 1, 0.225, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, false, DPR)
        .expect("t=0.225 frame");
    let c1 = card(&f1);
    assert!((c1.opacity.unwrap() - 0.957).abs() < 0.01, "opacity at t=0.225");
    let between = |a: i32, b: i32, v: i32| (v > a.min(b)) && (v < a.max(b));
    assert!(between(bx, end.0, c1.x), "x interpolates: {c1:?} bay x={bx} end x={}", end.0);
    assert!(between(by, end.1, c1.y), "y interpolates: {c1:?} bay y={by} end y={}", end.1);
    assert!(c1.width > bw && c1.width < end.2, "width interpolates");

    // t = 0.45: exactly the end rect, opacity 1.0.
    let f2 = render_floor_transition(&h, 1, 0.45, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, false, DPR)
        .expect("t=0.45 frame");
    let c2 = card(&f2);
    assert_eq!((c2.x, c2.y, c2.width, c2.height), end);
    assert!((c2.opacity.unwrap() - 1.0).abs() < 1e-9, "opacity at t=0.45");

    // t beyond the clamp (0.9) is the end frame too.
    let f3 = render_floor_transition(&h, 1, 0.9, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, false, DPR)
        .expect("t=0.9 frame");
    let c3 = card(&f3);
    assert_eq!((c3.x, c3.y, c3.width, c3.height), end);
    assert!((c3.opacity.unwrap() - 1.0).abs() < 1e-9, "opacity clamped");
}

/// AT-93: instant=true returns only the end frame (end rect, opacity 1.0)
/// regardless of t.
#[test]
fn at93_instant_transition_is_end_frame() {
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at93".into()),
            HyperTile::Consent(vec![grant(subject(0x22), NOW, 0)]),
        ],
        true,
    );
    let ctx = test_context();
    let h = open_bytes(&ctx, &fx.bytes);
    proceed_to_warehouse(&h).expect("proceed");
    for t in [0.0, 0.225, 0.45] {
        let f = render_floor_transition(&h, 1, t, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, true, DPR)
            .expect("instant frame");
        let c = card(&f);
        assert_eq!(
            (c.x, c.y, c.width, c.height),
            (
                (0.06 * CSS_WIDTH).round() as i32,
                (0.08 * CSS_HEIGHT).round() as i32,
                (0.88 * CSS_WIDTH).round() as u32,
                (0.84 * CSS_HEIGHT).round() as u32,
            ),
            "instant frame is the end rect at t={t}"
        );
        assert!((c.opacity.unwrap() - 1.0).abs() < 1e-9, "instant opacity 1.0");
    }
}
