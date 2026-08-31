# Thugz Swap — implementation appendix

Everything `SWAP_SPEC.md` leaves for a builder to invent. The spec says *what* and *why*;
this says *how*, so decisions get implemented rather than improvised.

Checked against the Solana Foundation `solana-dev` skill, 2026-08-26.

---

## 1. Toolchain — do NOT pin "latest stable"

`IMPLEMENTATION_PLAN.md` Phase 1 originally said pin whatever is latest stable that day.
**That is wrong and would have produced an untested combination.**

Anchor 1.1.x still pins the **`solana-*` 3.x crate line** and its CI installs **Solana CLI
3.1.10**. Agave validator releases (4.x) are versioned independently of the SDK crates. This
machine has CLI 4.1.1 — newer than the tested pairing, not better for this purpose.

| Component | Pin | Note |
|---|---|---|
| Anchor | **1.1.2** | Latest 1.1.x |
| Solana CLI | **3.1.10** | Anchor 1.1.x CI-tested pairing — *not* 4.1.x |
| `solana-*` crates | **^3** | Stay on 3.x until Anchor moves |
| Rust | MSRV **1.89** | Machine has 1.94.1 ✓ |
| Platform tools | v1.52+ | Ships with CLI 3.x |
| Node | ≥ **20.18** | Machine has 22.22 ✓ |
| GLIBC | ≥ **2.39** | Machine has 2.42 ✓ |

**Commit `Cargo.lock`.** Platform-tools versions below v1.52 bundle a cargo that cannot
build `edition = "2024"`, and several transitive crates now require it. A committed lockfile
is what stops that appearing months later on a different machine.

Agent-run builds: prefix `NO_DNA=1` (`NO_DNA=1 anchor build`, `NO_DNA=1 anchor test`).

### Verifiable builds — adopt this

Anchor 1.1.x supports `verifiedBuild` via OtterSec (`verify.osec.io`). Gate 6 currently
hand-rolls "on-chain hash matches a local build".

Use the standard mechanism instead. It makes the program **publicly verifiable by anyone**,
not just by us — which is the same argument that chose PDAs over merkle. A holder who can
check the pairing from chain should also be able to check that the deployed bytecode matches
the reviewed source.

---

## 2. Accounts

```rust
#[account]
pub struct Pool {
    pub admin: Pubkey,        // 32
    pub collection: Pubkey,   // 32
    pub expected: u16,        //  2
    pub deposited: u16,       //  2
    pub swapped: u16,         //  2
    pub recovered: u16,       //  2   <- lets monitoring reconcile the vault post-unlock
    pub sealed: bool,         //  1
    pub paused: bool,         //  1
    pub unlock_ts: i64,       //  8
    pub bump: u8,             //  1
    pub vault_bump: u8,       //  1   <- used by every PDA-signed CPI
    pub treasury_bump: u8,    //  1   <- rent payer signs with this
}
// 8 discriminator + 85 = 93

#[account]
pub struct Mapping {
    pub new_mint: Pubkey,     // 32
    pub claimed: bool,        //  1
    pub claimed_by: Pubkey,   // 32
    pub claimed_at: i64,      //  8
    pub recovered: bool,      //  1   <- recover-once guard (2026-08-27 audit)
    pub bump: u8,             //  1
}
// 8 discriminator + 75 = 83  (claimed stays at offset 40 — indexers unaffected)
```

`Mapping` at 83 bytes is **0.001469 SOL** each — rent-exempt is `(128 + len) × 3480 × 2`.

**The rent budget was wrong by roughly 5×** until a Codex review caught it. What was missing:
`deposit_bird` has to create a **vault ATA per remint**, and no document accounted for those.

| What | Count | Each | Total |
|---|---|---|---|
| Mapping accounts | 1,274 | 0.001462 | 1.86 SOL |
| Vault remint ATAs | 1,274 | 0.002039 | 2.60 SOL |
| Swap ATAs (2 each) | up to 1,274 | 0.004078 | 5.20 SOL |
| | | | **9.66 SOL** |

Before the deposit run: **4.46 SOL**. Opening balance: **5.5 SOL**, topped up from the
watermark.

### The rent payer cannot be `Pool`

`system_program::create_account` requires its funding account to be **system-owned with no
data**. `Pool` carries data, so it can never be the payer for Mapping init or ATA creation.

Use a separate **`["treasury"]` PDA, system-owned, zero data**, which signs as payer via
seeds. Closed-account rent returns there. An earlier draft had `Pool` paying and would have
failed on the first deposit and the first swap.

### Seeds

```rust
pub const POOL_SEED:  &[u8] = b"pool";
pub const VAULT_SEED: &[u8] = b"vault";
pub const MAP_SEED:   &[u8] = b"map";
pub const TREASURY_SEED: &[u8] = b"treasury";

// Deposit source owner; fix_mapping and recover destination. A compile-time constant,
// deliberately — an init-time field could be mis-set to admin, which would put admin
// back in the destination seat. The verified build pins this to the reviewed source.
pub const CUSTODIAN: Pubkey = pubkey!("HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB");

// Same reasoning: initialize_pool sets pool.expected from this constant, never from
// instruction data. A wrong expected is permanent and silently changes seal behavior.
pub const EXPECTED: u16 = 1274;

// The initializer must BE this key. The pool seeds are a singleton and the program ID
// is public before deployment, so an unconstrained initialize_pool is front-runnable —
// an attacker who initializes first captures the admin seat permanently (the PDA can
// never be re-derived under another key for this program id).
pub const ADMIN: Pubkey = pubkey!("thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7");

// Pool    : [POOL_SEED]
// Vault   : [VAULT_SEED]
// Treasury: [TREASURY_SEED]        system-owned, no data, pays all rent
// Mapping : [MAP_SEED, pool.key().as_ref(), old_mint.as_ref()]
```

Store the canonical bump in each account and use the stored bump on every subsequent
derivation. Never accept a caller-supplied bump.

### The `test-keys` feature

The LiteSVM suite must sign as admin and custodian, and nobody ships those private
keys. A `test-keys` cargo feature swaps ADMIN/CUSTODIAN for committed fixture keypairs
(`program/programs/thugz-swap/tests/fixtures/`) and scales `EXPECTED` to 20, so devnet —
which cannot forge account state the way LiteSVM can — reaches a REAL seal with mock
birds. Every other constant is identical. **The mainnet artifact is the default-features build**, pinned
by the Gate 6 verified build, and `program/scripts/verify_mainnet_artifact.py` is run
before any deploy (it checks the bytecode for the real custodian and the IDL constants
block, and refuses a test-keys artifact). All three ground constants are exported to
the IDL via `#[constant]`.

---

## 3. Errors

```rust
#[error_code]
pub enum SwapError {
    #[msg("Pool is not sealed yet")]                       NotSealed,
    #[msg("Pool is already sealed")]                       Sealed,
    #[msg("Swaps are paused")]                             Paused,
    #[msg("This original has already been swapped")]       AlreadyClaimed,
    #[msg("Vault account does not hold the mapped remint")] WrongRemint,
    #[msg("You do not own this original")]                 NotOwner,
    #[msg("Token account does not hold exactly one token")] NotHeld,
    #[msg("Deposited count does not match expected")]      Incomplete,
    #[msg("Unlock timestamp has not passed")]              Locked,
    #[msg("This account is not recoverable")]              NotRecoverable,
    #[msg("Custodian constraint violated")]                NotCustodian,
    #[msg("Duplicate account supplied")]                   DuplicateAccount,
    #[msg("A mapping for this original already exists")]   MappingExists,
    #[msg("Unlock timestamp must be in the future")]       InvalidUnlockTimestamp,
    #[msg("Arithmetic overflow or underflow")]             Arithmetic,
    #[msg("This remint was recovered by the custodian after the unlock window")] Recovered,
    #[msg("Only legacy SPL Token mints are accepted")]      LegacyTokenOnly,
    // DuplicateAccount above is reserved/unused (Anchor rejects dup mutables natively);
    // kept in place so later error codes stay stable.
}
```

Every row of the `SWAP_SPEC.md` failure matrix maps to one of these. Tests assert the
specific variant, never just "it reverted".

---

## 4. Anchor patterns — three things to get right

### Never `init_if_needed` on `Mapping`

It permits reinitialization attacks. `Mapping` is init-once, and an attempt to create one that
already exists must fail — that failure *is* the constraint.

**Do not read this as "use Anchor `init`".** See *Mapping init — manual, not Anchor `init`*
below: the account is created manually, and the existence check is explicit. This subsection is
about never accepting a second initialization, not about which mechanism creates the first.

**This is different from `create_idempotent` on ATAs**, which is the correct and recommended
pattern for token accounts and is not affected by this warning. Both appear in `swap`; they
are not the same mechanism.

### Mapping init — manual, not Anchor `init`

The spec says `init mapping`; this section says manual allocate/assign. **They are different
implementations and only one can be built.** Build the manual one, for two independent reasons:

1. Anchor's `init` takes a `payer` that must be system-owned with no data. Our payer is the
   treasury PDA, which qualifies — but `init` also fails outright on a pre-funded address.
2. Mapping addresses are derivable the moment the program ID is public, so pre-funding is
   reachable, not theoretical.

Take `UncheckedAccount`, require system-owned with zero data, transfer the deficit, allocate,
assign, then write the discriminator and every field explicitly. Test the pre-funded case.

### ATA creation — explicit CPI, not an Anchor constraint

Anchor's `init` / `init_if_needed` account constraints generate a CPI that signs for the
account being *created*, **not for the payer**. Our payer is the treasury PDA, which can
only sign via `invoke_signed`. So every ATA this program creates — both in `swap`, the
vault ATA in `deposit_bird`, the destination ATAs in `fix_mapping` and `recover` — must be
an explicit `associated_token::create_idempotent` CPI built with
`CpiContext::new_with_signer` and the `["treasury"]` seeds. An `associated_token` init
constraint with `payer = treasury` fails at runtime with a missing-signature error.

### Lamport griefing — the pattern

```rust
let required = Rent::get()?.minimum_balance(space);
let existing = mapping.lamports();
if existing < required {
    // transfer only (required - existing) from the TREASURY PDA
}
// then Allocate + Assign with signer seeds
```

Mapping addresses are derivable the moment the program ID is public, so treat this as
reachable, not theoretical. It lands on the admin during the deposit run, not on holders.

### Closing in `fix_mapping`

Rent returns to the **treasury**, not the pool and not the admin. Note that runtime `close`
*can* credit a data-bearing account — it is not `create_account` — so `close = pool` would
compile happily and silently disagree with the payer model. Pay and refund through treasury so
there is one story.

The remint itself goes to **HxwZ's ATA**, never admin's — see spec §4a.

Be aware of **revival**: a closed account can be restored within the same transaction by
refunding its lamports. `fix_mapping` is admin-only and pre-seal, so the exposure is small,
but do not rely on "it was closed" as a security property inside a multi-instruction
transaction.

---

## 5. Token transfers — use `transfer_checked`

The spec pseudocode says `transfer`. Use **`anchor_spl::token_interface::transfer_checked`**.

All 1,274 remints are verified legacy SPL Token, so plain `transfer` would work *today*. But
`transfer_checked` costs **nothing extra here** — it needs the mint and decimals, and both
mints are already in our account set for ATA derivation. It is also the only thing that works
if any token in this flow ever touches Token-2022.

Free correctness. Take it.

### PDA-signed CPI

```rust
let seeds = &[VAULT_SEED, &[ctx.accounts.pool.vault_bump]];   // stored in Pool, see §2
let signer = &[&seeds[..]];
let cpi = CpiContext::new_with_signer(token_program_id, accounts, signer);
```

Note for Anchor v1: `CpiContext::new` takes a **`Pubkey`**, not an `AccountInfo`. The program
account no longer belongs in the accounts struct.

---

## 6. Duplicate mutable accounts

Anchor 1.0+ **disallows duplicate mutable accounts by default** and adds a `dup` constraint
for intentional cases.

`swap` passes four writable token accounts. They cannot collide by construction — different
owners, different mints — but the check is on the security checklist and Anchor now enforces
it. Do not annotate anything here with `dup`; if two of them are ever equal, something is
wrong and the transaction should fail.

---

## 7. Events

Defined here and in spec §5b — the two lists must stay identical; this one carries the full
field types. `TEST_PLAN.md` §6 assumes swaps are observable; without these, monitoring means
scraping logs forever.

```rust
#[event] pub struct BirdDeposited { pub old_mint: Pubkey, pub new_mint: Pubkey, pub deposited: u16 }
#[event] pub struct MappingFixed  { pub old_mint: Pubkey, pub new_mint: Pubkey, pub deposited: u16 }
#[event] pub struct PoolSealed    { pub expected: u16, pub ts: i64 }
#[event] pub struct BirdSwapped   { pub old_mint: Pubkey, pub new_mint: Pubkey,
                                    pub holder: Pubkey, pub ts: i64, pub swapped: u16 }
#[event] pub struct PauseSet      { pub paused: bool, pub ts: i64 }
#[event] pub struct BirdRecovered { pub new_mint: Pubkey, pub ts: i64, pub recovered: u16 }
```

`BirdSwapped` is the one the monitoring job actually needs — it gives a swap count that can
be reconciled against `pool.swapped` without parsing instruction data.

---

## 8. Reading transactions — set `maxSupportedTransactionVersion: 1`

Every `getTransaction`, `getBlock` and `blockSubscribe` call in the monitoring job and the
frontend must pass `maxSupportedTransactionVersion: 1`.

Transaction v1 is not active yet (checked: gate absent on mainnet and devnet), but **reading
v1 is breaking and is not opt-in**. A consumer that has not adapted breaks, or silently
misreports, the moment v1 transactions land. This desk is expected to run two years.

Set it now. It costs one parameter and removes a dated time-bomb.

---

## 8b. Compute units — measured in bytes, never in CU

Every size figure in these documents is **bytes**. Nothing anywhere measures **compute
units**, and for `swap` that is the binding constraint before size is.

Rough shape of a first-time swap: two `create_idempotent` ATAs at roughly 20–25k CU each
when they actually create, two `transfer_checked` at ~6–9k, plus Anchor account validation,
PDA derivations and account writes. Call it **75–90k CU**.

| Case | Estimated CU | Against the 200k default |
|---|---|---|
| One swap, both ATAs created | ~85k | comfortable |
| One swap, ATAs already exist | ~45k | comfortable |
| **Two swaps batched** | **~170k** | **marginal** |

So a holder with several birds hits the compute ceiling before the byte ceiling — the byte
measurement said two per transaction fit at 1,014 bytes, but two may not fit in 200k CU.

**Action:** measure real CU in Level 3 and record it. If batching is offered at all, the
frontend must add an explicit `ComputeBudget` request. That costs ~45 bytes, which the swap
path can absorb (1,014 + 45 well under 1,232) — unlike `deposit_bird`, where the same
instruction drops the batch from 4 to 3.

> **On transaction v1 this changes shape.** The four compute-budget values move out of
> `ComputeBudget` instructions and into the message config — and **unset limits are zero**,
> not defaulted. A v1-aware client must set them explicitly or the transaction fails for a
> reason that looks nothing like a compute problem.

---

## 9. The sweep script

The most safety-critical off-chain code in the project, and the spec describes only its
behaviour. It runs immediately before an irreversible step.

```
load claim_map_all.json                       # 1,274 pairs, injective (verified)
assert len == 1274 and olds unique and news unique and olds ∩ news == ∅

for each (old_mint, new_mint) in the map:
    derive  map_pda = PDA([MAP_SEED, pool, old_mint])
    fetch   account                            # batched getMultipleAccounts, 100 per call
    assert  exists
    assert  decoded.new_mint == new_mint
    assert  decoded.claimed == false
    derive  vault_ata = ATA(vault_pda, new_mint)
    assert  vault_ata holds exactly 1 of new_mint

assert  every decoded.new_mint appears exactly once   # injective on-chain, not just in the file
assert  pool.deposited == 1274

# ANCHOR THE SET — which remints are in play is not the claim map's to define
fetch   members of verified collection 5Kwhy… via DAS getAssetsByGroup (minus the parent)
assert  that set == the claim map's new_mints, exactly   # no extras, no gaps

# CROSS-CHECK AGAINST ARWEAVE — the one record the admin cannot rewrite
for each new_mint:
    assert  its on-chain metadata uri matches ^https://arweave.net/<txid>$  # immutable host only
    fetch   the Arweave JSON
    assert  json.properties.provenance.original_mint == old_mint
    assert  json.name == remint on-chain name == original on-chain name
            # the Arweave name is frozen even though on-chain metadata is mutable —
            # it is what makes the name check independent of the update authority
    record  remint is_mutable + update_authority           # published with the report

# TAG CHECK — the indexed Original-Mint tag, until now asserted but never verified
fetch   tags of each metadata tx via arweave graphql (batched — the gateway caps `ids` at 9 per query)
assert  tag "Original-Mint" == old_mint for every pair

# FREEZE CHECK — a frozen destination ATA would strand fix_mapping/recover
assert  every remint's freeze authority == its own master edition PDA
        # the standard Metaplex shape: freezing then requires the destination owner's
        # own delegation. What must not exist is a foreign freeze key.

emit    a signed report: every pair, every PDA, the pool state, a timestamp
```

### Why the Arweave cross-check matters more than the rest of the sweep

Every remint's Arweave metadata already carries `provenance.original_mint`, and the same
pairing is written as an indexed, immutable Arweave tag (`Original-Mint`) on both the image
and metadata transactions. That happened at mint time, months before any of this, and
**cannot be altered by anyone, including us**.

Without this check the sweep compares two things we control — `claim_map_all.json` and the
Mapping PDAs we just wrote from it. A corrupted or tampered claim map produces a sweep that
passes perfectly. The Arweave provenance is the only independent witness, and it is precisely
the detector for the bad-`old_mint` deposit the sweep exists to catch.

It costs 1,274 HTTP fetches, cacheable, run once. Do not skip it because the rest of the
sweep passed.

### What the Arweave check cannot see — and what the name check adds

The tag was written by the remint pipeline. If the pairing was wrong *at mint time* — wrong
tag and wrong claim map together, from the same bug — tag and map agree and the Arweave
check passes clean. This matters because the set has two pedigrees: phase 1's 541 birds went
through `build_remint_manifest.py` and its ten cross-checks; **phase 2's 733 went through a
different pipeline with none of them**.

The name check closes it. The original's on-chain `name` has been immutable since 2021 and
owes nothing to either pipeline; the remint's name came from the recovered art bundle. A
mint-time mis-pairing puts a different bird's name on the remint than on the original it is
tagged with. Two on-chain reads, no trust in any file of ours.

Residual after the name check: a remint whose name is right but whose *image* is the wrong
bird's. Name and image travel together through both pipelines, so this needs the bundle
itself to be internally wrong — cover it with a human pass: eyeball a random sample (≥30,
weighted toward the 733) against the recovered art during the Phase 4 rehearsal.

### What "independent" actually rests on

Two of the sweep's anchors are weaker than they look, and the design must say so:

- **The collection-membership anchor is downstream of Phase 8.** HxwZ creates that
  membership *from the claim map*, so membership agreeing with the map proves nothing by
  itself. It is independent only because Phase 8 is gated on the Arweave-anchored
  pre-flight passing first. Keep that ordering.
- **On-chain names are rewritable.** The remints are mutable with HxwZ as update
  authority. The name check binds through the Arweave JSON's `name`, which was frozen at
  mint — that copy, not the on-chain field, is the witness.

Threat model, stated plainly: these checks defend against pipeline bugs and a corrupted
claim map, not against a fully malicious custodian — an HxwZ holder can rewrite names,
verify fake birds into the collection, and already holds every remint outright. That party
is the operator. The upgrade-authority disclosure in the spec covers the same trust
position.

Requirements:

- **Reads only.** It must not be able to write anything.
- **Fails closed.** Any RPC error, Arweave/HTTP error, timeout or malformed response is a
  failure, never a skip — retry transient errors, then stop. A silent skip is the exact
  failure that killed the 2021 collection. (Retrying may fetch the same txid through a
  second gateway — content is txid-addressed, so the bytes are the same record; the
  arweave.net edge has been seen caching an error page for individual txids. The on-chain
  URI itself must still be `arweave.net`.)
- **Independently runnable.** A reviewer should be able to run it against mainnet and get the
  same report without our machine.
- **Publish the output.** The report plus `seal` is the public commitment to the set — it is
  what replaces a merkle root.

---

## 10. The deposit runner

- Idempotent from a state file keyed by `old_mint`, the same shape as the mint runs.
- Records the signature per bird; a resumed run re-checks chain state rather than trusting
  the file alone.
- 4 per transaction with two signers (admin + HxwZ). **Re-measure before the run** — if
  transaction v1 has activated, the limit is 4,096 bytes rather than 1,232.
- No ComputeBudget or priority-fee instruction without re-measuring: at 1,214 of 1,232 bytes
  either one drops the batch to 3.

---

## 11. Testing stack — our plan is out of date

`TEST_PLAN.md` says "Anchor tests, local validator". Anchor 1.0+ changed this:

| Level | Tool |
|---|---|
| Unit | **LiteSVM** (scaffolded by `anchor init`) or **Mollusk** |
| Integration | **Surfpool** — now the default `anchor test` runner |
| Full-fidelity | `solana-test-validator`, only where Surfpool's emulation is insufficient |

**Surfpool forks mainnet with lazy account cloning.** That is worth more here than the plan
assumed: Level 4 can run the full-scale rehearsal against **forked real state** — the actual
1,274 remints, the actual HxwZ ownership, the actual 83 escrowed originals — without touching
mainnet or minting 1,274 mock pairs.

The rehearsal gets closer to the real thing and cheaper at the same time. Cheatcodes also let
you time-travel past `unlock_ts` to test `recover`, which otherwise needs a two-year wait or a
throwaway pool with a fake timestamp.
