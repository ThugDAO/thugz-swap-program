# Trail of Bits Solana vuln-pattern scan — thugz-swap

Date: 2026-08-27 · Commit: post-beb1a33 · Scope: `program/programs/thugz-swap/src/`
Method: the tob-solana-vuln-scanner workflow (greps + manual analysis of every hit).

## Verdict: no findings at CRITICAL / HIGH / MEDIUM. Two informational notes.

### 1. Arbitrary CPI — PASS
9 CPI sites, all via Anchor helper wrappers. Program IDs come from compiled constants
(`System::id()`, `AssociatedToken::id()`) or from `token_program.key()` where
`token_program` is a validated `Interface<'info, TokenInterface>` (Anchor rejects any
account that is not the SPL Token / Token-2022 program). No user-controlled program ID
can reach any `invoke`. Program accounts in every context are typed
`Program`/`Interface` — a fake executable fails account validation.

### 2. Improper PDA validation — PASS
No `create_program_address` anywhere. Every PDA is an Anchor `seeds = [...]` constraint.
Canonical bumps are found once (`ctx.bumps.pool`, `find_program_address` for
vault/treasury at init; `bump` on Mapping creation) and stored on `Pool`/`Mapping`;
every later use is `bump = <stored>`. No instruction accepts a caller-supplied bump.

### 3. Missing ownership check — PASS
No raw `try_from_slice`/manual deserialization of untrusted data. Every account whose
data is read is typed (`Account`, `InterfaceAccount`). The 13 `UncheckedAccount` uses
divide into: the vault authority PDA (seeds-validated, data never read — authority
only), destination ATAs (address asserted against canonical derivation in-program AND
re-validated by the ATA program on create; our program never reads their data — the
token program validates them on transfer), the custodian wallet (address pinned to the
compiled constant, data never read), and the Mapping at creation (explicitly required
to be system-owned + zero-data before init — that check IS the init-once guard).

### 4. Missing signer check — PASS
Every authority is a `Signer` type: admin (+ `has_one = admin` on Pool) on all five
admin instructions, the custodian on `deposit_bird` (plus `key() == CUSTODIAN`), the
holder on `swap`, the compiled-ADMIN-pinned signer on `initialize_pool`. The devnet
matrix asserts the failures (ConstraintHasOne 2001 / NotCustodian) and mutation
testing confirmed the tests detect removed checks.

### 5. Sysvar spoofing — PASS (N/A)
Only `Clock::get()` (syscall path — not an account a caller can substitute). No
account-passed sysvars. Toolchain is Solana 3.x, far past the 1.8.1 hardening.

### 6. Instruction introspection — PASS (N/A)
No `load_instruction_at*` / introspection of any kind.

## Informational

- `deposit_bird`'s Mapping constraint uses `bump` (canonical search) on every call —
  a CU cost on the admin-only deposit path, not a security issue. Deliberate: the bump
  cannot be stored before the account exists.
- `require_canonical_ata` duplicates a check the ATA program also enforces on
  `create_idempotent` — deliberate defense-in-depth so a wrong account fails with a
  program-owned error before any CPI.
