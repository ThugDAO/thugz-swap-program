> **HISTORICAL — do not implement from this document.**
>
> This was the first external review, and parts of it are now wrong. Most importantly its
> **build order runs a 5-swap pilot before depositing the rest and sealing**. The current
> spec forbids any swap before `seal` — that constraint is load-bearing, and following the
> order below would open the exact window it exists to close.
>
> It also refers to `unlock_ts` as 4 years; it is **2**. Kept for the reasoning, not the
> instructions. Implement `SWAP_SPEC.md` + `IMPLEMENTATION_APPENDIX.md`.

# Thug Swap plan review

Read against the four Claude artifacts (holder desk, program spec, decision
register, PDA vs Merkle) plus the live comments on the register. Claim map is
1,274 pairs. Nothing is written on-chain yet.

| | |
|---|---|
| **Verdict** | Build it |
| **Storage** | PDA (your call) |
| **Pairs already minted** | 1,274 |
| **Issues that can ship wrong** | 3 |

**The shape is right.** A vault PDA that only releases a remint when the
matching original arrives is the correct product. A hot distribution wallet
cannot sit there for two years. Verify-upfront so HxwZ goes cold, sealed
mappings, originals locked forever, admin cannot withdraw before unlock —
those calls are correct and they compound.

## Do not implement the spec as written

The program spec is a merkle draft. You already chose PDA. The holder page is
an older co-sign desk. If someone builds from those two artifacts they will
ship the wrong program and the wrong copy.

| Artifact | What it still says | What you decided |
|---|---|---|
| Program spec | `merkle_root` on Pool, proof in `swap()`, separate receipt PDA, upgrade burned | PDA per original, merged mapping+receipt, sealed flag, upgrade to Squads after 20 swaps |
| Holder desk | 541 birds, two signatures (holder + DAO wallet), certify days later, empty wallet works | 1,274 birds, holder-only signature against a program, pre-verified into the collection |
| Claim map note | Minted uncollected; collection size is a live redemption counter | Verify all 1,274 before the vault; parent size jumps on day one |

## The three things I would change or pin

### Empty wallet cannot submit a transaction

Product lie if shipped. The spec says the pool pays rent so a holder with no
SOL can still swap. Rent, yes. The network fee, no. A Solana transaction needs
a signed fee payer with lamports. A program cannot be that payer. So one of
these is false:

1. nobody on your side signs anything, or
2. an empty wallet can swap.

Pick one in the spec before anyone writes copy.

Honest version: holder needs a dust of SOL for the fee; the program pays ATA
and mapping rent. That is a good desk. Do not promise (2) unless you run a fee
relayer, which is a server again — smaller trust surface, but not unattended.

### Squads after 20 swaps is not a burn

Trust story. Your call: new wallet, then Squads after launch plus 20
successful swaps. Fine as a staging path. It is not the claim the spec wants
to make (“nobody, including us, can change the code”). A live upgrade
authority over 1,274 NFTs **is** the trust model, and it rests on the Squads
config.

20 is a thin sample against a two-year tail. Make the trigger countable and
public: 20 swaps, then 30 days with no pause incident, then publish the Squads
signers. Decide now whether Squads is permanent or a waypoint to a real burn.
Those are different products.

### Two-year unlock vs the last tail

I would push back. You picked 2 years, then leftover remints to treasury. The
last cantangler round ran two years and was still trickling. A skeptical
holder in month 23 should not feel a deadline.

Four years is the number that makes the time lock feel like infrastructure
rather than a promotion. Two is defensible if you say the date on the desk the
day it opens and never move it. Do not leave this as “about two years.”

## PDA over merkle — you are right

For a fixed 1,274-pair set with no additions, merkle’s only remaining
advantage is auditor familiarity and atomic completeness. You already priced
both. Completeness becomes a sweep-before-seal, which you have to do anyway
because this collection died the first time from “uploaded but never
confirmed.” PDA is the design that matches that lesson.

**Keep**

- Seeds from old mint. Mapping holds new mint + claimed.
- Merge mapping and receipt. One account, one lock.
- `sealed` flipped only after a full read-back of all 1,274, mismatch = hard stop.
- Pool stores `expected = 1274`. Seal requires deposited == expected == mapping count.

**Add to the PDA spec**

- `deposit_bird` creates the mapping PDA in the same tx as the token transfer. No mapping without a bird, no bird without a mapping.
- Swap must fail if `!sealed`. Do not let the desk take originals while admin can still rewrite pairs.
- Batch size 5 is at 1,231/1,232 bytes. Freeze the account set before writing the ix, or it drops to 4.
- Merkle’s “deposit must prove new-side membership” becomes trivial: the mapping PDA is the allowlist.

**On merkle familiarity.** An auditor who has seen OpenZeppelin merkle more
than NFT vault PDAs will spend an extra hour, not an extra week. The 2022
cantangler already used a merkle root because it was minting from a list. You
are not minting. You are looking up a pair that already exists. PDA is the
boring answer for that.

## What I agree with, briefly

| Call | You | Notes |
|---|---|---|
| Verify all 1,274 before the vault | Yes | HxwZ signs once, then cold. Parent size is not a redemption counter anymore — say so on the desk. |
| Vault named publicly, 39% optics | Publish it | Silence reads as team supply. Count + vault address on the swap page. |
| Originals locked forever, not burned | Lock | Correct. Receipt already records the swap. Burning 2021 tokens after recovering them is the wrong story. |
| No stray-NFT rescue | Accept / document | Fine. Same as sending to a wrong wallet. Do not grow the instruction set for this. |
| Admin = new offline wallet, not HxwZ | Yes | Collection authority stays out of routine ops. |
| Pause = admin alone | Yes | A safety valve that needs two people is not a safety valve. |
| Audit = fwaz + two Solana devs | Internal-ish | Acceptable if at least one reviewer did not design it, and they review the rewritten PDA spec, not the merkle draft. |
| Pin toolchain at repo init | Yes | rust-toolchain.toml + Anchor.toml frozen through audit and deploy. |
| One worker, both /swap routes | Yes | Two deploys will drift. |

## Holes the rewritten spec has to close

### Token accounts the sketch skips

Remints are standard Metaplex NonFungible (not pNFT), so a plain SPL transfer
is enough. The swap still has to `create_idempotent` two ATAs: the holder’s
ATA for the remint, and the vault’s ATA for the original. Both rents come from
the pool. If you forget the holder ATA, the “empty of NFTs but has SOL” wallet
fails on first swap.

### Instruction data vs accounts

Do not pass `old_mint` / `new_mint` as free ix data. Derive the mapping from
the mint of the token account being transferred, then require the destination
mint matches `mapping.new_mint`. Otherwise a mismatched account set becomes a
review finding for no reason.

### One-way door

The 2022 entangler let people swap back. This desk does not. That is the right
call for making remints canonical, and it is louder than the holder page
currently is. “The old bird is not destroyed” is true and still a one-way
door. Say that.

### Delegates and listings

Freeze snapshot had 11 delegated tokens and ~79 in marketplace escrow. Delist
copy is enough for escrow. Delegates should be treated the same as listings in
the UI: you can swap, but only after you revoke. The program just sees a
failed transfer otherwise.

## Build order I would actually run

| Step | Done when |
|---|---|
| Rewrite the program spec for PDA (sealed, merged receipt, no proof, fee-payer stated) | Spec and holder page no longer contradict |
| Devnet: full revert matrix + completeness sweep + seal-before-swap | Every failure mode in the spec has a test |
| Mainnet program, unfunded, upgrade on the new wallet | Initialize with expected=1274, sealed=false |
| HxwZ verifies all 1,274 into the parent, then goes cold | Collection size includes the vault set; vault address published |
| Pilot 5 real swaps on team wallets | Right bird, claimed flag, second attempt reverts |
| Deposit remaining 1,269, sweep all mappings, seal | Mismatch is a hard stop, not a retry-later |
| Open the desk; delist note for listed originals | Copy matches the program (one signature, SOL dust, one-way) |
| After 20 swaps + 30 quiet days: move upgrade authority to Squads, publish signers | Or burn, if you decide that is the product |

Sources: Claude artifacts `bde00011`, `d8cef71e`, `d09dbff2`, `2a4a0ddb`;
`recovered/remint/claim_map_all.json` (1,274); register comments from pyle,
2026-08-26.
