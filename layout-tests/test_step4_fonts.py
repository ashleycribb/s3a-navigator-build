#!/usr/bin/env python3
"""Fail-first tests for the v24.2 required CJK and emoji UA font packs.

Build-plan step 4 (Q-05 / DD-35 reversed): Noto Sans SC and monochrome
Noto Emoji are required UA fonts, with license files and NOTICE entries.
These tests must fail until the pack required-set and the emoji asset
files are added. Do not edit them to make them pass.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CONTEXT_RS = REPO_ROOT / "crates" / "s3a-pack-kite" / "src" / "context.rs"
CJK_WOFF2 = (
    REPO_ROOT
    / "vendor"
    / "s3a-Navigator"
    / "packages"
    / "engine"
    / "public"
    / "fonts"
    / "NotoSansCJKsc-Regular.woff2"
)
CJK_OFL = REPO_ROOT / "vendor" / "s3a-Navigator" / "licenses" / "OFL-1.1-Noto-SC.txt"
VENDOR_NOTICE = REPO_ROOT / "vendor" / "s3a-Navigator" / "NOTICE"
EMOJI_TTF = REPO_ROOT / "assets" / "ua-fonts" / "NotoEmoji[wght].ttf"
EMOJI_OFL = REPO_ROOT / "assets" / "ua-fonts" / "OFL-1.1-Noto-Emoji.txt"
EMOJI_NOTICE = REPO_ROOT / "assets" / "ua-fonts" / "NOTICE"

REQUIRED_UA_FONTS = [
    "Noto Sans",
    "Noto Sans Italic",
    "Noto Serif",
    "Noto Sans Mono",
    "Noto Sans Symbols",
    "Noto Sans SC",
    "Noto Emoji",
]

CJK_SIZE = 6_093_444
EMOJI_SIZE = 1_982_596


def _required_ua_fonts_from_pack() -> list[str]:
    text = CONTEXT_RS.read_text(encoding="utf-8")
    match = re.search(
        r"pub const REQUIRED_UA_FONTS: &\[&str\] = &\[(.*?)\];",
        text,
        flags=re.S,
    )
    if not match:
        return []
    return re.findall(r'"([^"]+)"', match.group(1))


class Step4RequiredFontPackTests(unittest.TestCase):
    def test_01_pack_required_set_includes_cjk_and_emoji(self) -> None:
        self.assertTrue(CONTEXT_RS.is_file(), f"missing {CONTEXT_RS}")
        names = _required_ua_fonts_from_pack()
        self.assertEqual(
            names,
            REQUIRED_UA_FONTS,
            "crates/s3a-pack-kite REQUIRED_UA_FONTS must be the v24.2 required set, "
            "including Noto Sans SC and Noto Emoji (Q-05 reversed)",
        )

    def test_02_cjk_font_license_and_vendor_notice_are_present(self) -> None:
        self.assertTrue(CJK_WOFF2.is_file(), f"missing required Noto Sans SC file: {CJK_WOFF2}")
        self.assertEqual(
            CJK_WOFF2.stat().st_size,
            CJK_SIZE,
            f"{CJK_WOFF2} size must be the measured {CJK_SIZE} bytes recorded in SPEC v24.2",
        )
        self.assertTrue(CJK_OFL.is_file(), f"missing Noto Sans SC OFL text: {CJK_OFL}")
        ofl = CJK_OFL.read_text(encoding="utf-8")
        self.assertIn("SIL Open Font License", ofl)
        self.assertTrue(VENDOR_NOTICE.is_file(), f"missing {VENDOR_NOTICE}")
        notice = VENDOR_NOTICE.read_text(encoding="utf-8")
        self.assertIn("Noto Sans SC", notice)
        self.assertIn("OFL-1.1-Noto-SC.txt", notice)

    def test_03_monochrome_emoji_font_is_present_at_measured_size(self) -> None:
        self.assertTrue(
            EMOJI_TTF.is_file(),
            f"missing required monochrome Noto Emoji file: {EMOJI_TTF}",
        )
        self.assertEqual(
            EMOJI_TTF.stat().st_size,
            EMOJI_SIZE,
            f"{EMOJI_TTF} size must be the measured {EMOJI_SIZE} bytes recorded in SPEC v24.2",
        )
        data = EMOJI_TTF.read_bytes()
        self.assertTrue(
            data.startswith(b"\x00\x01\x00\x00") or data.startswith(b"true"),
            f"{EMOJI_TTF} must be a TrueType font",
        )
        # Monochrome: parse the sfnt table directory; don't fake COLR/CBDT support.
        import struct
        self.assertGreaterEqual(len(data), 12, f"{EMOJI_TTF} is too short to be an sfnt font")
        num_tables = struct.unpack(">H", data[4:6])[0]
        tags = []
        for i in range(num_tables):
            off = 12 + 16 * i
            self.assertLessEqual(off + 4, len(data), f"{EMOJI_TTF} table directory is truncated")
            tags.append(data[off:off + 4])
        for tag in (b"COLR", b"CPAL", b"CBDT", b"CBLC", b"sbix", b"SVG "):
            self.assertNotIn(
                tag,
                tags,
                f"{EMOJI_TTF} must be monochrome; found color table {tag!r} in the sfnt directory",
            )
        self.assertIn(b"glyf", tags, f"{EMOJI_TTF} must be a monochrome outline font (glyf table)")

    def test_04_emoji_license_and_notice_are_present(self) -> None:
        self.assertTrue(EMOJI_OFL.is_file(), f"missing Noto Emoji OFL text: {EMOJI_OFL}")
        ofl = EMOJI_OFL.read_text(encoding="utf-8")
        self.assertIn("SIL Open Font License", ofl)
        self.assertIn("Copyright 2013 Google LLC", ofl)
        self.assertTrue(EMOJI_NOTICE.is_file(), f"missing {EMOJI_NOTICE}")
        notice = EMOJI_NOTICE.read_text(encoding="utf-8")
        self.assertIn("Noto Emoji", notice)
        self.assertIn("SIL Open Font License 1.1", notice)
        self.assertIn("monochrome", notice.lower())
        self.assertIn("google/fonts", notice)
        self.assertIn("ofl/notoemoji", notice)
        self.assertIn("Noto Sans SC", notice)
        self.assertIn("OFL-1.1-Noto-SC.txt", notice)
        self.assertIn("6,093,444", notice)
        self.assertIn("1,982,596", notice)
        # Size impact on budgets is not invented here.
        self.assertIn("Q-06", notice)
        self.assertIn("Q-17", notice)


if __name__ == "__main__":
    unittest.main(verbosity=2)
