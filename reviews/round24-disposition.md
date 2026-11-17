# Round 24 disposition — s3a-kite SPEC v24.2

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round24.md`.
14 findings (0 blockers, 0 majors, 14 minors).

**Counts:** 5 fixed in v24.2 (F-01, F-02, F-03, F-08, F-12), 0 rejected, 9 deferred to later BUILD-PLAN steps (F-04–F-07, F-09–F-11 in step 5; F-13 in step 6; F-14 in step 7).

This version also records Ashley's later 2026-11-16 reversal of Q-05 / DD-35 (not a goldfish finding): CJK and emoji are required UA fonts. DD-22 and DD-35 are amended. Q-06 and Q-17 stay measure-first.

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

## Deferred (unchanged schedule)

**F-04–F-07, F-09–F-11.** BUILD-PLAN step 5 (host and fetch hardening).

**F-13.** BUILD-PLAN step 6. fd-based snapshot API in s3a-engine. Do not touch s3a-storage until Ashley says so.

**F-14.** BUILD-PLAN step 7. Nesting-depth bound.

## AT audit

- Inventory: AT-01–AT-239, all unique, none missing, none removed. DD-01–DD-159 (DD-22 and DD-35 amended; DD-63 still retired; DD-126 still unused). Q-01–Q-20 (Q-05 reversed; Q-06 and Q-17 still measure-first).
- Existing ATs changed in place: AT-10, AT-42, AT-46, AT-49, AT-65, AT-90, AT-165, AT-211.
- No new AT numbers. No new DDs.
- Identical copies: `/workspace/s3a-kite/SPEC.md` and `/workspace/goldfish/s3a-kite.md`.
