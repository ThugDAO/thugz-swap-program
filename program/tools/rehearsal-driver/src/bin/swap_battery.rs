//! Phase 4 swap battery — Level 4's swap-side rows against the sealed fork.
//!
//!   THUGZ_RPC=http://127.0.0.1:8999 cargo run -p rehearsal-driver --bin swap_battery -- \
//!       [--pre-seal-check] [--seal] [--count 50]
//!
//!   --pre-seal-check  attempt one swap while unsealed; expect NotSealed
//!   --seal            admin seals the pool (deposited must equal expected)
//!   (default)         the battery: N wallets swap N distinct birds; a two-wallet race
//!                     on one bird; a zero-SOL wallet; a drained-treasury case with the
//!                     exact error recorded and treasury restored after; CU + tx bytes
//!                     measured for a single swap and a two-birds-batched transaction.
//!
//! Wallets get their originals via the surfnet_setTokenAccount cheatcode (the real 2021
//! holders' keys do not exist here). Writes swap_battery_report.json next to the state
//! file. Every assertion failure exits 1.

use anchor_lang::prelude::Pubkey;
use anchor_lang::{InstructionData, ToAccountMetas};
use anchor_spl::associated_token::get_associated_token_address;
use anchor_spl::associated_token::ID as ATA_PROGRAM_ID;
use anchor_spl::token::ID as TOKEN_PROGRAM_ID;
use serde_json::{json, Value};
use solana_commitment_config::CommitmentConfig;
use solana_instruction::Instruction;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_rpc_client::rpc_client::RpcClient;
use solana_rpc_client_api::request::RpcRequest;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::str::FromStr;
use thugz_swap::{MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

const SOL: u64 = 1_000_000_000;

fn keypair_from_json(path: &str) -> Keypair {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing keypair {path}"));
    let bytes: Vec<u8> = raw.trim().trim_start_matches('[').trim_end_matches(']')
        .split(',').map(|t| t.trim().parse::<u8>().unwrap()).collect();
    Keypair::try_from(&bytes[..]).unwrap()
}
fn home(p: &str) -> String { format!("{}/{}", std::env::var("HOME").unwrap(), p) }

struct Pair { name: String, old_mint: Pubkey, new_mint: Pubkey }

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let rpc_url = std::env::var("THUGZ_RPC").unwrap_or_else(|_| "http://127.0.0.1:8999".into());
    let count: usize = args.iter().position(|a| a == "--count")
        .and_then(|i| args.get(i + 1)).map(|s| s.parse().unwrap()).unwrap_or(50);
    let rpc = RpcClient::new_with_commitment(rpc_url.clone(), CommitmentConfig::confirmed());
    let admin = keypair_from_json(&home(".thugbirdz-keys/swap/admin-thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7.json"));
    let program_id = thugz_swap::ID;
    let pool = Pubkey::find_program_address(&[POOL_SEED], &program_id).0;
    let vault = Pubkey::find_program_address(&[VAULT_SEED], &program_id).0;
    let treasury = Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0;

    let raw = std::fs::read_to_string(home("claude/thugbirdz/recovered/remint/claim_map_all.json")).unwrap();
    let v: Value = serde_json::from_str(&raw).unwrap();
    let pairs: Vec<Pair> = v["claims"].as_array().unwrap().iter().map(|c| Pair {
        name: c["name"].as_str().unwrap().into(),
        old_mint: Pubkey::from_str(c["old_mint"].as_str().unwrap()).unwrap(),
        new_mint: Pubkey::from_str(c["new_mint"].as_str().unwrap()).unwrap(),
    }).collect();

    let cheat = |method: &'static str, params: Value| -> Value {
        rpc.send::<Value>(RpcRequest::Custom { method }, params)
            .unwrap_or_else(|e| panic!("cheatcode {method}: {e}"))
    };
    let give_bird = |owner: &Pubkey, mint: &Pubkey| {
        cheat("surfnet_setTokenAccount", json!([owner.to_string(), mint.to_string(), {"amount": 1}]));
    };
    let swap_ix = |holder: &Pubkey, p: &Pair| -> Instruction {
        let mapping = Pubkey::find_program_address(
            &[MAP_SEED, pool.as_ref(), p.old_mint.as_ref()], &program_id).0;
        Instruction::new_with_bytes(
            program_id,
            &thugz_swap::instruction::Swap {}.data(),
            thugz_swap::accounts::Swap {
                holder: *holder,
                pool,
                holder_original_ata: get_associated_token_address(holder, &p.old_mint),
                mapping,
                old_mint: p.old_mint,
                new_mint: p.new_mint,
                vault,
                vault_new_ata: get_associated_token_address(&vault, &p.new_mint),
                vault_original_ata: get_associated_token_address(&vault, &p.old_mint),
                holder_new_ata: get_associated_token_address(holder, &p.new_mint),
                treasury,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ATA_PROGRAM_ID,
                system_program: anchor_lang::solana_program::system_program::ID,
            }.to_account_metas(None),
        )
    };
    let send = |ixs: &[Instruction], signers: &[&Keypair]| -> Result<String, String> {
        let bh = rpc.get_latest_blockhash().map_err(|e| e.to_string())?;
        let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &bh);
        let tx = Transaction::new(signers, msg, bh);
        rpc.send_and_confirm_transaction(&tx).map(|s| s.to_string()).map_err(|e| e.to_string())
    };
    let token_amount = |ata: &Pubkey| -> u64 {
        match rpc.get_account(ata) {
            Ok(a) if a.data.len() >= 72 => u64::from_le_bytes(a.data[64..72].try_into().unwrap()),
            _ => 0,
        }
    };
    let pool_u16 = |off: usize| -> u16 {
        let d = rpc.get_account(&pool).expect("pool").data;
        u16::from_le_bytes(d[off..off + 2].try_into().unwrap())
    };
    let mut passes = 0u32; let mut fails = 0u32;
    let mut check = |label: &str, ok: bool, detail: String| {
        if ok { passes += 1; println!("PASS  {label}"); }
        else  { fails += 1; println!("FAIL  {label} — {detail}"); }
    };

    // ---- --pre-seal-check ----
    if args.iter().any(|a| a == "--pre-seal-check") {
        let w = Keypair::new();
        rpc.request_airdrop(&w.pubkey(), SOL / 2).and_then(|s| { rpc.poll_for_signature(&s) }).ok();
        give_bird(&w.pubkey(), &pairs[0].old_mint);
        let r = send(&[swap_ix(&w.pubkey(), &pairs[0])], &[&w]);
        let is_not_sealed = matches!(&r, Err(e) if e.contains("0x177") || e.contains("NotSealed") || e.contains("custom program error"));
        check("swap BEFORE seal fails", is_not_sealed, format!("{r:?}"));
        println!("done: {passes} pass {fails} fail");
        std::process::exit(if fails > 0 { 1 } else { 0 });
    }

    // ---- --seal ----
    if args.iter().any(|a| a == "--seal") {
        let ix = Instruction::new_with_bytes(
            program_id, &thugz_swap::instruction::Seal {}.data(),
            thugz_swap::accounts::Seal { admin: admin.pubkey(), pool }.to_account_metas(None));
        match send(&[ix], &[&admin]) {
            Ok(sig) => println!("SEALED — {sig}"),
            Err(e) => { println!("seal FAILED: {e}"); std::process::exit(1); }
        }
        return;
    }

    // ---- the battery ----
    let swapped_before = pool_u16(76);
    println!("battery: pool.swapped = {swapped_before} at start");
    let mut used = 0usize;
    let mut next_pair = || { let p = &pairs[used]; used += 1; p };

    // 1. N wallets, N birds
    let mut cu_single: Option<u64> = None;
    let mut bytes_single: Option<usize> = None;
    for i in 0..count {
        let p = next_pair();
        let w = Keypair::new();
        let s = rpc.request_airdrop(&w.pubkey(), SOL / 10).expect("airdrop");
        rpc.poll_for_signature(&s).expect("airdrop confirm");
        give_bird(&w.pubkey(), &p.old_mint);
        let ix = swap_ix(&w.pubkey(), p);
        let bh = rpc.get_latest_blockhash().unwrap();
        let msg = Message::new_with_blockhash(&[ix], Some(&w.pubkey()), &bh);
        let tx = Transaction::new(&[&w], msg, bh);
        let txbytes = bincode::serialize(&tx).map(|b| b.len()).unwrap_or(0);
        match rpc.send_and_confirm_transaction(&tx) {
            Ok(sig) => {
                let got_new = token_amount(&get_associated_token_address(&w.pubkey(), &p.new_mint));
                let vault_old = token_amount(&get_associated_token_address(&vault, &p.old_mint));
                if got_new != 1 || vault_old != 1 {
                    check(&format!("swap {} state", p.name), false,
                          format!("holder_new={got_new} vault_old={vault_old}"));
                } else if i == 0 {
                    // measure CU on the first one
                    let cfg = json!([sig.to_string(), {"encoding":"json","commitment":"confirmed","maxSupportedTransactionVersion":1}]);
                    if let Ok(t) = rpc.send::<Value>(RpcRequest::GetTransaction, cfg) {
                        cu_single = t["meta"]["computeUnitsConsumed"].as_u64();
                    }
                    bytes_single = Some(txbytes);
                }
            }
            Err(e) => check(&format!("swap {}", p.name), false, e.to_string()),
        }
        if (i + 1) % 10 == 0 { println!("battery: {}/{count} swaps done", i + 1); }
    }
    let swapped_now = pool_u16(76);
    check("50-wallet batch all counted", swapped_now == swapped_before + count as u16,
          format!("pool.swapped {swapped_now}, expected {}", swapped_before + count as u16));
    println!("single swap: {:?} CU, {:?} tx bytes", cu_single, bytes_single);

    // 2. race: two wallets, one bird, raw sends with preflight skipped
    let p = next_pair();
    let (w1, w2) = (Keypair::new(), Keypair::new());
    for w in [&w1, &w2] {
        let s = rpc.request_airdrop(&w.pubkey(), SOL / 10).unwrap();
        rpc.poll_for_signature(&s).unwrap();
        give_bird(&w.pubkey(), &p.old_mint);
    }
    let bh = rpc.get_latest_blockhash().unwrap();
    let mk = |w: &Keypair| {
        let msg = Message::new_with_blockhash(&[swap_ix(&w.pubkey(), p)], Some(&w.pubkey()), &bh);
        Transaction::new(&[w], msg, bh)
    };
    use solana_rpc_client_api::config::RpcSendTransactionConfig;
    let cfg = RpcSendTransactionConfig { skip_preflight: true, ..Default::default() };
    let s1 = rpc.send_transaction_with_config(&mk(&w1), cfg);
    let s2 = rpc.send_transaction_with_config(&mk(&w2), cfg);
    std::thread::sleep(std::time::Duration::from_secs(3));
    let ok = |s: &Result<solana_signature::Signature, _>| -> bool {
        match s { Ok(sig) => matches!(rpc.get_signature_status(sig), Ok(Some(Ok(())))), Err(_) => false }
    };
    let (r1, r2) = (ok(&s1), ok(&s2));
    check("race: exactly one winner", r1 ^ r2, format!("w1={r1} w2={r2}"));
    let loser_status = if r1 { format!("{:?}", s2.map(|s| rpc.get_signature_status(&s))) }
                       else  { format!("{:?}", s1.map(|s| rpc.get_signature_status(&s))) };
    println!("race loser status (frontend maps this): {}", &loser_status[..loser_status.len().min(300)]);

    // 3. zero-SOL wallet
    let p = next_pair();
    let w = Keypair::new();               // never funded
    give_bird(&w.pubkey(), &p.old_mint);
    let r = send(&[swap_ix(&w.pubkey(), p)], &[&w]);
    check("zero-SOL wallet fails at fee layer", r.is_err(), format!("{r:?}"));
    println!("zero-SOL error (frontend maps this): {}", r.err().unwrap_or_default());

    // 4. drained treasury: legible failure, then recovery after refill
    let treasury_before = rpc.get_balance(&treasury).unwrap();
    cheat("surfnet_setAccount", json!([treasury.to_string(), {"lamports": 1000u64}]));
    let p = next_pair();
    let w = Keypair::new();
    let s = rpc.request_airdrop(&w.pubkey(), SOL / 10).unwrap();
    rpc.poll_for_signature(&s).unwrap();
    give_bird(&w.pubkey(), &p.old_mint);
    let r = send(&[swap_ix(&w.pubkey(), p)], &[&w]);
    check("drained treasury: swap fails", r.is_err(), format!("{r:?}"));
    println!("drained-treasury error (frontend maps this): {}", r.clone().err().unwrap_or_default());
    cheat("surfnet_setAccount", json!([treasury.to_string(), {"lamports": treasury_before}]));
    let r = send(&[swap_ix(&w.pubkey(), p)], &[&w]);
    check("treasury refilled: same swap now succeeds", r.is_ok(), format!("{r:?}"));

    // 5. two swaps batched in one tx (same holder, two birds) — CU + bytes
    let (pa, pb) = { let a = next_pair() as *const Pair; let b = next_pair() as *const Pair;
                     unsafe { (&*a, &*b) } };
    let w = Keypair::new();
    let s = rpc.request_airdrop(&w.pubkey(), SOL / 10).unwrap();
    rpc.poll_for_signature(&s).unwrap();
    give_bird(&w.pubkey(), &pa.old_mint);
    give_bird(&w.pubkey(), &pb.old_mint);
    let bh = rpc.get_latest_blockhash().unwrap();
    let msg = Message::new_with_blockhash(
        &[swap_ix(&w.pubkey(), pa), swap_ix(&w.pubkey(), pb)], Some(&w.pubkey()), &bh);
    let tx = Transaction::new(&[&w], msg, bh);
    let batched_bytes = bincode::serialize(&tx).map(|b| b.len()).unwrap_or(0);
    let mut cu_batched: Option<u64> = None;
    match rpc.send_and_confirm_transaction(&tx) {
        Ok(sig) => {
            let cfg = json!([sig.to_string(), {"encoding":"json","commitment":"confirmed","maxSupportedTransactionVersion":1}]);
            if let Ok(t) = rpc.send::<Value>(RpcRequest::GetTransaction, cfg) {
                cu_batched = t["meta"]["computeUnitsConsumed"].as_u64();
            }
            check("two batched swaps in one tx", true, String::new());
        }
        Err(e) => check("two batched swaps in one tx", false, e.to_string()),
    }
    println!("batched: {:?} CU, {batched_bytes} tx bytes (limit 1232; 200k CU default)", cu_batched);

    let report = json!({
        "swaps": count, "pool_swapped_before": swapped_before, "pool_swapped_after": pool_u16(76),
        "cu_single": cu_single, "tx_bytes_single": bytes_single,
        "cu_two_batched": cu_batched, "tx_bytes_two_batched": batched_bytes,
        "passes": passes, "fails": fails,
    });
    std::fs::write(home("claude/thugbirdz/program/tools/swap_battery_report.json"),
                   serde_json::to_string_pretty(&report).unwrap()).ok();
    println!("battery done: {passes} pass, {fails} fail — report written");
    std::process::exit(if fails > 0 { 1 } else { 0 });
}
