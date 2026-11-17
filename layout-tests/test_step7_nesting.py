#!/usr/bin/env python3
"""Fail-first acceptance tests for build-plan step 7 (F-14 nesting-depth bound).

YAML front matter and page HTML/Markdown must fail closed above an
implementation nesting limit. Deeply nested YAML, Markdown, and HTML
seeds join the AT-71 corpus. These tests must fail on the post-step-6
(v24.3) tree because that wording and those seeds are not there yet.
Do not edit them to make them pass. They do not require pack
open/render or an s3a-engine API; parser enforcement lands with the
later open/render steps and uses these seeds.
"""

from __future__ import annotations

import hashlib
import re
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
SPEC = REPO_ROOT / "SPEC.md"
GOLDFISH = Path("/workspace/goldfish/s3a-kite.md")

# cargo-fuzz default corpus path for the AT-71 target `open_and_render`.
# The target and corpus-replay CI job land in step 13; the F-14 seeds
# are checked in now so that job can replay them.
CORPUS = REPO_ROOT / "fuzz" / "corpus" / "open_and_render"
YAML_SEED = CORPUS / "deeply_nested_yaml_front_matter.md"
MARKDOWN_SEED = CORPUS / "deeply_nested_markdown.md"
HTML_SEED = CORPUS / "deeply_nested_html.html"

# Test-fixture depth only: deep enough to overflow a recursive parser or
# layout pass. This is not an implementation limit and is not a SPEC default.
MIN_SEED_DEPTH = 4096


def _spec() -> str:
    return SPEC.read_text(encoding="utf-8")


def _contains(test: unittest.TestCase, needle: str, text: str) -> None:
    test.assertTrue(needle in text, f"SPEC.md is missing: {needle}")


def _yaml_front_matter_depth(text: str) -> int:
    """Depth of a `{` / `[` flow nest inside the first YAML front-matter block."""
    if not text.startswith("---"):
        return 0
    rest = text[3:]
    # Second delimiter starts a new line: \n---
    end = rest.find("\n---")
    if end < 0:
        return 0
    body = rest[:end]
    depth = 0
    max_depth = 0
    for ch in body:
        if ch in "{[":
            depth += 1
            if depth > max_depth:
                max_depth = depth
        elif ch in "}]":
            depth = max(0, depth - 1)
    return max_depth


def _markdown_blockquote_depth(text: str) -> int:
    """Maximum run of leading blockquote markers on any line."""
    max_depth = 0
    for raw in text.splitlines():
        depth = 0
        i = 0
        n = len(raw)
        while i < n:
            if raw[i] == ">":
                depth += 1
                i += 1
                if i < n and raw[i] == " ":
                    i += 1
            else:
                break
        if depth > max_depth:
            max_depth = depth
    return max_depth


def _html_element_depth(text: str) -> int:
    """Maximum nesting of well-formed <tag>...</tag> elements, ignoring closing."""
    depth = 0
    max_depth = 0
    for m in re.finditer(r"</?([A-Za-z][A-Za-z0-9]*)\b[^>]*>", text):
        token = m.group(0)
        if token.startswith("</"):
            depth = max(0, depth - 1)
        elif token.endswith("/>"):
            continue
        else:
            depth += 1
            if depth > max_depth:
                max_depth = depth
    return max_depth


class Step7NestingDepthBoundTests(unittest.TestCase):
    def test_01_spec_is_v24_4(self) -> None:
        text = _spec()
        self.assertTrue(SPEC.is_file())
        _contains(self, "**Status:** v24.4", text)
        _contains(self, "the whole v24.4 contract", text)

    def test_02_goldfish_copy_is_identical_when_present(self) -> None:
        if not GOLDFISH.is_file():
            self.skipTest(f"{GOLDFISH} is not present in this environment")
        self.assertEqual(
            hashlib.sha256(SPEC.read_bytes()).digest(),
            hashlib.sha256(GOLDFISH.read_bytes()).digest(),
            "SPEC.md must be byte-identical to /workspace/goldfish/s3a-kite.md",
        )

    def test_03_f14_yaml_and_page_parsing_fail_closed_above_nesting_limit(self) -> None:
        text = _spec()
        # Goldfish round-24 F-14: fail closed above an implementation
        # nesting limit. No numeric cap is asserted here (none is specified).
        _contains(self, "implementation nesting limit", text)
        _contains(self, "fail closed", text)
        _contains(
            self,
            "YAML front matter and page HTML/Markdown parsing fail closed above an implementation nesting limit",
            text,
        )
        _contains(
            self,
            "a dossier whose front matter exceeds that limit is omitted from the list and no paper id is written to logs",
            text,
        )
        _contains(
            self,
            "`render_page` of a page whose HTML or Markdown exceeds that limit is a page error",
            text,
        )

    def test_04_at_71_names_the_nested_yaml_markdown_and_html_seeds(self) -> None:
        text = _spec()
        _contains(
            self,
            "including the AT-08 fixture, MICRO-VEC-01/02/03, a malformed file, and deeply nested YAML, Markdown, and HTML seeds",
            text,
        )
        # No dedicated new AT number for F-14; the seeds join AT-71.
        self.assertIn("**AT-71**", text)

    def test_05_at_71_corpus_contains_deeply_nested_yaml_markdown_and_html_seeds(self) -> None:
        self.assertTrue(
            CORPUS.is_dir(),
            f"missing AT-71 seed corpus directory: {CORPUS}",
        )
        self.assertTrue(YAML_SEED.is_file(), f"missing YAML seed: {YAML_SEED}")
        self.assertTrue(
            MARKDOWN_SEED.is_file(), f"missing Markdown seed: {MARKDOWN_SEED}"
        )
        self.assertTrue(HTML_SEED.is_file(), f"missing HTML seed: {HTML_SEED}")

        yaml_text = YAML_SEED.read_text(encoding="utf-8")
        md_text = MARKDOWN_SEED.read_text(encoding="utf-8")
        html_text = HTML_SEED.read_text(encoding="utf-8")

        yaml_depth = _yaml_front_matter_depth(yaml_text)
        self.assertGreaterEqual(
            yaml_depth,
            MIN_SEED_DEPTH,
            f"{YAML_SEED.name} YAML front-matter nesting is {yaml_depth}, need >= {MIN_SEED_DEPTH}",
        )
        md_depth = _markdown_blockquote_depth(md_text)
        self.assertGreaterEqual(
            md_depth,
            MIN_SEED_DEPTH,
            f"{MARKDOWN_SEED.name} Markdown blockquote nesting is {md_depth}, need >= {MIN_SEED_DEPTH}",
        )
        html_depth = _html_element_depth(html_text)
        self.assertGreaterEqual(
            html_depth,
            MIN_SEED_DEPTH,
            f"{HTML_SEED.name} HTML element nesting is {html_depth}, need >= {MIN_SEED_DEPTH}",
        )


if __name__ == "__main__":
    unittest.main(verbosity=2)
