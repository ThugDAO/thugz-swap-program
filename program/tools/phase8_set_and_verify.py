#!/usr/bin/env python3
"""Phase 8 — HxwZ sets-and-verifies all 1,274 remints into the parent collection.

A resumable wrapper around `metaboss collections set-and-verify` (the tool that ran
the 2,024-bird MCC campaign). Resume is CHAIN-DERIVED: before any transaction, every
remint's metadata account is fetched and parsed; mints whose collection field already
reads {5Kwhy…, verified} are skipped. The state file records progress only.

  THUGZ_RPC=http://127.0.0.1:8999 python3 phase8_set_and_verify.py [--limit N] [--status]

Plan Phase 8 ordering is load-bearing: re-run verification/preflight_check.py FIRST and
only proceed on PASS — this script refuses to run without a fresh passing preflight
report unless --skip-preflight-gate is given (rehearsal convenience; NEVER on mainnet).

Gate 8 afterwards: verified count re-read from the chain by this script's own parser,
never from metaboss output.
"""
import base64, hashlib, json, os, subprocess, sys, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
CLAIM_MAP = os.path.join(HERE, "..", "..", "recovered", "remint", "claim_map_all.json")
PREFLIGHT_REPORT = os.path.join(HERE, "..", "..", "verification", "preflight_report.json")
STATE = os.path.join(HERE, "phase8_state.json")
PARENT = "5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc"
KEYPAIR = os.path.expanduser("~/.thugbirdz-keys/hxwz.json")
RPC = os.environ.get("THUGZ_RPC") or sys.exit("THUGZ_RPC not set")
LIMIT = int(sys.argv[sys.argv.index("--limit") + 1]) if "--limit" in sys.argv else None

B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
IDX = {c: i for i, c in enumerate(B58)}
def b58d(x):
    n = 0
    for c in x: n = n * 58 + IDX[c]
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return b"\x00" * (len(x) - len(x.lstrip("1"))) + raw
def b58e(b):
    n = int.from_bytes(b, "big"); out = ""
    while n: n, r = divmod(n, 58); out = B58[r] + out
    return "1" * (len(b) - len(b.lstrip(b"\x00"))) + out
P = 2**255 - 19; D = (-121665 * pow(121666, P - 2, P)) % P
def on_curve(b):
    y = int.from_bytes(b, "little") & ((1 << 255) - 1)
    if y >= P: return False
    y2 = y * y % P; u, v = (y2 - 1) % P, (D * y2 + 1) % P
    x = u * pow(v, 3, P) % P * pow(u * pow(v, 7, P) % P, (P - 5) // 8, P) % P
    return v * x * x % P in (u % P, (-u) % P)
MPL = b58d("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s")
def meta_pda(mint58):
    m = b58d(mint58)
    for bump in range(255, -1, -1):
        h = hashlib.sha256(b"metadata" + MPL + m + bytes([bump]) + MPL + b"ProgramDerivedAddress").digest()
        if not on_curve(h): return b58e(h)
    raise RuntimeError("no bump")

def rpc(method, params):
    req = urllib.request.Request(RPC, json.dumps({"jsonrpc": "2.0", "id": 1, "method": method,
        "params": params}).encode(), {"Content-Type": "application/json"})
    out = json.load(urllib.request.urlopen(req, timeout=60))
    if "error" in out: raise RuntimeError(out["error"])
    return out["result"]

def parse_collection(buf):
    o = 65
    for _ in range(3):
        l = int.from_bytes(buf[o:o+4], "little"); o += 4 + l
    o += 2
    if buf[o] == 1: o += 1 + 4 + int.from_bytes(buf[o+1:o+5], "little") * 34
    else: o += 1
    o += 2
    for _ in range(2):                      # edition_nonce, token_standard Options
        if buf[o] == 1: o += 2
        else: o += 1
    if buf[o] == 0: return None
    return {"verified": buf[o+1] == 1, "key": b58e(buf[o+2:o+34])}

def chain_verified(news):
    pdas = [meta_pda(n) for n in news]
    out = {}
    for i in range(0, len(pdas), 100):
        res = rpc("getMultipleAccounts", [pdas[i:i+100], {"encoding": "base64"}])
        for n, acc in zip(news[i:i+100], res["value"]):
            if acc is None:
                out[n] = "NO-METADATA"
                continue
            c = parse_collection(base64.b64decode(acc["data"][0]))
            out[n] = "verified" if (c and c["verified"] and c["key"] == PARENT) else \
                     ("wrong-parent" if c and c["verified"] else "unverified")
    return out

news = [c["new_mint"] for c in json.load(open(CLAIM_MAP))["claims"]]
print(f"phase8: {len(news)} remints, parent {PARENT}, rpc {RPC.split('?')[0]}")

state = chain_verified(news)
done = [n for n, s in state.items() if s == "verified"]
wrong = [n for n, s in state.items() if s == "wrong-parent"]
todo = [n for n in news if state[n] in ("unverified",)]
missing = [n for n, s in state.items() if s == "NO-METADATA"]
print(f"phase8: chain says {len(done)} verified, {len(todo)} to do, "
      f"{len(wrong)} WRONG PARENT, {len(missing)} missing metadata")
if wrong or missing:
    for n in (wrong + missing)[:5]: print("  PROBLEM:", n, state[n])
    sys.exit(1)
if "--status" in sys.argv: sys.exit(0)

# ---- preflight gate (plan Phase 8: the verify list must come from a just-verified map) ----
if "--skip-preflight-gate" not in sys.argv:
    try:
        rep = json.load(open(PREFLIGHT_REPORT))
        ok = not rep.get("failures")
        age_note = rep.get("run_at") or rep.get("ran_at", "unknown time")
    except Exception:
        ok, age_note = False, "missing"
    if not ok:
        sys.exit(f"preflight report not PASS ({age_note}) — run preflight_check.py first, "
                 f"or --skip-preflight-gate (rehearsal only)")
    print(f"phase8: preflight gate ok ({age_note})")

if LIMIT: todo = todo[:LIMIT]
t0 = time.time()
fails = []
for i, n in enumerate(todo, 1):
    for attempt in range(3):
        r = subprocess.run(["metaboss", "-r", RPC, "collections", "set-and-verify",
                            "-k", KEYPAIR, "-c", PARENT, "--nft-mint", n],
                           capture_output=True, text=True, timeout=120)
        if r.returncode == 0 and "Tx sig" in r.stdout + r.stderr:
            break
        time.sleep(1.5 * (attempt + 1))
    else:
        fails.append((n, (r.stdout + r.stderr).strip()[-200:]))
    if i % 50 == 0 or i == len(todo):
        el = time.time() - t0
        print(f"phase8: {i}/{len(todo)} sent ({len(fails)} failures) {el:.0f}s", flush=True)
        json.dump({"sent": i, "todo": len(todo), "failures": fails, "elapsed": el},
                  open(STATE, "w"), indent=1)

# ---- Gate 8: fresh chain recount, never the tool's own output ----
state = chain_verified(news)
verified = sum(1 for s in state.values() if s == "verified")
print(f"phase8: GATE — fresh chain recount: {verified}/{len(news)} verified "
      f"({time.time()-t0:.0f}s, {len(fails)} tx failures)")
sys.exit(0 if verified == len(news) and not fails else 1)
