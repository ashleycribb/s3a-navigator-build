# Goldfish review: s3a-kite v11, round 11

**Summary: 0 blocker / 6 major / 16 minor (22 findings).** This is not a clean round: six findings are major.

Scope: only `/workspace/goldfish/s3a-kite.md` (v11) was read. I did not count items that the spec already records as DDs, Q-xx, or scope cuts unless the spec contradicts itself about them.

---

## Major

### F-01 (major): Revoke must block LRS "immediately", but the only consent flag is guarded by the mutex, and no mechanism is defined for blocking sooner
**Where:** §5.2.1 "Consent transitions and in-flight LRS"; DD-102; DD-113; DD-142; §5.5 step 3; AT-156; AT-198.
**Quotes:**
- §5.2.1: "The in-memory consent flag is **read and written only while holding that mutex**." and "As soon as a revoke request is received … the host inhibits all LRS (no new send; in-flight LRS POSTs are aborted). Under the mutex a revoke sets the flag to `off`…"
- DD-113: "On revoke: in-memory `off` first, then under the mutex write `revoke-pending`…"
- DD-142: "The in-memory consent flag is read and written only while holding the write mutex … A received revoke inhibits LRS immediately."
- §5.2.1: "An `on` request that was already received or waiting when a revoke started, or that started before that revoke completed, must not set the flag to `on`…"

**Problem:**
1. DD-113 sets the flag `off` *before* taking the mutex. §5.2.1 and DD-142 say the flag is written only under the mutex. These cannot both hold.
2. A revoke can be queued behind a long disk write that holds the mutex, such as a snapshot `verify_sealed_archive`. The step-3 pre-send check also takes the mutex, and the mutex is not fair. So a pre-send check can win the mutex after the revoke was received, read `on`, and POST. That breaks "no new send" as soon as the revoke is received.
3. Aborting an in-flight POST "as soon as a revoke request is received" needs a signal outside the mutex, such as a cancellation token. The spec defines none.
4. Read literally, the AT-198 rule ("started before that revoke completed") also covers an `on` that finished before the revoke even arrived. That `on` cannot be undone after the fact.

The implementer has to invent the mechanism, and AT-156 and AT-198 can pass or fail depending on scheduling.

**Fix:** Define two pieces of state:
- **(a)** A lock-free `lrs_inhibit` latch (an atomic revoke epoch). It is set on receipt of `POST {"state":"off"}`, before the mutex is taken. The step-3 pre-send check, the retry-file write check, and replay all check it. Setting it cancels every in-flight LRS POST through a shared cancellation token.
- The persisted/in-memory `consent` flag stays mutex-only.

Then restate DD-113 as: "set `lrs_inhibit` on receipt; under the mutex set the flag `off` and persist." Restate the AT-198 rule as: "an `on` whose mutex acquisition happens after a revoke was received (epoch changed since the `on` was received) returns `off` and writes nothing." Clear `lrs_inhibit` only when a qualifying `on` persists.

### F-02: A tampered seal or a corrupted consent tile can *release* records that the intact file withholds, which breaks the restrict-only rule
**Where:** §5.3 item 4 (restrict-only paragraph and first sub-bullet); DD-67; DD-111; AT-90; AT-184; AT-185.
**Quotes:**
- §5.3 item 4: "Records from those tiles **can only restrict, never release**." and "A hyper-tile is unauthenticated as a whole when a seal trailer is present and `verify_sealed_archive` fails…"
- "If the grant map is empty, no withdrawn marks apply, **and** no parseable invalid or unauthenticated tile holds a research withdrawal, the tour is **unrestricted**…"

**Problem:** Take a sealed hyper-tile that holds a research grant for subject X and payload records for subject Y, where Y has no consent record. While the file is intact, the grant map is non-empty (filter mode), so Y is withheld. Now flip one byte of the seal trailer, or corrupt the CRC of the tile that holds the grants. Every grant is then ignored, the grant map is empty, no withdrawals exist, and the tour becomes **unrestricted**: Y's records are painted (AT-90 still lists the CRC-valid floors). The same happens when the only grant tile sits after a CRC failure or chain break. Making a file less authentic shows *more* data, which is the opposite of the DD-111 invariant. AT-185 tests only subject X, so it misses this.

**Fix (pick one):**
- **(a)** Any parseable research `ConsentRecord`/type-71 record, in any tile, authenticated or not, puts the file in filter mode. Grants from unauthenticated tiles still never allow their subject. This is compatible with AT-184 if it is limited to *grants*, since AT-184's lone invalid-tile withdrawal does not switch filter mode for others.
- **(b)** When a file is unauthenticated as a whole and any parseable research consent record exists, withhold every subject-bearing payload (fail closed).

Either way, add an AT: the AT-185 fixture plus an un-consented subject Y, which must stay withheld after the seal is tampered.

### F-03: Signer attribution for Edge type-70 records is undefined, so `signer_not_pinned`, `signature_known_signer` and `missing_signature` overlap
**Where:** §5.4 item 2 (Ed25519 bullets, per-tile outcomes, label table); DD-79; DD-100; DD-111; DD-130 priority; AT-133; AT-134; AT-155; AT-165.
**Quotes:**
- "A **signature failure from a known signer** (the signer key is in the pinned list, or the pin list is empty and the signer is the embedded device key)…: the tile at that `tile_index` and every subsequent tile are not valid floors; earlier tiles … remain…"
- "A mid-chain (or any) signature whose signer is **not** in a non-empty pinned list: the file is **Quarantined** and no tile is a valid floor"
- Label table: "`FLAG_ED25519_SIGNED` set, no type-70 / no usable key | any | Quarantined"

**Problem:** A `DeviceSealRecord` carries no signer key or key id (§4.1 fields: `tile_index`, `generation`, `seal_hash`, `prev_seal_hash`, `signature`, CRC). The signer is only known when some key *verifies* the signature. With a non-empty pin list, a record that verifies under no pinned key could be either of these:
- a corrupted signature from a pinned signer (`signature_known_signer`, so earlier floors stay), or
- a valid signature from an unpinned key (`signer_not_pinned`, so the whole file has no floors).

The spec does not say how to decide. These cases are also undefined:
- earlier records verify under a pinned key and a later record verifies only under the embedded unpinned key;
- several `DeviceIdentityRecord`s carry different keys;
- `FLAG_ED25519_SIGNED` is set and type-70 tiles are present, but there is no `DeviceIdentityRecord` and the pin list is empty, so no key is usable. The label-table phrase "no type-70 / no usable key" can be read as AND or OR;
- `FLAG_ED25519_SIGNED` is clear but type-70 tiles are present.

The valid-floor list, `quarantine_reason`, and the consent "unauthenticated as a whole" status all depend on these outcomes, and AT-165 requires pack/reader agreement on them.

**Fix:** Add a normative signer-attribution algorithm. Example:
1. Candidate keys are the pinned list if it is non-empty, else the embedded keys.
2. A record is *authentic* if it verifies under any candidate key.
3. With a non-empty list: a record that fails every pinned key but verifies under an embedded key → `signer_not_pinned`. A record that fails every key → `signature_known_signer` at its `tile_index` if an earlier record verified under a pinned key, otherwise `signer_not_pinned`.
4. Type-70 present but no candidate key at all → `missing_signature`.
5. Flag clear but type-70 present → apply the signed rules.

Add AT rows for each case and add them to the AT-165 fixture set.

### F-04: Startup changes shared state before taking the process sidecar locks, so a colliding second host can delete a live host's `revoke-pending` or retry queue
**Where:** §5.2.1 "Process exclusivity" (DD-114 lock 1); §5.2.1 startup paragraphs on `revoke-pending` / `lrs-retry`; §5.5 retry queue ("If consent is `off` at process start, the host deletes the queue"); AT-199; AT-164; DD-61 index bootstrap; DD-96 consent bootstrap; DD-88 / DD-122 temp cleanup.
**Quotes:**
- "A Scholar start **without** `--lrs-url` … deletes `--data-dir/lrs-retry/` and `--data-dir/revoke-pending` if either is present…"
- "At Scholar start, after the sidecar lock is held and if consent is `on`, the host renames every leftover `{id}.claimed`…"
- AT-164: "…is a startup error that names the colliding flag and binds nothing."

**Problem:** Only the `.claimed` rename is explicitly ordered after the sidecar lock. Nothing orders these after it:
- the `lrs-retry/` delete (consent off, or no `--lrs-url`);
- the `revoke-pending` delete (AT-199);
- the `revoke-pending` retry write of the consent file;
- the empty-index creation;
- the consent-file bootstrap;
- the temp cleanup.

Suppose a second `s3a-kite host` is started on the same `--data-dir` without `--lrs-url` while host A is mid-revoke. It can delete A's `revoke-pending` and queue before it fails the `flock`. If A then crashes, the revoke is no longer durable. A consent-on host's queued statements can also be destroyed. AT-164 only asserts "binds nothing", not "mutates nothing".

**Fix:** Make it normative that all four sidecar `flock`s are taken (non-blocking) **before** any read-modify of `--data-dir`, the consent file, `--snapshot-dir`, `--download-dir`, or the working archive. Order all startup cleanup, bootstrap, and replay after the locks. Extend AT-164: start a colliding host B (with and without `--lrs-url`) while A has `revoke-pending` / `lrs-retry/*.json` on disk, and assert those files are byte-identical after B exits.

### F-05: AT-180 says step 3 "still ran" on a later download, but step 0 skips step 3 for that request
**Where:** AT-180 (second sentence); §5.5 step 0 / step 3; DD-120; DD-131; Q-07 draft; AT-186.
**Quotes:**
- AT-180: "A later download whose snapshot reassembly fails returns HTTP 200 with `download_error: "snapshot_invalid"`, `path: null`, and an `lrs_status` (step 3 still ran)…"
- Step 0: "If a matching verb line was present, do **not** emit a new LRS statement (step 3 is skipped)."
- DD-120: "Step 3 still runs on the request that first wrote the inclusion line, including after `snapshot_conflict` or `download_conflict`" (it does not list `snapshot_invalid`).

**Problem:** `snapshot_invalid` can only happen when this `(file_uuid, identity)` already has an index line with a non-null `snapshot_sha256`. That means an earlier inclusion exists. Under the Q-07 draft (one verb for select and download), the later download's step 0 finds a matching line, so step 3 is skipped and `lrs_status` is `skipped`. AT-180's "(step 3 still ran)" tells implementers to emit a second statement, which contradicts DD-34, AT-83, and AT-186. The §5.5 step-3 sentence "Step 3 still runs after … `snapshot_invalid`" has the same problem.

**Fix:** In AT-180, change the second case to "`lrs_status` is `skipped` (step 0 matched; no new statement)". In §5.5 step 3, limit "still runs after … `snapshot_invalid`" to a request that is itself the first inclusion for its verb, which is only possible if Q-07 picks two verbs. Alternatively, drop `snapshot_invalid` from that sentence.

### F-06: Paths are canonicalized and then opened later (TOCTOU), so a symlink or FIFO swap can copy secrets into `--download-dir` or hang the host under the write mutex
**Where:** §5.2.2 Path rule (DD-89); §5.5 source decision table / `drive_path` checks; §5.5 step 2b; AT-197; DD-129.
**Quotes:**
- Step 2b: "The source must already have been accepted as a regular file no larger than `--max-open-bytes` (otherwise there is no usable on-disk source and 2b is not a hanging copy)."
- "Membership for select/download/skip is computed before the write mutex is taken."

**Problem:** The `drive_path` check (lstat regular file, canonical inside roots, size cap) runs during the membership recompute, before the mutex. The copy in 2b runs later under the mutex. Anyone who can write the corpus or wiki tree can swap the file in that window:
- **Symlink to `--auth-token-file`, the LRS credentials, or the TLS key:** the secret is copied into `--download-dir`, which is a request root. That is exactly what DD-129 forbids.
- **FIFO:** the copy blocks while holding the process-wide mutex, which AT-197 says must not happen.
- **Growth past the cap:** the copied file can exceed the size cap.

`/v1/open` has the same canonicalize-then-open gap, and so do Link follow-ups.

**Fix:** Require descriptor-based opening for every request path and `drive_path`:
- Open with `openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS)`, or walk each component with `O_NOFOLLOW`, plus `O_NONBLOCK`.
- `fstat` the open fd and require a regular file.
- Read at most `max_open_bytes + 1` bytes, and fail as "no usable on-disk source" / `file_too_large` if more arrive.
- Do the 2b copy from that fd.

Add an AT that swaps the `drive_path` to a symlink to the token file between `GET /v1/candidates` and `POST /v1/download` (using a test hook), and assert that no token bytes land in `--download-dir`.

---

## Minor

### F-07: §5.3 item 1 contradicts `open_path`'s `name_hint` parameter
**Where:** §5.3 item 1 vs §5.1 `open_path` row, DD-139, §5.2.5.
**Quote:** §5.3: "`open_path` … passing the path basename as the name hint" vs §5.1: "`name_hint` is `Option<String>`: the optional `/v1/open` `name` when the host has one, otherwise `None` (the pack then uses the path basename)."
**Problem:** These give different kind detection when a `/v1/open` `name` is supplied.
**Fix:** Reword §5.3 item 1 as "passing `name_hint` (the `/v1/open` `name`, else the basename)".

### F-08: The §8 preamble gives a stale DD range
**Where:** §8 intro.
**Quote:** "DD-01–DD-141 keep their topics except **DD-63**…"
**Problem:** DD-142 and DD-143 exist, and §12 v11 says "DD-01–DD-143".
**Fix:** Change it to DD-01–DD-143.

### F-09: Error precedence for `/v1/frame` and `/v1/hit-test` is only partly defined
**Where:** §5.2.1 error catalogue; §5.2.6; DD-125; AT-138; AT-196.
**Quote:** "The host evaluates the matrix **before** `no_frame`." AT-196: "A `view=page` frame request that includes `floor` is also `400` `invalid_param`" (on a hyper-tile handle, where the §5.2.6 matrix says `page` is `409 view_mismatch`).
**Problem:** The spec gives no order among:
- `missing_param` / `invalid_param` (400)
- `invalid_view` (400)
- `unknown_handle` (404)
- `view_mismatch` / `not_archive` / `unsupported` (409)
- `invalid_floor` (409)
- `invalid_frame_size` (400)

For example, `floor-detail` with `floor` omitted in the first state could be 400 or 409, and AT-196's page+floor case could be either. It is also unspecified whether `t`, `scroll_x`, or `instant` on views that do not use them are ignored or `invalid_param`.
**Fix:** Add one ordered list, e.g. auth → 413/415 → malformed/missing/invalid param (including floor-on-non-floor-view) → `invalid_view` → `unknown_handle` → matrix → `invalid_floor` → `invalid_frame_size` → `no_frame`. State that irrelevant optional params are ignored.

### F-10: In-flight registry entries can leak, and registry removal is not tied to the index append
**Where:** §5.5 step 0–2; §5.2.1 in-flight registry; DD-112; AT-89; AT-126.
**Quote:** Step 0: "if a fetch is required, check-or-insert the in-flight registry in this same hold." Registry: "After the snapshot is published (or the fetch fails), the registry entry is removed."
**Problem:**
- If the registry entry is inserted and step 1 then fails, "skip 2, 2b, and 3" leaves the entry in place. Later requests wait forever for a GET that never starts, so AT-89's later successful select would hang.
- Removal is not required to happen in the same hold as the snapshot publish and index append. A request arriving in between sees neither the entry nor the index line and starts a second GET, which breaks AT-126.

**Fix:** Insert into the registry only after step 1 succeeds (same hold). Remove the entry in the same mutex hold as the snapshot publish and index append, or the fetch-failure outcome. On any step-1 failure, remove any entry the request inserted.

### F-11: The outcome for a request that only waited on another request's fetch is unspecified
**Where:** §5.5 step 2; DD-112.
**Quote:** "A request that only waited for an in-flight GET does **not** append an index line; it may use the published snapshot."
**Problem:** The spec does not say what the waiter returns when the starter's fetch failed, or when the starter got `snapshot_conflict`. Possible answers include `fetch_error` copied from the starter, `snapshot_conflict`, a retry, or `download_error`. A download waiter has no bytes for 2b.
**Fix:** State that waiters receive the starter's outcome: snapshot path, `fetch_error` → `download_error: "fetch_failed"`, or `snapshot_conflict` → the same 409 with the waiter's own `included` / `lrs_status`. State that waiters never start a GET in the same request.

### F-12: Removal of `revoke-pending` after a successful startup retry is not specified
**Where:** §5.2.1 "On the next start…"; DD-113; AT-181.
**Quote:** "if `revoke-pending` is present … the host treats consent as `off`, retries the consent-file write and queue deletion, and does not send LRS."
**Problem:** The spec never says to remove `revoke-pending` afterwards. If it stays, every later start forces consent `off`, even after a later `POST on` in a subsequent process.
**Fix:** Add "then removes `revoke-pending`; failure to remove it is a startup error naming `revoke-pending`". Have AT-181 assert that it is absent.

### F-13: How the consent filter treats authentication under `open_verified` is not stated
**Where:** §5.1 `open_verified`; §5.3 item 4; §5.4 item 6; DD-80.
**Problem:** The filter needs "authenticated valid floors" and "unauthenticated as a whole". With a supplied report, kite must not re-verify, but the spec does not say how to derive these from report fields.
**Fix:** Say that under `open_verified`:
- Valid floors are the complement of `invalid_tile_indices`.
- The file is unauthenticated as a whole iff (hyper-tile ∧ `seal_status == seal_failed`) ∨ `quarantine_reason ∈ {signer_not_pinned, missing_signature}`.
- For Edge, tiles at or after `break_index` are unauthenticated.

### F-14: The consent clauses of AT-134 and AT-195 cannot fail
**Where:** AT-134 ("Consent grants on that file are ignored"); AT-195 ("The grant does not release event-log payloads…"); §5.3 item 6 "Payload display uses only valid floors."
**Problem:** These files have no valid floors, so no payload is ever displayed. The withholding assertions pass no matter how the filter behaves.
**Fix:** Assert something observable, such as `withheld_count` on the banner. Alternatively, move the restrict-only check to a fixture that keeps some valid floors (as AT-194 does for chain breaks).

### F-15: The `--egress-allowlist` `host[:port]` syntax is ambiguous for IPv6 literals
**Where:** §5.2.4 allowlist bullet; DD-133; AT-183.
**Quote:** "exact hostnames or exact IP literals, each optionally followed by `:port`."
**Problem:** In an entry like `2001:db8::1:443`, the port cannot be told apart from the address.
**Fix:** Require bracketed IPv6 (`[2001:db8::1]:443`). Make an unbracketed IPv6 literal with a port a startup error naming the flag.

### F-16: The PROVENANCE `url=` and `EXT_SOURCE_URL` do not say whether they hold the original or the final redirected URL
**Where:** §5.6 provenance document; AT-32; §5.5 LRS statement.
**Problem:** After redirects, `url=` could be either the requested URL or the final URL, so AT-32 is ambiguous for redirect fixtures.
**Fix:** State that it is the candidate's `source_url` as listed (pre-redirect). Optionally add a `final_url=` line only via a DD.

### F-17: Temp cleanup's "pid not running **or** marker can be taken" is unsafe across PID namespaces
**Where:** §5.2.2 Logs and ephemeral temp; §5.6 temp cleanup; DD-88; DD-122; DD-136.
**Problem:** Hosts can run in different PID namespaces (for example containers) that share `/tmp` or `$XDG_RUNTIME_DIR`. There, a live host's pid looks "not running", so its live temp is deleted. Directories under `s3a-kite-*` whose names contain no parseable pid are also unaddressed.
**Fix:** Delete only when the flock'd marker can be taken (that is, require the marker). Skip names whose pid cannot be parsed.

### F-18: `invalid_tile_indices` uses two range definitions, `tile_count` and `tiles_total`
**Where:** §5.1 mapping table ("`0 .. tiles_total-1`"); §5.1 pack text, AT-165 and AT-195 ("`0 .. tile_count-1`").
**Problem:** For Edge files with torn or uncommitted trailing tiles, the header `tile_count` and the reader's scanned `tiles_total` can differ. AT-165 then requires agreement on an undefined range.
**Fix:** Define one range, e.g. "every tile index the reader enumerates, including torn ones". Use the same name in both places.

### F-19: The `--working-archive` file itself is not checked for symlinks
**Where:** §5.2.2 `--working-archive`; §5.6; DD-27; DD-93.
**Quote:** "At startup the host canonicalizes its **parent directory** … the file itself may be missing."
**Problem:** If the final component is a symlink into `--corpus-root`, the host appends tile-10 records to the corpus, which DD-27 says is read-only. `/v1/open` of "exactly the working-archive path" would also follow the symlink.
**Fix:** `lstat` the final component. If it exists, it must be a regular file (not a symlink), or startup fails naming `--working-archive`. Create and open with `O_NOFOLLOW`.

### F-20: The meaning of `included: true` is undefined when step 1 was skipped by step 0
**Where:** §5.2.1 `download_conflict` / `snapshot_conflict` rows; §5.6.
**Quote:** "Body includes `included: true` when step 1 already succeeded."
**Problem:** On a repeat request, step 1 is skipped because the paper is already included. The spec does not say whether `included` is true or absent.
**Fix:** "`included: true` whenever an inclusion record exists for the key after step 0/1 (written now or earlier)."

### F-21: The timing of DNS resolution for the LRS deny entry is unspecified
**Where:** §5.2.4 deny list.
**Quote:** "the configured LRS host plus every IP it resolves to."
**Problem:** The spec does not say whether the LRS host is resolved once at startup or on every fetch. This matters with DNS changes and affects reproducibility.
**Fix:** Resolve at each fetch's connect check (and on each redirect hop), and union the result with the startup resolution.

### F-22: The write mutex is described as covering "only local disk writes", but it also covers reads
**Where:** §5.2.1 write mutex; DD-102.
**Quote:** "held **only for local disk writes** (step 0 lookup, … the working-archive read-copy for `/v1/open`)"; consent GET also takes it (DD-142).
**Problem:** The enumerated list includes reads (step 0 lookup, archive copy, consent GET). The headline rule contradicts the list, and readers could drop GET from the mutex.
**Fix:** Reword to "held only for local disk I/O and consent-state access listed here; never across network I/O".
