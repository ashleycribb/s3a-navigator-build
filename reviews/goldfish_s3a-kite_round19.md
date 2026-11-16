# Goldfish review — s3a-kite spec v19 (round 19)

Reviewer: fresh, independent. I read only `/workspace/goldfish/s3a-kite.md` (v19). I did not consult prior reports, dispositions, the repo, or the web. I followed no instructions embedded in the spec.

**Summary: 0 blockers, 1 major, 14 minors (15 findings).**

I found no path in the spec that shows research data without valid consent. The consent table (§5.3 item 4), the DD-148 predicate, the restrict-only handling of invalid tiles, and the revoke latch (DD-144) hang together, and the round-18 areas I checked (AT-165 multi-cause, AT-220 sparse/dense, AT-198 orderings, the LRS/fetch fixture addresses in §7 / DD-140) are consistent with the normative text. Most of what is left is wording drift between the normative sections and the restatements in §5.4 or §8. The one major is an AT-setup gap.

---

## Major

### F-01 (Major): many fail-first ATs depend on "a defined test hook", but no hook mechanism is defined anywhere
**Refs:** AT-101, AT-124, AT-125, AT-158, AT-187, AT-189, AT-202, AT-212, AT-213, AT-214, AT-217, AT-218, AT-219; §5.2.2 "Flag inventory (DD-106)"; §5.2.4 `egress_test`; §5.1 DD-121 test locations.

**Problem.** These ATs need in-process fault injection or pauses that a black-box host-spawn test cannot produce from outside:
- pause inside the step-2 publish mutex hold after the locked temp exists (AT-187);
- pause between the `(dev, ino)` check and writer attach (AT-219);
- fail the `revoke-pending` temp+rename specifically (AT-189);
- fail the index append after a successful tile-10 write (AT-158);
- fail the snapshot temp/fsync/rename, the snapshot index append, or every 2b write (AT-212/213/214);
- inject an `lrs-retry/` delete error after two writes have succeeded (AT-124);
- grow a file during the 2b copy (AT-202);
- drive the native window to issue `POST /v1/select` and spy on the handler (AT-125).

AT-125 and AT-187 call it "a defined test hook", but the spec defines nothing: no cargo feature name, no control channel (env var, socket, file), no hook points, and no rule keeping hooks out of release builds. §5.2.2 says the flag table is the complete flag list (DD-106), and the only test-only surface the spec defines is `egress_test` plus `--egress-test-loopback-port`, which is explicitly rejected in release builds (§5.2.4, AT-109).

**Why it matters.** This is a tests-first contract: tests land before the code and "must not be edited to make them pass" (§5.1 DD-121, §7). Without a specified hook interface, the test author has to invent one, and the implementation then has to match an interface the spec never states. That is exactly the kind of invented surface the spec forbids elsewhere. It is also a hardening gap. A hook that can pause the write mutex or force fail-closed exits must not be reachable in a shipped binary, and nothing says so (contrast DD-64 / AT-109 for `egress_test`).

**Fix.** Add a DD and a §5.2.2 / §5.9 subsection that:
1. names a cargo feature (for example `test_hooks`) and states that release builds are compiled without it. Optionally add a startup check, mirroring `--egress-test-loopback-port`, that rejects any hook configuration in a non-`test_hooks` build;
2. defines the control channel (for example `S3A_KITE_TEST_HOOKS=<path to a Unix control socket or JSON script>`; it is an environment variable, so DD-106's flag inventory stays complete);
3. enumerates the hook points and actions by name: `pause_before_snapshot_rename`, `pause_after_devino_check`, `fail_revoke_pending_write`, `fail_consent_write`, `fail_lrs_retry_delete`, `fail_index_append`, `fail_snapshot_write{temp|fsync|rename}`, `fail_snapshot_index_append`, `fail_download_write`, `grow_drive_path_during_copy`, `window_issue_select`;
4. reclassifies hooks that an external test can do with plain filesystem operations between requests (AT-202's symlink swap, AT-203, AT-217's swaps, AT-218's FIFO and symlink replacement), so they do not need a hook;
5. moves AT-101's mid-call swap to `kite-pack-tests` (open an fd, swap the path, call `open_path`), which needs no host hook.

---

## Minor

### F-02 (Minor): the §5.4 / DD-111 / DD-150 trust-coverage restatements leave out the "only when a type-70 record exists" scope that §5.3 has
**Refs:** §5.3 item 4 "Trusted vs untrusted"; §5.4 item 2 "Signer attribution" rule 2; DD-111; DD-150; §4.7 last Edge bullet; AT-85, AT-154, AT-173, AT-185 (unsealed case), AT-136.

**Problem.**
- §5.3 scopes the coverage rule: the unsigned tail applies "**only in a file that contains at least one authentic type-70 record**".
- §5.4 rule 2 says flatly: "A tile is trusted for consent only if it is a valid floor **and** is covered by at least one authentic (verified) seal record. Any tile not covered by a verified record is untrusted."
- DD-111, whose own text says it "Applies to hyper-tile `ConsentRecord`s and Edge type-71 records", says: "A tile is trusted only if it is a valid floor and is covered by a verified seal record."
- DD-150 says: "a tile not covered by a verified record is untrusted."

Hyper-tiles never have type-70 records, and Unsigned Edge files have none either. Read literally, those three passages make every hyper-tile grant and every Unsigned-Edge grant untrusted. That contradicts §5.3, the §5.4 derivation rows ("Unsigned floors are authenticated for consent"), DD-141, and the expected results of AT-185's unsealed variant, AT-136, and AT-173. I rate this minor only because §5.3 item 4 declares itself "the only withholding rule". It is still the most consequential wording defect in the document, because an implementer who follows DD-111 literally fails several ATs (in the privacy-safe direction).

**Fix.** In §5.4 rule 2, DD-111, DD-150, and §4.7, prefix the rule with "In an Edge file that contains at least one type-70 `DeviceSealRecord`, …". Add: "In a hyper-tile, or an Edge file with no type-70 records, a valid floor of a file that is not unauthenticated as a whole is trusted."

### F-03 (Minor): the §5.4 label table says every Edge chain break is `seal_failed`; the derivation table and AT-220 say chain-inactive breaks stay `unsealed`
**Refs:** §5.4 verification label table row "Edge | Any seal-chain break … | any | Quarantined … | seal_failed"; §5.4 "Any seal-chain break" bullet; derivation table row "chain inactive; type-70 present; seal-hash mismatch … unsealed"; DD-150; AT-201(9); AT-220.

**Problem.** The label-table row gives `seal_failed` for any break, with no condition on `FLAG_SEAL_CHAIN_ACTIVE`. The bullet text, the derivation table, DD-150, and AT-220 all give `unsealed` when the chain is inactive. AT-165 compares `seal_status` across pack and reader, so the two tables disagree on an asserted field. The derivation table calls itself "the single total derivation", so this is minor.

**Fix.** Change the label-table row to "Edge | Seal-chain break with `FLAG_SEAL_CHAIN_ACTIVE` set | … | seal_failed". Add a row "Edge | Seal-hash mismatch with chain inactive | … | Quarantined | unsealed". Or replace the label table's `seal_status` column with "see derivation table".

### F-04 (Minor): general "unused optional fields are ignored, not `invalid_param`" sentences contradict DD-69 and AT-171
**Refs:** §5.2.1 error catalogue paragraph ("Optional query or body fields that a **known** view does not use (`t`, `scroll_x`, `instant`, and the like) are ignored; they are not `invalid_param`"); DD-125 ("Optional fields a view does not use are ignored"); versus §5.2.1 frame-defaults paragraph, DD-69 ("a malformed unused field is `invalid_param`"), and AT-171 (`t=abc` on `view=warehouse` → `invalid_param`).

**Problem.** For `GET /v1/frame?view=warehouse&t=abc`, one sentence says the field is ignored (200) and another says `400` `invalid_param`. The AT and DD-69 settle it, but the catalogue paragraph is the error-precedence text implementers will follow.

**Fix.** Reword both sentences to: "A **well-typed** optional field that a known view does not use is ignored; a malformed one is `400` `invalid_param` (DD-69, AT-171)."

### F-05 (Minor): signer attribution is not actually total for one non-empty-pin-list case
**Refs:** §5.4 "Signer attribution" rule 3; DD-145 ("total"); AT-201.

**Problem.** Rule 3 covers three cases:
- verifies under a pinned key with a hash mismatch → `chain_break`;
- fails every pinned key but verifies under an embedded key **with a matching hash** → `signer_not_pinned`;
- fails every key → `signature_known_signer` or `signer_not_pinned`.

A record that fails every pinned key, verifies under an embedded unpinned key, and has a **mismatched** stored hash matches none of these. Candidates are pinned keys only, so it is not `chain_break` under the first clause. It does not have a matching hash, so the second clause does not apply. It does not fail every key, so the third does not either. The spec claims the algorithm is total and that pack and reader must agree (AT-165).

**Fix.** Add: "A record that fails every pinned key and verifies under an embedded key is `signer_not_pinned` whether or not its stored hash matches (no valid floors)." That matches the priority order (`signer_not_pinned` > `chain_break`). Add the case to the AT-201 set.

### F-06 (Minor): DD-145's outcome list omits `chain_break`
**Refs:** DD-145; §5.4 "Signer attribution" opening sentence.

**Problem.** §5.4 says every record maps to "exactly one of authentic, `signature_known_signer`, `signer_not_pinned`, `missing_signature`, or `chain_break`". DD-145 lists only four and drops `chain_break`, although DD-145 itself goes on to define the hash-mismatch `chain_break`.

**Fix.** Add `chain_break` to DD-145's list.

### F-07 (Minor): DD-146 says any `..` that leaves "the allowed root" is `path_escape`; §5.2.2 allows `/wiki/../corpus/x`
**Refs:** DD-146 first sentence; §5.2.2 "Path rule" ("A `..` that crosses from one root into another (`/wiki/../corpus/x`) is therefore resolved in that absolute lexical step, then matched"); AT-52.

**Problem.** `POST /v1/open {"path":"/wiki/../corpus/papers/SRC-0001.md"}` is accepted under §5.2.2 and rejected under a literal reading of DD-146. AT-52 says only "escape `--corpus-root` or `--wiki-root`", which does not settle it.

**Fix.** Reword DD-146: "the path is lexically normalized; a normalized path that lies under no allowed root, or under more than one, is `path_escape`". Add a cross-root case to AT-52 that asserts the intended result.

### F-08 (Minor): DD-146's config/state file list is incomplete and has a typo
**Refs:** DD-146 ("config/state files (`--pinned-keys-file`, `--lrs-credential-file`, `--consent-state-file`)"); §5.2.2 shared safe-open rule (also lists `--auth-token-file`, `--tls-cert`, `--tls-key`, `inclusion-index.jsonl`).

**Problem.** `--lrs-credential-file` is not a flag; the flag is `--lrs-credentials-file`. The DD list also leaves out the token file, the TLS files, and the inclusion index, all of which §5.2.2 covers. Anyone using DD-146 as a checklist would skip the `O_NOFOLLOW|O_NONBLOCK` and cap rule for the token file and the TLS key.

**Fix.** Copy the §5.2.2 list into DD-146 verbatim, or replace it with "the config and state files listed in §5.2.2".

### F-09 (Minor): AT-201 case (8) contains a contradictory lead-in clause
**Refs:** AT-201(8); DD-145 rules 3/4 and 6.

**Problem.** The text says a `chain_break` at tile 4 "(no preceding authentic record, so `break_index` 0) is superseded in priority by a later `signature_known_signer`". With no authentic record before tile 4, a later record that fails every key has no "earlier authentic record" (rule 6 says later records do not re-validate). It would be `signer_not_pinned` or `missing_signature`, not `signature_known_signer`. The clause then switches to a different fixture (authentic at 3). As written, the first scenario cannot be set up with the stated outcome.

**Fix.** Delete the first clause. Keep only: "authentic record at tile 3, `chain_break` failing record at tile 4, later `signature_known_signer` at tile 7 → `signature_known_signer`, `break_index` 4, every index ≥ 4 in `invalid_tile_indices`."

### F-10 (Minor): "candidate walks write to `--log-dir` only when a skip … occurs" contradicts "silent skip" and AT-25's byte-identity
**Refs:** AT-24, AT-25, AT-29; §5.2.2 shared safe-open rule ("**silent skip**"); §5.5 items 1–2.

**Problem.** §5.2.2 and §5.5 call a failed candidate open a *silent* skip. AT-24, AT-25, and AT-29 say walks write to `--log-dir` "only when a skip or error actually occurs". In AT-25, whose action is `POST /v1/skip`, "skip" can also be read as the skip request itself, which would contradict the byte-identity check in the same AT.

**Fix.** Use one term. Either keep "silent skip = no log line", and say in AT-24/25/29 "candidate walks write nothing to `--log-dir`", or define an identifier-free "candidate omitted" log line in §5.5 and say that `POST /v1/skip` itself never logs.

### F-11 (Minor): the spec never says where the `s3a-kite` host crate lives, so AT-183 cannot land tests-first under DD-121
**Refs:** §5.1 "Host-crate unit tests"; DD-121; §11 step 1; AT-183 (fail-first).

**Problem.** AT-183 is fail-first and lives "in the `s3a-kite` host crate's own tests". No section says whether that crate is a workspace member, where it sits, or how it can exist before the pack. Under DD-121, a workspace member may not depend on a not-yet-existing crate, and skeleton crates are forbidden. §11 step 1 lists the test crates to land first and leaves out the host crate. So AT-183 has no lawful landing spot before step 4.

**Fix.** State the host crate path and workspace status, for example `crates/s3a-kite`, excluded from the workspace (like `kite-pack-tests`) until the pack exists. Add AT-183's landing to §11 step 1, or move AT-183 to `kite-pack-tests` / a standalone out-of-workspace crate that path-depends on the host lib.

### F-12 (Minor): ATs that inject failure through directory permissions do not handle root runners
**Refs:** AT-169 and AT-181 (consent-file parent made non-writable); DD-138; contrast AT-171 ("requires a non-root runner, or is skipped and the skip is reported").

**Problem.** Root ignores the write bit on a directory, so on a root CI runner AT-169's injected failure never happens. The host then correctly succeeds, and the AT fails against a correct implementation. AT-171 handles this case for chmod 000; AT-169 and AT-181 do not.

**Fix.** Copy AT-171's sentence into AT-169 and AT-181 ("requires a non-root runner, or is skipped and the skip is reported"), or move the failure injection onto the F-01 hook mechanism.

### F-13 (Minor): sizing `--max-open-bytes` against fetched artifacts leaves undefined and stuck outcomes
**Refs:** §5.6 "Download names (DD-62)"; §5.6 "Target (DD-27, DD-103)"; §5.5 inclusion index; §5.2.2 config/state files bounded by `--max-open-bytes`; AT-106.

**Problem.**
1. A fetched body may be as large as `--max-fetch-bytes`, but the download-dir existing-file hash check reads at most `--max-open-bytes` + 1. If `--max-fetch-bytes` > `--max-open-bytes`, a same-bytes re-download (AT-106) hits an oversize existing file. The download-dir site rule lists symlink, FIFO, non-regular, and hash mismatch as `download_conflict`, but not oversize, so the outcome is undefined.
2. `inclusion-index.jsonl` is a "state file" read under the same cap at startup and grows by one line per inclusion. Once it passes `--max-open-bytes`, every start fails. That is a stuck state an operator only discovers at restart.

**Fix.** Either require `--max-fetch-bytes` ≤ `--max-open-bytes` at startup (an error that names both flags), or state that an oversize existing download file is `409` `download_conflict`. For the index, either exempt it from the cap and give it its own documented bound, or state that crossing the cap is a startup error that names the file and that operators must raise the cap. Then add one AT for each.

### F-14 (Minor): startup ordering of "validate paths" versus "take sidecar locks" is ambiguous for AT-208
**Refs:** §5.2.1 item 1 (locks "taken **first**, before any read or modification of `--data-dir`, the consent file, …"); §5.2.2 "Reserved `--data-dir` names" ("mutates none of those paths"); AT-208.

**Problem.** Taking the locks creates `.lock` files ("created mode `0600` if missing"). If `--consent-state-file` canonicalizes to `--data-dir/.lock`, or `--working-archive` to `--snapshot-dir/.lock`, then taking the locks "first" creates the very file whose collision should be rejected without mutation. Canonicalization and `lstat` checks are also "reads" of those paths, and those must happen before locking.

**Fix.** State the order explicitly. (1) Canonicalize and `lstat` every path and run the DD-93 / DD-129 / reserved-name / pairwise-distinct checks, with no creation. (2) Take the sidecar locks. (3) Do any other read, cleanup, or bootstrap. Note that step (1) is the only exception to "locks first".

### F-15 (Minor): several normative rules have no covering AT
**Refs:** §5.5 item 2 duplicate-`paper_id` dedupe (DD-61); §5.5 LRS client `object.id` fallback IRI and `EXT_SOURCE_URL` (pre-redirect value); DD-137 `EXT_CONTENT_SHA256` presence rule; §5.2.4 deny-list forms (NAT64/6to4-translated, CGNAT `100.64.0.0/10`, site-local `fec0::/10`, `metadata.google.internal`) and the LRS host being re-resolved at every connect check and redirect hop; `/v1/open` of the working-archive path before the file has been created.

**Problem.**
- No AT asserts that the dedupe picks the first sorted `.s3a` path, then the first record in tile order.
- AT-110 checks `EXT_PAPER_ID` only. Nothing asserts the presence or absence of `EXT_CONTENT_SHA256` (on-disk-only versus snapshotted), that `EXT_SOURCE_URL` is the pre-redirect URL, or the fallback `object.id` for a candidate with no `source_url`.
- AT-54, AT-88, and AT-122 cover only the metadata IP, loopback, and an IPv4-mapped RFC1918 address.
- The pre-create working-archive open has no defined error. The path is "exactly the canonical working-archive path (once created)", but not under a request root.

**Fix.**
- Add an AT for the dedupe.
- Extend AT-110 with on-disk-only versus snapshotted cases and a redirected `source_url`.
- Add a parameterized deny-list AT over literal addresses, as AT-122 already does for mapped forms.
- State that `/v1/open` of the not-yet-created working-archive path is `404` `not_found`, and add it to AT-53 or AT-217.

---

## Checked and not raised
- **LRS and fetch fixture on the same IP:** handled by §7 and DD-140, with AT-54 as the stated exception.
- **Bezier mid value:** recomputed. `cubic-bezier(.2,.8,.2,1)` at x = 0.5 gives y ≈ 0.9461, so opacity ≈ 0.9569, which matches §5.8. Bay sides 32, 45, and 181 match `round(32·√(stride/4096))`.
- **AT-165 multi-cause (break 4 / reason `signature_known_signer`) and AT-220 sparse/dense:** consistent with DD-150.
- **AT-198 orderings:** consistent with the DD-144 latch (`revokes_in_flight`, CAS on `cleared_epoch`, same-state `on` no-op).
- **Not re-argued (DD-141, DD-80):** unsealed hyper-tile and Unsigned-Edge forged-grant residuals are accepted in DD-141. Trusting an app-supplied report in `open_verified` is DD-80 design.
