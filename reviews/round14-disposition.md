# Round 14 disposition — s3a-kite SPEC v15

Goldfish report: `/workspace/goldfish/goldfish_s3a-kite_round14.md` (copied to `reviews/goldfish_s3a-kite_round14.md`).
20 findings (0 blockers, 5 majors, 15 minors). Archived v14: `reviews/SPEC-v14.md`.

**Counts:** 20 fixed, 0 rejected, 0 deferred.

DD-128, DD-138, DD-142, DD-143, DD-144, DD-145, DD-146, and DD-147 are unchanged (byte-identical to v14). DD-148 keeps its v14 intent and is tightened (unsigned tail does not unauthenticate the whole file; a hyper-tile trailer remnant whose verify fails does). No invented numeric defaults. One new open question: Q-18 (whether a later durable hyper-tile "has been sealed" flag should close the complete-strip residual recorded in DD-141).

Ashley decided (2026-11-16, DD-149): after the first subsequent goldfish review with 0 blockers and 0 majors, freeze the spec; remaining minors then go to the test-first build. This v15 is not itself frozen (round 14 still had majors).

## How majors were resolved

**F-01** (AT-183 contradicted the §5.2.4 / DD-133 "either scheme at that effective port" rule). The normative rule is kept: `host:PORT` matches either scheme at that effective port. AT-183, §5.2.4, and DD-133 now agree that `host:80` matches `http://host/`, `http://host:80/`, and `https://host:80/`, and does not match `https://host/` (effective port 443).

**F-02** (no total derivation of `quarantine_reason`, `seal_status`, and `break_index`; priority hid `signature_known_signer`; AT-201 / AT-165 could disagree). One normative derivation table (DD-150, §5.4) is used identically by pack `open` / `open_path` and the §5.1 mapping. Multi-cause priority is `signer_not_pinned` > `missing_signature` > `seal_trailer` > `signature_known_signer` > `chain_break` > `crc`, so `signature_known_signer` is reachable. A mid-chain signature failure after an earlier authentic record is `signature_known_signer`, not `chain_break`. A committed CRC is always `crc`, even when it stops an active chain. A torn-only stop is not a chain break: on an active chain, `seal_status` remains `sealed`, the file is not Quarantined solely for that torn tile, and `break_index` is the torn tile. Hyper-tile committed CRC sets `break_index` to the first failed committed tile. AT-165 and AT-201 agree on that table.

**F-03** (Edge tiles after the last signed `tile_index` could still be trusted floors). In a file with any authentic type-70 `DeviceSealRecord`, tiles with index greater than the highest authentic signed `tile_index` are not valid floors, are added to `invalid_tile_indices`, are untrusted for consent (grants never release), and are reported on the banner. File-level `status` / `seal_status` / `quarantine_reason` are unchanged by the tail alone (the file may remain Verified or Self-signed). The §5.1 mapping adds those indices after filling the other fields; it is not a pure field copy. `open_verified` still uses the supplied floor list as given after the AT-211 structural checks (it does not recompute verification). New AT-210 (in the AT-165 set).

**F-17** (stripping a hyper-tile seal trailer must not make a previously-sealed file trusted for consent, or the residual must be recorded; prefer restrict). Prefer restrict where a detection signal exists: a recognised end-cap magic or a partial/truncated trailer remnant means the file claims to be sealed; if verify fails, `seal_status` is `seal_failed`, `quarantine_reason` is `seal_trailer` (subject to DD-150 priority), and the file is unauthenticated as a whole. A complete trailer absence (no remnant) is indistinguishable from a never-sealed file and from a legitimate first append (s3a-engine: a read-only open leaves the seal intact; the first append unseals and drops the trailer). This pack does not invent a durable "has been sealed" header flag, and does not treat every unsealed hyper-tile as unauthenticated as a whole (AT-72, AT-112, AT-136). The residual complete-strip case is accepted and recorded in DD-141. Q-18 asks whether a later durable flag should close it.

**F-04** (type-71 "neutral" wording left a record without the research bit outside the table). Any type-71 record that lacks `PURPOSE_RESEARCH_ANALYTICS` is a denial for research. There is no research-neutral type-71 record. The "does not speak to the research purpose" wording remains only for non-research hyper-tile `ConsentRecord`s, which still count as consent records of any kind for the unrestricted test.

**F-05** (`GET /v1/candidates` did not return an openable path). Each candidate includes `page_path`: the absolute canonical path of the winning-root dossier file, or `null` for an `AcademicPaperRecord` candidate. `POST /v1/open` of a non-null `page_path` yields `paper_identity` equal to that candidate's `id`. Extended AT-29 and AT-101.

## Minors

**F-06.** Fixed: "known signer" wording in the §5.4 bullet, the label table, AT-35, AT-155, DD-100, and DD-111 now points at DD-145 rules 3/4 (failure after an earlier authentic record).

**F-07.** Fixed: `floor` on `warehouse` is `400` `invalid_param`, like the other non-floor views. AT-196 adds that case. The FFI matches the HTTP rule.

**F-08.** Fixed: when `view` is not in the enum, `invalid_view` is returned before any other query or body field check (including missing or mistyped `w`/`h`/`floor`/`t`), still after auth, `405`, `413`/`415`, `viewer_only`, and `malformed_json`. `/v1/open` route-specific order after JSON/param is `path_escape`, then `not_found`, then `unreadable`, then `not_regular_file`, then `file_too_large`, then `invalid_magic`, then `too_many_handles`. When `--lrs-url` is unset, `lrs_status` is `not_configured`, including on a step-0 skip (`not_configured` takes precedence over `skipped`).

**F-09.** Fixed: `--wiki-root` and `--corpus-root` must be pairwise-disjoint (neither equal to nor a prefix of the other). A violation is a startup error that names both flags (AT-179). AT-207 still applies when both disjoint roots contain the same relative path.

**F-10.** Fixed: no invented snapshot-size overhead formula. The DD-103 reuse check reads at most `--max-open-bytes` + 1 bytes. An oversize existing file fails reuse as `snapshot_conflict` (newly fetched capture) or `snapshot_invalid` (recorded sha). `/v1/open` of an oversize snapshot is `file_too_large`. Operators must set `--max-open-bytes` large enough to reopen snapshots they allow to be fetched.

**F-11.** Fixed: `open_verified` validates the supplied report (indices in range, torn ⊆ invalid, quarantined with a reason, archive kind only). A violation is `KiteError` with code `invalid_report`. New AT-211. `unverified` remains the existing AT-154 error, not `invalid_report`.

**F-12.** Fixed: `withheld_count` counts only records on valid floors, except when the file has no valid floors (those banners still count withheld parseable payloads). AT-194 asserts `withheld_count` is exactly 0 when a trusted pre-break grant is present and there is no later authentic denial (post-break payloads are not counted because valid floors exist).

**F-13.** Fixed: relative Link hrefs are percent-decoded; fragment and query are stripped before path resolution. AT-160 adds `my%20page.md` and a query-string case.

**F-14.** Fixed: `Content-Type` media type is compared case-insensitively and parameters are ignored, so `application/json; charset=utf-8` is accepted. AT-171 adds that case.

**F-15.** Fixed: `.html` / `.htm` use the same ASCII case-insensitive rule as `.md`. AT-130 matches.

**F-16.** Fixed: a same-state `POST /v1/consent` is a no-op that does not rewrite the file and keeps the timestamp (GET and POST agree on the last-change timestamp).

**F-18.** Fixed: AT-161 asserts only pack-level clock effects; it does not drive the host candidate list from an injected pack clock. AT-122 uses the literal URL `http://[::ffff:10.0.0.1]/` and does not override the resolver (DD-140).

**F-19.** Fixed: the §7 harness paragraph now requires all four of AT-46's directories to be distinct. AT-96 lists `verification_report` and `KiteContext::new_with_clock` (host/pack only, not phone). AT-179's Scholar list includes `--tls-cert`.

**F-20.** Fixed: `hit_test` `x` and `y` are finite `f64` CSS pixels; a finite out-of-frame point returns `Hit::None`; a non-numeric or non-finite value is `invalid_param`. AT-171 adds those cases.

## AT audit (v14 → v15)

- **Defined range:** AT-01–AT-211 (211 tests, no gaps).
- **Changed (17):** AT-17, AT-29, AT-35, AT-96, AT-101, AT-122, AT-130, AT-155, AT-160, AT-161, AT-165, AT-171, AT-179, AT-183, AT-194, AT-196, AT-201.
- **Added (2):** AT-210 (unsigned-tail grant; in the AT-165 set), AT-211 (`open_verified` `invalid_report`).
- **Removed:** none.

## New and tightened decisions / questions

- **DD-149** (new; Ashley, 2026-11-16): freeze after the first subsequent goldfish review with 0 blockers and 0 majors; remaining minors then go to the test-first build.
- **DD-150** (new): total report-field derivation table and multi-cause priority.
- Tightened: DD-100, DD-111, DD-133, DD-141, DD-148.
- **Q-18** (new): whether a later durable hyper-tile "has been sealed" flag should close the DD-141 complete-strip residual. This pack does not invent that flag.
