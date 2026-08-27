use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::SwapError;
use crate::state::Pool;

#[derive(Accounts)]
pub struct InitializePool<'info> {
    /// Pinned to the compiled ADMIN constant: the `["pool"]` seeds are a singleton
    /// and the program ID is public before deployment, so an unconstrained init is
    /// front-runnable — an attacker who initializes first captures the admin seat
    /// permanently (the PDA can never be re-derived under another key).
    #[account(mut, constraint = admin.key() == ADMIN)]
    pub admin: Signer<'info>,

    /// Admin pays the Pool's own rent. Everything the PROGRAM creates later is paid
    /// by the treasury PDA — Pool carries data and legally cannot fund
    /// `create_account`.
    #[account(
        init,
        payer = admin,
        space = 8 + Pool::INIT_SPACE,
        seeds = [POOL_SEED],
        bump
    )]
    pub pool: Box<Account<'info, Pool>>,

    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_pool(ctx: Context<InitializePool>, unlock_ts: i64, collection: Pubkey) -> Result<()> {
    // Sentinel guard: unlock_ts feeds `now >= unlock_ts` comparisons for two years.
    // A zero or past value would make `recover` live immediately.
    let now = Clock::get()?.unix_timestamp;
    require!(unlock_ts > now, SwapError::InvalidUnlockTimestamp);

    // Canonical bumps found once, stored, and reused on every later derivation.
    // The treasury account itself is deliberately NOT touched here: it must stay
    // system-owned with zero data (funded by a plain transfer to the derived
    // address), or it dies as a legal `create_account` payer.
    let (_, vault_bump) = Pubkey::find_program_address(&[VAULT_SEED], &crate::ID);
    let (_, treasury_bump) = Pubkey::find_program_address(&[TREASURY_SEED], &crate::ID);

    let pool = &mut ctx.accounts.pool;
    pool.admin = ADMIN;
    pool.collection = collection;
    pool.expected = EXPECTED; // compiled constant — never instruction data
    pool.deposited = 0;
    pool.swapped = 0;
    pool.recovered = 0;
    pool.sealed = false;
    pool.paused = false;
    pool.unlock_ts = unlock_ts;
    pool.bump = ctx.bumps.pool;
    pool.vault_bump = vault_bump;
    pool.treasury_bump = treasury_bump;

    Ok(())
}
