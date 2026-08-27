//! Stage 2 one-off operations: fix_mapping, recover, and the time-warp cheatcode.
//!
//!   THUGZ_RPC=... cargo run -p rehearsal-driver --bin stage2_ops -- fix-mapping <OLD_MINT>
//!   THUGZ_RPC=... cargo run -p rehearsal-driver --bin stage2_ops -- recover <OLD_MINT>
//!
//! Prints the transaction result verbatim (success or the exact program error) and the
//! relevant post-state; the orchestrator asserts. Admin signs; the remint destination is
//! always the compiled CUSTODIAN's ATA — that property is re-checked here after the fact.

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
use thugz_swap::{CUSTODIAN, MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

fn keypair_from_json(path: &str) -> Keypair {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing keypair {path}"));
    let bytes: Vec<u8> = raw.trim().trim_start_matches('[').trim_end_matches(']')
        .split(',').map(|t| t.trim().parse::<u8>().unwrap()).collect();
    Keypair::try_from(&bytes[..]).unwrap()
}
fn home(p: &str) -> String { format!("{}/{}", std::env::var("HOME").unwrap(), p) }

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (cmd, old_mint) = (args.get(1).map(String::as_str).unwrap_or(""),
                           args.get(2).map(|s| Pubkey::from_str(s).expect("bad old mint")));
    let rpc_url = std::env::var("THUGZ_RPC").unwrap_or_else(|_| "http://127.0.0.1:8999".into());
    let rpc = RpcClient::new_with_commitment(rpc_url, CommitmentConfig::confirmed());
    let admin = keypair_from_json(&home(".thugbirdz-keys/swap/admin-thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7.json"));
    let program_id = thugz_swap::ID;
    let pool = Pubkey::find_program_address(&[POOL_SEED], &program_id).0;
    let vault = Pubkey::find_program_address(&[VAULT_SEED], &program_id).0;
    let treasury = Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0;

    let old = old_mint.expect("usage: stage2_ops fix-mapping|recover <OLD_MINT>");
    let mapping = Pubkey::find_program_address(&[MAP_SEED, pool.as_ref(), old.as_ref()], &program_id).0;
    // new_mint comes from the on-chain mapping, never from us
    let macc = rpc.get_account(&mapping).expect("mapping account missing");
    let new_mint = Pubkey::try_from(&macc.data[8..40]).unwrap();
    let custodian_ata = get_associated_token_address(&CUSTODIAN, &new_mint);
    let vault_ata = get_associated_token_address(&vault, &new_mint);
    println!("op={cmd} old={old} new={new_mint} mapping={mapping}");

    let ix = match cmd {
        "fix-mapping" => Instruction::new_with_bytes(
            program_id,
            &thugz_swap::instruction::FixMapping { old_mint: old }.data(),
            thugz_swap::accounts::FixMapping {
                admin: admin.pubkey(), pool, mapping, new_mint, vault, vault_ata,
                custodian: CUSTODIAN, custodian_ata, treasury,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ATA_PROGRAM_ID,
                system_program: anchor_lang::solana_program::system_program::ID,
            }.to_account_metas(None)),
        "recover" => Instruction::new_with_bytes(
            program_id,
            &thugz_swap::instruction::Recover { old_mint: old }.data(),
            thugz_swap::accounts::Recover {
                admin: admin.pubkey(), pool, mapping, new_mint, vault, vault_ata,
                custodian: CUSTODIAN, custodian_ata, treasury,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ATA_PROGRAM_ID,
                system_program: anchor_lang::solana_program::system_program::ID,
            }.to_account_metas(None)),
        other => panic!("unknown op {other}"),
    };
    let bh = rpc.get_latest_blockhash().unwrap();
    let msg = Message::new_with_blockhash(&[ix], Some(&admin.pubkey()), &bh);
    let tx = Transaction::new(&[&admin], msg, bh);
    match rpc.send_and_confirm_transaction(&tx) {
        Ok(sig) => println!("RESULT: OK {sig}"),
        Err(e) => println!("RESULT: ERR {e}"),
    }
    // post-state
    let amt = |a: &Pubkey| rpc.get_account(a).ok()
        .filter(|x| x.data.len() >= 72)
        .map(|x| u64::from_le_bytes(x.data[64..72].try_into().unwrap())).unwrap_or(0);
    let pd = rpc.get_account(&pool).expect("pool").data;
    let dep = u16::from_le_bytes(pd[74..76].try_into().unwrap());
    let rec = u16::from_le_bytes(pd[78..80].try_into().unwrap());
    let map_now = rpc.get_account(&mapping).ok();
    println!("POST: deposited={dep} recovered={rec} custodian_ata={} vault_ata={} mapping_exists={} mapping_recovered_flag={}",
        amt(&custodian_ata), amt(&vault_ata), map_now.is_some(),
        map_now.map(|a| a.data.get(81).copied().unwrap_or(0)).unwrap_or(0));
}
