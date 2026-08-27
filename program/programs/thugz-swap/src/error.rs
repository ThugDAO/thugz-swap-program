use anchor_lang::prelude::*;

// IMPLEMENTATION_APPENDIX.md §3, plus three implementation-required variants
// (MappingExists, InvalidUnlockTimestamp, Arithmetic) recorded as spec deltas.
#[error_code]
pub enum SwapError {
    #[msg("Pool is not sealed yet")]
    NotSealed,
    #[msg("Pool is already sealed")]
    Sealed,
    #[msg("Swaps are paused")]
    Paused,
    #[msg("This original has already been swapped")]
    AlreadyClaimed,
    #[msg("Vault account does not hold the mapped remint")]
    WrongRemint,
    #[msg("You do not own this original")]
    NotOwner,
    #[msg("Token account does not hold exactly one token")]
    NotHeld,
    #[msg("Deposited count does not match expected")]
    Incomplete,
    #[msg("Unlock timestamp has not passed")]
    Locked,
    #[msg("This account is not recoverable")]
    NotRecoverable,
    #[msg("Custodian constraint violated")]
    NotCustodian,
    #[msg("Duplicate account supplied")]
    DuplicateAccount,
    #[msg("A mapping for this original already exists")]
    MappingExists,
    #[msg("Unlock timestamp must be in the future")]
    InvalidUnlockTimestamp,
    #[msg("Arithmetic overflow or underflow")]
    Arithmetic,
}
