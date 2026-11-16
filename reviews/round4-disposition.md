# s3a-kite goldfish round 4 — disposition

*   **Report:** `reviews/goldfish_s3a-kite_round4.md` (copied unchanged from `/workspace/goldfish/goldfish_s3a-kite_round4.md`).
*   **Spec:** `/workspace/s3a-kite/SPEC.md` v5 (self-contained copy at `/workspace/goldfish/s3a-kite.md`). v4 is archived as `reviews/SPEC-v4.md`.
*   **Totals:** 40 findings (1 Blocker, 17 Major, 22 Minor). 40 fixed in the spec; 0 rejected; 0 deferred.
*   **Grounding:** unchanged — Kitesurf / s3a-Kite `235111c6fe10f2d63f3476bf5e3d19b4c7089415`; s3a-storage `b7bdcda0df63524ffe205f2ad92cd6a5b69a40f6`; s3a-reader `792742ca9c1659cb5fd4766237d13c2f5ae24404` (engine pin `ff89564`). F-13 is grounded in `s3a-reader-core/src/verify.rs` (`verify(pinned_keys)`, `VerificationStatus`, `SignerTrust`), s3a-vault contact pinning, and S3A-EDGE-PROFILE.md §543; the pack does not treat a key embedded in the file as a trust anchor.
*   **v5 counts:** AT-01–AT-150 (1–125 reworded in place where needed, no gaps; AT-126–AT-150 new). DD-01–DD-94 (DD-01–DD-78 keep their topics except **DD-63, retired**; meaning changes noted below). Q-01–Q-14 (Q-14 is the timed fuzz CI duration).
*   The elephant did not self-review; the next independent goldfish round checks this disposition.

| ID | Severity | Finding (shortened) | Disposition | Spec section(s) | DD / Q / AT |
| --- | --- | --- | --- | --- | --- |
| F-01 | Blocker | AT-61 forbids paper ids in `--data-dir`, but the inclusion index lives there | Fixed. Option (a): AT-61 and the §5.5 privacy bullet now allow identities only in `inclusion-index.jsonl` (and in `lrs-retry/` only when consent is on). The index stays in `--data-dir` as local research data. | §5.5, AT-61 | DD-61; AT-61 |
| F-02 | Major | No identity→snapshot mapping; repeat select and download-bytes source undefined | Fixed. Each index line stores nullable `snapshot_sha256`. An idempotent second select returns the existing snapshot path. A later download reassembles BLOB tiles and does not fetch again. | §5.5 Inclusion index | DD-61; AT-83 |
| F-03 | Major | Index idempotency names the working archive but never records it | Fixed. Each line stores the canonical `working_archive` path and `file_uuid`. Lookup is current archive `file_uuid` + identity + verb. Non-matching uuid lines do not suppress a new inclusion. | §5.5 | DD-61 |
| F-04 | Major | No rule for concurrent writes or snapshot file races | Fixed. Process-wide write mutex serializes select, download, skip, and consent end-to-end. Snapshots use temp + atomic rename. The archive exclusive lock is held only for step 1. | §5.2.1, §5.5, §5.6 | DD-84; AT-126 |
| F-05 | Major | Download write order contradicts the side-effect order | Fixed. Numbered steps: (1) inclusion+index, (2) fetch+snapshot, (2b) download delivery, (3) LRS. Step-1 failure skips 2/2b/3 and leaves `--download-dir` unchanged. The unreachable DD-56 "keep a download" sentence is deleted. | §5.5, §5.6 | DD-56, DD-84; AT-89, AT-127 |
| F-06 | Major | No-source candidates; `on_disk` / `drive_path` underspecified | Fixed. Decision table: url only / disk only / both (prefer disk) / neither (`download_error: "no_source"`, `path: null`, inclusion still written). Relative `drive_path` is resolved against the dossier parent. The dossier `.md` is not the on-disk source. | §5.5 | DD-59; AT-128, AT-129, AT-150 |
| F-07 | Major | "Current candidate list" is undefined | Fixed. There is no persisted list. Membership is recomputed at request time. Select with no prior GET is allowed if the id is in the fresh set; otherwise 404. | §5.5, AT-105 | DD-61; AT-105 |
| F-08 | Major | No kind / `first_view` detection; `kind: "other"` undefined | Fixed. Ordered table in §5.2.5. Optional `/v1/open` `name` hint (else path basename). Bad S3A version remains an error (AT-69), not `other`. | §5.2.5, §5.1 | DD-87; AT-130, AT-148 |
| F-09 | Major | `first_view: "banner"` cannot be rendered | Fixed. `view=banner` is an alias of `render_manifest`. AT-66 and AT-97 fetch the frame and assert banner text. | §5.1, §5.2.1 | DD-81; AT-66, AT-97 |
| F-10 | Major | No HTTP error-status catalogue | Fixed. Normative table in §5.2.1 maps each `error` code to a status. ATs assert the listed codes. | §5.2.1 | DD-83 |
| F-11 | Major | AT-76 cannot pass as written (missing required flags) | Fixed. AT-76 now includes `--max-open-handles`, `--max-frame-pixels`, `--handle-idle-secs`, and `--bind 127.0.0.1` (or `--headless`). Companion AT-131 covers a viewer without `--max-open-handles`. | AT-76, §5.2.2 | AT-76, AT-131 |
| F-12 | Major | Hyper-tile consent filter incomplete | Fixed. §5.3 item 4 writes the `tour.rs` / `consent.rs` algorithm: latest-wins per `subject_id` for Research only; `is_active` as in `consent.rs`; empty map (including only-non-research records) is unrestricted; records without `subject_id` are withheld only by marks; type 82 is never consent. | §5.3 item 4 | DD-67; AT-11, AT-135, AT-136 |
| F-13 | Major (security) | Ed25519 trust anchor unspecified; must not trust a key embedded in the file | Fixed. Trust is an optional pinned-key list (`--pinned-keys-file`), matching `s3a-reader-core` `verify(pinned_keys)` and vault contact pinning. Empty/omitted list: at best Self-signed, never Verified. Non-empty list and signer not in it: Quarantined. UI labels: Verified / Self-signed / Unsigned / Quarantined. `FLAG_ED25519_SIGNED` with no type-70/key is Quarantined. See the F-13 section below. | §5.4 item 2, §4.7 | DD-79; AT-133, AT-134 |
| F-14 | Major | Mobile `VerificationReport` hand-off missing from FFI | Fixed. `open_verified(bytes, name_hint, max_frame_pixels, report)`. Pack type `KiteVerificationReport`; the app maps a reader-core report. If a report is supplied, kite uses it even when a fresh verify would disagree. The pack does not depend on `s3a-reader-core`. | §5.1, §5.4, §5.7 | DD-80; AT-137 |
| F-15 | Major | `floor_index` / `Floor(n)` ambiguous after hiding invalid tiles | Fixed. They are the tile `index`. Invalid or omitted (where required) is `409` `invalid_floor`. The invalid-list is banner text only and is not hit-testable. | §5.1, §5.3 item 4 | DD-82; AT-10, AT-138 |
| F-16 | Major | Fetch response handling unspecified (non-2xx, redirects, compression) | Fixed. Required `--max-redirects` ≥ 0. Only a final 2xx is stored. Body is decoded `Content-Encoding` (`identity`/`gzip`/`deflate`); unknown encoding or non-2xx is `fetch_error` and no snapshot. `--max-fetch-bytes` applies to decoded bytes. | §5.2.4 | DD-86; AT-139, AT-140 |
| F-17 | Major | No bounds on frame size | Fixed. Required `--max-frame-pixels` and UniFFI `max_frame_pixels`. Pixmap product `ceil(w*dpr)×ceil(h*dpr)` must be ≤ the cap; `dpr` must be finite and > 0. Else `400` `invalid_frame_size`. | §5.2.1, §5.8 | DD-90; AT-141, AT-143 |
| F-18 | Major (security/privacy) | LRS credentials / TLS key lack 0600 and no-logging | Fixed. `--lrs-credentials-file`, `--tls-key`, `--pinned-keys-file`, and an existing consent file must be euid+0600. Host-created index, retry-queue, and consent writes are 0600. No logging of Authorization, LRS passwords, paper ids, source URLs, or `--researcher-id`. LRS HTTPS uses the system / webpki root store; v1 does not pin LRS certificates. | §5.2.2, §5.5 | DD-85; AT-117, AT-142 |
| F-19 | Minor | AT-27 puts the snapshot in the working archive | Fixed. AT-27 now says the snapshot is `--snapshot-dir/{sha256}.s3a`, not a tile in the working archive. | AT-27 | — |
| F-20 | Minor | AT-07 / AT-77 require `--port` in every mode | Fixed. `--port` is required unless `--bind` starts with `unix:`. AT-113 remains the Unix-socket success case. | AT-07, AT-77 | DD-72 |
| F-21 | Minor | Tokenless-health wording inconsistent | Fixed. The glossary term "tokenless health case" is used in §1, §5.2.1, and AT-06. | §1, §5.2.1, glossary | DD-71 |
| F-22 | Minor | DD-63 snapshot exemption is vacuous; AT-107 cannot fail on its terms | Fixed. DD-63 is **retired**. BLOB+PROVENANCE snapshots have no `ConsentRecord`, so DD-67 already leaves them unrestricted. AT-107 / AT-108 are reworded on that rule; sitting in `--snapshot-dir` grants no exemption. | §5.6, AT-107, AT-108 | DD-63 retired; DD-67 |
| F-23 | Minor | AT-125 can pass for the wrong reason; needs a display | Fixed. The window half passes required TLS flags, asserts the error names `--headless`, and is skipped (and reported) when there is no `DISPLAY`. | AT-125 | — |
| F-24 | Minor | Weak egress ATs | Fixed. AT-122 asserts an allowed `egress_test` fixture is reached with zero proxy hits. AT-54 denies a fetch to an otherwise-allowed address that is the LRS host. | AT-122, AT-54 | DD-76 |
| F-25 | Minor | AT-71 is not a fuzz criterion | Fixed. Seed corpus and a corpus-replay CI job are required. Timed fuzz duration is Q-14 (not invented). | §5.9, AT-71 | Q-14 |
| F-26 | Minor | AT-100 accepts "400 or 404" | Fixed. Unknown `view` is `400` `invalid_view`. | AT-100, §5.2.1 | DD-83 |
| F-27 | Minor | AT-29 calls `/v1/open` a GET | Fixed. AT-29 lists only `GET /v1/candidates`, `POST /v1/open`, and `GET /v1/frame`. | AT-29 | — |
| F-28 | Minor | Path rule is broader than intended | Fixed. The inside-root rule applies to request paths, Link follow-ups, candidates, and resolved `drive_path`. Host config/state files are listed separately. | §5.2.2 | DD-89 |
| F-29 | Minor | Coordinate spaces and DPR undefined | Fixed. `w`/`h`/`x`/`y` are CSS px. Pixmap is `ceil(w*dpr)` by `ceil(h*dpr)`. `TextRun`/`SemanticNode` `x`/`y` are `i32` and may lie off-pixmap. AT-143 covers DPR 2. | §5.8 | DD-90; AT-143 |
| F-30 | Minor | Select hits where select cannot be called | Fixed. `render_page(..., select_enabled)`. False in viewer-only mode and on the phone; no Select control is painted. | §5.1, §5.8 | DD-91; AT-144 |
| F-31 | Minor | `--log-dir` contents and ephemeral temp undefined | Fixed. Logs are startup (flag names only), shutdown, and identifier-free errors. Temp is `mkdtemp` 0700 under `$XDG_RUNTIME_DIR` or `/tmp` with prefix `s3a-kite-`; stale same-euid dirs are cleaned at startup and on exit. | §5.2.2 | DD-88 |
| F-32 | Minor | Consent and flag edge cases | Fixed. Consent `off` without a consent file is `409` `lrs_not_configured`. GET consent without `--lrs-url` is `200` `{off, null}`. Viewer rejects all Scholar-only flags (AT-132). | §5.2.2, §5.2.3 | AT-132 |
| F-33 | Minor | Provenance document is not byte-exact | Fixed. Exact bytes `sha256=<hex>\nurl=<source_url>\ncaptured=<RFC3339>\n`. PROVENANCE `blob_key` is SHA-256 of that document. The "or empty" alternative is dropped. | §5.6, AT-32 | DD-47 |
| F-34 | Minor | Handles never expire | Fixed. Required `--handle-idle-secs` ≥ 1. Idle handles return `404` `unknown_handle`. | §5.2.1 | DD-92; AT-149 |
| F-35 | Minor | Candidate-list edge cases (join, dupes, glob) | Fixed. The dead SRC/`paper_id` join is deleted. Duplicate `paper_id`s: first sorted `.s3a` path, then first in tile order. The SRC glob is exactly `{root}/papers/SRC-*.md` (not recursive). | §5.5 | DD-61 |
| F-36 | Minor | Overlapping roots not prohibited | Fixed. Write locations (snapshot, download, data, log, working-archive parent) are pairwise-disjoint from read roots. A violation is a startup error. | §5.2.2 | DD-93; AT-145 |
| F-37 | Minor | Host font sourcing unspecified | Fixed. The host embeds the same required UA fonts with `include_bytes!`. AT-42 applies to the host. | §5.7, AT-42 | DD-94 |
| F-38 | Minor | Implementation order omits Edge, fuzz, license packaging | Fixed. §11 step 3 covers Edge and pinned-key verification. New step 7 covers the fuzz corpus and LICENSE/NOTICE / OFL packaging. | §11 | — |
| F-39 | Minor | DD-14 promises PDF-skim behavior no AT defines | Fixed. DD-14 is the `other` + exact label `no text conversion in archive` rule. No invented title or abstract. | §5.2.5, DD-14 | AT-148 |
| F-40 | Minor | Edge flag edge cases (size flags on 128 KiB; seal-chain flag) | Fixed. A 4 KiB/8 KiB size flag on a 128 KiB non-Edge stride is `unsupported`. `FLAG_SEAL_CHAIN_ACTIVE` clear is unsealed Edge: show status, do not auto-quarantine. | §5.3 item 1, §5.4 item 1 | AT-146, AT-147 |

## Blocker F-01, in short

AT-61 in v4 required that a consent-off select write no paper ids or URLs into `--log-dir` or `--data-dir`. That contradicted the inclusion index, which must record identities and lives in `--data-dir`. v5 takes option (a): the index stays in `--data-dir` as local research data (like the working archive). AT-61 and the privacy bullet now say: no paper ids or URLs in `--log-dir`, and none in `--data-dir` other than `inclusion-index.jsonl`. The retry queue may hold identities only when consent is on. The index was not moved out of `--data-dir`.

## F-13, in short (Edge device public-key provenance)

The pack must not treat a public key embedded in the file (`DeviceIdentityRecord.public_key`, or any key carried in the same bytes) as a trust anchor. That matches `s3a-reader-core/src/verify.rs`: `verify(pinned_keys)` fails with "signer key not in pinned list" when the pinned list is non-empty and the signer is absent; an empty pin list yields at best `SelfSigned`. It also matches s3a-vault contact pinning (`vault contact add` of a full 32-byte key; unpinned issuers are `UNKNOWN ISSUER`) and S3A-EDGE-PROFILE.md §543 ("cached or provisioned public_key").

v5 therefore:

*   Adds optional `--pinned-keys-file` (hex 32-byte keys, one per line, euid+0600).
*   Empty or omitted list: a signature that verifies against a key found in the file is at best **Self-signed**, never **Verified**.
*   Non-empty list and signer not in it: **Quarantined**.
*   UI labels are exactly Verified / Self-signed / Unsigned / Quarantined.
*   `FLAG_ED25519_SIGNED` set with no type-70 tile and no usable key is **Quarantined**.
*   AT-133: a file re-signed with a different unpinned key is Quarantined even if the new key is embedded.
*   AT-134: flag set, no signatures, is Quarantined.

## Meaning changes to existing draft decisions

*   **DD-14:** no invented PDF title/abstract; `kind: "other"` shows the exact label `no text conversion in archive`.
*   **DD-56:** step 2b runs only after a successful step 1; the "keep a delivered download if inclusion fails" sentence is deleted.
*   **DD-59:** full source decision table, including neither and both; relative `drive_path` resolution.
*   **DD-61:** index stores `file_uuid`, canonical archive path, and nullable `snapshot_sha256`; no SRC/`paper_id` join; request-time candidate set; `{root}/papers/SRC-*.md` glob.
*   **DD-63: retired.** Use DD-67. Sitting in `--snapshot-dir` grants no exemption.
*   **DD-67:** the explicit ConsentRecord algorithm (and snapshots).
*   **DD-73 / DD-74 / DD-76:** extended by DD-85 (secrets), DD-92 (idle), and DD-86 (2xx / encoding / redirects).

## New draft decisions and questions

*   **DD-79–DD-94:** pinned keys (F-13); `open_verified` / `KiteVerificationReport`; `view=banner`; `floor_index` is the tile index; error catalogue; write mutex and numbered steps; secrets / TLS store / no-logging; fetch 2xx and content encoding; kind detection; logs and ephemeral temp; path-rule scope; frame pixel cap and CSS vs pixmap; `select_enabled`; handle idle; disjoint write/read locations; host fonts.
*   **Q-14.** Timed `cargo-fuzz` duration for CI. The seed corpus and corpus-replay job are required; this spec does not invent a duration.

