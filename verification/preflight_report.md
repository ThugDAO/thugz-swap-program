# Pre-deposit verification — PASS

Ran 2026-08-27T01:19:40+00:00 against mainnet via Helius DAS + arweave.net. 1274 pairs.

| Check | Result |
|---|---|
| A. claim map structure (count/injective/disjoint) | PASS |
| B. chain: exists / owner=HxwZC… / not burnt | PASS |
| C. name match, original vs remint (all 1,274) | PASS |
| D. Arweave provenance + strict URI + frozen-name binding | PASS |
| E. Original-Mint tag on every metadata tx (GraphQL) | PASS |
| F. Freeze authority == own master edition PDA on every mint | PASS |

> Collection grouping of remints (Phase 8 not run yet, informational): {'news_verified_in_parent': 0, 'news_ungrouped': 1274, 'news_other': 0}
> Remints mutable: 1274/1274; update authority != custodian on 0
> D: 6 metadata file(s) fetched via fallback gateway https://permagate.io/ after arweave.net served a cached CDN error page; content is txid-addressed, so the bytes are the same record
