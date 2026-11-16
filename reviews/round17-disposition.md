# Round 17 disposition — s3a-kite SPEC v18

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round17.md` (copied to `reviews/goldfish_s3a-kite_round17.md`).
14 findings (0 blockers, 4 majors, 10 minors). Archived v17: `reviews/SPEC-v17.md`.

**Counts:** 14 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-141, DD-143, DD-147, and DD-148 are unchanged (byte-identical to v17). DD-139, DD-142, DD-144, DD-145, DD-146, DD-149, and DD-150 keep their prior intent and are tightened (qualifying-wiki `shadowed_by_wiki` input, fd-only `open_path`; revoke completion and decrement on client disconnect; authentic requires a matching `seal_hash`; candidate-walk and fd-based working-archive writes under DD-146; v18 is not frozen; chain-inactive type-70 seal-hash mismatch is `chain_break`). DD-16, DD-28, DD-61, DD-93, DD-99, DD-113, DD-114, and DD-122 are also tightened to the same rules. No new open questions. No invented numeric defaults.

This v18 is not frozen (DD-149): round 17 still had majors.

## How majors were resolved

**F-01** (`paper_identity` "wiki wins" needed a filesystem fact the pack was not given, and "wins" had two meanings). "Wiki wins" is defined as **a qualifying item-1 wiki candidate exists** at the same relative path (regular file under the shared safe-open rule, parseable UTF-8 YAML, matching `source_id` if present, size ≤ cap) — not merely that a directory entry exists. The host, which already walks the wiki root, computes that fact and passes it as `ScholarRoots.shadowed_by_wiki: bool`. The pack stays fd-only and does no path I/O. If `shadowed_by_wiki` is true, `paper_identity` of the corpus copy is `None`. If the wiki path exists but is not qualifying (symlink, bad YAML, oversize), the corpus copy may be listed and `open_path` of it, with the flag false, yields `Some(stem)`, so AT-101 holds. Extended AT-207, AT-96, AT-101, DD-61, and DD-139.

**F-02** (the candidate walk was outside the shared safe-open rule). Item-1 dossier front-matter reads and item-2 corpus `*.s3a` opens are sites of the §5.2.2 / DD-146 shared rule: walk from the root startup dirfd with directory fds for descent, `openat(O_NOFOLLOW|O_NONBLOCK)`, `fstat` regular file, read at most `--max-open-bytes` + 1 bytes. Any failure is a silent skip (no hang, no token bytes). New AT-218.

**F-03** (working-archive writes checked `(dev, ino)` on one fd and then reopened by path). Step-1 writes operate on an already-validated fd: `openat` from the parent dirfd (`O_NOFOLLOW`; `O_NONBLOCK` when opening an existing file; `O_CREAT|O_EXCL` when creating, so create never follows or truncates a symlink), then an fd-taking `TileWriter` constructor in `s3a-engine` (no `s3a-core` change) or the Linux `/proc/self/fd/<fd>` magic link. After the writer is attached and before any tile-10 write, the host `fstat`s the writer's open fd and compares `(dev, ino)` to the recorded identity; a mismatch is `500` `write_failed` and the symlink target is not appended to or truncated. New AT-219 covers a mid-request swap after the check and before the writer is attached. Tightened DD-28, DD-114, and DD-146.

**F-04** (chain-inactive Edge files with type-70 records did not require `seal_hash` to match tile contents). A type-70 record is **authentic** only if the signature verifies under a candidate key **and** the stored `seal_hash` equals the recomputed `compute_seal_hash` at that `tile_index`, whenever any type-70 record exists, whether or not the chain is active. A mismatch is `chain_break` at the first mismatch; tiles at or after `break_index` are untrusted (a later grant does not release). When the chain is inactive, `seal_status` stays `unsealed`. New DD-150 rows cover chain-inactive type-70 files whose hashes all match (Verified / Self-signed, unsealed) and whose hashes mismatch (Quarantined, unsealed, `chain_break`). New AT-220; extended AT-201 and AT-165. Tightened DD-145 and DD-150.

## Minors

**F-05.** Fixed: the §5.3 unsigned-tail clause applies only in a file that contains at least one authentic type-70 record; a verifying type-70 container is exempt (§5.4).

**F-06.** Fixed: DD-113 states that persist writes run only when the flag was `on`; a same-state `off` performs no persistent writes (DD-144).

**F-07.** Fixed: once receipt-time latch actions have run, the revoke runs to completion of its mutex hold, decrement included, even if the client disconnects mid-hold. AT-198's third case closes the socket during the hold and then checks that a later `POST on` returns `on`. Tightened DD-142 and DD-144.

**F-08.** Fixed: a 2b failure affects only the request whose 2b failed. Registry entries are for a GET and are removed in the step-2 publish hold; waiters receive the step-2 outcome and then run their own 2b. AT-214's hook fails every 2b in the test.

**F-09.** Fixed: AT-15 / AT-21 assert that each floor's `payload_hash_short` equals the short hash of the tile bytes at the expected offset. A `card` `SemanticNode` role, with rect and `opacity`, is defined for `floor-transition` frames; AT-13 and AT-62 sample that node. AT-125 uses a defined test hook that causes the native window client to issue `POST /v1/select`; any window-originated request carries the Bearer.

**F-10.** Fixed: `tiles_crc_failed` is `Vec<u32>` (increasing tile indices) and `detail` is `String`. Tightened DD-99.

**F-11.** Fixed: `ENOTDIR` from a non-symlink intermediate component is `404` `not_found`. Added to AT-171 and DD-146.

**F-12.** Fixed: the `ADDON_EDU` check on an existing working archive runs at each step 1, not at startup. The result is `409` `addon_missing`, nothing is written, and there is no `lrs_status`. For select/download this is after `unknown_paper` and before any write. AT-46 requires host B to start, then fail on select. Tightened DD-16.

**F-13.** Fixed: a basename matching `s3a-kite-tmp-*` directly inside `--snapshot-dir` or `--download-dir` is `409` `not_regular_file`, checked after the walk and before the read. The same basename under a request root is a normal path. Added to AT-171. Tightened DD-122.

**F-14.** Fixed: disjointness is "equal to, or an ancestor directory of (path-component prefix after canonicalization)"; `/x/a` vs `/x/ab` is not a violation. Negative control added to AT-179 (and AT-145). Tightened DD-93.

## AT audit (v17 → v18)

- **Defined range:** AT-01–AT-220 (220 tests, no gaps).
- **Changed (17):** AT-13, AT-15, AT-21, AT-46, AT-62, AT-96, AT-101, AT-125, AT-145, AT-165, AT-171, AT-179, AT-198, AT-201, AT-203, AT-207, AT-214.
- **Added (3):** AT-218 (candidate-walk FIFO/symlink swap), AT-219 (mid-request working-archive symlink swap), AT-220 (chain-inactive type-70 seal-hash mismatch).
- **Removed:** none.

## New and tightened decisions / questions

- No new DDs or Qs.
- Tightened: DD-16, DD-28, DD-61, DD-93, DD-99, DD-113, DD-114, DD-122, DD-139, DD-142, DD-144, DD-145, DD-146, DD-149, DD-150.
