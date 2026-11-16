# Round 8 disposition — s3a-kite SPEC v9

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round8.md` (copied to `reviews/goldfish_s3a-kite_round8.md`).
21 findings (0 blockers, 9 majors, 12 minors). Archived v8: `reviews/SPEC-v8.md`.

**Counts:** 21 fixed, 0 rejected, 0 deferred.

Phone scope (DD-128) is unchanged. No new open questions. No invented defaults: LRS credentials follow `s3a-lrs-server` at s3a-storage `b7bdcda`; report fields are grounded in reader-core `VerificationReport` / `SealInfo` / `SignatureCheck.detail` at s3a-reader `792742c`.

## How majors were resolved

**F-01** (hyper-tile empty map cancels restrict-only withdrawals). Fixed (option a). The tour is unrestricted only if the grant map is empty, no withdrawn marks apply, **and** no parseable invalid or unauthenticated tile holds a research withdrawal. An invalid-tile withdrawal withholds **only that `subject_id`** and does not make the grant map non-empty for other subjects. Tightened §5.3 item 4, the empty-map bullet, DD-67, and DD-111. Added AT-184 (mirrors AT-177 for hyper-tile). The same restrict-only rule is stated for Edge type-71 records.

**F-02** (seal-failed hyper-tile still honours grants on CRC-valid floors). Fixed (option a). Every tile of a hyper-tile (or Edge file) whose seal trailer is present and fails `verify_sealed_archive` is unauthenticated for consent: grants are ignored, withdrawals and denials still restrict. Display is unchanged: CRC-valid tiles may remain floors (AT-90). An **unsealed** file is **not** unauthenticated as a whole (AT-136 still holds). Added AT-185. Distinguished authenticated valid floors (grant map) from valid floors (display). Applied uniformly with Edge item 6.

**F-03** (GET consent returns file state, which can disagree with in-memory after `consent_write_failed`). Fixed. GET and POST return the in-memory state. The timestamp is that of the last successful host write or startup read. After `consent_write_failed` (or a failed `revoke-pending` write), state is `off` and the timestamp is the revoke time in `revoke-pending`. Asserted in AT-181. Tightened DD-113.

**F-04** (repeat select contradicts itself on retrying a failed fetch). Fixed (option a: retry allowed). A repeat returns the existing snapshot if one exists; otherwise it retries the fetch. A verb-matching repeat still skips step 1 and step 3. DD-120 already allowed step 2 when `snapshot_sha256` is null; the "or when step 0 skipped it" wording is deleted. Added AT-186.

**F-05** (`drive_path` escape is both `409 path_escape` and "not an on-disk source"). Fixed. `drive_path` is not a request path and is not a `path_escape` case. An escaping, missing, or non-canonicalizable `drive_path` is silently no usable on-disk source (`on_disk: false`). Select and download of that candidate are not `409`. Tightened DD-89 and AT-150. Grounded 3Pass dossiers have Windows `drive_path` values; that is the normal case.

**F-06** (LRS credential parse contradicts s3a-lrs-server). Fixed (option a, per Ashley): parse exactly as `load_credentials_file` at s3a-storage `b7bdcda`. `line.split(':')`; username is `parts[0]`, password is `parts[1]`; further fields (scopes, archive) are ignored by kite. Passwords cannot contain `:`. Fewer than two fields, or an empty username or password, is a startup error naming the file. Corrected AT-182 and DD-132 (v8 had locked in split-at-first-colon). JSON array support on the server is not added; the cited colon-split line format is what kite matches.

**F-07** (retry file can be written after revoke if the pre-queue check is outside the mutex). Fixed. The consent check that allows writing a retry file runs inside the same mutex hold as that write. If the flag is `off`, the host writes nothing and returns `lrs_status: "skipped"`. Added this interleaving as a third case on AT-156. Tightened DD-102 and the step-3 / retry-queue text.

**F-08** (startup cleanup deletes live temps of another same-euid host). Fixed (a)+(b). Temp names include the host pid (`s3a-kite-{pid}-*`, `s3a-kite-tmp-{pid}-*`). Startup deletes only same-euid leftovers whose owning process is gone (or whose flock'd marker can be taken). Scholar start also takes process-lifetime `flock`s on `--snapshot-dir/.lock` and `--download-dir/.lock`. AT-46 now requires distinct snapshot and download dirs. AT-164 covers a collision on those dirs. Added AT-187 and DD-136. Tightened DD-88, DD-114, and DD-122.

**F-09** (`KiteVerificationReport` cannot express break index / torn vs CRC vs chain-break / quarantine reason; pack `invalid_tile_indices` undefined for whole-file quarantine). Fixed. Pack `invalid_tile_indices` is every index not in the valid floor list (all of `0..tile_count-1` when no tile is valid, and when `quarantine_reason` is `signer_not_pinned`). Added `break_index`, `torn_tile_indices`, and `quarantine_reason` (`crc` | `seal_trailer` | `chain_break` | `signature_known_signer` | `signer_not_pinned` | `missing_signature`). Mapping is grounded in `SealInfo`, `SignatureCheck.detail`, and `tiles_crc_failed`. Priority if several reasons apply: `signer_not_pinned`, `missing_signature`, `seal_trailer`, `chain_break`, `signature_known_signer`, `crc`. Tightened DD-99, DD-115, DD-130, and AT-165.

## Minors

**F-10.** Fixed. A failed `revoke-pending` write is `500` `consent_write_failed`, in-memory `off`, no LRS for process life, and no `revoke-pending` file (AT-189). `revoke_delete_failed` leaves `revoke-pending`; a later POST on is `409` `revoke_pending`. A failed POST-on write is `500` `consent_write_failed` and the flag stays `off`. Tightened DD-113.

**F-11.** Fixed. Any 2xx is success; a `409` on a replayed statement `id` also counts as success. 408/429/5xx are queued. Any other 4xx is a permanent failure (log, keep the file, never retry that file). Replay claims each queue file under the mutex before sending. New DD-134 and AT-191.

**F-12.** Fixed. The last-frame cache and `hit_test` lookup are keyed by `(handle, view, floor)` everywhere. `/v1/proceed` and `/v1/close` share the same per-handle lock as frame and hit-test. Tightened DD-125 and the Handles paragraph.

**F-13.** Fixed. Added `KiteContext::new_with_clock(ua_fonts, clock)` for pack tests. Production `new` uses the process clock. Not exported on phone UniFFI. Withholding is evaluated at each render and each candidate recompute. Tightened DD-110 and AT-161.

**F-14.** Fixed. Reworded DD-99: `unverified` is a legal field value but is rejected by `open_verified` (DD-116).

**F-15.** Fixed. An entry without a port matches only port 443 with scheme `https`. `host:PORT` matches either scheme at that effective port. AT-183 now includes `host:80` and `http://host:443`. Tightened DD-133.

**F-16.** Fixed. `--bind` accepts any IP literal (including a specific interface IP and `::`). Non-loopback, non-Unix still requires TLS unless `--behind-proxy`. Passing `--tls-cert` or `--tls-key` on a loopback or `unix:` bind is a startup error naming the flag. New DD-135 and AT-190.

**F-17.** Fixed. Added `--pinned-keys-file` to the viewer-only vs `--view-root` rule. The working-archive parent is disjoint from snapshot, download, and log dirs. Extended AT-179, DD-93, and DD-129.

**F-18.** Fixed. `paper_identity` is `Some(stem)` only when the canonical opened path is a current-candidate dossier under the §5.5 glob (wiki-root wins); otherwise `None`. Deleted "bound paper page." Select remains only when `paper_identity` is `Some`.

**F-19.** Fixed. The candidate walk skips files larger than `--max-open-bytes` (as fail-to-open). A dossier with missing, unparseable, or non-UTF-8 front matter is omitted and no id is logged. Added AT-188.

**F-20.** Fixed. `EXT_CONTENT_SHA256` is present iff the index has a non-null `snapshot_sha256` for `(file_uuid, identity)` at step 3, and equals it. On-disk-only downloads without a snapshot omit it. New DD-137.

**F-21** (editorial). Fixed. §8 header now says DD-01–DD-133 (then DD-137). Preamble says "v9 keeps v3's v1 scope cuts." §5.4 item 4 says Continue is a hit-test target and proceed reaches the tour (DD-97 / AT-152). §6.3 / AT-70: a host-directory artifact satisfies the license inventory with the files alone; the in-app About screen is required on APK/IPA.

## New DDs and questions

- **DD-134** LRS HTTP classification and serialized replay (F-11).
- **DD-135** Bind literals and TLS-on-loopback (F-16).
- **DD-136** Pid-aware temps and snapshot/download dir locks (F-08).
- **DD-137** `EXT_CONTENT_SHA256` presence (F-20).

Tightened in place: DD-67, DD-88, DD-89, DD-93, DD-99, DD-102, DD-110, DD-111, DD-113, DD-114, DD-115, DD-120, DD-122, DD-125, DD-129, DD-130, DD-132, DD-133.

**DD-128 is byte-identical to v8.** DD-63 remains retired; DD-126 remains unused.

**New Qs:** none. Q-01–Q-16 unchanged (Q-03 answered; Q-15 and Q-16 remain future questions).

## Acceptance-test audit

Compared every AT in archived `reviews/SPEC-v8.md` (AT-01–AT-183) to SPEC v9 (AT-01–AT-191). Numbering has no gaps and no renames.

- **172 existing ATs passed unchanged.**
- **11 existing ATs changed in place:** 46, 70, 150, 156, 161, 164, 165, 179, 181, 182, 183.
- **8 ATs added:** AT-184–AT-191.
- **0 ATs removed.**

### Changed in place

| AT | Why |
| --- | --- |
| AT-46 | Distinct `--snapshot-dir` and `--download-dir` as well as data-dir and working-archive (F-08). |
| AT-70 | Host-directory artifact is file-only; About screen is required on APK/IPA (F-21). |
| AT-150 | Escaping / non-canonicalizable `drive_path` is `on_disk: false`, not `409 path_escape`; select/download asserted (F-05). |
| AT-156 | Third case: 503 then revoke before the queue write; check is inside the mutex; `lrs_status` is `"skipped"` (F-07). |
| AT-161 | Clock is `KiteContext::new_with_clock`; withholding re-evaluated on a later render (F-13). |
| AT-164 | Collision on `--snapshot-dir` or `--download-dir` as well as data-dir and working-archive (F-08). |
| AT-165 | Agreement also on `break_index`, `torn_tile_indices`, and `quarantine_reason`; unpinned signer yields all indices (F-09). |
| AT-179 | Working-archive parent vs snapshot/download/log; viewer-only `--pinned-keys-file` vs `--view-root` (F-17). |
| AT-181 | GET timestamp is the revoke time in `revoke-pending` (F-03). |
| AT-182 | Credentials parse as `s3a-lrs-server` `split(':')`; password is field 2 only (F-06). |
| AT-183 | `host:80` and `http://host:443` cases; unported entry is https/443 only (F-15). |

### Added

| AT | Finding |
| --- | --- |
| AT-184 | Hyper-tile invalid-tile withdrawal withholds only that subject (F-01). |
| AT-185 | Seal-failed file: grants ignored, withdrawal still restricts; unsealed still latest-wins (F-02). |
| AT-186 | Repeat select retries a failed fetch once; a third select does not fetch (F-04). |
| AT-187 | Second same-euid host must not delete the first's live temps; shared snapshot-dir is AT-164 (F-08). |
| AT-188 | Unparseable / non-UTF-8 YAML omitted; oversized file skipped (F-19). |
| AT-189 | Failed `revoke-pending` write: 500, in-memory off, no file (F-10). |
| AT-190 | TLS flags on loopback/`unix:` are a startup error; `::` is an IP literal (F-16). |
| AT-191 | LRS `409` replay is success; `400` is permanent; `503` is queued (F-11). |

Passed unchanged: AT-01–AT-45, AT-47–AT-69, AT-71–AT-149, AT-151–AT-155, AT-157–AT-160, AT-162–AT-163, AT-166–AT-178, AT-180.

## Contract range

Acceptance tests: AT-01–AT-191.
Draft decisions: DD-01–DD-137 (DD-63 retired; DD-126 unused).
Open questions: Q-01–Q-16 (no new question).
