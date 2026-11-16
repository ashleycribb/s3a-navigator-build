# Goldfish review — s3a-kite v13 (round 13)

Reviewer: independent, fresh read. Only `/workspace/goldfish/s3a-kite.md` was read; no prior reviews, dispositions, repos, or wiki were consulted.

**Summary: 16 findings: 0 blocker / 4 major / 12 minor.** (Not a clean round: 4 majors.)

Severity key: **blocker** = cannot be implemented or tested as written; **major** = ATs contradict normative text or each other, or there is a security/consent gap an implementer would have to guess at; **minor** = wording, local inconsistency, or a small undefined case.

---

## Majors

### F-01 (major): Edge signer attribution leaves "empty pin list, first type-70 record fails every embedded key" undefined, and the consent rule for "unauthenticated as a whole" disagrees with the `open_verified` row

**Where:** §5.4 item 2 "Signer attribution (DD-145, AT-201)" steps 3–4; §5.4 per-tile bullet "signature failure from a known signer"; §5.3 item 4 "Trusted vs untrusted"; §5.1 `open_verified` row; label table row "Signature present from a known signer … that does not verify"; AT-35, AT-155, AT-201, AT-165; DD-145, DD-111.

**Quotes:**
- Step 4 (empty pin list): "A record that fails every embedded key is `signature_known_signer` at its `tile_index` **if an earlier record already verified under an embedded key** (earlier floors may remain). Type-70 tiles present with **no** candidate key at all … is `missing_signature`." Nothing covers the case where embedded keys exist and the **first** record (or every record) fails them all.
- Attribution preamble: "If attribution is impossible, the file is Quarantined (`signer_not_pinned` or `missing_signature` as below) and no tile is a valid floor." With an empty pin list, `signer_not_pinned` makes no sense.
- Per-tile bullet and AT-35: a "known signer" is "the embedded device key when the pin list is empty". Read that way, a first-record failure is `signature_known_signer` with earlier floors kept, which contradicts step 4's "only if an earlier record already verified".
- §5.3 item 4: "An Edge file is unauthenticated as a whole when `quarantine_reason` is `signer_not_pinned` or `missing_signature` … **or when it is otherwise Quarantined with no verified signer**."
- §5.1 `open_verified`: "The file is unauthenticated as a whole for consent **iff** (it is a hyper-tile and `seal_status == seal_failed`) or `quarantine_reason` is `signer_not_pinned` or `missing_signature`."

**Why it matters:** This changes `quarantine_reason`, `invalid_tile_indices`, and `break_index`, which are the fields AT-165 compares between pack and reader. It also decides whether research grants are trusted. "Otherwise Quarantined with no verified signer" also covers an **Unsigned** Edge file with a committed CRC failure (Quarantined `crc`, no signer at all). Under §5.3 that file would be unauthenticated as a whole and every event-log payload would be withheld. Under the `open_verified` "iff" it is not. So the same bytes withhold differently through `open` and `open_verified`, and AT-177 / AT-194-style fixtures have no single oracle.

**Fix:**
- (a) Add a step-4 rule: with an empty pin list and at least one embedded key, a record that fails every embedded key with no earlier authentic record is `missing_signature`, the whole file is unauthenticated, and there are no valid floors. Delete "or when it is otherwise Quarantined with no verified signer" from §5.3, so the `open_verified` "iff" is the single definition. Add this case to AT-201 and the AT-165 set.
- (b) Alternatively, call it `signature_known_signer` at that `tile_index`, with floors before it kept. Then reword step 4, and still delete the §5.3 clause.

Either way, state explicitly that an Unsigned Edge file with a CRC failure is not unauthenticated as a whole.

### F-02 (major): AT-35 gives Edge CRC failures a trailer-based `seal_status`, which contradicts the Edge label table

**Where:** AT-35; §5.4 label table (Edge rows); §5.4 per-tile bullets; DD-100.

**Quotes:**
- AT-35: "A hyper-tile **or Edge** file with a committed-tile CRC failure is labeled Quarantined (`seal_status` is `seal_failed` if a trailer is present, else `unsealed`)."
- Label table, Edge row: "Any seal-chain break (`SealedChainBreak`, …, **or a committed CRC that breaks the chain**) | any | Quarantined … | **seal_failed**".
- §5.4: "**For a hyper-tile**, `seal_status` is `seal_failed` if a seal trailer is present, otherwise `unsealed`." DD-100 likewise says "hyper-tile `seal_status`".

**Why it matters:** Edge files have no seal trailer, because the Edge trust model is the seal chain. Under AT-35, a chain-active Edge file with a committed CRC failure reports `unsealed`. Under the table it reports `seal_failed`. AT-35 and AT-165 cannot both pass with one implementation.

**Fix:** Reword AT-35 to: "hyper-tile: `seal_failed` if a trailer is present, else `unsealed`; Edge: `seal_failed` when `FLAG_SEAL_CHAIN_ACTIVE` is set (the CRC stop breaks the chain), else `unsealed`." Also state the Edge chain-inactive case in the label table.

### F-03 (major): AT-183 requires rejecting `2001:db8::1:443`, but that string is a valid bare IPv6 address, which normative text accepts

**Where:** §5.2.4 `--egress-allowlist` bullet; DD-133; AT-183.

**Quotes:**
- §5.2.4: "An unbracketed IPv6 literal with a `:port` is a startup error … **A bare unbracketed IPv6 literal with no port is an exact address and is valid.**"
- AT-183: "an unbracketed `2001:db8::1:443` is a startup error that names `--egress-allowlist`."

**Why it matters:** `2001:db8::1:443` parses as a legal IPv6 address (`2001:db8::1:443`, with `443` as a hex group). The rule "unbracketed with a port is an error" cannot be detected, which the spec itself admits ("the port cannot be distinguished"). An implementer that follows §5.2.4 accepts the string and fails AT-183. One that rejects it must invent a heuristic that also rejects legitimate addresses.

**Fix:**
- (a) Every IPv6 entry must be bracketed (`[addr]` or `[addr]:port`). Any unbracketed entry with more than one `:` is a startup error. Update §5.2.4, DD-133, and AT-183.
- (b) An unbracketed entry is parsed as an IPv6 address first; if it parses, it is a portless address. Change AT-183 to use an unparseable form (for example `2001:db8::1::443`) as the error case.

### F-04 (major): AT-180's second half ("later download → `snapshot_invalid`") cannot happen under the step-0/step-2 rules; the flow actually re-fetches and gets `409 snapshot_conflict` again

**Where:** AT-180; §5.5 steps 0–2; source decision table; DD-120; AT-186.

**Quotes:**
- AT-180: a planted `--snapshot-dir/{sha}.s3a` makes the first select or download return `409 snapshot_conflict` and "does not overwrite the planted file." Then: "A later download whose snapshot reassembly fails returns HTTP 200 with `download_error: "snapshot_invalid"`, `path: null`, and `lrs_status` `"skipped"`."
- Step 0: "A snapshot already exists if any index line for this `(file_uuid, identity)` (any verb) has a non-null `snapshot_sha256`." Step 2 appends a snapshot index line only "on a successful publish".
- §5.5: "A repeat retries the fetch when no index line … has a non-null `snapshot_sha256` and the source table requires a fetch (AT-186)."

**Why it matters:** After the conflict, no index line carries the sha, so no snapshot "exists". The later download takes source-table row 1 and fetches again. The fixture returns the same bytes, so the result hits the planted file and returns **409 `snapshot_conflict`**, not 200 `snapshot_invalid`. Reassembly from a snapshot is never attempted. The AT asserts an outcome that the normative steps never produce. An implementer either fails AT-180 or treats an unrecorded on-disk `{sha}.s3a` as an existing snapshot, which contradicts step 0.

**Fix:**
- (a) Rewrite AT-180's second half: "A later download of the same identity re-fetches, hits the same planted file, and returns 409 `snapshot_conflict` with `included: true` and `lrs_status: "skipped"` (step 0 matched)." Test the 200 `snapshot_invalid` download path in a separate fixture where a snapshot **was** recorded and then corrupted (as AT-209 does for select).
- (b) Alternatively, specify that a conflict outcome records the sha (for example a `snapshot_conflict_sha256` index field) and that later requests skip the fetch. That is a bigger change and needs a DD.

---

## Minors

### F-05 (minor): The error catalogue and DD-103 say any failed reuse check is `409 snapshot_conflict`, but step 0, DD-120, and AT-209 return 200 `snapshot_invalid` for a recorded snapshot

**Where:** §5.2.1 error table (`snapshot_conflict` row); DD-103; §5.5 step 0; DD-120; AT-209.

**Quotes:**
- Error table: "`snapshot_conflict` | An existing `{sha}.s3a` in `--snapshot-dir` failed the reuse checks."
- DD-103: "Reuse only after `verify_sealed_archive` and a BLOB reassembly … Failure is `409` `snapshot_conflict`."
- Step 0: "failure yields `200` with `snapshot: null` and `snapshot_error: "snapshot_invalid"`."

**Why it matters:** Both cases are "an existing `{sha}.s3a` failed the reuse checks". The catalogue is declared normative (DD-83). The only distinguishing condition, a freshly fetched capture versus a sha recorded in the index, is not stated in the table or in DD-103.

**Fix:** Reword the catalogue row and DD-103 to: "`snapshot_conflict`: a **newly fetched** capture's `{sha}.s3a` already exists and fails the reuse checks; a sha already **recorded** in the index that fails the checks is 200 `snapshot_invalid` (step 0, AT-209)."

### F-06 (minor): The retry-file write gate in §5.5 checks only the consent flag; DD-102 and AT-156 also require the inhibit latch

**Where:** §5.5 "Queue file names" paragraph; §5.5 HTTP outcomes; DD-102; DD-134; AT-156 case 3.

**Quotes:**
- §5.5: "Writing a new retry file, and the consent check that allows that write, run in the same mutex hold. If consent is `off` in that hold, the host writes nothing."
- DD-102: "if `lrs_inhibit` is set **or** the flag is `off`, write nothing and return `lrs_status: "skipped"`."

**Why it matters:** A revoke increments `epoch` before it takes the mutex. If a failed POST reaches the retry-write hold in that window, the flag is still `on`. §5.5 then writes `{id}.json` and returns `queued`, while DD-102 returns `skipped`. AT-156 case 3 then passes or fails depending on timing.

**Fix:** In §5.5 and DD-134, say "if inhibit is active or the flag is `off` in that hold, write nothing; `lrs_status` is `skipped`."

### F-07 (minor): The §5.7 phone UniFFI export list omits `verification_report`

**Where:** §5.7 first paragraph; §11 step 6; compare §5.1, DD-45, DD-109, AT-99.

**Quote:** §5.7: "v1 phone UniFFI exports `KiteContext::new`, `open`, `open_verified`, `close`, `proceed_to_warehouse`, the render functions, `hit_test`, and the `KiteVerificationReport` type." AT-99 fails "if … `verification_report` is missing."

**Fix:** Add `verification_report` to the §5.7 list and to §11 step 6.

### F-08 (minor): The condition for `included: true` on 409 conflicts is stated two ways

**Where:** §5.2.1 error table (`download_conflict`, `snapshot_conflict`); `/v1/download` row; §5.6 Target and Download names; DD-119; AT-94; AT-180.

**Quotes:**
- Error table: "`included: true` whenever an inclusion record exists for the key after step 0/1 (**written on this request or earlier**)."
- §5.6, DD-119, AT-94: "`included: true` … **when step 1 already succeeded**."

**Why it matters:** For a repeat download where step 0 matched and step 1 was skipped, one reading includes the field and the other omits it.

**Fix:** Use the error-table wording everywhere: present whenever a matching inclusion exists after step 0/1.

### F-09 (minor): The DD-125 error-precedence list differs from §5.2.1, and two small hit-test details are unclear

**Where:** §5.2.1 error-catalogue preamble; DD-125; §5.1 `hit_test` bullets.

**Quotes:**
- §5.2.1: "authentication, then `405`, then `413` / `415`, then `409` `viewer_only`, then JSON / param errors …"
- DD-125: "auth, then 413/415, then malformed/missing/invalid param …" (no 405, no `viewer_only`).
- §5.1: "`floor` is 0 / unused for non-floor views; a caller-supplied `floor` on a non-floor view is `400` `invalid_param`." The FFI signature, however, is `floor: Option<u32>` with `None` on other views.

**Why it matters:**
- The two precedence lists disagree.
- The required/forbidden status of `floor` depends on `view`, but `invalid_view` is checked **after** param errors. The result for an unknown `view` combined with a present or missing `floor` is undefined.
- "0 / unused" suggests `Some(0)` is acceptable, which contradicts the `invalid_param` rule.

**Fix:**
- Make DD-125 repeat the §5.2.1 order exactly.
- State that when `view` is unknown, `invalid_view` is returned before any `floor` check (or the reverse).
- Replace "0 / unused" with "`None`; `Some(_)` on a non-floor view is `invalid_param`" for the FFI as well.

### F-10 (minor): Nothing forbids host config or state files from naming the same file as each other or as the working archive

**Where:** §5.2.2 disjointness, reserved names (AT-208), path rule (DD-129).

**Why it matters:** The disjointness rules cover directories and request roots, and AT-208 covers only `--data-dir` reserved names. Nothing rejects:
- `--consent-state-file` == `--working-archive`. A consent-file temp+rename would replace the archive, or a missing pair would bootstrap `off\n…` at the archive path.
- `--working-archive` == `--auth-token-file`, `--lrs-credentials-file`, or `--pinned-keys-file`.
- `--consent-state-file` == `<working-archive>.lock`.

Most of these fail later by accident rather than by rule.

**Fix:** Add one startup rule: the canonical paths of `--working-archive`, `<working-archive>.lock`, `--consent-state-file`, `--auth-token-file`, `--lrs-credentials-file`, `--pinned-keys-file`, `--tls-cert`, and `--tls-key` must be pairwise distinct. A violation names both flags, binds nothing, and mutates nothing. Extend AT-208.

### F-11 (minor): "Every snapshot/download temp includes a flock'd marker" is undefined for a single temp file

**Where:** §5.6 "Atomic publish and temp cleanup"; DD-122; DD-136; AT-187.

**Why it matters:** An ephemeral temp **directory** can contain a marker file. A snapshot or download temp is a single **file**, `s3a-kite-tmp-{pid}-*`, so "includes a marker" has no defined meaning. Possible readings: flock the temp file itself, or use a sibling `.marker` file (which is then also an `s3a-kite-tmp-*` entry). Cleanup depends on which one is meant.

**Fix:** Specify one mechanism. For example: "the host holds `LOCK_EX` on the temp file's own fd until publish or delete; startup cleanup deletes a same-euid `s3a-kite-tmp-*` file only if `LOCK_EX|LOCK_NB` on it succeeds." Note also that the `--snapshot-dir/.lock` / `--download-dir/.lock` sidecars already exclude a second host from those directories.

### F-12 (minor): `paper_identity` checks and candidate-set rules can disagree, so Select can be painted for an identity that returns 404

**Where:** §5.1 `paper_identity` row; §5.5 candidate item 1 and size/parse rules; AT-188.

**Quotes:**
- `paper_identity`: "front matter is parseable UTF-8; size is ≤ `max_open_bytes`".
- Candidates: a dossier "whose YAML front matter is **missing**, unparseable, or not UTF-8 is omitted", and one that "cannot be canonicalized inside its root is silently omitted".

**Why it matters:** For a dossier with no front matter at all, it is unclear whether `paper_identity` is `Some`. If it is, `render_page(select_enabled=true)` paints Select, and `POST /v1/select` returns `404 unknown_paper`.

**Fix:** Define `paper_identity` as `Some(stem)` iff the opened canonical path would be a member of item 1 of the candidate set under the same rules (present, parseable UTF-8 front matter; `source_id` match; size cap; wiki-wins). Reference AT-188.

### F-13 (minor): AT-11's parenthetical restates a different consent algorithm from the single normative table

**Where:** AT-11; §5.3 item 4 (DD-147); AT-136 "(`is_active` is false on the winning record)".

**Quote:** AT-11: "(core `ConsentRecord` + marks only; latest-wins per `subject_id` for `purpose == Research`; `is_active` as in `consent.rs` …)".

**Why it matters:** DD-147 is not pure latest-wins. It adds the clauses "no later denial/withdrawal/expiry from any parseable tile" and "only trusted grants", ignores `scope` and `study_id`, and uses `now <= expires_at`. `consent.rs` `is_active` may differ on those fields or at the expiry boundary. A golden fixture hand-verified under AT-11's wording could disagree with DD-147.

**Fix:** Replace the parenthetical with "hand-verified under the §5.3 item 4 / DD-147 table". Drop the `is_active` references in AT-11 and AT-136, or state that `is_active` must agree with DD-147 on the fixtures used.

### F-14 (minor): How `/v1/open` matches the working-archive path, and whether its `path` accepts `file://`, is not specified

**Where:** §5.2.2 path rule; §5.2.1 working-archive copy paragraph; AT-53; §5.1 `Link` bullet.

**Quotes:**
- Path rule: "must end strictly inside an allowed request root, **or be exactly the working-archive path** (once created). The host first lexically normalizes the path relative to the allowed root … opens the root's dirfd and walks…"
- AT-53: "a `file://` URL that canonicalizes outside the configured request roots returns `409 path_escape`."

**Why it matters:**
- The working-archive parent is disjoint from every request root, so the dirfd walk has no root to start from. It is not stated whether "exactly" means a lexical match against the flag value or against the canonical path, or whether the open goes through the canonical parent dirfd with `O_NOFOLLOW` (as at startup).
- AT-53 implies `/v1/open` parses `file://` URLs, but the request contract says `"path": "<path>"`. Whether an in-root `file://` URL is accepted, and how it is percent-decoded, is undefined. This matters because `Link` hrefs for `file://` are returned raw.

**Fix:**
- Specify that `path` equal (after lexical normalization) to the canonical working-archive path is opened via the startup parent dirfd with `openat(O_NOFOLLOW)`, then copied under the mutex.
- Specify that `path` is either a plain absolute path only (`file://` is always `400 invalid_param` or `409 path_escape`), or a `file://` URL that is percent-decoded and then subject to the same rule. Align AT-53 with the choice.

### F-15 (minor): The viewer-only secret-file disjointness is listed narrower in §5.2.2 and AT-179 than in the path rule and DD-129

**Where:** §5.2.2 "Disjoint write and read locations" (last sentences); path-rule paragraph; DD-129; AT-179.

**Quotes:**
- §5.2.2: "Viewer-only mode applies the same disjointness between `--log-dir` and `--view-root`, and between `--pinned-keys-file` and `--view-root`."
- Path rule and DD-129: "In viewer-only mode the same rule applies to `--log-dir`, `--pinned-keys-file`, **the token file, and any TLS files** versus `--view-root`."

**Why it matters:** AT-179's viewer case tests only `--log-dir` and `--pinned-keys-file`. A viewer whose `--auth-token-file` sits inside `--view-root` would let an authenticated client open and render the token as a page. That is low risk, since the client already holds the token, but it is untested, and the two lists disagree.

**Fix:** Use the full list in §5.2.2 and add token and TLS-key cases to AT-179's viewer part.

### F-16 (minor): Fail-closed exit does not say what happens to other requests already in flight

**Where:** §5.2.1 "Fail closed (DD-138)"; DD-138; DD-144.

**Quote:** "may write one last `500` … for the request that hit the failure, does not accept any further request, and exits non-zero."

**Why it matters:** Concurrent select/download requests may be mid-fetch, holding a snapshot temp, or waiting on the mutex. The spec does not say whether they are answered (for example `500 internal`), dropped, or allowed to finish their local inclusion write first. It does say `epoch` is incremented on fail-closed exit, but not whether a pending step-1 write may still commit. That affects what the next start sees in the index versus the archive.

**Fix:** State that on a fail-closed exit, in-flight requests receive no further disk writes after the failing hold releases the mutex. Their connections are closed without a response (or get `500 internal`). Leftover temps are removed by the next start's DD-122 cleanup. A test hook in AT-169 can check that no second tile-10 is written after the failure.

---

## Not raised (checked and consistent)

Bezier mid-point (≈0.946, opacity ≈0.957) and bay sides (32/45/181) verify numerically. The §5.2.6 view matrix agrees with the error-table rows for `not_archive`, `unsupported`, and `view_mismatch`. The consent epoch/CAS rules agree with AT-198. The source decision table agrees with AT-104, AT-128, AT-129, and AT-150. The flag inventory agrees with AT-76, AT-77, and AT-132. Phone scope (DD-128, Q-15, Q-16), Q-07, Q-09, Q-10, and Q-17 are recorded decisions or open questions and were not raised.
