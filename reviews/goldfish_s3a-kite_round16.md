# Goldfish review: s3a-kite spec v16, round 16

Reviewer: fresh, independent. I read only `/workspace/goldfish/s3a-kite.md` (v16, 1,359 lines).

**Summary: 0 blockers / 3 majors / 10 minors (13 findings).** This is **not** a clean round, so DD-149's freeze condition is not met.

The majors are:

* F-01: A same-state `off` POST leaves `revokes_in_flight` stuck above 0, so consent can never be turned back on in that process.
* F-02: AT-187 needs a snapshot temp that the normative step 2 never creates mid-fetch.
* F-03: Three opens that run under the write mutex have no `O_NONBLOCK` or regular-file check, so a FIFO or other non-regular file can hang the host.

---

## F-01 (major): A same-state `POST {"state":"off"}` runs the revoke-receipt latch updates but never decrements `revokes_in_flight`, which blocks consent `on` for the rest of the process

**Where:** §5.2.1 "Consent transitions and in-flight LRS" (revoke receipt, the four persist steps, and "After those four writes succeed, the host decrements `revokes_in_flight`"); §5.2.3 (same-state no-op); §5.2.1 table row `POST /v1/consent` ("a same-state POST is a no-op that keeps the timestamp"); DD-142; DD-144; AT-198 second ordering.

**Conflicting text:**
- §5.2.1: "As soon as a revoke request (`POST {"state":"off"}`) is received, and before the mutex is taken, the host increments `epoch`, increments `revokes_in_flight`, …" and "After those four writes succeed, the host decrements `revokes_in_flight`."
- §5.2.3: "A `POST /v1/consent` whose `state` equals the current in-memory state is a no-op: the host does not rewrite the consent file and the timestamp is unchanged."
- DD-142 / DD-144: an `on` "qualifies only when `revokes_in_flight` was 0 at receipt and is 0 at the CAS."

**Why this is a problem:** The receipt-time actions happen before the host can know whether the `off` is same-state. Under the mutex, a same-state `off` is defined as a no-op, and a no-op does not perform the "four writes" (it must not rewrite the consent file). The only specified decrement is "after those four writes succeed," so a same-state `off` leaves `revokes_in_flight` permanently above 0. Every later `POST on` then fails to qualify and returns `off`. Consent cannot be turned back on until the process restarts.

The spec also never says whether a same-state `off` does any of the revoke sequence: write `revoke-pending`, rewrite the file, delete `lrs-retry/`, or change the timestamp. The two texts contradict each other on this. AT-198's second ordering ("a revoke is received … while the flag is already `off` from an earlier revoke") is exactly a same-state `off`. It asserts only the immediate outcome and never checks that a later `on` can succeed. A literal implementation would pass AT-198 and still lock consent off. The same increment-without-decrement happens for `POST off` on a host without `--consent-state-file`, which §5.2.3 also calls a no-op. That case is harmless because LRS is unconfigured, but the rule should say so.

**Suggested fix (pick one):**
- (a) Say that a same-state `off` under the mutex still re-cancels and swaps the token, performs no persistent writes (the timestamp is unchanged), and **decrements `revokes_in_flight` before releasing the mutex**. Generalize the decrement rule to "every revoke decrements `revokes_in_flight` at the end of its mutex hold, whether or not it persisted; only a fail-closed exit skips it."
- (b) Make every `off` a full revoke: run the four-step persist even when already `off`, so the timestamp changes and §5.2.3's no-op sentence becomes on-only.

Either way, extend AT-198's second ordering: after both requests complete, a `POST on` issued later returns `on`.

---

## F-02 (major): AT-187 requires a `--snapshot-dir` temp to exist mid-fetch, but step 2 creates that temp only after the GET returns

**Where:** AT-187; §5.5 numbered step 2; §5.2.1 "In-flight fetch registry" paragraph; §5.6 "Atomic publish and temp cleanup"; DD-122; DD-136.

**Conflicting text:**
- AT-187: "While Scholar host A is mid-flight on a slow `egress_test` fetch (temp already created under `--snapshot-dir` and under `$XDG_RUNTIME_DIR` or `/tmp`) …"
- §5.5 step 2: "insert into the registry in this mutex hold and start one GET (mutex released across the GET). **Then, under the mutex**, if this request started the GET: write the snapshot (temp file + fsync + atomic rename) …"
- §5.5: "Render caches and per-request temp files live in the ephemeral directory of §5.2.2."

**Why this is a problem:** Under the normative order, the `--snapshot-dir` temp exists only inside the post-GET mutex hold. The in-flight body belongs in the ephemeral directory, and the mutex is never held across network I/O. A conforming build therefore has no snapshot-dir temp while the fetch is in flight, so AT-187's precondition can't be set up against it.

To pass AT-187 as written, an implementer would have to create, and probably stream into, a `--snapshot-dir` temp during the GET. That is local disk I/O in a kite-written directory that the mutex list does not cover, and that §5.5 step 2 does not describe. Either the test fails, or it forces a build that departs from step 2. The cleanup property AT-187 is meant to protect (B must not delete A's live `s3a-kite-tmp-*`) is real and worth testing, but it has to be reachable.

**Suggested fix (pick one):**
- (a) Reword AT-187 to use a test hook that pauses host A **inside the step-2 publish hold**, after the `s3a-kite-tmp-*` temp is created and `LOCK_EX`-locked and before the rename. Keep the "mid-fetch" wording only for the ephemeral-directory temp.
- (b) Make normative that step 2 creates and locks the `--snapshot-dir` temp (empty) in the same hold as the registry insert, before the GET, and streams nothing into it until the publish hold. Add that create to the §5.2.1 mutex list and to the temp-cleanup paths for fetch failure and conflict.

---

## F-03 (major): Opens that run under the write mutex have no `O_NONBLOCK` or regular-file validation, so a FIFO hangs every select, download, and consent call

**Where:** §5.6 "Target (DD-27, DD-103)" (opening an existing `{sha}.s3a` for reuse); §5.6 "Download names (DD-62)" (hashing an existing `--download-dir/{sha}`); §5.2.2 path rule and §5.2.1 (the working-archive `/v1/open` copy "via the startup parent dirfd with `openat(O_NOFOLLOW)` and then copies the bytes under the write mutex"); §5.5 step 0 and steps 2/2b (all in a mutex hold); compare DD-146 and AT-197 / AT-202.

**Conflicting text:**
- §5.6: "Before reuse it opens the existing file (reading at most `--max-open-bytes` + 1 bytes …), runs `verify_sealed_archive` …". No `O_NOFOLLOW`, no `O_NONBLOCK`, no `fstat` regular-file check.
- §5.6: "If that path already exists and the existing file's SHA-256 equals the name, download is a success no-op …". No open discipline, and no read cap.
- AT-197 / AT-202: a FIFO `drive_path` "do[es] not hang holding the write mutex". DD-146 opens `drive_path` with `O_NOFOLLOW|O_NONBLOCK` and `fstat` for exactly this reason.

**Why this is a problem:** The spec's own threat model includes planted files in `--snapshot-dir` and `--download-dir` (AT-94, AT-106, AT-180, AT-209). All three opens run inside the process-wide write mutex: the step-0 reuse check, the step-2 publish-hold reuse check, the 2b existing-file check, and the working-archive read-copy. A FIFO planted at `{sha}.s3a` or `--download-dir/{sha}`, or swapped in as the working archive's final component, makes a plain `open`/`read` block forever while holding the mutex. That blocks:

* every later select, download, and consent GET/POST;
* the persist half of a revoke (the `lrs_inhibit` latch still stops sends, but the revoke never completes or persists);
* any further `/v1/open` of the working archive.

The spec closed this exact hazard for `drive_path` and request paths but left these three sites to the implementer's guess. A planted symlink at `--download-dir/{sha}` is also read through, for example into the token file. That read only feeds a hash comparison, so nothing leaks, but it is still inconsistent with "never follow".

**Suggested fix:** Add one rule (for example as a DD-146 extension) for every host-internal open of an existing artifact under a mutex hold: snapshot reuse, the download-dir existing-file check, the working-archive `/v1/open` copy, and 2b snapshot reassembly. Each must use `openat` from the directory's startup dirfd with `O_NOFOLLOW|O_NONBLOCK`, `fstat` for a regular file, and read at most `--max-open-bytes` + 1 bytes. Map the failures as follows:
- Snapshot reuse failure: the existing "reuse fails" outcome (`409 snapshot_conflict`, or `200 snapshot_invalid` when the sha is recorded).
- Download-dir non-regular file or symlink: `409 download_conflict`. Say so explicitly, because a symlink or FIFO "does not hash to the name".
- Working-archive copy failure: `500 write_failed` or a named open error.

Add an AT that plants a FIFO at `--snapshot-dir/{sha}.s3a` and asserts that select returns and a concurrent `GET /v1/consent` is not blocked.

---

## F-04 (minor): AT-198's first-ordering assertion doesn't cover the legal case where the in-flight same-state `on` wins the mutex and returns `on`

**Where:** AT-198; §5.2.1 consent-transitions paragraph ("A same-state `POST {"state":"on"}` … is a no-op"); the `POST /v1/consent` table row ("`200` with the same body as GET").

**Quote:** AT-198: "every completed `on` that raced the revoke has returned `off` (or the `off` won and that `on` wrote nothing)". The host starts with consent `on`.

**Why this is a problem:** If the `on` was received before the revoke and takes the mutex first, the flag is still `on`. It is a same-state no-op, and by the table it returns the GET body `{"state":"on"}`. Then the revoke runs, and the final state is `off`. That `on` did not "return `off`". "The `off` won" can only be read as the final state, not the mutex order. A strict test author could assert `off`, which would force the no-op to consult the inhibit latch and break the "same body as GET" rule.

**Fix:** Reword the assertion to: "each racing `on` either returned `on` before the revoke's mutex hold (a same-state no-op that wrote nothing) or returned `off`. In every case the final GET, the consent file, and the LRS observations are `off`/zero."

---

## F-05 (minor): AT-208 asserts a `--consent-state-file.lock` collision that the normative distinctness list doesn't include

**Where:** §5.2.2 "Reserved `--data-dir` names (AT-208)"; DD-114; AT-208.

**Quote:** §5.2.2's pairwise-distinct set is "`--working-archive`, `--working-archive.lock` …, `--consent-state-file`, `--auth-token-file`, `--lrs-credentials-file`, `--pinned-keys-file`, `--tls-cert`, and `--tls-key`". Its reserved names don't include `--consent-state-file.lock` either. AT-208: "with `--working-archive` equal to … `--consent-state-file.lock` … is the same kind of error."

**Why this is a problem:** The test asserts a startup error that the normative text doesn't require. The reverse case (`--consent-state-file` equal to `--working-archive.lock`) is covered, so this one is just missing.

**Fix:** Add `--consent-state-file.lock` (when configured) to the §5.2.2 reserved and pairwise-distinct lists. Also consider covering `--snapshot-dir/.lock` and `--download-dir/.lock`, which "a reserved sidecar `*.lock`" in AT-208 suggests but §5.2.2 checks only against `--consent-state-file` and `--working-archive`.

---

## F-06 (minor): The spec doesn't say which allowed root a request path is walked from when roots nest, and `--view-root` may contain `--wiki-root` or `--corpus-root`

**Where:** §5.2.2 path rule and disjointness rules; DD-146; §5.1 `open_path` (`validated_root`) and `paper_identity`; AT-101.

**Quote:** `validated_root` is "the allowed request root that walk started from". The disjointness rules make wiki and corpus disjoint from each other and from the write dirs. **No rule** makes `--view-root` (optional in Scholar mode) disjoint from `--wiki-root` or `--corpus-root`.

**Why this is a problem:** If `--view-root` is an ancestor of `--wiki-root`, a dossier path lies under two roots. Root choice changes observable behavior:
- Lexical `..` handling: `/view/wiki/../x` stays inside the view root but leaves the wiki root, so one choice gives a successful open and the other gives `path_escape`.
- The `validated_root` passed to the pack, and with it how `paper_identity`'s "using `validated_root` as the root of the walk" check is read. That decides whether the Select control appears (AT-101).

**Fix (pick one):**
- (a) Require `--view-root` to be pairwise-disjoint from `--wiki-root` and `--corpus-root` in Scholar mode, and add that to AT-179.
- (b) Define the selection order: the most specific (longest) matching root, with `--wiki-root` > `--corpus-root` > `--snapshot-dir` > `--download-dir` > `--view-root` on ties. Also say that `paper_identity`'s glob root is `wiki_root`/`corpus_root`, independent of `validated_root`.

---

## F-07 (minor): The error for a non-absolute `/v1/open` `path` is undefined

**Where:** §5.2.2 path rule; §5.2.1 `/v1/open` error tail.

**Quote:** "The JSON `path` is a plain absolute filesystem path, or a `file://` URL …"

**Why this is a problem:** A relative path such as `papers/SRC-0001.md`, or an empty string, has no specified outcome. It could be `invalid_param`, `path_escape`, or resolved against some root. Tests and clients will diverge.

**Fix:** State that a non-absolute, empty, or NUL-containing `path` is `400` `invalid_param`, or alternatively `409` `path_escape`, and add it to AT-171.

---

## F-08 (minor): The spec doesn't order revoke receipt-time latch actions relative to auth and request validation

**Where:** §5.2.1 consent-transitions paragraph ("As soon as a revoke request … is received, and before the mutex is taken, the host increments `epoch` …"); the §5.2.1 error-precedence list; DD-144.

**Why this is a problem:** The error-precedence list puts auth, 405, 413/415, `viewer_only`, and JSON errors before route logic. "Received" isn't tied to that order. An implementation that peeks at the body early could let an unauthenticated or malformed `{"state":"off"}` bump `epoch`. That lets an unauthenticated caller inhibit LRS (a DoS on consented tracing) and, combined with F-01, leak `revokes_in_flight`.

**Fix:** State that receipt-time actions run only after the request has passed every check in the §5.2.1 error-precedence list (auth through `invalid_param`) and the body is a valid `{"state":"off"}` on a Scholar host.

---

## F-09 (minor): Edge torn-tile handling with the seal chain inactive is inconsistent between the derivation table, the §5.4 prose, and the §5.1 mapping

**Where:** §5.4 "Per-tile outcomes" torn bullet; §5.4 DD-150 rows "Hyper-tile: torn or uncommitted trailing tile only" (`break_index` = first torn) and "Edge: torn-only; chain inactive" (`break_index` None); §5.1 mapping row `invalid_tile_indices` ("uncommitted indices (… or after the first torn, CRC-failed, or chain-stop tile)").

**Why this is a problem:** In the Edge torn predicate, a mid-file tile can be torn: an erased or invalid header with committed tiles after it. With `FLAG_SEAL_CHAIN_ACTIVE` clear, the DD-150 row sets no `break_index`. The only rule that invalidates later tiles is "The Edge seal chain … stops at the first torn … tile; … every subsequent tile is uncommitted", and it's unclear whether that chain walk runs at all on a chain-inactive file. The §5.1 mapping invalidates every tile after the first torn regardless.

If pack `open` keeps later tiles as valid floors, a trusted type-71 grant on such a tile would release payloads that the mapped reader report treats as invalid. That is a privacy-relevant disagreement, and AT-165 requires equality. Hyper-tile, by contrast, sets `break_index` at the first torn tile.

**Fix:** In §5.4, state that for Edge, whether or not the chain is active, every tile after the first torn or uncommitted tile is uncommitted and not a valid floor. Either set `break_index` to that tile for chain-inactive files too (matching hyper-tile, and updating the §5.1 `break_index` row to drop "on an active chain"), or keep it None and say explicitly that invalidity comes from the uncommitted predicate. Add a chain-inactive mid-file torn fixture to AT-165.

---

## F-10 (minor): The pack entry point, read cap, and identity check for the working-archive `/v1/open` copy are unspecified

**Where:** §5.2.1 ("Opening the Scholar working-archive path via `/v1/open` succeeds: the host takes a `from_bytes` copy …"); §5.2.2 path rule; §5.1 `open_path` (which requires `validated_root`, an allowed request root).

**Why this is a problem:**
- The working-archive parent is deliberately not a request root, so the host has no valid `validated_root` to pass to `open_path`. The spec doesn't say whether the host calls `open(bytes)` instead, which is implied but not stated.
- The copy has no stated `--max-open-bytes` + 1 read bound. `open` checks the cap only after the whole file is in memory.
- Unlike step 1, there is no `(dev, ino)` re-check, so a regular file swapped in at the final component is opened under the working-archive exception, outside any request root.
- The `O_NONBLOCK` and `fstat` gap is covered in F-03.

**Fix:** State that the host reads at most `--max-open-bytes` + 1 bytes from the dirfd-opened fd (otherwise `409 file_too_large`), re-checks `(dev, ino)` (otherwise `409 path_escape` or `500 write_failed`, pick one), and calls pack `open(ctx, bytes, name_hint=basename, …, pinned_keys)`, so `paper_identity` is None.

---

## F-11 (minor): `verification_report` and `open_verified` on `kind: "unsupported"` handles are undefined

**Where:** §5.1 `verification_report` row (errors only for `wiki`/`html`/`other`); `open_verified` validation ("the bytes are an archive kind (not `wiki` / `html` / `other`)"); §5.2.6 (archive kinds include `unsupported`).

**Why this is a problem:** For an unsupported stride or an inconsistent Edge header, `MmapReader` can't reliably enumerate tiles, so `tiles_total` (and with it `invalid_tile_indices`) is undefined. Yet both functions accept that kind. Implementers will return arbitrary reports, and a phone `open_verified` with a mapped report for such bytes has no defined validation.

**Fix (pick one):**
- (a) `verification_report` on an `unsupported` handle returns `status: quarantined`, an explicit `invalid_tile_indices: []` with `tiles_total` defined as 0, and `break_index: None`. `open_verified` of unsupported bytes ignores the report and opens the banner.
- (b) Both return `KiteError` `not_archive` (or a new `unsupported`) for that kind.

---

## F-12 (minor): For `--lrs-url`, the spec doesn't define what counts as a "loopback" host

**Where:** §5.2.2 inventory (`--lrs-url`: "`https`, or `http` on loopback"); §5.5 LRS client ("`http` is allowed when the host is loopback").

**Why this is a problem:** The spec doesn't say whether `http://localhost:…` (a name) is accepted, or only the IP literals `127.0.0.0/8` and `::1`. If names are accepted, it doesn't say whether resolution happens at startup or per connection (a name that later resolves off-host would send Basic credentials in cleartext). Test fixtures use IP literals; production operators will try `localhost`.

**Fix:** Accept only loopback IP literals (`127.0.0.0/8`, `[::1]`) for `http`, and make a hostname with `http` a startup error that names `--lrs-url`. Or, if names are allowed, require every resolved address to be loopback at each connect.

---

## F-13 (minor): A step-1 index-append failure returns `write_failed` without `included: true`, even though the tile-10 record was kept

**Where:** §5.5 "Step 1 order is: write the tile-10 record, then append the index line. If the index append fails after the tile-10 write, the response is `500` `write_failed` and the record is kept."; error catalogue `write_failed` row; AT-158; compare AT-212–AT-214, which do carry `included: true`.

**Why this is a problem:** The client can't tell this case from a step-1 failure where nothing was written (AT-89). The spec also treats the whole of step 1 as failed ("skip 2, 2b, and 3"), so no LRS statement is ever sent for an inclusion that exists. A later select writes a duplicate tile-10 and does emit one. That outcome is defensible, but it's implicit.

**Fix:** State the body explicitly. Either include `included: true` (no `lrs_status`, since step 3 is skipped) for the post-tile-10 index failure, or state that the body carries no `included` field and that the duplicate-on-retry behavior is the accepted outcome (Risk 10). Assert the chosen body in AT-158.

---

### Blocker / major titles (for the parent)

1. **F-01 (major):** A same-state `POST {"state":"off"}` runs the revoke-receipt latch updates but never decrements `revokes_in_flight`, which blocks consent `on` for the rest of the process.
2. **F-02 (major):** AT-187 requires a `--snapshot-dir` temp to exist mid-fetch, but step 2 creates that temp only after the GET returns.
3. **F-03 (major):** Opens that run under the write mutex have no `O_NONBLOCK` or regular-file validation, so a FIFO hangs every select, download, and consent call.
