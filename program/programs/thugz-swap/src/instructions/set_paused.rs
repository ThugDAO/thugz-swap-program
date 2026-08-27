use anchor_lang::prelude::*;

use crate::constants::*;
use crate::events::PauseSet;
use crate::state::Pool;

#[derive(Accounts)]
pub struct SetPaused<'info> {
    pub admin: Signer<'info>,

    #[account(mut, seeds = [POOL_SEED], bump = pool.bump, has_one = admin)]
    pub pool: Box<Account<'info, Pool>>,
}

/// Stops swaps only. Cannot move anything, cannot touch mappings, and is the one
/// admin power that keeps working after seal — by design, as the safety valve.
pub fn handle_set_paused(ctx: Context<SetPaused>, paused: bool) -> Result<()> {
    ctx.accounts.pool.paused = paused;

    emit!(PauseSet {
        paused,
        ts: Clock::get()?.unix_timestamp,
    });
    Ok(())
}
