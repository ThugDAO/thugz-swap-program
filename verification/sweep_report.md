# Sweep report — 2026-08-27T17:54:58.921225+00:00

**PASS — SEALABLE**  (0 failures, 111.9s)

- target: `http://127.0.0.1:8999`  pool: `7gDE9pxPVV7Cfz5hGfvXUs2x6T7xNL7rto2zDJyqaDoP`
- claim map sha256: `1d5da51ae50f16b1e5b70c959c6ec05f159e385dd72fe0f31c4623cc704f980a`
- pool state: `{'admin': 'thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7', 'collection': '5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc', 'expected': 1274, 'deposited': 1274, 'swapped': 0, 'recovered': 0, 'sealed': False, 'paused': False, 'unlock_ts': 1850921565}`
- report sha256: `b74a715d309c4033579a450f8b653aaa9a50227a6d60a7b0763df4d80a84ea47`


## Notes
- Membership: 6a target-chain verified 1274/1274; 6b mainnet DAS lists 2024 members, 0 of them claim-map remints, 0 foreign. On mainnet 6a and the DAS view must agree; a lagging indexer shows up here as a note, not a pass.
- Remint mutability recorded: {'mutable': 1274, 'immutable': 0}
- 8: 3 fetched via fallback gateway (txid-addressed, same bytes)
