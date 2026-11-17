#!/usr/bin/env python3
"""Fail-first acceptance tests for build-plan step 3 (SPEC.md v24.1, §11, DD-121).

Create the s3a-pack-kite skeleton at crates/s3a-pack-kite, with KiteContext,
KiteError, and crate types rlib/cdylib/staticlib. The crate is not a workspace
member yet (DD-121). AT-42 lives in kite-pack-tests. No CJK/emoji font packs
(Q-05, DD-35). No s3a-core → kite/kitesurf edge (AT-01).

These tests must fail on the post-step-2 tree (the pack crate is absent).
Do not edit them to make them pass.
"""

from __future__ import annotations

import subprocess
import tomllib
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
WORKSPACE_TOML = REPO_ROOT / "Cargo.toml"
PACK_DIR = REPO_ROOT / "crates" / "s3a-pack-kite"
PACK_MANIFEST = PACK_DIR / "Cargo.toml"
PACK_TESTS_MANIFEST = REPO_ROOT / "kite-pack-tests" / "Cargo.toml"
WORKFLOWS_DIR = REPO_ROOT / ".github" / "workflows"
KITE_ACCEPTANCE = REPO_ROOT / "kite-acceptance" / "Cargo.toml"


def _load_toml(path: Path) -> dict:
    with path.open("rb") as fh:
        return tomllib.load(fh)


def _dep_tables(manifest: dict) -> list[dict]:
    tables = []
    for key in ("dependencies", "dev-dependencies", "build-dependencies"):
        block = manifest.get(key) or {}
        if isinstance(block, dict):
            tables.append(block)
    for tgt in (manifest.get("target") or {}).values():
        if not isinstance(tgt, dict):
            continue
        for key in ("dependencies", "dev-dependencies", "build-dependencies"):
            block = tgt.get(key) or {}
            if isinstance(block, dict):
                tables.append(block)
    return tables


def _has_dep(manifest: dict, name: str) -> bool:
    return any(name in table for table in _dep_tables(manifest) if isinstance(table, dict))


def _crate_types(manifest: dict) -> list[str]:
    lib = manifest.get("lib") or {}
    types = lib.get("crate-type")
    if types is None:
        return ["rlib"]
    if isinstance(types, str):
        return [types]
    return list(types)


def _workflow_text() -> str:
    if not WORKFLOWS_DIR.is_dir():
        return ""
    bodies = []
    for path in sorted(WORKFLOWS_DIR.glob("*.yml")) + sorted(WORKFLOWS_DIR.glob("*.yaml")):
        bodies.append(path.read_text(encoding="utf-8"))
    return "\n".join(bodies)


def _run(args: list[str], cwd: Path) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, capture_output=True, text=True, check=False)


class PackSkeletonTests(unittest.TestCase):
    def test_01_pack_crate_exists_with_required_crate_types(self) -> None:
        # AT-01: the s3a-pack-kite package exists and is testable.
        self.assertTrue(
            PACK_MANIFEST.is_file(),
            f"missing pack crate manifest: {PACK_MANIFEST}",
        )
        data = _load_toml(PACK_MANIFEST)
        self.assertEqual(data.get("package", {}).get("name"), "s3a-pack-kite")
        types = _crate_types(data)
        for required in ("rlib", "cdylib", "staticlib"):
            self.assertIn(
                required,
                types,
                f"{PACK_MANIFEST} must include crate-type {required} (§5.1)",
            )

    def test_02_pack_crate_is_out_of_the_workspace(self) -> None:
        # DD-121 / BUILD-PLAN step 3: the pack is not a workspace member
        # until it exists and is ready to join. This skeleton stays excluded
        # so cargo commands at the workspace root stay green.
        self.assertTrue(WORKSPACE_TOML.is_file(), f"missing {WORKSPACE_TOML}")
        workspace = _load_toml(WORKSPACE_TOML)["workspace"]
        members = list(workspace.get("members") or [])
        exclude = list(workspace.get("exclude") or [])
        self.assertNotIn("crates/s3a-pack-kite", members)
        self.assertIn(
            "crates/s3a-pack-kite",
            exclude,
            "crates/s3a-pack-kite must be excluded from the root workspace (DD-121)",
        )
        proc = _run(
            ["cargo", "metadata", "--format-version", "1", "--no-deps"],
            REPO_ROOT,
        )
        self.assertEqual(
            proc.returncode,
            0,
            "cargo metadata at the workspace root failed with the pack crate "
            f"present:\n{proc.stderr}",
        )

    def test_03_pack_does_not_create_an_s3a_core_to_kite_edge(self) -> None:
        # AT-01: cargo tree -p s3a-core has no kite/kitesurf edge.
        # s3a-core lives in s3a-storage, which is not this repo and must not
        # be touched in this step. Check the local manifests instead: no
        # crate named s3a-core is introduced, and no workspace member gains
        # a kite / kitesurf / s3a-pack-kite dependency.
        for manifest_path in REPO_ROOT.glob("**/Cargo.toml"):
            if "target" in manifest_path.parts or "vendor" in manifest_path.parts:
                continue
            data = _load_toml(manifest_path)
            name = (data.get("package") or {}).get("name")
            if name == "s3a-core":
                self.fail(
                    f"{manifest_path} declares package s3a-core; AT-01 forbids "
                    "an s3a-core → kite/kitesurf edge, and this repo must not "
                    "add s3a-core"
                )
        self.assertTrue(KITE_ACCEPTANCE.is_file())
        acceptance = _load_toml(KITE_ACCEPTANCE)
        for forbidden in ("s3a-pack-kite", "kitesurf-core", "kitesurf", "s3a-kite"):
            self.assertFalse(
                _has_dep(acceptance, forbidden),
                f"kite-acceptance must not depend on {forbidden} (DD-121, AT-01)",
            )
        proc = _run(
            ["cargo", "tree", "--workspace", "--prefix", "none"],
            REPO_ROOT,
        )
        self.assertEqual(proc.returncode, 0, f"cargo tree --workspace failed:\n{proc.stderr}")
        for line in proc.stdout.splitlines():
            lower = line.lower()
            self.assertNotIn("kitesurf", lower, f"workspace cargo tree has a kitesurf edge: {line}")
            self.assertNotIn("s3a-pack-kite", lower, f"workspace cargo tree has a pack edge: {line}")

    def test_04_pack_crate_and_kite_pack_tests_cargo_test_succeed(self) -> None:
        # AT-01: cargo test of the pack crate exists. The pack is out of the
        # workspace, so the test uses --manifest-path rather than -p.
        # AT-42 lives in kite-pack-tests (DD-121 pack-linking crate).
        self.assertTrue(PACK_MANIFEST.is_file(), f"missing {PACK_MANIFEST}")
        self.assertTrue(PACK_TESTS_MANIFEST.is_file(), f"missing {PACK_TESTS_MANIFEST}")
        pack_tests = _load_toml(PACK_TESTS_MANIFEST)
        self.assertTrue(
            _has_dep(pack_tests, "s3a-pack-kite"),
            "kite-pack-tests must path-depend on s3a-pack-kite once the pack exists (DD-121)",
        )
        for args, label in (
            (["cargo", "test", "--manifest-path", str(PACK_MANIFEST), "--", "--test-threads=1"], "s3a-pack-kite"),
            (["cargo", "test", "--manifest-path", str(PACK_TESTS_MANIFEST), "--", "--test-threads=1"], "kite-pack-tests"),
        ):
            proc = _run(args, REPO_ROOT)
            self.assertEqual(
                proc.returncode,
                0,
                f"cargo test for {label} failed (AT-01 / AT-42):\n"
                f"{proc.stdout}\n{proc.stderr}",
            )

    def test_05_ci_runs_pack_and_pack_linking_tests(self) -> None:
        text = _workflow_text()
        self.assertIn("layout-tests", text, "CI must still run the layout tests")
        self.assertIn(
            "s3a-pack-kite",
            text,
            "CI must run cargo test for crates/s3a-pack-kite (AT-01)",
        )
        self.assertIn(
            "kite-pack-tests",
            text,
            "CI must run cargo test for kite-pack-tests (AT-42, DD-121)",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
