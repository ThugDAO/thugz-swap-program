use anchor_lang::prelude::*;

/// Singleton, seeds `["pool"]`. 8 discriminator + 85 = 93 bytes.
///
/// Carries data, and therefore legally CANNOT fund `create_account` — the
/// system-owned, zero-data `["treasury"]` PDA pays all rent instead. The treasury
/// is never `init`/`allocate`/`assign`-ed; only its bump is recorded here.
#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub admin: Pubkey,      // pinned to the ADMIN constant at init
    pub collection: Pubkey, // parent collection of the remints (recorded, not enforced)
    pub expected: u16,      // from the EXPECTED constant, never instruction data
    pub deposited: u16,
    pub swapped: u16,
    pub recovered: u16, // vault reconciliation: in-vault == expected - swapped - recovered
    pub sealed: bool,   // one-way; gates swap on one side, deposit/fix on the other
    pub paused: bool,   // stops swaps only
    pub unlock_ts: i64, // unix seconds; before it nothing leaves except swap / pre-seal fix
    pub bump: u8,
    pub vault_bump: u8,    // every PDA-signed token CPI uses this
    pub treasury_bump: u8, // the rent payer signs with this
}

/// One per original, seeds `["map", pool, old_mint]`. 8 discriminator + 75 = 83 bytes.
///
/// Both the pairing and the receipt — existence IS eligibility. Created only inside
/// `deposit_bird`, via manual allocate/assign (never Anchor `init`, which aborts on a
/// pre-funded address — the lamport-griefing vector). Init-once: the only mutation
/// anywhere is `claimed` flipping true inside `swap`, and the only close is
/// `fix_mapping` while `!sealed`.
#[account]
#[derive(InitSpace)]
pub struct Mapping {
    pub new_mint: Pubkey,
    pub claimed: bool,
    pub claimed_by: Pubkey,
    pub claimed_at: i64,
    pub recovered: bool, // set by `recover`; makes recover idempotent without touching `claimed`
    pub bump: u8,
}
