use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::constants::*;
use crate::error::SwapError;
use crate::events::BirdRecovered;
use crate::instructions::{create_ata_idempotent_treasury_pays, require_canonical_ata};
use crate::state::{Mapping, Pool};

#[derive(Accounts)]
#[instruction(old_mint: Pubkey)]
pub struct Recover<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut, seeds = [POOL_SEED], bump = pool.bump, has_one = admin)]
    pub pool: Box<Account<'info, Pool>>,

    /// Read-only, deliberately: `claimed` stays false so a late holder's swap fails
    /// atomically on the empty vault account and they keep their original. Flipping
    /// it to "tidy up" would strand a real 2021 bird in a dead mapping.
    #[account(
        mut,
        seeds = [MAP_SEED, pool.key().as_ref(), old_mint.as_ref()],
        bump = mapping.bump,
    )]
    pub mapping: Box<Account<'info, Mapping>>,

    #[account(constraint = new_mint.key() == mapping.new_mint @ SwapError::NotRecoverable)]
    pub new_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: The vault authority PDA; signs the outbound transfer via seeds.
    #[account(seeds = [VAULT_SEED], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,

    /// `vault_ata.mint == mapping.new_mint` is the on-chain guard that recovery can
    /// only touch an account whose mint a Mapping names as a REMINT. Because the
    /// claim map is injective with no address on both sides (verified before seal),
    /// no original can ever satisfy it. Do not weaken this on the assumption the
    /// sweep carries it.
    #[account(
        mut,
        constraint = vault_ata.mint == mapping.new_mint @ SwapError::NotRecoverable,
        constraint = vault_ata.owner == vault.key() @ SwapError::NotRecoverable,
    )]
    pub vault_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: The custodian wallet (compiled constant). Same rule as `fix_mapping`:
    /// admin triggers recovery; admin never receives the token.
    #[account(constraint = custodian.key() == CUSTODIAN @ SwapError::NotCustodian)]
    pub custodian: UncheckedAccount<'info>,

    /// CHECK: ATA(CUSTODIAN, new_mint); derivation asserted in the handler, created
    /// idempotently (rent: treasury).
    #[account(mut)]
    pub custodian_ata: UncheckedAccount<'info>,

    #[account(mut, seeds = [TREASURY_SEED], bump = pool.treasury_bump)]
    pub treasury: SystemAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_recover(ctx: Context<Recover>, _old_mint: Pubkey) -> Result<()> {
    // Sealed: recover is a post-open operation. Without this gate, an admin who set a
    // short unlock_ts could pull remints out of an unsealed vault and then seal a desk
    // with holes (deposited stays at expected while the vault is short).
    require!(ctx.accounts.pool.sealed, SwapError::NotSealed);
    let now = Clock::get()?.unix_timestamp;
    require!(now >= ctx.accounts.pool.unlock_ts, SwapError::Locked);
    require!(!ctx.accounts.mapping.claimed, SwapError::AlreadyClaimed);
    // Recover-once per mapping: `claimed` must stay false (spec §4b — a late swap must
    // still fail atomically and leave the holder their original), so idempotency rides
    // its own flag. Without it, returning a recovered remint to the vault would let
    // recover run twice and inflate Pool.recovered, breaking the reconciliation invariant.
    require!(!ctx.accounts.mapping.recovered, SwapError::AlreadyClaimed);
    require!(ctx.accounts.vault_ata.amount == 1, SwapError::NotHeld);

    let new_mint_key = ctx.accounts.new_mint.key();

    require_canonical_ata(
        &ctx.accounts.custodian_ata.key(),
        &CUSTODIAN,
        &new_mint_key,
        &ctx.accounts.token_program.key(),
        SwapError::NotCustodian,
    )?;
    create_ata_idempotent_treasury_pays(
        &ctx.accounts.treasury,
        ctx.accounts.pool.treasury_bump,
        &ctx.accounts.custodian_ata,
        ctx.accounts.custodian.to_account_info(),
        ctx.accounts.new_mint.to_account_info(),
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
    )?;

    let vault_seeds: &[&[u8]] = &[VAULT_SEED, &[ctx.accounts.pool.vault_bump]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault_ata.to_account_info(),
                mint: ctx.accounts.new_mint.to_account_info(),
                to: ctx.accounts.custodian_ata.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            &[vault_seeds],
        ),
        1,
        ctx.accounts.new_mint.decimals,
    )?;

    ctx.accounts.mapping.recovered = true;
    let pool = &mut ctx.accounts.pool;
    pool.recovered = pool.recovered.checked_add(1).ok_or(SwapError::Arithmetic)?;

    emit!(BirdRecovered {
        new_mint: new_mint_key,
        ts: now,
        recovered: pool.recovered,
    });
    Ok(())
}
