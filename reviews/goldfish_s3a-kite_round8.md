# Goldfish review: s3a-kite spec v8 (round 8)

Reviewer: fresh, independent reviewer. Input: `/workspace/goldfish/s3a-kite.md` only. No other files, prior reports, wiki, or repos were read.

**Summary: 0 blocker / 9 major / 12 minor (21 findings).** This is not a clean round: there are 9 majors.

---

## Major

### F-01 (major): Hyper-tile "empty map is unrestricted" rule cancels the DD-111 restrict-only withdrawals from invalid tiles
- **Where:** §5.3 item 4 (first bullet vs the restrict-only paragraph), DD-67, DD-111, AT-176, AT-177. Compare §5.4 item 6.
- **Quote:** §5.3 item 4: "It then applies, as extra restrictions only, any withdrawal or mark found in a parseable invalid or unauthenticated tile: a research `ConsentRecord` with `is_active == false` for a `subject_id` withholds that subject". Next bullet: "If the `subject_research_status` map is empty and no withdrawn marks apply, the tour is **unrestricted** and `withheld_count` is 0." DD-67 says the same: "empty map and no withdrawn marks ... is unrestricted."
- **Problem:** Take a hyper-tile file with no valid-floor research `ConsentRecord` (so the map is empty), no marks, and one parseable CRC-failed tile that holds a research withdrawal for subject X. The restrict-only paragraph says X is withheld. The empty-map bullet and DD-67 say the tour is unrestricted, so X is released. It is also undefined whether an invalid-tile withdrawal *enters* the map. If it does, the map becomes non-empty and every other subject flips to withheld (bullet 2). If it does not, the empty-map bullet wins. For Edge, §5.4 item 6 and AT-177 settle the same case explicitly ("the tour is not unrestricted"). Hyper-tile has no matching rule or AT. AT-176 covers only the case where a valid-floor withdrawal already exists. The result is a privacy hole: a withdrawal that the spec says must restrict can be ignored.
- **Fix:** (a) Rewrite the empty-map bullet and DD-67 as: "unrestricted only if the map is empty, no withdrawn marks apply, **and** no parseable invalid or unauthenticated tile holds a research withdrawal." State that an invalid-tile withdrawal withholds only that `subject_id` and does not make the map non-empty for other subjects. Add an AT that mirrors AT-177 for hyper-tile. (b) Alternatively, treat any invalid-tile research withdrawal as putting the file into filter mode (map non-empty, so every unlisted subject is withheld). Say so, and add the AT.

### F-02 (major): A seal-failed hyper-tile still releases consent grants, so a tampered sealed archive can un-withhold data
- **Where:** §5.3 item 4 (definition of "unauthenticated"), §5.3 item 6, §5.4 label table (rows "Seal trailer present and `verify_sealed_archive` fails"), AT-90, DD-111, AT-166.
- **Quote:** §5.3 item 6 / AT-90: "CRC-valid committed tiles may still be listed as floors". The restrict-only list is "a tile that is not a valid floor, that sits after a seal-chain break, or that is unauthenticated (including a CRC-failed tile, a mid-chain signature failure, or any tile in a file Quarantined because the signer is not in a non-empty pin list)".
- **Problem:** A hyper-tile whose seal trailer fails verification is Quarantined with `seal_failed`, but nothing in the list above covers it:
  - Its CRC-valid tiles remain "valid floors".
  - A whole-file trailer has no "break index".
  - It is not a signer case.

  So an attacker can append or edit a research grant in a sealed archive (which breaks the seal), and kite honours it as a valid-floor grant that overrides an earlier withdrawal. The candidate path takes the opposite stance: AT-166 / DD-117 drop every record from a Quarantined corpus file. It is also unstated whether tiles of an *unsealed* hyper-tile (`Unsigned`, `unsealed`) count as "unauthenticated". If they do, every grant in an unsealed file is ignored, which would break AT-136.
- **Fix:** (a) Add "every tile of a hyper-tile whose seal trailer is present and fails `verify_sealed_archive`" to the restrict-only list. Say explicitly that unsealed hyper-tiles are *not* unauthenticated for this rule. Add an AT: sealed fixture, valid withdrawal, then a tampered grant, so the seal fails, the subject stays withheld. (b) Alternatively, make a seal-failed hyper-tile show no payloads at all (consistent with AT-166), and state that.

### F-03 (major): Normative text says GET `/v1/consent` returns the file's state, but AT-181 / DD-113 require GET to return `off` while the file may still say `on`
- **Where:** §5.2.3 vs §5.2.1 "Consent revoke", DD-113, AT-169, AT-181.
- **Quote:** §5.2.3: "When a consent file is configured, GET and POST return the file's `state` and `timestamp`." AT-181: "after the AT-169 `consent_write_failed` response ... `GET /v1/consent` stays `{ "state": "off", ... }`." AT-169 makes the file unwritable while it says `on`.
- **Problem:** After `consent_write_failed`, the file still contains `on` but the in-memory flag is `off`. Following §5.2.3 makes GET return `on`, which fails AT-181 and tells the researcher tracing is on when it is off. The timestamp to return in that state is also undefined, because the file timestamp belongs to the old `on`.
- **Fix:** Change §5.2.3 to: "GET and POST return the host's in-memory consent state. The timestamp is that of the last successful host write or startup read. After `consent_write_failed`, the state is `off` and the timestamp is (a) the revoke time, or (b) `null`." Pick one of (a) or (b) and assert it in AT-181.

### F-04 (major): The idempotent repeat select contradicts itself on whether a failed fetch is retried
- **Where:** §5.2.1 `/v1/select` row, §5.5 step 0 / step 2 and the paragraph after the steps, §5.5 "Idempotency", DD-120, AT-102, AT-89.
- **Quote:**
  - `/v1/select` row: "An idempotent repeat returns the same `snapshot` path (or null) and does not emit a new LRS statement."
  - §5.5: "A fetch is not started when a snapshot already exists for the identity or when step 0 skipped it".
  - Idempotency paragraph: "a failed snapshot may be retried by a later select/download of the same paper."
  - DD-120: "Step 2 runs only if `snapshot_sha256` is null and a fetch is required."
- **Problem:** Suppose the first select wrote tile 10 but the fetch failed, so `snapshot` was null. On a repeat select, step 0 skips step 1. DD-120 and the Idempotency paragraph then require a new fetch, which can return a non-null `snapshot`. That breaks "returns the same `snapshot` path (or null)". The phrase "when step 0 skipped it" can be read as "no fetch on a repeat", which forbids the retry. AT-89 covers only a step-1 failure, so no AT pins this case. Two implementers would ship different wire behaviour.
- **Fix:** (a) Retry allowed: delete "or when step 0 skipped it". Reword the select row to "a repeat returns the existing snapshot path if one exists, otherwise it retries the fetch". Add an AT: fetch fails, the fixture is fixed, a repeat select gives one tile-10 record, one GET on the retry, and a snapshot. (b) No retry: delete the "may be retried" sentence, change DD-120 to "step 2 runs only when step 1 ran", and add the opposite AT.

### F-05 (major): A `drive_path` outside the roots is both `409` `path_escape` and "not an on-disk source"
- **Where:** §5.2.1 error catalogue (`path_escape`), §5.2.2 Path rule (DD-89), §5.5 source decision table, AT-150.
- **Quote:**
  - Catalogue: "`path_escape` | A request path, `Link` follow-up, or `drive_path` canonicalizes outside allowed roots".
  - Path rule: "This rule applies to paths that come from requests: ... a candidate file, and a resolved `drive_path`."
  - §5.5: "A Windows Google Drive path that does not so canonicalize is not an on-disk source."
  - AT-150: "a relative path that escapes is not an on-disk source (`on_disk` is false)."
- **Problem:** Consider select or download of a candidate whose `drive_path` escapes the roots. Under the catalogue the request fails `409` and no inclusion record is written. Under the decision table and AT-150 the request goes ahead as "no usable `drive_path`": it writes the inclusion, then either fetches the `source_url` or returns `no_source`. These are different outcomes, both for local inclusion records and on the wire. The grounded 3Pass tree has Windows `drive_path` values on every dossier, so this case is the norm, not an edge case.
- **Fix:** Remove `drive_path` from the `path_escape` row and from the request-path rule. State that an escaping or non-canonicalizable `drive_path` is silently treated as "no usable `drive_path`" (`on_disk: false`). Add select/download outcomes for that case to AT-150.

### F-06 (major): LRS credential parsing contradicts the s3a-lrs-server format it claims to follow
- **Where:** §4.7 (fact), §5.5 "LRS client", DD-132, AT-182.
- **Quote:** §5.5: "the s3a-lrs-server format: lines `user:password[:scopes[:archive]]` ... Each credential line is split at the **first** `:`; the username is the part before it and the password is the remainder". AT-182: "`user:pass:word:scope` is accepted: the username is `user` and the password is `pass:word:scope`."
- **Problem:** In the format the spec cites, the text after the second `:` is `scopes` (and then `archive`), not password. A normal server credentials line such as `alice:s3cret:write:arch1` makes kite send the password `s3cret:write:arch1`, and authentication fails against the server's own parse. AT-182 locks in that mismatch. Either the file is a different format from the server's (in which case "s3a-lrs-server format" is wrong), or the parse is wrong.
- **Fix:** (a) Parse as `user:password[:scopes[:archive]]`. The password is field 2, and the remaining fields are ignored by kite. Passwords cannot contain `:`, matching the server. Correct AT-182 and DD-132. (b) Alternatively, define a kite-only credentials file (`user:password`, split at the first colon), drop the "s3a-lrs-server format" claim, and say the file must not be shared with the server.

### F-07 (major): A retry file can be written after a revoke, because the consent re-check is not required inside the mutex hold
- **Where:** §5.2.1 "Write mutex", "Consent revoke and in-flight LRS", §5.5 step 3, DD-102, AT-58, AT-156.
- **Quote:** "Step 3 ... re-checks the in-memory flag immediately before any LRS HTTP POST (and before queueing a retry)". Revoke "sets an atomic in-memory consent flag to `off` **before** waiting for the write mutex. Under the mutex the host ... deletes `--data-dir/lrs-retry/`".
- **Problem:** Retry-queue mutation takes the mutex, but the spec does not say the "before queueing" check happens *inside* that hold. One valid interleaving is:
  1. The select's LRS POST fails.
  2. The select checks the flag (`on`).
  3. Revoke sets the flag `off`, takes the mutex, deletes `lrs-retry/`, and releases.
  4. The select takes the mutex and writes a retry file.

  The queue then exists after revoke. That violates AT-58 ("including any retry queue ... deleted") and the claim that `lrs-retry/` holds identities "only when consent is on". The file would be sent if consent is later turned back on.
- **Fix:** Require the consent check for queueing to run inside the same mutex hold that writes the retry file. If the flag is `off`, write nothing and return `lrs_status: "skipped"` (or `failed`). Add this interleaving to AT-156 (revoke between the LRS failure and the queue write).

### F-08 (major): Startup cleanup deletes live temp files and directories of another running host
- **Where:** §5.2.2 "Logs and ephemeral temp" (DD-88), §5.6 "Atomic publish and temp cleanup" (DD-122), §5.2.1 DD-114.
- **Quote:** "At startup the host deletes leftover `s3a-kite-*` directories under those locations that are owned by the same euid." "At startup the host deletes stale `s3a-kite-tmp-*` entries in those directories that are owned by the process euid."
- **Problem:** Several hosts under one euid are allowed: a viewer plus a Scholar host, or two Scholar hosts with distinct `--data-dir` / `--working-archive` (AT-46 does exactly this). They all share `$XDG_RUNTIME_DIR` or `/tmp`. Nothing stops them sharing `--snapshot-dir` or `--download-dir` either, because the DD-114 sidecar locks cover only the data dir and the archive. A second host's startup therefore deletes the first host's live ephemeral directory and its in-progress snapshot/download temps. The first host then gets render failures or a failed rename (`write_failed`). "Same euid" does not mean "stale".
- **Fix:**
  - (a) Put the pid (or an flock'd marker) in each temp name, and delete only entries whose owning process is gone, or whose marker lock can be taken.
  - (b) Also, or instead, take a process-lifetime `flock` on `--snapshot-dir/.lock` and `--download-dir/.lock` (like DD-114 lock 1), so two Scholar hosts cannot share those directories, and make the ephemeral cleanup lock-aware.

  Add an AT that starts a second host during a slow fetch on the first and checks that the first host's snapshot still publishes.

### F-09 (major): `KiteVerificationReport` cannot express what §5.4 needs, and the pack-side `invalid_tile_indices` is undefined for whole-file quarantine
- **Where:** §5.1 report fields and mapping table (DD-130), §5.4 item 2, per-tile outcomes, §5.3 item 4 restrict-only list, DD-80, DD-115, AT-137, AT-165.
- **Quote:** Report fields: "`status`, `seal_status`, `tiles_crc_failed`, `invalid_tile_indices`, and `detail`".
  - §5.4: "The banner reports the break index."
  - Torn tiles are "counted on the banner as ignored/torn".
  - Unpinned signer: "**no tile is a valid floor**".
  - The restrict-only rule depends on *why* a tile is invalid ("any tile in a file Quarantined because the signer is not in a non-empty pin list").
- **Problem:**
  1. Under `open_verified`, kite "uses the supplied report and does not recompute". The report carries no break index, no torn versus CRC versus chain-broken split, and no quarantine reason. The banner text, the torn count, and the restrict-only classification therefore cannot be produced as §5.4 requires (`detail` is free text).
  2. For the unpinned-signer case, the pack must treat every tile as invalid. The reader mapping lists only tiles where `tile_at` errors, CRC is not `Ok`, or `problem` is set. Unless reader-core marks every tile, the two sides disagree on `invalid_tile_indices`, and AT-165 requires them to be identical for the AT-133 / AT-155 fixtures.
  3. The spec never defines what pack `open` puts in `invalid_tile_indices` for that case (all indices, or only failed ones?).
- **Fix:**
  - Define the pack-side `invalid_tile_indices` normatively. For example: "every index not in the valid floor list, so all indices when no tile is valid".
  - Add structured fields to `KiteVerificationReport`, such as `break_index: Option<u32>`, `torn_tile_indices`, and `quarantine_reason` (enum: `crc`, `seal_trailer`, `chain_break`, `signature_known_signer`, `signer_not_pinned`, `missing_signature`), plus their reader-core mapping.
  - (a) Alternatively, state that under `open_verified`, a `quarantined` status whose reason is unknown is treated as "no valid floors, and every consent record is restrict-only". Remove the unpinned-signer fixtures from AT-165's `invalid_tile_indices` parity, or state the reader-core behaviour that makes them agree.

---

## Minor

### F-10 (minor): Revoke and consent-write error cases left undefined
- **Where:** §5.2.1 "Consent revoke", DD-113, DD-66, AT-124.
- **Problem:**
  - (1) If writing `--data-dir/revoke-pending` itself fails, the outcome is unspecified: does the consent-file write still happen, and which error is returned?
  - (2) After `revoke_delete_failed`, is `revoke-pending` left in place, and does a later `POST {on}` return `409` `revoke_pending`? AT-181 covers only `consent_write_failed`. "A send barrier until the directory is gone" does not say whether new statements may be queued into that directory or sent directly.
  - (3) A failed consent-file write on `POST {on}` has no error code (`consent_write_failed` is defined "on revoke (or on a completing bootstrap)"), and it is not stated that the in-memory flag stays `off`.
- **Fix:** Add rows or sentences:
  - A `revoke-pending` write failure gives `500` `consent_write_failed`, with the in-memory flag `off` and no LRS for the life of the process.
  - `revoke_delete_failed` leaves `revoke-pending`, and `POST {on}` returns `409` `revoke_pending`.
  - A failed `on` write gives `500` `consent_write_failed` with the flag still `off`.

### F-11 (minor): LRS response classification and concurrent queue replay are undefined
- **Where:** §5.5 LRS client / retry queue, DD-131, Q-09.
- **Problem:** `sent` means "the LRS accepted the statement", but which statuses count as accepted is not defined. Is it 200 and 204 only? Is a `409` (duplicate statement `id` on replay) success? Is a 4xx permanent (dropped) or queued forever? (Backoff is Q-09; classification is not.) Also, "retries ... after each later successful LRS emit" lets two concurrent successful emits both replay the same queue file, so the same statement can be sent twice at the same time.
- **Fix:**
  - Define accepted as any 2xx. Decide whether 409 on a replayed `id` counts as success (recommended).
  - Decide whether 4xx other than 408/429 is a permanent failure: log it, keep the file, and stop retrying.
  - Serialize replay with a single replay worker, or claim each queue file under the mutex before sending.

### F-12 (minor): Hit-test cache key stated two ways; `/v1/proceed` is not under the per-handle lock
- **Where:** §5.2.1 "Handles", DD-74 vs §5.1, §5.2.6, DD-125.
- **Quote:** "`hit_test` uses the last `RenderFrame` produced for that handle and `view`" (§5.2.1, DD-74) vs "keyed by `(view, floor)`" (DD-125).
- **Problem:** The two keys disagree for floor views. Separately, only `/v1/frame` and `/v1/hit-test` are serialized per handle. A concurrent `/v1/proceed` (or `/v1/close`) can change handle state in the middle of a render or matrix check.
- **Fix:** Use `(handle, view, floor)` everywhere. Put `/v1/proceed` and `/v1/close` under the same per-handle lock.

### F-13 (minor): The injectable clock has no interface, and when `now` is evaluated is unspecified
- **Where:** §5.3 item 4, §5.4 item 6, DD-110, AT-161; §5.1 API table.
- **Problem:** "Pack tests inject a clock", but no `open*` or `render_*` signature, and no `KiteContext` constructor, accepts a clock. It is also undefined whether `now` is sampled at open, at each render, or at each candidate recompute. A consent record that expires while a handle is open could therefore stay visible.
- **Fix:** Add a clock to `KiteContext` (for example `KiteContext::new_with_clock(ua_fonts, clock)`, test-only or exported). State that withholding is evaluated at each render (and at each candidate recompute) with that clock.

### F-14 (minor): DD-99 says `unverified` is not a legal status; §5.4 and DD-116 say it is
- **Where:** DD-99 vs §5.4 label table paragraph, DD-116, §5.1 mapping table.
- **Quote:** DD-99: "Reader-core `Unverified` is not a legal `status`." §5.4: "`unverified` ... is a legal report field value and is a `KiteError` from `open_verified`."
- **Fix:** Reword DD-99 to: "`unverified` is a legal field value but is rejected by `open_verified` (DD-116)."

### F-15 (minor): Allowlist wording "matches only that scheme default port" conflicts with the AT-183 example
- **Where:** §5.2.4, DD-133, AT-183.
- **Problem:** "An entry without a port matches only that scheme default port" reads naturally as 80 for `http` and 443 for `https`. AT-183 says the entry `host` blocks `http://host/`, so a no-port entry actually means 443 only. It is also unstated whether `host:80` permits `http://host/`, and whether `host:443` permits `http://host:443/`.
- **Fix:** State: "An entry without a port matches only port 443 with scheme `https`. `host:PORT` matches either scheme at that effective port." Or put the scheme in the entry. Add the `host:80` case to AT-183.

### F-16 (minor): The set of allowed `--bind` values does not match the text and ATs; TLS flags on loopback are undefined
- **Where:** §5.2.2 flag table (`--bind`: "loopback IP, `0.0.0.0`, or `unix:`"), §5.2.2 bullet "A non-loopback, non-Unix `--bind` requires ...", AT-87 "(or any non-loopback address)", §5.2 "Native window" ("those binds never use TLS").
- **Problem:** It is unclear whether a specific interface IP (`192.0.2.10`) or `::` is a valid bind or a startup error. Passing `--tls-cert`/`--tls-key` with a loopback bind is neither forbidden nor defined (does it serve TLS? is it ignored?). That conflicts with "those binds never use TLS".
- **Fix:** Either widen the type to "any IP literal" or state that only `0.0.0.0` is accepted as non-loopback. Define TLS flags on loopback or `unix:` as either forbidden (a startup error naming the flag) or honoured.

### F-17 (minor): Gaps in the state-file and request-root disjointness rules
- **Where:** §5.2.2 "Disjoint write and read locations", "Path rule", DD-93, DD-129, AT-179.
- **Problem:**
  - (1) In viewer-only mode, the rule lists "`--log-dir`, the token file, and any TLS files versus `--view-root`" and omits `--pinned-keys-file`, which is accepted in viewer mode and is a trust anchor.
  - (2) The working-archive parent is not required to be disjoint from `--snapshot-dir`, `--download-dir`, or `--log-dir`. The `{working-archive}.lock` sidecar and the archive itself can therefore sit inside a request root, contrary to "secret and state files must not sit inside a request root".
- **Fix:** Add `--pinned-keys-file` to the viewer rule. Require the working-archive parent to be disjoint from snapshot, download, and log dirs, or exclude `*.lock` sidecars from `/v1/open` (`not_regular_file`). Extend AT-179.

### F-18 (minor): `paper_identity` "bound paper page" is undefined; Select can appear for a page that is not a candidate
- **Where:** §5.1 `paper_identity`, Select hit rule, DD-91.
- **Problem:** "a paper dossier or a bound paper page" is never defined. Nor is how the host decides that an opened `.md` is a dossier: by matching `{wiki,corpus}-root/papers/SRC-*.md`, or by basename only? A `SRC-0001.md` opened from `--view-root` or `--snapshot-dir` might paint a Select control whose `POST /v1/select` then returns `404` `unknown_paper`.
- **Fix:** Define `paper_identity` as `Some(stem)` only when the canonical opened path is a current-candidate dossier under the §5.5 glob (with wiki-root winning), and otherwise `None`. Delete "bound paper page", or define it.

### F-19 (minor): The candidate walk has no size bound, and malformed dossier YAML is unhandled
- **Where:** §5.5 candidate set items 1–2.
- **Problem:** The recursive `*.s3a` walk opens every corpus archive (`from_bytes`, so the whole file goes into memory) on every candidates, select, download, and skip request. The spec does not say whether `--max-open-bytes` applies, so a single large corpus file can exhaust memory. YAML front matter that is missing, malformed, or non-UTF-8 has no defined outcome: should the file be omitted, or listed with an empty title?
- **Fix:** Say the walk skips files larger than `--max-open-bytes` (as "fail to open"). Say a dossier with unparseable front matter is omitted (no id logged), or is listed with `title: ""` and `source_url: null`. Pick one and add it to an AT.

### F-20 (minor): `EXT_CONTENT_SHA256` "when bytes were captured" is ambiguous
- **Where:** §5.5 LRS statement, `object.definition.extensions`; the paragraph after step 3 ("the statement may omit content SHA-256").
- **Problem:** It is unclear whether bytes are "captured" for an on-disk copy (a download from `drive_path`, with no snapshot), for a reused snapshot (AT-157), or for a waiter on an in-flight fetch. "May omit" is also not testable.
- **Fix:** Define it as "present if and only if the index has a non-null `snapshot_sha256` for `(file_uuid, identity)` at step 3, and equal to it". Optionally, for downloads, add the delivered file's SHA-256.

### F-21 (minor): Editorial inconsistencies
- §8 header: "DD-01–DD-128 keep their topics", but the table and §12 v8 run to DD-133.
- Preamble: "v7 keeps v3's v1 scope cuts" in a v8 document.
- §5.4 item 4: "The Continue control from the header sheet reaches the tour". Per DD-97 / AT-152, Continue alone does not; `proceed` does.
- §6.3 / AT-70: a "host directory" artifact fails without "the in-app About screen", but a headless host has no app screen. Say the host artifact satisfies this with the files only.
- **Fix:** Correct each of these sentences.

---

## Blocker and major titles
- F-01: Hyper-tile "empty map is unrestricted" rule cancels the DD-111 restrict-only withdrawals from invalid tiles
- F-02: A seal-failed hyper-tile still releases consent grants (tampered sealed archive can un-withhold data)
- F-03: Normative text says GET `/v1/consent` returns the file's state; AT-181 / DD-113 require `off`
- F-04: The idempotent repeat select contradicts itself on retrying a failed fetch
- F-05: A `drive_path` outside the roots is both `409` `path_escape` and "not an on-disk source"
- F-06: LRS credential parsing contradicts the s3a-lrs-server format it cites
- F-07: A retry file can be written after a revoke (re-check not required in the mutex hold)
- F-08: Startup cleanup deletes live temp files and directories of another running host
- F-09: `KiteVerificationReport` cannot express what §5.4 needs; pack-side `invalid_tile_indices` undefined
