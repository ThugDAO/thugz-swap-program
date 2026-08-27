//! Seed a fresh devnet deployment for frontend testing (Phase 3b).
//!
//!   THUGZ_PROGRAM_ID=<id> cargo run -p devnet-driver --bin seed -- \
//!       --holder <PUBKEY> [--holder-birds 5]
//!
//! Initializes the pool (unlock +7 days), mints EXPECTED (20) mock pairs —
//! the first N originals go to --holder so a real wallet can drive the UI,
//! the rest to throwaway holders — deposits all 20, seals, and writes
//! website_build/swap/devnet_claims.json for the page.

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
use std::time::{SystemTime, UNIX_EPOCH};
use thugz_swap::{EXPECTED, MAP_SEED, POOL_SEED, TREASURY_SEED, VAULT_SEED};

const RPC_URL: &str = "https://api.devnet.solana.com";
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

fn send(rpc: &RpcClient, ixs: &[Instruction], signers: &[&Keypair]) {
    let blockhash = rpc.get_latest_blockhash().expect("blockhash");
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &blockhash);
    let tx = Transaction::new(signers, msg, blockhash);
    rpc.send_and_confirm_transaction(&tx).expect("tx failed");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let holder: Pubkey = args.iter().position(|a| a == "--holder")
        .and_then(|i| args.get(i + 1))
        .map(|s| Pubkey::from_str(s).expect("bad --holder pubkey"))
        .expect("--holder <PUBKEY> required");
    let holder_birds: usize = args.iter().position(|a| a == "--holder-birds")
        .and_then(|i| args.get(i + 1)).map(|s| s.parse().unwrap()).unwrap_or(5);

    let program_id: Pubkey = std::env::var("THUGZ_PROGRAM_ID")
        .expect("THUGZ_PROGRAM_ID required (a fresh throwaway deployment)")
        .parse().unwrap();
    let rpc = RpcClient::new_with_commitment(RPC_URL.to_string(), CommitmentConfig::confirmed());
    let payer = keypair_from_json(&home(".thugbirdz-keys/devnet/dev.json"));
    let admin = keypair_from_json(concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/thugz-swap/tests/fixtures/test_admin.json"));
    let custodian = keypair_from_json(concat!(env!("CARGO_MANIFEST_DIR"), "/../../programs/thugz-swap/tests/fixtures/test_custodian.json"));

    let pool = Pubkey::find_program_address(&[POOL_SEED], &program_id).0;
    let vault = Pubkey::find_program_address(&[VAULT_SEED], &program_id).0;
    let treasury = Pubkey::find_program_address(&[TREASURY_SEED], &program_id).0;
    println!("seed: program {program_id}\nseed: pool {pool}\nseed: holder {holder} gets {holder_birds} birds");

    if rpc.get_account(&pool).is_ok() {
        eprintln!("pool already exists at this program id — use a fresh throwaway deployment");
        std::process::exit(2);
    }

    // fund
    for (to, amt) in [(admin.pubkey(), SOL / 4), (custodian.pubkey(), SOL / 4), (treasury, SOL * 3 / 5), (holder, SOL / 50)] {
        let ix = solana_system_interface::instruction::transfer(&payer.pubkey(), &to, amt);
        send(&rpc, &[ix], &[&payer]);
    }
    println!("seed: funded admin/custodian/treasury (+fee dust to holder)");

    // init: unlock 7 days out so recover never interferes with UI testing
    let unlock_ts = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64 + 7 * 24 * 3600;
    let ix = Instruction::new_with_bytes(
        program_id,
        &thugz_swap::instruction::InitializePool { unlock_ts, collection: Pubkey::new_unique() }.data(),
        thugz_swap::accounts::InitializePool {
            admin: admin.pubkey(),
            pool,
            system_program: anchor_lang::solana_program::system_program::ID,
        }.to_account_metas(None),
    );
    send(&rpc, &[ix], &[&admin]);
    println!("seed: pool initialized, unlock {unlock_ts}");

    let rent = rpc.get_minimum_balance_for_rent_exemption(82).unwrap();
    let mut claims = vec![];
    for i in 0..EXPECTED as usize {
        let old = Keypair::new();
        let new = Keypair::new();
        let this_holder = if i < holder_birds { holder } else { Keypair::new().pubkey() };
        let h_ata = get_associated_token_address(&this_holder, &old.pubkey());
        let c_ata = get_associated_token_address(&custodian.pubkey(), &new.pubkey());
        let mut ixs = vec![];
        for m in [&old, &new] {
            ixs.push(solana_system_interface::instruction::create_account(
                &payer.pubkey(), &m.pubkey(), rent, 82, &TOKEN_PROGRAM_ID));
            ixs.push(spl_token_interface::instruction::initialize_mint2(
                &TOKEN_PROGRAM_ID, &m.pubkey(), &payer.pubkey(), None, 0).unwrap());
        }
        ixs.push(spl_associated_token_account_interface::instruction::create_associated_token_account(
            &payer.pubkey(), &this_holder, &old.pubkey(), &TOKEN_PROGRAM_ID));
        ixs.push(spl_associated_token_account_interface::instruction::create_associated_token_account(
            &payer.pubkey(), &custodian.pubkey(), &new.pubkey(), &TOKEN_PROGRAM_ID));
        ixs.push(spl_token_interface::instruction::mint_to(
            &TOKEN_PROGRAM_ID, &old.pubkey(), &h_ata, &payer.pubkey(), &[], 1).unwrap());
        ixs.push(spl_token_interface::instruction::mint_to(
            &TOKEN_PROGRAM_ID, &new.pubkey(), &c_ata, &payer.pubkey(), &[], 1).unwrap());
        send(&rpc, &ixs, &[&payer, &old, &new]);

        // deposit
        let mapping = Pubkey::find_program_address(
            &[MAP_SEED, pool.as_ref(), old.pubkey().as_ref()], &program_id).0;
        let ix = Instruction::new_with_bytes(
            program_id,
            &thugz_swap::instruction::DepositBird { old_mint: old.pubkey() }.data(),
            thugz_swap::accounts::DepositBird {
                admin: admin.pubkey(),
                pool,
                custodian: custodian.pubkey(),
                new_mint: new.pubkey(),
                source_ata: c_ata,
                mapping,
                vault,
                vault_ata: get_associated_token_address(&vault, &new.pubkey()),
                treasury,
                token_program: TOKEN_PROGRAM_ID,
                associated_token_program: ATA_PROGRAM_ID,
                system_program: anchor_lang::solana_program::system_program::ID,
            }.to_account_metas(None),
        );
        send(&rpc, &[ix], &[&admin, &custodian]);
        let mine = i < holder_birds;
        claims.push(format!(
            "{{\"name\": \"THUG #{:04}\", \"old_mint\": \"{}\", \"new_mint\": \"{}\", \"held_by_test_wallet\": {}}}",
            i + 1, old.pubkey(), new.pubkey(), mine));
        println!("seed: bird {}/{} deposited{}", i + 1, EXPECTED, if mine { " (test wallet)" } else { "" });
    }

    // seal
    let ix = Instruction::new_with_bytes(
        program_id,
        &thugz_swap::instruction::Seal {}.data(),
        thugz_swap::accounts::Seal { admin: admin.pubkey(), pool }.to_account_metas(None),
    );
    send(&rpc, &[ix], &[&admin]);
    println!("seed: SEALED — desk is live");

    let out = format!(
        "{{\n \"program_id\": \"{}\",\n \"rpc\": \"{}\",\n \"claims\": [\n  {}\n ]\n}}\n",
        program_id, RPC_URL, claims.join(",\n  "));
    let path = home("claude/thugbirdz/website_build/swap/devnet_claims.json");
    std::fs::create_dir_all(home("claude/thugbirdz/website_build/swap")).unwrap();
    std::fs::write(&path, out).unwrap();
    println!("seed: wrote {path}");
}
