// ============================================================
// Program: Thugz Swap
// Framework: Anchor 1.1.2 (pinned — IMPLEMENTATION_APPENDIX.md §1)
// Testing:   LiteSVM (TEST_PLAN.md Level 1)
// Risk Level: 🔴 Critical — NFT vault custody, admin key, irreversible seal
// Spec: SWAP_SPEC.md + IMPLEMENTATION_APPENDIX.md (appendix wins on conflict)
// Security: see program/SECURITY_CHECKLIST.md
// ============================================================
//
// A one-way redemption desk: a holder surrenders a 2021 original and receives its
// reminted twin in the same transaction. 1,274 fixed pairs; pairing is loaded by the
// admin pre-seal, verified off-chain by the sweep, then sealed forever. Originals
// lock in the vault permanently. No instruction can deliver a token to the admin.

pub mod constants;
pub mod error;
pub mod events;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs");

#[program]
pub mod thugz_swap {
    use super::*;

    /// Creates the Pool singleton. `expected` comes from the compiled `EXPECTED`
    /// constant and `admin` is pinned to the compiled `ADMIN` constant (front-run
    /// guard on the singleton seeds). Records the vault and treasury bumps.
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        unlock_ts: i64,
        collection: Pubkey,
    ) -> Result<()> {
        instructions::initialize_pool::handle_initialize_pool(ctx, unlock_ts, collection)
    }

    /// Admin + custodian: moves one remint into the vault AND creates its Mapping in
    /// the same transaction. `new_mint` is read from the token account being moved;
    /// only `old_mint` comes from instruction data (the sweep catches a wrong value
    /// before seal). Fails once sealed.
    pub fn deposit_bird(ctx: Context<DepositBird>, old_mint: Pubkey) -> Result<()> {
        instructions::deposit_bird::handle_deposit_bird(ctx, old_mint)
    }

    /// Admin, only while `!sealed`: closes a Mapping (rent to the treasury) and
    /// returns the remint to the custodian's ATA — never to the admin.
    pub fn fix_mapping(ctx: Context<FixMapping>, old_mint: Pubkey) -> Result<()> {
        instructions::fix_mapping::handle_fix_mapping(ctx, old_mint)
    }

    /// Admin, one-way. Requires `deposited == expected`. After this no Mapping can be
    /// created, altered, or closed — and only after this can anything swap.
    pub fn seal(ctx: Context<Seal>) -> Result<()> {
        instructions::seal::handle_seal(ctx)
    }

    /// The only instruction a holder calls. Requires `sealed && !paused`.
    pub fn swap(ctx: Context<Swap>) -> Result<()> {
        instructions::swap::handle_swap(ctx)
    }

    /// Admin safety valve. Stops swaps only; cannot move anything.
    pub fn set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
        instructions::set_paused::handle_set_paused(ctx, paused)
    }

    /// Admin, only after `unlock_ts`: moves one unclaimed remint out of the vault to
    /// the custodian's ATA. Never touches deposited originals; leaves `claimed`
    /// false so a late holder fails atomically instead of losing their original.
    pub fn recover(ctx: Context<Recover>, old_mint: Pubkey) -> Result<()> {
        instructions::recover::handle_recover(ctx, old_mint)
    }
}
