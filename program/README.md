# Thugz Swap — Anchor workspace

The on-chain program specified by `../SWAP_SPEC.md` + `../IMPLEMENTATION_APPENDIX.md`
(the appendix wins where they disagree). Program ID
`CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs`; deploy keypair lives OUTSIDE this repo
in `~/.thugbirdz-keys/swap/`.

## Toolchain (pinned — do not "upgrade to latest")

Anchor **1.1.2** · Solana CLI **3.1.10** · Rust **1.89.0** (rust-toolchain.toml) ·
`Cargo.lock` committed. Agent-run builds prefix `NO_DNA=1`.

## Build & test

```bash
# Level 1 test suite (LiteSVM) — build the test-keys artifact first:
NO_DNA=1 anchor build -- --features test-keys
NO_DNA=1 cargo test --features test-keys -- --test-threads=1

# Mainnet artifact (default features — the ONLY deployable build):
NO_DNA=1 anchor build
python3 scripts/verify_mainnet_artifact.py   # must print SAFE TO DEPLOY
```

The `test-keys` feature swaps the compiled `ADMIN`/`CUSTODIAN` constants for the
committed fixture keypairs in `programs/thugz-swap/tests/fixtures/` so the suite can
sign as them. Everything else — `EXPECTED = 1274` included — is identical to mainnet.
**Never deploy a test-keys build**: run the artifact guard before any deploy; Gate 6's
verifiedBuild independently ties the deployed bytecode to default-features source.

## Deploying (devnet today, mainnet at Phase 6)

**Never use `anchor deploy`** — it ignored a pass-through `--program-keypair` on devnet
and deployed to a generated program ID (2 SOL burned, recovered via `solana program
close`). Always:

```bash
python3 scripts/verify_mainnet_artifact.py          # mainnet only: SAFE TO DEPLOY required
solana program deploy target/deploy/thugz_swap.so \
  --program-id ~/.thugbirdz-keys/swap/program-CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs.json
# then CONFIRM the printed Program Id is CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs
```

If the printed ID is anything else, a generated keypair was used: stop and
`solana program close <wrong-id> --bypass-warning` to reclaim rent.

## Layout

| | |
|---|---|
| `programs/thugz-swap/src/constants.rs` | Seeds + compiled ground constants (exported to the IDL) |
| `programs/thugz-swap/src/state.rs` | `Pool` (93 B) and `Mapping` (83 B) |
| `programs/thugz-swap/src/instructions/` | One file per instruction |
| `programs/thugz-swap/tests/level1.rs` | TEST_PLAN Level 1 — 25 tests, mutation-checked |
| `SECURITY_CHECKLIST.md` | Applied rules, high-risk decisions, measured CU |
| `scripts/verify_mainnet_artifact.py` | Pre-deploy guard against a test-keys artifact |

## Gate status

- **Gate 1** (byte-identical build from a clean checkout): pending a second-machine
  run; reproducibility is finally proven by verifiedBuild at Gate 6.
- **Gate 2** (every invariant has a named test that fails when broken): suite green
  25/25; spot mutations verified on the sealed gate, the claimed check, and the
  fix_mapping destination. Full mutation matrix at formal sign-off.
