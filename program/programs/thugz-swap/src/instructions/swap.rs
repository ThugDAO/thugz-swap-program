use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::constants::*;
use crate::error::SwapError;
use crate::events::BirdSwapped;
use crate::instructions::{create_ata_idempotent_treasury_pays, require_canonical_ata};
use crate::state::{Mapping, Pool};

#[derive(Accounts)]
pub struct Swap<'info> {
    /// The holder signs and pays the network fee. Nobody else signs anything.
    #[account(mut)]
    pub holder: Signer<'info>,

    #[account(mut, seeds = [POOL_SEED], bump = pool.bump)]
    pub pool: Box<Account<'info, Pool>>,

    /// The surrendered token account. Any token account works — some 2021 holders
    /// keep NFTs outside ATAs — as long as the holder owns it and it holds the bird.
    #[account(
        mut,
        constraint = holder_original_ata.owner == holder.key() @ SwapError::NotOwner,
    )]
    pub holder_original_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The mint is READ FROM the token account being surrendered, never taken from
    /// instruction data: the Mapping PDA derives from `holder_original_ata.mint`, so
    /// naming bird A while surrendering bird B derives B's mapping (or none) and
    /// fails. This seed expression IS the core security property of the design.
    #[account(
        mut,
        seeds = [MAP_SEED, pool.key().as_ref(), holder_original_ata.mint.as_ref()],
        bump = mapping.bump,
    )]
    pub mapping: Box<Account<'info, Mapping>>,

    /// Needed for `transfer_checked` of the original; tied to the surrendered
    /// account's mint so it cannot be substituted.
    #[account(
        constraint = old_mint.key() == holder_original_ata.mint,
        constraint = old_mint.decimals == 0 @ SwapError::NotHeld,
    )]
    pub old_mint: Box<InterfaceAccount<'info, Mint>>,

    #[account(
        constraint = new_mint.key() == mapping.new_mint @ SwapError::WrongRemint,
        constraint = new_mint.decimals == 0 @ SwapError::WrongRemint,
    )]
    pub new_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: The vault authority PDA; signs the outbound remint transfer via seeds.
    #[account(seeds = [VAULT_SEED], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = vault_new_ata.mint == mapping.new_mint @ SwapError::WrongRemint,
        constraint = vault_new_ata.owner == vault.key() @ SwapError::WrongRemint,
    )]
    pub vault_new_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: ATA(vault, old_mint) — the original's permanent home. Created
    /// idempotently (rent: treasury); derivation asserted in the handler.
    #[account(mut)]
    pub vault_original_ata: UncheckedAccount<'info>,

    /// CHECK: ATA(holder, new_mint). The holder has never held their remint, so this
    /// usually does not exist — created idempotently (rent: treasury), which also
    /// survives a retry or a pre-existing account.
    #[account(mut)]
    pub holder_new_ata: UncheckedAccount<'info>,

    #[account(mut, seeds = [TREASURY_SEED], bump = pool.treasury_bump)]
    pub treasury: SystemAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_swap(ctx: Context<Swap>) -> Result<()> {
    // No swaps while pairs are still writable — constraint 1 in §6b, no exceptions.
    require!(ctx.accounts.pool.sealed, SwapError::NotSealed);
    require!(!ctx.accounts.pool.paused, SwapError::Paused);
    require!(!ctx.accounts.mapping.claimed, SwapError::AlreadyClaimed);
    require!(ctx.accounts.holder_original_ata.amount == 1, SwapError::NotHeld);

    let old_mint_key = ctx.accounts.old_mint.key();
    let new_mint_key = ctx.accounts.new_mint.key();
    let holder_key = ctx.accounts.holder.key();

    // ---- Both destination ATAs, created idempotently, rent from the treasury ----
    require_canonical_ata(
        &ctx.accounts.vault_original_ata.key(),
        &ctx.accounts.vault.key(),
        &old_mint_key,
        &ctx.accounts.token_program.key(),
        SwapError::NotRecoverable,
    )?;
    require_canonical_ata(
        &ctx.accounts.holder_new_ata.key(),
        &holder_key,
        &new_mint_key,
        &ctx.accounts.token_program.key(),
        SwapError::WrongRemint,
    )?;
    create_ata_idempotent_treasury_pays(
        &ctx.accounts.treasury,
        ctx.accounts.pool.treasury_bump,
        &ctx.accounts.vault_original_ata,
        ctx.accounts.vault.to_account_info(),
        ctx.accounts.old_mint.to_account_info(),
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
    )?;
    create_ata_idempotent_treasury_pays(
        &ctx.accounts.treasury,
        ctx.accounts.pool.treasury_bump,
        &ctx.accounts.holder_new_ata,
        ctx.accounts.holder.to_account_info(),
        ctx.accounts.new_mint.to_account_info(),
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
    )?;

    // ---- Original in: holder signs as owner ----
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.holder_original_ata.to_account_info(),
                mint: ctx.accounts.old_mint.to_account_info(),
                to: ctx.accounts.vault_original_ata.to_account_info(),
                authority: ctx.accounts.holder.to_account_info(),
            },
        ),
        1,
        ctx.accounts.old_mint.decimals,
    )?;

    // ---- Remint out: the vault PDA signs ----
    let vault_seeds: &[&[u8]] = &[VAULT_SEED, &[ctx.accounts.pool.vault_bump]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.vault_new_ata.to_account_info(),
                mint: ctx.accounts.new_mint.to_account_info(),
                to: ctx.accounts.holder_new_ata.to_account_info(),
                authority: ctx.accounts.vault.to_account_info(),
            },
            &[vault_seeds],
        ),
        1,
        ctx.accounts.new_mint.decimals,
    )?;

    // ---- The receipt: claimed exactly once, never unset ----
    let now = Clock::get()?.unix_timestamp;
    let mapping = &mut ctx.accounts.mapping;
    mapping.claimed = true;
    mapping.claimed_by = holder_key;
    mapping.claimed_at = now;

    let pool = &mut ctx.accounts.pool;
    pool.swapped = pool.swapped.checked_add(1).ok_or(SwapError::Arithmetic)?;

    emit!(BirdSwapped {
        old_mint: old_mint_key,
        new_mint: new_mint_key,
        holder: holder_key,
        ts: now,
        swapped: pool.swapped,
    });
    Ok(())
}
