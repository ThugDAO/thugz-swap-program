# Thugz Swap — audit pass 2026-08-27

Three independent passes over the BUILT program (live on devnet, real swaps processed).
**No P1 in any pass.** Five code hardenings applied, three doc-accuracy fixes, one finding
rejected with reason. Full suites green after (Level 1: 26, Level 2: 1000 random sequences).

## 1. tob-solana-vuln-scanner (6 critical Solana classes)
Clean. Report: `tob_vuln_scan_2026-08-27.md`. Arbitrary CPI, PDA validation, ownership,
signer, sysvar, introspection — all PASS.

## 2. tob-spec-to-code (multi-agent, 15 requirements checked, 45 agents)
Report: `spec-compliance/REPORT.md`. 13 requirements hold, **0 confirmed divergences**
(2 "partial" flags refuted). Three pieces of undocumented behavior — all DOCUMENTATION
overclaims, no code bug — now fixed:
- "No instruction can deliver a token to admin" was overbroad: `swap` pays the current
  holder, admin included if they hold an original. Narrowed to "no admin-GATED instruction"
  in lib.rs / constants.rs / SECURITY_CHECKLIST.md.
- Checklist claimed token CPIs use a compile-time program id; they use the caller's
  `token_program` constrained by `Interface<TokenInterface>`. Corrected + noted the deposit
  path is Token-2022-capable (the sweep is the extension guard).
- `swap` returns `NotRecoverable` for a mis-derived `vault_original_ata` — an error the spec
  associated only with `recover`. (Cosmetic; noted.)

## 3. Codex (gpt high, program source vs spec) — 5 × P2, dispositions:
| # | Finding | Disposition |
|---|---|---|
| 1 | `Signer` accepts CPI-signed PDAs → a custody program could route a remint to its PDA | **Rejected.** The fix (reject off-curve holders) would break legitimate Squads/multisig holders who must be able to swap. Not a theft path — the escrow position's owner gets their own remint. |
| 2 | `transfer_checked` passes `mint.decimals`, not literal 0 | **Fixed.** Added `decimals == 0` constraints on the swap/deposit mints (on-chain NFT-semantics guard, not only the off-chain sweep). |
| 3 | `unlock_ts` only required to be future, not opening+2yr | **Fixed.** 1-year floor at init on mainnet (60 s under test-keys). recover only ever pays the custodian, so this just protects the stated 2-year term. |
| 4 | `recover` has no `sealed` gate → recover pre-seal after a mis-set unlock, then seal a holed desk | **Fixed.** `require!(pool.sealed)` added; new test `fail_recover_before_seal`. |
| 5 | recover not idempotent — returned remint re-recoverable, inflates `Pool.recovered` | **Fixed.** `Mapping.recovered` flag (82→83 B); `claimed` still stays false per spec §4b; new idempotency assertions in `happy_recover…`. |

## Layout / doc deltas synced
Mapping 82→83 bytes (recovered bool; `claimed` stays at offset 40, indexers unaffected).
SWAP_SPEC §4b recover constraints, appendix §2 Mapping struct, SECURITY_CHECKLIST updated.
Mainnet artifact guard: SAFE TO DEPLOY. Level 1 26/26, Level 2 (1000 cases) green.
