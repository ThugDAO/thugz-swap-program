# Thugz Swap

A one-way redemption desk on Solana mainnet: a holder of a 2021 thugbirdz original signs
**one transaction** that locks their original in a program vault and hands back its
reminted twin with recovered art. 1,274 fixed pairs, no pricing, no ordering, no un-swap.

**Status: fully specified, nothing built, nothing deployed.** A final internal review pass
completed 2026-08-26. The one remaining gate before implementation (Phase 1) is three
independent external reviews — see `AUDIT_BRIEF.md`.

## Read in this order

| | | |
|---|---|---|
| 1 | `FINAL_REVIEW_PLAN.md` | Where the design is weakest, what six review passes found, exit criteria |
| 2 | `SWAP_SPEC.md` | The specification — accounts, instructions, invariants, failure matrix |
| 3 | `IMPLEMENTATION_APPENDIX.md` | **Part of the spec; wins where they disagree.** Layouts, seeds, errors, toolchain, sweep script |
| 4 | `IMPLEMENTATION_PLAN.md` | 14 phases, a gate between each, reversibility map |
| 5 | `TEST_PLAN.md` | Six levels, from LiteSVM units to production monitoring |
| 6 | `AUDIT_BRIEF.md` | What the three external reviewers are asked to attack |
| — | `SWAP_PLAN_REVIEW.md` | **Historical.** Contradicts the current spec; kept as review record only |

## Ground addresses

| | |
|---|---|
| Program | `CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs` |
| Admin | `thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7` |
| Upgrade authority | `birdAyQ1d6UXissFpwx9WcaxvJanzRcMSvzUkQxPpaV` |
| Custodian (`CUSTODIAN` constant) | `HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB` |
| Collection parent | `5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc` |

`recovered/remint/claim_map_all.json` is the **redemption contract** — the canonical
1,274-pair mapping the deposit run and pre-seal sweep are checked against. The same pairing
is independently and immutably recorded on Arweave (`provenance.original_mint`, written at
mint time).

## Related repos

- [`ThugDAO/thugbirdz-recovery`](https://github.com/ThugDAO/thugbirdz-recovery) — archive of
  the 2026 recovery campaign that produced the remints (start with its `HANDOFF.md`)
- [`ThugDAO/thugbirdz`](https://github.com/ThugDAO/thugbirdz) — thugbirdz.com / thugdao.com
  sites, community map, games

## Security

No credentials are in this repo or its history. Keypairs live outside the working tree —
see `SECRETS.md`. The program workspace (Phase 1) will land in `program/`.
