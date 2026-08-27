//! Phase 3 devnet driver — runs the TEST_PLAN failure matrix against the REAL
//! devnet deployment of the test-keys build (EXPECTED = 20).
//!
//!   cargo run -p devnet-driver
//!
//! One-shot by design: the Pool is a singleton, so a completed run leaves devnet
//! sealed. To rerun from scratch, redeploy the program at a fresh throwaway
//! program id (or accept post-seal-only scenarios).
//!
//! Every scenario prints PASS/FAIL and asserts SPECIFIC error codes; exit code
//! is non-zero if anything failed. Total runtime ~18 minutes (the unlock window
//! for `recover` is set 15 minutes out and the driver waits for it).

use anchor_lang::error::ERROR_CODE_OFFSET;
use anchor_lang::prelude::Pubkey;
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::associated_token::ID as ATA_PROGRAM_ID;
use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
use solana_commitment_config::CommitmentConfig;
use solana_instruction::error::InstructionError;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use thugz_swap::error::SwapError;
use thugz_swap::{EXPECTED, MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

const RPC_URL: &str = "https://api.devnet.solana.com";
const SOL: u64 = 1_000_000_000;

struct Ctx {
    program_id: Pubkey,
    rpc: RpcClient,
    payer: Keypair,
    admin: Keypair,
    custodian: Keypair,
    pool: Pubkey,
    vault: Pubkey,
    treasury: Pubkey,
    pass: u32,
    fail: u32,
}

fn keypair_from_json(path: &str) -> Keypair {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing keypair {path}"));
    let bytes: Vec<u8> = raw
        .trim().trim_start_matches('[').trim_end_matches(']')
        .split(',').map(|t| t.trim().parse::<u8>().unwrap()).collect();
    Keypair::try_from(&bytes[..]).unwrap()
}

fn home(p: &str) -> String {
    format!("{}/{}", std::env::var("HOME").unwrap(), p)
}

impl Ctx {
    fn send(&self, ixs: &[Instruction], signers: &[&Keypair]) -> Result<(), TransactionError> {
        let blockhash = self.rpc.get_latest_blockhash().expect("blockhash");
        let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &blockhash);
        let tx = Transaction::new(signers, msg, blockhash);
        match self.rpc.send_and_confirm_transaction(&tx) {
            Ok(_) => Ok(()),
            Err(e) => match e.get_transaction_error() {
                Some(te) => Err(te),
                None => panic!("RPC error (not a tx error): {e}"),
            },
        }
    }

    fn check(&mut self, label: &str, ok: bool, detail: String) {
        if ok {
            self.pass += 1;
            println!("PASS  {label}");
        } else {
            self.fail += 1;
            println!("FAIL  {label} — {detail}");
        }
    }

    fn expect_ok(&mut self, label: &str, res: Result<(), TransactionError>) {
        let detail = format!("{res:?}");
        self.check(label, res.is_ok(), detail);
    }

    fn expect_err_code(&mut self, label: &str, res: Result<(), TransactionError>, code: u32) {
        let got = match &res {
            Err(TransactionError::InstructionError(_, InstructionError::Custom(c))) => Some(*c),
            _ => None,
        };
        let detail = format!("wanted custom {code}, got {res:?}");
        self.check(label, got == Some(code), detail);
    }

    fn expect_swap_err(&mut self, label: &str, res: Result<(), TransactionError>, e: SwapError) {
        self.expect_err_code(label, res, ERROR_CODE_OFFSET + e as u32);
    }

    fn token_balance(&self, ata: &Pubkey) -> u64 {
        match self.rpc.get_account(ata) {
            Ok(a) if a.data.len() >= 72 => u64::from_le_bytes(a.data[64..72].try_into().unwrap()),
            _ => 0,
        }
    }

    fn mapping_pda(&self, old_mint: &Pubkey) -> Pubkey {
        Pubkey::find_program_address(
            &[MAP_SEED, self.pool.as_ref(), old_mint.as_ref()],
            &self.program_id,
        ).0
    }

    fn transfer(&self, to: &Pubkey, lamports: u64) {
        let ix = solana_system_interface::instruction::transfer(&self.payer.pubkey(), to, lamports);
        self.send(&[ix], &[&self.payer]).expect("funding transfer");
    }
}

struct Bird {
    old_mint: Pubkey,
    new_mint: Pubkey,
    holder: Keypair,
    holder_original_ata: Pubkey,
    custodian_remint_ata: Pubkey,
}

/// One transaction: create+init both mints, both ATAs, mint 1 of each.
fn make_bird(ctx: &Ctx) -> Bird {
    let old = Keypair::new();
    let new = Keypair::new();
    let holder = Keypair::new();
    let payer = ctx.payer.pubkey();
    let rent = ctx.rpc.get_minimum_balance_for_rent_exemption(82).unwrap();
    let mut ixs = vec![];
    for mint in [&old, &new] {
        ixs.push(solana_system_interface::instruction::create_account(
            &payer, &mint.pubkey(), rent, 82, &TOKEN_PROGRAM_ID,
        ));
        ixs.push(
            spl_token_interface::instruction::initialize_mint2(
                &TOKEN_PROGRAM_ID, &mint.pubkey(), &payer, None, 0,
            ).unwrap(),
        );
    }
    let holder_original_ata = get_associated_token_address(&holder.pubkey(), &old.pubkey());
    let custodian_remint_ata = get_associated_token_address(&ctx.custodian.pubkey(), &new.pubkey());
    ixs.push(spl_associated_token_account_interface::instruction::create_associated_token_account(
        &payer, &holder.pubkey(), &old.pubkey(), &TOKEN_PROGRAM_ID,
    ));
    ixs.push(spl_associated_token_account_interface::instruction::create_associated_token_account(
        &payer, &ctx.custodian.pubkey(), &new.pubkey(), &TOKEN_PROGRAM_ID,
    ));
    ixs.push(spl_token_interface::instruction::mint_to(
        &TOKEN_PROGRAM_ID, &old.pubkey(), &holder_original_ata, &payer, &[], 1,
    ).unwrap());
    ixs.push(spl_token_interface::instruction::mint_to(
        &TOKEN_PROGRAM_ID, &new.pubkey(), &custodian_remint_ata, &payer, &[], 1,
    ).unwrap());
    // fund the holder for their swap fee
    ixs.push(solana_system_interface::instruction::transfer(&payer, &holder.pubkey(), SOL / 50));
    ctx.send(&ixs, &[&ctx.payer, &old, &new]).expect("make_bird");
    Bird {
        old_mint: old.pubkey(),
        new_mint: new.pubkey(),
        holder,
        holder_original_ata,
        custodian_remint_ata,
    }
}

fn init_pool_ix(ctx: &Ctx, admin: &Pubkey, unlock_ts: i64) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::InitializePool { unlock_ts, collection: Pubkey::new_unique() }.data(),
        thugz_swap::accounts::InitializePool {
            admin: *admin,
            pool: ctx.pool,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    )
}

fn deposit_ix(ctx: &Ctx, bird: &Bird, source_ata: &Pubkey, custodian: &Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::DepositBird { old_mint: bird.old_mint }.data(),
        thugz_swap::accounts::DepositBird {
            admin: ctx.admin.pubkey(),
            pool: ctx.pool,
            custodian: *custodian,
            new_mint: bird.new_mint,
            source_ata: *source_ata,
            mapping: ctx.mapping_pda(&bird.old_mint),
            vault: ctx.vault,
            vault_ata: get_associated_token_address(&ctx.vault, &bird.new_mint),
            treasury: ctx.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    )
}

fn deposit(ctx: &Ctx, bird: &Bird) -> Result<(), TransactionError> {
    let ix = deposit_ix(ctx, bird, &bird.custodian_remint_ata, &ctx.custodian.pubkey());
    ctx.send(&[ix], &[&ctx.admin, &ctx.custodian])
}

fn seal(ctx: &Ctx) -> Result<(), TransactionError> {
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::Seal {}.data(),
        thugz_swap::accounts::Seal { admin: ctx.admin.pubkey(), pool: ctx.pool }.to_account_metas(None),
    );
    ctx.send(&[ix], &[&ctx.admin])
}

fn set_paused(ctx: &Ctx, paused: bool) -> Result<(), TransactionError> {
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::SetPaused { paused }.data(),
        thugz_swap::accounts::SetPaused { admin: ctx.admin.pubkey(), pool: ctx.pool }.to_account_metas(None),
    );
    ctx.send(&[ix], &[&ctx.admin])
}

fn swap_ix(ctx: &Ctx, bird: &Bird, holder: &Pubkey, mapping: Pubkey, surrendered: Pubkey, vault_new_ata: Pubkey) -> Instruction {
    Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::Swap {}.data(),
        thugz_swap::accounts::Swap {
            holder: *holder,
            pool: ctx.pool,
            holder_original_ata: surrendered,
            mapping,
            old_mint: bird.old_mint,
            new_mint: bird.new_mint,
            vault: ctx.vault,
            vault_new_ata,
            vault_original_ata: get_associated_token_address(&ctx.vault, &bird.old_mint),
            holder_new_ata: get_associated_token_address(holder, &bird.new_mint),
            treasury: ctx.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    )
}

fn swap(ctx: &Ctx, bird: &Bird) -> Result<(), TransactionError> {
    let ix = swap_ix(
        ctx, bird, &bird.holder.pubkey(),
        ctx.mapping_pda(&bird.old_mint),
        bird.holder_original_ata,
        get_associated_token_address(&ctx.vault, &bird.new_mint),
    );
    ctx.send(&[ix], &[&bird.holder])
}

fn fix_mapping(ctx: &Ctx, bird: &Bird, custodian_ata: Pubkey) -> Result<(), TransactionError> {
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::FixMapping { old_mint: bird.old_mint }.data(),
        thugz_swap::accounts::FixMapping {
            admin: ctx.admin.pubkey(),
            pool: ctx.pool,
            mapping: ctx.mapping_pda(&bird.old_mint),
            new_mint: bird.new_mint,
            vault: ctx.vault,
            vault_ata: get_associated_token_address(&ctx.vault, &bird.new_mint),
            custodian: ctx.custodian.pubkey(),
            custodian_ata,
            treasury: ctx.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    );
    ctx.send(&[ix], &[&ctx.admin])
}

fn recover(ctx: &Ctx, bird: &Bird, vault_ata: Pubkey) -> Result<(), TransactionError> {
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::Recover { old_mint: bird.old_mint }.data(),
        thugz_swap::accounts::Recover {
            admin: ctx.admin.pubkey(),
            pool: ctx.pool,
            mapping: ctx.mapping_pda(&bird.old_mint),
            new_mint: bird.new_mint,
            vault: ctx.vault,
            vault_ata,
            custodian: ctx.custodian.pubkey(),
            custodian_ata: get_associated_token_address(&ctx.custodian.pubkey(), &bird.new_mint),
            treasury: ctx.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    );
    ctx.send(&[ix], &[&ctx.admin])
}

fn now_ts() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64
}

fn main() {
    // THUGZ_PROGRAM_ID overrides the target (throwaway devnet reruns — the pool is
    // a singleton, so a completed run permanently seals a deployment).
    let program_id: Pubkey = std::env::var("THUGZ_PROGRAM_ID")
        .ok()
        .map(|v| v.parse().expect("bad THUGZ_PROGRAM_ID"))
        .unwrap_or_else(thugz_swap::id);
    let mut ctx = Ctx {
        program_id,
        rpc: RpcClient::new_with_commitment(RPC_URL.to_string(), CommitmentConfig::confirmed()),
        payer: keypair_from_json(&home(".thugbirdz-keys/devnet/dev.json")),
        admin: keypair_from_json(concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/thugz-swap/tests/fixtures/test_admin.json")),
        custodian: keypair_from_json(concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/thugz-swap/tests/fixtures/test_custodian.json")),
        pool: Pubkey::find_program_address(&[POOL_SEED], &program_id).0,
        vault: Pubkey::find_program_address(&[VAULT_SEED], &program_id).0,
        treasury: Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0,
        pass: 0,
        fail: 0,
    };
    println!("driver: program {program_id}");
    println!("driver: pool {}, treasury {}, EXPECTED = {EXPECTED}", ctx.pool, ctx.treasury);

    if ctx.rpc.get_account(&ctx.pool).is_ok() {
        eprintln!("Pool already exists on devnet — this driver is one-shot per deployment.");
        eprintln!("Redeploy at a fresh program id to rerun the full sequence.");
        std::process::exit(2);
    }

    // ---- funding ----
    ctx.transfer(&ctx.admin.pubkey().clone(), SOL / 2);
    ctx.transfer(&ctx.custodian.pubkey().clone(), SOL / 2);
    ctx.transfer(&ctx.treasury.clone(), SOL); // plain system transfer to the derived address
    println!("driver: funded admin / custodian / treasury");

    // ---- initialize_pool matrix ----
    let unlock_ts = now_ts() + 15 * 60;
    let ix = init_pool_ix(&ctx, &ctx.admin.pubkey(), now_ts() - 100);
    let r = ctx.send(&[ix], &[&ctx.admin]);
    ctx.expect_swap_err("init with past unlock_ts", r, SwapError::InvalidUnlockTimestamp);

    let mallory = Keypair::new();
    ctx.transfer(&mallory.pubkey(), SOL / 10);
    let ix = init_pool_ix(&ctx, &mallory.pubkey(), unlock_ts);
    let r = ctx.send(&[ix], &[&mallory]);
    ctx.expect_err_code("init by non-ADMIN (front-run guard)", r, 2003);

    let ix = init_pool_ix(&ctx, &ctx.admin.pubkey(), unlock_ts);
    let r = ctx.send(&[ix], &[&ctx.admin]);
    ctx.expect_ok("initialize_pool", r);

    let ix = init_pool_ix(&ctx, &ctx.admin.pubkey(), unlock_ts);
    let r = ctx.send(&[ix], &[&ctx.admin]);
    ctx.check("second initialize_pool refused", r.is_err(), format!("{r:?}"));

    // ---- mint the mock set ----
    println!("driver: minting {EXPECTED} mock pairs…");
    let birds: Vec<Bird> = (0..EXPECTED).map(|i| {
        let b = make_bird(&ctx);
        if i % 5 == 4 { println!("driver: minted {}/{EXPECTED}", i + 1); }
        b
    }).collect();

    // ---- deposit matrix ----
    let r = deposit(&ctx, &birds[0]);
    ctx.expect_ok("deposit_bird (first)", r);
    let vault_ata0 = get_associated_token_address(&ctx.vault, &birds[0].new_mint);
    let held = ctx.token_balance(&vault_ata0) == 1;
    ctx.check("remint held by vault after deposit", held, "vault ATA empty".into());

    // Same bird again: its remint already moved to the vault, so the custodian's
    // source ATA is empty and NotHeld fires FIRST (the program's check order).
    let r = deposit(&ctx, &birds[0]);
    ctx.expect_swap_err("double deposit same bird (empty source first)", r, SwapError::NotHeld);
    // True MappingExists: a FRESH remint (custodian holds it) against the same old_mint.
    let fresh = make_bird(&ctx);
    let franken = Bird {
        old_mint: birds[0].old_mint,
        new_mint: fresh.new_mint,
        holder: Keypair::new(),
        holder_original_ata: fresh.holder_original_ata,
        custodian_remint_ata: fresh.custodian_remint_ata,
    };
    let r = deposit(&ctx, &franken);
    ctx.expect_swap_err("double deposit same old_mint (fresh remint)", r, SwapError::MappingExists);

    // source not owned by custodian: move a rogue REMINT to mallory first, so the
    // mint matches and only the owner is wrong (else WrongRemint fires first)
    let rogue = make_bird(&ctx);
    let mallory_rogue_ata = get_associated_token_address(&mallory.pubkey(), &rogue.new_mint);
    let ixs = vec![
        spl_associated_token_account_interface::instruction::create_associated_token_account(
            &ctx.payer.pubkey(), &mallory.pubkey(), &rogue.new_mint, &TOKEN_PROGRAM_ID,
        ),
        spl_token_interface::instruction::transfer_checked(
            &TOKEN_PROGRAM_ID, &rogue.custodian_remint_ata, &rogue.new_mint,
            &mallory_rogue_ata, &ctx.custodian.pubkey(), &[], 1, 0,
        ).unwrap(),
    ];
    ctx.send(&ixs, &[&ctx.payer, &ctx.custodian]).expect("stage rogue remint");
    let ix = deposit_ix(&ctx, &rogue, &mallory_rogue_ata, &ctx.custodian.pubkey());
    let r = ctx.send(&[ix], &[&ctx.admin, &ctx.custodian]);
    ctx.expect_swap_err("deposit from non-custodian source", r, SwapError::NotCustodian);

    // imposter in the custodian slot
    let ix = deposit_ix(&ctx, &rogue, &rogue.custodian_remint_ata, &mallory.pubkey());
    let r = ctx.send(&[ix], &[&ctx.admin, &mallory]);
    ctx.expect_swap_err("imposter custodian signer", r, SwapError::NotCustodian);

    // non-admin deposit
    let mut ix = deposit_ix(&ctx, &birds[1], &birds[1].custodian_remint_ata, &ctx.custodian.pubkey());
    ix.accounts[0].pubkey = mallory.pubkey();
    let r = ctx.send(&[ix], &[&mallory, &ctx.custodian]);
    ctx.expect_err_code("non-admin deposit", r, 2001);

    // swap before seal
    let r = swap(&ctx, &birds[0]);
    ctx.expect_swap_err("swap before seal", r, SwapError::NotSealed);

    // fix_mapping to admin's ATA — the custody control
    let admin_ata = get_associated_token_address(&ctx.admin.pubkey(), &birds[0].new_mint);
    let r = fix_mapping(&ctx, &birds[0], admin_ata);
    ctx.expect_swap_err("fix_mapping cannot send to admin", r, SwapError::NotCustodian);

    // fix_mapping happy: back to custodian, mapping closed, re-deposit
    let r = fix_mapping(&ctx, &birds[0], birds[0].custodian_remint_ata);
    ctx.expect_ok("fix_mapping to custodian", r);
    let closed = ctx.rpc.get_account(&ctx.mapping_pda(&birds[0].old_mint)).is_err();
    ctx.check("mapping closed after fix", closed, "mapping account still exists".into());
    let back = ctx.token_balance(&birds[0].custodian_remint_ata) == 1;
    ctx.check("remint back with custodian after fix", back, "not returned".into());
    let r = deposit(&ctx, &birds[0]);
    ctx.expect_ok("re-deposit after fix", r);

    // lamport-griefing: pre-fund the last bird's mapping PDA, deposit must absorb
    let last = &birds[EXPECTED as usize - 1];
    ctx.transfer(&ctx.mapping_pda(&last.old_mint), 5_000_000);

    // deposit the rest (leave the last for the Incomplete check)
    for (i, b) in birds.iter().enumerate().skip(1).take(EXPECTED as usize - 2) {
        deposit(&ctx, b).unwrap_or_else(|e| panic!("bulk deposit {i}: {e:?}"));
    }
    println!("driver: bulk deposits done");

    let r = seal(&ctx);
    ctx.expect_swap_err("seal one short of expected", r, SwapError::Incomplete);

    let r = deposit(&ctx, last);
    ctx.expect_ok("deposit onto pre-funded mapping PDA (griefing absorbed)", r);

    let r = seal(&ctx);
    ctx.expect_ok("seal at expected", r);
    let r = seal(&ctx);
    ctx.expect_swap_err("second seal", r, SwapError::Sealed);

    let post_seal_bird = make_bird(&ctx);
    let r = deposit(&ctx, &post_seal_bird);
    ctx.expect_swap_err("deposit after seal", r, SwapError::Sealed);
    let r = fix_mapping(&ctx, &birds[2], birds[2].custodian_remint_ata);
    ctx.expect_swap_err("fix_mapping after seal", r, SwapError::Sealed);

    // ---- swap matrix ----
    let r = swap(&ctx, &birds[1]);
    ctx.expect_ok("swap (the whole desk)", r);
    let got = ctx.token_balance(&get_associated_token_address(&birds[1].holder.pubkey(), &birds[1].new_mint));
    let locked = ctx.token_balance(&get_associated_token_address(&ctx.vault, &birds[1].old_mint));
    ctx.check("holder received remint", got == 1, format!("balance {got}"));
    ctx.check("original locked in vault", locked == 1, format!("balance {locked}"));

    let r = swap(&ctx, &birds[1]);
    ctx.expect_swap_err("swap same original twice", r, SwapError::AlreadyClaimed);

    // name bird A, surrender bird B → seeds violation
    let ix = swap_ix(
        &ctx, &birds[2], &birds[2].holder.pubkey(),
        ctx.mapping_pda(&birds[3].old_mint), // A's... rather, wrong mapping
        birds[2].holder_original_ata,
        get_associated_token_address(&ctx.vault, &birds[2].new_mint),
    );
    let r = ctx.send(&[ix], &[&birds[2].holder]);
    ctx.expect_err_code("name bird A surrender bird B (seeds)", r, 2006);

    // vault_new_ata pointed at a different remint
    let ix = swap_ix(
        &ctx, &birds[2], &birds[2].holder.pubkey(),
        ctx.mapping_pda(&birds[2].old_mint),
        birds[2].holder_original_ata,
        get_associated_token_address(&ctx.vault, &birds[3].new_mint),
    );
    let r = ctx.send(&[ix], &[&birds[2].holder]);
    ctx.expect_swap_err("wrong vault_new_ata", r, SwapError::WrongRemint);

    // not the owner
    let ix = swap_ix(
        &ctx, &birds[2], &mallory.pubkey(),
        ctx.mapping_pda(&birds[2].old_mint),
        birds[2].holder_original_ata,
        get_associated_token_address(&ctx.vault, &birds[2].new_mint),
    );
    let r = ctx.send(&[ix], &[&mallory]);
    ctx.expect_swap_err("swap an original you don't own", r, SwapError::NotOwner);

    // pause gate
    set_paused(&ctx, true).unwrap();
    let r = swap(&ctx, &birds[2]);
    ctx.expect_swap_err("swap while paused", r, SwapError::Paused);
    set_paused(&ctx, false).unwrap();
    let r = swap(&ctx, &birds[2]);
    ctx.expect_ok("swap after unpause", r);

    // feed the remint back in → no mapping derives from a remint
    let holder2_new_ata = get_associated_token_address(&birds[2].holder.pubkey(), &birds[2].new_mint);
    let ix = swap_ix(
        &ctx, &birds[2], &birds[2].holder.pubkey(),
        ctx.mapping_pda(&birds[2].new_mint),
        holder2_new_ata,
        get_associated_token_address(&ctx.vault, &birds[2].new_mint),
    );
    let r = ctx.send(&[ix], &[&birds[2].holder]);
    ctx.expect_err_code("swap a remint back in", r, 3012);

    // ---- recover: locked, then wait for the unlock ----
    let vault_ata3 = get_associated_token_address(&ctx.vault, &birds[3].new_mint);
    let r = recover(&ctx, &birds[3], vault_ata3);
    ctx.expect_swap_err("recover before unlock_ts", r, SwapError::Locked);

    let wait = unlock_ts - now_ts() + 45; // margin for cluster clock drift
    if wait > 0 {
        println!("driver: waiting {wait}s for unlock_ts…");
        std::thread::sleep(Duration::from_secs(wait as u64));
    }

    let r = recover(&ctx, &birds[3], vault_ata3);
    ctx.expect_ok("recover after unlock", r);
    let rec = ctx.token_balance(&get_associated_token_address(&ctx.custodian.pubkey(), &birds[3].new_mint));
    ctx.check("recovered remint with custodian", rec == 1, format!("balance {rec}"));
    let r = recover(&ctx, &birds[3], vault_ata3);
    ctx.expect_swap_err("recover same bird again (vault empty)", r, SwapError::NotHeld);

    // recover a claimed mapping → AlreadyClaimed
    let vault_ata1 = get_associated_token_address(&ctx.vault, &birds[1].new_mint);
    let r = recover(&ctx, &birds[1], vault_ata1);
    ctx.expect_swap_err("recover a claimed mapping", r, SwapError::AlreadyClaimed);

    // recover aimed at a deposited ORIGINAL → NotRecoverable
    let vault_original1 = get_associated_token_address(&ctx.vault, &birds[1].old_mint);
    let ix = Instruction::new_with_bytes(
        ctx.program_id,
        &thugz_swap::instruction::Recover { old_mint: birds[4].old_mint }.data(),
        thugz_swap::accounts::Recover {
            admin: ctx.admin.pubkey(),
            pool: ctx.pool,
            mapping: ctx.mapping_pda(&birds[4].old_mint),
            new_mint: birds[4].new_mint,
            vault: ctx.vault,
            vault_ata: vault_original1,
            custodian: ctx.custodian.pubkey(),
            custodian_ata: get_associated_token_address(&ctx.custodian.pubkey(), &birds[4].new_mint),
            treasury: ctx.treasury,
            token_program: TOKEN_PROGRAM_ID,
            associated_token_program: ATA_PROGRAM_ID,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    );
    let r = ctx.send(&[ix], &[&ctx.admin]);
    ctx.expect_swap_err("recover cannot touch a deposited original", r, SwapError::NotRecoverable);

    // the desk does NOT close at unlock — swapping still works
    let r = swap(&ctx, &birds[4]);
    ctx.expect_ok("swap still works after unlock (window is a floor, not a cliff)", r);

    println!("\n=== devnet matrix: {} passed, {} failed ===", ctx.pass, ctx.fail);
    std::process::exit(if ctx.fail == 0 { 0 } else { 1 });
}
