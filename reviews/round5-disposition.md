# s3a-kite goldfish round 5 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round5.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round5.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v6 (self-contained copy at `/workspace/goldfish/s3a-kite.md`). v5 is archived as `reviews/SPEC-v5.md`.
*   **Totals:** 33 findings (2 Blocker, 12 Major, 19 Minor). 33 fixed in the spec; 0 rejected; 0 deferred.
*   **Grounding:** unchanged — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`). New v6 grounding: `ConsentRecord` / `actor_id_to_subject_id` / `is_active` in `s3a-core` `consent.rs`; `HumanLrsRecord` has no `subject_id` (`s3a-pack-scholar` `records.rs`); `tour.rs` `build_consent_state` latest-wins is HashMap insert in floor then record order; `EdgeConsentRecord` fields including `consent_version`, `expires_at`, `allowed_purposes`; `VerificationStatus::Unverified` means "verify has not been called yet" (`s3a-reader-core/src/verify.rs`).
*   **v6 counts:** AT-01–AT-163 (1–150 reworded in place where needed, no gaps; AT-151–AT-163 new). DD-01–DD-110 (DD-63 remains retired). Q-01–Q-14 (no new question).
*   After the edits, a full consistency sweep was run over flags, routes, error codes, views, labels, and FFI signatures. Results are in the sweep section below.
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | AT-04 flag list vs §5.2.2 / AT-77; undefined `s3a-kite host` | Fixed. CLI is `s3a-kite host [flags...]` (only v1 subcommand). AT-04, AT-76, and AT-77 are built from the §5.2.2 inventory rather than a copied list, so new required flags cannot drift again. | §5.2, §5.2.2, AT-04, AT-77 | DD-95, DD-106; AT-04, AT-77 |
| F-02 | Blocker | Non-S3A `open`: kind `other` vs AT-95 `Err` / §5.3 magic error | Fixed. Kind detection runs first. No-magic, no-hint bytes return `Ok` with kind `other`. Magic present but an unopenable header is `Err` / `400` `invalid_magic`. A `.s3a`-named file without magic is `other` with the label `no text conversion in archive`, not `This is not an S3A file`. | §5.2.5, §5.3 item 1, AT-95, AT-130 | DD-87; AT-95 |
| F-03 | Major | Consent-file bootstrap impossible ("host is only writer") | Fixed. When `--lrs-url` is set and the file is missing, the host creates it as `off` plus timestamp, mode 0600. That is not an invented `on`. Malformed existing files remain a startup error. | §5.2.3 | DD-96; AT-151 |
| F-04 | Major | Omitted `floor` is both 400 `missing_param` and 409 `invalid_floor` | Fixed. Omitted (where required) is `400` `missing_param`; present but not a valid tile index is `409` `invalid_floor`. AT-138 covers both. | §5.2.1, §5.3 item 4, DD-82, AT-138 | DD-82; AT-138 |
| F-05 | Major | Does a Continue `hit_test` change handle state? | Fixed. `hit_test` is pure. The client must call `proceed_to_warehouse` / `POST /v1/proceed`. AT-152 asserts warehouse is `view_mismatch` until proceed. | §5.1, §5.3 item 3, DD-36, AT-09 | DD-97; AT-09, AT-152 |
| F-06 | Major | Pinned keys not passable to `open` / `open_path` | Fixed. Both take `pinned_keys: Vec<[u8; 32]>`. Empty means the omitted list. The host passes keys from `--pinned-keys-file` (or empty). The pack does not read the host flag. | §5.1, §5.4, §5.7 | DD-98; AT-153 |
| F-07 | Major | Verification labels incomplete (hyper-tile seal, Unverified, unsealed, no-sig + pin list) | Fixed. `KiteVerificationReport` has `status` and `seal_status`. Table in §5.4. Sealed-OK hyper-tile is Unsigned + sealed, not Verified. Reader-core `Unverified` is not a legal `status` (`open_verified` returns `KiteError`). No signatures and `FLAG_ED25519_SIGNED` clear, even with a non-empty pin list, is Unsigned. | §5.4, §5.3 item 6, §5.1 | DD-99; AT-72, AT-147, AT-154 |
| F-08 | Major | Mid-chain bad sig vs whole-file Quarantine; AT-17 vs AT-35 torn tiles | Fixed. Torn/uncommitted tiles are ignored and do not quarantine (AT-17). Committed CRC failure quarantines the file (AT-35). Mid-chain sig failure with a trusted signer keeps earlier floors and does not change Verified/Self-signed. An unpinned signer on a non-empty list quarantines the whole file with no valid floors. | §5.4, AT-17, AT-35 | DD-100; AT-155 |
| F-09 | Major | AT-42 cannot remove `include_bytes!` fonts from the host binary | Fixed. Font registration is injectable. Shipped binaries pass embedded bytes. Tests pass an empty set and expect `KiteError`. | §5.7, AT-42 | DD-101; AT-42 |
| F-10 | Major | Mutex held across fetch+LRS; no LRS timeout; revoke cannot stop an in-flight send | Fixed. Mutex covers disk writes only, not fetch or LRS. Skip does not take it. Membership is computed first. Required `--lrs-timeout-secs` when `--lrs-url` is set. Fetch timeout covers the whole redirect chain. Consent-off sets an atomic flag before waiting for the mutex, re-checks immediately before send, and aborts an in-flight LRS POST. | §5.2.1, §5.5, DD-84 | DD-102; AT-156 |
| F-11 | Major | Identical snapshot bytes across papers overwrite provenance | Fixed. If `{content sha}.s3a` exists, do not overwrite; reuse it and keep the first provenance. Both identities store the same `snapshot_sha256`. | §5.6 | DD-103; AT-157 |
| F-12 | Major | Index rewrite not crash-safe; malformed index and step-1 mid-crash unspecified | Fixed. Append-only, last matching line wins. Each mutation is copy + append + fsync + atomic rename. Malformed index at start is a startup error (no invented recovery). Step 1 is tile-10 then index; index failure is `500` `write_failed` with the record kept. | §5.5 | DD-104; AT-158 |
| F-13 | Major | Create-time `ConsentRecord` fields unspecified; rationale vs no `subject_id`; AT-79 weak | Fixed. Every field is listed in §5.6, using `ConsentRecord::new`, `hash_iri`, and `actor_id_to_subject_id`. `HumanLrsRecord` has no `subject_id`, so inclusion records are not withheld by `ConsentRecord`. AT-79 requires `withheld_count == 0`. | §5.6, AT-79 | DD-105; AT-79, AT-159 |
| F-14 | Major | §4.2 withholding wording disagrees with §5.3 item 4 | Fixed. §4.2 now states that §5.3 item 4 / §5.4 item 6 are the only normative kite withholding rules. The tour.rs sentence is a source fact, not a second algorithm. | §4.2, §5.3 item 4 | DD-53, DD-67 |
| F-15 | Minor | Unreachable "2b skipped unless an earlier snapshot exists" | Fixed. Clause deleted. A fetch is not started when a snapshot already exists. | §5.5 | — |
| F-16 | Minor | DD-09 says snapshots are working-archive writes | Fixed. DD-09 now says local writes (working archive and `--snapshot-dir`). | DD-09 | DD-09 |
| F-17 | Minor | DD-53 cites retired DD-63 | Fixed. "No snapshot exemption (DD-63 retired; DD-67 applies)." | DD-53 | DD-53 |
| F-18 | Minor | Opacity curve and `t` range unspecified | Fixed. Opacity is `0.2 + 0.8 * progress` with the same cubic-bezier as position. `t` is clamped to `[0, 0.45]`. The end frame shows floor-detail content. | §5.8, AT-62 | — |
| F-19 | Minor | AT-143 cites `rgba` on the HTTP response | Fixed. FFI asserts `rgba.len()==80000`; HTTP asserts the decoded PNG is 200 by 100. | AT-143 | — |
| F-20 | Minor | Weak negative ATs (AT-132, AT-51, AT-52, AT-53) | Fixed. AT-132 is parameterized over every Scholar-only inventory flag. AT-51 includes `--headless` and asserts the error names TLS. AT-52 and AT-53 assert `409` `path_escape` and name the JSON `path` field. | AT-132, AT-51, AT-52, AT-53 | DD-106, DD-83 |
| F-21 | Minor | No complete flag inventory | Fixed. §5.2.2 table lists every flag with type/range, mode, and dependents. | §5.2.2 | DD-106 |
| F-22 | Minor | `/v1/candidates` `title` from a record with no string fields | Fixed. Title is dossier YAML `title`, or empty for `AcademicPaperRecord` candidates. | §5.2.1, §5.5 | — |
| F-23 | Minor | `Link { href }` resolution base unspecified | Fixed. Relative hrefs resolve against the opened page's parent directory; `#fragment` is stripped. AT-160 covers a sibling link. | §5.1 | AT-160 |
| F-24 | Minor | No view validity matrix | Fixed. Kind x state x view table in §5.2.6. `manifest`, `header`, and `banner` are aliases of `render_manifest` on archive kinds. | §5.2.6 | DD-107 |
| F-25 | Minor | Candidate walk: symlinks, cost, mutex | Fixed. Walk does not follow symlinks. Cost is acknowledged; an mtime/size cache is allowed if observably equivalent. Membership is computed before the write mutex is taken. No invented depth cap. | §5.5, §5.2.1 | DD-102 |
| F-26 | Minor | Transport hardening gaps | Fixed. Unix socket 0600, euid; stale path is a startup error (no unlink). TLS 1.2 or newer. Required `--max-request-bytes`; over is `413` `request_too_large`. | §5.2.2, §5.2.1 | DD-108; AT-162, AT-163 |
| F-27 | Minor | Phone `open_path` has no authorization root | Fixed. Phone UniFFI does not export `open_path`. SAF/share-sheet bytes go to `open` / `open_verified`. | §5.1, §5.7, AT-99 | DD-109; AT-99 |
| F-28 | Minor | Consent ordering key and test clock unspecified | Fixed. Latest-wins is increasing tile index, then record index (grounded in `build_consent_state`). Type-71 uses the same order; last CRC-valid record wins; `expires_at` uses the same clock. Pack tests inject `now`. | §5.3 item 4, §5.4 item 6 | DD-110; AT-161 |
| F-29 | Minor | Preamble "no fallbacks" vs temp dir and auto-created index | Fixed. Preamble lists explicit exceptions: DD-69, DD-88, DD-61 (empty index), and DD-96 (consent `off` bootstrap). | Preamble, DD-88, DD-61, DD-96 | — |
| F-30 | Minor | GET/POST consent timestamp without `--lrs-url` | Fixed. `--consent-state-file` without `--lrs-url` is a startup error. GET without LRS remains `{off, null}`. When a consent file is configured, responses use the file's timestamp. | §5.2.3, AT-114 | DD-96; AT-114 |
| F-31 | Minor | AT-125 dead TLS clause; "native-window build" vs runtime flag | Fixed. TLS clause dropped (window only on loopback/Unix). Window is cargo feature `native-window` plus runtime `--headless`. A build without the feature must pass `--headless`. | §5.2, AT-125 | DD-60; AT-125 |
| F-32 | Minor | AT-10 CLI oracle on unsafe fixtures | Fixed. AT-10 is restricted to a sealed, type-82-free hyper-tile. | AT-10 | — |
| F-33 | Minor | AT-96 omits `open_verified`; AT-06 misses Unix-socket health | Fixed. AT-96 includes `open_verified` and `pinned_keys`. AT-06 uses the glossary term "tokenless health case". | AT-96, AT-06 | — |

## Blockers, in short

*   **F-01.** v5's AT-04 copied a Scholar flag list that omitted `--max-redirects`, `--max-frame-pixels`, and `--handle-idle-secs`, so AT-04 and AT-77 could not both pass. v6 defines the CLI as `s3a-kite host [flags...]` and adds a complete flag inventory. AT-04 (and AT-76 / AT-77 / AT-132) must be built from that table, not from a copied list.
*   **F-02.** v5 required the same no-magic bytes to be both `Ok(kind=other)` and `Err`. v6 picks the kind-detection rule: no magic means `other` (including a `.s3a` name hint). Magic present but an unopenable header is the error path (AT-69 / AT-95).

## F-10, in short (mutex, LRS timeout, revoke)

The write mutex no longer covers the source fetch or the LRS POST, so a stalled network call cannot block consent. `--lrs-timeout-secs` is required whenever `--lrs-url` is set. `--fetch-timeout-secs` bounds the whole redirect chain. `POST /v1/consent {off}` sets an atomic in-memory flag before waiting for the mutex, step 3 re-checks immediately before any LRS POST, and an in-flight LRS POST is aborted (AT-156). Skip does not take the mutex.

## Consistency sweep (required after these blockers)

A scripted sweep was run on the assembled v6 draft (`/tmp/s3a-kite-spec-v6/sweep.py`) over §5, the AT list, and the DD table. Findings and fixes:

*   **Flags.** 32 flags in the §5.2.2 inventory. Every kite-host `--flag` in the contract is in that table. Apparent extras (`--json`, `--floor`, `--slice`, `--cross`, `--history`, `--html`, `--target`) are s3a-cli or `cargo build` flags in grounded facts / ATs, not host flags. AT-04, AT-76, AT-77, and AT-132 now cite the inventory instead of a copied list. `--lrs-timeout-secs` and `--max-request-bytes` are in the table and in AT-77 / AT-76 / AT-156 / AT-162.
*   **Routes.** All 12 Host HTTP API routes (`GET /health`, `POST /v1/open`, `POST /v1/close`, `POST /v1/proceed`, `GET /v1/frame`, `POST /v1/hit-test`, `GET /v1/candidates`, `POST /v1/select`, `POST /v1/download`, `POST /v1/skip`, `GET /v1/consent`, `POST /v1/consent`) appear in the §5.2.1 table and are referenced from ATs.
*   **Error codes.** 25 catalogue entries. Sweep confirmed omitted `floor` is no longer also specified as `409` `invalid_floor`; unknown `view` is not "400 or 404". Sweep fix: moved `413` `request_too_large` to sit after the 400-group (it had been inserted in the middle of the 400 rows).
*   **Views.** `manifest`, `header`, `page`, `banner`, `warehouse`, `floor-transition`, and `floor-detail` are all in the `/v1/frame` enum and in the §5.2.6 matrix. `hit_test` outcomes for `banner` (Continue, as a first-view alias) and for `floor-detail` / `floor-transition` (`None`, or `409` `no_frame`) are stated in §5.1.
*   **Labels.** UI status labels Verified / Self-signed / Unsigned / Quarantined and `seal_status` sealed / unsealed / seal_failed appear in §5.4, the report type in §5.1, and ATs 72 / 147 / 154 / 155.
*   **FFI signatures.** `open(bytes, name_hint, max_frame_pixels, pinned_keys)`, `open_verified(..., report)`, `open_path(..., pinned_keys)` match §5.1, AT-96, and AT-153. Phone UniFFI omits `open_path` in §5.1, §5.7, and AT-99. `open_verified` is in AT-96.
*   **Leftover-contradiction scan.** No remaining v5 sentences for: mutex held across LRS send; in-place index line rewrite; "keep 2b if an earlier snapshot exists"; "If it is not an S3A file, fail with engine…"; active "Snapshot exemption is DD-63"; AT-95 "non-S3A returns Err" as the only rule; old `open(...)` signature without `pinned_keys`.
*   **AT / DD numbering.** AT-01–AT-163, no gaps. DD-01–DD-110, no gaps. DD-63 still marked retired.

## Meaning changes to existing draft decisions

*   **DD-09:** snapshots are local writes to `--snapshot-dir`, not the working archive.
*   **DD-36:** proceed, not a Continue hit, changes state.
*   **DD-50 / DD-53:** bootstrap is DD-96; no snapshot exemption; §5.3 item 4 is the only hyper-tile withholding rule.
*   **DD-60:** cargo feature `native-window` plus `--headless`.
*   **DD-82:** omitted floor is `400` `missing_param`.
*   **DD-84:** superseded in part by DD-102 (mutex is disk-only).
*   **DD-94:** fonts are injectable for tests (DD-101).

## New draft decisions and questions

*   **DD-95–DD-110:** CLI shape; consent bootstrap; pure `hit_test`; `pinned_keys` on `open`/`open_path`; verification label table; torn vs CRC vs mid-chain signatures; injectable fonts; mutex / LRS timeout / revoke; snapshot reuse; index crash safety; create-time `ConsentRecord` fields; flag inventory; view matrix; transport hardening; phone omits `open_path`; consent ordering and test clock.
*   No new open question. Q-01–Q-14 remain.

