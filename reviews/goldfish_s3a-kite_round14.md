# Goldfish review — s3a-kite spec v14 (round 14)

Reviewer: fresh, independent read of `/workspace/goldfish/s3a-kite.md` only (1,314 lines). No other files were consulted. Line numbers (`L###`) refer to that file as reviewed on 2026-11-16.

**Summary: 0 blocker / 5 major / 15 minor (20 findings).**

This round is **not clean**: there are 5 majors.

Major titles:
- F-01 AT-183 contradicts §5.2.4 / DD-133 on `host:80` vs `https://host:80/`
- F-02 Pack-side derivation of `quarantine_reason`, `seal_status`, and `break_index` is undefined for compound causes; `chain_break` priority makes `signature_known_signer` unreachable (contradicts AT-201 (2)/(7))
- F-03 Edge tiles after the last signed `tile_index` have undefined authentication (an appended grant can release consent)
- F-04 Edge type-71 "denial" vs "does not speak to the research purpose" contradict each other in the only normative consent table
- F-05 Interface gap: `GET /v1/candidates` exposes no path, so a client cannot open a candidate's dossier page (and so cannot reach the Select control)

---

## Major

### F-01 (major) — AT-183 contradicts §5.2.4 / DD-133 on `host:PORT` matching

**Where:** §5.2.4 bullet 3 (L438), DD-133 (L1223), AT-183 (L1046).

**Conflicting text:**
- §5.2.4: "An entry `host:PORT` matches **either scheme** at that effective port." DD-133 says the same.
- AT-183: "An allowlist entry `host:80` matches `http://host/` and `http://host:80/`, and **not `https://host/` or `https://host:80/`**."

**Why it matters:** `https://host:80/` has scheme `https` and effective port 80. The normative rule says `host:80` matches either scheme at port 80, so it matches. AT-183 says it does not. An implementer has to break either the AT or the normative rule. The `host:443` half of AT-183 (`http://host:443/` matches) follows the "either scheme" rule, so the AT also contradicts itself.

**Fix (pick one):**
- (a) Keep the "either scheme" rule, and change AT-183 to say that `host:80` **matches** `https://host:80/` and does not match `https://host/`.
- (b) Make port entries scheme-specific (for example, `host:80` matches only `http`). Then update §5.2.4, DD-133, and the `host:443` → `http://host:443/` clause of AT-183 to match.

---

### F-02 (major) — Pack-side reason, seal status, and break index are undefined for compound causes; the priority order hides `signature_known_signer`

**Where:** §5.1 mapping table rows `break_index` / `quarantine_reason` (L222, L224) and the priority paragraph (L227); §5.4 item 2 bullets on torn tiles, chain breaks, and known-signer failures (L544, L546, L547); label-table rows L560 and L564; AT-17 (L849); AT-165 (L1015); AT-201 (2) and (7) (L1069); DD-100; DD-111; DD-115.

**Conflicting text:**
- L546: "Any **seal-chain break** (`SealedChainBreak`, reordered tiles, **a mid-chain signature that does not verify**, or a committed CRC that breaks the chain) makes the file-level label Quarantined and `seal_status` `seal_failed`." Row L560 says the same.
- L227: the priority order is "`signer_not_pinned` > `missing_signature` > `seal_trailer` > **`chain_break` > `signature_known_signer`** > `crc`."
- AT-201 (2) and (7): a record that fails every key after an earlier authentic record gives "`quarantine_reason: signature_known_signer`".
- L544: "`break_index` is the index of that first torn or CRC-failed tile when the chain stops there". Torn tiles "do **not** by themselves make the file Quarantined" (AT-17).

**Why it matters:** The spec never says, for pack `open`, which conditions make each `QuarantineReason` applicable. It gives only a priority order and reader-side mapping hints (L224). As written, a failed signature is a "seal-chain break", so both `chain_break` and `signature_known_signer` apply, and the priority order picks `chain_break`. That makes `signature_known_signer` unreachable, which contradicts AT-201 (2)/(7) and the AT-165 agreement. Similar gaps:
- An Edge committed CRC failure with an active chain: is the reason `chain_break` or `crc`?
- A torn tile on an active chain (MICRO-VEC-02): `break_index` is set and the chain "stops", but the file is not quarantined. Is `seal_status` `sealed` or `seal_failed`? AT-165 compares `seal_status`, so the pack and the reader mapping must give the same answer.
- `break_index` on a hyper-tile with a committed CRC failure: L544 defines `break_index` only for Edge, but AT-165 compares it for the sealed hyper-tile CRC fixture.
- `break_index` on a `signature_known_signer` failure: L547 says "the banner reports the break index", but the mapping row L222 derives `break_index` only from `SealInfo`/`checks` "where the seal chain breaks". If the reader does not record a break for a signature-only failure, the mapped `invalid_tile_indices` will miss the tiles from `tile_index` onward.

**Fix:** Add one normative "reason derivation" table in §5.4 for pack `open`, giving `status`, `seal_status`, `quarantine_reason`, and `break_index` for each case: torn-only on an active chain, torn-only on an inactive chain, Edge CRC with the chain active or inactive, hash mismatch or reorder, known-signer signature failure, unpinned signer, missing signature, hyper-tile trailer failure, and hyper-tile CRC with or without a trailer. Then either:
- (a) remove "a mid-chain signature that does not verify" from the "seal-chain break" definition, so a signature failure gives `signature_known_signer` with `break_index = tile_index` and `seal_status` unchanged; or
- (b) keep it as a chain break, and move `signature_known_signer` above `chain_break` in the priority order (L224, L227, DD-115).

Also state explicitly that a torn-tile stop is **not** a chain break for `seal_status`, and say whether `break_index` is set for hyper-tiles. Make the L222 mapping row match.

---

### F-03 (major) — Edge tiles after the last signed `tile_index` have undefined authentication; this leaves a consent-release hole

**Where:** §5.4 item 2, the Ed25519 provenance paragraph and signer attribution (L522–L540); the label table (L559–L568); §5.3 item 4 "Trusted vs untrusted" and the per-subject decision (L488, L496–L499); DD-145; DD-147.

**Text:** "Each record's `signature` is verified … over that record's 32-byte `seal_hash`. The record's `tile_index` is the tile whose chain hash the signature covers." Nothing says what happens to committed, chain-consistent tiles whose index is **greater than every signed `tile_index`**. Nothing requires every tile, or the last tile, to be covered by a signature.

**Why it matters:** The seal chain is unkeyed SHA-256 (L129). An attacker can append tiles to a pinned, Verified Edge file and recompute the chain hashes without adding a type-70 record. Under the current text those tiles are CRC-valid, chain-intact valid floors of a file that is "not unauthenticated as a whole", so a type-71 grant on them is **trusted**. Example: a file has a signed denial on tile 3; an attacker appends a grant on tile 10. Clause 1 is met because the latest trusted record is the grant. Clause 2 is not triggered because the denial is earlier. The subject's event-log payloads are released. The file label also stays "Verified" while it contains unsigned data. The DD-141 downgrade acceptance covers stripping all signatures, not unsigned tails on a file that is still signed.

**Fix (pick one):**
- (a) In a file with any authentic type-70 record, tiles with index greater than the highest authentic signed `tile_index` are unauthenticated: they are not valid floors, or they are display-only and untrusted for consent. Their grants never release. Report them in the banner and in `invalid_tile_indices`.
- (b) Require the last committed tile to be covered by an authentic signature. Otherwise the file is Quarantined with a new or existing reason, such as `missing_signature` at that index.

Either way, add an AT for an appended-unsigned-tail grant, add it to the AT-165 fixture set, and define the reader-mapping behavior.

---

### F-04 (major) — Edge type-71 "denial" vs "does not speak to the research purpose" contradict each other

**Where:** §5.3 item 4, record classification (L485–L486); §5.4 item 6 (L572); DD-147; AT-23.

**Conflicting text:**
- L485: "A **denial** does not allow that purpose." Every type-71 record whose `allowed_purposes` lacks `PURPOSE_RESEARCH_ANALYTICS` is therefore a research denial.
- L486: "type-71 records that **do not speak to the research purpose** still count as consent records of any kind … but they do not enter the per-subject table as a grant, denial, withdrawal, or expiry."

**Why it matters:** `allowed_purposes` is a bitmask, so a type-71 record either has the research bit or does not. That leaves no type-71 record that "does not speak to" research, unless the implementer invents a rule. The two readings differ on privacy. Consider an earlier trusted grant followed by a later type-71 record allowing only some other purpose:
- If the later record is a denial, the subject is withheld.
- If it is neutral, the subject is shown.

This is the spec's "only normative" consent rule, so it must be unambiguous.

**Fix (pick one):**
- (a) Delete the type-71 clause from L486. Every type-71 record without the research bit is a denial (the privacy-safe reading, consistent with AT-23).
- (b) Define precisely which type-71 records are neutral (for example, a record with a `purpose_scope` field that excludes research), cite the `EdgeConsentRecord` field that decides it, and add an AT for "earlier grant, later neutral record".

---

### F-05 (major) — Interface gap: candidates carry no path, so the client cannot open a candidate's dossier page

**Where:** §5.2.1 `GET /v1/candidates` (L267); §5.5 candidate list (L583–L586); §5.1 `paper_identity` and `Select` (L203, L209); AT-29 (L870); AT-101 (L940); DD-60.

**Text:** The candidates response is `{ "id", "title", "source_url", "on_disk" }`. The Select control is painted only on a page opened by `/v1/open` of the dossier's filesystem path (`paper_identity` is `Some` only for `open_path` of `{root}/papers/SRC-*.md`). AT-29 assumes a client that does "`GET /v1/candidates`, `POST /v1/open`, and `GET /v1/frame` (a candidate page with `view=page`)".

**Why it matters:** The Pass-1 flow is: list candidates, skim the dossier, press Select. That flow needs the client to turn a candidate `id` into a dossier path. The API gives no way to do it:
- The client is not told `--wiki-root` or `--corpus-root`.
- The client cannot know which root wins when both have the file.
- `AcademicPaperRecord` candidates have no page at all.

The native window is defined as a pure Host HTTP API client (DD-60), so it hits the same gap. Implementers will invent a field or an endpoint.

**Fix (pick one):**
- (a) Add `"page_path": <absolute canonical dossier path or null>` to each candidate. It is null for `AcademicPaperRecord` candidates and is the winning-root copy for dossiers. State that `POST /v1/open` of it yields `paper_identity == id`. Extend AT-29 / AT-101 to use it.
- (b) Add `POST /v1/open` `{ "paper_id": … }`, which resolves the identity through the request-time candidate set (404 `unknown_paper` otherwise) and opens the winning dossier with `scholar_roots`.

---

## Minor

### F-06 (minor) — "Known signer" wording contradicts the DD-145 attribution algorithm

**Where:** §5.4 L547 ("the signer key is in the pinned list, or the pin list is empty and the signer is the embedded device key"); label row L564; AT-35 (L890); DD-100; DD-111. Compare L533 / DD-145: "the signer is known only when a key verifies the signature."

**Problem:** A failing signature verifies under no key, so the L547 definition cannot be evaluated. DD-145 instead attributes a failure by whether an **earlier** record verified. Several sections use different definitions of the same term.

**Fix:** Replace the L547, L564, AT-35, DD-100, and DD-111 wording with "a record that fails every candidate key after an earlier authentic record (DD-145 rules 3/4)", or point all of them to DD-145.

### F-07 (minor) — Is `floor` on `view=warehouse` an error or ignored?

**Where:** §5.2.1 `invalid_param` row (L282: "Supplying `floor` on a non-floor view (`manifest`, `header`, `banner`, `page`)"), the error-order paragraph (L276: "Optional … fields that a view does not use … are ignored"), and §5.1 `hit_test` (L202, L211: "`Some(_)` on a non-floor view is `400` `invalid_param`"), DD-125.

**Problem:** `warehouse` is a non-floor view in the FFI rule but is left out of the HTTP list. Under the HTTP text, `floor` on a warehouse request reads as an ignored optional field. The two layers would behave differently.

**Fix:** Add `warehouse` to the L282 list, so `floor` on a warehouse view is `invalid_param`. Alternatively, state that it is ignored on `warehouse` in both layers. Add a case to AT-196.

### F-08 (minor) — Gaps in error precedence

**Where:** §5.2.1 L276 and DD-125; the `/v1/open` errors; the `/v1/select` `lrs_status` values.

**Problems:**
- (i) The global order puts `missing_param` before the route-specific `invalid_view`. The special rule only lets `invalid_view` win over `floor` checks. For an unknown `view` with `w` missing, which code wins?
- (ii) `/v1/open` has several route-specific errors (`path_escape`, `not_found`, `unreadable`, `not_regular_file`, `file_too_large`, `invalid_magic`, `too_many_handles`) with no stated order. Example: a missing path while the handle cap is full.
- (iii) A repeat select on a host without `--lrs-url`: is `lrs_status` `skipped` (step 0 skipped step 3) or `not_configured`?

**Fix:** State the complete order for `/v1/frame`, `/v1/hit-test`, and `/v1/open`, and say whether `not_configured` takes precedence over `skipped`.

### F-09 (minor) — Overlapping or equal `--wiki-root` and `--corpus-root` are undefined

**Where:** §5.2.2 disjointness (L402); §5.5 item 1 and "wiki-root wins" (L584, L586); `paper_identity` (L203); AT-207.

**Problem:** The read roots do not have to be disjoint from each other. If `--corpus-root == --wiki-root`, the "losing corpus copy" in L203 is the same path as the winning copy, so `paper_identity` is undefined. Nesting also makes the recursive `*.s3a` walk and the root used for the path walk ambiguous.

**Fix:** Either (a) require `--wiki-root` and `--corpus-root` to be disjoint (a startup error naming both flags), or (b) define the equal and nested cases explicitly.

### F-10 (minor) — Snapshot size is not bounded by `--max-open-bytes`

**Where:** §5.2.2 inventory (`--max-fetch-bytes`, `--max-open-bytes`); §5.6 DD-103 reuse checks (L654); AT-31 / AT-80 (re-open of a snapshot); AT-82.

**Problem:** A snapshot is about the decoded body size plus tile overhead. If `--max-fetch-bytes` is larger than `--max-open-bytes`, a valid snapshot cannot be re-opened (`file_too_large`), so AT-31/AT-80 fail depending on configuration. The DD-103 reuse-check read also has no cap: it could be unbounded, or `snapshot_invalid` for an oversize file.

**Fix:** Either (a) add a startup rule such as `--max-open-bytes ≥ snapshot_size_bound(--max-fetch-bytes)`, giving the bound formula, or (b) state the read cap for the reuse check and which error an oversize snapshot gets.

### F-11 (minor) — `open_verified` does not validate the supplied report

**Where:** §5.1 `open_verified` row (L192); DD-80.

**Problem:** The spec does not say what happens when:
- `invalid_tile_indices` or `break_index` falls outside `0 .. tiles_total-1`;
- `torn_tile_indices` is not a subset of `invalid_tile_indices`;
- `status: quarantined` comes with `quarantine_reason: None`;
- a report is supplied for bytes that classify as `wiki`, `html`, or `other`.

**Fix:** Define each case as `KiteError` (code `invalid_report`) or as ignored, and add one AT.

### F-12 (minor) — `withheld_count` scope when some tiles are invalid

**Where:** §5.3 item 4 (L501); §5.4 item 6 (L572); AT-194 (L1060), AT-195.

**Problem:** L501 says withheld records on invalid tiles are counted **only when the file has no valid floors**. When valid floors exist, should payloads on invalid tiles (for example, after an Edge chain break) count toward `withheld_count`? AT-194's "withheld_count is 0 **for this reason**" leaves it open.

**Fix:** State that `withheld_count` counts only records on valid floors, except in the no-valid-floors banner case, or state the alternative. Make AT-194 assert an exact value.

### F-13 (minor) — Relative `Link` href resolution is under-specified

**Where:** §5.1 `Link { href }` (L210); AT-160.

**Problem:** Relative hrefs are "resolved absolute filesystem path[s]" with the query string kept. The spec does not say whether `%xx` escapes are decoded (`file://` paths are decoded, per L406). A kept `?q` becomes part of a literal filename, so a later `/v1/open` gives `not_found`.

**Fix:** State that relative hrefs are percent-decoded and that the query is stripped for path resolution. Alternatively, keep both but say so, and add an AT case such as `my%20page.md`.

### F-14 (minor) — 415 rule and media-type parameters

**Where:** §5.2.1 `415` row (L307); AT-171.

**Problem:** It is unclear whether `Content-Type: application/json; charset=utf-8` counts as "not `application/json`". Many clients send it.

**Fix:** Say that the media type is compared case-insensitively with parameters ignored (or only `charset=utf-8` allowed). Add a case to AT-171.

### F-15 (minor) — Kind detection is case-insensitive only for `.md`

**Where:** §5.2.5 items 2–3 (L451–L452); AT-130.

**Problem:** `.md` is "ASCII, case-insensitive", but `.html` / `.htm` have no such note. Is `INDEX.HTML` an `html` file or `other`?

**Fix:** Apply the same case-insensitive rule to `.html` / `.htm`, or state that they are case-sensitive.

### F-16 (minor) — Consent timestamp when the state does not change

**Where:** §5.2.3 (L424: "timestamp is that of the last successful host write or startup read"; L426: the file timestamp is "of the last **change**").

**Problem:** On `POST {"state":"on"}` while already `on`, it is unclear whether the host rewrites the file with a new timestamp. The two sentences disagree on what the timestamp means, and GET's value depends on the answer.

**Fix:** Either define a same-state POST as a no-op that keeps the timestamp, or say that every successful POST rewrites the file and the timestamp. Align L424 and L426.

### F-17 (minor) — Hyper-tile seal-trailer-strip downgrade is not recorded as accepted

**Where:** §5.3 item 4 (L490); DD-141 (Edge only); DD-148.

**Problem:** Stripping the trailer from a sealed hyper-tile turns it from `seal_failed` (unauthenticated as a whole) into `unsealed`, which is trusted, so an attacker can then add a grant. DD-141 records the equivalent downgrade only for Edge signatures and explicitly says it does not cover hyper-tiles.

**Fix:** Extend DD-141 (or add a DD) to accept the hyper-tile trailer-strip downgrade explicitly, since legitimate `open_append` also unseals. Alternatively, define a detection signal.

### F-18 (minor) — Two ATs cannot be set up as written

**Where:** AT-161 (L1006) and AT-122 (L961), compared with DD-110 / DD-140.

**Problems:**
- AT-161 asserts candidate-list behavior "after the clock advance". Candidate recompute is a host behavior, and the host uses only the process clock, with no injection.
- AT-122 needs a hostname whose "DNS is an IPv4-mapped RFC1918 address", but DD-140 forbids a resolver override.

**Fix:**
- AT-161: limit it to pack-level assertions, or use real expiry times with a short sleep.
- AT-122: use a literal URL (`http://[::ffff:10.0.0.1]/`), or allow an `egress_test`-only resolver hook.

### F-19 (minor) — Stale or incomplete AT text

**Where:** §7 harness paragraph (L805); AT-96 (L931); AT-179 (L1042).

**Problems:**
- L805 says AT-46's second host uses a distinct `--working-archive` and `--data-dir` only. With shared snapshot or download dirs, B fails at startup (AT-164). AT-46 itself (L875) requires all four dirs to be distinct.
- AT-96's export list leaves out `verification_report` and `KiteContext::new_with_clock`.
- AT-179's Scholar list leaves out `--tls-cert`, which DD-129 requires.

**Fix:** Align L805 with AT-46 (all four dirs), add the missing exports to AT-96, and add `--tls-cert` to AT-179.

### F-20 (minor) — `hit_test` `x` / `y` type and range are unspecified

**Where:** §5.1 `hit_test` (L202); §5.2.1 `/v1/hit-test` (L266) and `invalid_param` (L282).

**Problem:** The spec does not say whether `x`/`y` are integers or floats (needed at `device_pixel_ratio` ≠ 1), or whether negative or out-of-frame values are `invalid_param` or `Hit::None`. The FFI and HTTP layers could diverge.

**Fix:** Define the type (for example, a finite `f64` in CSS px). Say that out-of-frame points return `None` and that non-numeric or non-finite values are `invalid_param`. Add the case to AT-171.
