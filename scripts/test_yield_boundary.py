#!/usr/bin/env python3
"""Boundary tests for near-lisp yield/resume.

Probes limits, error paths, gas edge cases, multi-yield, and state persistence.
Runs on testnet against kampy.testnet.

Usage:
    python3 scripts/test_yield_boundary.py
    python3 scripts/test_yield_boundary.py --verbose
"""

import json
import subprocess
import sys
import time
import base64
import urllib.request

ACCOUNT = "kampy.testnet"
RPC = "https://archival-rpc.testnet.near.org"

passed = 0
failed = 0
verbose = "-v" in sys.argv or "--verbose" in sys.argv


def call_eval_async(code, gas="100 Tgas", deposit="0 NEAR"):
    args = json.dumps({"code": code})
    cmd = [
        "near", "contract", "call-function", "as-transaction",
        ACCOUNT, "eval_async",
        "json-args", args,
        "prepaid-gas", gas,
        "attached-deposit", deposit,
        "sign-as", ACCOUNT,
        "network-config", "testnet",
        "sign-with-legacy-keychain", "send",
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    for line in (r.stdout + r.stderr).splitlines():
        line = line.strip()
        if "Transaction ID:" in line:
            return line.split()[-1]
    return None


def fetch_tx(tx_hash):
    if not tx_hash:
        return None
    for _ in range(5):
        time.sleep(8)
        payload = json.dumps({
            "jsonrpc": "2.0", "id": 1, "method": "tx",
            "params": [tx_hash, ACCOUNT]
        }).encode()
        req = urllib.request.Request(
            RPC, data=payload,
            headers={"Content-Type": "application/json"}
        )
        try:
            with urllib.request.urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read())
            receipts = data.get("result", {}).get("receipts_outcome", [])
            if receipts:
                return receipts
        except Exception:
            continue
    return None


def decode_return(receipt):
    sv = receipt["outcome"]["status"].get("SuccessValue", "")
    if sv:
        return base64.b64decode(sv).decode()
    fail = receipt["outcome"]["status"].get("Failure")
    if fail:
        return "FAILURE:" + json.dumps(fail)
    return None


def decode_lisp(raw):
    if raw is None:
        return None
    if raw.startswith("FAILURE:"):
        return raw
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return raw


def find_lisp_result(receipts):
    for r in receipts:
        raw = decode_return(r)
        if raw is None:
            continue
        decoded = decode_lisp(raw)
        if decoded == "YIELDING":
            continue
        return decoded
    return None


def has_receipt_failure(receipts):
    for r in receipts:
        status = r["outcome"]["status"]
        if "Failure" in status:
            return status["Failure"]
    return None


def all_gas_burned(receipts):
    return sum(r["outcome"]["gas_burnt"] for r in receipts)


def get_logs(receipt):
    return receipt["outcome"].get("logs", [])


def test_async(name, code, check_fn, gas="100 Tgas"):
    global passed, failed
    print(f"  {name} ... ", end="", flush=True)
    tx = call_eval_async(code, gas=gas)
    if not tx:
        print("FAIL (no tx)")
        failed += 1
        return None

    receipts = fetch_tx(tx)
    if not receipts:
        print("FAIL (no receipts)")
        failed += 1
        return None

    result = find_lisp_result(receipts)
    try:
        ok = check_fn(result, receipts)
    except Exception as e:
        print(f"FAIL (check: {e})")
        failed += 1
        return receipts

    if ok:
        print("PASS")
        passed += 1
    else:
        print(f"FAIL (got {result!r})")
        if verbose:
            for i, r in enumerate(receipts):
                raw = decode_return(r)
                g = r["outcome"]["gas_burnt"]
                print(f"    R{i}: gas={g/1e12:.1f}T ret={raw}")
                for l in get_logs(r):
                    print(f"      LOG: {l}")
        failed += 1
    return receipts


print("=" * 60)
print("near-lisp yield/resume BOUNDARY tests")
print(f"Contract: {ACCOUNT}")
print("=" * 60)

# ═══════════════════════════════════════════════════════════════
# 1. CAPACITY LIMITS — default 10T gas per ccall
# ═══════════════════════════════════════════════════════════════

print("\n-- 1. Capacity Limits --")

test_async("4 ccalls in one yield",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "4",
           gas="100 Tgas")

test_async("5 ccalls",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "5",
           gas="100 Tgas")

test_async("6 ccalls",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "6",
           gas="120 Tgas")

test_async("8 ccalls",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "8",
           gas="140 Tgas")

test_async("10 ccalls",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "10",
           gas="170 Tgas")

# ═══════════════════════════════════════════════════════════════
# 2. CUSTOM GAS OVERRIDE
# ═══════════════════════════════════════════════════════════════

print("\n-- 2. Custom Gas Override --")

test_async("ccall with explicit 3T gas (lightweight target)",
           '(near/ccall "kampy.testnet" "get_owner" "{}" "3")\n'
           '(near/ccall-result)',
           lambda r, _: r == '"kampy.testnet"',
           gas="80 Tgas")

test_async("ccall with explicit 50T gas (heavy target margin)",
           '(near/ccall "kampy.testnet" "get_owner" "{}" "50")\n'
           '(near/ccall-result)',
           lambda r, _: r == '"kampy.testnet"',
           gas="100 Tgas")

test_async("10 ccalls with 3T each (tight gas)",
           '\n'.join(['(near/ccall "kampy.testnet" "get_owner" "{}" "3")'] * 10)
           + '\n(near/ccall-count)',
           lambda r, _: r == "10",
           gas="100 Tgas")

test_async("15 ccalls with 3T each",
           '\n'.join(['(near/ccall "kampy.testnet" "get_owner" "{}" "3")'] * 15)
           + '\n(near/ccall-count)',
           lambda r, _: r == "15",
           gas="120 Tgas")

# ═══════════════════════════════════════════════════════════════
# 3. ERROR PATHS
# ═══════════════════════════════════════════════════════════════

print("\n-- 3. Error Paths --")

test_async("ccall to non-existent account",
           '(near/ccall "nonexistent_abcdef.testnet" "get_owner" "{}")\n'
           '(near/ccall-result)',
           lambda r, _: r is not None,
           gas="100 Tgas")

test_async("ccall to non-existent method",
           '(near/ccall "kampy.testnet" "this_method_does_not_exist" "{}")\n'
           '(near/ccall-result)',
           lambda r, _: r is not None,
           gas="100 Tgas")

test_async("batch: 1 good + 1 bad ccall (non-existent account)",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define b (near/ccall "nonexistent_abcdef.testnet" "get_owner" "{}"))\n'
           '(str-concat a "-" b)',
           lambda r, _: r is not None,
           gas="100 Tgas")

test_async("batch: 1 good + 1 bad ccall (non-existent method)",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define b (near/ccall "kampy.testnet" "no_such_method" "{}"))\n'
           '(str-concat a "-" b)',
           lambda r, _: r is not None,
           gas="100 Tgas")

# ═══════════════════════════════════════════════════════════════
# 4. GAS EDGE CASES
# ═══════════════════════════════════════════════════════════════

print("\n-- 4. Gas Edge Cases --")

test_async("gas exhaustion: 10T too low for eval_async",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall-result)',
           lambda r, receipts: has_receipt_failure(receipts) is not None,
           gas="10 Tgas")

test_async("gas: minimum for 1 ccall (60T)",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(str-concat "ok=" a)',
           lambda r, _: r == '"ok=kampy.testnet"',
           gas="60 Tgas")

test_async("gas: minimum for 2 ccalls (80T)",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define b (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(+ (len (list a b)) 0)',
           lambda r, _: r == "2",
           gas="80 Tgas")

# ═══════════════════════════════════════════════════════════════
# 5. MULTI-YIELD CYCLES
# ═══════════════════════════════════════════════════════════════

print("\n-- 5. Multi-Yield Cycles --")

test_async("2 yield cycles: ccall group + non-ccall + ccall group",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(+ 1 2)\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "3",
           gas="150 Tgas")

test_async("2 yield cycles: ccall -> non-ccall -> ccall with define",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define x (+ 1 2))\n'
           '(define b (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(+ (len (list a b)) x)',
           lambda r, _: r == "5",
           gas="150 Tgas")

test_async("3 yield cycles: ccall + non-ccall + ccall + non-ccall + ccall",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(+ 1 1)\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(+ 2 2)\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall-count)',
           lambda r, _: r == "3",
           gas="250 Tgas")

# ═══════════════════════════════════════════════════════════════
# 6. STATE PERSISTENCE ACROSS YIELD
# ═══════════════════════════════════════════════════════════════

print("\n-- 6. State Persistence --")

test_async("define before yield survives resume",
           '(define pre-ccall "hello")\n'
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(str-concat pre-ccall ":" a)',
           lambda r, _: r == '"hello:kampy.testnet"',
           gas="100 Tgas")

test_async("multiple defines before yield survive",
           '(define x 10)\n'
           '(define y 20)\n'
           '(define a (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(+ x y a)',
           lambda r, _: r is not None,
           gas="100 Tgas")

test_async("storage write before ccall, read after resume",
           '(near/storage-write "boundary_key" "before_yield")\n'
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(near/storage-read "boundary_key")',
           lambda r, _: r == '"before_yield"',
           gas="100 Tgas")

test_async("ccall result used in subsequent define",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define b (str-concat "got:" a))\n'
           '(near/storage-write "ccall_result" b)\n'
           '(near/storage-read "ccall_result")',
           lambda r, _: r == '"got:kampy.testnet"',
           gas="100 Tgas")

test_async("state survives across 2 yield cycles",
           '(define master "first")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(+ 1 2)\n'
           '(define a (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(str-concat master ":" (to-string a))',
           lambda r, _: r is not None and "first" in str(r),
           gas="150 Tgas")

test_async("standalone ccall result via near/ccall-result",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(define x (near/ccall-result))\n'
           '(str-concat "r=" x)',
           lambda r, _: r == '"r=kampy.testnet"',
           gas="100 Tgas")

# ═══════════════════════════════════════════════════════════════
# 7. CROSS-YIELD DATA FLOW
# ═══════════════════════════════════════════════════════════════

print("\n-- 7. Cross-Yield Data Flow --")

test_async("ccall result feeds into arithmetic across yield",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(+ 1 2)\n'
           '(define len-a (len (list a)))\n'
           '(+ len-a 99)',
           lambda r, _: r == "100",
           gas="150 Tgas")

test_async("arithmetic on ccall number result persists",
           '(define lim (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(define half (/ lim 2))\n'
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(> half 0)',
           lambda r, _: r == "true",
           gas="100 Tgas")

# ═══════════════════════════════════════════════════════════════
# 8. CORNER CASES
# ═══════════════════════════════════════════════════════════════

print("\n-- 8. Corner Cases --")

test_async("ccall-count returns 0 before any ccall",
           '(near/ccall-count)',
           lambda r, _: r == "0",
           gas="100 Tgas")

test_async("ccall-view explicit syntax",
           '(define v (near/ccall-view "kampy.testnet" "get_owner" "{}"))\n'
           '(str-concat "view=" v)',
           lambda r, _: r == '"view=kampy.testnet"',
           gas="100 Tgas")

test_async("define+ccall result used in if-branch",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(if (str-contains owner "kampy")\n'
           '  (str-concat "match:" owner)\n'
           '  "no-match")',
           lambda r, _: r == '"match:kampy.testnet"',
           gas="100 Tgas")

test_async("batch: all defines preserved across yield",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define b (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(define c (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(str-concat a ":" (to-string b) ":" c)',
           lambda r, _: r == '"kampy.testnet:100000000000000:kampy.testnet"',
           gas="100 Tgas")

# ── Summary ───────────────────────────────────────────────────

print()
print("=" * 60)
total = passed + failed
print(f"Results: {passed}/{total} passed, {failed} failed")
if failed == 0:
    print("ALL TESTS PASSED")
else:
    print("SOME TESTS FAILED")
print("=" * 60)
sys.exit(1 if failed else 0)
