# Thugz Swap — program spec (PDA design)

**Status:** design complete, nothing written, nothing deployed.
**This supersedes the merkle draft.** There is no merkle root, no proof, and no separate
receipt account. If you have an older copy, discard it.

**`IMPLEMENTATION_APPENDIX.md` is part of this spec.** It carries the account layouts, seed
constants, error enum, event definitions, toolchain pins and the sweep-script spec. Where the
two disagree, the appendix is newer — it was written against the Solana Foundation
`solana-dev` skill after this document.

Reviewers: the questions we most want challenged are at the bottom under
[Open questions](#open-questions).

---

## 1. What it does

A 2021 Solana NFT collection lost its art — the Arweave uploads were posted but never
confirmed mined, and the originals are immutable so their URIs can't be repaired.

1,274 replacement NFTs have been minted with recovered art. They sit in a vault the
program controls. A holder signs **one transaction** that hands over their original and
receives the matching remint.

- Pairing is fixed and complete before the desk opens. Nothing is added later.
- One-way. There is no un-swap.
- Originals stay in the vault permanently — locked, not burned.
- No pricing, no ordering, no auction. One instruction.

---

## 2. Ground facts (verified on-chain, 2026-08-26)

| | |
|---|---|
| Pairs | 1,274 |
| Remints | all `NonFungible`, legacy SPL Token program, **none frozen**, no pNFT |
| Remint custody | all 1,274 currently in `HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB` |
| Originals in marketplace escrow | **83** (46 Magic Eden, 36 Tensor, 1 other) — must delist |
| Originals with a delegate | 14, **none frozen** → owner can still transfer, no revoke needed |
| Distinct original holders | 890 |
| Claim map | **injective** — 1,274 unique olds, 1,274 unique news, no address is both |

Because nothing is frozen and nothing is pNFT, a plain SPL transfer is sufficient
throughout. No rule-set or delegate handling in the swap path.

### What the remint carries that this spec did not previously mention

Found by an independent compliance pass, not by design review. None of it is changeable now —
it was set at mint time — but a holder is entitled to know it and the docs said nothing.

| | |
|---|---|
| Royalty | **5%** (`seller_fee_basis_points: 500`) |
| Royalty recipient | `4td5cAuEmpD8U1icAbXpCZpmEKJjAPjMzEgkbL225FxY` at 100% share |
| Verified creator | `HxwZ…iiUB` occupies a `verified: true` creator slot at 0% share on all 1,274 |
| Pairing record | Each remint's Arweave metadata carries `provenance.original_mint`, plus an indexed immutable `Original-Mint` tag |

Two things follow. **§1's "no pricing, no ordering, no auction" describes the swap, not the
asset** — the bird handed back carries a 5% fee to a wallet these documents never named. Say
so on the desk rather than letting a holder discover it at first sale.

And the HxwZ creator slot **outlives this program entirely, which cannot remove it** —
invariant 8 now says so.

**The pairing was already public.** The full 1,274-pair mapping has been readable and
immutable on Arweave since mint. That does not weaken the PDA design — it strengthens the
sweep, which now cross-checks against it (appendix §9) — but it does mean §6b's claim that
`seal` is *the* public commitment is not quite right. The first commitment happened months
ago, in a place we cannot edit.

---

## 3. Accounts

### Pool — singleton, seeds `["pool"]`

| Field | Type | Notes |
|---|---|---|
| `admin` | Pubkey | Deposits, pause, post-unlock recovery. Cannot take a bird before unlock. |
| `collection` | Pubkey | Parent collection of the remints. |
| `expected` | u16 | `1274`, from the compiled-in `EXPECTED` constant. Never from instruction data; never changes. |
| `deposited` | u16 | Incremented by `deposit_bird`. |
| `swapped` | u16 | Incremented by `swap`. |
| `recovered` | u16 | Incremented by `recover`. Vault reconciliation: remints in vault `== expected - swapped - recovered`. |
| `sealed` | bool | False until the set is complete and verified. Gates swapping. |
| `paused` | bool | Admin safety valve. Stops swaps only. |
| `unlock_ts` | i64 | Unix seconds, **opening + 2 years**. Before it, nothing leaves except by a valid swap. Immutable. |
| `bump` | u8 | |

### Mapping — one per original, seeds `["map", pool, old_mint]`

This is **both the pairing and the receipt**. There is no second account.

| Field | Type | Notes |
|---|---|---|
| `new_mint` | Pubkey | The remint this original earns. |
| `claimed` | bool | Set once in `swap`. Never unset. |
| `claimed_by` | Pubkey | Zero until claimed. |
| `claimed_at` | i64 | Zero until claimed. |
| `recovered` | bool | Set by `recover`; makes recovery idempotent without touching `claimed`. |
| `bump` | u8 | |

Existence of this account **is** eligibility. There is no list to publish and no proof to
supply — a holder derives the address from their own mint and reads the answer straight
off the chain without trusting anything we host. That property is the main reason this
design was chosen over a merkle root.

### Treasury — seeds `["treasury"]`, **system-owned, no data**

Holds the SOL that pays rent. It must be a **plain system account with no data**, because
`system_program::create_account` requires its funding account to be system-owned and empty —
a data-bearing PDA like `Pool` **cannot** be the payer for Mapping init or ATA creation.

An earlier draft had `Pool` paying. That would have failed at the first deposit and the first
swap. Rent returned by `fix_mapping` goes back here, not to `Pool`.

**Never `init`, `allocate` or `assign` this account.** Fund it with a plain system transfer to
the derived address so it stays system-owned with zero data. `initialize_pool` records
`treasury_bump` and nothing else. The first `assign` to this program kills it as a legal
`create_account` payer and the desk stops working.

It signs as payer via `invoke_signed` with `["treasury"]` seeds; the signer bit carries
through into the ATA program's inner `create_account`. Anyone can send SOL in; only the
program sends it out, and only as rent.

### Vault authority — seeds `["vault"]`

Owns every token account holding a remint before swap, and every original after. Token
accounts are ordinary ATAs owned by that PDA.

---

## 4. Instructions

| Instruction | Signer | Effect |
|---|---|---|
| `initialize_pool` | admin | Creates Pool. `expected` comes from the compiled `EXPECTED` constant and the signer must BE the compiled `ADMIN` constant — the singleton `["pool"]` seeds are front-runnable otherwise, and a captured pool is permanent. Takes `unlock_ts` (must be in the future), `collection`. `sealed = false`. |
| `deposit_bird` | admin **+ token owner (HxwZ)** | Moves one remint into the vault **and** creates its Mapping in the same transaction. Fails if sealed. |
| `fix_mapping` | admin | **Only while `!sealed`.** Closes a Mapping, returns rent to the **treasury**, decrements `deposited`, and returns the remint **to HxwZ's ATA — never to admin**. The one way to correct a bad deposit. |
| `seal` | admin | One-way. Requires `deposited == expected`. After this no Mapping can be created or altered. |
| `swap` | holder | The only instruction a holder calls. Requires `sealed && !paused`. |
| `set_paused` | admin | Stops swaps. Cannot move anything. |
| `recover` | admin | Only after `unlock_ts`. Moves unclaimed remints to the **custodian's ATA**. Never touches deposited originals. Constraints in §4b. |

### `swap` — the whole desk

```rust
// signer: holder (also the fee payer)
require!(pool.sealed, NotSealed);        // no swaps while pairs are still writable
require!(!pool.paused, Paused);

// The mint is READ FROM the token account being surrendered,
// never taken from instruction data.
let old_mint = holder_original_ata.mint;
// Mapping PDA derives from that mint, so a wrong bird derives a
// different (or non-existent) account and the instruction fails.
seeds = ["map", pool.key().as_ref(), old_mint.as_ref()];

require!(!mapping.claimed,                        AlreadyClaimed);
require!(vault_new_ata.mint == mapping.new_mint,  WrongRemint);
require!(holder_original_ata.owner == holder.key(), NotOwner);
require!(holder_original_ata.amount == 1,         NotHeld);

create_idempotent(holder_new_ata);       // rent paid by treasury
create_idempotent(vault_original_ata);   // rent paid by treasury

transfer_checked(holder_original_ata -> vault_original_ata, 1, old_mint, 0);
transfer_checked(vault_new_ata -> holder_new_ata, 1, new_mint, 0);  // vault PDA signs

mapping.claimed    = true;
mapping.claimed_by = holder.key();
mapping.claimed_at = clock.unix_timestamp;
pool.swapped      += 1;
```

### `deposit_bird` — same discipline as `swap`

```rust
// signers: admin (tx fee payer, pool authority) + token owner (HxwZ)
require!(!pool.sealed, Sealed);

// new_mint is READ FROM the remint token account being transferred.
// Only old_mint comes from instruction data — the remint carries no
// reference to its original on-chain, so there is nothing to read it from.
let new_mint = source_ata.mint;
require!(source_ata.amount == 1, NotHeld);
require!(source_ata.owner == CUSTODIAN, NotCustodian);  // deposits only ever come from HxwZ
// The transfer authority is the OWNER itself, signing — never a delegate. Without this,
// "HxwZ co-signs" is prose, not a constraint.
require!(token_authority.key() == CUSTODIAN && token_authority.is_signer, NotCustodian);

// Mapping is init-once. No update path exists anywhere in the program.
// init must ABSORB a pre-sent lamport balance rather than aborting (see griefing).
// Rent payer is the TREASURY PDA, which must already be funded.
init mapping @ ["map", pool, old_mint] { new_mint, claimed: false }   // rent from treasury

create_idempotent(vault_new_ata);        // MUST be created here — it does not exist yet
transfer_checked(source_ata -> vault_new_ata, 1, new_mint, 0);  // token owner signs
pool.deposited += 1;
```

Admin supplies `old_mint`; a wrong value here writes a wrong pair, which is exactly what the
three-way sweep exists to catch before `seal`. `new_mint` is never supplied — it is read off
the token being moved, so the admin path cannot suffer the "name bird A, hand over bird B"
class of bug either.

**`transfer_checked`, not `transfer`.** Use `anchor_spl::token_interface`. All 1,274 remints
are verified legacy SPL Token so plain `transfer` would work today, but `transfer_checked`
costs nothing extra here — both mints are already in the account set for ATA derivation — and
it is the only version that survives contact with Token-2022.

**`init`, never `init_if_needed`, on Mapping.** `init_if_needed` permits reinitialization
attacks. Mapping is init-once and the init failure *is* the constraint. This is a different
mechanism from `create_idempotent` on ATAs, which is correct and expected; both appear in
`swap`.

**Why the mint is read, not declared.** If the program trusted an `old_mint` passed as
instruction data, a caller could name a valuable bird while surrendering a worthless one.
Reading the mint off the account actually being transferred makes that impossible.

**Account-init griefing.** Mapping addresses are derivable as soon as the program ID is
public, so anyone can pre-send lamports to one and make a naive `create_account` fail. This
lands on the admin during the deposit run, not on holders — but use an init path that
absorbs an existing lamport balance rather than aborting. Note this is *not* fixed by
adding the pool to the seeds: the pool is itself a deterministic PDA, so the addresses stay
predictable either way. The pool in the seeds is there to stop a second deployment
colliding with the first.

**Why both ATAs are created idempotently.** The holder has never held their remint, and
the vault has never held their original, so neither token account exists. Idempotent
creation also survives a retry, or a holder who already has the account. Omitting the
holder's ATA fails for every first-time swapper — which is everyone.

---

### 4a. `fix_mapping` — why a correction path must exist

Without it, a single wrong pair among 1,274 is unfixable: the Mapping cannot be updated
(init-once) and the remint cannot leave the vault until `unlock_ts` — two years. The
pre-seal sweep would find the fault and there would be nothing to do about it.

```rust
require!(!pool.sealed, Sealed);       // hard gate, no exceptions
require!(!mapping.claimed, AlreadyClaimed);   // unreachable pre-seal, asserted anyway
close(mapping) -> rent back to treasury;
// Destination is the custodian's ATA — the address the bird came from — never admin's.
// CUSTODIAN is a compile-time constant (HxwZ), not an account the caller supplies and
// not a field admin ever sets. Admin can un-deposit. Admin cannot pocket.
require!(custodian_ata == ata(CUSTODIAN, new_mint), NotCustodian);
create_idempotent(custodian_ata);        // destination may not exist; rent from treasury
transfer_checked(vault_new_ata -> custodian_ata, 1, new_mint, 0);
pool.deposited -= 1;
```

### The correction is two steps, not one

`fix_mapping` only undoes. It does not put a correct pair back, and the sweep cannot pass
until something does. The full cycle is:

| Step | Effect |
|---|---|
| 1. `fix_mapping` on the bad pair | Mapping closed, `deposited` 1274 → 1273, remint back in **HxwZ's** ATA |
| 2. `deposit_bird` with the correct `old_mint` | New Mapping, `deposited` back to 1274 — admin + HxwZ co-sign, as in the bulk run |
| 3. Re-run the sweep from scratch | Clean, or repeat |

Nothing changes for step 2: the remint is back in **HxwZ's** ATA, so the re-deposit is the
same two-signer path as the bulk run — admin + HxwZ, 4 per transaction. (An earlier draft of
this table said the remint returned to admin and re-deposit was single-signer at 5 per tx.
That was the v2 design; under v3 admin never holds a bird, ever.)

Skipping step 2 leaves `deposited == 1273`, and `seal` refuses at `deposited != expected` —
which is the counter doing its job, but it means an incomplete correction stalls the launch
rather than failing loudly.

**An earlier draft of this section claimed this "does not widen the trust window". That was
false.** Writing a mapping is not custody of a token. Every other movement of these birds
needs HxwZ's signature as owner — so an admin-only instruction that sends a remint out of the
vault was a genuinely new power, and with 1,274 of them a unilateral drain until `seal`.

Keeping the HxwZ key in service does **not** fix that, because HxwZ is not a signer here.

**The fix is the destination, and the destination is a constant.** `fix_mapping` returns the
remint to **HxwZ's ATA**, the address it came from. HxwZ is compiled in as `CUSTODIAN` — not
an account the caller supplies, and not a field set at `initialize_pool`: a mis-set init
value would put admin right back in the destination seat, so it is deliberately not
configurable. A reviewer verifies the constant in source; the verified build ties source to
bytecode. Admin can undo a bad deposit; admin cannot move value to themselves. Re-depositing
then needs HxwZ again — the same two-key path as the original deposit, which is honest
rather than convenient.

The alternative, equally valid, is requiring HxwZ as a co-signer on `fix_mapping`. Either
closes it; the destination version needs no extra signature at correction time.

**One edge the destination rule inherits: a frozen destination ATA.** `create_idempotent`
does not thaw an existing frozen account, and `transfer_checked` into one fails — which
would strand `fix_mapping` (and `recover`) for that bird. Every Metaplex NFT carries a
freeze authority — the mint's own master edition PDA — and that shape is safe: only Token
Metadata's delegate paths can freeze through it, and those need the destination owner's own
delegation, which HxwZ never grants. What must not exist is a **foreign** freeze key, so
the pre-flight asserts every remint's freeze authority is exactly its own master edition
PDA. The devnet suite still exercises the failure (mock mints where you hold freeze
authority) so the error is legible if the assumption is ever wrong.

### 4b. `recover` — constraints, not prose

"Moves unclaimed remints out" is not an invariant; without explicit checks it can be
implemented as "admin passes any vault ATA." Require all of:

```rust
require!(pool.sealed,                            NotSealed);   // post-open only
require!(clock.unix_timestamp >= pool.unlock_ts, Locked);
require!(!mapping.claimed,                        AlreadyClaimed);
require!(!mapping.recovered,                      AlreadyClaimed); // recover-once per mapping
require!(vault_ata.mint == mapping.new_mint,     NotRecoverable);
require!(vault_ata.amount == 1,                  NotHeld);
require!(vault_ata.owner == vault_pda,           NotRecoverable);
```

`recover` sets `mapping.recovered = true` and increments `pool.recovered`. The `sealed`
gate and the `recovered` flag were added in the 2026-08-27 audit pass: without the gate an
admin who mis-set a short `unlock_ts` could recover from an unsealed vault; without the
flag a remint returned to the vault could be recovered twice and break the
`in_vault == expected − swapped − recovered` reconciliation.

`recover` also increments `pool.recovered`, so the Level 6 vault reconciliation stays true
after unlock.

**Destination: the custodian's ATA, `ata(CUSTODIAN, new_mint)` — same rule as `fix_mapping`.**
Admin triggers recovery; admin never receives the token. Disposal of recovered birds is a
separate, deliberate HxwZ action after unlock. Rent for the destination ATA comes from the
treasury, like every other rent in this program.

**How originals are actually protected.** `vault_ata.mint == mapping.new_mint` **is** the
on-chain guard, and it is a real one: recovery can only ever touch an account whose mint a
Mapping names as a remint.

What the program cannot do is ask "is this mint an original?" directly — `old_mint` is seed
material, not a stored field. It does not need to. Because the claim map is **injective with
no address in both sets** (verified: 1,274 unique olds, 1,274 unique news, zero overlap) and
the sweep confirms that on-chain before `seal`, no original can satisfy the mint check.

So it is the mint check plus a property of the data, not the sweep alone. Do not weaken the
mint check on the assumption that the sweep carries it.

**Leave `claimed` false after recovery.** A later `swap` against that mapping then fails
atomically on an empty vault account, and the holder keeps their original. Flipping `claimed`
to "tidy up" would strand the original in a dead mapping — the holder hands over a real 2021
bird and gets nothing.

---

## 5. Fees and rent

> **The holder needs a small amount of SOL.** An earlier draft said an empty wallet could
> swap. That was true of the original co-signed design, where a DAO wallet signed and
> therefore paid. It is false here. A Solana transaction requires a signing fee payer with
> lamports, and a program cannot be one. The holder signs, so the holder pays the network
> fee — a fraction of a cent.

The **treasury** pays rent: the two token accounts created during a swap and
the Mapping accounts and vault token accounts created during setup. Nobody is asked for rent.

| What | Count | Each | Total |
|---|---|---|---|
| Mapping accounts (82 B) | 1,274 | 0.001462 | **1.86 SOL** |
| Vault remint ATAs (165 B) | 1,274 | 0.002039 | **2.60 SOL** |
| Per swap: vault-original ATA + holder-remint ATA | 2 per swap | 0.002039 | **5.20 SOL** if all 1,274 swap |
| | | | **9.66 SOL total** |

An earlier draft said ~1.74 SOL and a 2 SOL opening balance. That omitted the 1,274 vault
token accounts entirely and undercounted the rest — the real exposure is roughly **five times**
larger.

### The treasury is the funded account — `Pool` never holds rent

| | |
|---|---|
| Funded by | admin, as a **plain system transfer to the derived `["treasury"]` address** |
| Before `deposit_bird` | ≥ **4.46 SOL** — 1.86 Mapping + 2.60 vault ATAs |
| Over the desk's life | **0.00408 SOL × swaps remaining** (two ATAs each) |
| Suggested opening balance | **5.5 SOL** — covers setup plus ~20% of swaps; top up from the watermark |
| Watermark | alert below **0.05 SOL** (~12 swaps left); admin refills |

A treasury that runs dry in year two cannot pay `create_idempotent` and every swap fails. That
is a silent outage that looks like a holder error, so the monitoring job checks the treasury
balance alongside everything else.

---

## 5b. Events

Every state change emits one. Without these the monitoring in `TEST_PLAN.md` §6 has to scrape
logs, and reconciling `pool.swapped` against reality becomes guesswork.

```rust
#[event] pub struct BirdDeposited { old_mint, new_mint, deposited }
#[event] pub struct MappingFixed  { old_mint, new_mint, deposited }
#[event] pub struct PoolSealed    { expected, ts }
#[event] pub struct BirdSwapped   { old_mint, new_mint, holder, ts, swapped }
#[event] pub struct PauseSet      { paused, ts }
#[event] pub struct BirdRecovered { new_mint, ts, recovered }
```

`BirdSwapped` is the one the desk actually runs on. Full field types in the appendix.

**Anything reading these must pass `maxSupportedTransactionVersion: 1`.** Transaction v1 is
not active yet, but reading it is breaking and not opt-in, and this desk runs for two years.

---

## 6. Invariants

1. A Mapping is created only inside `deposit_bird`, in the same transaction as the token
   transfer. No mapping without a bird; no bird without a mapping.
2. After `seal`, no Mapping can be created, modified or closed. `sealed` never returns to false.
   `fix_mapping` exists only for the pre-seal window and is dead afterwards.
3. `swap` is impossible unless `sealed`.
4. A Mapping can be claimed exactly once. `claimed` is never unset.
5. Originals that enter the vault never leave. `recover` cannot touch them.
6. Nothing leaves the vault before `unlock_ts` except through a valid swap, **or through
   `fix_mapping` while `!sealed`**.
7. Admin cannot take a remint before `unlock_ts` under any instruction **once `sealed`**.
   Before sealing, `fix_mapping` is exactly that power, deliberately — it is the only way to
   correct a bad deposit, and it dies at `seal`.
8. The program never holds collection authority. Verification is external, and HxwZ remains
   the update authority on the parent and all 1,274 remints throughout — and occupies a
   verified creator slot (0% share) on all 1,274. The program cannot change, revoke or
   inherit any of it.
9. Mapping is **init-once and admin-only**, created only inside `deposit_bird`. There is no
   `update_mapping`, no public create instruction, and no `unseal`.
10. No merkle root is stored. A root the program never checks in `swap` is a comment, not a
    constraint — the program cannot verify a root against 1,274 accounts in one transaction.

> **Every invariant above is conditional on the deployed code.** The upgrade authority stays
> live on `birdAyQ1d6UX…` indefinitely by decision, and that key can replace this program
> after review, after `seal`, and after launch. Read invariants 1–10 as "the program as
> reviewed does this", not "this can never happen". Anyone relying on them should check the
> upgrade authority first. Burning it is what would make them unconditional.

---

## 6b. Load-bearing constraints

Without all three of these, the merkle design is genuinely better. They are not
nice-to-haves.

| # | Constraint | If skipped |
|---|---|---|
| 1 | `swap` reverts if `!sealed`. **No exception for a pilot.** | During the 255-tx deposit window a mistaken or malicious mapping for an excluded original — e.g. one of the six live cantangler birds — lets its holder take a remint that belongs to someone else. |
| 2 | Mapping is init-once, admin-only, inside `deposit_bird`. No update, no public create, no unseal. | Front-run garbage mappings, rewritten pairs, or a reopened set. |
| 3 | The pre-seal sweep is **three-way and injective**, not a count. | `deposited == 1274` is satisfied by 1,274 *wrong* pairs. A count is not a commitment. |

### What the sweep must actually check

`deposited == expected` is enforced on-chain but proves only arity. Before `seal`, off-chain:

1. Every `old_mint` in the claim map has a Mapping PDA that exists.
2. Every Mapping's `new_mint` equals the claim map's value for that old.
3. Every `new_mint` appears exactly once across all mappings (injective — no two originals
   pointing at one remint).
4. Every `new_mint`'s vault ATA actually holds that NFT, amount 1.
5. The set of `new_mint`s equals, exactly, the 1,274 remints among the chain-derived
   membership of the verified collection `5Kwhy…` (DAS `getAssetsByGroup`; the collection
   also holds the ~2,024 migrated entangled originals and the parent — filter to the remint
   set). This anchors *which* remints are in play — a dropped-and-substituted pair fails
   here even when everything else is self-consistent. **This anchor is downstream of
   Phase 8**, which creates the membership from the claim map itself; it is independent
   only because Phase 8 is gated on the pre-flight (checks 6–7) passing first.
6. Every remint's Arweave `provenance.original_mint` equals the claim map's `old_mint`
   (appendix §9), fetched from a strict `https://arweave.net/<txid>` URI.
7. Every remint's on-chain `name` equals its original's on-chain `name` **and** the `name`
   inside the remint's Arweave metadata JSON. The on-chain names alone are not a fully
   independent witness — the remints are mutable and HxwZ is their update authority, and
   ~300 of the originals are technically mutable under an authority nobody has accessed —
   but the Arweave copy of the name was frozen at mint and binds the check: a rewritten
   on-chain name disagrees with its own Arweave metadata. A pairing that was wrong *at
   remint time*, where tag and claim map are wrong together and the provenance check cannot
   see it, disagrees here. The sweep also records each remint's `is_mutable` and update
   authority alongside the report.

Any mismatch is a hard stop, not a retry-later. Publish the sweep report; that report plus
`seal` is the real commitment to the set, and it is what replaces a merkle root.

## 7. Test matrix

Every row is a devnet test before mainnet.

### Core

| Attempt | Expected |
|---|---|
| Valid swap | Succeeds; correct remint out; `claimed` set |
| Swap a bird with no Mapping | `AccountNotInitialized` |
| Swap the same original twice | `AlreadyClaimed` |
| Swap while `!sealed` | `NotSealed` |
| Swap while paused | `Paused` |
| Swap an original you do not own | `NotOwner` |
| Name bird A, surrender bird B | Derives B's Mapping; A untouched |
| Point `vault_new_ata` at a different remint | `WrongRemint` |
| Seal before `deposited == expected` | `Incomplete` |
| `deposit_bird` after sealing | `Sealed` |
| `fix_mapping` after sealing | `Sealed` |
| `fix_mapping` by a non-admin | Constraint violation |
| `deposit_bird` from a source not owned by `CUSTODIAN` | `NotCustodian` |
| `fix_mapping` or `recover` with a destination that is not the custodian's ATA | `NotCustodian` |
| `fix_mapping` then re-`deposit_bird` same `old_mint` | Succeeds; `deposited` back to 1274 |
| `seal` while a correction is half-done (`deposited == 1273`) | `Incomplete` |
| `recover` before `unlock_ts` | `Locked` |
| `recover` targeting a deposited original | `NotRecoverable` |
| Holder already has an ATA for the remint | Succeeds (idempotent) |
| Holder with zero SOL | Fails at the network layer, before the program |

### Custody and ownership changes

| Attempt | Expected |
|---|---|
| A holds bird, transfers to B, **B swaps** | Succeeds — program checks current holder only |
| A transfers to B, then **A tries to swap** | `NotHeld` — A's ATA still exists but holds 0 |
| A and B both submit a swap for the same bird | One lands; the other fails cleanly, no partial state |
| Bird transferred **after** the swap tx is signed but before it lands | Fails; no state change |
| Swap a **remint** back in | No Mapping derives from a remint address → fails |
| Original transferred directly into the vault (not via swap) | Inert; counters unchanged; documented as unrecoverable |

### Listings and delegates

Mock these with plain SPL commands — do not try to use a marketplace. Tensor isn't on
devnet and ME's devnet presence is unreliable.

| Scenario | How to mock | Expected |
|---|---|---|
| Escrow listing (what all 83 real listings are) | Transfer test NFT to a PDA you control | Not in `getAssetsByOwner`; swap fails on ownership |
| Freeze-delegate listing | `spl-token approve` then `spl-token freeze` (you hold freeze authority on devnet mints) | Transfer fails; UI must explain rather than show empty |
| Plain delegate, not frozen (the real 14) | `spl-token approve` alone | **Swaps normally** — owner authority is unaffected, transfer clears the delegate |

The third row matters: a defensive reading would send those 14 holders chasing a revoke
they don't need.

---

## 8. Setup sequence

> **`IMPLEMENTATION_PLAN.md` is authoritative on sequence and gates.** This table is the
> summary; where the two disagree, the plan wins. (An earlier version of this table
> pre-dated the external-review gate and merged deploy with init — a builder following it
> could have initialized before review. That ordering bug is why this note exists.)

| # | Step | Done when |
|---|---|---|
| 1 | Grind program ID, pin toolchain, write program | `declare_id!` matches deploy key; `rust-toolchain.toml` + `Anchor.toml` frozen |
| 2 | Devnet test matrix + full-scale Surfpool rehearsal | Every row passes; the deliberately planted bad pair is caught and corrected |
| 3 | **Three external reviews submitted, all findings closed** | Gates everything irreversible below (plan Phase 5) |
| 4 | Deploy mainnet + verified build. **Do not initialize yet** | Bytecode independently reproducible |
| 5 | `initialize_pool`, then **system-transfer 5.5 SOL to the treasury address** | `expected=1274`, `sealed=false`, treasury ≥4.46 SOL and still system-owned with zero data |
| 6 | Pre-flight re-run, then HxwZ verifies all 1,274 into the collection | Verify list drawn from the just-verified claim map (§6b). **HxwZ still holds the tokens** |
| 7 | Deposit all 1,274 — **319 txs at 4 per tx**, HxwZ co-signs as token owner | `deposited == 1274` |
| 8 | Three-way injective sweep (§6b) | Every pair verified against the claim map. Mismatch is a hard stop |
| 9 | `seal` | Irreversible. Only now can anything swap |
| 10 | Pilot swaps on team wallets, desk not yet announced | Right bird out, `claimed` set, second attempt reverts |
| 11 | Open the desk | Copy matches the program: one signature, SOL dust, one-way |
| 12 | After 20 swaps + 30 quiet days, revisit upgrade authority — burn, Squads, or extend | Decision published |

> **HxwZ stays in service.** The operator retains access to the HxwZ key for as long as the
> desk needs it (decision 2026-08-26). It remains the update authority on the collection
> parent and all 1,274 remints, and a verified creator on each. One hygiene rule still
> holds: after step 7 no scheduled job, worker or automation needs an HxwZ signature —
> verify-upfront removed the recurring certify sweep that would otherwise have kept an
> authority key inside an automated job for the desk's whole life. Keep it that way:
> deposits, corrections and metadata fixes are deliberate, manual signings.
>
> **Why HxwZ cannot stop signing at step 6.** All 1,274 remints are owned by HxwZ today.
> `deposit_bird` transfers them, and an SPL transfer must be signed by the token's owner, so
> HxwZ signs every deposit transaction. An earlier draft had it stop after verification,
> which left no signer able to move the birds and made the rest of this sequence impossible.
>
> The alternative — HxwZ transfers all 1,274 to the admin wallet first — costs an extra ~200
> transactions *and* ~2.5 SOL creating 1,274 admin ATAs that are discarded minutes later.
> Co-signing is strictly cheaper: **64 extra transactions and no extra rent.**

> **The pilot happens after sealing, not before.** An earlier draft of this spec had a
> 5-bird pilot inside the deposit window, which contradicts invariant 3 and constraint 1. Sealing does not
> open the desk — announcing does — so a private pilot on a sealed pool costs nothing and
> keeps the rule absolute.

> **Batch size 5 is at the ceiling** — measured at 1,231 of 1,232 bytes with real
> serialised transactions. Freeze the account set before writing `deposit_bird`; one more
> account per bird drops it to 4 and adds 64 transactions. A ComputeBudget or priority-fee
> instruction does the same — that is the usual way people blow this without noticing.
> Re-measure the moment the instruction grows. Versioned transactions do **not** raise the
> packet cap. Address lookup tables were measured and are a **net loss** here: 17 per tx,
> but 190 setup txs to populate them.

### Measured transaction sizes

| Shape | Per tx | Txs for 1,274 |
|---|---|---|
| `deposit_bird`, admin alone (hypothetical — admin does not own the tokens) | 5 | 255 |
| **`deposit_bird`, HxwZ co-signs as token owner — the real path** | **4** | **319** |
| Merkle — transfer only | 7 | 182 |
| PDA — create mapping only | 8 | 160 |
| PDA via address lookup tables | 17 | 75 + 190 setup = 265 |

> **Transaction v1 (SIMD-0385) is coming and would change these numbers.** It raises the
> per-transaction limit from 1,232 to 4,096 bytes. Checked 2026-08-26: the feature gate
> `txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL` is **absent on both mainnet and devnet**, so
> everything below holds today. Targeted for Agave v4.2, explicitly tentative.
>
> Two consequences. **If it activates before the deposit run**, re-measure — 4,096 bytes takes
> `deposit_bird` from 4 per tx to roughly 13, and 319 transactions to about 95. **And reading
> v1 is breaking whether or not we send it**: once v1 transactions exist on-chain, consumers
> that have not opted in break or, worse, silently misreport. This desk runs for two years, so
> the monitoring in `TEST_PLAN.md` §6 will outlive the current format.

**`swap` measured too** — it is the public instruction, so it matters more than deposit:

| Shape | Bytes | Notes |
|---|---|---|
| One swap, both ATAs created | **670** of 1,232 | 562 bytes of headroom |
| Batched by a holder with several birds | 2 per tx | 1,014 bytes |

Swap is comfortable **on bytes**. The tight one is `deposit_bird` at 1,214/1,232 with two
signers — a ComputeBudget or priority-fee instruction drops it to 3 per tx. Freeze that
account set.

> **Compute units are the other ceiling, and nothing here has measured them.** A first-time
> swap is roughly 75–90k CU; one swap is comfortable against the 200k default, but **two
> batched is marginal at ~170k**. A multi-bird holder hits the compute limit before the byte
> limit. Measure in Level 3 before the frontend offers batching. See appendix §8b.

---

## 9. Why PDA and not merkle

The set is fixed at 1,274 with nothing to add, so merkle's advantages narrow to two:
atomic completeness, and auditor familiarity.

The trade we accepted: PDA costs ~1.86 SOL of Mapping rent and 64 extra setup transactions, and
completeness stops being structural — 1,274 separate writes means a silent failure leaves
one bird unswappable until its owner tries. Mitigated by a full read-back before `seal`,
with `deposited == expected` enforced on-chain.

What it buys: no proof file to host, no ~350 bytes of proof per swap, and a holder can
verify their own pair straight from the chain rather than from a file we publish. Given
this collection died the first time because nobody confirmed that what was uploaded
actually landed, "you don't have to trust our file" carries real weight here.

---

## 10. Constants

| | |
|---|---|
| Program ID | `CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs` |
| Admin (compiled-in `ADMIN`) | `thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7` — pinned at `initialize_pool` |
| Upgrade authority | `birdAyQ1d6UXissFpwx9WcaxvJanzRcMSvzUkQxPpaV` — stays here indefinitely; burn vs Squads deferred |
| Collection parent | `5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc` |
| Custodian (compiled-in `CUSTODIAN`) | `HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB` — deposit source; `fix_mapping` and `recover` destination |
| `expected` | 1274 |
| Redemption contract | `recovered/remint/claim_map_all.json` — frozen; note its `_meta.note` ("collection is verified at claim time") predates the verify-upfront decision and is superseded by Phase 8. The pairing data is what is canonical, not the prose around it |

Keypairs live outside this repo in `~/.thugbirdz-keys/swap/`. None are funded or used yet.

---

## Open questions

Things we would genuinely like challenged:

1. ~~**`unlock_ts` — 2 years or 4?**~~ **Settled: 2 years**, by the operator, on evidence the
   reviewers did not have.

   Reviewers argued 4, but all of them were reasoning from a figure this document got wrong:
   that the previous swap round ran two years. **It ran over three**, with active outreach
   pushing holders to claim, and still had non-claimers — people who left Solana or simply
   did not care. A fourth year does not lengthen that tail; it was already exhausted.

   The 83 originals in marketplace escrow reinforce it: no marketplace carries a collection
   for the 2021 thugbirdz, so those have sat as unverified, unwatched listings for three
   years. They are abandoned inventory, not holders waiting for a deadline.

   **Condition:** the exact unix timestamp is published on the desk the day it opens and is
   never moved. A fixed, stated date is what makes two years read as a term rather than a
   countdown.
2. **Is read-back-before-seal genuinely sufficient**, or is there a way to make the PDA set
   atomically verifiable? `deposited == expected` on-chain is the cheap version. Better ideas?
3. **Is `sealed` the right closure mechanism**, and does "admin can create mappings until
   sealed" read as an acceptable window to an auditor?
4. **Merging mapping and receipt into one account** — any ordering or failure modes we're
   not seeing?
5. **1,274 permanent rent-exempt accounts that are never closed** — any objection beyond
   the rent?
6. ~~**Anything that pushes the batch over 1,232 bytes?**~~ **Measured.** Real path is 4 per tx
   at 1,214 bytes with two signers; swap is 670 bytes with 562 spare. A ComputeBudget or
   priority-fee instruction on `deposit_bird` drops it to 3. Remaining question: anything else that we haven't accounted
   for — compute budget instructions, priority fees, a larger account set than assumed?
7. ~~**Squads as waypoint or destination?**~~ **Deferred by decision.** Upgrade authority
   stays on `birdAyQ1d6UX…` indefinitely; burn vs Squads is revisited at 20 public swaps +
   30 quiet days. Say it publicly rather than leaving people to find it: a single keypair can
   replace the code guarding 1,274 NFTs until that changes.

---

*Nothing is deployed. Nothing is sunk. If a decision here is wrong, now is the cheap time
to say so.*
