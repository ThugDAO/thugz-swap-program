use anchor_lang::prelude::*;
use anchor_lang::system_program;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

use crate::constants::*;
use crate::error::SwapError;
use crate::events::BirdDeposited;
use crate::instructions::{create_ata_idempotent_treasury_pays, require_canonical_ata};
use crate::state::{Mapping, Pool};

#[derive(Accounts)]
#[instruction(old_mint: Pubkey)]
pub struct DepositBird<'info> {
    /// Transaction fee payer and pool authority.
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(mut, seeds = [POOL_SEED], bump = pool.bump, has_one = admin)]
    pub pool: Box<Account<'info, Pool>>,

    /// The token owner. Must BE the compiled custodian, signing — never a delegate.
    /// Without this, "HxwZ co-signs" would be prose, not a constraint.
    #[account(constraint = custodian.key() == CUSTODIAN @ SwapError::NotCustodian)]
    pub custodian: Signer<'info>,

    /// Legacy SPL Token only: every real remint is Tokenkeg-owned (chain-verified),
    /// and Token-2022 extensions (transfer hooks, permanent delegates) could undermine
    /// vault custody. The sweep also enforces this off-chain; after Phase 5 reviews
    /// 1+2 converged here, the program now refuses at the door as well.
    #[account(
        constraint = new_mint.decimals == 0 @ SwapError::WrongRemint,
        constraint = *new_mint.to_account_info().owner == anchor_spl::token::ID @ SwapError::LegacyTokenOnly,
    )]
    pub new_mint: Box<InterfaceAccount<'info, Mint>>,

    /// `new_mint` is READ FROM this account (enforced by the mint constraint), so the
    /// admin path cannot suffer the "name bird A, hand over bird B" class of bug.
    #[account(
        mut,
        constraint = source_ata.mint == new_mint.key() @ SwapError::WrongRemint,
        constraint = source_ata.owner == CUSTODIAN @ SwapError::NotCustodian,
    )]
    pub source_ata: Box<InterfaceAccount<'info, TokenAccount>>,

    /// CHECK: Manually initialized below — NOT Anchor `init`, which aborts on a
    /// pre-funded address (Mapping addresses are derivable the moment the program ID
    /// is public, so lamport griefing is reachable, not theoretical). The handler
    /// requires system-owned + zero data before creating, which is also the
    /// init-once guard.
    #[account(mut, seeds = [MAP_SEED, pool.key().as_ref(), old_mint.as_ref()], bump)]
    pub mapping: UncheckedAccount<'info>,

    /// CHECK: The vault authority PDA. Holds no data; only ever signs via seeds.
    #[account(seeds = [VAULT_SEED], bump = pool.vault_bump)]
    pub vault: UncheckedAccount<'info>,

    /// CHECK: ATA(vault, new_mint) — MUST be created here (it does not exist yet);
    /// derivation asserted in the handler and re-validated by the ATA program.
    #[account(mut)]
    pub vault_ata: UncheckedAccount<'info>,

    /// The rent payer. `SystemAccount` re-asserts system ownership on every call —
    /// an assigned or allocated treasury is dead as a `create_account` payer, so
    /// failing loudly here is the desired behavior.
    #[account(mut, seeds = [TREASURY_SEED], bump = pool.treasury_bump)]
    pub treasury: SystemAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_deposit_bird(ctx: Context<DepositBird>, old_mint: Pubkey) -> Result<()> {
    require!(!ctx.accounts.pool.sealed, SwapError::Sealed);
    require!(ctx.accounts.source_ata.amount == 1, SwapError::NotHeld);

    let pool_key = ctx.accounts.pool.key();
    let new_mint_key = ctx.accounts.new_mint.key();
    let mapping_bump = ctx.bumps.mapping;

    // ---- Manual Mapping init (appendix §4): absorb a pre-sent lamport balance ----
    let mapping_info = ctx.accounts.mapping.to_account_info();

    // Init-once guard: an existing Mapping is system-owned no longer, or carries
    // data. Either way, a second deposit for the same original must fail here.
    require!(
        mapping_info.owner == &system_program::ID && mapping_info.data_is_empty(),
        SwapError::MappingExists
    );

    let space = 8 + Mapping::INIT_SPACE;
    let required = Rent::get()?.minimum_balance(space);
    let existing = mapping_info.lamports();
    if existing < required {
        // Transfer only the deficit from the TREASURY PDA (never Pool — a
        // data-bearing account cannot fund account creation).
        let treasury_seeds: &[&[u8]] = &[TREASURY_SEED, &[ctx.accounts.pool.treasury_bump]];
        system_program::transfer(
            CpiContext::new_with_signer(
                System::id(),
                system_program::Transfer {
                    from: ctx.accounts.treasury.to_account_info(),
                    to: mapping_info.clone(),
                },
                &[treasury_seeds],
            ),
            required
                .checked_sub(existing)
                .ok_or(SwapError::Arithmetic)?,
        )?;
    }

    // Allocate + assign, signed by the Mapping PDA itself.
    let mapping_seeds: &[&[u8]] = &[
        MAP_SEED,
        pool_key.as_ref(),
        old_mint.as_ref(),
        &[mapping_bump],
    ];
    system_program::allocate(
        CpiContext::new_with_signer(
            System::id(),
            system_program::Allocate {
                account_to_allocate: mapping_info.clone(),
            },
            &[mapping_seeds],
        ),
        space as u64,
    )?;
    system_program::assign(
        CpiContext::new_with_signer(
            System::id(),
            system_program::Assign {
                account_to_assign: mapping_info.clone(),
            },
            &[mapping_seeds],
        ),
        &crate::ID,
    )?;

    // Write discriminator + every field explicitly.
    let state = Mapping {
        new_mint: new_mint_key,
        claimed: false,
        claimed_by: Pubkey::default(),
        claimed_at: 0,
        recovered: false,
        bump: mapping_bump,
    };
    let mut data = mapping_info.try_borrow_mut_data()?;
    state.try_serialize(&mut &mut data[..])?;
    drop(data);

    // ---- Vault ATA: created here, rent from the treasury ----
    require_canonical_ata(
        &ctx.accounts.vault_ata.key(),
        &ctx.accounts.vault.key(),
        &new_mint_key,
        &ctx.accounts.token_program.key(),
        SwapError::WrongRemint,
    )?;
    create_ata_idempotent_treasury_pays(
        &ctx.accounts.treasury,
        ctx.accounts.pool.treasury_bump,
        &ctx.accounts.vault_ata,
        ctx.accounts.vault.to_account_info(),
        ctx.accounts.new_mint.to_account_info(),
        &ctx.accounts.system_program,
        &ctx.accounts.token_program,
    )?;

    // ---- Move the remint: custodian signs as owner; transfer_checked always ----
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.source_ata.to_account_info(),
                mint: ctx.accounts.new_mint.to_account_info(),
                to: ctx.accounts.vault_ata.to_account_info(),
                authority: ctx.accounts.custodian.to_account_info(),
            },
        ),
        1,
        ctx.accounts.new_mint.decimals,
    )?;

    let pool = &mut ctx.accounts.pool;
    pool.deposited = pool.deposited.checked_add(1).ok_or(SwapError::Arithmetic)?;

    emit!(BirdDeposited {
        old_mint,
        new_mint: new_mint_key,
        deposited: pool.deposited,
    });
    Ok(())
}
