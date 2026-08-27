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
| Source | `ThugDAO/thugz-swap-v2`, branch under review |
| Spec | `SWAP_SPEC.md` — accounts, instructions, invariants, failure matrix |
| Sequence | `IMPLEMENTATION_PLAN.md` — phases 0–13 plus 3b, gates, reversibility map |
| Tests | `TEST_PLAN.md` — six levels, plus the suite itself |
| Live | Devnet deployment with 1,274 mock pairs, seeded and sealed |
| History | `SWAP_PLAN_REVIEW.md` and PR #1 comments — three prior review passes |

Devnet keys and mock birds will be provided so you can attack a live deployment, not just
read code.

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

A sweep that passes a corrupt set is the worst available bug. It is also code nobody has
reviewed yet.

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
| Batch sizes | Deposit 4/tx at 1,214 bytes with two signers; swap 670 of 1,232 |
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

## Timing

The review gates the mainnet deposit run. Nothing irreversible happens until it is done, so
take the time it takes — but tell us roughly how long, because it sets the launch date more
than anything else on the list.
