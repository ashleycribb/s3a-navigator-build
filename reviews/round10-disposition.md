# Round 10 disposition — s3a-kite SPEC v11

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round10.md` (copied to `reviews/goldfish_s3a-kite_round10.md`).
22 findings (0 blockers, 5 majors, 17 minors). Archived v10: `reviews/SPEC-v10.md`.

**Counts:** 22 fixed, 0 rejected, 0 deferred.

DD-128 and DD-138 are unchanged (byte-identical to v10). One new open question (Q-17). No invented numeric defaults.

## How majors were resolved

**F-01** (concurrent `on`/`off` can leave the flag `on` after a persisted revoke). Fixed by simplification, recorded as **DD-142**. Consent GET and POST are serialized by the write mutex. The in-memory flag is read and written only while holding that mutex (no longer set `off` before waiting for the mutex). Revoke always wins: `off` is final until a later explicit `POST on` that **starts after** the revoke has completed. An `on` already in flight when the revoke starts must not set the flag or write `on`. A received revoke still inhibits LRS immediately. Added AT-198.

**F-02** (`lrs_status` for a revoke-aborted POST is `failed` vs `skipped`). Fixed by picking one outcome: **`skipped`**. That matches the DD-134 no-response rule when consent is `off` in the following mutex hold, and it matches AT-156's third case. Changed AT-156's second case, DD-131, the `/v1/select` row, and DD-134 so they agree. No precedence exception.

**F-03** (optional candidate cache ignores clock-based expiry). Fixed by simplification, recorded as **DD-143** (scope cut): v1 has no candidate cache. Every candidates/membership check is a fresh walk plus a current-clock filter. Ashley may reinstate a cache later. Extended AT-161 with a candidates/select case after the clock advances.

**F-04** (AT-35 vs AT-90 floors on a sealed hyper-tile with a committed CRC failure). Fixed by precedence: when a committed CRC failure is present, the CRC rule decides the valid floors, even if a trailer is also present. AT-90's "CRC-valid tiles are listed" applies only when every committed tile passes CRC. `quarantine_reason` still follows the existing priority (`seal_trailer` outranks `crc`). Aligned AT-35, AT-90, AT-165, §5.3 item 6, and DD-111.

**F-05** (AT-54's second case unbuildable). Fixed without new flags: dropped the global-unicast / resolver-hook case (no such flag exists, and a real global-unicast destination is not a CI fixture). The second case is now a loopback same-IP denial: LRS at `http://127.0.0.1:<lrs-port>/`, fetch at `http://127.0.0.1:<egress-port>/`, still denied because that IP is the LRS host; `egress_test` does not override. Tests that need a successful fetch plus LRS still bind distinct addresses (DD-140: LRS on `[::1]`, fetch on `127.0.0.1`).

## Minors

**F-06.** At start with consent `on`, leftover `{id}.claimed` files are renamed to `{id}.json` before replay. Extended AT-91.

**F-07.** Added `name_hint: Option<String>` to `open_path` (host passes `/v1/open` `name`, else the pack uses the path basename). Updated AT-96 and DD-139.

**F-08.** `href` in the Hit and SemanticNode is the client-usable value: resolved absolute path (fragment stripped) for relative links; raw attribute (fragment stripped) for `http`/`https`/`file://`. Updated AT-160.

**F-09.** Deleted "hyper-tile" from the `signer_not_pinned` / `missing_signature` sentences. Those reasons are Edge-only. Tightened DD-141.

**F-10.** The Edge seal chain stops at the first torn or CRC-failed tile; later tiles are uncommitted and not floors. `break_index` is that index. A torn tile alone does not quarantine the file (AT-17). Extended AT-17 and AT-165.

**F-11.** If `--consent-state-file` is missing, the parent must exist and is canonicalized; parent+basename is checked against request roots. Extended AT-179.

**F-12.** A Scholar start without `--lrs-url` deletes leftover `lrs-retry/` and `revoke-pending` (startup error naming the path on failure). Added AT-199.

**F-13.** `failed` includes a permanent 4xx (`{id}.permanent` written). Updated DD-131 and the select row.

**F-14.** AT-150 now asserts `on_disk: false` on `GET /v1/candidates`, and `path` / `download_error` on download.

**F-15.** The size-cap and unparseable-YAML rules apply to candidate item 1 and item 2.

**F-16.** Deleted the impossible "or of revoke-pending until rewritten" parenthetical from AT-181.

**F-17.** §8 preamble now says DD-01–DD-141 (then DD-143). DD-66 now refers to DD-138 instead of an in-process send barrier.

**F-18.** Added `KiteVerificationReport` to the §5.1 phone export list, DD-45, and DD-109, matching AT-99 / §5.7.

**F-19.** Did not invent phone numeric defaults. Added **Q-17** for `max_frame_pixels` and `max_open_bytes` on the phone.

**F-20.** Dropped the vacuous "unless `--behind-proxy`" clause from DD-135.

**F-21.** AT-83 now says "The download response's `snapshot` is the existing snapshot path."

**F-22.** AT-74 now returns `500` `write_failed` (existing catalogue code, also extended to name AT-74).

## New DDs and questions

- **DD-142** Consent transitions serialized; revoke wins (F-01).
- **DD-143** No candidate cache in v1, a scope cut (F-03). Ashley may reinstate later.

Tightened in place: DD-45, DD-66, DD-102, DD-109, DD-111, DD-131, DD-134, DD-135, DD-139, DD-140, DD-141.

**DD-128 and DD-138 are byte-identical to v10.** DD-63 remains retired; DD-126 remains unused.

**New Qs:** Q-17 (phone `max_frame_pixels` and `max_open_bytes`). Q-01–Q-16 unchanged (Q-03 answered; Q-15 and Q-16 remain future questions).

## Acceptance-test audit

Compared every AT in archived `reviews/SPEC-v10.md` (AT-01–AT-197) to SPEC v11 (AT-01–AT-199). Numbering has no gaps and no renames.

- **182 existing ATs passed unchanged.**
- **15 existing ATs changed in place:** 17, 35, 54, 74, 83, 90, 91, 96, 150, 156, 160, 161, 165, 179, 181.
- **2 ATs added:** AT-198, AT-199.
- **0 ATs removed.**

### Changed in place

| AT | Why |
| --- | --- |
| AT-17 | `break_index` and later tiles when the chain stops at a torn tile (F-10). |
| AT-35 | Sealed-plus-CRC fixture: CRC rule decides floors (F-04). |
| AT-54 | Buildable loopback same-IP LRS-deny case; no resolver hook (F-05). |
| AT-74 | `500` `write_failed` (F-22). |
| AT-83 | Download response's `snapshot` is the existing path (F-21). |
| AT-90 | Listed floors only when every committed tile passes CRC (F-04). |
| AT-91 | Startup renames leftover `.claimed` to `.json` before replay (F-06). |
| AT-96 | `open_path` takes `name_hint` (F-07). |
| AT-150 | `on_disk` on candidates; download `path` / `download_error` (F-14). |
| AT-156 | Revoke-aborted POST is `skipped` (F-02). |
| AT-160 | `href` is the resolved absolute path (F-08). |
| AT-161 | After clock advance, candidates drop the id and select is 404 (F-03). |
| AT-165 | Torn `break_index`; sealed+CRC floor agreement (F-04, F-10). |
| AT-179 | Missing `--consent-state-file` uses parent+basename (F-11). |
| AT-181 | Dropped the impossible timestamp parenthetical (F-16). |

### Added

| AT | Finding |
| --- | --- |
| AT-198 | Concurrent `on`/`off`: final state is `off`; later `on` after revoke may succeed (F-01). |
| AT-199 | Scholar start without `--lrs-url` deletes leftover `lrs-retry/` and `revoke-pending` (F-12). |

Passed unchanged: AT-01–AT-16, AT-18–AT-34, AT-36–AT-53, AT-55–AT-73, AT-75–AT-82, AT-84–AT-89, AT-92–AT-95, AT-97–AT-149, AT-151–AT-155, AT-157–AT-159, AT-162–AT-164, AT-166–AT-178, AT-180, AT-182–AT-197.

## Contract range

Acceptance tests: AT-01–AT-199.
Draft decisions: DD-01–DD-143 (DD-63 retired; DD-126 unused).
Open questions: Q-01–Q-17 (Q-17 new).
