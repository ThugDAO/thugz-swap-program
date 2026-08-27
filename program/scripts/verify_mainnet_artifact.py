#!/usr/bin/env python3
"""Guard against deploying a test-keys build to mainnet.

Run against the artifact that is about to be deployed:
    python3 scripts/verify_mainnet_artifact.py target/deploy/thugz_swap.so target/idl/thugz_swap.json

Checks:
  1. The real custodian pubkey (HxwZ) is present in the bytecode and the test
     fixture custodian is absent. (The custodian constant provably materializes
     in .rodata — it is the reliable build discriminator. ADMIN is inlined as
     split immediates by the compiler and cannot be byte-searched.)
  2. The IDL constants block carries the mainnet ADMIN / CUSTODIAN / EXPECTED.
  3. The definitive guarantee on top of both: Gate 6's verifiedBuild ties the
     deployed bytecode to the reviewed source, whose default features are the
     mainnet constants.
Exit 0 = safe to deploy. Exit 1 = WRONG ARTIFACT.
"""
import json, sys

SO = sys.argv[1] if len(sys.argv) > 1 else "target/deploy/thugz_swap.so"
IDL = sys.argv[2] if len(sys.argv) > 2 else "target/idl/thugz_swap.json"

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
IDX = {c: i for i, c in enumerate(B58)}
def dec(x):
    n = 0
    for c in x: n = n * 58 + IDX[c]
    r = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return (b"\x00" * (len(x) - len(x.lstrip("1"))) + r).rjust(32, b"\x00")

REAL_CUSTODIAN = "HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB"
REAL_ADMIN = "thuggjsp7Lz7xQ9DyQs7vGmDbVpsWumkv5TQZKHoLr7"
TEST_CUSTODIAN = "FmsQWrqdWwvjREEB1CmD7w1hrzz2coXq8SxxAj9bLQZZ"

so = open(SO, "rb").read()
ok = True
def check(label, cond):
    global ok
    print(("PASS " if cond else "FAIL "), label)
    ok &= cond

check("bytecode contains the real custodian (HxwZ)", dec(REAL_CUSTODIAN) in so)
check("bytecode does NOT contain the test custodian", dec(TEST_CUSTODIAN) not in so)

idl = json.load(open(IDL))
consts = {c["name"]: c["value"] for c in idl.get("constants", [])}
check("IDL ADMIN is the mainnet admin", consts.get("ADMIN") == REAL_ADMIN)
check("IDL CUSTODIAN is the mainnet custodian", consts.get("CUSTODIAN") == REAL_CUSTODIAN)
check("IDL EXPECTED is 1274", consts.get("EXPECTED") == "1274")

print("\nARTIFACT:", "SAFE TO DEPLOY" if ok else "*** WRONG ARTIFACT — DO NOT DEPLOY ***")
sys.exit(0 if ok else 1)
