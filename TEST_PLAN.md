# Thugz Swap — test plan

Companion to `SWAP_SPEC.md` and `IMPLEMENTATION_PLAN.md`.

Six levels, each gating the next. Levels 1–2 run in-process against mock birds. Levels 3–4
run on Surfpool against forked mainnet state — real accounts, every write local to the
fork. Level 5a is devnet with real people and mock birds; Level 5b is mainnet with real
birds and team wallets. Level 6 runs forever.

**The standard throughout:** a test that only proves the happy path proves almost nothing.
Most of what follows is deliberately trying to break the desk, and **a test only counts if
it has been seen to fail against a deliberately broken build.** A green suite that stays
green when you delete a `require!` is decoration.

---

## Level 1 — Unit and instruction tests (automated)

**LiteSVM** (scaffolded by `anchor init` in Anchor 1.0+) or **Mollusk**. In-process, fast,
run on every commit. Not `solana-test-validator` — that is only for fidelity Surfpool cannot
emulate.

### Happy paths

| Test | Asserts |
|---|---|
| `initialize_pool` | `expected=1274`, `sealed=false`, `unlock_ts` as passed, admin set |
| `deposit_bird` | Mapping exists at the right PDA, `new_mint` matches the token moved, `deposited` +1, remint in vault ATA |
| `seal` at `deposited == expected` | `sealed == true` |
| `swap` after seal | Original in vault, remint with holder, `claimed=true`, `claimed_by` correct, `swapped` +1 |
| `set_paused` / unpause | Swap blocked then allowed |
| `recover` after `unlock_ts` | Unclaimed remint leaves, `claimed` still false |
| `fix_mapping` while `!sealed` | Mapping closed, rent to **treasury**, `deposited` −1, remint in **HxwZ's** ATA |
| **`fix_mapping` cannot send the remint to admin** | Any destination other than the custodian's (HxwZ's) ATA is rejected — this is the custody control |
| `deposit_bird` when the vault ATA does not exist | Creates it idempotently, then transfers — this is every first deposit |
| `fix_mapping` when the custodian (HxwZ) ATA does not exist | Creates it, then transfers |
| `recover` when the destination ATA does not exist | Creates it, then transfers |
| Re-`deposit_bird` the same `old_mint` after a fix | New Mapping, `deposited` back up, admin + HxwZ signing as in the bulk run |

### Failure paths — the actual work

Every row from `SWAP_SPEC.md` §7, each asserting a **specific** error, not just "it reverted":

| Attempt | Must fail with |
|---|---|
| Swap with no Mapping | `AccountNotInitialized` |
| Swap same original twice | `AlreadyClaimed` |
| **Swap while `!sealed`** | `NotSealed` |
| Swap while paused | `Paused` |
| Swap an original you do not own | `NotOwner` |
| Swap an original you hold 0 of (stale ATA) | `NotHeld` |
| Name bird A, surrender bird B | Derives B's mapping; A untouched |
| `vault_new_ata` pointed at a different remint | `WrongRemint` |
| Seal at `deposited < expected` | `Incomplete` |
| `deposit_bird` after seal | `Sealed` |
| `fix_mapping` after seal | `Sealed` |
| `seal` with a correction half-done (`deposited` one short) | `Incomplete` |
| `recover` before `unlock_ts` | `Locked` |
| `recover` targeting a deposited original | `NotRecoverable` |
| `recover` a claimed mapping | `AlreadyClaimed` |
| Non-admin calls `deposit_bird` / `seal` / `recover` / `set_paused` / `fix_mapping` | Constraint violation |
| `deposit_bird` from a source ATA not owned by `CUSTODIAN` | `NotCustodian` |
| `deposit_bird` signed by a delegate instead of the custodian itself | `NotCustodian` |
| `fix_mapping` or `recover` with a destination that is not the custodian's ATA | `NotCustodian` |
| `fix_mapping`/`recover` to an existing **frozen** destination ATA (mock mint with freeze authority) | Fails legibly — unreachable on mainnet without the owner's own delegation; the pre-flight asserts every freeze authority is the mint's own master edition PDA |
| `initialize_pool` with any way to vary `expected` | Impossible — `expected` comes from the compiled `EXPECTED` constant |
| Second `initialize_pool` | Already initialized |
| Second `seal` | Already sealed |

### Adversarial

| Attack | Expected |
|---|---|
| Pre-send lamports to a Mapping PDA, then deposit | Manual init absorbs the balance, succeeds. **Anchor `init` would fail here** |
| Attempt to use `Pool` as the rent payer | Fails — a data-bearing account cannot fund `create_account` |
| Treasury that someone `assign`ed or `allocate`d | Rejected as payer. This is the way to kill the desk permanently, so it gets its own test |
| Pre-sent lamports to treasury (system-owned, 0 data) | Still a legal payer — succeeds |
| Admin runs `fix_mapping` 1,274 times pre-seal | Every remint lands with HxwZ. Admin balance unchanged |
| Drain the treasury below one Mapping's rent mid-deposit | Deposit fails legibly, not mysteriously |
| Pass a Mapping PDA derived from a different pool | Seeds constraint fails |
| Pass a token account owned by someone else as `holder_original_ata` | `NotOwner` |
| Pass a fake vault ATA the caller owns | Owner constraint fails |
| Swap a **remint** back in | No mapping derives → fails |
| Reentrancy via a malicious token program | Program IDs are pinned; fails |
| Integer edges: `deposited` at 0 and 1274 | No overflow, seal boundary exact |

**Exit:** 100% pass, zero skips, and each of the bolded rows verified to fail against a build
with that check removed.

---

## Level 2 — Property and differential tests (automated)

| Property | Method |
|---|---|
| No sequence of instructions makes `sealed` false again | Randomised op sequences, assert monotonic |
| No sequence lets a Mapping be claimed twice | Randomised, including interleaved swaps |
| `swapped` never exceeds `deposited` | Invariant checked after every op |
| A remint leaves the vault only via `swap`, pre-seal `fix_mapping`, or post-unlock `recover` | Assert after every op |
| A deposited original is never transferable out | Attempt after every op |
| Sweep logic rejects every corruption class | Inject: missing mapping, wrong new_mint, duplicate new_mint, empty vault ATA, a new_mint outside the verified collection (substituted pair), a mint-time mis-pair (name mismatch), an Arweave fetch that errors (must fail, never skip) |

> **How to inject a bad pair.** There is no instruction that edits a Mapping, so "corrupt a
> mapping" cannot mean editing one. It means calling `deposit_bird` with a **wrong
> `old_mint`** — which succeeds, because the program has no way to know the pairing is wrong.
> That is precisely why the sweep exists, and it is the unit test the sweep needs: **the bad
> deposit must succeed and the sweep must fail.**

The sweep is code and gets its own tests. It is the last line before an irreversible step,
so a sweep that silently passes a corrupt set is the worst bug available.

---

## Level 3 — Integration (Surfpool)

**Surfpool** is the default `anchor test` runner in Anchor 1.0+ and forks mainnet with lazy
account cloning. Deployed program, real RPC semantics, real confirmation — against forked
real state rather than hand-made mocks.

- All of Level 1 again against the deployed program — local validator hides confirmation,
  rent, and compute realities
- Transaction size measured on the real deposit and swap instructions, compared against the
  1,214 / 670 byte figures in the spec
- **Compute units measured and recorded** for both instructions — nothing in these documents
  has measured CU, and for `swap` it binds before size does. Specifically: a first-time swap
  (both ATAs created), a repeat swap (ATAs exist), and two swaps batched. If two batched
  exceed the 200k default, the frontend needs an explicit `ComputeBudget` request and that
  needs to be known before the page is written, not after a holder reports a failure
- Retry behaviour: kill the deposit script mid-run, resume, assert no double-deposit

**Exit:** functional parity with Level 1, plus measured bytes and CU within expectation.

---

## Level 4 — Devnet full-scale rehearsal

The whole sequence, end to end, against **forked mainnet state** — the real 1,274 remints,
the real HxwZ ownership, the real escrowed originals — with no mainnet writes and no mock
minting.

| Step | Assert |
|---|---|
| Deposit run, ~319 txs | `deposited == 1274`, state file agrees, wall-clock recorded |
| Kill and resume mid-run | No double-deposit, no gap |
| **Deliberately deposit a bad pair** (`deposit_bird`, wrong `old_mint`) | Deposit succeeds; sweep catches it |
| `fix_mapping` on the bad pair | `deposited` 1274 → 1273, remint back in **HxwZ's** ATA, `sealed` still false |
| `seal` attempted at 1273 | `Incomplete` |
| Re-`deposit_bird` with the correct `old_mint` | Admin + HxwZ co-sign, `deposited` back to 1274 |
| Sweep again from scratch | Clean |
| Swap attempt before seal | Fails |
| `seal` | Succeeds |
| 50 swaps across 50 wallets | All correct |
| Two wallets race the same bird | One wins, one fails cleanly |
| Wallet with zero SOL | Fails at network layer with a message the frontend can explain |
| Treasury drained to below one swap's rent | Swap fails; confirm the failure is legible, not mysterious |

**Exit:** every row green, including the bad pair being caught *and fully corrected*. This is
the single most important test in the plan: it is the only proof that the sweep catches what
the program cannot, and that a mistake found at 1,274 scale is recoverable rather than
terminal.

---

## Level 5 — User testing

### 5a. Devnet, real people

Recruit 5–10 holders. Give them devnet mock originals. Point the **real frontend** — the one
built in Implementation Phase 3b — at devnet. Give them the public instructions and nothing
else.

Watch for, and record:

- Do they understand they need SOL for the fee?
- Do they understand it is one-way before they sign?
- Does a listed bird produce a comprehensible message, or a blank screen?
- Does a wallet with no eligible birds land on the empty state and know what to do?
- Does anyone try to swap twice?
- How long from landing to signed?

**Exit:** nobody is confused about the fee, the one-way nature, or why a listed bird is
missing. Any confusion is a copy bug — fix the page, retest.

### 5b. Mainnet pilot

5–10 real swaps, team wallets, desk not announced. Sealed pool, real birds.

> **This is past the point of correction.** The pool is sealed, so `fix_mapping` is dead and
> nothing can be undone. Restrict the pilot to originals the team already holds — never ask
> an outside holder to be the first real swap.

| Check | |
|---|---|
| Correct remint delivered | Compare mint against `claim_map_all.json` |
| Bird arrives already in the collection | Fresh DAS query, not the tool's own output |
| `claimed` set, `claimed_by` correct | Decode the mapping |
| Second attempt on the same bird | Reverts |
| Treasury balance | Moved by exactly the expected rent |
| Explorer and wallet display | Renders correctly, allowing for indexer lag |

At least one tester must be someone who did not build this, following only the public page.

**Exit:** all green, and any indexer lag documented so it is not mistaken for a failure at
launch.

---

## Level 6 — Production monitoring

The scheduled worker (one job, shared with the health checks) watches:

| Signal | Alert when |
|---|---|
| Treasury SOL balance | Below watermark (~12 swaps × 0.00408 SOL of headroom) |
| `swapped` vs successful transactions | They diverge |
| Failed swap transactions | Any cause other than delist or insufficient SOL |
| Frontend availability, both routes | Non-200 |
| RPC health | Errors or rate limiting |
| Vault contents | Remints in the vault match `1274 - swapped - recovered` (all counters on Pool) |
| Transaction format v1 activation | Gate `txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL` becomes present |

> **Why the v1 row is there.** Transaction v1 (SIMD-0385) raises the size limit to 4,096
> bytes and moves signatures to the tail, so a v1 transaction starts with byte `0x81`.
> *Reading* it is breaking and not opt-in: an indexer or RPC consumer that has not adapted
> will break or silently misreport once v1 transactions land. This desk is expected to run
> two years. Anything here that parses transactions needs to survive that, and the cheapest
> version is to know the day the gate flips rather than the day a dashboard goes quiet.

The vault-contents row is the ongoing version of the sweep: it would catch a bird leaving the vault by
any route other than a swap. That should be impossible. Check anyway — the whole reason this
project exists is that in 2021 nobody checked whether a thing that reported success had
actually worked.

---

## Test data

Levels 1–2 and 5a use mock pairs, so a mistake in a test script can never touch a real
remint. Levels 3–4 run against Surfpool's fork of mainnet state — the real accounts, but
every write stays inside the fork; nothing can reach mainnet. The mock set should mirror the real one in shape: 1,274 pairs,
injective, plain SPL `NonFungible`, one owner holding all remints.

Reuse `claim_map_all.json`'s structure for the mock manifest so the sweep code under test is
the same code that runs on mainnet.
