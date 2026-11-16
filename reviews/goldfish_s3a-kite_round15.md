# Goldfish review — s3a-kite spec v15 (round 15)

**Summary: 0 blocker / 5 major / 14 minor.** This is **not** a clean round: 5 majors remain.

Reviewer: a fresh, independent reviewer. I read only `/workspace/goldfish/s3a-kite.md`. Items already recorded as DDs, Q-xx, scope cuts, or accepted residuals (DD-141, DD-128, Q-17, Q-18, etc.) are not raised as findings unless the spec contradicts itself about them.

---

## Majors

### F-01 (major) — `open_path` takes a path, so the host cannot keep the DD-146 "use that fd, never reopen by name" rule
**Where:** §5.1 `open_path` row; §5.2.2 Path rule (DD-89, DD-146); DD-146; DD-139; AT-101, AT-202.
- §5.2.2: the host "opens the root's dirfd and walks each remaining component with `openat(O_NOFOLLOW)` … The host uses that fd; it does not reopen the path by name later."
- §5.1: `open_path(ctx, path, …, scholar_roots, name_hint)`: "Implementation opens the path then validates the fd (DD-146: `O_NOFOLLOW`, …)". `paper_identity` also re-`lstat`s the canonical path.

**Problem:** the pack API takes a path string, not an fd, and it does not receive the allowed request root. That leaves the host two choices, and both break the spec:
- (i) Open the fd per DD-146, then call `open_path(path)`. The pack then reopens the path by name. Only the final component is `O_NOFOLLOW`, so a symlink swapped into an intermediate directory is followed. This is the TOCTOU the rule exists to stop, and it violates "does not reopen".
- (ii) Read the bytes from its own fd and call `open(bytes)`. Then `paper_identity` is always `None` (it requires `open_path`). Select never appears on dossier pages, and AT-101 fails.

Also, `open_path` cannot do the root-anchored walk itself. For `--view-root`, `--snapshot-dir`, and `--download-dir` it is never told the root (`scholar_roots` carries only wiki/corpus).

**Fix:**
- (a) Change the signature to `open_fd(ctx, fd: OwnedFd, canonical_path, …, scholar_roots, name_hint)`. The pack reads from the host-validated fd. It computes `paper_identity` from the `canonical_path` the host already proved by the dirfd walk, plus the YAML read from the same fd bytes, with no second `lstat`.
- (b) Pass `allowed_root: dirfd/path` plus the root-relative path into `open_path`, and require the pack to do the full component-wise `openat(O_NOFOLLOW)` walk.

Either way, update AT-96 / AT-202 to match.

### F-02 (major) — The observed-epoch CAS lets an `on` received *after* a revoke but serviced *before* it re-enable LRS
**Where:** §5.2.1 "Consent transitions and in-flight LRS"; DD-142; DD-144; AT-198.
- "`off` always wins … final until a later explicit `POST {"state":"on"}` that **starts after** the revoke has completed."
- Mechanism: "An `on` records `observed = epoch` when it is received … clears inhibit only by a single compare-and-swap of `cleared_epoch` to `observed`, and only if `epoch` still equals `observed`." Revoke cancels in-flight POSTs and swaps the token **only at receipt**.

**Problem:** start with the flag `off` and inhibit active from an earlier revoke R1.
1. Revoke R2 is received: `epoch` = 2, the token is swapped.
2. `on` O is received: `observed` = 2.
3. O wins the mutex before R2. `epoch == observed`, so the CAS succeeds. O persists `on`, clears inhibit, and returns `on`.
4. Before R2 takes the mutex, a step-3 / replay sender re-checks: inhibit inactive, flag `on`. It starts a POST under the *fresh* token.
5. R2 then sets `off`, but never cancels again. The statement goes out after the researcher's revoke was received.

O started before R2 completed, so DD-142 says O must not set `on`. The epoch scheme cannot tell "received after the revoke's receipt" from "after its completion". The same gap exists when the flag is `on`, if a same-state no-op `on` is allowed to run the CAS (the spec does not say whether a no-op runs the CAS).

**Fix:**
- (a) Add a `revokes_in_flight` counter, incremented at receipt and decremented after the revoke's persist (or exit). An `on` qualifies only if the counter was 0 when it was received **and** is 0 at its CAS.
- (b) Also make the revoke, inside its mutex hold after setting the flag `off`, cancel the current token again and swap it. Any POST started in the window is then aborted (`skipped`).

Separately, state that a same-state `on` never touches `cleared_epoch`. Extend AT-198 with the "on received after revoke receipt, wins the mutex first" ordering.

### F-03 (major) — The "single total" derivation table has no correct row for an active seal chain with no signatures
**Where:** §5.4 report-field derivation table (DD-150) vs §5.4 label table, the `seal_status` prose rule, and AT-165 (MICRO-VEC-01).
- Derivation row: "Edge: flag clear, no type-70, no CRC | unsigned | **unsealed** | None | None".
- Label table: "Edge | Seal chain active and intact | … | **sealed**". "No signatures, `FLAG_ED25519_SIGNED` clear | Unsigned".
- Prose: "`seal_status` is `sealed` … if Edge `FLAG_SEAL_CHAIN_ACTIVE` is set and `seal_status` is not `seal_failed`."

**Problem:** "flag clear" in the row does not say which flag. Its note ("Unsigned floors are authenticated …, DD-141") points at `FLAG_ED25519_SIGNED`. In that case the row gives `unsealed` for a `FLAG_SEAL_CHAIN_ACTIVE` file, which contradicts the label table and the prose. If it means `FLAG_SEAL_CHAIN_ACTIVE`, then the table has no row at all for chain-active + unsigned, so it is not total.

It is also undefined how an active chain is judged "intact" when there is no type-70 `DeviceSealRecord` to compare recomputed hashes against. Is it intact, `chain_break`, or `missing_signature`? AT-165 asserts pack/reader agreement on `seal_status` for MICRO-VEC-01 and the AT-154 fixtures, so the two sides can diverge.

**Fix:** split the row into two:
- "Edge: chain inactive, `ED25519` clear, no type-70, no CRC → unsigned / unsealed."
- "Edge: chain active, `ED25519` clear, no type-70, no CRC → unsigned / **sealed** / None / None."

Then state explicitly what "intact" means with zero seal records (recommend: the recomputed chain is internally consistent and there are no records to contradict it, so `sealed`). Or (b) declare chain-active + zero type-70 a `chain_break`. Either way, add the case to AT-165.

### F-04 (major) — `break_index` is undefined when several causes apply, and the priority order points at the wrong tile
**Where:** §5.4 derivation table (DD-150); §5.1 mapping (`invalid_tile_indices` = "… every index at or after `break_index`"); `open_verified` row ("tiles at or after `break_index` … are unauthenticated"); AT-165.
- "When more than one row applies, `quarantine_reason` is the highest-priority applicable reason: … `signature_known_signer` > `chain_break` > `crc`." Each row has its own `break_index`, and nothing says which one wins.

**Problem:** take an Edge file with a `chain_break` at tile 4 and a later `signature_known_signer` at tile 7. The reason is `signature_known_signer` (higher priority). An implementer who takes `break_index` from the winning row reports 7. The reader-side mapping, and every phone `open_verified` consumer, then derive "invalid = indices ≥ 7". Chain-broken tiles 4–6 become valid floors, and their type-71 grants count as trusted. That is a consent release on tampered tiles, which is exactly what DD-111 / DD-147 forbid. The same ambiguity exists for CRC + chain break.

**Fix:** add "`break_index` is the **minimum** `break_index` over all applicable rows (independent of the `quarantine_reason` priority)." Add a multi-cause fixture (chain break before a later signature failure) to AT-165 / AT-201.

### F-05 (major) — Disk-write failure in step 2 or step 2b, after step 1 succeeded, is undefined, and the registry can leak
**Where:** §5.5 numbered steps 2 / 2b / 3; §5.2.1 in-flight registry; error catalogue `write_failed`; DD-112; DD-120.
- Catalogue: "`500` `write_failed` — Inclusion **or snapshot** write failed after the preconditions were checked."
- Registry: "The entry is removed in the same mutex hold as the snapshot publish and index append, or as the fetch-failure or `snapshot_conflict` outcome." A publish failure is not in that list.

**Problem:** none of these are specified:
- What happens when snapshot temp/fsync/rename fails, or the snapshot index-line append fails after a successful rename.
- What happens when the step-2b download publish fails.
- Whether the registry entry is removed. If it is not, every later waiter on that key hangs forever.
- What waiters receive.
- Whether step 3 still runs.

If the response is a bare `500 write_failed` and step 3 is skipped, the consented LRS statement is lost for good: every repeat matches step 0, which skips step 3. A `500` also drops `included` and `lrs_status`, unlike the documented `snapshot_conflict` / `download_conflict` bodies.

The sub-case "rename OK, index append fails" leaves a published snapshot that is never recorded. The next repeat re-fetches, and reuse then succeeds only if the bytes are identical.

**Fix:** specify that on any step-2/2b disk failure after a successful step 1:
- The registry entry is removed in that same hold, and waiters get the same outcome.
- The inclusion is kept.
- Step 3 still runs.
- The response is either (a) `200` with `snapshot_error: "write_failed"` (and `download_error: "write_failed"`), or (b) `500` `write_failed` with `included: true` and `lrs_status` in the body.

For "published but index append failed", say whether the file is left for DD-103 reuse (recommended) or deleted. Add an AT that injects each failure.

---

## Minors

### F-06 (minor) — `open_verified` accepts reports whose `break_index` / whole-file reason contradicts `invalid_tile_indices`
**Where:** §5.1 `open_verified` row; AT-211.
- "Valid floors are the complement of `invalid_tile_indices`" vs "tiles at or after `break_index` … are unauthenticated (they are not valid floors)". The validation list checks only range, the torn ⊆ invalid subset, and quarantined ⇒ reason.

**Problem:** consider a report with `break_index: Some(3)` but an empty `invalid_tile_indices`, or `quarantine_reason: signer_not_pinned` with a non-full list. It passes validation, but the two sentences give different floor sets. Consent stays safe (those tiles are unauthenticated), but the display is undefined.

**Fix:** add these to the `invalid_report` checks: every index ≥ `break_index` is in `invalid_tile_indices`; `signer_not_pinned` / `missing_signature` ⇒ the list is the full range; `break_index` < `tiles_total`. Extend AT-211.

### F-07 (minor) — Hyper-tile torn / partial trailing tile has no derivation row
**Where:** §5.3; §5.4 derivation table; §5.1 mapping ("uncommitted indices (index ≥ header `tile_count`, or after the first torn …)").

**Problem:** the torn predicate and the `break_index` rule are written only for Edge. A crash during a host append to the working archive (a common path) can leave a partial trailing hyper-tile. The table gives no `status`, `break_index`, or floor rule for that case, while the mapping implies every later index is invalid.

**Fix:** add a row: "Hyper-tile: torn/uncommitted tail only → status from the other rows (not Quarantined solely for torn), `break_index` None (or first torn), torn and later indices not valid floors."

### F-08 (minor) — The unsigned-tail rule appears to always invalidate the type-70 container tile
**Where:** §5.4 "Unsigned tail (AT-210, DD-150)"; derivation row "Signed committed tiles are valid floors".

**Problem:** a `DeviceSealRecord` signs a chain hash for `tile_index`, and it is natural for the container tile to come after the tiles it covers. Then "every tile whose index is greater than the highest authentic signed `tile_index`" includes the last type-70 tile in every normally signed file. Every Verified file would then show an unsigned-tail banner. It is unclear whether that is intended.

**Fix:** state explicitly either (a) type-70 container tiles whose records all verify are exempt from the unsigned-tail rule, or (b) they are in the tail by design and are reported as such. Pin the choice in the MICRO-VEC-03 / AT-210 expectations.

### F-09 (minor) — AT-166's "signer not in a non-empty pin list" case needs an undefined corpus input
**Where:** AT-166; §5.5 candidate item 2; DD-117.

**Problem:** the candidate walk takes every `*.s3a` under `--corpus-root`. Only Edge files have signers. Hyper-tile files are Unsigned at best, so they can never be `signer_not_pinned`. AT-166 therefore implies `AcademicPaperRecord` extraction from an Edge file, but Scholar tile layouts in the Edge profile are not defined (and Edge reuses type numbers). The spec also never says that the walk uses `--pinned-keys-file`.

**Fix:** (a) state that the walk opens corpus files with the host pin list and skips Edge files for item 2, and drop the parenthetical from AT-166. Or (b) define Edge extraction.

### F-10 (minor) — AT-178's alternative fixture cannot exist
**Where:** AT-178 vs §5.4 ("If the pinned list is non-empty, the signer must be in it or … **no tile is a valid floor**").
- AT-178: "a valid-floor type-71 denial …, then a later type-71 grant … (or in a file Quarantined because the signer is not pinned)".

**Problem:** under `signer_not_pinned` there are no valid floors, so a "valid-floor denial" is impossible in that variant.

**Fix:** reword the variant as "a type-71 denial and a later grant in a file Quarantined `signer_not_pinned` (no valid floors; payloads withheld, banner count per AT-195)", or drop it, since AT-195 already covers it.

### F-11 (minor) — The §7 fixture-address rule contradicts AT-54 / DD-140
**Where:** §7 Fixtures paragraph vs AT-54 and DD-140.
- §7: "When a test uses both an LRS fixture and an `egress_test` fetch fixture, … the LRS fixture is `[::1]` or `127.0.0.2` (not `127.0.0.1`)."
- AT-54 deliberately configures the LRS at `http://127.0.0.1:<lrs-port>/` together with a fetch fixture.

**Fix:** scope the §7 sentence the way DD-140 does: "tests that need a **successful** fetch and an LRS …; AT-54's denial case is the stated exception."

### F-12 (minor) — Stale AT ranges and lists
**Where:** §5.1 "Test locations and pins (DD-121)"; §7 Guards bullet.
- "Pack tests (AT-01–AT-37, AT-41, AT-42, AT-44–AT-62, AT-64–AT-209 …)": this omits **AT-210** and **AT-211**, which are pack tests added in v15.
- The §7 guards bullet names AT-05, AT-36, and AT-41 as "skipped until the subject exists". AT-34 ("guard once the pack crate exists; skipped until then") and AT-39 (same pattern) are left out.

**Fix:** change the range to AT-64–AT-211 and add AT-34 and AT-39 to the guards sentence.

### F-13 (minor) — AT-183 cannot tell allowlist blocking from deny-list blocking or unreachability end-to-end
**Where:** AT-183; §5.2.4; AT-54 ("a real off-host address is not a CI fixture"); DD-140.

**Problem:**
- The "allowed" `https://host/` cases need a global-unicast HTTPS origin that validates against the system root store. CI cannot provide one.
- `[2001:db8::1]` is documentation space and is denied as non-global anyway.
- The "blocked" cases surface only as `fetch_error`, which they would also get from the deny list or from being unreachable. So the blocked assertions are vacuous.

**Fix:** (a) specify AT-183 as a unit test of a named matcher (for example `egress_allowlist_matches(entry, url) -> bool`) plus the parser errors. And/or (b) require a distinct `fetch_error` value (for example `"egress_not_allowlisted"`) with zero connects, exercised against the `egress_test` loopback port using IP-literal entries.

### F-14 (minor) — AT-194 / AT-210 say "no later *authentic* denial", which is looser than clause 2
**Where:** AT-194, AT-210 vs §5.3 item 4 clause 2 ("No parseable record from **any** tile (valid or invalid, trusted or not) … is a later denial").

**Problem:** a fixture with a later *unauthenticated* denial satisfies "no later authentic denial", yet clause 2 then withholds. That contradicts AT-194's "`withheld_count` is exactly 0".

**Fix:** reword both ATs as "no later denial, withdrawal, or expiry on any parseable tile". AT-194's second sentence already covers the post-break-denial case.

### F-15 (minor) — Engine `TileWriter` path APIs vs "creates and opens via the parent dirfd with `openat(O_NOFOLLOW)`"
**Where:** §5.2.2 `--working-archive` paragraph; §5.6 API and locking; DD-146; AT-203.

**Problem:** the grounded engine write APIs are `TileWriter::create` / `open_append`, and they take paths (§4.4 lists no fd constructor). The no-follow guarantee is tested only at startup (AT-203). Suppose the final component is swapped for a symlink (for example into `--corpus-root`) after startup. A path-based `open_append` follows it, which breaks "the corpus is read-only".

**Fix:** (a) require an fd-based `TileWriter` constructor in the pack's engine use, or (b) require an `lstat`/`fstat` (dev, ino) re-check against the startup identity before each step-1 write, failing with `write_failed`. Add a post-startup symlink-swap case to AT-203.

### F-16 (minor) — The consent file is not exclusive per host
**Where:** §5.2.1 DD-114 sidecar locks; §5.2.2 reserved names / pairwise-distinct rule; §5.2.3.

**Problem:** the four sidecar locks cover `--data-dir`, the working archive, the snapshot dir, and the download dir, but not `--consent-state-file`. The pairwise-distinct checks are per process only. Two hosts with distinct data dirs can share one consent file and race its temp+rename. A revoke via host A is then silently not honored by host B ("Mid-session outside edits are not honored"). Cross-host collisions between different flags (B's `--working-archive` = A's consent file) are also not rejected.

**Fix:** take a `LOCK_EX|LOCK_NB` sidecar on `--consent-state-file.lock`, or document cross-host consent-file sharing as an accepted misconfiguration.

### F-17 (minor) — Consent timestamp and file grammar are ambiguous
**Where:** §5.2.3.
- "The timestamp is that of the last successful host write or startup read that **changed** the stored state." A startup read never changes state, so it is unclear whether GET reports the file's stored timestamp after a plain restart.
- "The exact UTF-8 tokens `on` or `off`, a newline, and an RFC3339-seconds-UTC timestamp". It does not say whether a trailing newline after the timestamp is allowed, or whether its absence is "malformed" (a startup error).

**Fix:**
- State that after startup, GET returns the timestamp stored in the file, unless startup itself rewrote the file.
- Give the exact grammar: `("on"|"off") "\n" RFC3339 "\n"?`.

### F-18 (minor) — AT-180 / AT-209 expect `lrs_status: "skipped"` without stating that LRS is configured
**Where:** AT-180 second case, AT-209 download case vs §5.2.1 ("`not_configured` takes precedence over `skipped` when LRS is not configured").

**Fix:** add "with `--lrs-url` configured (otherwise `not_configured`)" to both ATs.

### F-19 (minor) — AT-06 "an `Upgrade: websocket` request returns 404" does not say which path
**Where:** AT-06 vs §5.2.1 (a valid token on a **known** path is routed normally).

**Problem:** a WebSocket upgrade sent to `/health` or `/v1/frame` hits a known route, so 404 is not implied by the normative text.

**Fix:** say "an `Upgrade: websocket` request to any path not in the table (for example `/devtools/page/1`) returns 404; on a known path, the Upgrade header is ignored, and no protocol switch (101) is ever returned."
