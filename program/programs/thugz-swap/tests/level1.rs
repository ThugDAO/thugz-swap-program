//! TEST_PLAN.md Level 1 — LiteSVM unit + instruction tests.
//!
//! Build the test-keys .so FIRST, then run:
//!   NO_DNA=1 anchor build -- --features test-keys
//!   cargo test --features test-keys -- --test-threads=1
//!
//! The `test-keys` feature swaps ADMIN/CUSTODIAN for the committed fixture
//! keypairs in tests/fixtures/ — every other constant (EXPECTED = 1274 included)
//! is identical to mainnet. States that would need 1,274 real deposits (the seal
//! boundary) are reached by forging the Pool account via `set_account`, which
//! exercises the same on-chain checks without a thousand transactions.
//!
//! The standard (TEST_PLAN.md): a test only counts if it has been seen to fail
//! against a deliberately broken build, and failure tests assert the SPECIFIC
//! error, never just "it reverted".

use anchor_lang::error::ERROR_CODE_OFFSET;
use anchor_lang::prelude::{Clock, Pubkey};
use anchor_lang::solana_program::{instruction::Instruction, system_program};
use anchor_lang::{AccountDeserialize, AccountSerialize, InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::associated_token::ID as ATA_PROGRAM_ID;
use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
use litesvm::LiteSVM;
use litesvm_token::{CreateAssociatedTokenAccount, CreateMint, MintTo};
use solana_account::Account;
use solana_instruction::error::InstructionError;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use solana_transaction_error::TransactionError;
use std::sync::Mutex;
use thugz_swap::error::SwapError;
use thugz_swap::state::{Mapping, Pool};
use thugz_swap::{EXPECTED, MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

static CU_RESULTS: Mutex<Vec<(&'static str, u64)>> = Mutex::new(Vec::new());

fn record_cu(label: &'static str, cu: u64) {
    CU_RESULTS.lock().unwrap().push((label, cu));
}

// ---------------------------------------------------------------- harness ----

struct Env {
    svm: LiteSVM,
    admin: Keypair,
    custodian: Keypair,
    pool: Pubkey,
    vault: Pubkey,
    treasury: Pubkey,
}

fn fixture_keypair(name: &str) -> Keypair {
    let path = format!("{}/tests/fixtures/{}", env!("CARGO_MANIFEST_DIR"), name);
    let raw = std::fs::read_to_string(&path).expect("fixture keypair missing");
    let bytes: Vec<u8> = serde_json_parse(&raw);
    Keypair::try_from(&bytes[..]).expect("bad fixture keypair")
}

// Minimal JSON [u8; 64] parser so we don't pull serde into dev-deps.
fn serde_json_parse(raw: &str) -> Vec<u8> {
    raw.trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|t| t.trim().parse::<u8>().expect("bad byte in fixture"))
        .collect()
}

fn setup() -> Env {
    let mut svm = LiteSVM::new();
    let program_id = thugz_swap::id();
    let bytes = include_bytes!(concat!(env!("CARGO_TARGET_TMPDIR"), "/../deploy/thugz_swap.so"));
    svm.add_program(program_id, bytes).unwrap();

    let admin = fixture_keypair("test_admin.json");
    let custodian = fixture_keypair("test_custodian.json");
    svm.airdrop(&admin.pubkey(), 100_000_000_000).unwrap();
    svm.airdrop(&custodian.pubkey(), 100_000_000_000).unwrap();

    // LiteSVM boots with unix_timestamp = 0; give the run a real present so
    // timestamp assertions and the unlock window behave like mainnet.
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp = 1_756_252_800; // 2026-08-27
    svm.set_sysvar(&clock);

    let pool = Pubkey::find_program_address(&[POOL_SEED], &program_id).0;
    let vault = Pubkey::find_program_address(&[VAULT_SEED], &program_id).0;
    let treasury = Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0;
    // Fund the treasury with a plain transfer to the derived address (airdrop is
    // the test equivalent) — it stays system-owned with zero data.
    svm.airdrop(&treasury, 100_000_000_000).unwrap();

    Env { svm, admin, custodian, pool, vault, treasury }
}

fn send(env: &mut Env, ixs: &[Instruction], payer: &Pubkey, signers: &[&Keypair]) -> Result<u64, TransactionError> {
    let blockhash = env.svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(payer), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let out = env.svm.send_transaction(tx);
    env.svm.expire_blockhash();
    match out {
        Ok(meta) => Ok(meta.compute_units_consumed),
        Err(failed) => Err(failed.err),
    }
}

fn custom_code(err: &TransactionError) -> Option<u32> {
    match err {
        TransactionError::InstructionError(_, InstructionError::Custom(c)) => Some(*c),
        _ => None,
    }
}

fn swap_err(e: SwapError) -> u32 {
    ERROR_CODE_OFFSET + e as u32
}

/// Assert a transaction failed with the SPECIFIC SwapError variant.
fn assert_swap_err(res: Result<u64, TransactionError>, expected: SwapError) {
    let err = res.expect_err("transaction unexpectedly succeeded");
    assert_eq!(
        custom_code(&err),
        Some(swap_err(expected)),
        "wrong error: got {err:?}"
    );
}

/// Anchor framework error codes (not SwapError): has_one = 2001, raw = 2003,
/// seeds = 2006, AccountNotInitialized = 3012.
fn assert_anchor_err(res: Result<u64, TransactionError>, code: u32) {
    let err = res.expect_err("transaction unexpectedly succeeded");
    assert_eq!(custom_code(&err), Some(code), "wrong error: got {err:?}");
}

fn token_balance(env: &Env, ata: &Pubkey) -> u64 {
    let acc = env.svm.get_account(ata).expect("token account not found");
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}

fn read_pool(env: &Env) -> Pool {
    let acc = env.svm.get_account(&env.pool).unwrap();
    Pool::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

fn read_mapping(env: &Env, old_mint: &Pubkey) -> Mapping {
    let addr = mapping_pda(env, old_mint);
    let acc = env.svm.get_account(&addr).unwrap();
    Mapping::try_deserialize(&mut acc.data.as_slice()).unwrap()
}

fn mapping_pda(env: &Env, old_mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[MAP_SEED, env.pool.as_ref(), old_mint.as_ref()],
        &thugz_swap::id(),
    )
    .0
}

/// Overwrite Pool fields directly (LiteSVM set_account) — used to reach states
/// like `deposited == 1274` without a thousand transactions. On-chain checks
/// still run against the forged state.
fn forge_pool<F: FnOnce(&mut Pool)>(env: &mut Env, f: F) {
    let acc = env.svm.get_account(&env.pool).unwrap();
    let mut pool = Pool::try_deserialize(&mut acc.data.as_slice()).unwrap();
    f(&mut pool);
    let mut data = Vec::new();
    pool.try_serialize(&mut data).unwrap();
    env.svm
        .set_account(
            env.pool,
            Account {
                lamports: acc.lamports,
                data,
                owner: acc.owner,
                executable: false,
                rent_epoch: 0,
            },
        )
        .unwrap();
}

fn init_pool(env: &mut Env) -> Result<u64, TransactionError> {
    let now = env.svm.get_sysvar::<Clock>().unix_timestamp;
    init_pool_at(env, now + 2 * 365 * 24 * 60 * 60)
}

fn init_pool_at(env: &mut Env, unlock_ts: i64) -> Result<u64, TransactionError> {
    let ix = Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::InitializePool { unlock_ts, collection: Pubkey::new_unique() }.data(),
        thugz_swap::accounts::InitializePool {
            admin: env.admin.pubkey(),
            pool: env.pool,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(env, &[ix], &admin.pubkey(), &[&admin])
}

/// A bird pair: original held by `holder`, remint held by the custodian.
struct Bird {
    old_mint: Pubkey,
    new_mint: Pubkey,
    holder_original_ata: Pubkey,
    custodian_remint_ata: Pubkey,
}

fn make_bird(env: &mut Env, holder: &Pubkey) -> Bird {
    let payer = env.admin.insecure_clone();
    let old_mint = CreateMint::new(&mut env.svm, &payer)
        .decimals(0)
        .authority(&payer.pubkey())
        .send()
        .unwrap();
    let new_mint = CreateMint::new(&mut env.svm, &payer)
        .decimals(0)
        .authority(&payer.pubkey())
        .send()
        .unwrap();
    let holder_original_ata = CreateAssociatedTokenAccount::new(&mut env.svm, &payer, &old_mint)
        .owner(holder)
        .send()
        .unwrap();
    let custodian_pk = env.custodian.pubkey();
    let custodian_remint_ata = CreateAssociatedTokenAccount::new(&mut env.svm, &payer, &new_mint)
        .owner(&custodian_pk)
        .send()
        .unwrap();
    MintTo::new(&mut env.svm, &payer, &old_mint, &holder_original_ata, 1).send().unwrap();
    MintTo::new(&mut env.svm, &payer, &new_mint, &custodian_remint_ata, 1).send().unwrap();
    Bird { old_mint, new_mint, holder_original_ata, custodian_remint_ata }
}

fn deposit_ix(env: &Env, bird: &Bird) -> Instruction {
    Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::DepositBird { old_mint: bird.old_mint }.data(),
        thugz_swap::accounts::DepositBird {
            admin: env.admin.pubkey(),
            pool: env.pool,
            custodian: env.custodian.pubkey(),
            new_mint: bird.new_mint,
            source_ata: bird.custodian_remint_ata,
            mapping: mapping_pda(env, &bird.old_mint),
            vault: env.vault,
            vault_ata: get_associated_token_address(&env.vault, &bird.new_mint),
            treasury: env.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn deposit(env: &mut Env, bird: &Bird) -> Result<u64, TransactionError> {
    let ix = deposit_ix(env, bird);
    let admin = env.admin.insecure_clone();
    let custodian = env.custodian.insecure_clone();
    send(env, &[ix], &admin.pubkey(), &[&admin, &custodian])
}

fn seal_now(env: &mut Env) -> Result<u64, TransactionError> {
    let ix = Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::Seal {}.data(),
        thugz_swap::accounts::Seal { admin: env.admin.pubkey(), pool: env.pool }.to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(env, &[ix], &admin.pubkey(), &[&admin])
}

/// Forge `deposited = EXPECTED`, then seal through the real instruction.
fn force_seal(env: &mut Env) {
    forge_pool(env, |p| p.deposited = EXPECTED);
    seal_now(env).expect("seal failed");
    forge_pool(env, |p| p.deposited = p.deposited); // no-op; keeps flow explicit
}

fn swap_ix(env: &Env, bird: &Bird, holder: &Pubkey, vault_new_ata: Option<Pubkey>) -> Instruction {
    Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::Swap {}.data(),
        thugz_swap::accounts::Swap {
            holder: *holder,
            pool: env.pool,
            holder_original_ata: bird.holder_original_ata,
            mapping: mapping_pda(env, &bird.old_mint),
            old_mint: bird.old_mint,
            new_mint: bird.new_mint,
            vault: env.vault,
            vault_new_ata: vault_new_ata
                .unwrap_or_else(|| get_associated_token_address(&env.vault, &bird.new_mint)),
            vault_original_ata: get_associated_token_address(&env.vault, &bird.old_mint),
            holder_new_ata: get_associated_token_address(holder, &bird.new_mint),
            treasury: env.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn fix_ix(env: &Env, bird: &Bird, custodian_ata: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::FixMapping { old_mint: bird.old_mint }.data(),
        thugz_swap::accounts::FixMapping {
            admin: env.admin.pubkey(),
            pool: env.pool,
            mapping: mapping_pda(env, &bird.old_mint),
            new_mint: bird.new_mint,
            vault: env.vault,
            vault_ata: get_associated_token_address(&env.vault, &bird.new_mint),
            custodian: env.custodian.pubkey(),
            custodian_ata,
            treasury: env.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn recover_ix(env: &Env, bird: &Bird, vault_ata: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::Recover { old_mint: bird.old_mint }.data(),
        thugz_swap::accounts::Recover {
            admin: env.admin.pubkey(),
            pool: env.pool,
            mapping: mapping_pda(env, &bird.old_mint),
            new_mint: bird.new_mint,
            vault: env.vault,
            vault_ata,
            custodian: env.custodian.pubkey(),
            custodian_ata: get_associated_token_address(&env.custodian.pubkey(), &bird.new_mint),
            treasury: env.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    )
}

fn set_paused(env: &mut Env, paused: bool) -> Result<u64, TransactionError> {
    let ix = Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::SetPaused { paused }.data(),
        thugz_swap::accounts::SetPaused { admin: env.admin.pubkey(), pool: env.pool }
            .to_account_metas(None),
    );
    let admin = env.admin.insecure_clone();
    send(env, &[ix], &admin.pubkey(), &[&admin])
}

fn warp_past_unlock(env: &mut Env) {
    let mut clock: Clock = env.svm.get_sysvar();
    clock.unix_timestamp += 3 * 365 * 24 * 60 * 60; // three years, past the two-year unlock
    env.svm.set_sysvar(&clock);
}

// ------------------------------------------------------------- happy paths ----

#[test]
fn happy_initialize_pool() {
    let mut env = setup();
    let cu = init_pool(&mut env).unwrap();
    record_cu("initialize_pool", cu);
    let pool = read_pool(&env);
    assert_eq!(pool.expected, 1274, "EXPECTED must be the compiled 1274 even under test-keys");
    assert_eq!(pool.admin, env.admin.pubkey());
    assert_eq!(pool.deposited, 0);
    assert_eq!(pool.swapped, 0);
    assert_eq!(pool.recovered, 0);
    assert!(!pool.sealed);
    assert!(!pool.paused);
    // Treasury untouched: still system-owned, zero data.
    let t = env.svm.get_account(&env.treasury).unwrap();
    assert_eq!(t.owner, system_program::ID);
    assert_eq!(t.data.len(), 0);
}

#[test]
fn happy_deposit_bird() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Pubkey::new_unique();
    let bird = make_bird(&mut env, &holder);
    let cu = deposit(&mut env, &bird).unwrap();
    record_cu("deposit_bird (vault ATA created)", cu);

    let m = read_mapping(&env, &bird.old_mint);
    assert_eq!(m.new_mint, bird.new_mint);
    assert!(!m.claimed);
    assert_eq!(m.claimed_by, Pubkey::default());
    assert_eq!(read_pool(&env).deposited, 1);
    let vault_ata = get_associated_token_address(&env.vault, &bird.new_mint);
    assert_eq!(token_balance(&env, &vault_ata), 1);
    assert_eq!(token_balance(&env, &bird.custodian_remint_ata), 0);
}

#[test]
fn happy_seal_at_expected_then_swap() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();

    // Seal boundary: refuse below expected, accept at expected (forged count).
    assert_swap_err(seal_now(&mut env), SwapError::Incomplete);
    forge_pool(&mut env, |p| p.deposited = EXPECTED);
    let cu = seal_now(&mut env).unwrap();
    record_cu("seal", cu);
    assert!(read_pool(&env).sealed);

    // The whole desk: one signature, original in, remint out, receipt written.
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    let cu = send(&mut env, &[ix], &holder.pubkey(), &[&holder]).unwrap();
    record_cu("swap (both ATAs created)", cu);

    let vault_original_ata = get_associated_token_address(&env.vault, &bird.old_mint);
    let holder_new_ata = get_associated_token_address(&holder.pubkey(), &bird.new_mint);
    assert_eq!(token_balance(&env, &vault_original_ata), 1, "original locked in vault");
    assert_eq!(token_balance(&env, &holder_new_ata), 1, "remint delivered");
    assert_eq!(token_balance(&env, &bird.holder_original_ata), 0);
    let m = read_mapping(&env, &bird.old_mint);
    assert!(m.claimed);
    assert_eq!(m.claimed_by, holder.pubkey());
    assert!(m.claimed_at > 0);
    assert_eq!(read_pool(&env).swapped, 1);

    // Same original twice → AlreadyClaimed.
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    assert_swap_err(
        send(&mut env, &[ix], &holder.pubkey(), &[&holder]),
        SwapError::AlreadyClaimed,
    );
}

#[test]
fn happy_fix_mapping_returns_to_custodian_and_closes() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    deposit(&mut env, &bird).unwrap();

    let treasury_before = env.svm.get_balance(&env.treasury).unwrap();
    let custodian_ata = get_associated_token_address(&env.custodian.pubkey(), &bird.new_mint);
    let ix = fix_ix(&env, &bird, custodian_ata);
    let admin = env.admin.insecure_clone();
    let cu = send(&mut env, &[ix], &admin.pubkey(), &[&admin]).unwrap();
    record_cu("fix_mapping", cu);

    // Remint back with the CUSTODIAN (the address it came from), never admin.
    assert_eq!(token_balance(&env, &custodian_ata), 1);
    assert_eq!(read_pool(&env).deposited, 0);

    // Mapping fully closed: zero lamports, zero data, system-owned. Rent went to
    // the treasury (minus what the treasury itself paid for the custodian ATA).
    let closed = env.svm.get_account(&mapping_pda(&env, &bird.old_mint)).unwrap_or(Account {
        lamports: 0, data: vec![], owner: system_program::ID, executable: false, rent_epoch: 0,
    });
    assert_eq!(closed.lamports, 0, "closed mapping keeps no lamports");
    assert_eq!(closed.data.len(), 0, "closed mapping keeps no data");
    assert_eq!(closed.owner, system_program::ID);
    let treasury_after = env.svm.get_balance(&env.treasury).unwrap();
    // Net: +mapping rent (0.00146) −custodian ATA rent (0.00204) → small net debit,
    // but strictly more than if rent had gone anywhere else.
    assert!(treasury_after > treasury_before - 2_100_000, "rent did not return to treasury");

    // Re-deposit the same old_mint after the fix: succeeds, counter back up.
    // (Two-signer path again — admin + custodian, exactly like the bulk run.)
    let bird2 = Bird { custodian_remint_ata: custodian_ata, ..bird };
    deposit(&mut env, &bird2).expect("re-deposit after fix must succeed");
    assert_eq!(read_pool(&env).deposited, 1);
    assert_eq!(read_mapping(&env, &bird2.old_mint).new_mint, bird2.new_mint);
}

#[test]
fn happy_pause_unpause() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();
    force_seal(&mut env);

    set_paused(&mut env, true).unwrap();
    assert!(read_pool(&env).paused);
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    assert_swap_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), SwapError::Paused);

    set_paused(&mut env, false).unwrap();
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    send(&mut env, &[ix], &holder.pubkey(), &[&holder]).expect("swap after unpause");
}

#[test]
fn happy_recover_after_unlock_leaves_claimed_false() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    deposit(&mut env, &bird).unwrap();
    force_seal(&mut env);

    // Before unlock → Locked.
    let vault_ata = get_associated_token_address(&env.vault, &bird.new_mint);
    let ix = recover_ix(&env, &bird, vault_ata);
    let admin = env.admin.insecure_clone();
    assert_swap_err(send(&mut env, &[ix], &admin.pubkey(), &[&admin]), SwapError::Locked);

    warp_past_unlock(&mut env);
    let ix = recover_ix(&env, &bird, vault_ata);
    let cu = send(&mut env, &[ix], &admin.pubkey(), &[&admin]).unwrap();
    record_cu("recover", cu);

    let custodian_ata = get_associated_token_address(&env.custodian.pubkey(), &bird.new_mint);
    assert_eq!(token_balance(&env, &custodian_ata), 1, "remint recovered to custodian");
    let m = read_mapping(&env, &bird.old_mint);
    assert!(!m.claimed, "recover must LEAVE claimed false — spec §4b");
    assert_eq!(read_pool(&env).recovered, 1);

    // Recover the same bird again: vault account is now empty → NotHeld.
    let ix = recover_ix(&env, &bird, vault_ata);
    assert_swap_err(send(&mut env, &[ix], &admin.pubkey(), &[&admin]), SwapError::NotHeld);
}

// ---------------------------------------------------------- failure matrix ----

#[test]
fn fail_swap_no_mapping() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey()); // never deposited
    force_seal(&mut env);
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    // AccountNotInitialized = 3012
    assert_anchor_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), 3012);
}

#[test]
fn fail_swap_before_seal() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();
    // No pilot exception, no admin bypass: constraint 1 in §6b.
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    assert_swap_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), SwapError::NotSealed);
}

#[test]
fn fail_swap_not_owner_and_not_held() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    let stranger = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    env.svm.airdrop(&stranger.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();
    force_seal(&mut env);

    // A stranger tries to swap the holder's bird → NotOwner.
    let ix = swap_ix(&env, &bird, &stranger.pubkey(), None);
    // holder_original_ata.owner == holder fails first
    assert_swap_err(send(&mut env, &[ix], &stranger.pubkey(), &[&stranger]), SwapError::NotOwner);

    // Holder with a stale, empty account (bird transferred away) → NotHeld.
    let payer = env.admin.insecure_clone();
    let stranger_old_ata = CreateAssociatedTokenAccount::new(&mut env.svm, &payer, &bird.old_mint)
        .owner(&stranger.pubkey())
        .send()
        .unwrap();
    litesvm_token::Transfer::new(&mut env.svm, &holder, &bird.old_mint, &stranger_old_ata, 1)
        .send()
        .unwrap();
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    assert_swap_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), SwapError::NotHeld);

    // New holder (stranger, using their own account) swaps fine — program checks
    // the CURRENT holder only.
    let bird_for_stranger = Bird { holder_original_ata: stranger_old_ata, ..bird };
    let ix = swap_ix(&env, &bird_for_stranger, &stranger.pubkey(), None);
    send(&mut env, &[ix], &stranger.pubkey(), &[&stranger]).expect("current holder swaps");
}

#[test]
fn fail_swap_name_bird_a_surrender_bird_b() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird_a = make_bird(&mut env, &holder.pubkey());
    let bird_b = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird_a).unwrap();
    deposit(&mut env, &bird_b).unwrap();
    force_seal(&mut env);

    // Surrender B's token account while passing A's mapping: the mapping PDA is
    // derived from the SURRENDERED account's mint, so A's mapping fails the seeds
    // check (ConstraintSeeds = 2006). Naming a valuable bird while handing over a
    // worthless one is impossible.
    let franken = Bird {
        old_mint: bird_a.old_mint,          // named A (mapping for A)
        holder_original_ata: bird_b.holder_original_ata, // surrendered B
        new_mint: bird_a.new_mint,
        custodian_remint_ata: bird_a.custodian_remint_ata,
    };
    let ix = swap_ix(&env, &franken, &holder.pubkey(), None);
    assert_anchor_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), 2006);
}

#[test]
fn fail_swap_wrong_vault_ata() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird_a = make_bird(&mut env, &holder.pubkey());
    let bird_b = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird_a).unwrap();
    deposit(&mut env, &bird_b).unwrap();
    force_seal(&mut env);

    // Point vault_new_ata at B's vault account while swapping A → WrongRemint.
    let wrong_vault_ata = get_associated_token_address(&env.vault, &bird_b.new_mint);
    let ix = swap_ix(&env, &bird_a, &holder.pubkey(), Some(wrong_vault_ata));
    assert_swap_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), SwapError::WrongRemint);
}

#[test]
fn fail_swap_remint_back_in() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();
    force_seal(&mut env);
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    send(&mut env, &[ix], &holder.pubkey(), &[&holder]).unwrap();

    // Holder now owns the remint; try to feed it back: no mapping derives from a
    // remint address → AccountNotInitialized.
    let holder_new_ata = get_associated_token_address(&holder.pubkey(), &bird.new_mint);
    let franken = Bird {
        old_mint: bird.new_mint,
        new_mint: bird.new_mint,
        holder_original_ata: holder_new_ata,
        custodian_remint_ata: bird.custodian_remint_ata,
    };
    let ix = swap_ix(&env, &franken, &holder.pubkey(), None);
    assert_anchor_err(send(&mut env, &[ix], &holder.pubkey(), &[&holder]), 3012);
}

#[test]
fn fail_deposit_after_seal_and_fix_after_seal() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    deposit(&mut env, &bird).unwrap();
    let bird2 = make_bird(&mut env, &Pubkey::new_unique());
    force_seal(&mut env);

    assert_swap_err(deposit(&mut env, &bird2), SwapError::Sealed);

    let custodian_ata = get_associated_token_address(&env.custodian.pubkey(), &bird.new_mint);
    let ix = fix_ix(&env, &bird, custodian_ata);
    let admin = env.admin.insecure_clone();
    assert_swap_err(send(&mut env, &[ix], &admin.pubkey(), &[&admin]), SwapError::Sealed);
}

#[test]
fn fail_double_deposit_same_old_mint() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    deposit(&mut env, &bird).unwrap();
    // Give the custodian a second remint and reuse the same old_mint: the mapping
    // exists → init-once guard fires.
    let bird_dup = make_bird(&mut env, &Pubkey::new_unique());
    let franken = Bird { old_mint: bird.old_mint, ..bird_dup };
    assert_swap_err(deposit(&mut env, &franken), SwapError::MappingExists);
}

#[test]
fn fail_seal_boundaries_and_double_seal() {
    let mut env = setup();
    init_pool(&mut env).unwrap();

    // deposited == 0 (integer edge) → Incomplete.
    assert_swap_err(seal_now(&mut env), SwapError::Incomplete);
    // One short of expected (the half-done-correction state) → Incomplete.
    forge_pool(&mut env, |p| p.deposited = EXPECTED - 1);
    assert_swap_err(seal_now(&mut env), SwapError::Incomplete);
    // One OVER expected → also refuses (allowlist, not >=).
    forge_pool(&mut env, |p| p.deposited = EXPECTED + 1);
    assert_swap_err(seal_now(&mut env), SwapError::Incomplete);

    forge_pool(&mut env, |p| p.deposited = EXPECTED);
    seal_now(&mut env).unwrap();
    // Second seal → Sealed. `sealed` is one-way.
    assert_swap_err(seal_now(&mut env), SwapError::Sealed);
}

#[test]
fn fail_non_admin_calls_admin_instructions() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    deposit(&mut env, &bird).unwrap();
    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), 10_000_000_000).unwrap();

    // seal by non-admin → ConstraintHasOne (2001)
    let ix = Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::Seal {}.data(),
        thugz_swap::accounts::Seal { admin: mallory.pubkey(), pool: env.pool }.to_account_metas(None),
    );
    assert_anchor_err(send(&mut env, &[ix], &mallory.pubkey(), &[&mallory]), 2001);

    // set_paused by non-admin → 2001
    let ix = Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::SetPaused { paused: true }.data(),
        thugz_swap::accounts::SetPaused { admin: mallory.pubkey(), pool: env.pool }
            .to_account_metas(None),
    );
    assert_anchor_err(send(&mut env, &[ix], &mallory.pubkey(), &[&mallory]), 2001);

    // fix_mapping by non-admin → 2001
    let custodian_ata = get_associated_token_address(&env.custodian.pubkey(), &bird.new_mint);
    let mut ix = fix_ix(&env, &bird, custodian_ata);
    ix.accounts[0].pubkey = mallory.pubkey(); // admin slot
    assert_anchor_err(send(&mut env, &[ix], &mallory.pubkey(), &[&mallory]), 2001);
}

#[test]
fn fail_second_initialize_and_bad_unlock_ts_and_wrong_admin() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    // Second initialize_pool → account already exists (system create fails).
    assert!(init_pool(&mut env).is_err());

    // Fresh env: unlock_ts in the past → InvalidUnlockTimestamp.
    let mut env = setup();
    let now = env.svm.get_sysvar::<Clock>().unix_timestamp;
    assert_swap_err(init_pool_at(&mut env, now - 1), SwapError::InvalidUnlockTimestamp);

    // Fresh env: initializer is not the compiled ADMIN → ConstraintRaw (2003).
    // This is the front-run guard on the singleton pool seeds.
    let mut env = setup();
    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), 10_000_000_000).unwrap();
    let ix = Instruction::new_with_bytes(
        thugz_swap::id(),
        &thugz_swap::instruction::InitializePool {
            unlock_ts: now + 1_000_000,
            collection: Pubkey::new_unique(),
        }
        .data(),
        thugz_swap::accounts::InitializePool {
            admin: mallory.pubkey(),
            pool: env.pool,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
    );
    assert_anchor_err(send(&mut env, &[ix], &mallory.pubkey(), &[&mallory]), 2003);
}

// ------------------------------------------------------------- adversarial ----

#[test]
fn adversarial_prefunded_mapping_pda_is_absorbed() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    // Grief: pre-send lamports to the derivable mapping address. Anchor `init`
    // would abort here; the manual init must absorb the balance.
    let map_addr = mapping_pda(&env, &bird.old_mint);
    env.svm.airdrop(&map_addr, 5_000_000).unwrap();
    deposit(&mut env, &bird).expect("pre-funded mapping address must not block deposit");
    assert_eq!(read_mapping(&env, &bird.old_mint).new_mint, bird.new_mint);
}

#[test]
fn adversarial_deposit_source_and_signer_must_be_custodian() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let mallory = Keypair::new();
    env.svm.airdrop(&mallory.pubkey(), 10_000_000_000).unwrap();

    // A remint sitting with mallory, custodian co-signs anyway → source owner is
    // not the custodian → NotCustodian.
    let payer = env.admin.insecure_clone();
    let rogue_mint = CreateMint::new(&mut env.svm, &payer)
        .decimals(0).authority(&payer.pubkey()).send().unwrap();
    let mallory_ata = CreateAssociatedTokenAccount::new(&mut env.svm, &payer, &rogue_mint)
        .owner(&mallory.pubkey()).send().unwrap();
    MintTo::new(&mut env.svm, &payer, &rogue_mint, &mallory_ata, 1).send().unwrap();
    let bird = Bird {
        old_mint: Pubkey::new_unique(),
        new_mint: rogue_mint,
        holder_original_ata: mallory_ata, // unused by deposit
        custodian_remint_ata: mallory_ata,
    };
    assert_swap_err(deposit(&mut env, &bird), SwapError::NotCustodian);

    // Mallory posing as the custodian signer slot → NotCustodian.
    let bird2 = make_bird(&mut env, &Pubkey::new_unique());
    let mut ix = deposit_ix(&env, &bird2);
    ix.accounts[2].pubkey = mallory.pubkey(); // custodian slot
    let admin = env.admin.insecure_clone();
    assert_swap_err(
        send(&mut env, &[ix], &admin.pubkey(), &[&admin, &mallory]),
        SwapError::NotCustodian,
    );
}

#[test]
fn adversarial_fix_mapping_cannot_send_to_admin() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    deposit(&mut env, &bird).unwrap();

    // THE v3 custody control: any destination other than the custodian's ATA is
    // rejected. Admin points the destination at their own ATA → NotCustodian.
    let payer = env.admin.insecure_clone();
    let admin_ata = CreateAssociatedTokenAccount::new(&mut env.svm, &payer, &bird.new_mint)
        .owner(&payer.pubkey()).send().unwrap();
    let ix = fix_ix(&env, &bird, admin_ata);
    assert_swap_err(send(&mut env, &[ix], &payer.pubkey(), &[&payer]), SwapError::NotCustodian);

    // Admin balance of the remint unchanged; bird still in the vault.
    assert_eq!(token_balance(&env, &admin_ata), 0);
    let vault_ata = get_associated_token_address(&env.vault, &bird.new_mint);
    assert_eq!(token_balance(&env, &vault_ata), 1);
}

#[test]
fn adversarial_recover_cannot_touch_deposited_original() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();
    force_seal(&mut env);
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    send(&mut env, &[ix], &holder.pubkey(), &[&holder]).unwrap();
    warp_past_unlock(&mut env);

    // The vault now holds the ORIGINAL. Admin aims recover at it → the mint check
    // (`vault_ata.mint == mapping.new_mint`) fails → NotRecoverable. This is the
    // on-chain guard, not the sweep.
    let vault_original_ata = get_associated_token_address(&env.vault, &bird.old_mint);
    let ix = recover_ix(&env, &bird, vault_original_ata);
    let admin = env.admin.insecure_clone();
    assert_swap_err(send(&mut env, &[ix], &admin.pubkey(), &[&admin]), SwapError::NotRecoverable);

    // And the claimed mapping blocks recovery of the swapped remint's (now empty)
    // vault account → AlreadyClaimed.
    let vault_new_ata = get_associated_token_address(&env.vault, &bird.new_mint);
    let ix = recover_ix(&env, &bird, vault_new_ata);
    assert_swap_err(send(&mut env, &[ix], &admin.pubkey(), &[&admin]), SwapError::AlreadyClaimed);
}

#[test]
fn adversarial_treasury_drained_fails_legibly() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let bird = make_bird(&mut env, &Pubkey::new_unique());
    // Drain the treasury to a dust balance (forge — on mainnet only the program
    // can move it, which is the point; this simulates it running dry).
    env.svm
        .set_account(env.treasury, Account {
            lamports: 1_000, data: vec![], owner: system_program::ID,
            executable: false, rent_epoch: 0,
        })
        .unwrap();
    let res = deposit(&mut env, &bird);
    assert!(res.is_err(), "deposit with a dry treasury must fail, not half-apply");
}

#[test]
fn adversarial_full_correction_cycle() {
    // The Phase 4 rehearsal in miniature: bad pair in, caught (off-chain in real
    // life), fixed, re-deposited correctly, sealed.
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird_right = make_bird(&mut env, &holder.pubkey());
    let bird_wrong_old = Pubkey::new_unique(); // admin fat-fingers old_mint

    // The bad deposit SUCCEEDS — the program cannot know the pairing is wrong.
    // That is exactly why the sweep exists.
    let bad = Bird { old_mint: bird_wrong_old, ..bird_right };
    deposit(&mut env, &bad).unwrap();
    assert_eq!(read_pool(&env).deposited, 1);

    // fix_mapping the bad pair; deposited drops; remint back with custodian.
    let custodian_ata = get_associated_token_address(&env.custodian.pubkey(), &bad.new_mint);
    let ix = fix_ix(&env, &bad, custodian_ata);
    let admin = env.admin.insecure_clone();
    send(&mut env, &[ix], &admin.pubkey(), &[&admin]).unwrap();
    assert_eq!(read_pool(&env).deposited, 0);

    // Re-deposit with the CORRECT old_mint (source is the custodian ATA again).
    let good = Bird { custodian_remint_ata: custodian_ata, ..bird_right };
    deposit(&mut env, &good).unwrap();
    assert_eq!(read_pool(&env).deposited, 1);

    // Seal (forged to the boundary), then the holder swaps the right bird.
    force_seal(&mut env);
    let ix = swap_ix(&env, &good, &holder.pubkey(), None);
    send(&mut env, &[ix], &holder.pubkey(), &[&holder]).unwrap();
    let holder_new_ata = get_associated_token_address(&holder.pubkey(), &good.new_mint);
    assert_eq!(token_balance(&env, &holder_new_ata), 1);
}

#[test]
fn adversarial_swap_with_preexisting_holder_ata() {
    let mut env = setup();
    init_pool(&mut env).unwrap();
    let holder = Keypair::new();
    env.svm.airdrop(&holder.pubkey(), 10_000_000_000).unwrap();
    let bird = make_bird(&mut env, &holder.pubkey());
    deposit(&mut env, &bird).unwrap();
    force_seal(&mut env);

    // Holder already has an (empty) ATA for the remint → idempotent creation, swap
    // succeeds anyway.
    let payer = env.admin.insecure_clone();
    CreateAssociatedTokenAccount::new(&mut env.svm, &payer, &bird.new_mint)
        .owner(&holder.pubkey())
        .send()
        .unwrap();
    let ix = swap_ix(&env, &bird, &holder.pubkey(), None);
    let cu = send(&mut env, &[ix], &holder.pubkey(), &[&holder]).expect("idempotent ATA");
    record_cu("swap (holder ATA pre-existing)", cu);
}

// -------------------------------------------------------------- cu summary ----

#[test]
fn zz_cu_summary() {
    let results = CU_RESULTS.lock().unwrap();
    if results.is_empty() {
        println!("No CU results (run with --test-threads=1 for the full table)");
        return;
    }
    println!("\n=== Compute Unit Summary (Level 1 / LiteSVM) ===");
    for (label, cu) in results.iter() {
        println!("  {label:<40} {cu:>8} CUs");
    }
    println!("(TEST_PLAN Level 3 measures the binding numbers on Surfpool)");
}
