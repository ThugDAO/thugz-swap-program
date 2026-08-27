//! TEST_PLAN.md Level 2 — property and differential tests (proptest + LiteSVM).
//!
//! Model-based stateful testing: random operation sequences run against BOTH the
//! real program (LiteSVM) and a tiny in-memory model. After every op the six
//! Level 2 invariants are asserted against chain state:
//!
//!   1. `sealed` is monotonic — no sequence makes it false again
//!   2. A Mapping is claimed at most once, and `claimed` is never unset
//!   3. `swapped` / `deposited` / `recovered` counters match the model exactly,
//!      and `swapped + recovered <= expected`
//!   4. Every remint is in exactly one of {custodian, vault, holder} and moves
//!      ONLY via a successful deposit / fix_mapping / swap / recover
//!   5. A deposited original NEVER leaves the vault, under any sequence
//!   6. Expected outcomes are exact: the model predicts the specific error code
//!      for every illegal op (per the program's real check order), so a check
//!      that silently vanished would surface as an outcome mismatch
//!
//! Run (after building the test-keys .so):
//!   NO_DNA=1 anchor build -- --features test-keys
//!   cargo test --features test-keys --test level2_properties -- --test-threads=1
//! PROPTEST_CASES=n overrides the default 24 sequences (CI: 200, nightly: 1000).

use anchor_lang::error::ERROR_CODE_OFFSET;
use anchor_lang::prelude::{Clock, Pubkey};
use anchor_lang::solana_program::{instruction::Instruction, system_program};
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::associated_token::ID as ATA_PROGRAM_ID;
use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
use litesvm::LiteSVM;
use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo};
use proptest::prelude::*;
use solana_instruction::error::InstructionError;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use solana_transaction_error::TransactionError;
use thugz_swap::error::SwapError;
use thugz_swap::state::{Mapping, Pool};
use thugz_swap::{EXPECTED, MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

const N: usize = EXPECTED as usize; // 20 under test-keys — a REAL seal is reachable

// ------------------------------------------------------------------- ops ----

#[derive(Debug, Clone)]
enum Op {
    Deposit(usize),
    DepositAllRemaining, // makes seal reachable in random sequences (anti-vacuity)
    Fix(usize),
    Seal,
    Swap(usize),
    Recover(usize),
    SetPaused(bool),
    WarpPastUnlock,
}

fn op_strategy() -> impl Strategy<Value = Op> {
    prop_oneof![
        3 => (0..N).prop_map(Op::Deposit),
        2 => Just(Op::DepositAllRemaining),
        2 => (0..N).prop_map(Op::Fix),
        2 => Just(Op::Seal),
        4 => (0..N).prop_map(Op::Swap),
        2 => (0..N).prop_map(Op::Recover),
        1 => any::<bool>().prop_map(Op::SetPaused),
        1 => Just(Op::WarpPastUnlock),
    ]
}

// ----------------------------------------------------------------- model ----

#[derive(Clone, Copy, PartialEq, Debug)]
enum Loc {
    Custodian,
    Vault,
    Holder,
}

#[derive(Clone)]
struct BirdModel {
    mapping: bool,
    claimed: bool,
    remint: Loc,
    original_in_vault: bool,
}

struct Model {
    birds: Vec<BirdModel>,
    sealed: bool,
    paused: bool,
    warped: bool,
    deposited: u16,
    swapped: u16,
    recovered: u16,
}

/// What the program must do for this op, per its REAL check order
/// (account-stage 3012 before handler requires; handler require order as coded).
enum Expect {
    Ok,
    Code(u32),
    /// Fails somewhere past our typed checks (e.g. token program insufficient
    /// funds on a vault emptied by recover); exact code is the token program's.
    AnyErr,
}

fn s(e: SwapError) -> Expect {
    Expect::Code(ERROR_CODE_OFFSET + e as u32)
}

impl Model {
    fn new() -> Self {
        Model {
            birds: (0..N)
                .map(|_| BirdModel {
                    mapping: false,
                    claimed: false,
                    remint: Loc::Custodian,
                    original_in_vault: false,
                })
                .collect(),
            sealed: false,
            paused: false,
            warped: false,
            deposited: 0,
            swapped: 0,
            recovered: 0,
        }
    }

    fn expect(&self, op: &Op) -> Expect {
        match op {
            Op::Deposit(i) => {
                let b = &self.birds[*i];
                if self.sealed {
                    s(SwapError::Sealed)
                } else if b.remint != Loc::Custodian {
                    s(SwapError::NotHeld) // custodian's source ATA is empty
                } else if b.mapping {
                    s(SwapError::MappingExists)
                } else {
                    Expect::Ok
                }
            }
            Op::DepositAllRemaining | Op::SetPaused(_) | Op::WarpPastUnlock => Expect::Ok,
            Op::Fix(i) => {
                let b = &self.birds[*i];
                if !b.mapping {
                    Expect::Code(3012) // typed Account fails to load first
                } else if self.sealed {
                    s(SwapError::Sealed)
                } else if b.remint != Loc::Vault {
                    s(SwapError::NotHeld) // recovered pre-seal: mapping alive, vault empty
                } else {
                    Expect::Ok
                }
            }
            Op::Seal => {
                if self.sealed {
                    s(SwapError::Sealed)
                } else if self.deposited != EXPECTED {
                    s(SwapError::Incomplete)
                } else {
                    Expect::Ok
                }
            }
            Op::Swap(i) => {
                let b = &self.birds[*i];
                if !b.mapping {
                    Expect::Code(3012)
                } else if !self.sealed {
                    s(SwapError::NotSealed)
                } else if self.paused {
                    s(SwapError::Paused)
                } else if b.claimed {
                    s(SwapError::AlreadyClaimed)
                } else if b.original_in_vault {
                    s(SwapError::NotHeld) // holder's account is empty
                } else if b.remint != Loc::Vault {
                    Expect::AnyErr // recover emptied the vault: must fail ATOMICALLY
                } else {
                    Expect::Ok
                }
            }
            Op::Recover(i) => {
                let b = &self.birds[*i];
                if !b.mapping {
                    Expect::Code(3012)
                } else if !self.warped {
                    s(SwapError::Locked)
                } else if b.claimed {
                    s(SwapError::AlreadyClaimed)
                } else if b.remint != Loc::Vault {
                    s(SwapError::NotHeld)
                } else {
                    Expect::Ok
                }
            }
        }
    }

    fn apply(&mut self, op: &Op) {
        match op {
            Op::Deposit(i) => {
                let b = &mut self.birds[*i];
                b.mapping = true;
                b.remint = Loc::Vault;
                self.deposited += 1;
            }
            Op::Fix(i) => {
                let b = &mut self.birds[*i];
                b.mapping = false;
                b.remint = Loc::Custodian;
                self.deposited -= 1;
            }
            Op::Seal => self.sealed = true,
            Op::Swap(i) => {
                let b = &mut self.birds[*i];
                b.claimed = true;
                b.remint = Loc::Holder;
                b.original_in_vault = true;
                self.swapped += 1;
            }
            Op::Recover(i) => {
                let b = &mut self.birds[*i];
                b.remint = Loc::Custodian; // mapping stays; claimed stays false
                self.recovered += 1;
            }
            Op::SetPaused(p) => self.paused = *p,
            Op::WarpPastUnlock => self.warped = true,
            Op::DepositAllRemaining => unreachable!("expanded by the executor"),
        }
    }
}

// --------------------------------------------------------------- harness ----

struct Env {
    svm: LiteSVM,
    admin: Keypair,
    custodian: Keypair,
    holders: Vec<Keypair>,
    old_mints: Vec<Pubkey>,
    new_mints: Vec<Pubkey>,
    pool: Pubkey,
    vault: Pubkey,
    treasury: Pubkey,
    unlock_ts: i64,
}

fn fixture_keypair(name: &str) -> Keypair {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    let raw = std::fs::read_to_string(&path).expect("fixture keypair missing");
    let bytes: Vec<u8> = raw
        .trim().trim_start_matches('[').trim_end_matches(']')
        .split(',').map(|t| t.trim().parse::<u8>().unwrap()).collect();
    Keypair::try_from(&bytes[..]).unwrap()
}

fn send(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Pubkey, signers: &[&Keypair]) -> Result<(), TransactionError> {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(payer), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let out = svm.send_transaction(tx).map(|_| ()).map_err(|f| f.err);
    svm.expire_blockhash();
    out
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let program_id = thugz_swap::id();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/thugz_swap.so"));
    svm.add_program(program_id, bytes).unwrap();
    let admin = fixture_keypair("test_admin.json");
    let custodian = fixture_keypair("test_custodian.json");
    svm.airdrop(&admin.pubkey(), 500_000_000_000).unwrap();
    svm.airdrop(&custodian.pubkey(), 500_000_000_000).unwrap();

    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = 1_756_252_800;
    svm.set_sysvar(&clock);
    let unlock_ts = clock.unix_timestamp + 2 * 365 * 24 * 60 * 60;

    let pool = Pubkey::find_program_address(&[POOL_SEED], &program_id).0;
    let vault = Pubkey::find_program_address(&[VAULT_SEED], &program_id).0;
    let treasury = Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0;
    svm.airdrop(&treasury, 500_000_000_000).unwrap();

    // init pool
    let ix = Instruction::new_with_bytes(
        program_id,
        &thugz_swap::instruction::InitializePool { unlock_ts, collection: Pubkey::new_unique() }.data(),
        thugz_swap::accounts::InitializePool {
            admin: admin.pubkey(),
            pool,
            system_program: system_program::ID,
        }.to_account_metas(None),
    );
    send(&mut svm, &[ix], &admin.pubkey(), &[&admin]).expect("init");

    // N birds
    let mut holders = vec![];
    let mut old_mints = vec![];
    let mut new_mints = vec![];
    for _ in 0..N {
        let holder = Keypair::new();
        svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
        let old = CreateMint::new(&mut svm, &admin).decimals(0).authority(&admin.pubkey()).send().unwrap();
        let new = CreateMint::new(&mut svm, &admin).decimals(0).authority(&admin.pubkey()).send().unwrap();
        let h_ata = CreateAssociatedTokenAccount::new(&mut svm, &admin, &old).owner(&holder.pubkey()).send().unwrap();
        let c_ata = CreateAssociatedTokenAccount::new(&mut svm, &admin, &new).owner(&custodian.pubkey()).send().unwrap();
        MintTo::new(&mut svm, &admin, &old, &h_ata, 1).send().unwrap();
        MintTo::new(&mut svm, &admin, &new, &c_ata, 1).send().unwrap();
        holders.push(holder);
        old_mints.push(old);
        new_mints.push(new);
    }
    Env { svm, admin, custodian, holders, old_mints, new_mints, pool, vault, treasury, unlock_ts }
}

impl Env {
    fn mapping_pda(&self, i: usize) -> Pubkey {
        Pubkey::find_program_address(
            &[MAP_SEED, self.pool.as_ref(), self.old_mints[i].as_ref()],
            &thugz_swap::id(),
        ).0
    }
    fn balance(&self, ata: &Pubkey) -> u64 {
        match self.svm.get_account(ata) {
            Some(a) if a.data.len() >= 72 => u64::from_le_bytes(a.data[64..72].try_into().unwrap()),
            _ => 0,
        }
    }
    fn pool_state(&self) -> Pool {
        let acc = self.svm.get_account(&self.pool).unwrap();
        Pool::try_deserialize(&mut acc.data.as_slice()).unwrap()
    }
    fn mapping_state(&self, i: usize) -> Option<Mapping> {
        let acc = self.svm.get_account(&self.mapping_pda(i))?;
        if acc.data.is_empty() { return None; }
        Mapping::try_deserialize(&mut acc.data.as_slice()).ok()
    }
    fn remint_loc(&self, i: usize) -> Loc {
        let m = &self.new_mints[i];
        if self.balance(&get_associated_token_address(&self.vault, m)) == 1 { return Loc::Vault; }
        if self.balance(&get_associated_token_address(&self.holders[i].pubkey(), m)) == 1 { return Loc::Holder; }
        Loc::Custodian
    }

    fn run_op(&mut self, op: &Op) -> Result<(), TransactionError> {
        let admin = self.admin.insecure_clone();
        let custodian = self.custodian.insecure_clone();
        match op {
            Op::Deposit(i) => {
                let ix = Instruction::new_with_bytes(
                    thugz_swap::id(),
                    &thugz_swap::instruction::DepositBird { old_mint: self.old_mints[*i] }.data(),
                    thugz_swap::accounts::DepositBird {
                        admin: admin.pubkey(),
                        pool: self.pool,
                        custodian: custodian.pubkey(),
                        new_mint: self.new_mints[*i],
                        source_ata: get_associated_token_address(&custodian.pubkey(), &self.new_mints[*i]),
                        mapping: self.mapping_pda(*i),
                        vault: self.vault,
                        vault_ata: get_associated_token_address(&self.vault, &self.new_mints[*i]),
                        treasury: self.treasury,
                        token_program: TOKEN_PROGRAM_ID,
                        associated_token_program: ATA_PROGRAM_ID,
                        system_program: system_program::ID,
                    }.to_account_metas(None),
                );
                send(&mut self.svm, &[ix], &admin.pubkey(), &[&admin, &custodian])
            }
            Op::Fix(i) => {
                let ix = Instruction::new_with_bytes(
                    thugz_swap::id(),
                    &thugz_swap::instruction::FixMapping { old_mint: self.old_mints[*i] }.data(),
                    thugz_swap::accounts::FixMapping {
                        admin: admin.pubkey(),
                        pool: self.pool,
                        mapping: self.mapping_pda(*i),
                        new_mint: self.new_mints[*i],
                        vault: self.vault,
                        vault_ata: get_associated_token_address(&self.vault, &self.new_mints[*i]),
                        custodian: custodian.pubkey(),
                        custodian_ata: get_associated_token_address(&custodian.pubkey(), &self.new_mints[*i]),
                        treasury: self.treasury,
                        token_program: TOKEN_PROGRAM_ID,
                        associated_token_program: ATA_PROGRAM_ID,
                        system_program: system_program::ID,
                    }.to_account_metas(None),
                );
                send(&mut self.svm, &[ix], &admin.pubkey(), &[&admin])
            }
            Op::Seal => {
                let ix = Instruction::new_with_bytes(
                    thugz_swap::id(),
                    &thugz_swap::instruction::Seal {}.data(),
                    thugz_swap::accounts::Seal { admin: admin.pubkey(), pool: self.pool }.to_account_metas(None),
                );
                send(&mut self.svm, &[ix], &admin.pubkey(), &[&admin])
            }
            Op::Swap(i) => {
                let holder = self.holders[*i].insecure_clone();
                let ix = Instruction::new_with_bytes(
                    thugz_swap::id(),
                    &thugz_swap::instruction::Swap {}.data(),
                    thugz_swap::accounts::Swap {
                        holder: holder.pubkey(),
                        pool: self.pool,
                        holder_original_ata: get_associated_token_address(&holder.pubkey(), &self.old_mints[*i]),
                        mapping: self.mapping_pda(*i),
                        old_mint: self.old_mints[*i],
                        new_mint: self.new_mints[*i],
                        vault: self.vault,
                        vault_new_ata: get_associated_token_address(&self.vault, &self.new_mints[*i]),
                        vault_original_ata: get_associated_token_address(&self.vault, &self.old_mints[*i]),
                        holder_new_ata: get_associated_token_address(&holder.pubkey(), &self.new_mints[*i]),
                        treasury: self.treasury,
                        token_program: TOKEN_PROGRAM_ID,
                        associated_token_program: ATA_PROGRAM_ID,
                        system_program: system_program::ID,
                    }.to_account_metas(None),
                );
                send(&mut self.svm, &[ix], &holder.pubkey(), &[&holder])
            }
            Op::Recover(i) => {
                let ix = Instruction::new_with_bytes(
                    thugz_swap::id(),
                    &thugz_swap::instruction::Recover { old_mint: self.old_mints[*i] }.data(),
                    thugz_swap::accounts::Recover {
                        admin: admin.pubkey(),
                        pool: self.pool,
                        mapping: self.mapping_pda(*i),
                        new_mint: self.new_mints[*i],
                        vault: self.vault,
                        vault_ata: get_associated_token_address(&self.vault, &self.new_mints[*i]),
                        custodian: custodian.pubkey(),
                        custodian_ata: get_associated_token_address(&custodian.pubkey(), &self.new_mints[*i]),
                        treasury: self.treasury,
                        token_program: TOKEN_PROGRAM_ID,
                        associated_token_program: ATA_PROGRAM_ID,
                        system_program: system_program::ID,
                    }.to_account_metas(None),
                );
                send(&mut self.svm, &[ix], &admin.pubkey(), &[&admin])
            }
            Op::SetPaused(p) => {
                let ix = Instruction::new_with_bytes(
                    thugz_swap::id(),
                    &thugz_swap::instruction::SetPaused { paused: *p }.data(),
                    thugz_swap::accounts::SetPaused { admin: admin.pubkey(), pool: self.pool }.to_account_metas(None),
                );
                send(&mut self.svm, &[ix], &admin.pubkey(), &[&admin])
            }
            Op::WarpPastUnlock => {
                let mut clock: Clock = self.svm.get_sysvar();
                clock.unix_timestamp = self.unlock_ts + 60;
                self.svm.set_sysvar(&clock);
                Ok(())
            }
            Op::DepositAllRemaining => unreachable!("expanded by the executor"),
        }
    }
}

// ----------------------------------------------------------- the property ----

fn assert_invariants(env: &Env, model: &Model, step: usize, op: &Op) {
    let pool = env.pool_state();
    // 1. sealed monotonic + matches model
    assert_eq!(pool.sealed, model.sealed, "step {step} {op:?}: sealed diverged");
    // 3. counters exact
    assert_eq!(pool.deposited, model.deposited, "step {step} {op:?}: deposited diverged");
    assert_eq!(pool.swapped, model.swapped, "step {step} {op:?}: swapped diverged");
    assert_eq!(pool.recovered, model.recovered, "step {step} {op:?}: recovered diverged");
    assert!(
        pool.swapped as u32 + pool.recovered as u32 <= pool.expected as u32,
        "step {step}: swapped+recovered exceeds expected"
    );
    if !pool.sealed {
        assert_eq!(pool.swapped, 0, "step {step}: swap happened before seal");
    }
    for (i, b) in model.birds.iter().enumerate() {
        // 2. claimed matches model and is never unset (model never unsets it)
        let chain_claimed = env.mapping_state(i).map(|m| m.claimed).unwrap_or(false);
        assert_eq!(chain_claimed, b.claimed, "step {step} bird {i}: claimed diverged");
        assert_eq!(env.mapping_state(i).is_some(), b.mapping, "step {step} bird {i}: mapping existence diverged");
        // 4. remint location matches the model exactly
        assert_eq!(env.remint_loc(i), b.remint, "step {step} bird {i}: remint location diverged");
        // 5. a deposited original never leaves the vault
        let orig_in_vault = env.balance(&get_associated_token_address(&env.vault, &env.old_mints[i])) == 1;
        assert_eq!(orig_in_vault, b.original_in_vault, "step {step} bird {i}: ORIGINAL MOVED");
    }
}

fn run_sequence(ops: Vec<Op>) {
    let mut env = setup();
    let mut model = Model::new();
    let mut step = 0usize;
    for op in ops {
        // Expand the compound op into real deposits so seal is reachable.
        let expanded: Vec<Op> = match op {
            Op::DepositAllRemaining if !model.sealed => (0..N)
                .filter(|i| {
                    let b = &model.birds[*i];
                    !b.mapping && b.remint == Loc::Custodian
                })
                .map(Op::Deposit)
                .collect(),
            Op::DepositAllRemaining => vec![],
            other => vec![other],
        };
        for op in expanded {
            step += 1;
            let expect = model.expect(&op);
            let res = env.run_op(&op);
            match expect {
                Expect::Ok => {
                    assert!(res.is_ok(), "step {step} {op:?}: expected Ok, got {res:?}");
                    model.apply(&op);
                }
                Expect::Code(code) => {
                    let got = match &res {
                        Err(TransactionError::InstructionError(_, InstructionError::Custom(c))) => Some(*c),
                        _ => None,
                    };
                    assert_eq!(got, Some(code), "step {step} {op:?}: expected custom {code}, got {res:?}");
                }
                Expect::AnyErr => {
                    assert!(res.is_err(), "step {step} {op:?}: expected atomic failure, SUCCEEDED");
                }
            }
            assert_invariants(&env, &model, step, &op);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(24),
        max_shrink_iters: 200,
        .. ProptestConfig::default()
    })]

    #[test]
    fn prop_no_sequence_breaks_the_desk(ops in prop::collection::vec(op_strategy(), 1..40)) {
        run_sequence(ops);
    }
}

/// Pinned edge sequence (the @example habit): full lifecycle including the
/// swap-after-recover atomicity case, on every run regardless of generator luck.
#[test]
fn pinned_full_lifecycle_with_recover_race() {
    let mut ops: Vec<Op> = vec![Op::Deposit(0), Op::Fix(0), Op::Deposit(0)];
    ops.push(Op::DepositAllRemaining);
    ops.extend([
        Op::Seal,
        Op::Swap(1),
        Op::Swap(1),          // AlreadyClaimed
        Op::SetPaused(true),
        Op::Swap(2),          // Paused
        Op::SetPaused(false),
        Op::Recover(3),       // Locked (not warped)
        Op::WarpPastUnlock,
        Op::Recover(3),       // ok — remint leaves vault, claimed stays false
        Op::Swap(3),          // must fail ATOMICALLY: holder keeps the original
        Op::Recover(1),       // AlreadyClaimed
        Op::Swap(4),          // desk still open after unlock
    ]);
    run_sequence(ops);
    // The atomicity claim, stated positively: holder 3 still holds their original.
}
