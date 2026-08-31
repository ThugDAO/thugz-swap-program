# Phase 4 — Stage 2 clean-run evidence log (2026-08-27)

Full mainnet-order rehearsal on a fresh Surfpool fork of mainnet (slot ~442.1M),
mainnet artifact (SAFE TO DEPLOY), real keypairs signing locally only.

| Step | Result | Evidence |
|---|---|---|
| Preflight re-run (mainnet, 6 checks) | PASS, 0 failures | preflight_report.json (fresh) |
| Phase 6 deploy at CaWcaw…, authority birdAyQ… | ✅ | program show output |
| Phase 7 init + treasury 5 SOL | ✅ 34s | pool decoded back |
| Phase 8 set-and-verify 1,274 (preflight-gated) | ✅ 618s, 0 tx failures | Gate-8 fresh recount 1274/1274 |
| Phase 9 deposit run (plan order: AFTER verify) | ✅ 319 txs, 177s | pool.deposited 1274 |
| **Planted bad pair**: THUG #3329 remint under cantangler #0600 mint | deposit succeeded (program cannot know) | tx 64QPfTEc… |
| **Sweep catches the bad pair** | ✅ FAIL—DO NOT SEAL, exit 1 | `2.mapping 7UB43uaJ… account missing` |
| fix_mapping on wrong mint | ✅ deposited→1273, remint → **HxwZ ATA** (never admin), mapping closed, rent→treasury | stage2_ops POST line |
| Seal at 1273 | ✅ refused: `Incomplete` 6007 / 0x1777 | seal_attempt.txt |
| Re-deposit correct pair | ✅ deposited→1274 | chain-derived resume found exactly 1 |
| Clean sweep | ✅ PASS—SEALABLE, 0 failures, exit 0 | sweep_report.json |
| Seal at 1274 | ✅ | tx pkEqmPw2… |
| Recover before unlock_ts | ✅ refused: `Locked` 0x1778 | stage2_ops |
| Swap battery post-seal | ✅ 6/6 (incl. race, zero-SOL, drained treasury legible + refill recovery) | swap_battery_report.json |
| Time-warp past unlock_ts (+2y) | ✅ surfnet_timeTravel absoluteTimestamp (ms) | |
| Recover post-warp | ✅ remint → HxwZ ATA, pool.recovered=1, Mapping.recovered=1, claimed untouched | stage2_ops POST |
| Recover same pair again | ✅ refused (idempotent, 0x1773) | |
| Recover a swapped pair | ✅ refused (0x1773) | |
| **Reconciliation invariant** | ✅ vault holds 1,264 = 1274 expected − 9 swapped − 1 recovered | direct ATA count |

## Measurements (mainnet planning numbers)
- Deposit run: 319 txs / 177s local → mainnet estimate 10–20 min at real confirmation latency
- Phase 8 verify: 1,274 txs / 618s local → mainnet estimate 15–25 min
- Swap: 73,518 CU / 607 bytes single; 133,553 CU / 856 bytes two-batched (fits 200k default, no ComputeBudget ix needed)
- Sweep: ~2–4 min with warm immutable-content cache; gateway-weather-resilient

## Notes
- Two late-mined Arweave image txs surfaced (mined days after upload; gateways lag serving
  them). All 1,274 image hashes were verified against claim-map art_sha256 in sweep run 1;
  the sweep now caches proven hashes of immutable content (sweep_immutable_cache.json).
- Frontend TODO carried forward: drained-treasury error maps to "your wallet needs SOL" —
  must become "desk needs a top-up" (holder never pays rent in swap).

**GATE 4: ALL CRITERIA MET.**

---

# Phase 3+4 RERUN — post-Gate-5 batch (2026-08-27, same day)

Per the standing rule: the pooled fix batch changed program code, so both phases reran
against the patched artifact (`fa8b448`+).

| Step | Result |
|---|---|
| Level 1 (now 27 tests, incl. new Token-2022 rejection) | 27/27 |
| Level 2 property suite | 2/2 |
| Mainnet artifact guard | SAFE TO DEPLOY |
| Devnet matrix (fresh throwaway `GHFY…`, 42 rows incl. 2 new Recovered-guard rows and the corrected idempotency expectation) | **42/42** |
| Fork: deploy + init under the NEW 2-year machine-enforced floor | ✅ |
| Phase 8 rerun (preflight-gated) | 1274/1274, 631s, 0 failures |
| Deposit rerun + planted bad pair (cantangler #0600) | 319 txs, 166.6s |
| **Sweep --no-cache catches the bad pair** | ✅ FAIL, exit 1, `2.mapping` missing for #3329's real old |
| fix_mapping → seal@1273 `Incomplete` → redeposit | ✅ (all re-proven) |
| Clean sweep | PASS — SEALABLE, 0 failures (cache of same-day-verified hashes; see note) |
| Seal → battery | 6/6; CU measured 105,023 single / 151,546 two-batched this fork (vs 73,518/133,553 first rehearsal — treat as a RANGE across runtime contexts; every value fits the 200k default, two-batched needs no ComputeBudget ix) |
| Recover drill (Locked pre-warp, OK post-warp, idempotent, swapped-pair refusal) | ✅ all error codes verbatim |

**Arweave gateway-weather note:** the late-mined image/metadata cohort (Arweave blocks
~1,986.8k–1,987.6k) 404s erratically at gateways right now; a full `--no-cache` fetch
during a bad window stalls in fail-closed retries. All content was fetched and
hash-verified against the claim map earlier the same day (bad-pair sweep run + prior
full passes); the rerun's clean sweep used that same-day cache. **The mainnet sealing
run still mandates `--no-cache`** — by then the cohort will have propagated, and if
weather strikes on sealing day, refusing to seal is the correct outcome.

**GATE 4 (rerun): ALL CRITERIA MET against the patched artifact. GATE 5: CLOSED.**
