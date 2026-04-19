#!/usr/bin/env python3
"""On-chain ccall/yield-resume tests for near-lisp.

Runs Lisp programs via eval_async on the already-deployed contract.
Checks ALL receipt outcomes (including deferred resume_eval results).

Usage:
    python3 scripts/test_ccall.py
    python3 scripts/test_ccall.py --verbose
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
    """Call eval_async on-chain, return tx_hash."""
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
    output = r.stdout + r.stderr
    tx_hash = None
    for line in output.splitlines():
        line = line.strip()
        if "Transaction ID:" in line:
            parts = line.split()
            tx_hash = parts[-1]
            break
    return tx_hash


def call_eval(code, gas="100 Tgas"):
    """Call sync eval on-chain, return tx_hash."""
    args = json.dumps({"code": code})
    cmd = [
        "near", "contract", "call-function", "as-transaction",
        ACCOUNT, "eval",
        "json-args", args,
        "prepaid-gas", gas,
        "attached-deposit", "0 NEAR",
        "sign-as", ACCOUNT,
        "network-config", "testnet",
        "sign-with-legacy-keychain", "send",
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    output = r.stdout + r.stderr
    tx_hash = None
    for line in output.splitlines():
        line = line.strip()
        if "Transaction ID:" in line:
            parts = line.split()
            tx_hash = parts[-1]
            break
    return tx_hash


def get_all_receipt_results(tx_hash):
    """Fetch all receipts from a transaction, return list of decoded return values."""
    if not tx_hash:
        return []
    for attempt in range(5):
        time.sleep(2)
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
        except Exception:
            continue
        receipts = data.get("result", {}).get("receipts_outcome", [])
        if receipts:
            results = []
            for r in receipts:
                sv = r["outcome"]["status"].get("SuccessValue", "")
                logs = r["outcome"].get("logs", [])
                if sv:
                    decoded = base64.b64decode(sv).decode()
                    results.append({"return": decoded, "logs": logs})
                else:
                    results.append({"return": None, "logs": logs})
            return results
    return []


def decode_lisp_return(raw):
    """
    Decode a receipt's base64-decoded return value to the Lisp result string.
    
    NEAR SDK JSON-encodes the String return value.
    json.loads gives us the Display output of the LispVal.
    For Str, Display wraps in quotes; for Num/Bool, it doesn't.
    """
    if raw is None:
        return None
    try:
        return json.loads(raw)
    except (json.JSONDecodeError, ValueError):
        return raw


def find_lisp_result(receipts):
    """
    Find the Lisp evaluation result from the receipt chain.
    
    For single-ccall eval_async:
      Receipt 0: eval_async → "YIELDING" (JSON)
      Receipt 1: resume_eval → the Lisp result (JSON-encoded Display output)
      Receipt 2: empty (callback setup)
      Receipt 3: raw ccall view result
      Receipts 4-7: overhead
    
    For multi-ccall:
      Receipt 0: eval_async → "YIELDING"
      Receipt 1: resume_eval (re-yield) → empty or "YIELDING"
      ... more receipts for second ccall chain ...
      Last non-empty before trailing empties: final resume_eval result
    
    For sync eval:
      Receipt 0: eval → the Lisp result
      Receipt 1: empty (overhead)
    
    Strategy: find the first receipt that's a non-YIELDING, non-None result
    after decoding. That's always the resume_eval (or eval) result.
    """
    for r in receipts:
        ret = r["return"]
        if ret is None:
            continue
        decoded = decode_lisp_return(ret)
        if decoded is None:
            continue
        if decoded == "YIELDING":
            continue
        return decoded
    return None


# ─── Test helpers ─────────────────────────────────────────────────────────────

def test_async(name, code, check_fn, gas="100 Tgas"):
    """Run an eval_async test. check_fn(decoded_lisp_result) → True/False."""
    global passed, failed
    print(f"  {name} ... ", end="", flush=True)

    tx_hash = call_eval_async(code, gas=gas)
    if not tx_hash:
        print("FAIL (no tx hash)")
        failed += 1
        return

    receipts = get_all_receipt_results(tx_hash)
    if not receipts:
        print("FAIL (no receipts)")
        failed += 1
        return

    result = find_lisp_result(receipts)

    try:
        ok = check_fn(result)
    except Exception as e:
        print(f"FAIL (check error: {e})")
        failed += 1
        return

    if ok:
        print("✓")
        passed += 1
    else:
        print(f"FAIL (got {result!r})")
        if verbose:
            for i, r in enumerate(receipts):
                print(f"    Receipt {i}: {r}")
        failed += 1


def test_sync(name, code, expected):
    """Run a sync eval test. Compare decoded result."""
    global passed, failed
    print(f"  {name} ... ", end="", flush=True)

    tx_hash = call_eval(code)
    if not tx_hash:
        print("FAIL (no tx hash)")
        failed += 1
        return

    receipts = get_all_receipt_results(tx_hash)
    if not receipts:
        print("FAIL (no receipts)")
        failed += 1
        return

    result = find_lisp_result(receipts)

    if result == expected:
        print("✓")
        passed += 1
    else:
        print(f"FAIL (got {result!r}, expected {expected!r})")
        if verbose:
            for i, r in enumerate(receipts):
                print(f"    Receipt {i}: {r}")
        failed += 1


# ─── Tests ────────────────────────────────────────────────────────────────────

print("=" * 60)
print("near-lisp ccall/yield-resume on-chain tests")
print(f"Contract: {ACCOUNT}")
print("=" * 60)

# ── Single-ccall tests ───────────────────────────────────────────────────────

# After decode_lisp_return, Str Display gives: "owner=kampy.testnet" (with quotes)
test_async("view ccall: define + str-concat",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(str-concat "owner=" owner)',
           lambda r: r == '"owner=kampy.testnet"')

test_async("standalone ccall + ccall-result",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall-result)',
           lambda r: r == '"kampy.testnet"')

# Num Display: no quotes
test_async("view ccall: number result + arithmetic",
           '(define limit (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(+ limit 1)',
           lambda r: r == "100000000000001")

test_async("view ccall: array result → list len",
           '(define policies (near/ccall "kampy.testnet" "list_policies" "{}"))\n'
           '(len policies)',
           lambda r: r == "0")

test_async("ccall number: greater-than check",
           '(define lim (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(> lim 0)',
           lambda r: r == "true")

test_async("ccall + storage write + read",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(near/storage-write "last_caller" owner)\n'
           '(near/storage-read "last_caller")',
           lambda r: r == '"kampy.testnet"')

# gas_limit = 100000000000000, 100000000000000 / 100000000000000 = 1
test_async("ccall result: division",
           '(define lim (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(/ lim 100000000000000)',
           lambda r: r == "1")

test_async("ccall + if branching (owner match)",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(if (= owner "kampy.testnet") "yes" "no")',
           lambda r: r == '"yes"')

test_async("near/ccall-view explicit syntax",
           '(define owner (near/ccall-view "kampy.testnet" "get_owner" "{}"))\n'
           '(str-concat "view:" owner)',
           lambda r: r == '"view:kampy.testnet"')

test_async("ccall number: arithmetic proves Num type",
           '(define lim (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(* lim 2)',
           lambda r: r == "200000000000000")

test_async("ccall + str-contains on result",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(str-contains owner "kampy")',
           lambda r: r == "true")

test_async("ccall + list append",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(len (append (list owner) (list "extra")))',
           lambda r: r == "2")

test_async("ccall + not on comparison",
           '(define owner (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(not (= owner "wrong.near"))',
           lambda r: r == "true")

test_async("ccall: number < comparison",
           '(define lim (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(< lim 999999999999999)',
           lambda r: r == "true")

# ── Multi-ccall tests (batched — all ccalls in one yield cycle) ─────────────

test_async("multi-ccall: two sequential views",
           '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n'
           '(define b (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n'
           '(+ (len (list a b)) 0)',
           lambda r: r == "2",
           gas="80 Tgas")

test_async("multi-ccall: ccall-count after 2 ccalls",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall-count)',
           lambda r: r == "2",
           gas="80 Tgas")

test_async("three sequential ccalls + ccall-count",
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n'
           '(near/ccall "kampy.testnet" "get_owner" "{}")\n'
           '(near/ccall-count)',
           lambda r: r == "3",
           gas="90 Tgas")

# ── Sync eval sanity checks ──────────────────────────────────────────────────

test_sync("sync eval: arithmetic",
          "(+ 1 2 3)",
          "6")

test_sync("sync eval: string concat",
          '(str-concat "hello" " " "world")',
          '"hello world"')

test_sync("sync eval: list reduce",
          "(reduce + 0 (list 1 2 3 4 5))",
          "15")

test_sync("sync eval: loop/recur",
          "(loop ((i 0) (sum 0)) (if (> i 10) sum (recur (+ i 1) (+ sum i))))",
          "55")

# ── Summary ───────────────────────────────────────────────────────────────────

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
