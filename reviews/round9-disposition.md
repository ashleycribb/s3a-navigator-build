# Round 9 disposition — s3a-kite SPEC v10

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round9.md` (copied to `reviews/goldfish_s3a-kite_round9.md`).
25 findings (0 blockers, 9 majors, 16 minors). Archived v9: `reviews/SPEC-v9.md`.

**Counts:** 25 fixed, 0 rejected, 0 deferred.

Phone scope (DD-128) is unchanged (byte-identical to v9). No new open questions. No invented defaults.

Ashley decided (2026-11-16), recorded as **DD-138**: any consent-file write failure, `revoke-pending` write failure, or `lrs-retry/` delete failure on revoke stops all LRS sending immediately and shuts the host down (fail closed, non-zero exit, logged). There is no in-process recovery or degraded mode.

## How majors were resolved

**F-01** (`409 revoke_pending` catalogue contradicted AT-189). Fixed by removing the in-process state. Under DD-138 the process exits on the persist failure, so there is no later `POST on` in the same process and no `409` `revoke_pending` row. The catalogue now describes `500` `consent_write_failed` / `revoke_delete_failed` as a last response before exit.

**F-02** (failed `revoke-pending` write loses the revoke across restart). Fixed together with DD-138. Writes use temp + fsync + rename. If the `revoke-pending` write fails, the host still best-effort writes the consent file `off` and deletes `lrs-retry/`, then exits (AT-189). A new process then reads `off`. If the consent-file write fails after `revoke-pending` was written, the next start sees `revoke-pending` (a regular file) and treats consent as `off` (AT-181). If neither durable write succeeded, the next start may read `on`; that case is observable in the prior process's log. There is no "refuse for life" latch and no later POST-on in the same process.

**F-03** (AT-169's chmod-the-file fixture cannot defeat temp+rename). Fixed. AT-169 now makes the **parent directory** of `--consent-state-file` non-writable, and states that this parent is distinct from `--data-dir` so `revoke-pending` can still be published. Making only the file unwritable is explicitly not this test. After the failed consent write, the process exits non-zero (DD-138).

**F-04** (LRS transport failures unclassified). Fixed. No HTTP response (timeout, connection refused, DNS, or I/O/reset mid-body) is treated like 5xx: queued if consent is still `on` in the same mutex hold, else `skipped`. A TLS certificate-validation failure (webpki, name, or chain mismatch) is a permanent failure. Added AT-192. Tightened DD-134.

**F-05** (LRS-host deny vs `egress_test` loopback). Fixed (option b, per Ashley): test-harness separation. When a test uses both fixtures, the LRS bind address is distinct from the fetch fixture (`127.0.0.1` for fetch; `[::1]` or `127.0.0.2` for LRS). The LRS-host deny rule remains by IP and is not overridden by the `egress_test` exception. Added to the §7 fixtures paragraph, AT-54, and DD-140.

**F-06** (`paper_identity` / Select need roots the pack cannot receive). Fixed (option b, per Ashley): `open_path` takes `scholar_roots: Option<ScholarRoots>` `{ wiki_root, corpus_root }`. The pack applies the §5.5 dossier checks (lstat regular file, non-recursive glob, `source_id` equals stem, parseable UTF-8 front matter, size ≤ `max_open_bytes`, wiki-root wins). `None`, or `open` of bytes, yields `paper_identity` `None`. Updated AT-96. New DD-139. Phone UniFFI still does not export `open_path` or `paper_identity` (DD-128).

**F-07** (Edge "seal trailer" / whole-file unauthenticated vs pre-break floors). Fixed (option a): an Edge chain break makes only the breaking tile and later tiles unauthenticated. Verified tiles before the break remain authenticated, and their grants count. Replaced "seal trailer" in the Edge sentence with explicit chain-break wording. Added AT-194. Tightened DD-111.

**F-08** (signature-stripped / declared-but-missing-signature files). Fixed. `missing_signature` (and any Quarantined Edge status with no verified signer) is unauthenticated as a whole for consent, with **no valid floors**, like `signer_not_pinned`. Added to the §5.1 mapping table and AT-165. Added AT-195 (stripped-signature file carrying a grant does not release). Clearing `FLAG_ED25519_SIGNED` and dropping type-70 yields Unsigned, which remains authenticated for consent; that downgrade is recorded as DD-141 (it is indistinguishable from a never-signed file).

**F-09** (step 3 after `snapshot_conflict` / `download_conflict`). Fixed (option a): step 3 still runs after both conflicts and after `snapshot_invalid`; those bodies include `lrs_status`. Updated the catalogue, AT-94, and AT-180. Tightened DD-120.

## Minors

**F-10.** Fixed. Queue names are `lrs-retry/{id}.json` (sendable), `{id}.claimed`, and `{id}.permanent`, all inside `lrs-retry/`. The privacy sentence allows `.permanent` files while consent is `on`. Revoke deletes the whole directory. Combined with DD-138, a failed delete exits the process.

**F-11.** Fixed. `"snapshot_invalid"` is in the `/v1/download` row, with `path: null` and HTTP 200.

**F-12.** Fixed by removing the degraded timestamp case. A running host has no failed-write timestamp; GET/POST use the last successful write or startup read.

**F-13.** Fixed. The write-mutex list includes the working-archive read-copy for `/v1/open`.

**F-14.** Fixed. Only the GET starter appends a new index line, under its own verb. Step 0's any-verb snapshot test governs retries. The AT-186 prose sentence now says "no index line for `(file_uuid, identity)` has a non-null `snapshot_sha256`".

**F-15.** Fixed. `--snapshot-dir` and `--download-dir` are pairwise-disjoint (startup error, AT-179). Sidecar `flock`s use `LOCK_EX|LOCK_NB`.

**F-16.** Fixed. A usable `drive_path` must be a regular file no larger than `--max-open-bytes`; otherwise `on_disk: false`. The on-disk copy hashes bytes as they are written to the temp file and names the file from that hash. Added AT-197.

**F-17.** Fixed. CRC-valid committed tiles of a seal-failed hyper-tile **are** listed as valid floors (display only; unauthenticated for consent). Updated §5.3 item 6, AT-90, and AT-185.

**F-18.** Fixed. Pack `open` uses the same `quarantine_reason` priority as the §5.1 table. Tightened DD-115.

**F-19.** Fixed. First-view aliases normalize to cache key `first`. A supplied `floor` on a non-floor view is `400` `invalid_param`. Added AT-196. Tightened DD-125.

**F-20.** Fixed. AT-76's parenthetical is now "(control write on a separate Scholar host, per §7)".

**F-21.** Fixed. The §8 preamble now says DD-01–DD-137 (then DD-141 after the new rows) and "changes of meaning since v9".

**F-22.** Fixed. `revoke-pending` is written with temp + fsync + rename. "Present" means a regular file. AT-189 uses a test hook that fails that rename, not a planted directory, and covers restart after the best-effort consent `off` write.

**F-23.** Fixed. Candidate item 1 uses `lstat` (skip symlinks, including escapes). Escaping or non-canonicalizable dossiers are silently omitted. `GET /v1/candidates` never returns `path_escape`. Added AT-193.

**F-24.** Fixed. A `409` is success for every emit, first or retry (the `id` is host-minted). Updated AT-191 and DD-134.

**F-25.** Fixed. A Select control is painted only when `select_enabled` is true and `paper_identity` is `Some`. A `kind: "unsupported"` banner paints no Continue; `hit_test` returns `None` there (AT-97).

## New DDs and questions

- **DD-138** Fail-closed host exit on consent persist failure (**Ashley, 2026-11-16**).
- **DD-139** `ScholarRoots` on `open_path` (F-06).
- **DD-140** LRS and `egress_test` fixture addresses (F-05).
- **DD-141** Unsigned downgrade after signature strip is accepted (F-08).

Tightened in place: DD-93, DD-102, DD-111, DD-113, DD-114, DD-115, DD-120, DD-125, DD-130, DD-134, DD-136.

**DD-128 is byte-identical to v9.** DD-63 remains retired; DD-126 remains unused.

**New Qs:** none. Q-01–Q-16 unchanged (Q-03 answered; Q-15 and Q-16 remain future questions).

## Acceptance-test audit

Compared every AT in archived `reviews/SPEC-v9.md` (AT-01–AT-191) to SPEC v10 (AT-01–AT-197). Numbering has no gaps and no renames.

- **175 existing ATs passed unchanged.**
- **16 existing ATs changed in place:** 54, 76, 90, 94, 96, 97, 124, 134, 165, 169, 179, 180, 181, 185, 189, 191.
- **6 ATs added:** AT-192–AT-197.
- **0 ATs removed.**

### Changed in place

| AT | Why |
| --- | --- |
| AT-54 | Harness: LRS fixture binds a distinct address from `egress_test` fetch (F-05). |
| AT-76 | Control write on a separate Scholar host, per §7 (F-20). |
| AT-90 | CRC-valid tiles **are** listed as display floors (F-17). |
| AT-94 | `download_conflict` body includes `lrs_status`; step 3 ran (F-09). |
| AT-96 | `open_path` takes `scholar_roots`; `paper_identity` follows that (F-06). |
| AT-97 | Unsupported banner paints no Continue; `hit_test` returns `None` (F-25). |
| AT-124 | Queue-delete failure exits the process (DD-138). |
| AT-134 | `missing_signature`, no valid floors (F-08). |
| AT-165 | Agreement on `missing_signature` all-indices (F-08). |
| AT-169 | Parent directory non-writable (distinct from `--data-dir`); process exits (F-03, DD-138). |
| AT-179 | `--snapshot-dir` vs `--download-dir` disjoint (F-15). |
| AT-180 | Conflicts and `snapshot_invalid` include `lrs_status` (F-09, F-11). |
| AT-181 | Restart of a **new** process after AT-169, consent off; no in-process 409 (F-01, F-02, DD-138). |
| AT-185 | Floors **are** listed, display only (F-17). |
| AT-189 | Injected rename failure, best-effort consent `off`, exit, restart (F-02, F-22, DD-138). |
| AT-191 | 409 is success on first emit and retry; `{id}.permanent` (F-24, F-10). |

### Added

| AT | Finding |
| --- | --- |
| AT-192 | LRS timeout and connection refused are queued; TLS cert validation is permanent (F-04). |
| AT-193 | Symlink / escaping dossier omitted from candidates; never `path_escape` (F-23). |
| AT-194 | Edge pre-break grant counts; post-break grant is ignored (F-07). |
| AT-195 | Signature-stripped file: no valid floors, grant does not release (F-08). |
| AT-196 | First-view aliases share cache key `first`; `floor` on a non-floor view is `400` `invalid_param` (F-19). |
| AT-197 | FIFO / device / oversized `drive_path` is `on_disk: false` (F-16). |

Passed unchanged: AT-01–AT-53, AT-55–AT-75, AT-77–AT-89, AT-91–AT-93, AT-95, AT-98–AT-123, AT-125–AT-133, AT-135–AT-164, AT-166–AT-168, AT-170–AT-178, AT-182–AT-184, AT-186–AT-188, AT-190.

## Contract range

Acceptance tests: AT-01–AT-197.
Draft decisions: DD-01–DD-141 (DD-63 retired; DD-126 unused).
Open questions: Q-01–Q-16 (no new question).
