#!/usr/bin/env python3
"""Fail-first acceptance tests for build-plan step 5 (SPEC.md v24.3).

Round-24 host and fetch hardening minors: F-04, F-05, F-06, F-07, F-09,
F-10, F-11. These tests must fail on the post-step-4 (v24.2) tree
because the amended wording is not there yet. Do not edit them to make
them pass. They do not require pack open/render or an s3a-engine API.
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


def _contains(test: unittest.TestCase, needle: str, text: str) -> None:
    test.assertTrue(needle in text, f"SPEC.md is missing: {needle}")


class Step5HostFetchHardeningTests(unittest.TestCase):
    def test_01_spec_is_v24_3(self) -> None:
        # Retirement (step 7): the exact "**Status:** v24.3" / "whole v24.3
        # contract" assertions are retired because step 7 amends the
        # self-contained contract to v24.4. The v24.3 changelog entry and
        # the F-04 / F-05 / F-06 / F-07 / F-09 / F-10 / F-11 wording checks
        # below stay.
        text = _spec()
        self.assertTrue(SPEC.is_file())
        self.assertIn("*   2026-11-16: v24.3.", text)
        self.assertTrue(
            "**Status:** v24.3" in text or "**Status:** v24.4" in text,
            "status must remain v24.3 or become v24.4 (step 7); got a different status line",
        )

    def test_02_goldfish_copy_is_identical_when_present(self) -> None:
        if not GOLDFISH.is_file():
            self.skipTest(f"{GOLDFISH} is not present in this environment")
        self.assertEqual(
            hashlib.sha256(SPEC.read_bytes()).digest(),
            hashlib.sha256(GOLDFISH.read_bytes()).digest(),
            "SPEC.md must be byte-identical to /workspace/goldfish/s3a-kite.md",
        )

    def test_03_f04_fetch_error_vocab_dns_timeout_and_body_decode(self) -> None:
        text = _spec()
        # Closed vocabulary gains body_incomplete and decode_failed.
        _contains(self, "`body_incomplete`", text)
        _contains(self, "`decode_failed`", text)
        _contains(self, "When a fetch fails, `fetch_error` is exactly one of: `scheme_not_allowed`, `egress_denied`, `egress_not_allowlisted`, `connect_failed`, `timeout`, `too_many_redirects`, `http_status`, `unknown_encoding`, `headers_too_large`, `body_incomplete`, `decode_failed`, `body_too_large`, `snapshot_too_large`.", text)
        _contains(self, "then `unknown_encoding`, then `headers_too_large`, then `body_incomplete`, then `decode_failed`, then `body_too_large`", text)
        # DNS failure maps to existing connect_failed; no new dns_failed code.
        _contains(self, "A DNS failure (including NXDOMAIN or SERVFAIL on the `source_url` host or a redirect hop), or a connect or TLS failure after the address has passed the deny and allowlist checks, is `fetch_error: \"connect_failed\"`", text)
        self.assertNotIn("`dns_failed`", text)
        # Mid-body and corrupt decode.
        _contains(self, "A connection reset or truncated body after the response headers have been received is `fetch_error: \"body_incomplete\"` and writes no snapshot", text)
        _contains(self, "A corrupt `gzip` or `deflate` stream is `fetch_error: \"decode_failed\"` and writes no snapshot", text)
        _contains(self, "Unknown encoding is `fetch_error: \"unknown_encoding\"` and no snapshot", text)
        # Timeout starts before the first DNS query.
        _contains(self, "from the start of the fetch (before the first DNS query) to the final decoded body", text)
        # AT cases.
        _contains(self, "a `source_url` or redirect hop whose host does not resolve (NXDOMAIN or SERVFAIL) is `fetch_error: \"connect_failed\"`, no snapshot is written, and the inclusion is kept", text)
        _contains(self, "a mid-body connection reset or truncated body after the headers is `fetch_error: \"body_incomplete\"`", text)
        _contains(self, "a corrupt `gzip` or `deflate` body is `fetch_error: \"decode_failed\"`", text)

    def test_04_f05_consent_bootstrap_and_post_on_write_failure(self) -> None:
        text = _spec()
        _contains(self, "If that startup create fails (the parent is not writable, ENOSPC, or any other create or publish failure), startup is an error that names `--consent-state-file`, binds nothing, and does not leave a bound listener", text)
        _contains(self, "`POST /v1/consent` `{ \"state\": \"on\" }` with the consent-file parent made non-writable exits non-zero (DD-138), leaves the consent file `off`, and sends no LRS statement", text)
        _contains(self, "startup with `--lrs-url` set and a missing `--consent-state-file` whose parent is not writable is a startup error that names `--consent-state-file`, binds nothing, and creates no consent file", text)
        _contains(self, "The non-writable-parent injection requires a non-root runner, or is skipped and the skip is reported if the process is running as root", text)

    def test_05_f06_pause_rearm_and_revoke_only_receipt_hook(self) -> None:
        text = _spec()
        _contains(self, "After the host observes CONTINUE as a non-empty regular file, it deletes CONTINUE. On each subsequent firing of the same point it replaces READY with the hook-point name and a trailing newline, and waits for a new CONTINUE", text)
        _contains(self, "Hook point `pause_after_consent_receipt` fires for revokes only; it does not fire for an `on`", text)
        _contains(self, "The test's assertions do not depend on the release order of parked requests", text)

    def test_06_f07_step1_lstat_sidecar_lock_names(self) -> None:
        text = _spec()
        _contains(self, "`lstat` each sidecar lock name; any existing non-regular entry is a startup error that names the path, binds nothing, and mutates none of those paths", text)
        _contains(self, "This `lstat` runs in step (1), before any sidecar lock file is created, so a FIFO at `--snapshot-dir/.lock` with a missing `--data-dir/.lock` fails without creating `--data-dir/.lock`", text)

    def test_07_f09_bind_uses_lrs_url_loopback_definition(self) -> None:
        text = _spec()
        _contains(self, "For `--bind`, a **loopback** IP is an IPv4 address in `127.0.0.0/8`, IPv6 `::1`, or the IPv4-mapped form of a `127.0.0.0/8` address (for example `::ffff:127.0.0.1` or `127.0.0.2`)", text)
        _contains(self, "On `--bind 127.0.0.2` without `--behind-proxy`, `GET /health` without a token returns `ok\\n`", text)

    def test_08_f10_idle_deadline_restarts_after_each_response(self) -> None:
        text = _spec()
        _contains(self, "The `--handle-idle-secs` deadline restarts at the end of each response", text)
        _contains(self, "Idle time between requests on a kept-alive connection is bounded by the same flag; if it elapses, the connection is closed without a response", text)

    def test_09_f11_privileged_cases_skip_and_report(self) -> None:
        text = _spec()
        _contains(self, "The network-namespace observation requires root or `CAP_SYS_ADMIN` (or a pre-provisioned fixture) and is skipped and the skip is reported otherwise", text)
        _contains(self, "The not-owned-by-euid case requires root (or a pre-provisioned fixture) and is skipped and the skip is reported otherwise", text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
