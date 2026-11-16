# Round 16 disposition — s3a-kite SPEC v17

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round16.md` (copied to `reviews/goldfish_s3a-kite_round16.md`).
13 findings (0 blockers, 3 majors, 10 minors). Archived v16: `reviews/SPEC-v16.md`.

**Counts:** 13 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-143, DD-145, DD-147, and DD-148 are unchanged (byte-identical to v16). DD-142, DD-144, DD-146, DD-149, and DD-150 keep their prior intent and are tightened (decrement `revokes_in_flight` at the end of every revoke mutex hold, including a same-state no-op; receipt-time latch actions only after auth and request validation; shared safe-open rule; v17 is not frozen; Edge torn-only `break_index` whether or not the chain is active). DD-62, DD-78, DD-100, and DD-103 are also tightened to the same rules. No new open questions. No invented numeric defaults.

This v17 is not frozen (DD-149): round 16 still had majors.

## How majors were resolved

**F-01** (a same-state `POST {"state":"off"}` incremented `revokes_in_flight` but never decremented it, so a later `on` could not qualify). Chosen option: decrement `revokes_in_flight` at the **end of every revoke's mutex hold**, whether or not the revoke persisted, including a same-state no-op `off` and a `POST off` without `--consent-state-file`. Only a fail-closed exit (DD-138) skips the decrement. Under the mutex a revoke always re-cancels and swaps the token, even on a no-op; if the flag is already `off`, there are no persistent writes. AT-198's second ordering now checks that a later `POST {"state":"on"}` issued after both of those requests have completed returns `on`. Aligned DD-142 and DD-144.

**F-02** (AT-187 required a `--snapshot-dir` temp mid-fetch, but step 2 creates that temp only after the GET returns). Chosen option (a), consistent with §5.5 / §5.6: a defined test hook pauses host A **inside the step-2 publish mutex hold**, after the `--snapshot-dir` `s3a-kite-tmp-*` temp has been created and `LOCK_EX`-locked and before the rename. The GET has already returned; an in-flight fetch body may live in the ephemeral directory of §5.2.2. Host B's startup cleanup must not delete A's live temps; A's hook then resumes and the snapshot publishes. The same-directory collision case remains AT-164.

**F-03** (snapshot reuse, download-dir hash check, and the working-archive `/v1/open` copy could hang the write mutex on a FIFO, with no shared open discipline). One shared safe-open rule (DD-146, §5.2.2): `openat` from the site's startup dirfd, final component `O_NOFOLLOW|O_NONBLOCK`, `fstat` requires a regular file, and at most `--max-open-bytes` + 1 bytes are read. Sites and errors: `drive_path` → `on_disk: false`; snapshot reuse fail → `409` `snapshot_conflict` or HTTP 200 `snapshot_invalid`; download-dir symlink/FIFO/non-regular or hash mismatch → `409` `download_conflict`; working-archive copy → `409` `file_too_large` / `path_escape` / `not_regular_file`, then pack `open` on the bytes. New AT-215, AT-216, and AT-217. Tightened DD-62, DD-78, DD-103, and DD-146.

## Minors

**F-04.** Fixed: AT-198's first ordering allows a racing `on` that won the mutex before the revoke to return `on` (a same-state no-op that wrote nothing). After both complete, the final GET, file, and LRS are still `off` / empty / zero. Combined with F-01, the second ordering requires a later `on` to succeed.

**F-05.** Fixed: `--consent-state-file.lock` (when configured) is in the reserved and pairwise-distinct lists, along with `--snapshot-dir/.lock`, `--download-dir/.lock`, and `--data-dir/.lock`. AT-208 asserts those collisions.

**F-06.** Fixed: option (a) — when `--view-root` is set in Scholar mode, it is pairwise-disjoint from `--wiki-root` and from `--corpus-root`, so `validated_root` is unambiguous. Extended AT-179 and DD-146.

**F-07.** Fixed: a non-absolute, empty, or NUL-containing `/v1/open` `path` is `400` `invalid_param` (not resolved against any root). Added to the error catalogue and AT-171.

**F-08.** Fixed (elevated with F-01): receipt-time latch actions (increment `epoch` and `revokes_in_flight`, cancel, swap token) run **only after** the `POST /v1/consent` has passed every §5.2.1 error-precedence check (authentication through `invalid_param`) and the body is a valid Scholar `{"state":"off"}`. Rejected requests do not increment counters or cancel POSTs. AT-198 asserts that an unauthenticated or malformed `POST {"state":"off"}` does not increment `epoch` or `revokes_in_flight`.

**F-09.** Fixed: whether or not `FLAG_SEAL_CHAIN_ACTIVE` is set, tiles after the first torn or uncommitted Edge tile are uncommitted and not valid floors, and `break_index` is that first torn tile (matching the hyper-tile torn row). The §5.1 mapping drops the "on an active chain" qualifier. A chain-inactive mid-file torn fixture is in AT-165. Tightened DD-100 and DD-150.

**F-10.** Fixed: the working-archive `/v1/open` copy uses the shared safe-open rule under the write mutex, re-checks `(dev, ino)`, and calls pack `open` on those bytes (not `open_path`, because the parent is not a request root), so `paper_identity` is `None`. AT-217 covers oversize, symlink / identity mismatch, and FIFO / non-regular cases, and requires that a hanging open not block concurrent consent.

**F-11.** Fixed: option (b) — `verification_report` and `open_verified` on `kind: "unsupported"` return `KiteError` `not_archive` (`tiles_total` is not defined). Pack `open` of those bytes still succeeds with the unsupported banner. Extended AT-97.

**F-12.** Fixed: `http` `--lrs-url` is allowed only for loopback IP literals (`127.0.0.0/8`, `::1`, or the IPv4-mapped form of `127.0.0.0/8`). A hostname, including `localhost`, or any other address with `http` is a startup error that names `--lrs-url`. There is no DNS lookup to decide loopback. AT-77 and AT-54 (IP literals) pin the rule.

**F-13.** Fixed: a step-1 index-append failure after the tile-10 write is `500` `write_failed` with `included: true` and no `lrs_status` (steps 2, 2b, and 3 are skipped); the record is kept; a later select may write a duplicate tile-10 and emit LRS. Asserted in AT-158 and the error catalogue.

## AT audit (v16 → v17)

- **Defined range:** AT-01–AT-217 (217 tests, no gaps).
- **Changed (10):** AT-77, AT-94, AT-97, AT-158, AT-165, AT-171, AT-179, AT-187, AT-198, AT-208.
- **Added (3):** AT-215 (snapshot-dir FIFO/symlink/non-regular reuse), AT-216 (download-dir FIFO/symlink/non-regular hash check), AT-217 (working-archive `/v1/open` shared safe-open, pack `open`, `paper_identity` is `None`).
- **Removed:** none.

## New and tightened decisions / questions

- No new DDs or Qs.
- Tightened: DD-62, DD-78, DD-100, DD-103, DD-142, DD-144, DD-146, DD-149, DD-150.
