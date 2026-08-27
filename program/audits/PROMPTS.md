# Audit prompt templates — defensive framing

Copy these verbatim for audit work on the Thugz Swap program. They express the SAME
review as attacker-voiced prompts, but framed as verification/correctness so an audit
session stays on its intended model instead of downgrading. See CLAUDE.md → "Auditing
this program — framing convention".

## Codex — program source vs spec

> Security review of a built Anchor program that custodies 1,274 NFTs, following its
> written specification. Read every file under `program/programs/thugz-swap/src/` and
> check the implementation against `SWAP_SPEC.md` + `IMPLEMENTATION_APPENDIX.md` (the
> appendix wins on conflict). Report, by severity (P1 blocks mainnet / P2 fix before
> launch / P3 nit), any place where:
> 1. A holder could surrender an original without receiving the correct remint, or
>    receive the wrong one (i.e., the swap invariant does not hold on some path).
> 2. A remint could be delivered to anyone other than the current rightful holder of the
>    matching original.
> 3. The desk could reach a stuck state (no swaps and no recovery), or treasury/vault
>    funds could be stranded or double-counted.
> 4. The code diverges from the spec — especially the manual Mapping init, the
>    treasury-as-payer CPI, `transfer_checked`, the compiled constants, the seal gate,
>    the custodian-pinned destinations, and `Pool.recovered` accounting.
> 5. An Anchor correctness issue exists: reload-after-CPI, duplicate mutable accounts,
>    close/revival, rent math, missing `has_one`, init-once on Mapping.
> Terse, numbered findings; file:line; the exact issue; a suggested fix. If a point I
> raised is already handled correctly, say so briefly.

## tob-spec-to-code (skill / workflow)

> Check `program/programs/thugz-swap/` against `SWAP_SPEC.md` + `IMPLEMENTATION_APPENDIX.md`
> (appendix wins). Focus on enforceable program-behavior requirements: instruction
> constraints, the §6 invariants, the §6b load-bearing constraints, account layouts,
> error semantics, the manual Mapping init rules, the treasury payer rules, and the
> CUSTODIAN/ADMIN/EXPECTED compiled constants. Report where code and spec disagree.

## tob-solana-vuln-scanner (skill)

> Scan `program/programs/thugz-swap/` for the six Solana vulnerability classes and report
> which hold and which need attention.

## Adversarial-verify (correctness framing)

When double-checking a finding, dispatch independent reviewers to **confirm the guard
holds** — "does constraint X actually enforce invariant Y on every instruction path?" —
rather than to break it. A finding survives if a majority cannot show the guard holds.
