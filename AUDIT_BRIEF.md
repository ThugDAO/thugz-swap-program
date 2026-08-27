# Thugz Swap — reviewer brief

For the three developers reviewing the program on devnet.

Read `SWAP_SPEC.md` first. This document exists so three reviewers don't spend their time
re-checking the same three things, and so the parts most likely to be wrong actually get
attacked.

---

## Please submit before comparing

All three of you are independent of the design — none of you shaped it. That is worth more
than the process asked for, and it is easy to lose without noticing.

**Write up your findings before reading anyone else's.** A review that has already seen
another review is not a third opinion; it is agreement. The first submission anchors the
rest, people stop pursuing a hunch that "someone would have caught", and three reviews
collapse into one with two endorsements.

Send them to the operator individually. They get pooled afterwards, and anywhere two of you
independently flag the same thing carries real weight — which it would not if you had read
each other first.

If you want to argue after everyone has submitted, please do. The failure mode is only
about the first pass.

---

## What this is, honestly

A peer review by named Solana developers, not a paid firm audit. The program is small — one
meaningful instruction — and it guards 1,274 NFTs belonging to other people.

**It will be described publicly as exactly that.** Reviewed by three developers independent
of the design, named if you're willing. Not "audited", which implies a firm engagement this
isn't. If you'd rather not be named, say so and it becomes "three independent reviewers".

---

## What you get

| | |
|---|---|
| Source | `ThugDAO/thugz-swap-v2`, branch under review — program in `program/programs/thugz-swap/` |
| Spec | `SWAP_SPEC.md` + `IMPLEMENTATION_APPENDIX.md` (part of the spec; wins on conflict) |
| Sequence | `IMPLEMENTATION_PLAN.md` — phases 0–13 plus 3b, gates, reversibility map |
| Tests | `TEST_PLAN.md` — six levels; Level 1 = 26 LiteSVM tests, Level 2 = property suite (`PROPTEST_CASES` env), both in `program/programs/thugz-swap/tests/` |
| Devnet | Live 20-bird desk (test-keys build scales `EXPECTED` to 20): program `BGMFnkLVFynUXbeSAyhgNxUUx453f8kFBnvLUNjLAcEi`, page at https://thugz-swap-devnet.pages.dev — ask and we fund you a wallet with mock originals |
| Full scale | Phase 4 rehearsal at the true 1,274 scale on a Surfpool mainnet fork — every Level 4 row incl. a planted bad pair, evidence in `verification/phase4_stage2_log.md`, reproducible below |
| Sweep | `verification/sweep.py` + `sweep_report.json` — the pre-seal verifier (see §3) |
| Prior passes | `SWAP_PLAN_REVIEW.md`, PR #1, and `program/audits/` — three machine passes over the built source (vuln scan clean; 45-agent spec-to-code, 0 confirmed divergences; Codex, 5×P2 fixed) |
| Measured | swap 73,518 CU / 607 bytes; two batched 133,553 CU / 856 bytes; devnet failure matrix 40/40 (`program/devnet_matrix_results.txt`) |

---

## The one thing to know about prior review

Three reviews have already run, and one of them produced a bad outcome worth understanding:

**A wrong number in this repo became "consensus".** An early doc claimed the previous swap
round ran two years. Two reviewers cited it back as a reason to extend the unlock window, and
it was nearly written into an immutable on-chain value on the strength of "three reviewers
agree". It actually ran over three years. Nobody checked, because it came from the document
they were reviewing.

This is the specific reason for the submit-before-comparing request above. Three people
reading the same spec will inherit the same framing; the only defence is that each of you
formed a view before seeing the others'.

**So: do not trust a factual claim in these docs because it is in these docs.** Where a
number matters, verify it against the chain. Several are checkable in a single RPC call.

---

## Where to spend your time

Roughly in order of how likely we are to be wrong.

### 1. `fix_mapping` — the one place a constraint was loosened

Every other change in this design removed a power. This one adds one back: admin can close a
Mapping and pull a remint back out of the vault, while `!sealed`.

Two safety arguments for it have already been wrong. v1 said it "grants no new authority
because admin already writes every mapping" — false: writing a mapping is not custody of a
token, and this was a unilateral admin drain path for all 1,274. v2 said keeping HxwZ warm
closed it — false: HxwZ does not sign `fix_mapping`.

The current argument, v3: **the destination is the custodian's ATA, pinned to a compiled-in
`CUSTODIAN` constant (HxwZ) — never admin's, never caller-supplied, never set at init.**
Admin can undo a bad deposit; admin cannot pocket. The same constant pins the `deposit_bird`
source and the `recover` destination, so no instruction in the program delivers a token to
admin.

**Attack that argument.** It is the third one; the first two failed. If this design has a
hole, it is here.

### 2. The seal gate

`swap` must revert unless `sealed`. Everything about the deposit window's safety rests on it.
Try to find any path — pause state, recover, a partially-applied correction, account
substitution — that lets a swap through with `sealed == false`.

### 3. The sweep

`deposited == expected` is arity, not correctness: 1,274 *wrong* pairs satisfy it. The
off-chain sweep is the only thing that checks identity and injectivity, and it runs
immediately before an irreversible step.

A sweep that passes a corrupt set is the worst available bug. The sweep now exists
(`verification/sweep.py`, 11 sections), has had one machine review pass (four fail-closed
holes found and fixed — the report trail is in the git history), and in the Phase 4
rehearsal it caught a deliberately planted bad pair (a live cantangler bird's mint wearing
another bird's remint) at full scale. **No human has reviewed it yet. It runs immediately
before the irreversible seal; it deserves one of you reading it line by line.**

### 4. Mint provenance

`swap` reads `old_mint` from the surrendered token account; `deposit_bird` reads `new_mint`
from the token being moved. Neither should ever come from instruction data. Confirm there is
no path where a caller-supplied value reaches a PDA derivation.

### 5. `recover`

Runs only after `unlock_ts`, two years out, so it will get the least testing and the least
attention. It must never touch a deposited original, and must leave `claimed == false`.

### 6. Rent and lamport flow

A system-owned `["treasury"]` PDA funds ATA creation and Mapping rent — the Pool data
account never holds rent. Look for: a drained treasury causing confusing failures, rent not
returned to the treasury on `fix_mapping`, anything that could `assign` or `allocate` the
treasury (which would kill it as a `create_account` payer permanently), and account-init
griefing via pre-sent lamports.

---

## Already reviewed — say so if you disagree

Not settled law, just where prior passes landed. Contradict any of it.

| | |
|---|---|
| PDA per original over a merkle root | Fixed set, no additions; holder verifies from chain rather than our file |
| Mapping and receipt merged | Failed tx reverts all writes, so no half-swap persists |
| Read-back-before-seal is the best obtainable completeness check | 1,274 PDAs cannot be hashed in one transaction |
| No "souvenir" merkle root | A root `swap` never checks is a comment, not a constraint |
| Originals locked forever, not burned | Receipt already records the swap |
| Batch sizes | Deposit 4/tx with two signers; swap measured 607 bytes / 73,518 CU, two batched 856 bytes / 133,553 CU — both inside the 1,232-byte and 200k-CU defaults |
| Rent payer is a system-owned `["treasury"]` PDA, not `Pool` | A data account cannot fund `create_account`; an earlier draft had `Pool` paying and would have failed at the first deposit |
| `CUSTODIAN` is a compile-time constant, not an init parameter | A mis-set init field would put admin back in the destination seat; a constant is pinned by the verified build |

---

## What we would like out of it

1. Anything that lets a holder lose an original without receiving the right remint.
2. Anything that lets someone other than the rightful holder claim a remint.
3. Anything that makes the desk permanently stuck — a state where neither swaps nor recovery
   work.
4. Anything in the spec that the code does not actually do. Divergence between the two is how
   the last three problems were found.

Findings against the docs are as useful as findings against the code. If the spec says
something the implementation doesn't, we would rather hear it from you than from a holder.

---

## Reproducing the full-scale rehearsal

Everything in `verification/phase4_stage2_log.md` reruns from this repo (pinned toolchain:
Anchor 1.1.2, Solana CLI 3.1.10, Surfpool 1.5.0; agent builds prefix `NO_DNA=1`):

```
# unit + property suites
cd program && NO_DNA=1 anchor build -- --features test-keys
cargo test --features test-keys -- --test-threads=1

# mainnet-order rehearsal on a mainnet fork (no mainnet writes; needs any mainnet RPC)
surfpool start --no-tui --no-studio --no-deploy --port 8999 --rpc-url <mainnet rpc>
NO_DNA=1 anchor build            # default features = mainnet constants; then:
solana program deploy target/deploy/thugz_swap.so --program-id <program keypair> -u http://127.0.0.1:8999
THUGZ_RPC=http://127.0.0.1:8999 cargo run -p rehearsal-driver --bin deposit_run -- --init
python3 tools/phase8_set_and_verify.py            # collection verify (preflight-gated)
cargo run -p rehearsal-driver --bin deposit_run -- --plant-bad "THUG #3329=<any wrong mint>"
THUGZ_SWEEP_RPC=http://127.0.0.1:8999 python3 ../verification/sweep.py   # must FAIL on the plant
cargo run -p rehearsal-driver --bin stage2_ops -- fix-mapping <wrong mint>   # then redeposit, sweep, seal
cargo run -p rehearsal-driver --bin swap_battery                          # swaps, race, drained treasury
```

The rehearsal keypairs are ours; reviewers get throwaway equivalents on request, or read the
tools — each is a single file under `program/tools/`.

**Standing rule:** if any finding forces a code change, Phases 3 and 4 rerun in full before
anything reaches mainnet. Do not soften a finding to avoid triggering that; the rerun is an
afternoon.

---

## Timing

The review gates the mainnet deposit run. Nothing irreversible happens until it is done, so
take the time it takes — but tell us roughly how long, because it sets the launch date more
than anything else on the list.
