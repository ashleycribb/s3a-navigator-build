#!/usr/bin/env python3
"""Guard tests for SPEC.md v24.2 wording applied in build-plan step 4.

These assert the already-amended contract: goldfish round-24 F-01, F-02,
F-03, F-08, F-12, and Ashley's 2026-11-16 reversal of Q-05 / DD-35.
They are not claimed to fail first (the SPEC amendment landed first).
Fail-first coverage for the font-set change is in test_step4_fonts.py
and kite-pack-tests/tests/at_42.rs.
"""

from __future__ import annotations

import hashlib
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPEC = REPO_ROOT / "SPEC.md"
GOLDFISH = Path("/workspace/goldfish/s3a-kite.md")


def _spec() -> str:
    return SPEC.read_text(encoding="utf-8")


class Step4SpecWordingTests(unittest.TestCase):
    def test_01_spec_is_v24_2(self) -> None:
        text = _spec()
        self.assertTrue(SPEC.is_file())
        self.assertIn("**Status:** v24.2", text)
        self.assertIn("the whole v24.2 contract", text)

    def test_02_goldfish_copy_is_identical_when_present(self) -> None:
        if not GOLDFISH.is_file():
            self.skipTest(f"{GOLDFISH} is not present in this environment")
        self.assertEqual(
            hashlib.sha256(SPEC.read_bytes()).digest(),
            hashlib.sha256(GOLDFISH.read_bytes()).digest(),
            "SPEC.md must be byte-identical to /workspace/goldfish/s3a-kite.md",
        )

    def test_03_q05_reversed_cjk_and_emoji_are_required(self) -> None:
        text = _spec()
        self.assertIn('Reversed later on 2026-11-16 (Ashley; "Add the fonts in the 4th if it will help."', text)
        self.assertIn("CJK and emoji ship as required UA fonts in the first build", text)
        self.assertIn("Noto Sans SC", text)
        self.assertIn("monochrome Noto Emoji", text)
        self.assertIn("SIL OFL 1.1", text)
        self.assertIn("Color-emoji paint support is not established", text)
        self.assertIn("open measurement item (Q-06, Q-17)", text)
        self.assertIn("invents no budget numbers", text)
        # Measured file sizes, not invented budgets.
        self.assertIn("6,093,444", text)
        self.assertIn("1,982,596", text)
        self.assertIn("8,076,040", text)

    def test_04_f01_hyper_tile_invalid_header_is_torn(self) -> None:
        text = _spec()
        self.assertIn("**per-kind** §5.4 / DD-100 **torn predicate**", text)
        self.assertIn("this field does not restate a kind-independent predicate", text)
        self.assertIn(
            "or its header is absent, is erased (`0xFF` fill), or fails header validation, including at an index < `tile_count`",
            text,
        )
        self.assertIn(
            "A **hyper-tile** fixture with a committed tile (index < header `tile_count`) whose header is absent, erased (`0xFF` fill), or fails header validation is treated as torn",
            text,
        )

    def test_05_f02_signature_cutoff_uses_break_index_including_gaps(self) -> None:
        text = _spec()
        self.assertIn(
            "Every tile at or after `break_index` = (highest preceding authentic `tile_index`) + 1 is not a valid floor (gap tiles included)",
            text,
        )

    def test_06_f03_invalid_report_ties_seal_and_crc(self) -> None:
        text = _spec()
        self.assertIn(
            "a hyper-tile report has `quarantine_reason` `seal_trailer` if and only if `seal_status` is `seal_failed`",
            text,
        )
        self.assertIn(
            "a hyper-tile report with `seal_status` `seal_failed` has `status` `quarantined`",
            text,
        )
        self.assertIn("if `quarantine_reason` is `crc`, `tiles_crc_failed` is non-empty", text)
        self.assertIn("`tiles_crc_failed` is a subset of `invalid_tile_indices`", text)
        self.assertIn(
            "if `tiles_crc_failed` is non-empty, `break_index` is `Some` and is `<=` the minimum of `tiles_crc_failed`",
            text,
        )
        self.assertIn("or `quarantine_reason` is `crc` and `tiles_crc_failed` is empty", text)

    def test_07_f08_tile_78_quarantine_and_first_floor(self) -> None:
        text = _spec()
        self.assertIn(
            "A hyper-tile tile 78 is **quarantined** when it is not a valid floor under the §5.4 derivation table",
            text,
        )
        self.assertIn(
            "paint that stored manifest together with the persistent quarantine banner (AT-90)",
            text,
        )
        self.assertIn("**the first tile 78 is also listed as a floor**", text)
        self.assertIn("the first tile 78 is also listed as a floor (AT-10)", text)

    def test_08_f12_nits(self) -> None:
        text = _spec()
        self.assertIn(
            "`GET /v1/candidates`, select, download, skip, and consent routes return `409` `viewer_only`",
            text,
        )
        self.assertIn(
            "Also a working-archive `/v1/open` `(dev, ino)` mismatch against the recorded startup or create identity, and `ELOOP` from a symlink on that open (AT-217)",
            text,
        )
        self.assertIn(
            "If the source table requires a fetch and no **usable** snapshot exists for `(file_uuid, identity)`",
            text,
        )
        self.assertIn(
            "whose step 1 is skipped because a last matching inclusion-index line is already present (not because of the `ADDON_EDU` bit)",
            text,
        )
        self.assertIn(
            "Inside that hold, `lock_failed` and a `write_failed` for a missing, replaced, or unreadable working archive are evaluated before `addon_missing`",
            text,
        )

    def test_09_at42_and_at49_name_required_cjk_and_emoji(self) -> None:
        text = _spec()
        self.assertIn(
            "names the missing required UA fonts: Noto Sans, Noto Sans Italic, Noto Serif, Noto Sans Mono, Noto Sans Symbols, Noto Sans SC, and Noto Emoji (monochrome)",
            text,
        )
        self.assertIn(
            "With the required Noto Sans SC and monochrome Noto Emoji UA fonts present",
            text,
        )
        self.assertNotIn(
            "with Noto Sans SC and Color Emoji absent, opens successfully",
            text,
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
