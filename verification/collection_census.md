# Collection census — chain-verified 2026-08-27

Source of truth: howrare.is original mint list (3,311 mints — the post-mint 2021 index),
fully rescanned via DAS + direct RPC. Honoraries (4, creator `6qHDitum…`) excluded
throughout per operator instruction.

## The reconciliation (operator's equation)

| | Count |
|---|---|
| Original birds (creators `Avkbtawp…` 2,532 + `CzrE3Lhi…` 779) | **3,311** |
| − held inside the entangler wallet `8d8CYjuy…` | 2,019 |
| − dead (supply 0 or hard-burnt) | 18 |
| − live redemption-set originals (their remints wait in the desk) | 1,270 |
| **= remainder** | **4** |

The remainder is **not zero**. The four:

| Bird | Mint | State |
|---|---|---|
| THUG #0552 | `24zJuCGt…` | **art fine** (mined 2021, loads) — missing from every internal list incl. the website's canonical 3,318 |
| THUG #0758 | `DNgimUqL…` | **art fine** — same gap |
| **THUG #1074** | `A1pwRPTt…` | **ART LOST** — metadata + image 404/flicker from gateway cache; never-mined class. Holder `ALVNJn1E…`. **No remint exists.** |
| **THUG #1885** | `yZiYS9DS…` | **ART LOST** — same. Holder `GMuS11ma…`. **No remint exists.** |

## Dead-bird ledger (18)

- 4 — redemption set, token supply 0, metadata intact (THUG #1370/#1400/#1407/#3074):
  remints permanently unclaimable, recovered by custodian at unlock.
- 4 — hard-burnt (Metaplex burn, metadata destroyed), never in the redemption set.
- 10 — token supply 0, metadata intact, never in the redemption set.

## Cross-set totals

- Verified collection `5Kwhy…`: **2,024** = 2,020 entangled twins (creator `8d8CYjuy…`)
  + 4 honoraries. Exact.
- Remints: **1,274**, creator `HxwZ…`, ungrouped until Phase 8 verifies them in
  (target: `5Kwhy…` = 3,298).
- Redemption olds: 973 Avk + 301 CzrE — all from the exclusive cohort, zero entangled. Exact.
- Cantangler-deposited birds sit inside the `8d8CYjuy…` custody (counted in the 2,019);
  one cantangler record's mint is outside the howrare 3,311 (among the never-indexed).

## Open questions

1. **THUG #1074 + #1885** — two holders hold art-lost originals with no redemption path.
   Operator decision required (see below). → **Softened 2026-08-27: twins exist, see below.**
2. Entangler holds 2,019 originals vs 2,020 minted twins — Δ1 unexplained. → **Resolved
   2026-08-27: recount says 2,020 held; see twin reconciliation below.**
3. 3,333 marketing − 3,311 howrare = 22 birds never indexed anywhere. → **Resolved by the
   straggler search: 12 found on-chain beyond howrare (3,323 of 3,333 accounted); 10
   forever unindexed.**
4. THUG #0552/#0758 missing from the website's canonical list — site data gap, cosmetic.
   → **Team: add to lists.**

## The sixteen stragglers — straggler search + team review (2026-08-27/28)

Full result set and per-bird team comments live in the artifact "The Sixteen Stragglers"
(claude.ai/code/artifact/63e2e29d-45f6-4f6f-9473-5a3043f71995). Summary: 16 birds outside
every ledger — 10 art-lost with no remint, 5 art-fine but off the books, 1 cantangler
custody. Creators beyond the two known 2021 batches appear (`BfLqm23E…`, `798kJ3t6…`).

**FINAL operator dispositions (2026-08-28), covering all 16:**

| Group | Birds | Ruling |
|---|---|---|
| 4 — twin stuck in entangler, original in holder's wallet | #0552, #0758, #1074, #1885 | **Leave OUT of the new swap pool.** If an owner comes to us: take in the original, mint them a new one. Handled per-request, out-of-band. |
| 3 — honoraries | #0284 (Wiz Khalifa), #0683 (Sonny Digital), #0692 (Young Thug) | **Record them; can mint whenever.** |
| 1 — duplicate | #0600 (Couby's) | Excluded — "600 A and 600 B"; both original and twin duplicated at #600, chain-confirmed. |
| 2 — data exists, minted Dec 2021 | #0381 (metadata/img found on IPFS), #3311 (same holder `2ey77pDs…`, same creator `798kJ3t6…`) | **Do nothing.** |
| 6 — no metadata/image, minted mid-late Oct–Nov 2021, weird royalties | #0146, #0198, #0399, #0623, #0740, #0790 | **Do nothing** — likely honoraries or OG tests, nature unknown. |

Earlier per-bird team comments (00:16–01:09 UTC) are preserved in the artifact.

Dating context (operator): the original Thugbirdz mint ran late Aug–early Sept 2021.
Every bird in the last two groups was minted after that window (Oct–Dec 2021), which is
what marks them as outside the real collection — one-off honoraries, tests, or unknown,
never part of the drop.

## Twin reconciliation (2026-08-27, fresh full rescan)

A creator-based rescan (DAS, `onlyVerified`) cross-diffed the 2,020 twins against the
2,020 originals held by the entangler wallet, by name:

- **Twins exist with NO original in custody: #0552, #0758, #1074, #1885.** All four twin
  NFTs are themselves held by the entangler wallet `8d8CYjuy…`, unclaimed.
- **All four twins have permanently mined Arweave art + metadata**, verified via GraphQL
  (bundle `XSuUsI-dY…`, block 1,978,113, 2026-08-12, for #0552/#0758/#1074; L1 txs at
  block 1,978,768, 2026-08-13, for #1885 — uploaded by our own campaign JWK
  `H0KhUVGv…`). Gateway `/tx/status` 404s on the first three are the ANS-104 bundle
  artifact, not missing data. Images eyeballed: real thugbirdz pixel art.
- **⇒ THUG #1074 and #1885 (art-lost, no remint) each have a ready-made redemption
  path**: an entangler-held twin with sound, permanent art. Operator ruling 2026-08-28:
  not in the swap pool; if the owner comes to us, take in the original and mint them a
  new one (per-request, out-of-band).
- Symmetric remainder — originals in custody with NO twin: #2032, #2226, #2602, #2756.
  Presumed cantangler deposits (custody without twin mint); cross-check against
  cantangler records when convenient.
