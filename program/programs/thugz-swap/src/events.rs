use anchor_lang::prelude::*;

// SWAP_SPEC.md §5b / IMPLEMENTATION_APPENDIX.md §7 — the two doc lists and this file
// must stay identical. Every state change emits exactly one event.

#[event]
pub struct BirdDeposited {
    pub old_mint: Pubkey,
    pub new_mint: Pubkey,
    pub deposited: u16,
}

#[event]
pub struct MappingFixed {
    pub old_mint: Pubkey,
    pub new_mint: Pubkey,
    pub deposited: u16,
}

#[event]
pub struct PoolSealed {
    pub expected: u16,
    pub ts: i64,
}

#[event]
pub struct BirdSwapped {
    pub old_mint: Pubkey,
    pub new_mint: Pubkey,
    pub holder: Pubkey,
    pub ts: i64,
    pub swapped: u16,
}

#[event]
pub struct PauseSet {
    pub paused: bool,
    pub ts: i64,
}

#[event]
pub struct BirdRecovered {
    pub new_mint: Pubkey,
    pub ts: i64,
    pub recovered: u16,
}
