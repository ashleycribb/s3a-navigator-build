# Round 24 disposition — s3a-kite SPEC v24.2

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round24.md`.
14 findings (0 blockers, 0 majors, 14 minors).

**Counts:** 12 fixed (F-01, F-02, F-03, F-08, F-12 in v24.2; F-04, F-05, F-06, F-07, F-09, F-10, F-11 in v24.3), 0 rejected, 2 deferred (F-13 in BUILD-PLAN step 6; F-14 in step 7).

v24.2 also recorded Ashley's later 2026-11-16 reversal of Q-05 / DD-35 (not a goldfish finding): CJK and emoji are required UA fonts. DD-22 and DD-35 are amended. Q-06 and Q-17 stay measure-first. v24.3 is wording-only host and fetch hardening; it does not implement the host or an s3a-engine API.

## Applied in v24.2 (BUILD-PLAN step 4)

**F-01.** Fixed: a hyper-tile committed tile (index < header `tile_count`) whose header is absent, erased (`0xFF` fill), or fails validation is torn, same as Edge — not quarantined solely for that header, `break_index` is that tile when it is the first such stop, later tiles are uncommitted. A committed CRC failure still requires a header that validates. The §5.1 `torn_tile_indices` row now defers to the per-kind §5.4 predicate. The hyper-tile torn derivation-table row covers this case. AT-165 adds a hyper-tile invalid-header fixture. The binary fixture itself lives in the out-of-workspace s3a-reader test crate named by AT-165; this step records the contract and does not implement pack `open`.

**F-02.** Fixed: the §5.4 signature-failure prose now matches the derivation table: every tile at or after `break_index` = (highest preceding authentic `tile_index`) + 1 is not a valid floor (gap tiles included).

**F-03.** Fixed: the §5.1 `open_verified` `invalid_report` checks, and AT-211, now include: hyper-tile `seal_trailer` ⇔ `seal_status == seal_failed`; hyper-tile `seal_failed` ⇒ `status == quarantined`; `crc` ⇒ `tiles_crc_failed` is non-empty; `tiles_crc_failed` ⊆ `invalid_tile_indices`; when `tiles_crc_failed` is non-empty, `break_index ≤ min(tiles_crc_failed)`.

**F-08.** Fixed: a hyper-tile "quarantined tile 78" is one that is not a valid floor. On a seal-failed file whose committed tiles pass CRC, display-only floors remain valid floors: the stored manifest is painted together with the persistent quarantine banner (AT-90). The first tile 78 is also listed as a floor, consistent with AT-10 (`s3a-cli tour --json`) and AT-65.

**F-12.** Fixed:
- `GET /v1/candidates` is on the §5.2.2 `409` `viewer_only` list.
- The `path_escape` When text covers a working-archive `/v1/open` `(dev, ino)` mismatch and `ELOOP` (AT-217).
- §5.5 step 2 is "no **usable** snapshot".
- AT-46 (and the §5.6 `ADDON_EDU` check) now say step 1 is skipped because of a prior matching inclusion-index line, not because of the `ADDON_EDU` bit.
- Inside the step-0/1 hold, the order is `lock_failed`, then `write_failed` for a missing, replaced, or unreadable working archive, then `addon_missing`, then later write failures.

## Q-05 / DD-35 reversal (Ashley, 2026-11-16)

Ashley: "Add the fonts in the 4th if it will help." CJK and emoji ship as required UA fonts. Chosen fonts: Noto Sans SC (SIL OFL 1.1; already-vendored `NotoSansCJKsc-Regular.woff2`, measured 6,093,444 bytes) and monochrome Noto Emoji (SIL OFL 1.1; `assets/ua-fonts/NotoEmoji[wght].ttf`, measured 1,982,596 bytes; TrueType `glyf` outlines, no COLR/CBDT/sbix/SVG tables). Color-emoji paint support is not established in the vendored renderer, so monochrome Noto Emoji is required and color-emoji paint is not claimed. License files and NOTICE entries are required. Combined added size 8,076,040 bytes. Impact on the pack or APK and on `max_open_bytes` / Pixel 9 budgets remains an open measurement item (Q-06, Q-17); no budget numbers are invented. AT-42 and AT-49 are updated in place.

## Applied in v24.3 (BUILD-PLAN step 5)

**F-04.** Fixed: DNS failure (including NXDOMAIN or SERVFAIL on `source_url` or a redirect hop) is `connect_failed` (no additional DNS-only error code). Mid-body connection reset or truncated body after headers is `body_incomplete`. A corrupt `gzip` or `deflate` stream is `decode_failed` (`unknown_encoding` remains only for an unknown coding name). Precedence is `unknown_encoding`, then `headers_too_large`, then `body_incomplete`, then `decode_failed`, then `body_too_large`. `--fetch-timeout-secs` runs from the start of the fetch (before the first DNS query) to the final decoded body. AT-103 and AT-183 record the new cases. Tightened DD-49.

**F-05.** Fixed: if the startup create of a missing consent file fails, startup is an error that names `--consent-state-file`, binds nothing, and does not leave a bound listener (AT-151, DD-96). A `POST /v1/consent` `{ "state": "on" }` with the consent-file parent made non-writable exits non-zero (DD-138), leaves the file `off`, and sends no LRS statement (AT-169, same non-root skip rule).

**F-06.** Fixed: `pause_until_signal` re-arms — after observing CONTINUE, the host deletes CONTINUE; on each later firing it replaces READY and waits for a new CONTINUE. `pause_after_consent_receipt` fires for revokes only, not for an `on` (goldfish-recommended option, which makes AT-198 ordering 2 deterministic). AT-198 states that its assertions do not depend on the release order of parked requests.

**F-07.** Fixed: startup step (1) `lstat`s each sidecar lock name; an existing non-regular entry is a startup error that names the path, binds nothing, and mutates none of those paths, before any `.lock` is created. A FIFO at `--snapshot-dir/.lock` with a missing `--data-dir/.lock` therefore creates no `--data-dir/.lock`. The step-(2) post-open `fstat` remains the TOCTOU backstop (AT-208).

**F-09.** Fixed: `--bind` loopback reuses the `--lrs-url` definition (`127.0.0.0/8`, `::1`, IPv4-mapped 127/8). `--bind 127.0.0.2` gets the tokenless health case (AT-50). Tightened DD-135.

**F-10.** Fixed: the `--handle-idle-secs` deadline restarts at the end of each response. Idle time between requests on a kept-alive connection is bounded by the same flag; if it elapses, the connection is closed without a response (AT-230, DD-108).

**F-11.** Fixed: AT-226's network-namespace observation requires root or `CAP_SYS_ADMIN` (or a pre-provisioned fixture) and is skipped and the skip is reported otherwise (the `lrs_deny_union` unit-test half does not require root). AT-174's not-owned-by-euid case has the same skip-and-report rule.

## Deferred (unchanged schedule)

**F-13.** BUILD-PLAN step 6. fd-based snapshot API in s3a-engine. Do not touch s3a-storage until Ashley says so.

**F-14.** BUILD-PLAN step 7. Nesting-depth bound.

## AT audit

- Inventory: AT-01–AT-239, all unique, none missing, none removed. DD-01–DD-159 (DD-22 and DD-35 amended; DD-63 still retired; DD-126 still unused). Q-01–Q-20 (Q-05 reversed; Q-06 and Q-17 still measure-first).
- Existing ATs changed in place: AT-10, AT-42, AT-46, AT-49, AT-65, AT-90, AT-165, AT-211 (v24.2); AT-50, AT-103, AT-151, AT-169, AT-174, AT-183, AT-198, AT-208, AT-226, AT-230 (v24.3).
- No new AT numbers. No new DDs.
- Identical copies: `/workspace/s3a-kite/SPEC.md` and `/workspace/goldfish/s3a-kite.md`.
