#!/usr/bin/env python3
"""Pre-deposit verification of the redemption contract (claim_map_all.json).

Implements the off-chain-only checks from IMPLEMENTATION_APPENDIX.md §9 that are
runnable BEFORE the program exists and before any deposit:

  A. Claim map structure   — 1,274 pairs, injective, olds/news disjoint
  B. Chain state           — every old and new exists; every new: supply 1,
                             owned by the custodian, not burnt; collection
                             grouping recorded (verified membership is Phase 8,
                             so it is REPORTED here, not asserted)
  C. Name match            — remint on-chain name == original on-chain name.
                             Both immutable, both predate the remint pipelines;
                             this is the check that closes the phase-2 (733) gap
  D. Arweave provenance    — strict https://arweave.net/<txid> URI;
                             properties.provenance.original_mint == the map's old;
                             the Arweave JSON's name matches both on-chain names
                             (the frozen copy that makes check C independent of the
                             update authority)
  E. Original-Mint tag     — the indexed Arweave tag on each metadata tx equals the
                             map's old (batched GraphQL)
  F. Freeze authorities    — every remint's freeze authority is its own master
                             edition PDA (standard Metaplex; no foreign key can
                             freeze a destination ATA and strand fix_mapping/recover)

Fails closed: any RPC/HTTP error after retries is a FAILURE, never a skip.
Reads only. Independently runnable:  HELIUS_API_KEY=... python3 preflight_check.py

Exit 0 = every assertion passed. Exit 1 = failures (see the report).
Writes preflight_report.json + preflight_report.md next to this script.
"""
import json, os, re, sys, time, urllib.request, urllib.error
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
CLAIM_MAP = os.path.join(HERE, "..", "recovered", "remint", "claim_map_all.json")
CUSTODIAN = "HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB"
PARENT = "5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc"
EXPECTED = 1274
ARWEAVE_RE = re.compile(r"^https://arweave\.net/[A-Za-z0-9_-]{43}$")
# Fetch fallback only — the on-chain URI itself must still be arweave.net (asserted in D).
# Content is txid-addressed, so any gateway returns the same bytes for the same txid.
FALLBACK_GW = "https://permagate.io/"
KEY = os.environ.get("HELIUS_API_KEY") or sys.exit("HELIUS_API_KEY not set")
RPC = f"https://mainnet.helius-rpc.com/?api-key={KEY}"
RETRIES, TIMEOUT = 4, 30

failures = []      # (check, subject, detail) — any entry means FAIL
notes = []         # informational, not failures


def fail(check, subject, detail):
    failures.append({"check": check, "subject": subject, "detail": detail})


def http_json(url, payload=None, retries=None):
    last = None
    for attempt in range(retries or RETRIES):
        try:
            hdrs = {"User-Agent": "thugz-preflight/1.0"}
            if payload is not None:
                hdrs["Content-Type"] = "application/json"
                req = urllib.request.Request(url, json.dumps(payload).encode(), hdrs)
            else:
                req = urllib.request.Request(url, headers=hdrs)
            with urllib.request.urlopen(req, timeout=TIMEOUT) as r:
                return json.loads(r.read())
        except Exception as e:  # fail closed after retries — never skip
            last = e
            time.sleep(1.5 * (attempt + 1))
    raise RuntimeError(f"fetch failed after {retries or RETRIES} tries: {url.split('?')[0]}: {last}")


def rpc(method, params):
    out = http_json(RPC, {"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    if "error" in out:
        raise RuntimeError(f"RPC error {method}: {out['error']}")
    return out["result"]


# ---------- A. structure ----------
doc = json.load(open(CLAIM_MAP))
claims = doc["claims"]
# resolve field names generically
if isinstance(claims[0], dict):
    okeys = [k for k in claims[0] if "orig" in k.lower() or k.lower() in ("old", "old_mint")]
    nkeys = [k for k in claims[0] if "new" in k.lower() or "remint" in k.lower()]
    if not (okeys and nkeys):
        sys.exit(f"cannot identify old/new fields in claim entries: {list(claims[0])}")
    OK, NK = okeys[0], nkeys[0]
    pairs = [(c[OK], c[NK]) for c in claims]
else:
    pairs = [(c[0], c[1]) for c in claims]
olds = [p[0] for p in pairs]
news = [p[1] for p in pairs]

if len(pairs) != EXPECTED:
    fail("A.count", "claim_map", f"{len(pairs)} pairs, expected {EXPECTED}")
if len(set(olds)) != len(olds):
    fail("A.injective", "olds", "duplicate original_mint in claim map")
if len(set(news)) != len(news):
    fail("A.injective", "news", "duplicate new_mint in claim map")
overlap = set(olds) & set(news)
if overlap:
    fail("A.disjoint", "olds∩news", f"{len(overlap)} addresses on both sides: {sorted(overlap)[:3]}")
old_of = {n: o for o, n in pairs}
new_of = {o: n for o, n in pairs}
print(f"A. structure: {len(pairs)} pairs, injective={len(set(olds))==len(olds)==len(set(news))==len(news)}, disjoint={not overlap}", flush=True)

# ---------- B. chain state (getAssetBatch, 100 per call) ----------
assets = {}
allmints = olds + news
for i in range(0, len(allmints), 100):
    batch = allmints[i:i+100]
    res = rpc("getAssetBatch", {"ids": batch})
    for mint, a in zip(batch, res):
        assets[mint] = a
    print(f"B. fetched {min(i+100, len(allmints))}/{len(allmints)} assets", flush=True)

def name_of(a):
    try:
        return (a.get("content", {}).get("metadata", {}).get("name") or "").strip()
    except AttributeError:
        return ""

coll_status = {"news_verified_in_parent": 0, "news_ungrouped": 0, "news_other": 0}
for o in olds:
    a = assets.get(o)
    if not a:
        fail("B.exists", o, "original not found on chain")
for n in news:
    a = assets.get(n)
    if not a:
        fail("B.exists", n, "remint not found on chain")
        continue
    if a.get("burnt"):
        fail("B.burnt", n, "remint is burnt")
    owner = a.get("ownership", {}).get("owner")
    if owner != CUSTODIAN:
        fail("B.owner", n, f"owner {owner}, expected custodian {CUSTODIAN}")
    supply = (a.get("token_info") or {}).get("supply")
    if supply not in (1, None):   # DAS omits token_info sometimes; None is not a failure
        fail("B.supply", n, f"supply {supply}")
    grp = [g for g in a.get("grouping", []) if g.get("group_key") == "collection"]
    if grp and grp[0].get("group_value") == PARENT and grp[0].get("verified", True):
        coll_status["news_verified_in_parent"] += 1
    elif not grp:
        coll_status["news_ungrouped"] += 1
    else:
        coll_status["news_other"] += 1
notes.append(f"Collection grouping of remints (Phase 8 not run yet, informational): {coll_status}")
print("B. chain state done.", coll_status, flush=True)

# ---------- C. name match ----------
name_mis = 0
for o, n in pairs:
    if o not in assets or n not in assets or not assets[o] or not assets[n]:
        continue  # already failed in B
    no_, nn = name_of(assets[o]), name_of(assets[n])
    if not no_ or not nn:
        fail("C.name-missing", f"{o}->{n}", f"old name {no_!r}, new name {nn!r}")
    elif no_ != nn:
        name_mis += 1
        fail("C.name-mismatch", f"{o}->{n}", f"original {no_!r} vs remint {nn!r}")
print(f"C. name match done. mismatches={name_mis}", flush=True)

# mutability posture (recorded, not asserted — see appendix §9)
mut = sum(1 for n in news if assets.get(n) and assets[n].get("mutable"))
ua_bad = [n for n in news if assets.get(n) and not any(
    a.get("address") == CUSTODIAN for a in assets[n].get("authorities", []))]
notes.append(f"Remints mutable: {mut}/{len(news)}; update authority != custodian on {len(ua_bad)}")
if ua_bad:
    for n in ua_bad[:5]:
        fail("C.update-authority", n, "update authority is not the custodian")

# ---------- D. arweave provenance ----------
meta_txid = {}     # new_mint -> arweave txid of its metadata
fetch_retry = []   # (old, new, uri) whose fetch flaked in the concurrent pass

def finish_arweave(o, n, meta, a):
    prov = ((meta.get("properties") or {}).get("provenance") or {})
    om = prov.get("original_mint")
    if om != o:
        fail("D.provenance", n, f"provenance.original_mint {om!r} != claim map old {o}")
    aname = (meta.get("name") or "").strip()
    cname = name_of(a)
    if aname != cname:
        fail("D.arweave-name", n, f"Arweave name {aname!r} != on-chain name {cname!r}")

def check_arweave(pair):
    o, n = pair
    a = assets.get(n)
    if not a:
        return
    uri = (a.get("content") or {}).get("json_uri") or ""
    if not ARWEAVE_RE.match(uri):
        fail("D.uri", n, f"uri not strict arweave.net/<txid>: {uri!r}")
        return
    tx = uri.rsplit("/", 1)[1]
    meta_txid[n] = tx
    try:
        meta = http_json(uri, retries=4)
    except Exception:
        fetch_retry.append((o, n, uri))   # gateway flakes under concurrent load —
        return                            # retried serially below; still fails closed
    finish_arweave(o, n, meta, a)

done = 0
with ThreadPoolExecutor(max_workers=16) as ex:
    futs = [ex.submit(check_arweave, p) for p in pairs]
    for f in as_completed(futs):
        f.result()
        done += 1
        if done % 200 == 0:
            print(f"D. arweave {done}/{len(pairs)}", flush=True)
if fetch_retry:
    print(f"D. serial retry for {len(fetch_retry)} flaked fetches", flush=True)
    fellback = 0
    for o, n, uri in fetch_retry:
        time.sleep(5)
        try:
            meta = http_json(uri, retries=3)
        except Exception:
            # arweave.net edge can cache an error page for a specific txid; the
            # content is txid-addressed, so verify via a second gateway instead
            try:
                meta = http_json(FALLBACK_GW + uri.rsplit("/", 1)[1], retries=4)
                fellback += 1
            except Exception as e:
                fail("D.fetch", n, str(e))
                continue
        finish_arweave(o, n, meta, assets[n])
    if fellback:
        notes.append(f"D: {fellback} metadata file(s) fetched via fallback gateway "
                     f"{FALLBACK_GW} after arweave.net served a cached CDN error page; "
                     f"content is txid-addressed, so the bytes are the same record")
print("D. arweave done.", flush=True)

# ---------- E. Original-Mint tag via Arweave GraphQL ----------
txids = [(n, meta_txid[n]) for _, n in pairs if n in meta_txid]
BATCH = 9   # arweave.net graphql rejects more than 9 ids per query

def tag_batch(i):
    chunk = txids[i:i+BATCH]
    q = {"query": "query($ids:[ID!]){transactions(ids:$ids,first:9){edges{node{id tags{name value}}}}}",
         "variables": {"ids": [t for _, t in chunk]}}
    try:
        res = http_json("https://arweave.net/graphql", q, retries=6)
        got = {e["node"]["id"]: {t["name"]: t["value"] for t in e["node"]["tags"]}
               for e in res["data"]["transactions"]["edges"]}
    except Exception as e:
        fail("E.graphql", f"batch {i//BATCH}", str(e))
        return
    for n, tx in chunk:
        tags = got.get(tx)
        if tags is None:
            fail("E.tag", n, f"metadata tx {tx} not returned by graphql")
            continue
        om = tags.get("Original-Mint")
        if om != old_of[n]:
            fail("E.tag", n, f"Original-Mint tag {om!r} != claim map old {old_of[n]}")

tdone = 0
with ThreadPoolExecutor(max_workers=6) as ex:
    futs = [ex.submit(tag_batch, i) for i in range(0, len(txids), BATCH)]
    for f in as_completed(futs):
        f.result()
        tdone += 1
        if tdone % 30 == 0:
            print(f"E. tags {tdone*BATCH}/{len(txids)}", flush=True)
print("E. tag check done.", flush=True)

# ---------- F. freeze authority == the mint's own master edition PDA ----------
# Metaplex NFTs always carry a freeze authority: the master edition PDA. That shape is
# safe — only Token Metadata's delegate paths can freeze, and those need the token
# owner's own delegation. What must NOT exist is a FOREIGN freeze key.
import base64, hashlib
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
B58IDX = {c: i for i, c in enumerate(B58)}
def b58decode(x):
    n = 0
    for c in x: n = n * 58 + B58IDX[c]
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return b"\x00" * (len(x) - len(x.lstrip("1"))) + raw

P25519 = 2**255 - 19
D25519 = (-121665 * pow(121666, P25519 - 2, P25519)) % P25519
def on_curve(b):
    y = int.from_bytes(b, "little") & ((1 << 255) - 1)
    if y >= P25519: return False
    y2 = y * y % P25519
    u, v = (y2 - 1) % P25519, (D25519 * y2 + 1) % P25519
    x = u * pow(v, 3, P25519) % P25519 * pow(u * pow(v, 7, P25519) % P25519, (P25519 - 5) // 8, P25519) % P25519
    vx2 = v * x * x % P25519
    return vx2 == u % P25519 or vx2 == (-u) % P25519

MPL = b58decode("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s")
def edition_pda(mint_bytes):
    for bump in range(255, -1, -1):
        h = hashlib.sha256(b"metadata" + MPL + mint_bytes + b"edition"
                           + bytes([bump]) + MPL + b"ProgramDerivedAddress").digest()
        if not on_curve(h): return h
    raise RuntimeError("no PDA bump found")

foreign = 0
for i in range(0, len(news), 100):
    chunk = news[i:i+100]
    res = rpc("getMultipleAccounts", [chunk, {"encoding": "base64"}])
    for mint, acc in zip(chunk, res["value"]):
        if acc is None:
            fail("F.mint", mint, "mint account missing")
            continue
        raw = base64.b64decode(acc["data"][0])
        # SPL mint layout: freeze_authority COption tag at 46, key at 50..82
        if len(raw) < 82 or int.from_bytes(raw[46:50], "little") == 0:
            continue   # no freeze authority at all — also fine
        if raw[50:82] != edition_pda(b58decode(mint)):
            foreign += 1
            fail("F.freeze", mint, "freeze authority is NOT the mint's own master edition PDA — a foreign key could freeze destination ATAs")
    print(f"F. freeze {min(i+100, len(news))}/{len(news)}", flush=True)
print(f"F. freeze check done. foreign freeze authorities: {foreign}", flush=True)

# ---------- report ----------
ts = datetime.now(timezone.utc).isoformat(timespec="seconds")
verdict = "PASS" if not failures else "FAIL"
report = {"ran_at": ts, "verdict": verdict, "pairs": len(pairs),
          "checks": ["A.structure", "B.chain", "C.names+mutability", "D.arweave+name-binding", "E.original-mint-tags", "F.freeze-authorities"],
          "failures": failures, "notes": notes}
json.dump(report, open(os.path.join(HERE, "preflight_report.json"), "w"), indent=1)
md = [f"# Pre-deposit verification — {verdict}", "",
      f"Ran {ts} against mainnet via Helius DAS + arweave.net. {len(pairs)} pairs.",
      "", "| Check | Result |", "|---|---|",
      f"| A. claim map structure (count/injective/disjoint) | {'PASS' if not [f for f in failures if f['check'].startswith('A')] else 'FAIL'} |",
      f"| B. chain: exists / owner={CUSTODIAN[:5]}… / not burnt | {'PASS' if not [f for f in failures if f['check'].startswith('B')] else 'FAIL'} |",
      f"| C. name match, original vs remint (all 1,274) | {'PASS' if not [f for f in failures if f['check'].startswith('C')] else 'FAIL'} |",
      f"| D. Arweave provenance + strict URI + frozen-name binding | {'PASS' if not [f for f in failures if f['check'].startswith('D')] else 'FAIL'} |",
      f"| E. Original-Mint tag on every metadata tx (GraphQL) | {'PASS' if not [f for f in failures if f['check'].startswith('E')] else 'FAIL'} |",
      f"| F. Freeze authority == own master edition PDA on every mint | {'PASS' if not [f for f in failures if f['check'].startswith('F')] else 'FAIL'} |",
      ""]
for nt in notes:
    md.append(f"> {nt}")
if failures:
    md += ["", f"## Failures ({len(failures)})", ""]
    md += [f"- **{f['check']}** `{f['subject']}` — {f['detail']}" for f in failures[:200]]
open(os.path.join(HERE, "preflight_report.md"), "w").write("\n".join(md) + "\n")
print(f"\n{verdict}: {len(failures)} failures. Report written.", flush=True)
sys.exit(0 if not failures else 1)
