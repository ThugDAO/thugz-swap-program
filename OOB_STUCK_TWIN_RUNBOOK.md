# Out-of-band redemption — the four stuck-twin birds

Census ruling (2026-08-28, "Sixteen Stragglers"): four birds have their twin stuck in the
old entangler's custody wallet, which nobody holds keys to. They are deliberately OUTSIDE
the sealed swap pool. When a holder of one of these originals comes to us, the remedy is
per-request and manual: **take in the original, mint them a fresh new bird.**

This runbook is that procedure. It applies to exactly these four and nothing else.

## The four

| Bird | Original mint (holder side) | Holder (as of 2026-08-31) | Twin stuck in entangler `8d8CYjuy…uriA` | Original's art |
|---|---|---|---|---|
| THUG #0552 | `24zJuCGtYAPHFoPynNu68Cxt1SAHxZJMcbC6bYbNMWnC` | `APL7weGXY8dXgWzWLtBX7nfYhniYyVJREJW2hsR8K2Jx` | `AHeXEoy25s56aUWRdv3ptjkNRiV9BJETtDAFjUvKfFZ3` | fine |
| THUG #0758 | `DNgimUqLsdkvtRu5g4A9ycCSw2NNgsssgS19dX356sZz` | `APL7weGXY8dXgWzWLtBX7nfYhniYyVJREJW2hsR8K2Jx` | `4DPZt2FFmpkXwsbNaQfiygJZHNFA4LxPTSGdPkuh5iNM` | fine |
| THUG #1074 | `A1pwRPTt5MxpCUEvrj37Mir8o7y3RwX958Wffnc5g9Vo` | `ALVNJn1E9CqzvWbnFQLZaiS76ZuYMSny85u3mqVnexJm` | `BQUT6VT63mL6DnpwEPNiS3LaZwHUei7rym9Kx5sGGfZa` | **LOST** (never-mined) |
| THUG #1885 | `yZiYS9DS98irYbXw57owXMrwNAmKva5giFPkvLj2CC2` | `GMuS11ma4r3YcSAbKtCDhQt8Da2JKLoXS8T9dDFkKeGL` | `EYayzumFmvgwxURvnLkWbi8hgSKRsvh7XRcPPpdfnwqh` | **LOST** |

Holders may have changed since the snapshot — current ownership is what counts, verified
live at intake. All four stuck twins carry permanently mined Arweave art + metadata
(recovery uploads, verified 2026-08-27), so the art side is already solved. All four twins
are confirmed VERIFIED members of `5Kwhy…` (DAS-checked 2026-08-31) — which is why step 5's
unverify matters: without it a redemption would leave two verified assets sharing one name.

## Hard rules

- **Never** touch `recovered/remint/claim_map_all.json` — the sealed redemption contract
  does not change for these. Record out-of-band cases in their own ledger (below).
- **Never** invoke the swap program — no instruction of it plays any part here. This is a
  plain custody exchange plus a fresh mint, done by HxwZ. (The treasury PDA appears only
  as a passive lock-box address in step 4 — tokens are sent *to* it, nothing is asked
  *of* the program.)
- Only these four originals qualify. Anything else claiming to be "stuck" gets the
  standard answer: if it's in the claim map, use the desk; otherwise it's a census case.

## Procedure

### 1. Intake
Holder contacts us (usually @thugdao DM). Collect: which bird, and their wallet address.
No signatures, screenshots, or seed-anything needed — ownership is checked on-chain.

### 2. Verify (read-only)
- DAS `getAsset` on the original mint: `ownership.owner` equals the wallet they gave,
  `burnt: false`, supply 1.
- The token account must not be frozen (a marketplace listing) — if listed, they delist
  first, same as the desk.
- Confirm the matching twin is still in `8d8CYjuy…` (it always should be; if it ever
  moved, stop and investigate — that would mean the old entangler key is live somewhere).

### 3. Prepare the fresh bird (before anything moves)
Adapt the proven phase-2 pipeline (`recovered/phase2/mint_phase2.mjs`, metaboss under
`~/.thugbirdz-keys/hxwz.json`):
1. **Image**: reuse the stuck twin's mined Arweave image tx (fetch the twin's current
   metadata via DAS, take its image txid; confirm the bytes load from a gateway). Same
   art, already permanent — no new image upload needed.
2. **Metadata JSON**: same shape as the remints — name `THUG #NNNN`, symbol `THUG`,
   `seller_fee_basis_points: 500`, creators/royalties to Squads treasury
   `4td5cAuEmpD8U1icAbXpCZpmEKJjAPjMzEgkbL225FxY`, and crucially
   `properties.provenance.original_mint = <original mint>` plus the `Original-Mint`
   Arweave tag on the upload. Upload with the Arweave JWK.
3. **Mint** the new bird to HxwZ. Cost ≈ 0.02 SOL + upload dust, paid by HxwZ.
4. **Verify into the collection**: `metaboss collections set-and-verify` into
   `5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc` (same as Phase 8).
5. Show the holder the new mint address — they can see it on-chain, in the collection,
   with their original recorded in its provenance, before they send anything.

### 4. The exchange
Default flow (fine for a reputation-backed, publicly-recorded support case):
1. Holder sends the original to HxwZ (`HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB`).
2. On confirmation, HxwZ sends the prepared new bird to the holder's wallet. Do it in
   the same sitting — minutes, not hours.
3. HxwZ then forwards the surrendered original into a token account owned by the
   **treasury PDA** (`7kr7s7WmtSvUqq3TtmEKefXYB5yJw9ZEj7Uz1DeKkWRN`): create the
   treasury's ATA for that mint (HxwZ pays the ~0.002 rent) and transfer. No swap-program
   instruction can sign token transfers as the treasury, so the bird is locked exactly
   like its vaulted cousins — releasable only if the DAO ever deliberately ships a
   program upgrade to do so (and never, once the upgrade authority is burned).

For a technical holder who asks, offer the atomic variant instead: one transaction
containing both transfers, HxwZ partial-signs, holder countersigns and submits. Either
way the new bird exists and is inspectable before the holder commits.

### 5. Retire the stuck twin (after the exchange completes)
Unverify the stuck twin out of the collection: verification lives on the item's metadata
and needs only the **collection authority** (HxwZ) — the token itself can stay stuck in
`8d8CYjuy…` forever. `metaboss collections unverify` against the twin's mint, signed by
HxwZ. Confirm afterwards with a fresh DAS read: the twin gone from `5Kwhy…` membership,
the fresh mint present — collection size back to exactly one bird per number.

Do this LAST, only after the holder has their new bird: if anything aborts mid-procedure,
the collection state is unchanged.

### 6. Record
Append to `recovered/remint/out_of_band_ledger.json` (create on first use):
`{bird, original_mint, new_mint, twin_mint_unverified, holder_wallet, exchange_txs,
treasury_lock_tx, unverify_tx, date}` — and note it in the census doc. The surrendered
original ends up locked in the treasury PDA's token account per step 4, alongside-in-spirit
its vaulted cousins; the stuck twin remains where it is as an unverified orphan, a dead
letter unless the old entangler key ever surfaces.

## Net effect on the collection

Membership count is unchanged (twin out, fresh mint in) and the collection keeps exactly
one verified bird per number — no #0600-style name-pair is created. The redeemed bird's
`provenance.original_mint` ties it to the holder's surrendered original; the orphaned twin
is outside the verified collection and stays that way. Explorer/DAS surfaces update on
their own since both read live membership.
