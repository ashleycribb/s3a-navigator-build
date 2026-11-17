#!/usr/bin/env python3
"""Fail-first acceptance tests for build-plan step 2 (SPEC.md v24.1, §11.2).

Vendor the Navigator / Kitesurf Rust core at the pin in vendor/NAVIGATOR_PIN,
add LICENSE/NOTICE, enable native-host, fix getrandom/wasm_bindgen for native
and Android targets, keep Apache §4 change notices (AT-02, AT-03), and add a
GitHub Actions workflow that runs these layout tests plus cargo check/test on
the workspace.

AT-70's generated third-party inventory is deferred until a distributable
host-directory or mobile artifact exists, as BUILD-PLAN.md allows.

These tests must fail on the post-step-1 tree (no vendored core, no CI
workflow). Do not edit them to make them pass.
"""

from __future__ import annotations

import hashlib
import os
import re
import subprocess
import tomllib
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PIN_PATH = REPO_ROOT / "vendor" / "NAVIGATOR_PIN"
PIN_MANIFEST = REPO_ROOT / "vendor" / "NAVIGATOR_PIN.sha256"
PIN_URL = "https://github.com/ashleycribb/s3a-Navigator"
PIN_COMMIT = "235111c6fe10f2d63f3476bf5e3d19b4c7089415"
CHANGE_NOTICE = (
    "changed from upstream kitesurf "
    "235111c6fe10f2d63f3476bf5e3d19b4c7089415"
)
VENDOR_ROOT = REPO_ROOT / "vendor" / "s3a-Navigator"
VENDOR_WORKSPACE = VENDOR_ROOT / "Cargo.toml"
CORE_MANIFEST = VENDOR_ROOT / "packages" / "core" / "Cargo.toml"
RENDERER_MANIFEST = VENDOR_ROOT / "packages" / "renderer-core" / "Cargo.toml"
LICENSE_PATH = VENDOR_ROOT / "LICENSE"
NOTICE_PATH = VENDOR_ROOT / "NOTICE"
LICENSES_DIR = VENDOR_ROOT / "licenses"
WORKSPACE_TOML = REPO_ROOT / "Cargo.toml"
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"
GETRANDOM_JS_FEATURES = frozenset({"js", "wasm_js"})


def _load_toml(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def _sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _load_pin_manifest(path: Path) -> dict[str, str]:
    """Parse `sha256  relative/path` lines into {path: sha256}."""
    mapping: dict[str, str] = {}
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2:
            raise ValueError(f"malformed pin manifest line: {raw!r}")
        digest, rel = parts
        if len(digest) != 64 or any(c not in "0123456789abcdef" for c in digest):
            raise ValueError(f"malformed sha256 in pin manifest: {raw!r}")
        mapping[rel] = digest
    return mapping


def _is_probably_text(path: Path) -> bool:
    suffix = path.suffix.lower()
    if suffix in {
        ".png",
        ".jpg",
        ".jpeg",
        ".gif",
        ".webp",
        ".ttf",
        ".otf",
        ".woff",
        ".woff2",
        ".ico",
        ".pdf",
        ".bin",
        ".wasm",
        ".lock",
    }:
        return False
    try:
        data = path.read_bytes()
    except OSError:
        return False
    if b"\0" in data[:4096]:
        return False
    return True


def _crate_types(manifest: dict) -> list[str]:
    lib = manifest.get("lib") or {}
    types = lib.get("crate-type")
    if types is None:
        return ["rlib"]
    if isinstance(types, str):
        return [types]
    return list(types)


def _workspace_getrandom_entries(manifest: dict) -> list[tuple[str, dict]]:
    deps = (manifest.get("workspace") or {}).get("dependencies") or {}
    found = []
    for name, spec in deps.items():
        if not isinstance(spec, dict):
            continue
        pkg = spec.get("package", name)
        if pkg == "getrandom" or name in {"getrandom", "getrandom_03"}:
            found.append((name, spec))
    return found


def _spec_features(spec: dict) -> set[str]:
    features = spec.get("features") or []
    return set(features) if isinstance(features, list) else set()


def _target_tables(manifest: dict) -> dict[str, dict]:
    target = manifest.get("target") or {}
    return target if isinstance(target, dict) else {}


def _dep_is_unconditional(manifest: dict, name: str) -> bool:
    deps = manifest.get("dependencies") or {}
    return isinstance(deps, dict) and name in deps


def _workflow_text() -> str:
    if not WORKFLOWS_DIR.is_dir():
        return ""
    bodies = []
    for path in sorted(WORKFLOWS_DIR.glob("*.yml")) + sorted(
        WORKFLOWS_DIR.glob("*.yaml")
    ):
        bodies.append(path.read_text(encoding="utf-8"))
    return "\n".join(bodies)


class VendorTreeTests(unittest.TestCase):
    def test_01_vendored_rust_core_is_present_at_the_pin_path(self) -> None:
        self.assertTrue(
            VENDOR_WORKSPACE.is_file(),
            f"missing vendored workspace manifest: {VENDOR_WORKSPACE}",
        )
        self.assertTrue(
            CORE_MANIFEST.is_file(),
            f"missing vendored kitesurf-core manifest: {CORE_MANIFEST}",
        )
        self.assertTrue(
            RENDERER_MANIFEST.is_file(),
            f"missing vendored renderer-core manifest: {RENDERER_MANIFEST}",
        )
        data = _load_toml(VENDOR_WORKSPACE)
        self.assertIn("workspace", data, f"{VENDOR_WORKSPACE} is not a cargo workspace")
        members = list((data["workspace"].get("members") or []))
        self.assertIn("packages/core", members)
        self.assertIn("packages/renderer-core", members)
        core = _load_toml(CORE_MANIFEST)
        renderer = _load_toml(RENDERER_MANIFEST)
        self.assertEqual(core.get("package", {}).get("name"), "kitesurf-core")
        self.assertEqual(renderer.get("package", {}).get("name"), "renderer-core")

    def test_02_license_and_notice_are_present(self) -> None:
        # AT-02: Apache-2.0 LICENSE and Kitesurf NOTICE must be in the tree.
        # A generated cargo-about inventory (AT-70) waits for a distributable
        # artifact; the source texts themselves land with the vendored core.
        self.assertTrue(LICENSE_PATH.is_file(), f"missing {LICENSE_PATH}")
        license_text = LICENSE_PATH.read_text(encoding="utf-8")
        self.assertIn("Apache License", license_text)
        self.assertIn("Version 2.0", license_text)
        self.assertTrue(NOTICE_PATH.is_file(), f"missing {NOTICE_PATH}")
        notice = NOTICE_PATH.read_text(encoding="utf-8")
        self.assertIn("Copyright (c) 2026 Cloudflare, Inc.", notice)
        self.assertIn("stylo", notice.lower())
        self.assertIn("Public Suffix List", notice)
        self.assertIn("Mozilla Public License 2.0", notice)
        self.assertTrue(
            (LICENSES_DIR / "MPL-2.0-PSL.txt").is_file(),
            "NOTICE's PSL MPL text must be present at licenses/MPL-2.0-PSL.txt",
        )
        self.assertTrue(
            (LICENSES_DIR / "IJG-jpeg-encoder-0.7.0.txt").is_file(),
            "IJG text for jpeg-encoder must be present (AT-02)",
        )

    def test_03_vendored_tree_is_excluded_from_the_root_workspace(self) -> None:
        self.assertTrue(WORKSPACE_TOML.is_file(), f"missing {WORKSPACE_TOML}")
        data = _load_toml(WORKSPACE_TOML)
        workspace = data["workspace"]
        members = list(workspace.get("members") or [])
        exclude = list(workspace.get("exclude") or [])
        for forbidden in ("vendor/s3a-Navigator", "vendor"):
            self.assertNotIn(
                forbidden,
                members,
                f"{forbidden} must not be a member of the root workspace",
            )
        self.assertTrue(
            "vendor/s3a-Navigator" in exclude or "vendor" in exclude,
            "the vendored Navigator tree must be excluded from the root workspace",
        )
        # Root cargo metadata must still succeed with the nested workspace present.
        proc = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            proc.returncode,
            0,
            "cargo metadata at the workspace root failed with the vendored tree "
            f"present:\n{proc.stderr}",
        )

    def test_04_native_host_feature_and_rlib_crate_types(self) -> None:
        # DD-02 / §11.2: compile kitesurf-core and renderer-core with
        # native-host as native rlibs.
        for path in (CORE_MANIFEST, RENDERER_MANIFEST):
            self.assertTrue(path.is_file(), f"missing {path}")
            manifest = _load_toml(path)
            features = manifest.get("features") or {}
            self.assertIn(
                "native-host",
                features,
                f"{path} must declare the native-host feature",
            )
            types = _crate_types(manifest)
            self.assertIn(
                "rlib",
                types,
                f"{path} must include crate-type rlib so the pack can link a "
                "native host (DD-02)",
            )

    def test_05_getrandom_js_features_are_not_enabled_for_native_or_android(
        self,
    ) -> None:
        # §5.7 / AT-44: getrandom must not enable js / wasm_js on native or
        # Android targets. Wasm targets may still use those features.
        self.assertTrue(VENDOR_WORKSPACE.is_file(), f"missing {VENDOR_WORKSPACE}")
        manifest = _load_toml(VENDOR_WORKSPACE)
        entries = _workspace_getrandom_entries(manifest)
        self.assertTrue(
            entries,
            "vendored workspace must declare getrandom so the js-feature split "
            "can be checked",
        )
        for name, spec in entries:
            js_features = _spec_features(spec) & GETRANDOM_JS_FEATURES
            self.assertFalse(
                js_features,
                f"workspace.dependencies.{name} enables {sorted(js_features)} "
                "unconditionally; those features must be wasm-target-only",
            )

        # Target-specific tables may enable js / wasm_js only for wasm32.
        for label, manifest in (
            ("workspace", manifest),
            ("kitesurf-core", _load_toml(CORE_MANIFEST) if CORE_MANIFEST.is_file() else {}),
            (
                "renderer-core",
                _load_toml(RENDERER_MANIFEST) if RENDERER_MANIFEST.is_file() else {},
            ),
        ):
            for cfg, table in _target_tables(manifest).items():
                deps = table.get("dependencies") or {}
                if not isinstance(deps, dict):
                    continue
                for name, spec in deps.items():
                    if not isinstance(spec, dict):
                        continue
                    pkg = spec.get("package", name)
                    if pkg != "getrandom" and name not in {"getrandom", "getrandom_03"}:
                        continue
                    js_features = _spec_features(spec) & GETRANDOM_JS_FEATURES
                    if not js_features:
                        continue
                    self.assertIn(
                        "wasm32",
                        cfg,
                        f"{label} target.{cfg} enables getrandom {sorted(js_features)}; "
                        "js / wasm_js are allowed only on wasm32 targets, not native "
                        "or Android",
                    )

        # rustflags cfg getrandom_backend="wasm_js" must stay wasm-only.
        for config in VENDOR_ROOT.rglob(".cargo/config.toml"):
            if "target" in config.parts and "/target/" in str(config):
                continue
            text = config.read_text(encoding="utf-8")
            if "getrandom_backend" not in text:
                continue
            # A config that sets the wasm_js backend outside a wasm32 target
            # table is a failure. Parse roughly by sections.
            current = ""
            for line in text.splitlines():
                stripped = line.strip()
                if stripped.startswith("[") and stripped.endswith("]"):
                    current = stripped
                    continue
                if "getrandom_backend" in stripped and "wasm_js" in stripped:
                    self.assertIn(
                        "wasm32",
                        current,
                        f"{config} sets getrandom_backend=wasm_js under {current or 'top level'}, "
                        "not a wasm32 target table",
                    )

    def test_06_wasm_bindgen_is_not_required_for_native_host(self) -> None:
        # §5.7: native and Android builds must not require a JS host.
        # wasm-bindgen / js-sys / serde-wasm-bindgen must be target-gated to
        # wasm32 or optional and not enabled by native-host.
        js_crates = ("wasm-bindgen", "wasm-bindgen-futures", "js-sys", "serde-wasm-bindgen")
        for path in (CORE_MANIFEST, RENDERER_MANIFEST):
            self.assertTrue(path.is_file(), f"missing {path}")
            manifest = _load_toml(path)
            features = manifest.get("features") or {}
            native = features.get("native-host")
            # native-host must not pull JS crates back in.
            if isinstance(native, list):
                for item in native:
                    self.assertNotIn(
                        str(item),
                        js_crates,
                        f"{path} native-host feature enables JS crate {item}",
                    )
                    self.assertFalse(
                        str(item).startswith("dep:wasm-bindgen")
                        or str(item).startswith("dep:js-sys")
                        or str(item).startswith("dep:serde-wasm-bindgen"),
                        f"{path} native-host feature enables {item}",
                    )
            deps = manifest.get("dependencies") or {}
            for name in js_crates:
                if name not in deps:
                    continue
                spec = deps[name]
                optional = isinstance(spec, dict) and spec.get("optional") is True
                self.assertTrue(
                    optional,
                    f"{path} depends on {name} unconditionally; gate it on "
                    "wasm32 or make it optional and omit it from native-host",
                )

        # fonts.rs is the §5.7 example: #[wasm_bindgen] must be cfg-gated.
        fonts = VENDOR_ROOT / "packages" / "core" / "src" / "resources" / "fonts.rs"
        self.assertTrue(fonts.is_file(), f"missing {fonts} (native-host wasm_bindgen gate)")
        text = fonts.read_text(encoding="utf-8")
        self.assertIn("wasm_bindgen", text)
        # A bare `use wasm_bindgen...` or un-gated #[wasm_bindgen] on the
        # native-host path is a failure. Require a cfg that excludes native
        # host or non-wasm targets near the import.
        use_pat = re.compile(
            r"^use wasm_bindgen",
            re.MULTILINE,
        )
        for match in use_pat.finditer(text):
            window = text[max(0, match.start() - 400) : match.start()]
            self.assertTrue(
                re.search(
                    r"#\[cfg\([^)]*(native-host|target_arch\s*=\s*\"wasm32\")[^)]*\)\]",
                    window,
                ),
                "packages/core/src/resources/fonts.rs imports wasm_bindgen without "
                "a cfg gate for native-host or wasm32",
            )

    def test_07_modified_upstream_files_carry_the_apache_change_notice(self) -> None:
        # AT-03: diff the vendored tree against the pin; every modified file
        # includes a prominent change notice plus the original copyright.
        self.assertTrue(VENDOR_ROOT.is_dir(), f"missing vendored tree: {VENDOR_ROOT}")
        self.assertTrue(
            PIN_MANIFEST.is_file(),
            f"missing pin sha256 inventory {PIN_MANIFEST} (AT-03 compares the "
            "vendored tree to this pin listing so CI does not need the private "
            "source fork)",
        )
        manifest = _load_pin_manifest(PIN_MANIFEST)
        self.assertTrue(manifest, f"{PIN_MANIFEST} is empty")
        for required in (
            "LICENSE",
            "NOTICE",
            "Cargo.toml",
            "packages/core/Cargo.toml",
            "packages/renderer-core/Cargo.toml",
        ):
            self.assertIn(
                required,
                manifest,
                f"{PIN_MANIFEST} must inventory {required} at pin {PIN_COMMIT}",
            )

        modified: list[Path] = []
        for root, dirs, files in os.walk(VENDOR_ROOT):
            dirs[:] = [d for d in dirs if d not in {".git", "target"}]
            for name in files:
                path = Path(root) / name
                rel = path.relative_to(VENDOR_ROOT).as_posix()
                if rel == PIN_MANIFEST.name:
                    continue
                pinned = manifest.get(rel)
                if pinned is None:
                    # File is not in the pin (added here). Change notices
                    # apply to modified upstream files, not new files.
                    continue
                if _sha256_file(path) != pinned:
                    modified.append(path)

        self.assertTrue(
            modified,
            "step 2 must change the vendored core (native-host / getrandom / "
            "wasm_bindgen); AT-03 expects a non-empty modified-file list",
        )
        missing_notice: list[str] = []
        for path in modified:
            rel = path.relative_to(VENDOR_ROOT).as_posix()
            if not _is_probably_text(path):
                missing_notice.append(
                    f"{rel} (binary file changed from the pin; refuse silent "
                    "binary edits or document them with a sibling notice)"
                )
                continue
            body = path.read_text(encoding="utf-8", errors="replace")
            if CHANGE_NOTICE not in body:
                missing_notice.append(rel)
        self.assertFalse(
            missing_notice,
            "modified vendored files missing the Apache §4 notice "
            f"{CHANGE_NOTICE!r}:\n  " + "\n  ".join(missing_notice),
        )

    def test_08_ci_workflow_runs_layout_tests_and_workspace_cargo(self) -> None:
        self.assertTrue(
            WORKFLOWS_DIR.is_dir(),
            f"missing GitHub Actions workflow directory: {WORKFLOWS_DIR}",
        )
        yaml_files = list(WORKFLOWS_DIR.glob("*.yml")) + list(
            WORKFLOWS_DIR.glob("*.yaml")
        )
        self.assertTrue(yaml_files, f"no workflow files in {WORKFLOWS_DIR}")
        text = _workflow_text()
        self.assertIn(
            "layout-tests",
            text,
            "CI must run the python layout-tests suite",
        )
        self.assertRegex(
            text,
            r"python3(\s+-m\s+unittest|\s+layout-tests)",
            "CI must invoke the layout tests with python3",
        )
        self.assertRegex(
            text,
            r"cargo\s+check",
            "CI must run cargo check for the workspace",
        )
        self.assertRegex(
            text,
            r"cargo\s+test",
            "CI must run cargo test for the workspace",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
