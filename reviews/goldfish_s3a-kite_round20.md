# Goldfish review — s3a-kite spec v20 (round 20)

Reviewer: independent fresh reviewer. Input read: `/workspace/goldfish/s3a-kite.md` (v20) only. I did not read any other repo files, prior reports, or dispositions.

**Summary: 0 blockers, 3 majors, 15 minors.**

Majors:
- F-01: AT-219 (and, depending on how it is read, AT-203) expects a `(dev, ino)` mismatch on an fd that was already validated. A swap of the directory entry cannot change that fd's identity, so a correct fd-based implementation fails the AT.
- F-02: AT-226's "newly observed LRS IP" clause cannot be set up under the AT's own rules: literal addresses only, no resolver override (DD-140), and no global-unicast destination.
- F-03: AT-125's window half depends on a trigger, "after the first warehouse frame is shown", that nothing in the spec makes the native window reach. A correct window may never fire the hook.

---

## F-01 — Major — AT-219 / AT-203 expect a `(dev, ino)` mismatch that an fd-based writer cannot see

**Refs:** §5.2.2 `--working-archive` paragraph; §5.6 "API and locking"; DD-28; DD-114; DD-146; §5.2.7 hook `pause_after_devino_check`; AT-203; AT-219.

**Problem.** The normative rule says every step-1 write works on a fd that has already been validated. The host does `openat(O_NOFOLLOW)` from the parent dirfd, then attaches the writer to *that fd* (either through an fd-taking `TileWriter` or through a `/proc/self/fd/<fd>` reopen of the same inode). After that it `fstat`s the writer's open fd and compares `(dev, ino)` to the recorded identity.

AT-219 pauses after the openat check and before the writer is attached. It then swaps the final path component for a symlink and expects `500 write_failed`, on the grounds that "the writer's open fd … fails the `(dev, ino)` re-check."

This cannot happen. Renaming, unlinking, or replacing a directory entry never changes the `(dev, ino)` of a fd that is already open, and a `/proc/self/fd` reopen yields that same inode. A correct implementation therefore passes the re-check and writes tile 10 into the originally validated inode. That inode may now be unlinked or renamed (for example `archive.bak`). The symlink target is untouched, which the AT also requires, but the response is `200`, not `500`. The AT cannot pass against an implementation that follows the normative fd rule.

A second consequence matters for correctness. The inclusion record can be committed to an orphaned inode while the host reports success, so the record is silently lost from the working archive at its configured path.

AT-203 has a related ambiguity. The spec never says whether the working-archive fd is re-opened on every step-1 hold or held from startup or first create. If the host keeps one fd for the life of the process (a reasonable reading of "records its `(dev, ino)` at startup"), AT-203's post-start swap also produces `200` plus a write to the old inode, not `500`.

**Why it matters.** A correct implementation fails a fail-first AT, and the stated mechanism (a re-check on the fd's identity) does not protect what the AT means it to protect: the binding between the name and the inode.

**Suggested fix.**
1. State that the host re-opens the final component with `openat(O_NOFOLLOW|O_NONBLOCK)` from the parent dirfd in **every** step-0/1 hold. No working-archive fd is kept across requests.
2. Replace (or add to) the fd re-check with a **name-binding re-check** taken immediately before each tile-10 write. `fstatat(parent_dirfd, basename, AT_SYMLINK_NOFOLLOW)` must succeed, must be a regular file, and must equal both `fstat(writer_fd)` and the recorded identity. Otherwise return `500 write_failed` and write nothing.
3. Rename the hook to `pause_before_name_binding_check`, placed between writer attach and that re-check, so AT-219's swap lands where the check can see it.
4. Add the residual to DD-146: a swap after the final check sends the write to the old inode, never to the symlink target.

---

## F-02 — Major — AT-226's LRS re-resolution case cannot be set up under its own constraints

**Refs:** §5.2.4 deny-list bullet; DD-140; AT-226; AT-54; §7 fixture paragraph.

**Problem.** AT-226 requires that "a hop that resolves to a newly observed LRS IP is denied even if that IP was not in the startup set and would otherwise be allowed." To exercise this, the LRS host must be a **hostname** whose resolution **changes** between Scholar start and the fetch. Three constraints block that:
- An `http` `--lrs-url` must be a loopback IP literal (§5.2.2), so nothing is resolved. Only an `https` hostname LRS can change resolution.
- The same AT says it uses "literal addresses and no resolver override (DD-140)". DD-140 also forbids a global-unicast destination.
- "Would otherwise be allowed" means the destination must be global unicast (forbidden as a fixture) or the `egress_test` loopback port. The latter means the LRS hostname must start resolving to `127.0.0.1` part-way through the test, which takes a mid-test system resolver change (`/etc/hosts`, needing root, or a DNS server). That is a resolver override in everything but name.

The spec also never says what happens when the `https` LRS hostname **fails to resolve at Scholar start** (startup error, or an empty startup deny set?).

**Why it matters.** As written, the AT clause cannot be built without breaking its own stated setup rules, so a correct implementation cannot show it passing.

**Suggested fix.** Either:
- (a) move the re-resolution property into a host-crate unit test, like AT-183, of a pure function `deny_set(startup_ips, connect_time_ips, addr)`; or
- (b) allow a hermetic resolver for this one AT: for example a network namespace with its own `/etc/hosts`, or a `test_hooks` point `lrs_resolution_override`, added to the §5.2.7 closed list and the DD-140 note.

Separately, state the startup behaviour when the LRS hostname does not resolve. A startup error that names `--lrs-url` is the fail-closed choice.

---

## F-03 — Major — AT-125's window hook trigger is unreachable

**Refs:** §5.2 "Native window (DD-60)"; §5.2.7 `issue` action on `window_issue_select`; AT-125.

**Problem.** The `issue` action fires "after the first warehouse frame is shown". The spec says nothing about what the native window does at startup: which file, if any, it opens, or how it gets to a warehouse frame (that takes `POST /v1/open` and `POST /v1/proceed`). The window's UI layout is also unspecified, so a test cannot drive it with synthetic clicks. A conforming window that shows an empty "open a file" state never reaches the trigger, and the AT's window half then hangs or fails. A hook needs `GET /v1/candidates` to find "a current candidate", and that call is not described as part of the window's own flow either.

**Why it matters.** The only AT that checks that window-originated writes carry the UI token (DD-32, DD-60) cannot be set up against a correct implementation.

**Suggested fix.** Make the `issue` action self-contained. On the window's first successful authenticated API call (or right after startup once `/health` succeeds), the window client issues `GET /v1/candidates` and then `POST /v1/select` for the first listed candidate, both with the Bearer token. Drop the "first warehouse frame" precondition, or add a hook action that also opens a named fixture and proceeds.

---

## F-04 — Minor — `--max-fetch-bytes == --max-open-bytes` lets the host publish a snapshot it cannot reopen

**Refs:** §5.6 Target (DD-103) and Download names; §5.2.2 AT-223 rule; §5.5 step 2b.

**Problem.** The startup check only requires `--max-fetch-bytes ≤ --max-open-bytes`, and its stated rationale covers only the download-dir hash check. A snapshot `.s3a` is always larger than its captured body (512-byte tile headers per 128 KiB, the 40-byte chunk prefixes, PROVENANCE, and the seal). With equal caps, a body near the cap produces a snapshot larger than `--max-open-bytes`. A download of that identity then always returns `snapshot_invalid` (the 2b reassembly open is capped), repeat selects return `snapshot_invalid`, and `/v1/open` returns `file_too_large`. The spec warns operators to size the caps, but the startup check gives a false sense of safety.

**Suggested fix.** Either check at publish time (if the sealed snapshot is larger than `--max-open-bytes`, do not publish; return `fetch_error: "snapshot_too_large"`), or make the startup check strict with a bound documented from `HYPER_TILE_PAYLOAD_SIZE`. Add an AT for the edge case.

## F-05 — Minor — `open_verified` accepts internally contradictory reports

**Refs:** §5.1 `open_verified` validation list; AT-211; DD-99.

**Problem.** The validation checks "quarantined ⇒ reason is Some" but not the converse, so `status: verified` or `unsigned` with `quarantine_reason: signer_not_pinned` is accepted. It also does not reject impossible combinations: `verified` / `self_signed` or `signature_known_signer` / `chain_break` / `missing_signature` on a hyper-tile, or `seal_trailer` on Edge. The DD-148 predicate is applied restrictively, but the UI label ("Verified") can then contradict the floors and banner.

**Suggested fix.** Add the checks "`quarantine_reason` is Some ⇒ `status == quarantined`" and the kind-specific enum constraints, all as `invalid_report`, and extend AT-211.

## F-06 — Minor — How step 0 reads the working archive's `file_uuid` and `ADDON_EDU` is unspecified

**Refs:** §5.5 step 0; §5.6 ADDON_EDU check; shared safe-open site list (§5.2.2); DD-146.

**Problem.** Step 0 needs the current archive's `file_uuid`, and the step-0/1 hold checks `addon_mask` even when step 1 is skipped. Neither header read is on the shared safe-open site list or tied to the validated fd, so a by-name read could follow a swapped symlink into `--corpus-root`. Also unspecified: what happens if the working archive is deleted mid-process and then re-created by `O_CREAT|O_EXCL` (new `(dev, ino)` versus the recorded identity). Is that `write_failed` until restart, or does the host re-record?

**Suggested fix.** State that the header read for `file_uuid` and `addon_mask` uses the same per-hold validated fd as step 1 (F-01), and define the mid-process re-create behaviour.

## F-07 — Minor — Atomic-publish details for `--data-dir` and consent files are incomplete

**Refs:** §5.2.1 revoke persist order; §5.2.3; §5.5 inclusion index and retry queue; DD-104; DD-113.

**Problem.**
- "temp + fsync + rename" never requires an fsync of the **parent directory** after the rename. After a power loss, the `revoke-pending` / consent-`off` renames, or the `lrs-retry/` delete, may not be durable even though the revoke returned `200`. The next start could then read `on` and replay queued statements. That is a consent-durability hole in the exact path `revoke-pending` exists to protect.
- Temp names and startup cleanup for the index, consent, and `revoke-pending` temps are unspecified. Only snapshot/download temps and ephemeral dirs are cleaned.
- The mode of the `lrs-retry/` **directory** is unspecified. "Files … 0600" cannot apply to a directory.

**Suggested fix.** Require an `fsync` of the parent directory after each rename or unlink in these paths, before the response. Name the temps (for example `.s3a-kite-tmp-{pid}-*`, reserved, cleaned under the sidecar lock). Set the `lrs-retry/` directory to `0700`.

## F-08 — Minor — No AT covers withholding inside the floor-transition card

**Refs:** §5.8 Animation ("showing floor-detail content in the card"); §5.3 item 4; AT-11; AT-62; AT-13.

**Problem.** `render_floor_transition` paints floor-detail content, yet every withholding AT (AT-11, 23, 176, 184, 200, 204) exercises only floor-detail or the banner. The general rule forbids leaking into any frame, but there is no AT for the transition card, `instant=true` included.

**Suggested fix.** Extend AT-11 and AT-23 so the withheld unique string is checked as absent from `floor-transition` frames at t = 0.225 and with `instant=true`.

## F-09 — Minor — "Absent from pixels" conflicts with the no-OCR rule

**Refs:** §5.8 ("never OCR"); AT-11, AT-23, AT-176, AT-184, AT-200, AT-204.

**Problem.** These ATs require the withheld string to be absent from **pixels**, but the spec forbids OCR and defines no other pixel oracle, so the clause cannot be tested as written.

**Suggested fix.** Define the oracle. For example: the pixmap is byte-identical to a render of a control fixture in which the withheld payload bytes are replaced by different bytes of the same length. Or drop "pixels" and rely on text_runs, semantics, and hits.

## F-10 — Minor — Payload rendering for "unknown layout" type-64 floors and BLOB floors is unspecified

**Refs:** §5.4 item 5 and item 6; §5.3 item 4 (Subject); AT-67; AT-107.

**Problem.** The Edge consent class is "all 32-byte type-64 event-log payloads". A type-64 tile with `record_count` 0, or with an odd record size, is an "unknown layout" floor, falls outside the class, and is never withheld. If its raw payload is painted, event bytes appear under a denial. Likewise, AT-107 says "the captured BLOB is viewable" without defining how BLOB bytes are rendered. If the fetched body is itself an S3A archive, painting its bytes would bypass that archive's consent table.

**Suggested fix.** State that unknown-layout type-64 floors and BLOB/PROVENANCE floors show metadata only (size, chunk index/total, hash, PROVENANCE text) and never raw payload bytes. Alternatively, treat unknown-layout type-64 as part of the event-log class for withholding.

## F-11 — Minor — Injection for startup-time deletion failures is undefined

**Refs:** §5.2.7 closed hook list; AT-124 (second half); AT-169 (queue-delete failure at startup); AT-181 (failed `revoke-pending` removal); AT-199 (failed delete).

**Problem.** `fail_lrs_retry_delete` is described for a revoke *after* the `revoke-pending` and consent writes. The spec does not say whether it also fires during the startup retry, or during the AT-199 startup without `--lrs-url`. There is no hook for removing `revoke-pending` at startup. Filesystem-permission injection would work for some of these cases, but those ATs lack the root-runner skip clause that AT-169, AT-171, and AT-181 carry.

**Suggested fix.** Declare that `fail_lrs_retry_delete` applies to every `lrs-retry/` delete, startup included. Add `fail_revoke_pending_remove`, or name the permission-based injection plus the root skip in each AT.

## F-12 — Minor — Byte-identity baseline in AT-24 and AT-25 is ambiguous

**Refs:** AT-24 ("byte-identical to the start"), AT-25.

**Problem.** If "the start" means before the host starts, a correct host fails. Startup creates `--data-dir/.lock` and possibly an empty `inclusion-index.jsonl` (DD-61), and writes startup lines to `--log-dir`.

**Suggested fix.** Define the baseline as the state after the host has bound and answered `/health`, as AT-29 effectively does.

## F-13 — Minor — AT wording errors

**Refs:** AT-183; AT-168.

**Problem.**
- AT-183 asserts that `2001:db8::1::443` "is a startup error", but the AT is a unit test of `egress_allowlist_matches(entry, url) -> bool`, which cannot express a startup error.
- AT-168 says "a page or warehouse … (for example a Continue or Select control)", but Continue exists only on first views.

**Suggested fix.** Split the startup-error case into a host-spawn check, or a `parse_allowlist_entry -> Result` unit test. Reword AT-168 to "a first view, page, or warehouse".

## F-14 — Minor — The `metadata.google.internal` deny is vacuous as a connect-time check

**Refs:** §5.2.4 ("Denial is at connect time on the resolved address"); AT-226.

**Problem.** The deny list names a **hostname**, while denial is defined on resolved addresses. In CI that name fails to resolve, so AT-226 passes whatever the implementation does. On GCP it resolves to `169.254.169.254`, which is already denied.

**Suggested fix.** Add a name-based deny applied before any DNS query, assert in AT-226 that zero DNS queries or connects occur for that name, or drop the hostname from the AT.

## F-15 — Minor — No bounds on inbound headers, fetch response headers, or LRS response bodies

**Refs:** §5.2.1 (`--max-request-bytes` covers bodies only); §5.2.4 (`--max-fetch-bytes` covers decoded bodies only); §5.5 LRS client.

**Problem.** An unauthenticated client (the request is parsed before the 401), a remote source server, or the LRS can send unbounded headers, or an unbounded LRS response body within the timeout. Common HTTP libraries cap these by default, but the spec invents no bound and does not hand the matter off.

**Suggested fix.** State that header block size is bounded by the HTTP library's limit (named), that the LRS response body is read to at most a fixed small cap or discarded, and that an excess is the corresponding error.

## F-16 — Minor — Normative rules with no covering AT

**Refs:** §5.2.6 matrix; §5.2 native window; §5.1 `hit_test`; §5.2.1 catalogue.

**Problem.** Rules without an AT:
- `POST /v1/proceed` and `view=warehouse` / `floor-*` on a `wiki` / `html` / `other` handle → `409 not_archive`;
- startup without `--headless` and with no display → error naming `DISPLAY`;
- `hit_test` on `floor-detail` / `floor-transition` with a prior frame → `None`;
- `proceed_to_warehouse` being a no-op when already in the warehouse;
- `500 render_failed` carrying `kite_code`.

**Suggested fix.** Add these cases to AT-172 or AT-171 and to AT-125. For `render_failed`, either name a reachable trigger or mark it untestable on purpose.

## F-17 — Minor — The host-spawn ATs need a binary and feature-build matrix that the spec does not define

**Refs:** §5.1 DD-121 (host crate out of workspace); AT-109; AT-125; AT-222; §7.

**Problem.** `kite-acceptance` is a workspace member, and `crates/s3a-kite` is excluded from that workspace, so `CARGO_BIN_EXE_*` is not available. The tests also need several builds of the same binary: with `egress_test`, with `test_hooks`, with `native-window`, and a release build with none of them. The spec does not say how tests find each artifact (for example an env var such as `S3A_KITE_BIN_EGRESS_TEST`), or which ATs use which build. "Fails at runtime if the binary is missing" is therefore underdetermined.

**Suggested fix.** Name the environment variables or paths for each feature build. Map each AT to the build it uses.

## F-18 — Minor — A missing recorded snapshot file permanently blocks that identity

**Refs:** §5.5 step 0; DD-103; DD-120; AT-209.

**Problem.** The reuse-failure list covers oversize, non-regular, symlink, and failed checks, but not a **missing** `{sha}.s3a`. Read as a failure, a deleted snapshot gives `snapshot_invalid` forever for that `(file_uuid, identity)`, with no re-fetch and no recovery short of a new working archive. Nothing says whether that is intended.

**Suggested fix.** Either list "missing" explicitly as a reuse failure and record the outcome as an accepted residual, or let a missing (as opposed to corrupt) recorded snapshot fall through to a re-fetch, with an AT.
