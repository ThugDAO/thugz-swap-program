use anchor_lang::prelude::*;

pub const POOL_SEED: &[u8] = b"pool";
pub const VAULT_SEED: &[u8] = b"vault";
pub const MAP_SEED: &[u8] = b"map";
pub const TREASURY_SEED: &[u8] = b"treasury";

// Every ground identity is a compile-time constant, deliberately. An init-time field
// could be mis-set (or front-run) and would hand the role to the wrong key; a constant
// is reviewable in source and pinned to the deployed bytecode by the verified build.
//
// The `test-keys` feature swaps in committed fixture keypairs (tests/fixtures/) so the
// LiteSVM suite can sign as admin and custodian. The mainnet artifact is built with
// default features — never with `test-keys`.

/// Token custodian (HxwZ): the only legal `deposit_bird` source owner and signer, and
/// the only legal destination for `fix_mapping` and `recover`. Admin can never receive
/// a token from any instruction.
#[cfg(not(feature = "test-keys"))]
#[constant]
pub const CUSTODIAN: Pubkey = pubkey!("HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB");
#[cfg(feature = "test-keys")]
#[constant]
pub const CUSTODIAN: Pubkey = pubkey!("FmsQWrqdWwvjREEB1CmD7w1hrzz2coXq8SxxAj9bLQZZ");

/// Operational admin. Pinned at `initialize_pool` so the singleton `["pool"]` PDA
/// cannot be front-run and captured by an arbitrary initializer — the program ID is
/// public before deployment, so the race is real.
#[cfg(not(feature = "test-keys"))]
#[constant]
pub const ADMIN: Pubkey = pubkey!("thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7");
#[cfg(feature = "test-keys")]
#[constant]
pub const ADMIN: Pubkey = pubkey!("6ZUQ8tECckw6rATxFETLzKMoGtNJBkcRWVWz4FRvyCbZ");

/// The pair count. From the spec: set from this constant, never from instruction
/// data — a mistyped count would be permanent and silently change `seal` behavior.
#[constant]
pub const EXPECTED: u16 = 1274;
