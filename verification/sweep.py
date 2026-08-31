#!/usr/bin/env python3
"""THE SWEEP — pre-seal verification of the deposited set (IMPLEMENTATION_APPENDIX §9).

The most safety-critical off-chain code in the project. Runs immediately before `seal`,
which is irreversible. Any failure is a HARD STOP: exit 1, do not seal, do not retry-later.

Sections (appendix §9 pseudocode + spec §6b list; section 11 added on external review —
it EXCEEDS appendix §9 deliberately, using art_sha256/image_tx already in the claim map):

  1. MAP        claim_map_all.json structure — 1,274 pairs, canonical pubkeys, injective,
                olds/news disjoint
  2. MAPPINGS   every old_mint has a Mapping PDA on the target chain (owner = program,
                Anchor discriminator, exact 83 bytes); decoded new_mint equals the map's;
                claimed == false; recovered == false
  3. INJECTIVE  every decoded new_mint appears exactly once ON CHAIN
  4. VAULT      the vault ATA for every new_mint (owner = token program, 165+ bytes)
                holds exactly 1 of that mint
  5. POOL       full pool decode (93 bytes, discriminator): deposited == expected == 1274;
                sealed MUST be false (this is a PRE-seal sweep); paused/unlock_ts recorded
  6. MEMBERSHIP 6a (authoritative): every new_mint's METADATA ACCOUNT on the target
                chain carries collection == 5Kwhy… with verified == true — parsed from
                raw bytes, never from an indexer. 6b (enumeration): mainnet DAS lists the
                collection's members; every member must be a claim-map remint, the
                parent, or in the validated 2021-originals reference list (3,318
                canonical mints, covers all olds, disjoint from news). A missing
                `verified` field anywhere is a FAILURE, never a default.
                Passes only after Phase 8.
                Doc note 2026-08-27: appendix §9's "members minus parent == new_mints
                exactly" is wrong (the collection also holds the migrated originals);
                spec §6b's filtered form is implemented.
  7. METADATA   every remint's on-chain json_uri is strictly https://arweave.net/<txid>;
                remint on-chain name == original on-chain name; mutability + update
                authority recorded per pair
  8. ARWEAVE    the frozen Arweave JSON: provenance.original_mint == the map's old_mint;
                json name == both on-chain names
  9. TAGS       EXACTLY ONE Original-Mint tag on each metadata tx, equal to the map's
                old_mint (arweave GraphQL, 9 ids per query cap)
 10. FREEZE     every remint mint account (owner = token program, exact 82 bytes) has
                freeze authority == its own master edition PDA; truncated data is a FAILURE
 11. IMAGE      every remint's image bytes (claim map image_tx) are retrievable from a
                gateway and sha256 == the claim map's art_sha256

Fail-closed: any RPC/HTTP error, short batch, malformed account, or missing field after
retries is a FAILURE, never a skip. Thread workers return findings; nothing shared is
mutated off the main thread.

  THUGZ_SWEEP_RPC=http://127.0.0.1:8999 HELIUS_API_KEY=... python3 sweep.py

THUGZ_SWEEP_RPC is the chain holding pool/mappings/vault (Surfpool fork in Phase 4;
mainnet for the real run). DAS, Arweave, and membership always read real mainnet.
Optional: THUGZ_SIGN_KEYPAIR=<path> signs the report sha256 via
`solana sign-offchain-message` and embeds the signature.

Writes sweep_report.json + sweep_report.md next to this script. Exit 0 = sealable.
"""
import base64, hashlib, json, os, re, struct, subprocess, sys, time, urllib.request
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
CLAIM_MAP = os.path.join(HERE, "..", "recovered", "remint", "claim_map_all.json")
ORIGINALS_REF = os.path.join(HERE, "thugbird_mints_3318.json")   # vendored (review 1+3)
ORIGINALS_REF_SHA256 = "e51f570917fab1f45aadd7107b822ddd5f9e0907a30a81f9dbf6f7e3c338f86c"
ORIGINALS_REF_LEN = 3318
PROGRAM_ID = "CaWcaw5YfBYQZ1jraTPqiLx2CJc5CwBL8J4Z1DN5neVs"
PARENT = "5KwhyPToqeGQYmRQjnx3EDSRMnaiCJDMEH3aGT8R3HNc"
CUSTODIAN = "HxwZCEMgck9v24iP9y2YcBttBkM7GjX77oBiNmQYiiUB"
TOKEN_PROGRAM = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA"
EXPECTED = 1274
ARWEAVE_RE = re.compile(r"^https://arweave\.net/[A-Za-z0-9_-]{43}$")
TXID_RE = re.compile(r"^[A-Za-z0-9_-]{43}$")
FALLBACK_GW = "https://permagate.io/"   # fetch fallback only; content is txid-addressed
KEY = os.environ.get("HELIUS_API_KEY") or sys.exit("HELIUS_API_KEY not set")
MAINNET = f"https://mainnet.helius-rpc.com/?api-key={KEY}"
TARGET = os.environ.get("THUGZ_SWEEP_RPC") or sys.exit(
    "THUGZ_SWEEP_RPC not set (the chain holding pool/mappings/vault — fork or mainnet)")
RETRIES, TIMEOUT = 4, 30

failures, notes = [], []

# Immutable-content cache: an Arweave txid's bytes can never change, so a sha256 match
# proven once holds forever. Image entries are keyed by txid -> verified sha; metadata
# entries cache the frozen JSON fields. Anything absent or mismatched is fetched
# fail-closed exactly as before.
CACHE_PATH = os.path.join(HERE, "sweep_immutable_cache.json")
NO_CACHE = "--no-cache" in sys.argv    # MANDATORY for the mainnet sealing run (review 3):
                                       # a sealing run must not trust an earlier local run.
try:
    _CACHE = {"images": {}, "meta": {}} if NO_CACHE else json.load(open(CACHE_PATH))
except Exception:
    _CACHE = {"images": {}, "meta": {}}
def save_cache():
    json.dump(_CACHE, open(CACHE_PATH, "w"))

def fail(check, subject, detail):
    failures.append({"check": check, "subject": subject, "detail": detail})
    print(f"  FAIL {check} {subject}: {detail}", flush=True)

def http_json(url, payload=None, retries=None):
    last = None
    for attempt in range(retries or RETRIES):
        try:
            hdrs = {"User-Agent": "thugz-sweep/1.0"}
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

def http_bytes(url, retries=None):
    last = None
    for attempt in range(retries or RETRIES):
        try:
            req = urllib.request.Request(url, headers={"User-Agent": "thugz-sweep/1.0"})
            with urllib.request.urlopen(req, timeout=60) as r:
                data = r.read()
            if not data:
                raise RuntimeError("empty body")
            return data
        except Exception as e:
            last = e
            time.sleep(1.5 * (attempt + 1))
    raise RuntimeError(f"bytes fetch failed: {last}")

def rpc(endpoint, method, params):
    out = http_json(endpoint, {"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
    if "error" in out:
        raise RuntimeError(f"RPC error {method}: {out['error']}")
    return out["result"]

# ---------- base58 / PDA machinery ----------
B58 = "123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz"
B58IDX = {c: i for i, c in enumerate(B58)}
def b58decode(x):
    n = 0
    for c in x: n = n * 58 + B58IDX[c]
    raw = n.to_bytes((n.bit_length() + 7) // 8, "big")
    return b"\x00" * (len(x) - len(x.lstrip("1"))) + raw
def b58encode(b):
    n = int.from_bytes(b, "big")
    out = ""
    while n:
        n, r = divmod(n, 58)
        out = B58[r] + out
    return "1" * (len(b) - len(b.lstrip(b"\x00"))) + out

def decode_pubkey(x):
    """Canonical base58 pubkey -> 32 bytes. Anything else raises."""
    if not isinstance(x, str) or not (32 <= len(x) <= 44) or any(c not in B58IDX for c in x):
        raise ValueError(f"not a base58 pubkey: {x!r}")
    b = b58decode(x)
    if len(b) != 32 or b58encode(b) != x:
        raise ValueError(f"non-canonical pubkey: {x!r}")
    return b

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

def find_pda(seeds, program):
    for s in seeds:
        if len(s) > 32: raise ValueError("seed too long")
    for bump in range(255, -1, -1):
        h = hashlib.sha256(b"".join(seeds) + bytes([bump]) + program + b"ProgramDerivedAddress").digest()
        if not on_curve(h):
            return h
    raise RuntimeError("no PDA bump")

PROG = decode_pubkey(PROGRAM_ID)
MPL = decode_pubkey("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s")
TOKEN = decode_pubkey(TOKEN_PROGRAM)
ATA_PROG = decode_pubkey("ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL")
POOL = find_pda([b"pool"], PROG)
VAULT = find_pda([b"vault"], PROG)
def map_pda(old_b):    return find_pda([b"map", POOL, old_b], PROG)
def ata(owner_b, mint_b): return find_pda([owner_b, TOKEN, mint_b], ATA_PROG)
def edition_pda(mint_b):  return find_pda([b"metadata", MPL, mint_b, b"edition"], MPL)
MAPPING_DISC = hashlib.sha256(b"account:Mapping").digest()[:8]
POOL_DISC = hashlib.sha256(b"account:Pool").digest()[:8]

# PDA machinery self-test against a known-good vector: the deployed pool address must
# round-trip. (Set after first successful mainnet/fork read; guards a silent PDA bug.)
_KNOWN_POOL = "7gDE9pxPVV7Cfz5hGfvXUs2x6T7xNL7rto2zDJyqaDoP"
if b58encode(POOL) != _KNOWN_POOL:
    sys.exit(f"PDA self-test failed: derived pool {b58encode(POOL)} != known {_KNOWN_POOL}")

def get_multiple(endpoint, addrs):
    """Batched getMultipleAccounts with STRICT cardinality — a short batch is fatal."""
    out = []
    for i in range(0, len(addrs), 100):
        chunk = addrs[i:i+100]
        res = rpc(endpoint, "getMultipleAccounts", [chunk, {"encoding": "base64"}])
        vals = res.get("value")
        if not isinstance(vals, list) or len(vals) != len(chunk):
            raise RuntimeError(f"getMultipleAccounts returned {len(vals) if isinstance(vals, list) else type(vals)} "
                               f"for a {len(chunk)}-address batch — refusing to continue")
        out += vals
    return out

def acct_raw(acc, subject, check, want_owner=None, want_len=None, want_disc=None, min_len=None):
    """Validate an account envelope; return raw bytes or None (failure already recorded)."""
    if acc is None:
        fail(check, subject, "account missing")
        return None
    try:
        raw = base64.b64decode(acc["data"][0])
    except Exception as e:
        fail(check, subject, f"undecodable account data: {e}")
        return None
    if want_owner and acc.get("owner") != want_owner:
        fail(check, subject, f"owner {acc.get('owner')}, expected {want_owner}")
        return None
    if want_len is not None and len(raw) != want_len:
        fail(check, subject, f"{len(raw)} bytes, expected exactly {want_len}")
        return None
    if min_len is not None and len(raw) < min_len:
        fail(check, subject, f"{len(raw)} bytes, expected at least {min_len}")
        return None
    if want_disc is not None and raw[:8] != want_disc:
        fail(check, subject, f"wrong account discriminator {raw[:8].hex()}")
        return None
    return raw

t0 = time.time()
print(f"SWEEP target={TARGET.split('?')[0]}  assets/membership=mainnet  {datetime.now(timezone.utc).isoformat()}", flush=True)

# ---------- 1. MAP ----------
doc = json.load(open(CLAIM_MAP))
claim_sha = hashlib.sha256(open(CLAIM_MAP, "rb").read()).hexdigest()
claims = doc["claims"]
pairs, image_of, art_sha_of = [], {}, {}
for c in claims:
    try:
        decode_pubkey(c["old_mint"]); decode_pubkey(c["new_mint"])
    except ValueError as e:
        fail("1.pubkey", c.get("name", "?"), str(e)); continue
    pairs.append((c["old_mint"], c["new_mint"]))
    if not TXID_RE.match(c.get("image_tx", "")):
        fail("1.image-tx", c["name"], f"bad image_tx {c.get('image_tx')!r}")
    else:
        image_of[c["new_mint"]] = c["image_tx"]
    if not re.match(r"^[0-9a-f]{64}$", c.get("art_sha256", "")):
        fail("1.art-sha", c["name"], f"bad art_sha256 {c.get('art_sha256')!r}")
    else:
        art_sha_of[c["new_mint"]] = c["art_sha256"]
olds, news = [p[0] for p in pairs], [p[1] for p in pairs]
old_of = {n: o for o, n in pairs}
if len(pairs) != EXPECTED: fail("1.count", "claim_map", f"{len(pairs)} valid pairs, expected {EXPECTED}")
if len(set(olds)) != len(olds): fail("1.injective", "olds", "duplicate old_mint")
if len(set(news)) != len(news): fail("1.injective", "news", "duplicate new_mint")
if set(olds) & set(news): fail("1.disjoint", "olds∩news", "addresses on both sides")
print(f"1. MAP: {len(pairs)} pairs, sha256 {claim_sha[:16]}…", flush=True)

rows = {n: {"old": o, "new": n} for o, n in pairs}   # per-pair report rows

# ---------- 2. MAPPINGS + 3. INJECTIVE (target chain) ----------
map_addrs = [b58encode(map_pda(decode_pubkey(o))) for o in olds]
accs = get_multiple(TARGET, map_addrs)
onchain_new = {}
for (o, n), addr, acc in zip(pairs, map_addrs, accs):
    rows[n]["mapping_pda"] = addr
    raw = acct_raw(acc, o, "2.mapping", want_owner=PROGRAM_ID, want_len=83, want_disc=MAPPING_DISC)
    if raw is None: continue
    dec_new = b58encode(raw[8:40])
    if dec_new != n:
        fail("2.new-mint", o, f"on-chain new_mint {dec_new} != claim map {n}")
    if raw[40] != 0: fail("2.claimed", o, "mapping already claimed pre-seal")
    if raw[81] != 0: fail("2.recovered", o, "mapping already recovered pre-seal")
    onchain_new.setdefault(dec_new, []).append(o)
dupes = {n: os_ for n, os_ in onchain_new.items() if len(os_) > 1}
for n, os_ in dupes.items():
    fail("3.injective", n, f"remint pointed at by {len(os_)} originals: {os_}")
print(f"2/3. MAPPINGS: {len(onchain_new)} distinct on-chain new_mints, dupes={len(dupes)}", flush=True)

# ---------- 4. VAULT (target chain) ----------
vault_atas = [b58encode(ata(VAULT, decode_pubkey(n))) for n in news]
accs = get_multiple(TARGET, vault_atas)
for n, addr, acc in zip(news, vault_atas, accs):
    rows[n]["vault_ata"] = addr
    raw = acct_raw(acc, n, "4.vault", want_owner=TOKEN_PROGRAM, min_len=165)
    if raw is None: continue
    if b58encode(raw[0:32]) != n:
        fail("4.vault", n, "vault ATA mint mismatch")
    if raw[32:64] != VAULT:
        fail("4.vault-owner", n, "token account owner field is not the vault PDA")
    amount = struct.unpack_from("<Q", raw, 64)[0]
    if amount != 1:
        fail("4.vault", n, f"vault ATA holds {amount}, expected 1")
print("4. VAULT done.", flush=True)

# ---------- 5. POOL (target chain; full decode, PRE-seal assert) ----------
pool_addr = b58encode(POOL)
acc = rpc(TARGET, "getAccountInfo", [pool_addr, {"encoding": "base64"}])["value"]
pool_state = {}
raw = acct_raw(acc, pool_addr, "5.pool", want_owner=PROGRAM_ID, want_len=93, want_disc=POOL_DISC)
if raw is not None:
    expected, deposited, swapped, recovered = struct.unpack_from("<HHHH", raw, 72)
    sealed, paused = raw[80] == 1, raw[81] == 1
    unlock_ts = struct.unpack_from("<q", raw, 82)[0]
    pool_state = {"admin": b58encode(raw[8:40]), "collection": b58encode(raw[40:72]),
                  "expected": expected, "deposited": deposited, "swapped": swapped,
                  "recovered": recovered, "sealed": sealed, "paused": paused,
                  "unlock_ts": unlock_ts}
    if expected != EXPECTED: fail("5.expected", pool_addr, f"pool.expected {expected}")
    if deposited != EXPECTED: fail("5.deposited", pool_addr, f"pool.deposited {deposited}, expected {EXPECTED}")
    if sealed: fail("5.sealed", pool_addr, "pool is ALREADY SEALED — this is a pre-seal sweep")
    if pool_state["collection"] != PARENT:
        fail("5.collection", pool_addr, f"pool.collection {pool_state['collection']} != {PARENT}")
print(f"5. POOL {pool_state}", flush=True)

# ---------- 6. MEMBERSHIP (mainnet DAS; explicit verified only) ----------
try:
    _ref_raw = open(ORIGINALS_REF, "rb").read()
except FileNotFoundError:
    sys.exit(f"originals reference list missing: {ORIGINALS_REF} — refusing to run")
if hashlib.sha256(_ref_raw).hexdigest() != ORIGINALS_REF_SHA256:
    sys.exit("originals reference list does not match its pinned sha256 — refusing to run")
originals_ref = json.loads(_ref_raw)
ref_problems = []
if len(originals_ref) != ORIGINALS_REF_LEN:
    ref_problems.append(f"reference list has {len(originals_ref)} entries, expected {ORIGINALS_REF_LEN}")
try:
    ref_set = {m for m in originals_ref if decode_pubkey(m)}
except ValueError as e:
    ref_problems.append(f"non-canonical mint in reference list: {e}")
    ref_set = set()
if not set(olds) <= ref_set:
    ref_problems.append(f"{len(set(olds) - ref_set)} claim-map olds missing from reference list")
if ref_set & set(news):
    ref_problems.append("reference list overlaps claim-map news")
for p in ref_problems:
    fail("6.reference", os.path.basename(ORIGINALS_REF), p)

# 6a — authoritative: parse each remint's metadata account on the TARGET chain
def meta_pda_addr(mint58): return b58encode(find_pda([b"metadata", MPL, decode_pubkey(mint58)], MPL))
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
    return {"verified": buf[o+1] == 1, "key": b58encode(buf[o+2:o+34])}
meta_addrs = [meta_pda_addr(n) for n in news]
accs = get_multiple(TARGET, meta_addrs)
unverified_a = []
for n, acc in zip(news, accs):
    raw = acct_raw(acc, n, "6a.metadata", want_owner=b58encode(MPL))
    if raw is None: continue
    try:
        c = parse_collection(raw)
    except Exception as e:
        fail("6a.parse", n, f"metadata parse error: {e}"); continue
    if not (c and c["verified"] is True and c["key"] == PARENT):
        unverified_a.append(n)
for n in unverified_a[:10]:
    fail("6a.membership", n, "remint metadata does not carry a VERIFIED collection == parent "
         "(expected to fail before Phase 8 has run)")
if len(unverified_a) > 10:
    fail("6a.membership", "…", f"{len(unverified_a) - 10} more remints unverified")

# 6b — enumeration via mainnet DAS: no foreign members may exist
members, page = {}, 1
while True:
    r = rpc(MAINNET, "getAssetsByGroup",
            {"groupKey": "collection", "groupValue": PARENT, "page": page, "limit": 1000})
    for it in r["items"]:
        v = False
        for g in it.get("grouping", []):
            if g.get("group_key") == "collection" and g.get("group_value") == PARENT \
               and g.get("verified") is True:      # missing field is NOT verified
                v = True
        members[it["id"]] = v
    if len(r["items"]) < 1000: break
    page += 1
news_set = set(news)
foreign_members = [m for m in members if m not in news_set and m != PARENT and m not in ref_set]
for m in foreign_members[:10]:
    fail("6b.foreign-member", m, "collection member that is neither a claim-map remint, "
         "the parent, nor in the validated 2021-originals reference list")
if len(foreign_members) > 10:
    fail("6b.foreign-member", "…", f"{len(foreign_members) - 10} more foreign members")
das_verified_news = sum(1 for n in news if members.get(n) is True)
if "mainnet" in TARGET and (not members or das_verified_news != EXPECTED):
    fail("6b.das", "enumeration", f"target is mainnet but DAS lists {len(members)} members "
         f"with {das_verified_news}/{EXPECTED} remints verified — indexer outage or "
         f"membership gap; the foreign-member check cannot be trusted")
notes.append(f"Membership: 6a target-chain verified {len(news) - len(unverified_a)}/{EXPECTED}; "
             f"6b mainnet DAS lists {len(members)} members, {das_verified_news} of them "
             f"claim-map remints, {len(foreign_members)} foreign. On mainnet 6a and the DAS "
             f"view must agree; a lagging indexer shows up here as a note, not a pass.")
print(f"6. MEMBERSHIP: 6a unverified={len(unverified_a)}; 6b members={len(members)}, "
      f"foreign={len(foreign_members)}", flush=True)

# ---------- 7. METADATA (mainnet DAS getAssetBatch) ----------
assets = {}
allmints = olds + news
for i in range(0, len(allmints), 100):
    batch = allmints[i:i+100]
    res = rpc(MAINNET, "getAssetBatch", {"ids": batch})
    if not isinstance(res, list) or len(res) != len(batch):
        raise RuntimeError("getAssetBatch cardinality mismatch — refusing to continue")
    for mint, a in zip(batch, res):
        assets[mint] = a
def name_of(a):
    try: return (a.get("content", {}).get("metadata", {}).get("name") or "").strip()
    except AttributeError: return ""
mutability = {"mutable": 0, "immutable": 0}
meta_txid = {}
for o, n in pairs:
    ao, an = assets.get(o), assets.get(n)
    if not ao: fail("7.exists", o, "original not on chain"); continue
    if not an: fail("7.exists", n, "remint not on chain"); continue
    uri = (an.get("content") or {}).get("json_uri") or ""
    if not ARWEAVE_RE.match(uri):
        fail("7.uri", n, f"uri not strict arweave.net/<txid>: {uri!r}")
    else:
        meta_txid[n] = uri.rsplit("/", 1)[1]
        rows[n]["meta_tx"] = meta_txid[n]
    no_, nn = name_of(ao), name_of(an)
    rows[n]["name"] = nn
    if not no_ or not nn:
        fail("7.name-missing", f"{o}->{n}", f"old {no_!r} new {nn!r}")
    elif no_ != nn:
        fail("7.name-mismatch", f"{o}->{n}", f"original {no_!r} vs remint {nn!r}")
    rows[n]["mutable"] = bool(an.get("mutable"))
    mutability["mutable" if an.get("mutable") else "immutable"] += 1
    ua = [a.get("address") for a in an.get("authorities", [])]
    rows[n]["update_authority"] = ua
    if CUSTODIAN not in ua:
        fail("7.update-authority", n, "update authority is not the custodian")
notes.append(f"Remint mutability recorded: {mutability}")
print(f"7. METADATA done. {mutability}", flush=True)

# ---------- 8. ARWEAVE (workers return findings; merged on main thread) ----------
def check_arweave(pair):
    """Returns (n, findings:list, retry:bool)."""
    o, n = pair
    if n not in meta_txid: return (n, [], False)
    tx = meta_txid[n]
    cached = _CACHE["meta"].get(tx)
    if cached is not None:
        return (n, _arweave_findings(o, n, cached), False)
    try:
        meta = http_json(f"https://arweave.net/{tx}", retries=4)
    except Exception:
        return (n, [], True)
    _CACHE["meta"][tx] = {"properties": meta.get("properties"), "name": meta.get("name")}
    return (n, _arweave_findings(o, n, meta), False)

def _arweave_findings(o, n, meta):
    out = []
    prov = ((meta.get("properties") or {}).get("provenance") or {})
    if prov.get("original_mint") != o:
        out.append(("8.provenance", n, f"provenance.original_mint {prov.get('original_mint')!r} != {o}"))
    aname = (meta.get("name") or "").strip()
    if aname != name_of(assets[n]):
        out.append(("8.arweave-name", n, f"Arweave name {aname!r} != on-chain {name_of(assets[n])!r}"))
    return out

retry_list, done = [], 0
with ThreadPoolExecutor(max_workers=16) as ex:
    for f in as_completed([ex.submit(check_arweave, p) for p in pairs]):
        n, findings, retry = f.result()
        for c, s, d in findings: fail(c, s, d)
        if retry: retry_list.append(n)
        done += 1
        if done % 300 == 0: print(f"8. arweave {done}/{len(pairs)}", flush=True)
fellback = 0
for n in retry_list:
    o, uri = old_of[n], f"https://arweave.net/{meta_txid[n]}"
    time.sleep(4)
    try:
        meta = http_json(uri, retries=3)
    except Exception:
        try:
            meta = http_json(FALLBACK_GW + meta_txid[n], retries=4); fellback += 1
        except Exception as e:
            fail("8.fetch", n, str(e)); continue
    _CACHE["meta"][meta_txid[n]] = {"properties": meta.get("properties"), "name": meta.get("name")}
    for c, s, d in _arweave_findings(o, n, meta): fail(c, s, d)
if fellback: notes.append(f"8: {fellback} fetched via fallback gateway (txid-addressed, same bytes)")
print("8. ARWEAVE done.", flush=True)

# ---------- 9. TAGS (exactly one Original-Mint per metadata tx) ----------
txids = [(n, meta_txid[n]) for _, n in pairs if n in meta_txid]
def tag_batch(i):
    chunk = txids[i:i+9]
    q = {"query": "query($ids:[ID!]){transactions(ids:$ids,first:9){edges{node{id tags{name value}}}}}",
         "variables": {"ids": [t for _, t in chunk]}}
    out = []
    try:
        res = http_json("https://arweave.net/graphql", q, retries=6)
        got = {}
        for e in res["data"]["transactions"]["edges"]:
            node = e["node"]
            got[node["id"]] = [t["value"] for t in node["tags"] if t["name"] == "Original-Mint"]
    except Exception as e:
        return [("9.graphql", f"batch {i//9}", str(e))]
    for n, tx in chunk:
        vals = got.get(tx)
        if vals is None:
            out.append(("9.tag", n, f"tx {tx} not returned by graphql")); continue
        if len(vals) != 1:
            out.append(("9.tag", n, f"{len(vals)} Original-Mint tags, expected exactly 1: {vals}")); continue
        if vals[0] != old_of[n]:
            out.append(("9.tag", n, f"Original-Mint {vals[0]!r} != {old_of[n]}"))
    return out
with ThreadPoolExecutor(max_workers=6) as ex:
    for f in as_completed([ex.submit(tag_batch, i) for i in range(0, len(txids), 9)]):
        for c, s, d in f.result(): fail(c, s, d)
print("9. TAGS done.", flush=True)

# ---------- 10. FREEZE (mainnet mint accounts; truncation is a failure) ----------
accs = get_multiple(MAINNET, news)
for n, acc in zip(news, accs):
    raw = acct_raw(acc, n, "10.mint", want_owner=TOKEN_PROGRAM, want_len=82)
    if raw is None: continue
    if int.from_bytes(raw[46:50], "little") == 0:
        continue   # explicitly no freeze authority — acceptable
    if raw[50:82] != edition_pda(decode_pubkey(n)):
        fail("10.freeze", n, "freeze authority is NOT the mint's own master edition PDA")
print("10. FREEZE done.", flush=True)

# ---------- 11. IMAGE (retrievable + sha256 == claim map art_sha256) ----------
def check_image(n):
    tx = image_of.get(n)
    if tx is None: return (n, [("11.image", n, "no image_tx in claim map")])
    rows[n]["image_tx"] = tx
    want = art_sha_of.get(n)
    if _CACHE["images"].get(tx) == want:
        return (n, [])          # bytes are immutable; a proven hash holds forever
    try:
        data = http_bytes(f"https://arweave.net/{tx}", retries=3)
    except Exception:
        try:
            data = http_bytes(FALLBACK_GW + tx, retries=4)
        except Exception as e:
            return (n, [("11.image-fetch", n, f"image tx {tx}: {e}")])
    got = hashlib.sha256(data).hexdigest()
    if got != want:
        return (n, [("11.image-hash", n, f"sha256 {got[:16]}… != claim map art_sha256")])
    _CACHE["images"][tx] = got
    return (n, [])
done = 0
with ThreadPoolExecutor(max_workers=16) as ex:
    for f in as_completed([ex.submit(check_image, n) for n in news]):
        n, findings = f.result()
        for c, s, d in findings: fail(c, s, d)
        done += 1
        if done % 300 == 0: print(f"11. images {done}/{len(news)}", flush=True)
print("11. IMAGE done.", flush=True)

# ---------- report ----------
verdict = "PASS — SEALABLE" if not failures else "FAIL — DO NOT SEAL"
report = {
    "sweep": "pre-seal three-way injective sweep (appendix §9 / spec §6b + image section)",
    "run_at": datetime.now(timezone.utc).isoformat(),
    "target_rpc": TARGET.split("?")[0],
    "program_id": PROGRAM_ID, "pool": pool_addr,
    "claim_map_sha256": claim_sha,
    "pool_state": pool_state,
    "pairs": len(pairs),
    "no_cache": NO_CACHE,
    "originals_ref_sha256": ORIGINALS_REF_SHA256,
    "verdict": verdict,
    "failures": failures,
    "notes": notes,
    "elapsed_secs": round(time.time() - t0, 1),
    "rows": [rows[n] for n in news],
}
body = json.dumps({k: v for k, v in report.items() if k != "rows"}, indent=1) \
     + json.dumps(report["rows"])
report["report_sha256"] = hashlib.sha256(body.encode()).hexdigest()
sign_kp = os.environ.get("THUGZ_SIGN_KEYPAIR")
if sign_kp:
    try:
        sig = subprocess.run(["solana", "sign-offchain-message", "-k", sign_kp,
                              report["report_sha256"]],
                             capture_output=True, text=True, timeout=30)
        report["report_signature"] = sig.stdout.strip()
    except Exception as e:
        report["report_signature"] = f"SIGNING FAILED: {e}"
open(os.path.join(HERE, "sweep_report.json"), "w").write(json.dumps(report, indent=1))
md = [f"# Sweep report — {report['run_at']}", "",
      f"**{verdict}**  ({len(failures)} failures, {report['elapsed_secs']}s)", "",
      f"- target: `{report['target_rpc']}`  pool: `{pool_addr}`",
      f"- claim map sha256: `{claim_sha}`",
      f"- pool state: `{pool_state}`",
      f"- report sha256: `{report['report_sha256']}`", ""]
if failures:
    md.append("## Failures")
    for f_ in failures:
        md.append(f"- **{f_['check']}** `{f_['subject']}` — {f_['detail']}")
md += ["", "## Notes"] + [f"- {x}" for x in notes]
open(os.path.join(HERE, "sweep_report.md"), "w").write("\n".join(md) + "\n")
save_cache()
print(f"\n{verdict}  ({len(failures)} failures, {report['elapsed_secs']}s)", flush=True)
sys.exit(1 if failures else 0)
