//! AT-34: `cargo tree` for the pack crate and the host binary includes
//! neither `boa_engine` nor `wasmtime` (no JavaScript engine; DD-44).
//! Host target is asserted; the Android target half is skipped with a
//! printed note when `aarch64-linux-android` is not installed (SPEC:
//! skipped, not failed, on a host-only job).

use std::path::PathBuf;
use std::process::Command;

/// Resolve the cargo binary the way the repo's tests do: $CARGO, then
/// ~/.cargo/bin/cargo, then PATH.
fn cargo_bin() -> String {
    if let Ok(c) = std::env::var("CARGO") {
        if !c.is_empty() {
            return c;
        }
    }
    if let Some(home) = std::env::var_os("HOME") {
        let p = PathBuf::from(home).join(".cargo/bin/cargo");
        if p.is_file() {
            return p.to_string_lossy().into_owned();
        }
    }
    "cargo".to_string()
}

fn repo_root() -> PathBuf {
    // This test file lives at <repo>/kite-acceptance/tests/.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("kite-acceptance parent")
        .to_path_buf()
}

fn cargo_tree(manifest: &PathBuf, package: &str, target: Option<&str>) -> String {
    let mut cmd = Command::new(cargo_bin());
    cmd.arg("tree")
        .arg("--manifest-path")
        .arg(manifest)
        .arg("-p")
        .arg(package);
    if let Some(t) = target {
        cmd.arg("--target").arg(t);
    }
    let out = cmd.output().unwrap_or_else(|e| panic!("cargo tree {package}: {e}"));
    assert!(
        out.status.success(),
        "cargo tree {package} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("cargo tree output is UTF-8")
}

fn assert_no_js_engine(tree: &str, what: &str) {
    for name in ["boa_engine", "wasmtime"] {
        assert!(
            !tree.contains(name),
            "{what} cargo tree contains forbidden JS engine {name}:\n{tree}"
        );
    }
}

fn android_target_installed() -> bool {
    let rustup = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".cargo/bin/rustup"))
        .filter(|p| p.is_file())
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "rustup".to_string());
    let Ok(out) = Command::new(rustup)
        .args(["target", "list", "--installed"])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| l.trim() == "aarch64-linux-android")
}

#[test]
fn at_34_no_js_engine_in_pack_or_host_tree() {
    let root = repo_root();
    let pack_manifest = root.join("crates/s3a-pack-kite/Cargo.toml");
    let host_manifest = root.join("crates/s3a-kite/Cargo.toml");
    assert!(pack_manifest.is_file(), "missing {}", pack_manifest.display());
    assert!(host_manifest.is_file(), "missing {}", host_manifest.display());

    let pack_tree = cargo_tree(&pack_manifest, "s3a-pack-kite", None);
    assert_no_js_engine(&pack_tree, "s3a-pack-kite host");
    let host_tree = cargo_tree(&host_manifest, "s3a-kite", None);
    assert_no_js_engine(&host_tree, "s3a-kite host");

    if android_target_installed() {
        let pack_android = cargo_tree(&pack_manifest, "s3a-pack-kite", Some("aarch64-linux-android"));
        assert_no_js_engine(&pack_android, "s3a-pack-kite android");
        let host_android = cargo_tree(&host_manifest, "s3a-kite", Some("aarch64-linux-android"));
        assert_no_js_engine(&host_android, "s3a-kite android");
    } else {
        println!("AT-34 Android half skipped: aarch64-linux-android target not installed");
    }
}
