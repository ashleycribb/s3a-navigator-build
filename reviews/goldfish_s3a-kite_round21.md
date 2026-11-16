# Goldfish review: s3a-kite spec v21 (round 21)

Reviewer: independent fresh read of `/workspace/goldfish/s3a-kite.md` (v21) only. No other files were consulted.

**Summary: 0 blockers, 3 majors, 16 minors.**

Severity follows the brief: Blocker = unimplementable or consent leak; Major = AT impossible or failing on a correct implementation, a reachable security hole, a deadlock, or contradictory normative rules; Minor = everything else.

---

## Majors

### F-01 (Major): Rules contradict on when the working archive is created; AT-53, AT-74, and AT-79 depend on create-on-write, which v21 removed

**Where:** §5.2.2 (`--working-archive` paragraph: "Create uses `openat(O_CREAT|O_EXCL|O_NOFOLLOW)` **only at startup** if the file was missing … On first create at startup the host calls `enable_addon`"); DD-28, DD-146 ("create with `O_CREAT|O_EXCL` is startup-only"); §5.2.2 path rule ("or be exactly the canonical working-archive path (once created) … that file has not yet been created, `/v1/open` is `404` `not_found`"); §5.6 "API and locking" ("Those constructors [`TileWriter::create` / `open_append`] … are invoked only on the already-validated fd from the current step-0/1 hold"); error catalogue `write_failed` ("a running Scholar host cannot create or open the working-archive path on select (AT-74)"); AT-53, AT-74, AT-79, AT-173.

**Problem:** v21 makes creation startup-only: a running host never creates the archive, and if the file is missing mid-process the request fails with `write_failed`. Several places still assume the old create-on-first-select model:
- AT-79: "The **first successful Scholar write** creates the working-archive file with `TileWriter::create` and `enable_addon(ADDON_EDU)`." Under v21 the file already exists once `/health` answers, so a test that checks the file is absent before the first select, or that select creates it, fails against a correct implementation.
- AT-53, last sentence, and the §5.2.2 path rule: "`/v1/open` of that same canonical path **before the working-archive file has been created** is `404` `not_found`." A running host never has a not-yet-created archive. The test can only get there by deleting the file mid-process, and in that case the spec gives no defined error. The working-archive copy site lists `file_too_large`, `path_escape`, and `not_regular_file`, but not ENOENT.
- §5.6 says `TileWriter::create` runs on the fd "from the current step-0/1 hold", which is request-time creation.
- AT-74 and the `write_failed` row say "cannot be created … on select".
- The spec does not say what happens when the startup create fails (for example, the parent is not writable or `enable_addon` fails). It could be a startup error naming `--working-archive`, or a deferred failure.

**Why it matters:** Two normative statements disagree on observable behavior: when the archive file appears, and which component writes the ConsentRecord with which `granted_at`. AT-53's 404 case cannot be set up, and AT-79 as worded fails a correct v21 implementation.

**Fix:** Pick one model. If startup-only (the v21 intent):
- Reword AT-79 to say the host creates the archive at startup when the file is missing, with `TileWriter::create`, `enable_addon`, and the ConsentRecord, and that the first select appends to it.
- Change the §5.6 sentence to say `create` runs only at startup and `open_append` runs in the step-0/1 hold.
- Delete the "(once created)" / "not yet been created → 404" text from the path rule and AT-53, or replace it with an explicit rule such as: if the canonical working-archive path is missing mid-process, `/v1/open` returns `404` `not_found`.
- Reword AT-74 and the `write_failed` row to "cannot be opened (missing or identity changed)".
- Add: "If the startup create fails, startup is an error that names `--working-archive` and binds nothing," and add that case to AT-78.

### F-02 (Major): Under the §7 binary-feature matrix, several fetch-dependent ATs cannot be set up

**Where:** §7 (`S3A_KITE_BIN_TEST_HOOKS` list; "`S3A_KITE_BIN_EGRESS_TEST_HOOKS` … none is required today"; "Each host-spawn AT uses exactly one of these environment variables"; `S3A_KITE_BIN` list includes AT-227); §5.2.4 (loopback and all non-global addresses denied unless built with `egress_test`); DD-140 (no global-unicast destination in CI); AT-187, AT-212, AT-213, AT-214 (waiter case), AT-169 (mid-fetch case), AT-227.

**Problem:** A snapshot only comes from a step-2 fetch of a `source_url`. In CI the only reachable fetch target is a loopback fixture. That fixture is reachable only in a build with the `egress_test` feature plus `--egress-test-loopback-port`. Without the feature, that flag is a startup error and loopback is on the deny list. However:
- AT-187 (`pause_before_snapshot_rename`), AT-212 (`fail_snapshot_write_*`), AT-213 (`fail_snapshot_index_append`), and AT-214's "waiter of the GET" case all need a successful fetch **and** a test hook. They are assigned to `S3A_KITE_BIN_TEST_HOOKS`, which is `test_hooks` only, and §7 says the combined `egress_test`+`test_hooks` build is needed by no AT.
- AT-169's "concurrent select … mid-fetch" case has the same problem.
- AT-227 (`snapshot_too_large`) needs a successful fetch, but it is listed under `S3A_KITE_BIN`, which has no `egress_test`.

A correct implementation fails these ATs: every fetch returns `fetch_error`, so no snapshot temp, rename, index append, or publish-size check is ever reached.

**Why it matters:** These ATs cannot be set up as specified, so a correct implementation cannot pass them.

**Fix:**
- Move AT-187, AT-212, AT-213, AT-214 (at least its waiter case), and AT-169's mid-fetch case to `S3A_KITE_BIN_EGRESS_TEST_HOOKS`.
- Move AT-227 to `S3A_KITE_BIN_EGRESS_TEST`.
- Delete "none is required today".
- Optionally add a rule: "any AT that needs a successful source fetch uses an `egress_test` build."
- Also list AT-26/27/31/48/82/83/102/103/106/126/156/157/180/186/209/215/221 under `S3A_KITE_BIN_EGRESS_TEST` so the matrix is complete.

### F-03 (Major): AT-156 case 3 needs a pause between "LRS POST returned" and "retry-file write", but the closed hook list has none

**Where:** AT-156 third case; §5.2.7 ("Fail-first ATs that need in-process fault injection or a pause use the mechanism in this subsection and no other"; closed hook-point list); DD-102, DD-151.

**Problem:** Case 3 requires a revoke that lands **after the LRS POST returns 503 and before the retry file would be written**, and it asserts `lrs_status: "skipped"` and no retry file. That window is in-process and lasts only microseconds. If the revoke arrives during the POST, the POST is aborted (case 2). If it arrives after the mutex hold that writes the retry file, a correct host returns `queued` and the revoke then deletes the queue. Either way, AT-156 case 3's assertion fails against a correct implementation. No hook point covers this window (for example "pause after the LRS response, before the retry-write hold"), and §5.2.7 forbids any other mechanism.

**Why it matters:** The AT cannot be set up deterministically, so it fails or flakes on a correct implementation.

**Fix:** Add a hook point `pause_before_lrs_retry_write` (`pause_until_signal`) that fires after the LRS response is classified and before the mutex hold that writes the retry file. Cite it in AT-156 case 3. Assign AT-156 to `S3A_KITE_BIN_EGRESS_TEST_HOOKS`, since case 1 also needs a slow `egress_test` fetch.

---

## Minors

### F-04 (Minor): `snapshot_too_large` outcome incomplete for download and waiters
**Where:** §5.6 Target (AT-227); `/v1/download` row; §5.5 step 2 waiter outcomes; DD-112.
**Problem:** `snapshot_too_large` is defined for a select (200, `fetch_error: "snapshot_too_large"`). For a download it does not say whether `download_error` is `"fetch_failed"`, or what `path` and `snapshot` are, or that 2b is skipped. The waiter outcome list (snapshot path, fetch failure, `snapshot_conflict`, `write_failed`) does not mention it either.
**Fix:** State that on a download it is `download_error: "fetch_failed"`, `path: null`, `snapshot: null`, 2b skipped. State that waiters receive it as a fetch failure. Extend AT-227 with a download case and a waiter case.

### F-05 (Minor): The 413 header-block exception is missing from the normative error-precedence list
**Where:** §5.2.1 error-catalogue order ("authentication, then 405, then 413/415 …"); DD-125; 413 row; AT-162, AT-171.
**Problem:** The 413 row says header-block overflow is returned even without a valid token, which puts it before authentication. The precedence lists in §5.2.1 and DD-125 still put authentication first, so the two rules disagree for an unauthenticated oversized request.
**Fix:** Add to both lists: "(exception: a header block over `--max-request-bytes` is 413 before authentication; body overflow is 413 after authentication)."

### F-06 (Minor): Smaller error-precedence gaps
**Where:** §5.2.1 `invalid_view` rule; query-field types.
**Problem:**
- It is not stated whether an unknown `view` takes precedence over a missing or malformed `handle`. The list names only `w`, `h`, `floor`, and `t`.
- For `instant`, the text allows "a JSON boolean", but `/v1/frame` is a GET with query parameters only, so a JSON form cannot occur.
**Fix:** Name `handle` explicitly in the `invalid_view`-first rule (or explicitly exclude it). Drop "as a JSON boolean" for `/v1/frame`.

### F-07 (Minor): Unclear whether mark-withheld records count in `withheld_count`
**Where:** §5.3 item 4 (per-file and per-floor counts: "records this table withholds"; Unrestricted row: "`withheld_count` is 0 aside from marks"; "Withdrawn marks still withhold").
**Problem:** The count is defined as records the per-subject table withholds. The unrestricted wording ("0 aside from marks") implies mark-withheld records are counted too. Golden fixtures (AT-11, AT-79, AT-166) need one answer.
**Fix:** State whether `withheld_count` includes records withheld only by `MARK_CONSENT_WITHDRAWN` / `MARK_TARGET_ALL_RECORDS`. Add one AT row with a mark-only fixture.

### F-08 (Minor): Hyper-tile rendering of BLOB/PROVENANCE and unknown-layout records is specified only in the Edge section
**Where:** §5.4 item 5 ("BLOB and PROVENANCE floors likewise show metadata only … including when the captured body is itself an S3A archive"); §5.3 (no equivalent); AT-107.
**Problem:** Snapshots are hyper-tile files, but the metadata-only rule for BLOB and PROVENANCE floors appears only in the Edge section. §5.3 also does not say how a hyper-tile floor whose record layout kite cannot decode (so it cannot find a `subject_id`) is rendered in evaluated or default-withhold mode. Raw rendering could expose subject data that kite cannot attribute.
**Fix:** Move the BLOB/PROVENANCE metadata-only sentence to §5.3 (or a shared item). Add: "a hyper-tile floor whose layout kite cannot decode shows metadata only (type, size, `record_count`) and never raw payload bytes."

### F-09 (Minor): `open_verified` validation misses some report contradictions
**Where:** §5.1 `open_verified` checks; AT-211.
**Problem:**
- A report with `quarantine_reason` `chain_break`, `signature_known_signer`, or `crc` but `break_index: None` passes validation. DD-150 says those reasons always set `break_index`.
- A hyper-tile report with `status` `verified` or `self_signed` and no reason also passes, although the label table never produces those statuses for a hyper-tile.
These are caller-supplied (app) inputs, so this is hardening, not a leak path.
**Fix:** Add both checks (`invalid_report`) and the matching AT-211 cases.

### F-10 (Minor): No defined outcome when removing `revoke-pending` fails at runtime (revoke step 4)
**Where:** §5.2.1 revoke persist steps 1–4; DD-138, DD-113.
**Problem:** Fail-closed covers failures to write the consent file, to write `revoke-pending`, and to delete `lrs-retry/`. It does not cover the step-4 unlink of `revoke-pending`. The response code and whether the process exits are unspecified.
**Fix:** Either "a failed step-4 unlink is logged; the response is 200; the leftover is handled at the next start (AT-181)", or include it in DD-138's fail-closed list. Add a hook and AT if fail-closed is chosen.

### F-11 (Minor): How `drive_path` picks its starting root is unspecified
**Where:** §5.5 source decision table; §5.2.2 shared safe-open rule; DD-89, DD-146.
**Problem:** Request paths are lexically normalized and matched to a unique root before the dirfd walk. For `drive_path`, the spec says only "must canonicalize inside … allowed roots" and "walk each component with `O_NOFOLLOW`". "Canonicalize" suggests `realpath`, which follows symlinks, and that conflicts with the no-follow walk. A relative `drive_path` with `..` can cross from the dossier's root into another root, and the spec does not say which root dirfd the walk starts from.
**Fix:** State that `drive_path` (after resolving a relative path against the dossier parent) is lexically normalized, matched to the unique allowed root by path-component prefix, and walked from that root's startup dirfd with `O_NOFOLLOW`. No match, or more than one, means `on_disk: false`.

### F-12 (Minor): AT-183 placement contradicts §5.1 and §7
**Where:** §5.1 ("AT-183 does not live in `kite-acceptance`"; the crate's own tests); §7 ("Unit tests in `crates/s3a-kite` (AT-183 …) … do not spawn a binary"; `S3A_KITE_BIN_EGRESS_TEST` lists "AT-183's end-to-end fixture half"); AT-183 text (host-spawn startup-error case plus end-to-end case).
**Problem:** AT-183 includes host-spawn cases, but the spec says AT-183 does not live in `kite-acceptance` and that its tests do not spawn a binary. The startup-error half is not assigned to any `S3A_KITE_BIN*` variable.
**Fix:** Split AT-183 into a unit part (in `crates/s3a-kite`) and a host-spawn part (in `kite-acceptance`; startup case under `S3A_KITE_BIN`, end-to-end case under `S3A_KITE_BIN_EGRESS_TEST`).

### F-13 (Minor): AT-226 asks for "zero DNS queries" without saying how to observe them
**Where:** AT-226; DD-140 (no resolver override).
**Problem:** The test must show that `metadata.google.internal` causes no DNS query. The spec forbids a resolver-override flag and names no observation method (for example, a network namespace with a counting stub resolver in `/etc/resolv.conf`, or syscall tracing).
**Fix:** Name the permitted mechanism, for example: "the test runs the host in a network namespace whose `/etc/resolv.conf` points at a counting stub resolver; this is not a host flag."

### F-14 (Minor): AT-225 has an unspecified sort order and an unobservable assertion
**Where:** §5.5 item 2 ("sort the containing `.s3a` paths"); DD-61; AT-225.
**Problem:**
- The sort order (bytewise on canonical path, or locale order) is not stated.
- "A subsequent select of that identity succeeds against that record's file" has no observable effect. `AcademicPaperRecord` candidates have no source, and the inclusion record stores only the identity.
**Fix:** Say "bytewise ascending order of canonical absolute paths". Replace the last sentence of AT-225 with an observable check, or delete it.

### F-15 (Minor): AT-66 does not cover the case where both size flags are set
**Where:** §5.4 item 1 ("or both 4 KiB and 8 KiB size flags are set"); AT-66.
**Problem:** The normative rule has three triggers, and the AT covers only two.
**Fix:** Add a fixture with both `FLAG_TILE_SIZE_4KB` and `FLAG_TILE_SIZE_8KB` set: it should open as `unsupported` / `banner`.

### F-16 (Minor): AT-187's snapshot-temp protection is vacuous
**Where:** AT-187; DD-122, DD-136.
**Problem:** Host B uses a distinct `--snapshot-dir`, so B's startup cleanup never looks at A's `--snapshot-dir` temp. The "must not delete A's locked `--snapshot-dir` temp" assertion passes trivially. The sidecar locks mean no AT exercises the `LOCK_EX`-on-temp cleanup guard for snapshot or download dirs. Only the ephemeral-directory half of AT-187 is meaningful.
**Fix:** Either drop that clause, or add a case: plant a lockable stale `s3a-kite-tmp-*` file and a separately locked (live-looking) one in a fresh host's own `--snapshot-dir`, and assert that only the unlocked one is deleted.

### F-17 (Minor): ATs need conflicting runner privileges
**Where:** AT-117, AT-142 ("owned by another user"); AT-171, AT-169, AT-181 (non-root or skip).
**Problem:** Creating a 0600 file owned by another user generally needs root. The chmod-based ATs need a non-root runner and have skip rules. AT-117 and AT-142 have no skip rule for a non-root runner.
**Fix:** Add to AT-117/142: "the other-owner case requires root (or a pre-provisioned file) and is skipped and reported otherwise."

### F-18 (Minor): The AT-125 `--bind 0.0.0.0` without `--headless` case can be ambiguous
**Where:** AT-125; DD-127; §5.2 native window.
**Problem:** On a `native-window` build with no display, omitting `--headless` triggers two errors (`DISPLAY` and `--headless`). DD-127 lets the host name either one. The AT says every other flag is supplied, but the missing display is not a flag, so a correct host may name `DISPLAY` and fail the AT.
**Fix:** Run that case with `S3A_KITE_BIN` (no `native-window`), or only when a display is present.

### F-19 (Minor): Wording nits
- AT-46: the mid-process ADDON_EDU-clear case must clear the bit **in place** (same `(dev, ino)`). A rewrite-and-rename gives `500` `write_failed`, not `409` `addon_missing`. Say so.
- §5.5 step 3 calls the inhibit check "lock-free" and also re-checks the in-memory flag, which §5.2.1 says may be read only under the mutex. Say the flag check takes the mutex briefly (the inhibit check stays lock-free).
- §12: the v21 history entry sits between v7 and v20. Reorder.

---

## Blocker and major one-liners
- **F-01 (Major):** v21 makes working-archive creation startup-only, but AT-79, AT-74, AT-53 and §5.6 still assume create-on-first-select; AT-53's pre-create 404 cannot be set up, and startup-create failure is undefined.
- **F-02 (Major):** Under the binary-feature matrix, fetch-plus-hook ATs (AT-187, AT-212, AT-213, AT-214 waiter, AT-169 mid-fetch) run on a build without `egress_test`, and AT-227 runs on a build without it too; no successful fetch is possible there, so these ATs cannot be set up.
- **F-03 (Major):** AT-156 case 3 (revoke between LRS 503 and the retry-file write) needs an in-process pause that the closed §5.2.7 hook list does not provide, so it cannot be set up deterministically.
