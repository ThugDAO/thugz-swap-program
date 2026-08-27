# Spec-compliance report — thugz-swap

**Scope:** code under `program/` checked against IMPLEMENTATION_APPENDIX.md, SWAP_SPEC.md,
program/SECURITY_CHECKLIST.md, TEST_PLAN.md, IMPLEMENTATION_PLAN.md, program/README.md,
and the program source (constants.rs, lib.rs, state.rs, instructions/).
**Date:** 2026-08-27. Per-requirement analysis: `program/audits/spec-compliance/requirements/`.

## Verdict

On every requirement this pass actually checked — 15 of them, covering the toolchain pins,
the `Pool`/`Mapping` account layouts, the treasury-payer design, the PDA seed constants, and
the compile-time custodian — **the code does what the documents say. No divergence survived
refutation.** Two candidate divergences (REQ-06, REQ-07) were raised and knocked down; they
are listed under "Considered and dropped" so a reader can second-guess the refutations.

The important caveat is coverage, not compliance: this pass checked only the
IMPLEMENTATION_APPENDIX §1–§3 slice (roughly REQ-01 through REQ-16). **Everything else —
REQ-17 onward across all documents, several hundred requirement IDs including the entire
SWAP_SPEC instruction semantics, SECURITY_CHECKLIST items, and TEST_PLAN obligations — was
not checked at all** and its status is unknown, not compliant. The full unchecked list is at
the end of this report.

Separately, three behaviors exist in the code that no document describes, and two of them
contradict absolute claims the documentation makes about itself (the "admin can never receive
a token" invariant and the "no caller-supplied program reaches a CPI" checklist line). None
is a code defect on the evidence gathered; all three are documentation fixes. See
"Undocumented behavior" and "Problems in the documentation."

## Divergences

None confirmed. (Two candidates were refuted — see "Considered and dropped.")

## Alignment matrix

Every requirement checked in this pass. Quotes are from IMPLEMENTATION_APPENDIX.md unless
noted.

| Req | Requirement (verbatim, abridged) | Verdict | Key evidence |
|---|---|---|---|
| REQ-01 | "Anchor \| **1.1.2**" | implemented | `program/Anchor.toml` L3-4 `anchor_version = "1.1.2"`; `program/programs/thugz-swap/Cargo.toml` L26-32 pins anchor-lang/anchor-spl 1.1.2 for build and test; `program/Cargo.lock` L276-278, L360-362 resolves both to exactly 1.1.2 |
| REQ-02 | "Solana CLI \| **3.1.10** … *not* 4.1.x" | implemented | `program/README.md` L9-11 documents the 3.1.10 pin; IMPLEMENTATION_PLAN.md L102-103 repeats it; `program/Cargo.lock` keeps every solana-* crate on 3.x (e.g. L4356-4357), zero 4.x entries. Note: Anchor.toml carries no `solana_version` key, so the CLI pin itself is prose-only — nothing machine-enforces it |
| REQ-03 | "`solana-*` crates \| **^3**" | implemented | `program/programs/thugz-swap/Cargo.toml` L35-41 (all seven dev-deps on ^3); `program/tools/devnet-driver/Cargo.toml` L12-20 (all nine on ^3); workspace root `program/Cargo.toml` L1-19 declares no dependencies where a stray version could hide |
| REQ-04 | "Rust \| MSRV **1.89**" | implemented | `program/Cargo.toml` L8-10 `rust-version = "1.89.0"`; program crate inherits it (`thugz-swap/Cargo.toml` L6); `program/rust-toolchain.toml` L1-2 pins builds to exactly 1.89.0 |
| REQ-05 | "**Commit `Cargo.lock`.**" | implemented | `program/Cargo.lock` is tracked (6,066 lines, last committed 53cf354); `program/.gitignore` L1-4 excludes nothing that would drop it |
| REQ-06 | "Agent-run builds: prefix `NO_DNA=1`" | implemented (divergence refuted) | CLAUDE.md L23, `program/README.md` L11, L17-18, L21, and both test headers (`tests/level1.rs` L4, `tests/level2_properties.rs` L19) carry the prefix on every documented build/test command; the requirement prescribes an invocation convention, not a committed-script change — see "Considered and dropped." Open question: no consumer of NO_DNA exists in the repo, so what the variable gates could not be established |
| REQ-07 | "Anchor 1.1.x supports `verifiedBuild` via OtterSec … Use the standard mechanism instead." | implemented (divergence refuted) | Gate 6 was rewritten to verifiedBuild everywhere (IMPLEMENTATION_PLAN.md L107-108, L235-239); the forbidden hand-rolled hash comparison is absent repo-wide (search record in `requirements/07-REQ-07.md` L92-94); `program/scripts/verify_mainnet_artifact.py` is a constants guard that defers to Gate 6. Execution of the OtterSec verify is a Phase-6 deploy-time act that cannot occur pre-deploy — to be evidenced when mainnet deploy happens |
| REQ-08 | `Pool` struct: 12 fields, "8 discriminator + 85 = 93" | implemented | `src/state.rs` L8-23 — same fields, names, types, order; Borsh sum 85, +8 = 93; sole creation site `instructions/initialize_pool.rs` L19-26 allocates `8 + Pool::INIT_SPACE` so size cannot drift |
| REQ-09 | `Mapping` struct: 5 fields, "8 discriminator + 74 = 82" | implemented | `src/state.rs` L32-41; sole creation site `instructions/deposit_bird.rs` L84 `let space = 8 + Mapping::INIT_SPACE;`, allocate + `try_serialize` at L110-143 |
| REQ-11 | "`Pool` carries data, so it can never be the payer for Mapping init or ATA creation" | implemented | Single ATA-creation funnel `instructions/mod.rs` L35-58 hard-wires `payer: treasury`; Mapping rent funded by treasury transfer (`deposit_bird.rs` L89-98) with self-signed allocate/assign (L113, L123) — `create_account` never invoked; treasury typed `SystemAccount` (`deposit_bird.rs` L58-59); the program's only Anchor `init` creates Pool with admin as payer (`initialize_pool.rs` L19-27) |
| REQ-12 | "separate **`["treasury"]` PDA, system-owned, zero data**, which signs as payer via seeds" | implemented | `instructions/mod.rs` L35-58 signs with `[TREASURY_SEED, bump]`; `initialize_pool.rs` L38-43 deliberately never init/allocate/assigns the treasury; `tests/level1.rs` L389-392 asserts system-owned + zero data post-init |
| REQ-13 | "Closed-account rent returns there." | implemented | The program's only close: `instructions/fix_mapping.rs` L23-29 `close = treasury`, target pinned to the canonical PDA (L57-58); `tests/level1.rs` L454-482 verifies the lamport flow |
| REQ-14 | Four seed constants `"pool"/"vault"/"map"/"treasury"` | implemented | `src/constants.rs` L3-6 exact match; all PDA derivations use the constants (e.g. `swap.rs` L17, L34, L48, L69, L138); tests import the same constants (`tests/level1.rs` L37) |
| REQ-15 | `CUSTODIAN = pubkey!("HxwZCEMg…iiUB")` | implemented | `src/constants.rs` L19-21 exact key on the default build; L22-24 test-keys fixture arm is spec-sanctioned (appendix L136-138); `scripts/verify_mainnet_artifact.py` L42-48 guards the shipped artifact against the fixture key |
| REQ-16 | Custodian as compile-time constant for deposit source / fix_mapping + recover destination | implemented | `deposit_bird.rs` L24-36 (source ATA owner pinned to the constant, custodian must sign); `fix_mapping.rs` L74-79 and `recover.rs` L74-79 assert the destination is the ATA of the constant via `require_canonical_ata` (`mod.rs` L60-76); `state.rs` L10-23 — Pool carries no custodian field, so identity cannot come from mutable state |

## Undocumented behavior

Three behaviors no document describes. None is a confirmed vulnerability; each is a
documentation fix, and the first two contradict absolute claims the docs make.

**1. The admin CAN receive a remint through `swap`.**
`swap.rs` L15 accepts any signer as `holder`, L22-26 requires only that the signer owns a
token account holding an unclaimed original — the compiled ADMIN key is not excluded. An
admin who acquires an original (open market or personal holdings) and calls `swap` receives
its remint at ATA(admin, new_mint) (L96-102, L139-152). Three documents state the guarantee
as absolute: `lib.rs` L13 "No instruction can deliver a token to the admin", `constants.rs`
L17-18 "Admin can never receive a token from any instruction",
`SECURITY_CHECKLIST.md` L34-35 "**No instruction can deliver a token to the admin** —
mutation-tested". The real invariant is narrower: no *admin-gated* instruction names admin as
a destination (fix_mapping/recover/deposit are custodian-pinned). Benign in value terms — the
admin surrenders an original of equal value — but an auditor reading the docs alone would
believe token delivery to admin is impossible on every path, which is false as written.
Fix: narrow the wording in all three places.

**2. The token program id in every token CPI is caller-supplied, not compile-time fixed.**
All four token-moving instructions build their CPI from `ctx.accounts.token_program.key()`
(`swap.rs` L125, L141; `deposit_bird.rs` L167; `fix_mapping.rs` L94; `recover.rs` L94), and
the same caller key feeds the canonical-ATA derivations (`swap.rs` L93, L100). The
`Interface<TokenInterface>` type restricts it to the two official token programs, so the
caller selects classic SPL Token vs Token-2022 per call — `deposit_bird` will custody a
Token-2022 mint if the custodian presents one. `SECURITY_CHECKLIST.md` L53-54 claims
"CpiContext built with `Token/System/AssociatedToken::id()` — no caller-supplied program
reaches a CPI"; that is true only for the System and AssociatedToken CPIs (`mod.rs` L45-46;
`deposit_bird.rs` L92, L114, L124). Not exploitable on the evidence — the id must be one of
the two genuine token programs and must match the mint's owner — but the checklist misleads,
and it hides that the deposit path is Token-2022-capable. That matters because Token-2022
extensions (e.g. permanent delegate) could bypass the vault's custody if such a mint were
ever deposited; only the operational sweep, not the program, would catch it. Fix: correct the
checklist line and document the Token-2022 posture on deposit.

**3. `swap` returns `NotRecoverable` for a mis-derived vault ATA.**
`swap.rs` L89-95 emits `NotRecoverable` when the supplied `vault_original_ata` is not the
canonical ATA(vault, old_mint). SWAP_SPEC.md presents that error as recover-only (error
table L502; §4b L313-315), and the spec's swap pseudocode (L144-160) shows no error for this
account at all. Anyone writing client-side error handling or monitoring on-chain errors from
the docs would misattribute a holder's malformed swap as a recovery event. Cosmetic in
security terms; fix is to add the row to the spec's error table.

## Problems in the documentation

1. **Overbroad admin invariant, stated in three places** — `lib.rs` L13, `constants.rs`
   L17-18, `SECURITY_CHECKLIST.md` L34-35 claim no instruction can deliver a token to the
   admin; `swap` can, when the admin acts as an ordinary holder (item 1 above). The fix is
   to the documents.
2. **SECURITY_CHECKLIST.md L53-54 is factually wrong about token CPIs** — the token program
   id is caller-supplied (constrained by `Interface<TokenInterface>`), not
   `Token::id()`-fixed (item 2 above).
3. **SWAP_SPEC.md's error table is incomplete** — `NotRecoverable` is reachable from `swap`
   (item 3 above), but the spec attributes it exclusively to `recover`.
4. **`NO_DNA=1` has no discoverable consumer** — IMPLEMENTATION_APPENDIX.md L33 mandates
   the prefix on agent-run builds, but nothing in the repo reads the variable, and neither
   the compliance check nor the refutation could establish what it gates. The instruction is
   followable but unexplained; the appendix should say what NO_DNA does or link to what
   consumes it.
5. **The Solana CLI pin is prose-only** — REQ-02's "3.1.10" lives in README/appendix/plan
   text; `program/Anchor.toml` has no `solana_version` key, so no tooling enforces it.
   Minor: adding the key would make the documented pairing machine-checked.

No requirement in this pass was marked undecidable, no document was unreadable, and no
check failed without a verdict.

## Considered and dropped

Two candidate divergences were raised and refuted. A single refuter is enough to drop a
finding, so they are listed here for the reader to re-examine.

- **REQ-06 ("Agent-run builds: prefix `NO_DNA=1`") — claimed partial: the committed
  build/test scripts don't embed NO_DNA=1.** Dropped because the requirement, read verbatim
  (IMPLEMENTATION_APPENDIX.md L33), prescribes an invocation convention — the agent types
  the prefix — and its own second example (`NO_DNA=1 anchor test`) demonstrates the
  outer-prefix mechanism propagating into the Anchor.toml script's child process. Every
  agent-facing channel carries the prefix (CLAUDE.md L23, README L11/L17-18/L21, both test
  headers). The finding measured the code against a criterion the document does not state.
- **REQ-07 (verifiedBuild via OtterSec, "Use the standard mechanism instead") — claimed
  partial: no repo script/CI actually runs the verification.** Dropped because the
  requirement is a substitution directive aimed at the *definition of Gate 6* — which was
  rewritten to verifiedBuild everywhere (IMPLEMENTATION_PLAN.md L107-108, L235-239) — and
  the mechanism verifies *deployed* bytecode, which cannot exist before the Phase-6 mainnet
  deploy the plan scopes it to. Demanding a local verify script also runs against the
  requirement's own logic (public verification instead of local hand-rolls, which are absent
  repo-wide). Residual: actual execution at Phase 6 remains to be evidenced at deploy time.

## Coverage — requirements not checked

This pass verified 15 requirements (REQ-01 – REQ-09, REQ-11 – REQ-16). Everything else in
the extracted requirement set was **not checked**, and its status is unknown rather than
compliant. That includes REQ-10, REQ-17 – REQ-123 of the base pass, and every multi-pass
suffixed ID (the -2 through -8 series) — several hundred IDs in total, spanning the
remainder of IMPLEMENTATION_APPENDIX.md plus all requirements extracted from SWAP_SPEC.md's
instruction semantics, SECURITY_CHECKLIST.md, TEST_PLAN.md, IMPLEMENTATION_PLAN.md, and
program/README.md beyond the toolchain/state/PDA/custodian slice. The exact ID list is
recorded in the audit run's task log.

Until those are checked, this report supports the claim "the toolchain, account layouts,
treasury design, seeds, and custodian constant match their documents" — and nothing broader.
