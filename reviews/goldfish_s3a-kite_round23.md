# Goldfish review — s3a-kite spec v23 (round 23)

Reviewer: independent fresh review of `/workspace/goldfish/s3a-kite.md` only (no prior reports, dispositions, repo or web).
Date: 2026-11-16 (America/New_York)

**Summary: 0 blockers, 2 majors, 16 minors.**

Both majors are ATs that the spec's own closed test-hook mechanism (§5.2.7 / DD-151) cannot set up. I found no consent or privacy leak that follows from the normative text. The closest is F-10, an unspecified Edge tile-type display rule, which I rated minor.

---

## Majors

### F-01 (Major) — AT-229's first case can't be set up: one hook point fails both unlinks, and hooks are fixed at startup
**Refs:** §5.2.7 hook table (`fail_revoke_pending_remove`), DD-151, §5.2.1 consent transitions, DD-113, AT-229 (also affects AT-212 and AT-213 later-request clauses).

**Problem.** Hooks are set only through environment variables read at startup (§5.2.7, "Control channel"). There is no runtime way to turn one on or off. The spec doesn't say an action fires once: `fail_lrs_retry_delete` and `fail_download_write` are defined as failing **every** matching operation. DD-151 gives one point, `fail_revoke_pending_remove`, three uses: "startup, runtime step 4, and a qualifying `on` that unlinks a leftover". AT-229's first case needs:
1. the runtime step-4 unlink to **fail** (hook active), and then
2. "a later qualifying `POST /v1/consent {on}` **with the hook cleared**" to **succeed** at the same unlink, in the **same process**.

No mechanism can clear the hook mid-process. Restarting doesn't help either: on restart the startup path removes `revoke-pending` and forces consent `off` (AT-181), so the "later qualifying on removes the leftover" case never happens. The AT's second case (hook left in place, then fail-closed) is reachable. The first case isn't. AT-213 has the same problem in a weaker form: "a later select … reuses the left file". With `fail_snapshot_index_append` still active, that later select's index append fails again and returns `500`, so "reuses" can't be observed as a success. AT-212's "may retry the fetch" also runs against a still-active `fail_snapshot_write_*`.

**Why it matters.** A correct implementation can't pass AT-229's first case. It's a fail-first AT that can't be built.

**Fix.** Pick one:
- (a) Add a one-shot action, `fail_once_with_errno=N`, that fails only the first matching operation. Use it in AT-229 case 1, AT-212 and AT-213.
- (b) Split the point into `fail_revoke_pending_remove_startup`, `fail_revoke_pending_remove_runtime`, and `fail_revoke_pending_remove_on`. AT-229 case 1 sets only `_runtime`; case 2 sets `_runtime` and `_on`.

Also state explicitly in §5.2.7 that a hook action stays active for the whole process lifetime unless it is one-shot.

### F-02 (Major) — AT-187's consent-temp case needs a pause the closed hook list doesn't have
**Refs:** §5.2.7 ("Fail-first ATs that need in-process fault injection or a pause use the mechanism in this subsection and no other"; closed hook-point list), DD-122, DD-136, AT-187 (third case).

**Problem.** AT-187's third case says: "while A is paused in a consent-file publish with its `s3a-kite-tmp-*` held `LOCK_EX`, B's startup cleanup must not delete A's locked consent temp." The closed list has three pause points: `pause_before_snapshot_rename` (snapshot dir), `pause_before_name_binding_check`, and `pause_before_lrs_retry_write`. None pauses inside a consent-file (or `revoke-pending`) publish. A consent publish is a short temp, fsync, rename sequence, so nothing outside the host can hold A inside it. §5.2.7 forbids any other injection mechanism.

**Why it matters.** As written, that case can't be set up against a correct implementation. The DD-122 rule it guards (a lockable stale temp in the **consent-file parent** is deleted, a locked one is not) then has no reachable test for the cross-host case.

**Fix.** Pick one:
- (a) Add `pause_before_consent_rename` with `pause_until_signal`, covering the consent-file and `revoke-pending` publishes. List it as used by AT-187.
- (b) Rewrite the case so the test process plants and holds `LOCK_EX` on an `s3a-kite-tmp-{pid}-*` file in the shared consent parent. The second case already does this for snapshot and download dirs. Then assert that B's startup leaves it in place and deletes a lockable sibling.

---

## Minors

### F-03 (Minor) — §5.5 says "any index line" where step 0 and DD-104/120/152 say "last line"
**Refs:** §5.5 source-decision paragraph ("If a **usable snapshot** exists for `(file_uuid, identity)` (a non-null `snapshot_sha256` on **any** index line for that key whose `{sha}.s3a` is present and passes…)"); §5.5 step 0; DD-104; DD-120; DD-152; AT-209; AT-228 third case.
**Problem.** Step 0, DD-120 and DD-152 say usability looks **only** at the last line with a non-null sha, "even if an earlier line's sha would be usable". Under the "any line" reading, a download in the AT-209 setup could deliver from an earlier usable sha instead of returning `snapshot_invalid`.
**Fix.** In the §5.5 paragraph, replace "on any index line for that key" with "on the last index line for that key that has a non-null `snapshot_sha256` (step 0, DD-152)".

### F-04 (Minor) — `break_index` restatements say "the failing tile onward", but the rule is the highest preceding authentic `tile_index` + 1
**Refs:** DD-57 ("invalidates the tile at `tile_index` and every subsequent tile"); DD-148 ("An Edge chain break unauthenticates only the breaking tile and later tiles"); AT-35 ("invalidates the signed chain from that `tile_index` onward"); vs §5.4 DD-145 rules 3/4, the DD-150 table, DD-111, AT-201(2)(7), AT-220 sparse variant.
**Problem.** With sparse signing, the normative `break_index` is (highest preceding authentic `tile_index`) + 1. That can be well before the failing record's `tile_index`, and the uncovered gap tiles are untrusted. The three restatements could be read as leaving gap tiles valid or trusted. §5.4 is authoritative, so this is minor. It's still a consent-relevant reading trap: gap-tile grants must not release.
**Fix.** Reword all three to: "every tile at or after `break_index` = (highest preceding authentic `tile_index`) + 1, or 0".

### F-05 (Minor) — Unclear whether "uncommitted" tiles after a mid-file torn or CRC stop belong in `torn_tile_indices`
**Refs:** §5.1 mapping row `torn_tile_indices`; §5.4 per-tile outcomes ("every tile after the first torn or uncommitted tile is uncommitted"); DD-150 rows "Edge: torn-only" ("Torn tiles are in `torn_tile_indices`"); hyper-tile torn row ("that tile and later indices … are listed in `torn_tile_indices`"); AT-165.
**Problem.** The `torn_tile_indices` predicate is index ≥ `tile_count`, a header that is absent, erased or invalid, or a partial trailing tile. But the field is described as "torn **or uncommitted** tiles", and §5.4 calls every tile after the first torn tile (or after a CRC stop) "uncommitted". For an Edge file with a torn tile at *t* < `tile_count` and later tiles that have valid headers, one implementer will put *t*+1… in `torn_tile_indices` and another won't. AT-165 requires the pack and reader-core to agree exactly on this field.
**Fix.** State explicitly that `torn_tile_indices` contains only the indices that satisfy the predicate. Tiles invalidated only for following a torn or CRC stop go in `invalid_tile_indices`, not `torn_tile_indices`. Alternatively, state the opposite. Either way, make the hyper-tile row and AT-165 match.

### F-06 (Minor) — `open_path`: both the host and the pack "read at most cap + 1 bytes from the fd"
**Refs:** §5.2.2 path rule ("It reads at most `--max-open-bytes` + 1 bytes from that fd … The host uses that fd … calls pack `open_path` with that fd"); §5.1 `open_path` ("reads at most `max_open_bytes` + 1 bytes from `fd`"); `paper_identity` ("the bytes already read from `fd`").
**Problem.** If the host reads first, the fd offset is at EOF, so a plain `read` in the pack sees no bytes. If both read, the host's size check and the pack's bytes can differ when the file changes between reads. The spec doesn't say who reads, or whether `pread` from offset 0 is used.
**Fix.** State that the host only `fstat`s the fd (size and type) and doesn't consume it. The pack does the single bounded read with `pread` from offset 0 (or `lseek` to 0 first). `file_too_large` comes from whichever component reads.

### F-07 (Minor) — `invalid_report` doesn't reject a hyper-tile report with `signer_not_pinned`
**Refs:** §5.1 `open_verified` checks; AT-211; DD-141 ("`FLAG_ED25519_SIGNED` and type-70 tiles are Edge-only; this spec does not define them on a hyper-tile").
**Problem.** Hyper-tile `signature_known_signer`, `chain_break` and `missing_signature` are rejected, but `signer_not_pinned` isn't, though it's equally undefined for hyper-tiles. The effect only restricts (whole file unauthenticated), so it isn't a leak. It is an inconsistent validation rule, and AT-211 doesn't cover it.
**Fix.** Add `signer_not_pinned` to the hyper-tile prohibited reasons in §5.1 and AT-211.

### F-08 (Minor) — An oversized LRS response doesn't fit the queued/failed/permanent rules
**Refs:** §5.2.1 ("exceeding either fails that attempt (`lrs_status: "failed"`)"); DD-108; DD-131; DD-134; AT-191.
**Problem.** DD-131 defines `failed` as "no retry file could be written, or the outcome was permanent (`{id}.permanent` written)". An oversized LRS header block or body is neither. The spec doesn't say whether a `.permanent` file is written, whether the statement is queued, or whether it's dropped. A 2xx status with an oversized body means the LRS probably accepted the statement. Separately, the spec never says how a failed retry-file write is handled: is it fail-closed like other consent persistence?
**Fix.** Classify the case explicitly. For example: a known status line with an oversized remainder follows the status class (2xx is `sent`, 5xx is queued); an oversized header block before the status is read is a permanent failure (`.permanent` written). Extend AT-191 to match. State how a retry-file write failure is handled (log, `failed`, no exit), and add an AT.

### F-09 (Minor) — It's unclear whether `--handle-idle-secs` bounds the TLS handshake
**Refs:** §5.2.2 `--handle-idle-secs` row; DD-108; AT-230.
**Problem.** The bound covers time to send "the request header block" and idle time "between request bytes". On a non-loopback TLS bind, a peer that completes TCP but stalls the TLS handshake hasn't sent any request bytes. An implementation that starts the timer after `accept` of the TLS stream would hold that connection forever. With no connection cap (Q-20), that's an unbounded hold by an untrusted peer. The intent is clear, so this is minor.
**Fix.** State that the bound runs from TCP accept and includes the TLS handshake. Add an AT-230 case where a peer opens TCP and never sends a ClientHello (or stalls mid-handshake), and the connection is closed.

### F-10 (Minor, privacy-adjacent) — No display or consent rule for Edge tile types other than 64/70/71
**Refs:** §5.4 item 5 (only type 64 and BLOB/PROVENANCE are specified); §5.3 item 5 (the "cannot decode → metadata only" rule is stated for hyper-tile floors); §4.2 (types 64, 71 and 75 are reused across layouts); §5.3 item 4 Subject (Edge consent covers only 32-byte type-64 event-log payloads).
**Problem.** Valid Edge floors of any other type (for example Edge type 75) have no specified rendering. Edge consent gates only type-64 event logs. If an implementer renders another Edge payload type through `edge_display_layouts()`, which §5.4 already uses for identity records, those payloads would show without the type-71 check.
**Fix.** Add to §5.4 item 5: every Edge floor type other than 64 (32- or 96-byte layouts), 70 and 71 shows metadata only (type, size, `record_count`) and never raw or decoded payload fields. Add an AT with an Edge type-75 (or unknown-type) floor and a research denial.

### F-11 (Minor) — AT-172's `floor-detail` on a page handle doesn't say whether `floor` is supplied
**Refs:** AT-172; §5.2.1 error precedence (param errors before the §5.2.6 matrix).
**Problem.** "`/v1/open` of a `wiki` / `html` / `other` page followed by … `GET /v1/frame` with … `view=floor-detail` returns `409` `not_archive`" only holds if `floor` (and `w`/`h`) is supplied. Without `floor`, precedence gives `400` `missing_param`.
**Fix.** Write the request as `view=floor-detail&floor=0&w=…&h=…`. Optionally add the omitted-floor variant expecting `400` `missing_param`.

### F-12 (Minor) — AT-198's interleavings aren't deterministically reachable, and AT-198 has no binary variable
**Refs:** AT-198 (on-in-flight vs revoke; same-state-off receipt then `on` wins the mutex; client disconnect during the revoke hold); §5.2.7; §7 `S3A_KITE_BIN*` lists.
**Problem.** The cases need a request parked between receipt and mutex acquisition, or a disconnect inside the revoke's mutex hold. No hook does that. A test can approximate it by parking another request in `pause_before_snapshot_rename`, which holds the mutex, but the spec doesn't say so. For the disconnect case, if the client closes before the receipt-time actions run, the test passes trivially. AT-198 also isn't assigned to any `S3A_KITE_BIN*` variable.
**Fix.** Either add a hook `pause_after_consent_receipt` (after receipt-time latch actions, before taking the mutex), or document that AT-198 holds the mutex with `pause_before_snapshot_rename`. Assign AT-198 to `S3A_KITE_BIN_EGRESS_TEST_HOOKS`.

### F-13 (Minor) — The temp file's own `LOCK_EX` may conflict with engine locking on a second file description
**Refs:** DD-122 / §5.6 ("held `LOCK_EX` on its own fd from create until publish or delete"); §4.4 (`try_lock_exclusive`, `seal_archive`); §5.2.2 (`/proc/self/fd/<fd>` reopen).
**Problem.** `flock` locks belong to an open file description. Building the snapshot with `TileWriter` or `seal_archive` on a path, or through `/proc/self/fd/<fd>`, opens a new file description. If the engine calls `try_lock_exclusive` (`flock`) on it, that conflicts with the host's own `LOCK_EX` on the temp, and snapshot creation fails every time. The spec allows the `/proc/self/fd` route for the working archive, where no conflicting host lock exists, without warning about this case.
**Fix.** Require that snapshot temps are written and sealed through an fd-taking engine API on the same file description that holds `LOCK_EX`. Or require the engine lock to be taken on that same fd. Note this next to the `/proc/self/fd` option.

### F-14 (Minor) — AT-165 doesn't fix which `s3a-engine` revision the reader-core under test links
**Refs:** §4.5 (reader pins engine `ff89564`, "behind the storage commit grounded here"); §5.1 "Test locations" (the reader test crate uses path deps); AT-165; AT-220 (chain-inactive seal-hash recompute behavior is grounded at storage `b7bdcda`).
**Problem.** Agreement between pack and reader-core on the AT-220 and AT-210 fixtures depends on engine behavior (for example, recomputing `seal_hash` whenever type-70 records exist). That behavior is grounded only at storage `b7bdcda`. If the reader test crate resolves `s3a-reader-core` against engine `ff89564`, AT-165 may fail for reasons outside the pack.
**Fix.** State that the out-of-workspace reader test crate builds `s3a-reader-core` against the same `s3a-engine` path or revision as the pack (storage `b7bdcda` or later).

### F-15 (Minor) — Sidecar `.lock` opens aren't covered by the safe-open rule
**Refs:** §5.2.1 DD-114 lock 1; §5.2.2 shared safe-open rule (site list); AT-164; AT-208.
**Problem.** `--data-dir/.lock`, `--working-archive.lock`, `--snapshot-dir/.lock`, `--download-dir/.lock` and `--consent-state-file.lock` are opened, or created when missing, without `O_NOFOLLOW|O_NONBLOCK` or a regular-file check. A FIFO at one of those names hangs startup. A symlink makes the host create or lock a file outside the directory. These paths are operator-controlled, so this is hardening.
**Fix.** Open sidecar locks with `openat(dirfd, name, O_CREAT|O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC, 0600)` and require `fstat` to show a regular file. Otherwise fail startup naming the path. Add these opens to the safe-open site list and add a case to AT-164 or AT-208.

### F-16 (Minor) — Startup temp cleanup can delete a working archive or consent file named `s3a-kite-tmp-*`
**Refs:** DD-122; §5.6 "Atomic publish and temp cleanup"; reserved names (§5.2.2, AT-208).
**Problem.** Startup deletes any lockable same-euid `s3a-kite-tmp-*` **file** in `--data-dir` and in the consent-file parent. A `--working-archive` placed in `--data-dir` with that basename (allowed: the parent only has to be disjoint from snapshot, download and log), or a consent file with that basename, would be deleted at startup. That loses research data or the consent state.
**Fix.** Add a reserved-name rule: the `--working-archive` and `--consent-state-file` basenames must not match `s3a-kite-tmp-*` (startup error naming the flag). Add a case to AT-208.

### F-17 (Minor) — Normative codes and rules with no covering AT, or a mismatched one
**Refs:** §5.2.1 error catalogue; §5.2.4 `fetch_error` vocabulary ("ATs assert the specific code named below"); DD-134; §5.2.3 grammar; §5.2.1 frame params.
**Problem / gaps:**
- `415` when `Content-Type` is **missing** on a JSON-body route: AT-171 tests only `text/plain`.
- `fetch_error: "connect_failed"`: no AT asserts it.
- `fetch_error: "scheme_not_allowed"`: §5.2.4 cites AT-123, but AT-123's `file://` `source_url` is stored as `null` and never fetched. The code is only reachable through a redirect `Location` with a non-http(s) scheme, which no AT exercises.
- LRS `408` and `429` are queued: AT-191 tests only `503`.
- A consent file without the trailing newline is valid: no AT.
- `floor-transition` with `instant=false` and `t` omitted gives `400` `missing_param`: no AT.

**Fix.** Extend AT-171 (missing Content-Type; omitted `t`), AT-88 or AT-123 (a redirect to `ftp://` or `file://` gives `scheme_not_allowed`), AT-183 or AT-103 (an allowed fixture port with nothing listening gives `connect_failed`), AT-191 (408 and 429 queued), and AT-151 or AT-84 (a no-trailing-newline file is accepted).

### F-18 (Minor) — Other restatements that drift from the authoritative text
**Refs:** DD-93 (the viewer-only list names only `--log-dir` and `--pinned-keys-file` vs `--view-root`) vs §5.2.2, DD-129 and AT-179 (which also include the token file and TLS files); DD-130 (the `invalid_tile_indices` union leaves out the unsigned tail) vs the §5.1 mapping row and AT-210.
**Problem.** §5.2.2 and §5.1 are authoritative, but readers implementing from the DDs will miss constraints.
**Fix.** Add "token and TLS files" to DD-93's viewer clause. Add "and, when any authentic type-70 record exists, the unsigned tail (verifying container exempt)" to DD-130.

---

## Checked and not flagged (for the record)
- Error precedence for `/v1/frame` and `/v1/hit-test` (`invalid_view` first, matrix before `no_frame`) agrees across §5.2.1, §5.2.6, DD-125, AT-172 and AT-196.
- Bezier mid-point: cubic-bezier(.2,.8,.2,1) at x=0.5 gives y≈0.9461, opacity≈0.957, matching §5.8. Bay sides 32/45/181 are correct.
- The consent latch (DD-144) has no stuck state: a revoke always decrements, startup `revoke-pending` raises `epoch` and a later `on` clears it by CAS, and the AT-198 orderings come out `off` regardless of mutex order.
- The size caps fit together: `--max-fetch-bytes` ≤ `--max-open-bytes`; snapshots are never published above the cap; reuse, 2b and the download hash check all read at most cap + 1.
- The withholding table, the counting rules, and AT-11/23/85/112/134/135/136/176/177/178/184/185/194/195/200/204/205/210 are mutually consistent.
