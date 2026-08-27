use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::constants::*;
use crate::error::SwapError;
use crate::events::MappingFixed;
use crate::instructions::{create_ata_idempotent_treasury_pays, require_canonical_ata};
use crate::state::{Mapping, Pool};

#[derive(Accounts)]
#[instruction(old_mint: Pubkey)]
pub struct FixMapping<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut, seeds = [POOL_SEED], bump = pool.bump, has_one = admin)]
    pub pool: Box<Account<'info, Pool>>,

    /// Closed here — the ONE way to undo a bad deposit, dead after seal. Rent
    /// returns to the treasury (never Pool, never admin) so the payer model has one
    /// story. A wrong `old_mint` argument derives a different PDA and fails.
    #[account(
        mut,
        close = treasury,
        seeds = [MAP_SEED, pool.key().as_ref(), old_mint.as_ref()],
        bump = mapping.bump,
    )]
    pub mapping: Box<Account<'info, Mapping>>,

    #[account(constraint = new_mint.key() == mapping.new_mint @ SwapError::WrongRemint)]
    pub new_mint: Box<InterfaceAccount<'info, Mint>>,

    /// CHECK: The vault authority PDA; signs the outbound transfer via seeds.
    #[account(seeds = [VAULT_SEED], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,

    #[account(
        mut,
        constraint = vault_ata.mint == mapping.new_mint @ SwapError::WrongRemint,
        constraint = vault_ata.owner == vault.key() @ SwapError::WrongRemint,
    )]
    pub vault_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: The custodian wallet — pinned to the compiled CUSTODIAN constant, not
    /// caller-supplied, not a field admin ever sets. Present (unsigned) because the
    /// ATA program needs the wallet account to create the destination.
    #[account(constraint = custodian.key() == CUSTODIAN @ SwapError::NotCustodian)]
    pub custodian: UncheckedAccount<'info>,

    /// CHECK: ATA(CUSTODIAN, new_mint) — the address the bird came from. THE v3
    /// custody control: admin can un-deposit; admin cannot pocket. Derivation
    /// asserted in the handler; created idempotently (rent: treasury).
    #[account(mut)]
    pub custodian_ata: UncheckedAccount<'info>,

    #[account(mut, seeds = [TREASURY_SEED], bump = pool.treasury_bump)]
    pub treasury: SystemAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_fix_mapping(ctx: Context<FixMapping>, old_mint: Pubkey) -> Result<()> {
    // Hard gate, no exceptions — this instruction is dead after seal.
    require!(!ctx.accounts.pool.sealed, SwapError::Sealed);
    // Unreachable pre-seal (swap requires sealed), asserted anyway.
    require!(!ctx.accounts.mapping.claimed, SwapError::AlreadyClaimed);
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

    let pool = &mut ctx.accounts.pool;
    pool.deposited = pool.deposited.checked_sub(1).ok_or(SwapError::Arithmetic)?;

    emit!(MappingFixed {
        old_mint,
        new_mint: new_mint_key,
        deposited: pool.deposited,
    });
    // Anchor's `close = treasury` zeroes the data, moves the lamports, and reassigns
    // to the system program after this handler returns.
    Ok(())
}
