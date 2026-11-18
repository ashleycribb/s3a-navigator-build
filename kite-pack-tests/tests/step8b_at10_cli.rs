//! Step 8b — AT-10: the pack warehouse floor list agrees with
//! `s3a-cli tour --json` for a sealed, type-82-free hyper-tile archive
//! (Ashley's decision, Nov 18, 2026: keep this comparison).
//!
//! The CLI binary path comes from the env var `S3A_CLI_BIN`; the test
//! fails naming the variable when it is unset or not executable. CI builds
//! s3a-cli from the cargo-cached s3a-storage pin checkout and sets it.
//!
//! Compared per floor: index, type_id, record_count, size_bytes,
//! payload_hash_short (the CLI renders it as 8 hex chars + "..."; the pack
//! renders the same 8 hex chars without the suffix, so the suffix is
//! stripped before comparing).

mod fixture8b;
mod support;

use std::process::Command;

use s3a_pack_kite::{open, proceed_to_warehouse, render_warehouse};

use fixture8b::*;
use support::*;

const NOW: u64 = 1_700_000_000;

fn cli_bin() -> String {
    match std::env::var("S3A_CLI_BIN") {
        Ok(v) if !v.is_empty() => v,
        _ => panic!(
            "S3A_CLI_BIN is not set: set it to a built s3a-cli binary \
             (cargo build -p s3a-cli at the pinned s3a-storage checkout 2cf2b45)"
        ),
    }
}

/// Parse the pack warehouse floor lines out of the concatenated text layer.
/// The text layer joins runs without separators, so lines are split on
/// "index ": each chunk is "N type_id T record_count R size_bytes S
/// payload_hash_short H" (plus the next floor's prefix).
fn parse_pack_floors(text: &str) -> Vec<(u32, u32, u32, u64, String)> {
    let mut out = Vec::new();
    for chunk in text.split("index ").skip(1) {
        let parts: Vec<&str> = chunk.split_whitespace().take(9).collect();
        if parts.len() != 9
            || parts[1] != "type_id"
            || parts[3] != "record_count"
            || parts[5] != "size_bytes"
            || parts[7] != "payload_hash_short"
        {
            continue;
        }
        let ( Ok(n), Ok(t), Ok(r), Ok(s)) = (
            parts[0].parse(),
            parts[2].parse(),
            parts[4].parse(),
            parts[6].parse(),
        ) else {
            continue;
        };
        out.push((n, t, r, s, parts[8].to_string()));
    }
    out
}

#[test]
fn at10_cli_oracle() {
    let bin = cli_bin();
    assert!(
        std::path::Path::new(&bin).is_file(),
        "S3A_CLI_BIN={bin} is not a file"
    );
    let fx = build_hyper(
        vec![
            HyperTile::Manifest("at10 manifest".into()),
            HyperTile::Consent(vec![grant(subject(0x11), NOW, 0)]),
            HyperTile::Raw(68, b"blob-bytes".to_vec(), 1),
        ],
        true,
    );
    let path = tmp_path("at10");
    std::fs::write(&path, &fx.bytes).expect("write fixture");

    let out = Command::new(&bin)
        .args(["tour", "--json", path.to_str().unwrap()])
        .output()
        .expect("run s3a-cli tour --json");
    assert!(
        out.status.success(),
        "s3a-cli tour --json failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("parse tour --json output");
    assert_eq!(report["ok"], true, "tour report ok: {report}");
    let floors = report["floors"]
        .as_array()
        .expect("tour report must have a floors list");
    assert_eq!(floors.len(), 3, "tour floors: {report}");

    let ctx = test_context();
    let h = open(&ctx, &fx.bytes, None, MAX_FRAME_PIXELS, MAX_OPEN_BYTES, vec![])
        .expect("open");
    proceed_to_warehouse(&h).expect("proceed");
    let wh = render_warehouse(&h, CSS_WIDTH, CSS_HEIGHT, 0.0, 0.0, DPR).expect("warehouse");
    let pack_floors = parse_pack_floors(&text_layer(&wh));
    assert_eq!(pack_floors.len(), 3, "pack floors: {}", text_layer(&wh));

    for (i, cli) in floors.iter().enumerate() {
        let (pi, pt, pr, ps, ph) = &pack_floors[i];
        let (pi, pt, pr, ps) = (*pi, *pt, *pr, *ps);
        assert_eq!(cli["index"].as_u64().unwrap() as u32, pi, "floor {i} index");
        assert_eq!(cli["type_id"].as_u64().unwrap() as u32, pt, "floor {i} type_id");
        assert_eq!(
            cli["record_count"].as_u64().unwrap() as u32,
            pr,
            "floor {i} record_count"
        );
        assert_eq!(
            cli["size_bytes"].as_u64().unwrap(),
            ps,
            "floor {i} size_bytes"
        );
        let cli_hash = cli["payload_hash_short"].as_str().unwrap();
        let cli_hash = cli_hash.strip_suffix("...").unwrap_or(cli_hash);
        assert_eq!(cli_hash, ph, "floor {i} payload_hash_short");
    }
}
