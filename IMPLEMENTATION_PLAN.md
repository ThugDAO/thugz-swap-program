# Thugz Swap — implementation plan

Companion to `SWAP_SPEC.md`. This is the order of operations from empty repo to open
desk, with an explicit gate between every phase.

**Rule for the whole plan:** no phase begins until the previous phase's gate is signed
off in writing. A gate is not "it seemed fine" — it is a named person confirming a listed
condition. The 2021 collection died because a step reported success without anyone
checking the result.

---

## Reversibility map — read this first

| Action | Reversible? |
|---|---|
| Devnet anything | Yes, entirely |
| Mainnet program deploy | Yes — upgrade authority stays live (deferral policy, Phase 0) |
| `initialize_pool` | No. `expected` and `unlock_ts` are permanent |
| Verifying remints into the collection | Practically no (unverify exists, but it is messy and public) |
| `deposit_bird` | Yes, until `seal` — via `fix_mapping` + re-deposit |
| `seal` | **No. Nothing in the program can undo it** |
| Opening the desk | Yes — `set_paused` |

### Correcting a bad deposit

`deposit_bird` writes an init-once Mapping and the remint cannot leave the vault until
`unlock_ts`. `fix_mapping` (spec §4a) is the only escape, and it is hard-gated on `!sealed`.

**It is a two-step correction.** `fix_mapping` undoes; it does not put a correct pair back:

1. `fix_mapping` — mapping closed, `deposited` 1274 → 1273, remint returns to **HxwZ's**
   ATA (the compiled-in custodian — never admin's)
2. `deposit_bird` with the correct `old_mint` — admin + HxwZ co-sign, same as the bulk run
3. Re-run the sweep from scratch

Skip step 2 and `seal` refuses at `deposited != expected`. That is the counter working, but it
stalls the launch rather than failing loudly, so treat a correction as unfinished until the
re-deposit lands.

After `seal` there is no correction path at all. That is the point of the gate.

---

## Phase 0 — Decisions and hygiene

Nothing here is code. All of it blocks.

| Item | Owner | Done when |
|---|---|---|
| ~~`unlock_ts`~~ **settled: 2 years from opening** | operator | ✅ Exact unix timestamp fixed at Phase 7, published at Phase 13 |
| ~~Squads: waypoint or permanent~~ **deferred** | operator | ✅ Upgrade authority stays on `birdAyQ1d6UX…` indefinitely. Burn vs Squads decided later — see below |
| ~~Treasury funding and refiller~~ **admin wallet** | operator | ✅ `thuggjsp7Lz…` funds the treasury PDA at `initialize_pool` and refills at the watermark |
| ~~Rotate the Helius key~~ **in progress** | operator | 🔸 New key being issued for this implementation |
| ~~Review: independent reviewers?~~ **3, none involved in the design** | operator | ✅ Exceeds the condition, which asked for one |
| ~~Review gates the deposit run~~ **yes** | operator | ✅ Confirmed. Nothing irreversible on mainnet until all three have submitted |
| Move `arweave_upload/jwk.json`, `.env`, `gacha/.env.local` out of the repo working dir | operator | Folder can be shared without handing over a funded key |

### Upgrade authority — deferred, deliberately

It stays on `birdAyQ1d6UXissFpwx9WcaxvJanzRcMSvzUkQxPpaV`. Neither burned nor moved to Squads
at launch.

Be honest about what that means publicly: **a single keypair can replace the code guarding
1,274 NFTs, indefinitely.** That is a real trust position, not a neutral default, and holders
who look will see a live upgrade authority on a plain wallet.

Because "decide later" tends to mean "never", pin a revisit trigger rather than a feeling:

| Revisit at | Then decide |
|---|---|
| 20 public swaps + 30 days with no incident | Burn, move to Squads, or explicitly extend the deferral |

Whichever is chosen, publish it. An unexplained live upgrade authority reads worse than an
explained one.

### Treasury funding

`thuggjsp7Lz…` (admin) funds the treasury PDA at `initialize_pool` and is the named
refiller. (The Pool account never holds rent — a data account cannot fund
`create_account`.)

> **On moving all 1,274 birds to admin first:** it works, but it costs more and buys nothing.
> Creating 1,274 admin ATAs is **~2.6 SOL** of rent plus roughly 200 transactions that HxwZ
> has to sign anyway. It would make the deposit run single-signer — 5 per tx, 255 txs instead
> of 319 — so the net is ~455 transactions and 2.6 SOL against 319 transactions and nothing.
> HxwZ ends up warm for a comparable stretch either way, so there is no security gain. The
> rent is recoverable by closing 1,274 ATAs afterwards, which is another 1,274 instructions.
>
> Co-signing the deposit run stays the recommendation. Funding the treasury from admin is
> correct and unrelated — that is SOL, not birds.

**Gate 0: complete.** Every row above is answered. The only outstanding input is the
reviewers' estimate of how long they need, which sets the launch date.

---

## Phase 1 — Repo and toolchain

1. New Anchor workspace in `ThugDAO/thugz-swap-v2`.
2. `declare_id!("CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs")` and the same in `Anchor.toml`.
3. Pin the toolchain to the **tested pairing**, not to "latest stable" — see
   `IMPLEMENTATION_APPENDIX.md` §1. Anchor **1.1.2**, Solana CLI **3.1.10**, `solana-*` **^3**.
   Anchor 1.1.x still pins the 3.x crate line, so the 4.1.x CLI on this machine is newer than
   the combination Anchor actually tests.
4. Commit `Cargo.lock`.
5. Use `verifiedBuild` (OtterSec) rather than a hand-rolled hash comparison — it makes the
   deployed bytecode checkable by anyone, not only by us.

**Gate 1:** a clean checkout on a second machine produces a byte-identical build artifact.
If it does not, stop — nothing downstream can be verified.

---

## Phase 2 — Program

Implement `SWAP_SPEC.md` exactly: `initialize_pool`, `deposit_bird`, the pre-seal
correction path, `seal`, `swap`, `set_paused`, `recover`.

Non-negotiables from the spec, restated because they are the ones that get lost:

- `swap` reverts unless `sealed`. No pilot exception, no admin bypass, no feature flag.
- Mapping is init-once. The only mutation is `claimed`, and only inside `swap`.
- `new_mint` in `deposit_bird` and `old_mint` in `swap` are **read from token accounts**,
  never taken from instruction data.
- Account init absorbs a pre-sent lamport balance instead of aborting.
- `recover` carries the §4b constraints, and leaves `claimed == false`.
- `fix_mapping` (§4a) is admin-only and reverts unless `!sealed`.
- Deposit source, `fix_mapping` destination and `recover` destination are all pinned to the
  compiled-in `CUSTODIAN` constant (HxwZ). No instruction in the program delivers a token
  to admin.

**Gate 2:** every invariant in §6 and every constraint in §6b has a named test that fails
when you deliberately break it. A test that passes against a broken implementation is worse
than no test.

---

## Phase 3 — Devnet, functional

1. Deploy to devnet.
2. Mint ~20 mock originals and ~20 mock remints under a devnet authority you control.
3. Run the full automated suite (`TEST_PLAN.md` §1–2).

**Gate 3:** 100% of the failure matrix passes, including every row that must *fail*. Zero
skipped tests.

---

## Phase 3b — Frontend against devnet

The frontend cannot wait for Phase 13. Test Level 5a puts real people on the real page
pointed at devnet, and that needs the page to exist.

1. Build the swap page against the devnet program and mock birds.
2. Wire eligibility, the empty state, the delist notice, and the fee explanation.
3. Dogfood it internally through Phases 3–4 — every deposit and swap done through the UI
   at least once, not only by script.

**Gate 3b:** the page completes a real devnet swap end to end, and the copy already says
what the program does: one signature, holder needs SOL for the fee, one-way, two-year window.

---

## Phase 4 — Devnet, full scale rehearsal

This is the phase people skip and regret. Do the real thing at real size on fake money.

1. **Fork mainnet state with Surfpool** rather than minting 1,274 mock pairs — lazy account
   cloning gives the real remints, the real HxwZ ownership and the real 83 escrowed originals,
   without touching mainnet. Cheatcodes also let you time-travel past `unlock_ts` to exercise
   `recover`, which otherwise needs a two-year wait.
2. Run the real deposit script — 4 per transaction, two signers, ~319 transactions.
3. Deliberately create one **bad pair** — `deposit_bird` with a wrong `old_mint`. It will
   succeed; the program has no way to know. Run the sweep and confirm it is caught.
4. `fix_mapping` on the bad pair. Confirm `deposited` drops to 1273 and the remint lands in
   **HxwZ's** ATA, not admin's.
5. `deposit_bird` with the **correct** `old_mint` — admin + HxwZ co-sign again. Confirm
   `deposited` back to 1274.
6. Attempt `seal` at 1273 first (must fail with `Incomplete`), then at 1274.
7. Sweep clean, then `seal`.
8. Attempt a swap before seal (must fail) and after (must succeed).

**Gate 4:**
- Deposit run completes with `deposited == 1274` and no silent failures
- Sweep catches the deliberately bad pair
- Full correction cycle works: `fix_mapping` → re-deposit → clean sweep, nothing unsealed
- `seal` refuses at `deposited == 1273`
- Measured wall-clock time for the deposit run is recorded, so mainnet has an expectation

---

## Phase 5 — Audit

Reviewers get the built program, `SWAP_SPEC.md`, and the devnet deployment.

**All three reviewers are independent of the design.** They are asked to submit findings
before reading each other's — see `AUDIT_BRIEF.md`. A review that has already seen another
review is agreement, not a third opinion, and this project has already had one wrong premise
turn into false consensus.

Reviewing the merkle draft does not count — that document is superseded.

**Gate 5:** every finding is either fixed or has a written, accepted rationale, and **all
three reviewers have submitted**. If anything is fixed, **Phase 3 and 4 run again** — a
re-review of unrun code is not a test.

> **This gate blocks Phase 7 onward.** Phase 6 (deploy) may proceed at the operator's
> discretion because upgrade authority stays live. **Phase 7 may not**: `initialize_pool`
> writes `expected` and `unlock_ts` permanently, so a review finding that changes seeds,
> layout, the expected count or the unlock policy means redeploying everything. An earlier
> draft called Phases 6–8 reversible; init is not. Moving 1,274 birds into a vault is not:
> `fix_mapping` can undo individual pairs, but nothing brings the set back if the program
> guarding it turns out to be wrong.

---

## Phase 6 — Mainnet program

1. Deploy with the ground program keypair. Upgrade authority = `birdAyQ1d6UX…`.
2. Verify the deployed bytecode with `verifiedBuild` / OtterSec, so anyone can repeat the
   check independently.
3. Do **not** initialize yet.

**Gate 6:** verified build published and independently reproducible from the pinned
toolchain.

---

## Phase 7 — Initialize and fund

1. `initialize_pool` — `expected = 1274`, `unlock_ts` = the agreed timestamp, `sealed = false`.
2. Fund the **treasury PDA** with the opening balance (**5.5 SOL** — 1.86 Mapping + 2.60 vault ATAs + ~20% of swap ATAs; appendix §2). Not the Pool: a data account cannot pay rent.

**Gate 7:** pool decoded and read back on-chain. `expected` and `unlock_ts` are correct —
**this is the last moment either can be changed, and the change is "redeploy everything".**

---

## Phase 8 — Verify the collection

**First, re-run the pre-flight** (`verification/preflight_check.py` — the sweep's off-chain
checks: names, Arweave provenance + tags, freeze authorities) against `claim_map_all.json`,
and draw the verify list from that just-verified map. This ordering is load-bearing: the
sweep's collection-membership anchor compares the claim map against collection membership
that THIS step creates. If this step consumed a corrupted map, the anchor would agree with
the corruption. Independence comes from the pre-flight's Arweave-anchored checks running
BEFORE membership exists — not from the membership check alone.

Then HxwZ verifies all 1,274 remints into the parent collection. HxwZ still holds the
tokens.

**Gate 8:** pre-flight PASS published, then all 1,274 return `grouping: [{collection:
5Kwhy…}]` from a fresh DAS query, not from the tool's own success output. Parent collection
size reflects the addition.

---

## Phase 9 — Deposit run

> **Gated on Phase 5.** Do not start until all three reviews are in and every finding is
> closed. This is the first genuinely irreversible mainnet action at scale.
>
> **Re-measure batch size before starting.** If transaction v1 (SIMD-0385) has activated by
> then, the limit is 4,096 bytes rather than 1,232 and this run is ~95 transactions instead
> of ~319. Check the gate: `solana feature status txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL`

~319 transactions, 4 per transaction, admin + HxwZ co-signing.

Run it idempotently from a state file, the way the mint runs were done. A resumed run must
never double-deposit.

**Pre-flight, before the first transaction:** run the sweep's off-chain-only checks against
the claim map — set equality with the verified collection, Arweave provenance, and the name
match (appendix §9). Every one of those is checkable before a single deposit lands, and a
bad pair found now costs a file edit instead of a `fix_mapping` cycle. This is also the
decision on the phase-2 pipeline gap: the 733 birds that never went through
`build_remint_manifest.py` get their independent verification here, before anything
irreversible.

**Gate 9:** `deposited == 1274` on-chain, and the state file agrees. Any transaction that
errored is accounted for individually.

---

## Phase 10 — Sweep and seal

> HxwZ stays in service throughout (operator decision 2026-08-26 — key access is retained
> for as long as the desk needs it). An earlier draft parked the key offline before this
> phase, which would have left the correction path below without its required signer.

1. Run the three-way injective sweep (`SWAP_SPEC.md` §6b).
2. Publish the sweep report.
3. Any mismatch: **stop**. Run the full correction cycle — `fix_mapping`, then
   `deposit_bird` with the correct `old_mint` (admin + HxwZ co-sign, as in the bulk run),
   then sweep again from scratch. `seal` will refuse until `deposited` is back to 1274.
4. `seal`.

**Gate 10:** sweep is clean and published, `sealed == true` read back from chain.

> This is the point of no return. After `seal`, the pairing is permanent whether it is right
> or wrong. The sweep is the last thing standing between a mistake and two years.

---

## Phase 11 — Pilot

5–10 real swaps on team wallets. Desk is not announced.

**Gate 11:** correct bird out every time, `claimed` set, second attempt reverts, treasury
balance moved as expected. One tester must be someone who did not build this, following only the
public instructions.

---

## Phase 12 — Open

1. Point the frontend (built in Phase 3b, hardened by Level 5a) at mainnet and deploy to
   `thugbirdz.com/swap` and `thugdao.com/swap` (one worker, two routes).
2. Publish: vault address, exact `unlock_ts`, the 1,274 count, the delist note.
3. Announce.

**Gate 12:** copy matches the program — one signature, holder needs SOL for the fee, one-way,
two-year **recovery** window. Get this precise: at `unlock_ts` the desk does not close and
swapping does not stop — the admin merely *may* reclaim birds nobody has claimed. Copy that
implies "swap before the deadline or lose it" is false and would rush people for no reason.

---

## Phase 13 — Post-launch

| Trigger | Action |
|---|---|
| 20 successful public swaps + 30 days with no incident | Decide upgrade authority — burn, Squads, or explicitly extend — and publish the decision |
| Treasury balance below watermark | Named person refills |
| Any swap failure that is not delist/no-SOL | Investigate before it repeats |
| `unlock_ts` reached | Recovery decision, per whatever was agreed in Phase 0 |

---

## Abort conditions

Stop and reassess, at any phase:

- The sweep cannot be made clean
- A build stops reproducing from the pinned toolchain
- An audit finding has no accepted fix
- Any mainnet transaction does something the devnet rehearsal did not predict
