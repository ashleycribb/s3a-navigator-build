# Goldfish review — s3a-kite spec v22 (round 22)

Reviewer: independent fresh review. Input read: `/workspace/goldfish/s3a-kite.md` (v22) only. No prior reports, dispositions, repo trees, or web sources were consulted. Line references are `L<n>` in that file.

**Summary: 0 blockers, 2 majors, 11 minors.**

---

## F-01 — Major — AT-225 asserts a `page_path` that the normative text defines as `null`

**Where:** AT-225 (L1216); §5.2.1 `/v1/candidates` row (L273); §5.5 item 1/2 candidate fields (L682); AT-101 (L1045); DD-61 (L1293); history v22 "observable `page_path`" (L1470).

**Problem.** AT-225 requires that, for two corpus `*.s3a` files holding the same `AcademicPaperRecord.paper_id`, "the listed candidate's `page_path` is the canonical path of that first file, not the later path." But the normative text says `page_path` is "the absolute canonical path of the winning-root dossier file, or `null` for an `AcademicPaperRecord` candidate, which has no dossier path" (L682), the wire contract says `page_path` is an "absolute canonical dossier path or null" (L273), and AT-101 asserts "An `AcademicPaperRecord` candidate has `page_path` `null`." §5.5 also promises that `POST /v1/open` of a non-null `page_path` yields `paper_identity` equal to the candidate `id`, which can never hold for a `.s3a` path (`paper_identity` is only `Some` for `papers/SRC-*.md` dossiers via `open_path`).

**Why it matters.** A correct implementation cannot pass both AT-101 and AT-225. If it follows §5.2.1/§5.5, AT-225 fails. If it follows AT-225, AT-101 and the `page_path`/`paper_identity` promise break. Also, every other field of an `AcademicPaperRecord` candidate (`id`, `title: ""`, `source_url: null`, `on_disk: false`, `page_path: null`) is the same for both duplicate records, so the "first file in bytewise path order" rule (DD-61) has no observable output through the API as specified.

**Fix (pick one, and apply it everywhere):**
(a) Leave `page_path` as `null` for `AcademicPaperRecord` candidates. Add a separate optional field, e.g. `archive_path: <canonical .s3a path or null>`, to the `/v1/candidates` schema (L273) and to §5.5, and have AT-225 assert `archive_path`. Or
(b) Reduce AT-225 to "listed exactly once" and say in DD-61 that the choice of record is not observable in v1.
Do not redefine `page_path` for `.s3a` files unless the `POST /v1/open` → `paper_identity` sentence in §5.5 and AT-101 are changed to match.

---

## F-02 — Major — the "control-fixture pixmap oracle" breaks for a correct implementation that paints payload-derived digests

**Where:** §5.8 oracle (L837); used by AT-11, AT-23, AT-176, AT-184, AT-200, AT-204 (and their floor-transition sub-cases); §5.3 item 3 (`payload_hash_short`, L538); §4.2 floor detail fields (L113).

**Problem.** The oracle requires the fixture's pixmap to be **byte-identical** to a control render in which "those withheld payload bytes have been replaced by different bytes of the same length (the layout is otherwise unchanged)." Changing payload bytes always changes values derived from them:
- the tile payload SHA-256, so `payload_hash_short` (a normative floor field, §5.3 item 3) changes;
- the tile CRC, which the fixture builder must recompute or the tile becomes CRC-invalid and the floor list changes;
- for sealed or signed fixtures, the seal trailer, Edge `seal_hash`, and type-70 signatures.

The spec does not say that `render_floor_detail` or the `floor-transition` frame (whose card shows "floor-detail content", and whose background is unspecified) leaves out `payload_hash_short` or other digest/CRC text. An implementation that shows the floor's `payload_hash_short` in the floor-detail header, or on warehouse bays visible behind the transition card, is correct under §5.3. It would still render different pixels for fixture and control, and fail every AT that uses this oracle.

**Why it matters.** Several privacy ATs could fail against a correct implementation, or force one particular UI choice that the spec never states.

**Fix.** Choose one:
(a) State normatively that `floor-detail` and `floor-transition` frames do not paint payload-derived digests, CRCs, or seal/signature values. Also specify what the transition frame paints outside the card.
(b) Redefine the oracle: compare the pixmaps after masking the rects of every `TextRun` whose text differs between fixture and control, other than runs that contain the withheld string. Fail if a masked run contains the withheld string. The fixture builder must also rebuild CRCs, and for sealed or signed fixtures it must re-seal or re-sign, or use Unsigned / unsealed fixtures.
Either way, list in §5.8 which derived fields the control may differ in.

---

## F-03 — Minor — Stale restatements contradict DD-152 (a missing recorded snapshot is re-fetched)

**Where:** §5.2.1 `/v1/select` row (L274, "If a snapshot is recorded but the reuse checks fail … no fetch is started"); §5.5 step 2 / failure paragraph (L705, "A repeat retries the fetch when no index line … has a non-null `snapshot_sha256`"; "A fetch is not started when a snapshot already exists"); inclusion-index paragraph (L718); DD-120 (L1352, "Step 2 runs if no index line … has a non-null `snapshot_sha256` … A repeat of a recorded snapshot runs the DD-103 reuse checks; failure is … `snapshot_invalid` … no re-fetch").

**Problem.** Step 0 (L699), DD-103, and DD-152 say a *missing* recorded `{sha}.s3a` is a re-fetch, not `snapshot_invalid`. These four restatements do not make that exception. Read literally, a missing file "fails the reuse checks", which gives `snapshot_invalid` and no fetch, so they disagree with AT-228. §5.5 step 0 and DD-152 are clearly the intended authority, so this is Minor.

**Fix.** In each restatement, replace "recorded … fails the reuse checks" with "recorded, **present**, and fails the reuse checks". Change "a snapshot already exists" to "a **usable** snapshot exists (DD-152)". In DD-120 and L705, add "or the only recorded `{sha}.s3a` files are missing (DD-152)" to the re-fetch condition.

---

## F-04 — Minor — Step 0 is ambiguous when one recorded sha is usable and another is present but fails

**Where:** §5.5 step 0 (L699); DD-152 (L1384); AT-228 second case.

**Problem.** After a DD-152 re-fetch that produced `new_sha`, the key has two index lines (old sha, new sha). Step 0 says a usable snapshot exists if *any* line's sha passes the checks. It also says that if *a* recorded sha's file is present but fails, the request returns `snapshot_invalid` and starts no fetch. If the old `{sha}.s3a` reappears corrupted while `{new_sha}.s3a` is valid, the two sentences give different observable results (deliver the new snapshot vs `snapshot_invalid`).

**Fix.** Say that step 0 looks only at the sha on the **last** line for the key that has a non-null sha (consistent with "last matching line wins", DD-104). Alternatively, give precedence explicitly: "if any recorded sha is usable, use the one from the latest line; `snapshot_invalid` only if none is usable and at least one is present but failing." Add that sub-case to AT-228.

---

## F-05 — Minor — Garbled 2b condition in step 0

**Where:** §5.5 step 0 (L699): "Run 2b for a download request only when delivering on-disk or a usable snapshot (not after a failed reuse check, and after a successful re-fetch publish)."

**Problem.** The parenthetical can be read as "not … after a successful re-fetch publish", which contradicts AT-228 ("2b runs") and DD-152 ("A download then runs 2b from the published snapshot"). It also does not clearly say that 2b runs after a *first* fetch-and-publish (row 1 of the source table).

**Fix.** Reword: "Run 2b for a download only when there are bytes to deliver: a usable on-disk fd, an existing usable snapshot, or a snapshot this request (or the GET it waited on) has just published. Do not run 2b after a failed reuse check, a fetch failure, `snapshot_too_large`, or `snapshot_conflict`."

---

## F-06 — Minor — A stale `revoke-pending` can silently undo a later explicit `on` at restart

**Where:** §5.2.1 revoke step 4 (L344); DD-113 (L1345); startup handling (L352).

**Problem.** If the runtime unlink of `revoke-pending` fails after steps 1–3, the revoke returns 200 and leaves the file. A later qualifying `POST {"state":"on"}` persists `on` but nothing removes `revoke-pending`. At the next start the host sees `revoke-pending`, treats consent as `off`, and rewrites the consent file to `off`. That reverses an explicit, later consent decision without telling the researcher, and it conflicts with "`off` is final until a later explicit `POST on`" (DD-142), which implies the later `on` should hold. The rule also has no covering AT: `fail_revoke_pending_remove` is defined only for startup (L517), so the runtime 200 path cannot be injected.

**Fix.** Require a qualifying `on` to remove a stale `revoke-pending` (unlink plus parent `fsync`) before it persists `on`, and treat failure of that unlink as a DD-138 fail-closed event. Alternatively, record the timestamp in `revoke-pending` and let startup honor a consent file whose `on` timestamp is later. Extend the hook so it applies at runtime too, or add `fail_revoke_pending_remove_runtime`, and add an AT for the runtime 200 path.

---

## F-07 — Minor — Locking of consent-file and index temps is not stated, but cross-host cleanup relies on it

**Where:** §5.6 atomic publish (L768); DD-122 (L1354); §5.2.1 sidecar locks (L332).

**Problem.** Startup deletes stale `s3a-kite-tmp-*` files in "the consent-file parent" whenever `LOCK_EX|LOCK_NB` succeeds. Two hosts may legally keep *distinct* consent files in the *same* parent directory. The `.lock` sidecar is per file, not per directory. §5.6 states the "hold `LOCK_EX` on the temp's own fd" rule in the snapshot/download context and only says the same *name pattern* applies to consent and index publishes. If an implementation does not lock its consent temps, host B's startup can delete host A's live consent temp. A's rename then fails and A exits fail-closed (DD-138).

**Fix.** State that every `s3a-kite-tmp-*` temp (snapshot, download, inclusion index, consent file, `revoke-pending`) is held `LOCK_EX` on its own fd from create until publish or delete. Optionally add an AT-187 case with two hosts sharing a consent-file parent.

---

## F-08 — Minor — `fetch_error` has no defined vocabulary

**Where:** §5.2.4 (L455–L463); §5.2.1 select/download rows; AT-183 (L1151), AT-54, AT-88, AT-122, AT-139, AT-140, AT-226.

**Problem.** Only `egress_not_allowlisted` and `snapshot_too_large` are named. AT-183 requires `egress_not_allowlisted` to be "distinct from a deny-list or unreachability `fetch_error`", but neither of those values, nor timeout, redirect-limit, non-2xx, unknown-encoding, oversize-body, oversize-header, or scheme-rejected, is defined. ATs can only assert that the field is non-null, and clients cannot tell the failures apart.

**Fix.** Add a closed `fetch_error` enum to the §5.2.1 catalogue, for example `egress_denied`, `egress_not_allowlisted`, `scheme_not_allowed`, `connect_failed`, `timeout`, `too_many_redirects`, `http_status`, `unknown_encoding`, `body_too_large`, `headers_too_large`, `snapshot_too_large`. Have the listed ATs assert the specific value.

---

## F-09 — Minor — AT-19 has no defined way to find the severity badge pixels

**Where:** AT-19 (L956); §5.8 `SemanticNode` roles (L826); §5.4 item 5 (L667).

**Problem.** AT-19 asserts "three pairwise-different badge colors (pixel samples)". The badge is not a semantic role, and §5.8 gives no rect for it, so the test cannot reliably sample badge pixels without OCR or layout guesses (which §5.8 forbids in spirit).

**Fix.** Require each event row's severity badge to be exposed as a `SemanticNode` (for example role `label` with label `severity: N`) carrying its CSS rect. AT-19 then samples the pixmap at that rect's centre, scaled by `device_pixel_ratio`.

---

## F-10 — Minor — Unspecified error when an existing working archive is not a readable S3A file, or `lrs-retry` is not a directory

**Where:** §5.2.2 `--working-archive` (L410); §5.6 API and locking (L751); §5.5 retry queue (L739).

**Problem.** (a) An existing regular-file working archive that is not S3A, or whose header fails `FileHeader::verify`, passes startup ("the `ADDON_EDU` bit is not checked at startup"). The step-0/1 header read for `file_uuid` / `addon_mask` then fails, and no code is assigned (`addon_missing`? `write_failed`? `internal`?). (b) `--data-dir/lrs-retry` is created mode `0700` "if missing". Behaviour when it exists as a regular file, FIFO, or symlink is not specified, both for queue writes and for the revoke "delete the directory" step. A recursive delete that follows a symlink would leave `--data-dir`. Operator-local only, hence Minor.

**Fix.** (a) Make an unreadable or invalid working-archive header in the step-0/1 hold `500` `write_failed` (nothing written, no `lrs_status`), or validate the header at startup and fail naming `--working-archive`. Add an AT. (b) Require `lrs-retry` to be a real directory, checked by `lstat` with no following. Otherwise startup fails naming `lrs-retry`. Revoke deletion uses `unlinkat` within a directory fd opened `O_NOFOLLOW|O_DIRECTORY`.

---

## F-11 — Minor — Unauthenticated connections have no idle or concurrency bound

**Where:** §5.2.1 (auth first; 413 header cap, L282/L312); DD-108.

**Problem.** `--max-request-bytes` caps per-request memory, and the header-block 413 runs before authentication. Nothing bounds how long an unauthenticated client may take to send headers, or how many such connections may be open. On a non-loopback TLS bind, an unauthenticated peer can hold many slow connections and exhaust file descriptors or threads (slowloris). The spec is otherwise careful about untrusted-party exposure.

**Fix.** Add a required header-read timeout and a maximum concurrent-connection count (or reuse an existing flag, such as `--handle-idle-secs` for the header timeout). Over-limit connections are closed without a response. Add an AT on the TLS bind.

---

## F-12 — Minor — Normative rules with no covering AT, plus small wording slips

**Where / problem:**
- `--lrs-url` with a path (including `/xapi`), userinfo, query, or fragment is a startup error (L392, L728). AT-77 only covers http-with-hostname.
- `--consent-state-file` existing at startup must be euid-owned and `0600` (L433). AT-117 / AT-142 list the token, credentials, TLS-key, and pinned-keys files, but not the consent file.
- Config/state files that are a FIFO, symlink, or larger than `--max-open-bytes` are startup errors (L420). Only the inclusion index has an AT (AT-224).
- Several identity records: the header sheet uses the first in tile order (L665). No AT.
- `--fetch-timeout-secs` bounds the whole redirect chain (L463). AT-103 does not cover a slow multi-hop chain.
- AT-222's `test_hooks`-build half (unknown point, malformed value) needs `S3A_KITE_BIN_TEST_HOOKS`, but §7 lists AT-222 only under `S3A_KITE_BIN_RELEASE`.
- DD-121 (L1353) says "AT-226's `lrs_deny_union` / `parse_allowlist_entry` tests". `parse_allowlist_entry` belongs to AT-183.
- The `/v1/download` row (L275) says `download_conflict` carries `included: true` "when step 1 succeeded". The catalogue (L306) and AT-94 say "whenever an inclusion record exists for the key after step 0/1 (written on this request or earlier)", which also covers a skipped step 1.

**Fix.** Add one case each to AT-77 (path, userinfo, query, fragment), AT-117 (consent file `0644` or foreign owner), AT-224 or a new AT (token-file FIFO, symlink, oversize), AT-68 or AT-20 (two identity records), and AT-103 (a redirect chain whose total time exceeds the timeout while each hop is fast). Add `S3A_KITE_BIN_TEST_HOOKS` to AT-222's second half in §7. Fix the DD-121 attribution, and align the L275 wording with L306.

---

## F-13 — Minor — Leftover snapshot-size wording

**Where:** §5.6 (L749): "`/v1/open` of a snapshot larger than `--max-open-bytes` is `file_too_large` (AT-31, AT-80)."

**Problem.** Since DD-103 / AT-227, kite never publishes a snapshot larger than `--max-open-bytes`. The sentence is only reachable for a planted or operator-copied file, but it cites AT-31 and AT-80, which test normal kite-written snapshots. The general `/v1/open` cap (AT-170) already covers it.

**Fix.** Reword to "Any file under `--snapshot-dir` larger than `--max-open-bytes` (for example, one planted there) is `file_too_large` on `/v1/open` (AT-170)", and drop the AT-31 / AT-80 citations.

---

### Checked and not raised

These were examined and either hold, or are accepted design decisions, scope cuts, or residuals:
- DD-141: Unsigned and trailer-stripped residuals are accepted, and Self-signed grants on an empty pin list fall under the same accepted residual.
- `open_verified` trusting the supplied report (DD-80 / AT-137).
- The `lrs_inhibit` latch ordering, `revokes_in_flight` decrement, and fail-closed exit (no stuck state found).
- The in-flight registry and waiter wake-up (no deadlock found).
- Request-root disjointness, which makes root selection unique.
- The `/v1/frame` and `/v1/hit-test` error precedence chain.
- The DD-150 derivation versus AT-165 / AT-201 / AT-220 examples, including break_index 4 for the 3/4/7 fixture.
- The bounds on reads from untrusted peers: fetch headers and body, LRS headers and body, request header block and body.
- The host-spawn binary matrix versus the feature requirements of the fetch and hook ATs.
