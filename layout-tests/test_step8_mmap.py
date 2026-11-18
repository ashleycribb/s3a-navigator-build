#!/usr/bin/env python3
"""Fail-first acceptance tests for build-plan step 8 (first slice).

Hyper-tile and Edge open/render must use MmapReader::from_bytes for every
file type, must not copy the type-82 consent branch or the stride-blind
128 KiB unsealed scanner, and must pin s3a-engine at
2cf2b45fb1a3375735687252d93a612fb6c9f773. These tests must fail on the
post-step-7 tree because that pin, those APIs, and those guards are not
there yet. Do not edit them to make them pass.
"""

from __future__ import annotations

import re
import tomllib
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
PACK_MANIFEST = REPO_ROOT / "crates" / "s3a-pack-kite" / "Cargo.toml"
PACK_SRC = REPO_ROOT / "crates" / "s3a-pack-kite" / "src"
PACK_TESTS = REPO_ROOT / "kite-pack-tests"
STORAGE_PIN = "2cf2b45fb1a3375735687252d93a612fb6c9f773"
STORAGE_GIT = "https://github.com/ashleycribb/s3a-storage"


def _manifest() -> dict:
    return tomllib.loads(PACK_MANIFEST.read_text(encoding="utf-8"))


def _pack_source() -> str:
    parts: list[str] = []
    for path in sorted(PACK_SRC.rglob("*.rs")):
        parts.append(path.read_text(encoding="utf-8"))
    return "\n".join(parts)


def _owned_rust() -> str:
    """Pack-owned source and pack-linking tests, not vendored Kitesurf."""
    parts: list[str] = []
    for root in (PACK_SRC, PACK_TESTS / "tests", PACK_TESTS / "src"):
        if not root.exists():
            continue
        for path in sorted(root.rglob("*.rs")):
            parts.append(path.read_text(encoding="utf-8"))
    return "\n".join(parts)


def _git_dep(manifest: dict, name: str) -> dict:
    deps = manifest.get("dependencies") or {}
    dep = deps.get(name)
    if dep is None:
        raise AssertionError(f"crates/s3a-pack-kite/Cargo.toml has no {name} dependency")
    if isinstance(dep, str):
        raise AssertionError(f"{name} must be a git dependency pinned at {STORAGE_PIN}")
    return dep


class Step8MmapReaderAndScannerGuardTests(unittest.TestCase):
    def test_01_pack_pins_s3a_engine_at_merged_pr_210(self) -> None:
        self.assertTrue(PACK_MANIFEST.is_file(), f"missing {PACK_MANIFEST}")
        dep = _git_dep(_manifest(), "s3a-engine")
        git = dep.get("git", "")
        self.assertTrue(
            git.rstrip("/") in {STORAGE_GIT, STORAGE_GIT + ".git"}
            or git.startswith(STORAGE_GIT),
            f"s3a-engine git must be {STORAGE_GIT}, got {git!r}",
        )
        self.assertEqual(
            dep.get("rev"),
            STORAGE_PIN,
            f"s3a-engine must be pinned at {STORAGE_PIN}",
        )

    def test_02_open_paths_use_mmapreader_from_bytes(self) -> None:
        src = _pack_source()
        self.assertTrue(
            PACK_SRC.is_dir() and any(PACK_SRC.rglob("*.rs")),
            f"missing pack sources under {PACK_SRC}",
        )
        self.assertIn(
            "MmapReader::from_bytes",
            src,
            "pack open must use s3a_engine::MmapReader::from_bytes for every file type (DD-75)",
        )
        # Path mmap is not used in v1.
        self.assertNotIn(
            "MmapReader::open",
            src,
            "pack must not call MmapReader::open (no path mmap in v1, DD-75)",
        )

    def test_03_pack_does_not_copy_type_82_consent_branch(self) -> None:
        src = _pack_source()
        # tour.rs has `else if f.type_id == 82` treating VCS_RESOLUTION as Edge consent.
        prohibited = re.compile(
            r"type_id\s*==\s*82\b|TileType::VCS_RESOLUTION|VCS_RESOLUTION\s+as\s+Edge",
            re.MULTILINE,
        )
        self.assertIsNone(
            prohibited.search(src),
            "pack must not copy the type-82 EdgeConsent mistake from tour.rs",
        )

    def test_04_pack_does_not_use_stride_blind_unsealed_scanner(self) -> None:
        src = _pack_source()
        prohibited = re.compile(
            r"FILE_HEADER_SIZE\s*\+\s*\w+\s*\*\s*HYPER_TILE_SIZE",
            re.MULTILINE,
        )
        self.assertIsNone(
            prohibited.search(src),
            "pack must not copy the stride-blind 128 KiB unsealed walker from tour.rs",
        )

    def test_05_at_36_pack_owned_source_has_no_money_or_ots_write(self) -> None:
        # AT-36: pack-owned source and tests (not vendored Kitesurf) contain no
        # Bitcoin, OpenTimestamps-write, payment, or token-transfer code paths.
        # Reading Edge seal_hash values is allowed.
        owned = _owned_rust()
        self.assertTrue(owned, "expected pack-owned or pack-test Rust sources")
        lowered = owned.lower()
        for needle in (
            "bitcoin",
            "opentimestamps",
            "opentimestamps",
            "token-transfer",
            "token_transfer",
        ):
            self.assertNotIn(
                needle,
                lowered,
                f"AT-36: pack-owned source/tests must not contain {needle!r}",
            )
        # Payment paths; allow the English word only if clearly not a money path
        # by forbidding common crypto-payment identifiers.
        self.assertIsNone(
            re.search(r"\b(payment_url|lightning_invoice|onchain_payment)\b", lowered),
            "AT-36: pack-owned source/tests must not contain payment code paths",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
