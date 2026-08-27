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
| 3 | `IMPLEMENTATION_PLAN.md` — 14 phases with gates |
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
