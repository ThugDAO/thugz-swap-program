# Final review — what a fresh session should do

Read this first. It exists so the next review does not re-derive what six passes already
established, and spends its effort where the design is actually weakest.

**Nothing is written. Nothing is deployed.** Seven documents on `main` specify an Anchor
program that does not exist yet. This is the last review before Phase 1.

---

## Read in this order

| | |
|---|---|
| 1 | `SWAP_SPEC.md` — the specification |
| 2 | `IMPLEMENTATION_APPENDIX.md` — **part of the spec**; wins where they disagree |
| 3 | `IMPLEMENTATION_PLAN.md` — phases 0–13 plus 3b, with gates |
| 4 | `TEST_PLAN.md` — six levels |
| 5 | `AUDIT_BRIEF.md` — what the three external reviewers are being asked |
| — | `SWAP_PLAN_REVIEW.md` — **HISTORICAL**, contradicts current spec, do not implement from it |

---

## What six passes already found — do not re-find these

| Pass | Found |
|---|---|
| Grok #1 | Custody hole: HxwZ retired before it had to sign deposits, making the sequence impossible |
| PR review | `deposit_bird` unspecified; `recover` in prose not constraints; unfunded pool; swap never measured |
| Canvas (adversarial) | A count is not a commitment; sweep must be three-way and injective; no souvenir merkle root |
| `solana-dev` skill | Toolchain pinned to an untested pairing; `transfer_checked`; testing stack outdated; transaction v1 |
| Codex | **`Pool` cannot pay `create_account`**; **`deposit_bird` never created the vault ATA**; rent ~5× low |
| Grok #2 | Finding 4 wasn't actually fixed; payer language split across the same PR |
| Compliance workflow | Royalties undocumented; pairing already public on Arweave; sweep had no independent witness |

---

## Where to spend the review

### 1. `fix_mapping` — still the softest thing here

It is the only place a constraint was loosened rather than tightened, and its safety argument
has now been wrong **twice**:

- v1: "grants no authority, admin already writes mappings" — false, conflated the mapping
  table with custody of tokens.
- v2: "keeping HxwZ warm closes it" — false, HxwZ is not a signer on `fix_mapping`.

v3 is: **the destination is HxwZ's ATA, never admin's.** Admin can undo a bad deposit; admin
cannot pocket. Attack that. Two wrong answers in a row is not a good record.

### 2. The treasury PDA

New since the last full review, and load-bearing for every rent payment. System-owned, zero
data, signs as payer via `invoke_signed`. Never `init`/`allocate`/`assign` — the first
`assign` kills the desk permanently.

Is the shape right? Does the signer bit really carry into the ATA program's inner
`create_account`? What happens if someone sends it junk?

### 3. The sweep, now that it has an Arweave cross-check

The most safety-critical off-chain code, and still unwritten. It now cross-checks the pairing
against `provenance.original_mint` in each remint's Arweave metadata — the one record the
admin cannot rewrite.

Is that check sufficient? Does it fail closed on a fetch error? Is there any path where a
corrupted `claim_map_all.json` still produces a passing sweep?

### 4. The 733-bird pipeline gap

Phase 1's 541 birds went through `build_remint_manifest.py` with ten cross-checks including a
supply-based burn test. Phase 2's **733 birds came from a different pipeline** with none of
them.

That is 57% of the redemption set with less verification behind it. The Arweave cross-check
partly compensates. Decide whether it fully does, or whether those 733 need their own pass
before deposit.

### 5. Compute units

Estimated, never measured. A first-time swap is ~75–90k CU; two batched is ~170k against a
200k default, so batching hits the compute ceiling before the byte ceiling. Nothing downstream
depends on the estimate being right, but the frontend does.

---

## Settled — do not reopen without new evidence

PDA over merkle. Two-year `unlock_ts` (the operator has evidence the reviewers did not: the
previous round ran **over three years** with active outreach). Upgrade authority stays on
`birdAyQ…` with a revisit trigger. Originals locked, not burned. Verify-upfront. Seal before
any swap, no pilot exception. Admin on a new wallet, not HxwZ. Pause by admin alone.

---

## Two process failures worth carrying forward

**A wrong number became consensus.** "The last round ran two years" was ours, wrong, and cited
back by two independent reviewers as grounds to extend `unlock_ts` to four. It nearly became
an immutable on-chain value on the strength of apparent agreement. **Verify factual claims in
these documents against the chain — several are one RPC call.**

**Two copies of a truth always drift.** The merkle draft, the HTML spec artifact, the setup
table, the payer language — four times, same failure, every one caught by a person asking
rather than by a process. The appendix is now formally part of the spec with a stated
precedence order. Watch for the fifth.

---

## Tools available

Six skills are installed and were used in producing the current state:

| | |
|---|---|
| `solana-dev` | Foundation: toolchain matrix, Anchor patterns, security, testing |
| `safe-solana-builder` | 31 rules for hardening AI-written Solana code |
| `tob-spec-to-code` | Code against the doc that specifies it — **run this again once code exists** |
| `tob-solana-vuln-scanner` | Six critical Solana vulnerability classes |
| `tob-property-testing` | `proptest` — Test Plan Level 2 |
| `metaplex` | Token Metadata and collection verification — Phase 8 |

Codex is authenticated (`~/.codex/auth.json`, ChatGPT OAuth) and found the two fatal issues.
It is worth another pass on the merged state.

---

## Exit criteria

Phase 1 starts when:

- [x] `fix_mapping`'s third safety argument survives a hostile read — 2026-08-26. The read
      found the argument was unenforceable as written (no field, no constant named HxwZ);
      closed with the compiled-in `CUSTODIAN` constant
- [x] Treasury construction confirmed against real Anchor/Solana behaviour — 2026-08-26
- [x] The sweep spec has no known path to a false pass — 2026-08-26, after closing the
      substituted-pair pass (verified-collection set anchor) and adding the name match
- [x] A decision on the 733-bird pipeline gap — 2026-08-26: name-match verification in the
      sweep and in a Phase 9 pre-flight; human image sample in Phase 4
- [ ] The three external reviewers have submitted — **independently, before comparing**
- [x] No document contradicts another on payer, sequence, or numbers — 2026-08-26, after
      fixing the fifth drift (below)

---

## Final pass — 2026-08-26

Run by a fresh session against the merged state. Findings, all applied to the docs:

1. **The fifth drift arrived on schedule.** The v3 `fix_mapping` fix (destination = HxwZ)
   had not propagated: spec §4a's own correction table, the plan's correction section, the
   Phase 4 rehearsal, the sweep-and-seal phase and two test-plan rows all still said
   "remint to admin's ATA, single-signer re-deposit, 5 per tx." All rewritten to v3:
   destination HxwZ, re-deposit is admin + HxwZ at 4 per tx.
2. **Cold storage: removed entirely** (operator decision, 2026-08-26 — HxwZ key access is
   retained for as long as the desk needs it). The review had independently found a
   sequence hazard here: the plan parked the key offline *before* the sweep, but under v3
   a sweep mismatch is corrected by re-deposit, which needs HxwZ's signature. The removal
   dissolves it. The plan drops the phase (now 14 phases, 0–13) and the spec's setup
   sequence drops the step. One survivor as a hygiene rule: no scheduled job or automation
   ever signs with HxwZ.
3. **v3 had no enforcement mechanism.** "Destination is HxwZ's ATA" appeared nowhere the
   program could check — `Pool` has no HxwZ field and no constant existed. Specified as a
   compiled-in `CUSTODIAN` constant (an init parameter was considered and rejected: admin
   could mis-set it to admin). It also pins the `deposit_bird` source and the `recover`
   destination, which was previously unspecified. New `NotCustodian` error, new test rows.
4. **The sweep had a false pass.** Drop a real pair from `claim_map_all.json` and
   substitute a fake one — fake remint, forged provenance tag — and every check passed,
   because nothing asked *which* remints are legitimate. Closed by asserting the
   `new_mint` set equals the chain-derived verified-collection membership, plus a strict
   `arweave.net/<txid>` URI check and explicit HTTP fail-closed.
5. **The 733 gap: the Arweave check does *not* fully compensate.** A mint-time mis-pair
   corrupts tag and claim map together, from the same bug, and the Arweave check passes.
   The name match closes it: the original's on-chain name (immutable since 2021,
   independent of both pipelines) must equal the remint's. Runs in the sweep and in a
   Phase 9 pre-flight, before anything irreversible. Residual (right name, wrong image):
   ≥30-bird human sample in Phase 4, weighted toward the 733.
6. **Treasury confirmed structurally sound.** Signer-privilege extension carries a PDA
   payer through the ATA program's inner `create_account`; an outsider cannot `assign` or
   `allocate` a PDA that has no private key, so external junk is limited to donated
   lamports. One trap documented (appendix §4): Anchor `init` constraints do not sign for
   a PDA *payer* — every ATA creation must be an explicit `create_idempotent` CPI via
   `new_with_signer`.
7. **Payer language, again.** Two "rent paid by pool" comments in the `swap` pseudocode,
   "pool balance" monitoring in spec §5, the Phase 0 funding row, two test-plan rows and
   audit brief §6 all said pool where the model says treasury. All fixed.
8. **The audit brief briefed the wrong argument.** §1 asked reviewers to attack the v1
   safety argument — already known false. Rewritten to v3 with the failure history.
9. Compute units: left as estimates by design; Level 3 measures them. No change.
10. Minor: appendix §7 claimed events were missing from the spec (stale); plan Phase 1
    duplicate step numbering; reversibility map said upgrade authority live "until step
    13" (it is deferred indefinitely); test-plan header mislabeled Levels 3–4 as devnet
    mocks and the test-data rule contradicted the Surfpool rehearsal.

All rent figures, account sizes and transaction counts were re-derived this pass and check
out: Mapping 82 B / 0.001462 SOL, Pool 91 B (custodian is a constant, so no layout change),
ATA 0.002039 SOL, 1.86 + 2.60 + 5.20 = 9.66 SOL, 4.46 before deposits, 319 txs at 4/tx,
255 at 5/tx, watermark ≈ 12 swaps.

**Remaining before Phase 1: the three external reviews, on these updated documents.**

---

## Codex second pass + data verification — 2026-08-27

Codex (fresh session, high effort, 1.0M tokens) reviewed the merged post-final-pass state:
12 findings, all applied same day. The two P1s were both about sweep *independence*:

1. **The collection-membership anchor was circular** — Phase 8 creates membership from the
   claim map, so membership agreeing with the map proved nothing. Fixed: Phase 8 is now
   gated on the Arweave-anchored pre-flight, and the docs say which checks carry the
   independence (appendix §9 "What independent actually rests on").
2. **On-chain names are rewritable** (remints mutable, HxwZ is update authority), so the
   name match now binds through the `name` inside the frozen Arweave JSON, and the sweep
   records `is_mutable` + update authority.

Also applied: spec §8 sequence rewritten to match the plan (it had omitted the
external-review gate — a builder following it could have initialized before review), with
"the plan wins" precedence stated; the `Original-Mint` tag is now actually checked (it had
only ever been asserted); a freeze-authority assertion closes the frozen-destination-ATA
edge; `EXPECTED` joins `CUSTODIAN` as a compiled constant so init cannot take a wrong count
from instruction data; the deposit transfer authority must BE the custodian signing, not a
delegate; `Pool.recovered` (u16, Pool now 93 bytes) keeps the vault reconciliation true
after post-unlock recovery; the sixth payer drift ("Pool balance" in the post-launch
table); the phase count reads "0–13 plus 3b"; two test-plan rows contradicting `recover`
fixed; the claim map's stale `_meta.note` (claim-time verification) is flagged in §10
rather than edited — the file stays frozen.

**The data itself was verified against mainnet + Arweave the same day**
(`verification/preflight_check.py`, report committed alongside): all 1,274 pairs
structurally sound and injective, every remint present and custodian-held, **zero name
mismatches across all 1,274** — the phase-2 (733) pipeline gap is now measured, not
argued — and Arweave provenance confirmed for every pair. The remints are confirmed
ungrouped (Phase 8 not yet run), matching the plan.

---

## Phase 5 findings log

### Review 1 of 3 — received 2026-08-27 (against `3c2f000`)

Verdict: **no P1**; 1 P2; 8 P3; all six brief questions hold (sweep = "mostly").
Every factual claim spot-verified against the repo before logging. Fixes are HELD
until all three reviews are pooled (one batch, one Phase 3+4 rerun).

| ID | Finding | Triage |
|---|---|---|
| P2 | `MIN_LOCK_SECONDS` = 1 year; spec says `unlock_ts` = "opening + 2 years, immutable". A 366-day init is legal and opens `recover` a year early (still HxwZ-only). | **Confirmed divergence.** Operator decision: pin 2y in program, or spec becomes "≥1y, operator sets ~2". Code change either way → 3+4 rerun. |
| P3-1 | Mapping is 83 B; spec rent table, README, `state.rs` doc comment still say 82 (8+74). **Seventh doc drift.** | Confirmed. Doc/comment sync queued. Rent table +0.009 SOL total. |
| P3-2 | `swap` doesn't pre-check `mapping.recovered` / vault amount; post-recover swap fails on the transfer, tx reverts, holder keeps original. | Correct as designed (atomicity). Frontend gets a friendly mapping for this error. No code change. |
| P3-3 | `DuplicateAccount` error defined, never used. | Confirmed. Remove or document as reserved — cleanup batch. |
| P3-4 | `Interface<TokenInterface>` admits Token-2022; sweep §10 pins Tokenkeg. | Accepted: a 2022 remint requires admin+custodian depositing one AND skipping the sweep; sweep is the guard. Note added to spec queue. |
| P3-5 | Sweep §6b: empty DAS response ⇒ note, not failure — "no foreign members" silently unenforced on outage. | **Good catch.** Queued: fail when target is mainnet and DAS members == 0 or verified-remint count ≠ 1274. |
| P3-6 | Sweep originals allowlist (`thugbird_mints.json`, 3,318) is outside the repo and unpinned. | **Good catch.** Queued: vendor into `verification/` + pin sha256 in sweep; fail closed if missing. |
| P3-7 | Sweep §4 doesn't check the SPL owner field == vault PDA (address is canonical ATA, so belt-and-braces). | Queued, trivial. |
| P3-8 | No treasury withdraw; leftover SOL stays in the PDA when the desk dies. | Operator decision (Phase 13 territory). Stuck lamports, not stuck birds. |
| Brief-a | Live pages.dev desk is test-keys bytecode (EXPECTED=20, fixture keys) — not a review of the mainnet artifact. | Brief fixed immediately (packet accuracy for reviewers 2–3). |
| Brief-b | Two CU numbers in the packet (86,661 LiteSVM vs 73,518 Surfpool) with no source pinned — the "two-year consensus" failure shape. | Brief fixed immediately: both numbers, sources pinned, budget to the higher. |
| Brief-c | Matrix log ran on throwaway id `2jvGw7y…`; brief/page point at `BGMFnk…`. | Brief fixed immediately. |

### Review 2 of 3 — received 2026-08-27 ("Hermes", static source review against mirror `66d189d`)

Verdict: **0 critical/high code defects**; 1 High (trust/design, disclosed), 2 Medium
(design-accepted), 4 Low, 4 Informational. All six brief targets held line-by-line;
sweep byte offsets independently re-derived against the Anchor structs and confirmed.

| ID | Finding | Triage |
|---|---|---|
| H-1 | Live upgrade authority `birdAyQ…` conditions every invariant; disclosed. Recommends: desk UI shows machine-read authority status; consider Squads BEFORE launch (after-launch handoff needs the same key's cooperation). | Trust position, known. **Operator decision queued: Squads timing.** UI item → Phase 12 list. |
| M-1 | Seal is arity-only; sweep is load-bearing (verified fail-closed, offsets correct, anchors independent). Recommends versioned witness artifacts + pinned hashes in the published report. | Sweep already emits claim_map + report sha256; witness-pinning matches R1 P3-6 (vendor + hash the allowlist). Queued. |
| M-2 | 1-year floor vs promised 2 years; recommends raise floor or render `unlock_ts` from chain in the UI. | **CONVERGES with R1 P2** — independently double-flagged. |
| L-1 | Token-2022 deposit accepted on-chain; sweep-guarded only. Recommends on-chain legacy-SPL owner assert in `deposit_bird`. | **CONVERGES with R1 P3-4**, with a sharper fix. Queued for pool decision. |
| L-2 | Post-recover swap fails with opaque token error; recommends `require!(!mapping.recovered)` in `swap`. | **CONVERGES with R1 P3-2**, upgraded from "frontend copy" to a 1-line program fix. Queued. |
| L-3 | `state.rs` Mapping comment 82 vs actual 83. | **CONVERGES with R1 P3-1** (subset). Queued. |
| L-4 | `swap` lacks explicit `vault_new_ata.amount == 1` (only asymmetry in the check pattern). | Confirmed against source. Queued, trivial. |
| I-1..I-4 | pool.collection sweep-enforced; 0-decimal fungible deposit sweep-rejected; off-curve holder acceptance disposition endorsed; per-deposit bump search CU note. | No action. |

**Independent convergences after two reviews** (the brief's pooled-weight rule):
unlock floor (R1-P2 = R2-M2) · Token-2022 guard (R1-P3-4 = R2-L1) · recovered-check in
swap (R1-P3-2 = R2-L2) · Mapping 82→83 drift (R1-P3-1 = R2-L3). Four double-flags, zero
contradictions between reviews so far.

### Operator decisions — 2026-08-27 (pooled-fix batch, lands after review 3)

1. **Unlock floor: pin 2 years.** `MIN_LOCK_SECONDS` becomes 2 × 365 days on mainnet
   builds (test-keys keeps 60 s). Resolves R1-P2 / R2-M2 by enforcement, not prose.
2. **Upgrade authority: keep the live key + published revisit trigger.** Squads handoff
   after 20 public swaps + 30 quiet days. Frontend renders the live authority status
   from chain (Phase 12 item) so the posture is machine-checkable meanwhile.
3. **Treasury: no withdraw instruction.** Fund lean instead — Phase 7 funding computed
   from live `getMinimumBalanceForRentExemption` (rent-reduction gates pending), topped
   up operationally. Leftover lamports are accepted as the cost of "no instruction
   delivers value to admin".
4. **Deposit run: legacy 4/tx regardless of the tx-v1 gate.** Phase 9 note amended from
   "re-measure" to "check the gate for awareness, proceed legacy". Toolchain pin stands.

Ratified with the batch: deposit_bird legacy-SPL owner assert (T22), swap
`!mapping.recovered` + `vault_new_ata.amount == 1`, sweep hardenings (empty-DAS failure
on mainnet target, vendored+sha-pinned allowlist, §4 owner field), doc syncs (Mapping
83 B ×3, DuplicateAccount removed), frontend queue (blockhash retry, treasury-drained
copy, chain-rendered unlock_ts + authority status).

### Review 3 of 3 — received 2026-08-27 (independent third pass against mirror `66d189d`)

Verdict: **no P1, no P2.** The deepest pass: ran Level 1 (26/26) + Level 2 + artifact
guard independently; re-derived the sweep PDA self-test from scratch; scanned ALL 1,274
old mints on mainnet (supply, mint authority, freeze authority); hand-verified 5 random
Arweave provenance/tag records; read the sweep line-by-line and endorsed the fail-closed
structure.

| ID | Finding | Triage |
|---|---|---|
| P3-a | Sweep immutable cache trusted across runs — a poisoned earlier local run would be trusted by the sealing run. | **Good catch.** Batch: `--no-cache` flag + mainnet sealing runbook mandates it. |
| P3-b | 6b allowlist `thugbird_mints.json` not in the repo; sweep crashes if absent. | **THIRD flag (R1-P3-6, R3).** Batch: vendor + sha-pin. |
| P3-c | §11 art_sha256 shares pipeline with images — detects gateway drift, not mint-time wrong image; human sample non-optional. | Concurs with spec's own model; sample already done 31/31, logged. |
| Disclosure-1 | **Four originals burned (supply 0): THUG #1370, #1400, #1407, #3074** — their remints unclaimable by design; sit in vault until unlock_ts then recoverable. **Independently re-verified 2026-08-27: full 1,274-mint scan finds exactly these four.** | Batch: operator notes + desk FAQ + spec line; ~1,270 is the true max swap count. |
| Disclosure-2 | All 1,274 old-mint mint authorities immutable (973 none / 301 own-edition / 0 foreign) — the fresh-unit swap-theft path is closed by the data; currently unclaimed in the spec. | Batch: add the strengthening line to spec. |
| Disclosure-3 | Old-mint freeze authorities: 0 foreign (973 none / 301 own-edition). | Recorded; extends the remint-side §10 fact to originals. |
| P4 | Two CU figures (Surfpool 73,518 vs LiteSVM 86,661) unlabeled in SECURITY_CHECKLIST. | Batch: label contexts (brief already fixed). |
| Framing | fix_mapping v3 limits *admin-key* compromise, not *operator* compromise (admin+custodian are the same operator) — "admin cannot pocket" reads stronger than it is. No code change (HxwZ co-sign equivalent trust, worse ops). | Batch: precise wording in desk copy/spec. |

### GATE 5 — POOLED VERDICT (all three reviews in, 2026-08-27)

**No P1 from any reviewer. No P2 from reviews 2–3; review 1's single P2 (unlock floor)
resolved by operator decision (pin 2 years).** Zero contradictions across the three.
Convergences: unlock floor (R1+R2), Token-2022 assert (R1+R2), swap recovered-check
(R1+R2), Mapping 83 B drift (R1+R2), allowlist vendoring (R1+R3), CU labeling (R1+R3).
All three independently endorse: fix_mapping v3, the seal gate, mint provenance, recover,
treasury flow, and the sweep's fail-closed design.

Gate 5 exit: land the single pooled fix batch (specified above + operator decisions),
rerun Phases 3+4, then Phase 6 is at operator discretion and Phase 7+ is unblocked.
