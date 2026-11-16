# Goldfish review: s3a-kite spec v10, round 10

Reviewer: fresh, independent. I read only `/workspace/goldfish/s3a-kite.md` (1242 lines) and no other file, review, wiki, or repo.

**Summary: 0 blocker / 5 major / 17 minor (22 findings).**

This is not a clean round: there are 5 majors.

---

## Majors

### F-01 (major): a concurrent `on` and `off` consent request can leave the in-memory flag `on` after a revoke has been persisted

- **Where:** §5.2.1 "Consent revoke and in-flight LRS (DD-102, DD-113, DD-138)"; §5.2.3; DD-102; DD-113.
- **Quote:** "`POST /v1/consent` `{"state":"off"}` sets an atomic in-memory consent flag to `off` **before** waiting for the write mutex … Under the mutex the host then persists the revoke". §5.2.3: "GET and POST return the host's **in-memory** consent state".
- **Problem:** The spec says when revoke sets the flag, but not when `POST {"state":"on"}` sets it. Take this ordering:
  1. The `on` request holds the mutex and writes the file as `on`.
  2. The revoke request arrives and sets the flag to `off` before the mutex.
  3. The `on` request then sets the flag to `on` and releases the mutex.
  4. The revoke takes the mutex and persists `off`, but nothing makes it set the flag to `off` again.

  The result is consent file = `off`, `lrs-retry/` deleted, revoke answered `200 off`, and in-memory flag = `on`. LRS sends continue after the user revoked, and `GET /v1/consent` reports `on`. This breaks consent-only xAPI. No AT covers concurrent on/off.
- **Fix:**
  - (a) The revoke sets the flag to `off` again under the mutex, after it persists. The `on` request sets the flag to `on` only under the mutex and only after its file write.
  - (b) Use a monotonically increasing consent generation counter. A request may set the flag only if its generation is still the latest. Add an AT that races `on` against `off` and asserts the final flag is `off` with zero later LRS requests.

### F-02 (major): `lrs_status` for an LRS POST aborted by revoke is `failed` in DD-131 and AT-156, but `skipped` under the DD-134 HTTP-outcome rule

- **Where:** §5.2.1 `/v1/select` row; DD-131; AT-156 (second case) versus §5.5 "HTTP outcomes" and DD-134.
- **Quote:** DD-131 and AT-156 say "`failed`: … or an in-flight POST was aborted by revoke". §5.5 says "No HTTP response (timeout, connection refused, DNS failure, or I/O or reset mid-body) is treated like 5xx: queued if consent is still `on` in the same mutex hold, else `skipped`."
- **Problem:** An aborted POST is a case with no HTTP response, and consent is `off` in the following mutex hold. The DD-134 rule therefore gives `skipped`, while AT-156 asserts `failed`. AT-156's third case (revoke after the POST returns) gives `skipped`, so the two cases look the same to the queue logic. An implementer cannot satisfy both texts.
- **Fix:**
  - (a) Add to §5.5 and DD-134: "An LRS POST aborted because consent became `off` is `failed` (no retry file); this takes precedence over the no-response rule."
  - (b) Make the abort `skipped` and change AT-156 and DD-131 to match.

### F-03 (major): the optional candidate cache, keyed on `(path, mtime, size)`, can keep identities whose research consent has since expired

- **Where:** §5.5 candidate list item 2; DD-110; DD-117; §5.1 `new_with_clock`.
- **Quote:** §5.5 says "an implementation may cache results keyed on `(canonical path, mtime, size)` provided the observable result equals a fresh walk after any of those values change." DD-110 and §5.1 say "Withholding and Edge expiry are evaluated … at each candidate recompute."
- **Problem:** Withholding depends on `now` (`expires_at`), but the cache is invalidated only by file changes. Once a research `ConsentRecord` or a type-71 `expires_at` passes, a cached result still lists the withheld `AcademicPaperRecord` identity and accepts select/download for it. That is a consent leak, and it contradicts DD-110. The current wording allows exactly this.
- **Fix:**
  - (a) Cache only the parse, never the filter result: always re-apply the §5.3 item 4 / §5.4 item 6 filter with the current clock.
  - (b) Add the next relevant `expires_at` boundary to the cache key. Then add a candidate case to AT-161: advance the injected clock past `expires_at` and assert that `GET /v1/candidates` drops the id and select returns `404`.

### F-04 (major): for a sealed hyper-tile with a committed CRC failure, AT-35 and AT-90 disagree on the valid floors

- **Where:** §5.4 "Per-tile outcomes" (CRC bullet); label table rows 2 and 3 for hyper-tile; §5.3 item 6; AT-35; AT-90; AT-185; DD-100; DD-111; AT-165.
- **Quote:** §5.4 says "A CRC failure on a committed tile (hyper-tile or Edge) … That tile and every subsequent tile are not valid floors". §5.3 item 6 and AT-90 say "If `verify_sealed_archive` fails on a file that has a seal trailer … CRC-valid committed tiles **are** listed as valid floors".
- **Problem:** A committed CRC failure in a sealed hyper-tile also makes `verify_sealed_archive` fail, so both label-table rows apply. Take a CRC-valid tile after the bad one. AT-35 says it is not a floor; AT-90 says it is. `quarantine_reason` also differs (`seal_trailer` has higher priority than `crc`). Because `invalid_tile_indices` depends on which rule wins, pack `open` and a mapped reader-core `verify` can disagree, which AT-165 checks.
- **Fix:**
  - (a) State the precedence: "When a committed CRC failure is present, the CRC rule decides the valid floors, even if a trailer is present. AT-90's 'CRC-valid tiles are listed' applies only when every committed tile passes CRC." `quarantine_reason` stays under the existing priority.
  - (b) The reverse precedence. Either way, add a sealed-plus-CRC-failure fixture to AT-35/AT-90 and to the AT-165 list.

### F-05 (major): AT-54's second case cannot be built from the specified interfaces

- **Where:** AT-54; §5.2.4 deny list; §5.2.2 flag inventory (DD-106: "Every host flag is in this table"); DD-140.
- **Quote:** "the configured LRS hostname is resolved to an otherwise-allowed address (a global-unicast test address) and a fetch to that address is still denied because it is the LRS host, even if that IP would be allowed by `egress_test`."
- **Problems:**
  - The host uses the system resolver, and the spec has no flag, feature, or hook to make a hostname resolve to a chosen global-unicast address.
  - `egress_test` exempts only `127.0.0.1` and `::1`, so it can never "allow" a global-unicast IP. The final clause cannot be met.
  - A real global-unicast destination in CI would mean real off-host egress if the deny check were faulty.
- **Fix:**
  - (a) Add an `egress_test`-only resolver override (for example `--egress-test-resolve host=ip`, rejected in release builds like `--egress-test-loopback-port`) to the inventory. Rewrite the case so the LRS host resolves to `127.0.0.1` at the egress-test port, and assert the fetch is still denied because it is the LRS IP.
  - (b) Move this case to a pack unit test of the deny-list function with an injected resolver, and drop the host-level claim.

---

## Minors

### F-06 (minor): `.claimed` retry files left by a crash are never defined

- **Where:** §5.5 "Queue file names"; DD-134; §5.5 "Backoff" paragraph.
- **Quote:** "`{id}.claimed` is an in-flight exclusive claim … a queued outcome renames it back to `{id}.json`". Also: "without dropping a sendable (non-permanent) queue file".
- **Problem:** If the host dies mid-send, `{id}.claimed` stays behind. Startup replay only claims `.json` files, so that consented statement is silently never resent.
- **Fix:** At startup (consent `on`, and after the sidecar lock is held), rename every `*.claimed` back to `*.json` before replay. Add this to AT-91.

### F-07 (minor): the `/v1/open` `name` hint cannot reach `open_path`

- **Where:** §5.2.5 ("The name hint is the optional `/v1/open` `name`, or the path basename"); §5.1 `open_path` signature (no `name_hint`; "Kind detection uses the path's basename"); DD-115 ("The host always calls `open` or `open_path`").
- **Problem:** To honour `name`, the host must read the bytes and call `open`. That drops `paper_identity` (it is `None` for `open`) and re-adds the host-side read that DD-75/DD-139 put in the pack.
- **Fix:**
  - (a) Add `name_hint: Option<String>` to `open_path`.
  - (b) State that `name` is ignored, or rejected with `invalid_param`, on hosts where `open_path` is used.

### F-08 (minor): `Link { href }` does not say whether `href` is raw or resolved

- **Where:** §5.1 `Hit`; §5.2.1 hit-test response; AT-160.
- **Problem:** The text says the pack resolves relative hrefs against the page's parent and strips `#fragment`, but `href` in the hit and in `SemanticNode` could be either the raw or the resolved value. A client that only has the handle cannot resolve it reliably.
- **Fix:** Specify that `href` is the raw attribute. Add a `resolved_path: Option<String>` (absolute, fragment stripped, `None` for http(s)/file://), or state that `href` is already resolved.

### F-09 (minor): hyper-tile signature quarantine reasons cannot be reached

- **Where:** §5.3 item 4 ("A hyper-tile or Edge file is also unauthenticated as a whole when `quarantine_reason` is `signer_not_pinned` or `missing_signature`"); DD-141 "(or hyper-tile)"; §5.4 label table.
- **Problem:** The table has no hyper-tile signature rows. How a hyper-tile gets `FLAG_ED25519_SIGNED` or type-70 signatures, or one of these reasons, is undefined.
- **Fix:** Either add hyper-tile rows (flag set / no signature, signer unpinned) or delete "hyper-tile" from those sentences.

### F-10 (minor): a mid-file torn Edge tile versus the seal chain

- **Where:** §4.7 ("`edge_seal_chain` … stops at the first CRC-failed tile"); §5.4 "Torn or uncommitted … do not by themselves make the file Quarantined"; AT-17.
- **Problem:** It is undefined whether committed tiles after a torn tile are valid floors (the chain stops there) or the torn tile is skipped and the chain continues. `break_index` and `invalid_tile_indices` for AT-165 depend on this.
- **Fix:** Define it: torn tiles are only a tail (later tiles are uncommitted), or the chain skips torn tiles. State the `break_index` value.

### F-11 (minor): the DD-129 check on `--consent-state-file` when the file is missing

- **Where:** §5.2.2 path rule; DD-129; DD-96.
- **Problem:** `realpath` of a missing file fails, but DD-96 allows a missing file. It is unclear whether the parent must exist, be canonicalized, and lie outside request roots.
- **Fix:** "If the file is missing, its parent directory must exist and is canonicalized; the canonical parent joined with the basename is checked against the request roots." Add this case to AT-179.

### F-12 (minor): restarting without `--lrs-url` while `revoke-pending` or `lrs-retry/` is still on disk

- **Where:** §5.2.1 restart paragraph; §5.5 retry queue; §5.5 data-dir rule ("may hold identities only when consent is on").
- **Problem:** With no `--lrs-url`, `--consent-state-file` is forbidden, so the host cannot "retry the consent-file write". Whether it deletes `lrs-retry/` (which may hold identities) is not stated.
- **Fix:** State that a Scholar start without `--lrs-url` deletes `lrs-retry/` and `revoke-pending` (startup error naming `lrs-retry` on failure). Alternatively, make a leftover `revoke-pending` a startup error that requires `--lrs-url`.

### F-13 (minor): DD-131 does not cover the `lrs_status` for a permanent 4xx

- **Where:** DD-131; the §5.2.1 select row; AT-191.
- **Quote:** "`failed`: attempt failed and no retry file could be written". A permanent 4xx does write a `.permanent` file, and AT-191 asserts `failed`.
- **Fix:** Add "or the outcome was permanent (`{id}.permanent` written)" to the `failed` definition.

### F-14 (minor): AT-150 asserts a field the response does not have

- **Where:** AT-150; the §5.2.1 select/download response schemas.
- **Quote:** "Select and download of a candidate … return `on_disk: false`".
- **Problem:** Select and download responses have no `on_disk` field; only `/v1/candidates` does.
- **Fix:** Assert `on_disk: false` in `GET /v1/candidates`, and `path`/`download_error` in the download response.

### F-15 (minor): the dossier size-cap rule is in the wrong item

- **Where:** §5.5 candidate item 1 versus item 2; AT-188.
- **Problem:** "A file larger than `--max-open-bytes` is skipped" and the YAML-omission sentence are both inside item 2 (the `*.s3a` walk). AT-188 applies the size cap to dossiers from item 1.
- **Fix:** Move both sentences to item 1, or state that they apply to both items.

### F-16 (minor): AT-181 describes a timestamp case that cannot happen

- **Quote:** "with the timestamp of that successful startup write (or of `revoke-pending` until the consent file is rewritten)".
- **Problem:** §5.2.1 makes a failed startup consent write a startup error, so a running host has always rewritten the file.
- **Fix:** Delete the parenthetical.

### F-17 (minor): the §8 preamble and DD-66 are out of date

- **Problems:**
  - The §8 preamble says "DD-01–DD-137 keep their topics … DD-138 is a decision by Ashley", but DD-139–DD-141 exist (the v10 history says DD-01–DD-141).
  - DD-66 says "A failed revoke delete is a visible error and the queue is not sendable", which describes a degraded in-process state that DD-138 removed (the host exits).
- **Fix:** Update the range and reword DD-66 to refer to DD-138.

### F-18 (minor): phone export lists disagree on `KiteVerificationReport`

- **Where:** §5.1 (line "v1 UniFFI on the phone exports …"), DD-45, and DD-109 omit `KiteVerificationReport`. §5.7, AT-99, and §11 step 6 include it.
- **Problem:** AT-99 fails a binding that lacks the type.
- **Fix:** Add the type to §5.1, DD-45, and DD-109.

### F-19 (minor): the phone has no stated source for `max_frame_pixels` and `max_open_bytes`

- **Where:** §5.1 (both are required on every `open*`); §0 "No … invented runtime defaults"; §5.7.
- **Problem:** The host gets these from flags. The phone app has none, so it must invent constants, which the no-defaults rule forbids.
- **Fix:** Add a DD with draft phone values, or an open question. Alternatively, state that these are app-build constants, outside the no-defaults rule.

### F-20 (minor): DD-135's "unless `--behind-proxy`" clause is vacuous and confusing

- **Quote:** "A non-loopback, non-Unix bind still requires TLS and `--headless` unless `--behind-proxy` (which remains loopback/Unix only)".
- **Problem:** `--behind-proxy` with a non-loopback bind is itself a startup error (AT-87), so the clause never applies.
- **Fix:** Drop the "unless" clause.

### F-21 (minor): AT-83 refers to a "second select" it never performs

- **Quote:** "`POST /v1/select` then `POST /v1/download` … The second select returns `200` with the existing snapshot path."
- **Problem:** The sequence has no second select. Whether the download response's `snapshot` should be the existing path is unclear.
- **Fix:** "The download response's `snapshot` is the existing snapshot path."

### F-22 (minor): AT-74 does not give an error code

- **Quote:** "A running Scholar host whose working-archive path cannot be created or opened fails the select closed and writes nothing."
- **Problem:** No code is given, while DD-83 says the ATs assert catalogue codes.
- **Fix:** Name `500` `write_failed`, or `403` `unreadable` for EACCES.

---

## Blocker / major titles

- F-01 (major): a concurrent `on`/`off` consent race can leave the in-memory flag `on` after the revoke is persisted.
- F-02 (major): `lrs_status` for a POST aborted by revoke is `failed` (DD-131, AT-156) versus `skipped` (the DD-134 no-response rule).
- F-03 (major): the candidate cache keyed on `(path, mtime, size)` ignores clock-based consent expiry (DD-110).
- F-04 (major): for a sealed hyper-tile with a committed CRC failure, the valid floors conflict between AT-35 and AT-90.
- F-05 (major): AT-54's second case cannot be built (no resolver hook; `egress_test` cannot allow a global-unicast IP).
