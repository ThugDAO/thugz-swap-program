//! Phase 4 deposit run — all 1,274 pairs from the claim map, 4 per transaction,
//! admin + HxwZ co-signing, resumable at any point.
//!
//! Resume is CHAIN-DERIVED: before sending anything, every mapping PDA is fetched
//! and pairs whose mapping already exists are skipped. The state file records
//! progress and wall-clock only — it is never trusted for what is deposited.
//!
//!   THUGZ_RPC=http://127.0.0.1:8999 cargo run -p devnet-driver --bin deposit_run -- \
//!       [--init] [--limit N] [--kill-after N] [--plant-bad NAME=WRONG_OLD_MINT] [--status]
//!
//!   --init        initialize_pool (unlock +2y, real MCC collection) + fund treasury
//!   --limit N     only process the first N not-yet-deposited pairs (testing)
//!   --kill-after N  abort the process after N sent transactions (resume test)
//!   --plant-bad   deposit NAME's remint under WRONG_OLD_MINT instead of its true
//!                 original (the deliberate bad pair; NAME's true deposit is skipped)
//!   --status      report chain-derived progress and exit

use anchor_lang::prelude::Pubkey;
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::associated_token::ID as ATA_PROGRAM_ID;
use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
use solana_commitment_config::CommitmentConfig;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_rpc_client::rpc_client::RpcClient;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use thugz_swap::{MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

const MCC_COLLECTION: &str = "5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc";
const BATCH: usize = 4;
const SOL: u64 = 1_000_000_000;

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

fn arg_val(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

struct Pair {
    name: String,
    old_mint: Pubkey,
    new_mint: Pubkey,
}

fn load_claim_map(path: &str) -> Vec<Pair> {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing claim map {path}"));
    let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
    let claims = v["claims"].as_array().expect("claims array");
    claims.iter().map(|c| Pair {
        name: c["name"].as_str().unwrap().to_string(),
        old_mint: Pubkey::from_str(c["old_mint"].as_str().unwrap()).unwrap(),
        new_mint: Pubkey::from_str(c["new_mint"].as_str().unwrap()).unwrap(),
    }).collect()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rpc_url = std::env::var("THUGZ_RPC").unwrap_or_else(|_| "http://127.0.0.1:8999".into());
    let claim_path = arg_val(&args, "--claim-map")
        .unwrap_or_else(|| home("claude/thugbirdz/recovered/remint/claim_map_all.json"));
    let state_path = arg_val(&args, "--state")
        .unwrap_or_else(|| home("claude/thugbirdz/program/tools/deposit_state.json"));
    let limit: Option<usize> = arg_val(&args, "--limit").map(|s| s.parse().unwrap());
    let kill_after: Option<usize> = arg_val(&args, "--kill-after").map(|s| s.parse().unwrap());
    let plant_bad = arg_val(&args, "--plant-bad").map(|s| {
        let (name, mint) = s.split_once('=').expect("--plant-bad NAME=WRONG_OLD_MINT");
        (name.to_string(), Pubkey::from_str(mint).expect("bad wrong-mint pubkey"))
    });

    let rpc = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());
    let admin = keypair_from_json(&home(".thugbirdz-keys/swap/admin-thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7.json"));
    let custodian = keypair_from_json(&home(".thugbirdz-keys/hxwz.json"));
    let program_id = thugz_swap::ID;
    let pool = Pubkey::find_program_address(&[POOL_SEED], &program_id).0;
    let vault = Pubkey::find_program_address(&[VAULT_SEED], &program_id).0;
    let treasury = Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0;
    println!("deposit_run: rpc {rpc_url}");
    println!("deposit_run: program {program_id}\ndeposit_run: pool {pool}");
    println!("deposit_run: admin {} custodian {}", admin.pubkey(), custodian.pubkey());

    let send = |ixs: &[Instruction], signers: &[&Keypair]| -> Result<String, String> {
        let blockhash = rpc.get_latest_blockhash().map_err(|e| format!("blockhash: {e}"))?;
        let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &blockhash);
        let tx = Transaction::new(signers, msg, blockhash);
        rpc.send_and_confirm_transaction(&tx)
            .map(|s| s.to_string())
            .map_err(|e| format!("{e}"))
    };

    // ---- --init: initialize pool + fund treasury ----
    if args.iter().any(|a| a == "--init") {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let unlock_ts = now + 2 * 365 * 24 * 3600;
        let ix = Instruction::new_with_bytes(
            program_id,
            &thugz_swap::instruction::InitializePool {
                unlock_ts,
                collection: Pubkey::from_str(MCC_COLLECTION).unwrap(),
            }.data(),
            thugz_swap::accounts::InitializePool {
                admin: admin.pubkey(),
                pool,
                system_program: anchor_lang::solana_program::system_program::ID,
            }.to_account_metas(None),
        );
        match send(&[ix], &[&admin]) {
            Ok(sig) => println!("init: pool initialized, unlock_ts {unlock_ts} — {sig}"),
            Err(e) => { println!("init: FAILED — {e}"); std::process::exit(1); }
        }
        let fund = solana_system_interface::instruction::transfer(&admin.pubkey(), &treasury, 5 * SOL);
        match send(&[fund], &[&admin]) {
            Ok(_) => println!("init: treasury funded with 5 SOL"),
            Err(e) => { println!("init: treasury funding FAILED — {e}"); std::process::exit(1); }
        }
        return;
    }

    // ---- chain-derived progress: which mappings already exist ----
    let pairs = load_claim_map(&claim_path);
    println!("deposit_run: {} pairs in claim map", pairs.len());
    let mapping_pdas: Vec<Pubkey> = pairs.iter().map(|p| {
        Pubkey::find_program_address(&[MAP_SEED, pool.as_ref(), p.old_mint.as_ref()], &program_id).0
    }).collect();
    let mut deposited = vec![false; pairs.len()];
    for (chunk_i, chunk) in mapping_pdas.chunks(100).enumerate() {
        let res = rpc.get_multiple_accounts(chunk).expect("get_multiple_accounts");
        for (j, acc) in res.iter().enumerate() {
            deposited[chunk_i * 100 + j] = acc.is_some();
        }
    }
    let done_before = deposited.iter().filter(|d| **d).count();
    println!("deposit_run: {done_before} already on chain, {} to go", pairs.len() - done_before);

    if args.iter().any(|a| a == "--status") { return; }

    // ---- build work list ----
    let mut work: Vec<&Pair> = pairs.iter().zip(&deposited)
        .filter(|(p, d)| {
            if **d { return false; }
            // the planted-bad name's TRUE deposit is skipped; its bad twin is queued below
            if let Some((bad_name, _)) = &plant_bad { if &p.name == bad_name { return false; } }
            true
        })
        .map(|(p, _)| p).collect();
    if let Some(n) = limit { work.truncate(n); }

    let started = Instant::now();
    let mut sent_txs = 0usize;
    let mut confirmed = 0usize;
    let mut save_state = |sent: usize, conf: usize, done_total: usize, elapsed: Duration, finished: bool| {
        let s = serde_json::json!({
            "rpc": rpc_url,
            "chain_deposited_at_start": done_before,
            "txs_sent_this_run": sent,
            "pairs_confirmed_this_run": conf,
            "chain_deposited_now": done_total,
            "elapsed_secs_this_run": elapsed.as_secs_f64(),
            "finished": finished,
        });
        std::fs::write(&state_path, serde_json::to_string_pretty(&s).unwrap()).ok();
    };

    let deposit_ix = |p: &Pair, old_override: Option<Pubkey>| -> Instruction {
        let old = old_override.unwrap_or(p.old_mint);
        let mapping = Pubkey::find_program_address(&[MAP_SEED, pool.as_ref(), old.as_ref()], &program_id).0;
        Instruction::new_with_bytes(
            program_id,
            &thugz_swap::instruction::DepositBird { old_mint: old }.data(),
            thugz_swap::accounts::DepositBird {
                admin: admin.pubkey(),
                pool,
                custodian: custodian.pubkey(),
                new_mint: p.new_mint,
                source_ata: get_associated_token_address(&custodian.pubkey(), &p.new_mint),
                mapping,
                vault,
                vault_ata: get_associated_token_address(&vault, &p.new_mint),
                treasury,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ATA_PROGRAM_ID,
                system_program: anchor_lang::solana_program::system_program::ID,
            }.to_account_metas(None),
        )
    };

    // ---- the run: BATCH deposits per transaction ----
    for group in work.chunks(BATCH) {
        let ixs: Vec<Instruction> = group.iter().map(|p| deposit_ix(p, None)).collect();
        let names: Vec<&str> = group.iter().map(|p| p.name.as_str()).collect();
        let mut attempt = 0;
        loop {
            attempt += 1;
            match send(&ixs, &[&admin, &custodian]) {
                Ok(_) => {
                    confirmed += group.len();
                    break;
                }
                Err(e) if attempt < 3 && !e.contains("custom program error") => {
                    eprintln!("retry {attempt} for {names:?}: {e}");
                    std::thread::sleep(Duration::from_millis(800));
                }
                Err(e) => {
                    eprintln!("FATAL on {names:?}: {e}");
                    save_state(sent_txs, confirmed, done_before + confirmed, started.elapsed(), false);
                    std::process::exit(1);
                }
            }
        }
        sent_txs += 1;
        if sent_txs % 25 == 0 || confirmed == work.len() {
            println!("{} txs, {}/{} pairs this run, {:.1}s elapsed",
                sent_txs, confirmed, work.len(), started.elapsed().as_secs_f64());
            save_state(sent_txs, confirmed, done_before + confirmed, started.elapsed(), false);
        }
        if let Some(k) = kill_after {
            if sent_txs >= k {
                println!("KILL-AFTER {k}: aborting mid-run as requested");
                save_state(sent_txs, confirmed, done_before + confirmed, started.elapsed(), false);
                std::process::exit(137);
            }
        }
    }

    // ---- the deliberate bad pair, if requested ----
    if let Some((bad_name, wrong_old)) = &plant_bad {
        let p = pairs.iter().find(|p| &p.name == bad_name).expect("plant-bad name not in claim map");
        let ix = deposit_ix(p, Some(*wrong_old));
        match send(&[ix], &[&admin, &custodian]) {
            Ok(sig) => println!("PLANTED BAD PAIR: {bad_name} remint deposited under WRONG old_mint {wrong_old} — {sig}"),
            Err(e) => { println!("plant-bad FAILED — {e}"); std::process::exit(1); }
        }
        confirmed += 1;
    }

    let elapsed = started.elapsed();
    save_state(sent_txs, confirmed, done_before + confirmed, elapsed, true);
    println!("DONE: {sent_txs} txs, {confirmed} pairs this run, wall-clock {:.1}s ({:.2} min)",
        elapsed.as_secs_f64(), elapsed.as_secs_f64() / 60.0);
    println!("state written to {state_path}");
}
