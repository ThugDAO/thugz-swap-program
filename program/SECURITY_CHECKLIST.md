# Thugz Swap — security checklist

**Risk level: 🔴 Critical** — custody of 1,274 NFTs belonging to other people, an admin
key, and an irreversible `seal`. Built to `SWAP_SPEC.md` + `IMPLEMENTATION_APPENDIX.md`
(appendix wins) under the safe-solana-builder rules; every item below is implemented and
covered by `tests/level1.rs` unless marked otherwise.

## Account & identity validation

- [x] Every authority is a `Signer`; admin gates use `has_one = admin` against Pool
- [x] `Account<T>` / `InterfaceAccount<T>` everywhere typed data exists; every
      `UncheckedAccount` carries a `/// CHECK:` explaining why (vault authority PDA,
      to-be-created ATAs, manually-initialized Mapping, custodian wallet)
- [x] Cross-account links via constraints: `mapping.new_mint == new_mint`, token account
      `owner`/`mint` checks, vault ownership on every vault ATA
- [x] Reinit blocked: Pool via Anchor `init` (singleton); Mapping via the manual
      system-owned + zero-data check (`MappingExists`)
- [x] Mints are read FROM token accounts, never trusted from instruction data —
      `swap` derives the Mapping from `holder_original_ata.mint`; `deposit_bird` reads
      `source_ata.mint`

## PDAs

- [x] Canonical bumps found once (`initialize_pool` / first derivation), stored on
      Pool/Mapping, reused via `bump = …` constraints; callers can never supply a bump
- [x] Distinct seed prefixes (`pool`, `vault`, `map`, `treasury`); Mapping seeds include
      the pool key so a second deployment cannot collide
- [x] Vault is authority-only (no data); treasury is system-owned zero-data and is
      NEVER `init`/`allocate`/`assign`-ed — `SystemAccount` re-asserts that on every use

## Compiled ground constants (the custody model)

- [x] `CUSTODIAN` (HxwZ): only legal deposit source owner + signer; only legal
      `fix_mapping`/`recover` destination. **No instruction can deliver a token to the
      admin** — mutation-tested
- [x] `ADMIN`: pinned at `initialize_pool` — closes the singleton-seeds front-run /
      namespace-capture race (shared-base §29.1)
- [x] `EXPECTED = 1274`: never from instruction data
- [x] All three exported to the IDL via `#[constant]`

## Arithmetic & state

- [x] All counter math is `checked_*` with a typed `Arithmetic` error;
      `overflow-checks = true` in release
- [x] `seal` is an exact-equality allowlist (`== expected`, tested at `EXPECTED±1`),
      one-way, and absorbing (second seal → `Sealed`)
- [x] `recover` leaves `claimed == false` (tested) and increments `recovered` so the
      vault reconciliation (`expected − swapped − recovered`) stays true post-unlock
- [x] `unlock_ts` sentinel guard (`InvalidUnlockTimestamp` for past values)

## CPI

- [x] Program IDs: typed `Program`/`Interface` accounts; CpiContext built with
      `Token/System/AssociatedToken::id()` — no caller-supplied program reaches a CPI
- [x] `transfer_checked` everywhere (Token-2022-safe); mint + decimals always supplied
- [x] PDA signing minimal: vault seeds sign token transfers out; treasury seeds sign
      rent payments; mapping seeds sign only its own allocate/assign
- [x] ATA creation is explicit `create_idempotent` CPI via `new_with_signer` (Anchor
      `init` constraints cannot sign for a PDA payer); canonical ATA derivation is
      asserted in-program before every CPI *and* re-validated by the ATA program
- [x] No state is read after a CPI without knowing what the CPI touched; the two
      token transfers are the last CPIs and state writes use pre-read keys only

## Griefing & lifecycle

- [x] Pre-funded Mapping address absorbed (manual init transfers the deficit only) —
      tested with a real pre-funded PDA
- [x] Mapping close (`fix_mapping`) uses Anchor `close = treasury`: zeroed, drained,
      reassigned — closure asserted in tests (lamports 0, data 0, system-owned)
- [x] Dry treasury fails legibly instead of half-applying — tested
- [x] Duplicate mutable accounts: Anchor 1.x rejects by default; nothing here is
      annotated `dup`, and account pairs that must differ cannot alias by construction
      (different owners/mints)

## Stack / BPF

- [x] Every typed account Boxed in the large contexts (`swap` carries 14 accounts);
      build produces **no stack-offset warnings**

## Test suite (Level 1, LiteSVM)

- [x] 25 tests: full happy paths with state assertions, the §7 failure matrix with
      SPECIFIC error codes (never "it reverted"), adversarial cases, CU profiling
- [x] Mutation-tested: removing the sealed gate, the claimed check, or the
      `fix_mapping` destination check each makes its guarding test FAIL
- [x] Measured CU (Level 1, unoptimized-adjacent): swap first-time **86,661**
      (spec estimate 75–90k ✓), deposit 54k, fix 33k, recover 33k — two batched swaps
      ≈173k confirms "marginal vs the 200k default"; Level 3 (Surfpool) measures the
      binding numbers

## High-risk decisions (flagged, accepted)

1. **Upgrade authority stays live** on `birdAyQ…` indefinitely (operator decision,
   revisit trigger at 20 swaps + 30 quiet days). Every invariant is conditional on it.
2. **`test-keys` feature** produces a second build with fixture keys. Mitigations:
   mainnet = default features under verifiedBuild; `scripts/verify_mainnet_artifact.py`
   run before any deploy (checks bytecode + IDL); test fixture keys are throwaway and
   committed openly.
3. **Admin key is not rotatable** (no two-step rotation): deliberate — the admin's
   powers pre-seal are bounded by the custodian destination rule, and post-seal reduce
   to `set_paused`/`recover`-to-custodian. A compromised admin cannot extract value.
4. **No timelock on `seal`**: the gate is the published sweep + plan Phase 10, external
   to the program by design.

## Known limitations

- `deposit_bird` accepts any mint the custodian owns — a wrong PAIRING (right bird,
  wrong `old_mint`) is invisible on-chain by design; the pre-seal sweep +
  `fix_mapping` are the correction path (rehearsed in `adversarial_full_correction_cycle`).
- Level 1 numbers come from LiteSVM; byte sizes and binding CU ceilings are re-measured
  on Surfpool (Level 3) per the test plan.
