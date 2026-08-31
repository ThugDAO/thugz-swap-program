# Thugz Swap

A one-way redemption desk on Solana mainnet: a holder of a 2021 thugbirdz original signs
**one transaction** that locks their original in a program vault and hands back its
reminted twin with recovered art. 1,274 fixed pairs, no pricing, no ordering, no un-swap.

**Status: LIVE.** Sealed, verified, and open at [migrate.thugbirdz.com](https://migrate.thugbirdz.com). Built, reviewed by three independent
external reviewers (no P1 findings; see `FINAL_REVIEW_PLAN.md`), and deployed 2026-08-31 at
slot 443163190 (signature `45uY7zKG78vNZAfiWDdXLpcUPHHqYdxWxKoNazymh9cT9QZHZQsWWTnxwK2pevAXNqsUJ4hUTnASmFGpzdThHBuY`).
All 1,274 pairs are deposited, swept (0 failures), and sealed; the desk is open.

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

## The desk is live

**https://migrate.thugbirdz.com** (also `thugbirdz.com/migrate`)

- **1,274** originals are redeemable. Your 2021 original goes into the program vault; its
  remint with recovered art comes back in the same transaction. One signature. The desk
  charges nothing — you pay only the network fee.
- **If your bird is listed on a marketplace, delist it first.** Listings freeze the token,
  and a frozen original cannot be swapped; delisting thaws it.
- **The desk does not close.** `unlock_ts` is `1851379200` (2028-09-01T00:00:00Z). That is
  not a swap deadline: swapping continues after it. It is the earliest moment the admin
  *may* recover remints nobody has claimed — nothing more. There is no reason to rush.
- Verify everything yourself: the deployed bytecode ([verified](https://verify.osec.io/status/CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs)),
  your bird's pairing (its Mapping PDA, seeds `["map", pool, old_mint]`), and the
  [pre-seal sweep report](verification/sweep_report.md) — all from public data.

## Ground addresses

| | |
|---|---|
| Program | `CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs` |
| Pool | `7gDE9pxPVV7Cfz5hGfvXUs2x6T7xNL7rto2zDJyqaDoP` |
| Vault | `4JUcCejbDEHzPZLBq7tvrGNCobdTGuqy3wCu9wrxnxYN` |
| Treasury | `7kr7s7WmtSvUqq3TtmEKefXYB5yJw9ZEj7Uz1DeKkWRN` |
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

### Reporting a vulnerability

If you find a vulnerability in the swap program or this repo, please report it privately —
do not open a public issue, post details publicly, or attempt to exploit it against the
deployed program (`CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs`) or the vault it guards.

- **Contact:** DM [@thugdao](https://twitter.com/thugdao) on Twitter/X
- Include enough detail to reproduce the issue; a proof-of-concept against a local fork or
  devnet is welcome, mainnet is not.
- You'll get an acknowledgement within 48 hours and a status update as we triage. Please
  give us a reasonable window to remediate before any public disclosure.
- Good-faith research under this policy will never be met with legal action; public
  credit is offered once the issue is resolved, if you want it.

This policy is also published on-chain via the
[Program Metadata](https://github.com/solana-program/program-metadata) `security` account
for the program.
