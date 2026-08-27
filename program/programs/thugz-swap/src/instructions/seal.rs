use anchor_lang::prelude::*;

use crate::constants::*;
use crate::error::SwapError;
use crate::events::PoolSealed;
use crate::state::Pool;

#[derive(Accounts)]
pub struct Seal<'info> {
    pub admin: Signer<'info>,

    #[account(mut, seeds = [POOL_SEED], bump = pool.bump, has_one = admin)]
    pub pool: Box<Account<'info, Pool>>,
}

pub fn handle_seal(ctx: Context<Seal>) -> Result<()> {
    let pool = &mut ctx.accounts.pool;
    // One-way: `sealed` never returns to false, and a second seal fails loudly.
    require!(!pool.sealed, SwapError::Sealed);
    // A count is not a commitment — the off-chain three-way sweep is the real check —
    // but arity is enforced here: an incomplete correction stalls the launch instead
    // of sealing a hole.
    require!(pool.deposited == pool.expected, SwapError::Incomplete);

    pool.sealed = true;

    emit!(PoolSealed {
        expected: pool.expected,
        ts: Clock::get()?.unix_timestamp,
    });
    Ok(())
}
