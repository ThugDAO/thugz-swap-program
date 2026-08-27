pub mod deposit_bird;
pub mod fix_mapping;
pub mod initialize_pool;
pub mod recover;
pub mod seal;
pub mod set_paused;
pub mod swap;

pub use deposit_bird::*;
pub use fix_mapping::*;
pub use initialize_pool::*;
pub use recover::*;
pub use seal::*;
pub use set_paused::*;
pub use swap::*;

use anchor_lang::prelude::*;
use anchor_lang::system_program::System;
use anchor_spl::associated_token::{self, AssociatedToken};
use anchor_spl::token_interface::TokenInterface;

use crate::constants::TREASURY_SEED;
use crate::error::SwapError;

/// Create an ATA idempotently with the treasury PDA as rent payer.
///
/// This MUST be an explicit CPI built with `new_with_signer`: Anchor's `init` /
/// `init_if_needed` account constraints sign for the account being created, not for
/// the payer, and our payer is a PDA that can only sign via seeds
/// (IMPLEMENTATION_APPENDIX.md §4). The ATA program validates that `ata` is the
/// canonical derived address for (authority, mint, token_program), and its inner
/// `create_account` requires the payer to be system-owned with zero data — which is
/// the whole reason the treasury exists and `Pool` never pays.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_ata_idempotent_treasury_pays<'info>(
    treasury: &SystemAccount<'info>,
    treasury_bump: u8,
    ata: &UncheckedAccount<'info>,
    wallet: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    system_program: &Program<'info, System>,
    token_program: &Interface<'info, TokenInterface>,
) -> Result<()> {
    let seeds: &[&[u8]] = &[TREASURY_SEED, &[treasury_bump]];
    associated_token::create_idempotent(CpiContext::new_with_signer(
        AssociatedToken::id(),
        associated_token::Create {
            payer: treasury.to_account_info(),
            associated_token: ata.to_account_info(),
            authority: wallet,
            mint,
            system_program: system_program.to_account_info(),
            token_program: token_program.to_account_info(),
        },
        &[seeds],
    ))
}

/// Belt-and-braces: the ATA program already rejects a non-canonical address on
/// `create_idempotent`, but we assert the derivation ourselves before any CPI so a
/// wrong account fails with our error, not a CPI error.
pub(crate) fn require_canonical_ata(
    ata: &Pubkey,
    wallet: &Pubkey,
    mint: &Pubkey,
    token_program: &Pubkey,
    err: SwapError,
) -> Result<()> {
    let expected = associated_token::get_associated_token_address_with_program_id(
        wallet,
        mint,
        token_program,
    );
    require_keys_eq!(*ata, expected, err);
    Ok(())
}
