# Thugz Swap

A one-way redemption desk on Solana mainnet: a holder of a 2021 thugbirdz original signs
**one transaction** that locks their original in a program vault and hands back its
reminted twin with recovered art. 1,274 fixed pairs, no pricing, no ordering, no un-swap.

**Status: deployed to mainnet, not yet initialized.** Built, reviewed by three independent
external reviewers (no P1 findings; see `FINAL_REVIEW_PLAN.md`), and deployed 2026-08-31 at
slot 443163190 (signature `45uY7zKG78vNZAfiWDdXLpcUPHHqYdxWxKoNazymh9cT9QZHZQsWWTnxwK2pevAXNqsUJ4hUTnASmFGpzdThHBuY`).
The pool is not initialized and no birds are deposited; the desk is not open.

### Verify the deployed bytecode yourself

This repo is the source of the deployed program. The build is reproducible with
[`solana-verify`](https://github.com/Ellipsis-Labs/solana-verifiable-build) and the pinned
image (the default image's cargo is too old for the `edition2024` crates):

```
solana-verify build --base-image solanafoundation/solana-verifiable-build:3.1.10 \
    --library-name thugz_swap    # run in program/
solana-verify get-program-hash CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs
```

Both must print `3e0dd78315910f8c962ba9bab5f97e2cd1fd55252ddeb9f54c59b3b870246027`.

## Read in this order

| | | |
|---|---|---|
| 1 | `FINAL_REVIEW_PLAN.md` | Where the design is weakest, what six review passes found, exit criteria |
| 2 | `SWAP_SPEC.md` | The specification — accounts, instructions, invariants, failure matrix |
| 3 | `IMPLEMENTATION_APPENDIX.md` | **Part of the spec; wins where they disagree.** Layouts, seeds, errors, toolchain, sweep script |
| 4 | `IMPLEMENTATION_PLAN.md` | Phases 0–13 plus 3b, a gate between each, reversibility map |
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

No credentials are in this repo or its history — verified by scanning every object in
every commit against the live key material before publishing. All keypairs live outside
the working tree. The two JSON keypairs under `program/programs/thugz-swap/tests/fixtures/`
are throwaway devnet test fixtures compiled into the `test-keys` build only; the mainnet
build uses different, hardcoded public constants whose private halves have never been in
any repository. The program workspace is in `program/`.
