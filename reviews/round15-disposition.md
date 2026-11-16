# Round 15 disposition — s3a-kite SPEC v16

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round15.md` (copied to `reviews/goldfish_s3a-kite_round15.md`).
19 findings (0 blockers, 5 majors, 14 minors). Archived v15: `reviews/SPEC-v15.md`.

**Counts:** 19 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-143, DD-145, DD-147, and DD-148 are unchanged (byte-identical to v15). DD-142, DD-144, DD-146, DD-149, and DD-150 keep their v15 intent and are tightened (revoke-in-flight qualification and a second cancel; fd-based `open_path` and working-archive identity re-check; v16 is not frozen; min `break_index`, intact unsigned sealed chain, verifying type-70 container exemption, hyper-tile torn row). No new open questions. No invented numeric defaults.

This v16 is not frozen (DD-149): round 15 still had majors.

## How majors were resolved

**F-01** (`open_path` took a path string, so the host could not keep "use that fd, never reopen by name"). Option (a), as Ashley directed: `open_path` now takes a host-validated fd, the proved `canonical_path`, and the `validated_root` of that walk, plus the existing `scholar_roots` / `name_hint`. The pack reads from the fd only and does not reopen or `lstat` the path. `paper_identity` is computed from `canonical_path`, `validated_root`, and the YAML in the same fd bytes. Aligned DD-146, DD-139, DD-75, the §5.2.2 path rule, AT-96, AT-98, and AT-101.

**F-02** (an `on` received after a revoke's receipt but serviced before it could re-enable LRS). A `revokes_in_flight` counter is incremented at revoke receipt and decremented after the revoke's persist (or the process exits fail-closed). An `on` qualifies only if the counter was 0 at receipt and is 0 at the CAS, and its observed epoch is still current. Inside its mutex hold, after setting the flag `off`, the revoke cancels the current token again and swaps it, so a POST started in the window is aborted (`skipped`). A same-state `on` never touches `cleared_epoch`. Aligned DD-142, DD-144, and AT-198 (including the "on received after revoke receipt, wins the mutex first" ordering).

**F-03** (the derivation table had no correct row for an active chain with no signatures). The ambiguous "flag clear" row is split: chain inactive, `ED25519` clear, no type-70, no CRC → Unsigned / unsealed; chain active, `ED25519` clear, no type-70, no CRC, chain intact → Unsigned / **sealed**. An active chain is **intact** when no CRC, torn-stop, `SealedChainBreak`, reorder, or seal-hash mismatch applies. Zero type-70 records are not a `chain_break` and are not `missing_signature` when the signed flag is clear. Added to AT-165 and AT-154.

**F-04** (`break_index` was undefined under several causes, and priority could point at the wrong tile). `break_index` is the **minimum** over every applicable row that sets one, independent of `quarantine_reason` priority. Valid floors end before that index. The §5.1 mapping and `open_verified` follow the same rule. A multi-cause fixture (chain break at 4, later `signature_known_signer` at 7) is in AT-165 / AT-201: reason is `signature_known_signer`, `break_index` is 4.

**F-05** (step-2 / 2b disk-write failure after step 1 was undefined; the registry could leak). On any such failure after a successful step 1: the registry entry is always removed in that hold, waiters get the same `500` `write_failed` with their own `included` / `lrs_status`, the inclusion is kept, and step 3 still runs on the request that wrote the inclusion line. If rename succeeds and the index append then fails, the published snapshot is left for DD-103 reuse. New AT-212, AT-213, and AT-214. Tightened DD-112 and DD-120.

## Minors

**F-06.** Fixed: `invalid_report` also rejects a `break_index` out of range or not covered by `invalid_tile_indices`, and a `signer_not_pinned` / `missing_signature` report whose list is not the full range. Extended AT-211.

**F-07.** Fixed: a hyper-tile torn/uncommitted-tail row is in DD-150 (not Quarantined solely for torn; `break_index` is the first torn tile; later indices are not valid floors). The torn predicate is defined for hyper-tile as well as Edge.

**F-08.** Fixed: a type-70 container tile whose records all verify is exempt from the unsigned-tail rule (it may sit after the `tile_index` it covers). MICRO-VEC-03 / AT-210 pin that choice; tiles after the container remain in the tail.

**F-09.** Fixed: the item-2 corpus walk opens files with the host pin list and **skips Edge files**. AT-166 drops the signer-not-pinned parenthetical and uses a Quarantined hyper-tile.

**F-10.** Fixed: AT-178's second variant is a `signer_not_pinned` file with no valid floors (payloads withheld; banner count per AT-195), not a "valid-floor denial" in that file.

**F-11.** Fixed: the §7 distinct-address rule is scoped to tests that need a **successful** fetch and an LRS; AT-54's denial case is the stated exception.

**F-12.** Fixed: pack test range is AT-64–AT-214; the §7 guards sentence includes AT-34 and AT-39.

**F-13.** Fixed: AT-183 is a unit test of `egress_allowlist_matches(entry, url)` plus parser errors. End-to-end, a non-matching allowlist yields `fetch_error: "egress_not_allowlisted"` with zero connects, distinct from deny-list or unreachability, exercised against the `egress_test` loopback fixture with IP-literal entries.

**F-14.** Fixed: AT-194 and AT-210 now require "no later denial, withdrawal, or expiry on any parseable tile", matching clause 2.

**F-15.** Fixed: option (b) — before each step-1 write the host re-checks `(dev, ino)` against the recorded startup (or create) identity via the parent dirfd and `openat(O_NOFOLLOW)`. A post-startup symlink swap is `500` `write_failed`. Extended AT-203.

**F-16.** Fixed: when `--consent-state-file` is configured, the host takes `LOCK_EX|LOCK_NB` on `--consent-state-file.lock` with the other sidecars. A second host sharing that consent file is a startup error and mutates nothing. Extended AT-208 and DD-114.

**F-17.** Fixed: after a plain startup that did not rewrite the file, GET returns the stored timestamp. Grammar is `("on"|"off") "\n" RFC3339 "\n"?`; a missing trailing newline is not malformed.

**F-18.** Fixed: AT-180 and AT-209 state `lrs_status: "skipped"` only when `--lrs-url` is configured (otherwise `not_configured`).

**F-19.** Fixed: an `Upgrade: websocket` request to a path not in the table returns 404; on a known path the Upgrade header is ignored and no 101 is returned. AT-06 matches.

## AT audit (v15 → v16)

- **Defined range:** AT-01–AT-214 (214 tests, no gaps).
- **Changed (18):** AT-06, AT-96, AT-98, AT-101, AT-154, AT-165, AT-166, AT-178, AT-180, AT-183, AT-194, AT-198, AT-201, AT-203, AT-208, AT-209, AT-210, AT-211.
- **Added (3):** AT-212 (step-2 publish failure after step 1), AT-213 (rename OK, index append fails; file left for reuse), AT-214 (step-2b publish failure after step 1).
- **Removed:** none.

## New and tightened decisions / questions

- No new DDs or Qs.
- Tightened: DD-75, DD-112, DD-114, DD-120, DD-139, DD-142, DD-144, DD-146, DD-149, DD-150.
