#!/usr/bin/env python3
"""Fail-first layout tests for build-plan step 1 (SPEC.md v24.1, DD-121, §11.1).

Assert the cargo workspace scaffold against the Navigator fork pin, without
requiring the pack or a vendored tree to exist yet. These tests must fail on
the pre-scaffold tree. Do not edit them to make them pass.
"""

from __future__ import annotations

import tomllib
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PIN_PATH = REPO_ROOT / "vendor" / "NAVIGATOR_PIN"
PIN_URL = "https://github.com/ashleycribb/s3a-Navigator"
PIN_COMMIT = "235111c6fe10f2d63f3476bf5e3d19b4c7089415"
WORKSPACE_TOML = REPO_ROOT / "Cargo.toml"
KITE_ACCEPTANCE = REPO_ROOT / "kite-acceptance" / "Cargo.toml"
KITE_PACK_TESTS_TOML = REPO_ROOT / "kite-pack-tests" / "Cargo.toml"
KITE_PACK_TESTS_LOCK = REPO_ROOT / "kite-pack-tests" / "Cargo.lock"
HOST_CRATE = REPO_ROOT / "crates" / "s3a-kite" / "Cargo.toml"
PACK_CRATE = REPO_ROOT / "crates" / "s3a-pack-kite"
READER_TESTS = REPO_ROOT / "reader-pack-tests" / "Cargo.toml"


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
    for table in _dep_tables(manifest):
        if name in table:
            return True
    return False


class WorkspaceLayoutTests(unittest.TestCase):
    def test_01_workspace_manifest_exists(self) -> None:
        self.assertTrue(
            WORKSPACE_TOML.is_file(),
            f"missing workspace manifest: {WORKSPACE_TOML}",
        )

    def test_02_root_is_a_workspace_with_required_members_and_exclusions(self) -> None:
        self.assertTrue(WORKSPACE_TOML.is_file(), f"missing {WORKSPACE_TOML}")
        data = _load_toml(WORKSPACE_TOML)
        self.assertIn("workspace", data, f"{WORKSPACE_TOML} is not a cargo workspace")
        workspace = data["workspace"]
        members = list(workspace.get("members") or [])
        exclude = list(workspace.get("exclude") or [])
        self.assertIn(
            "kite-acceptance",
            members,
            "kite-acceptance must be a workspace member (DD-121)",
        )
        for forbidden in ("kite-pack-tests", "crates/s3a-kite", "crates/s3a-pack-kite"):
            self.assertNotIn(
                forbidden,
                members,
                f"{forbidden} must not be a workspace member while the pack is absent (DD-121)",
            )
        self.assertIn("crates/s3a-kite", exclude, "crates/s3a-kite must be excluded from the workspace")
        self.assertIn("kite-pack-tests", exclude, "kite-pack-tests must be excluded from the workspace")

    def test_03_kite_acceptance_exists_and_has_no_pack_dependency(self) -> None:
        self.assertTrue(
            KITE_ACCEPTANCE.is_file(),
            f"missing kite-acceptance manifest: {KITE_ACCEPTANCE}",
        )
        data = _load_toml(KITE_ACCEPTANCE)
        self.assertIn("package", data, "kite-acceptance must be a package")
        self.assertEqual(data["package"].get("name"), "kite-acceptance")
        self.assertFalse(
            _has_dep(data, "s3a-pack-kite"),
            "kite-acceptance must not depend on s3a-pack-kite (DD-121)",
        )

    def test_04_kite_pack_tests_is_out_of_workspace_with_own_lockfile(self) -> None:
        self.assertTrue(
            KITE_PACK_TESTS_TOML.is_file(),
            f"missing out-of-workspace pack-test manifest: {KITE_PACK_TESTS_TOML}",
        )
        self.assertTrue(
            KITE_PACK_TESTS_LOCK.is_file(),
            f"missing out-of-workspace pack-test lockfile: {KITE_PACK_TESTS_LOCK}",
        )
        data = _load_toml(KITE_PACK_TESTS_TOML)
        self.assertIn("package", data)
        self.assertEqual(data["package"].get("name"), "kite-pack-tests")
        self.assertNotIn(
            "workspace",
            data,
            "kite-pack-tests must have its own manifest and must not declare a workspace",
        )

    def test_05_host_crate_exists_excluded_and_has_no_pack_dependency(self) -> None:
        self.assertTrue(HOST_CRATE.is_file(), f"missing host crate manifest: {HOST_CRATE}")
        data = _load_toml(HOST_CRATE)
        self.assertIn("package", data)
        self.assertEqual(data["package"].get("name"), "s3a-kite")
        self.assertFalse(
            _has_dep(data, "s3a-pack-kite"),
            "crates/s3a-kite must have no pack dependency until the pack exists (DD-121)",
        )

    def test_06_pack_crate_is_absent_in_step_1(self) -> None:
        self.assertFalse(
            PACK_CRATE.exists(),
            "crates/s3a-pack-kite must not be created in step 1 (vendoring and the pack crate are later steps)",
        )

    def test_07_vendor_pin_file_records_the_navigator_fork(self) -> None:
        self.assertTrue(PIN_PATH.is_file(), f"missing vendor pin file: {PIN_PATH}")
        body = PIN_PATH.read_text(encoding="utf-8")
        self.assertIn(PIN_URL, body, f"{PIN_PATH} must name {PIN_URL}")
        self.assertIn(PIN_COMMIT, body, f"{PIN_PATH} must pin {PIN_COMMIT}")
        # The vendored tree itself is step 2, not step 1.
        self.assertFalse(
            (REPO_ROOT / "vendor" / "s3a-Navigator" / "Cargo.toml").is_file(),
            "do not vendor the fork in step 1; only the pin file belongs here",
        )

    def test_08_reader_pack_tests_crate_path_is_reserved(self) -> None:
        self.assertTrue(
            READER_TESTS.is_file(),
            f"missing out-of-workspace reader test crate: {READER_TESTS} "
            "(AT-38, AT-39, AT-43, AT-63, AT-165 live outside the s3a-reader workspace)",
        )
        data = _load_toml(READER_TESTS)
        self.assertIn("package", data)
        self.assertEqual(data["package"].get("name"), "reader-pack-tests")


if __name__ == "__main__":
    unittest.main(verbosity=2)
