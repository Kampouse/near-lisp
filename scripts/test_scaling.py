#!/usr/bin/env python3
"""Comprehensive batch ccall scaling test to map the N vs gas boundary."""
import subprocess, json, time, sys, base64

ACCOUNT = "kampy.testnet"
CONTRACT = "kampy.testnet"

def call_eval_async(code, gas_tgas):
    args = json.dumps({"code": code})
    cmd = [
        "near", "contract", "call-function", "as-transaction",
        CONTRACT, "eval_async",
        "json-args", args,
        "prepaid-gas", f"{gas_tgas} Tgas",
        "attached-deposit", "0 NEAR",
        "sign-as", ACCOUNT,
        "network-config", "testnet",
        "sign-with-legacy-keychain",
        "send",
    ]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=90)
        output = r.stdout + r.stderr
        for line in output.splitlines():
            line = line.strip()
            if "Transaction ID:" in line:
                return line.split()[-1]
        return None
    except Exception as e:
        print(f"    CLI error: {e}")
        return None

def fetch_receipts(tx_hash, retries=10, delay=5):
    for attempt in range(retries):
        try:
            cmd = [
                "curl", "-s", "-H", "Content-Type: application/json",
                "https://archival-rpc.testnet.near.org",
                "-d", json.dumps({
                    "jsonrpc": "2.0", "id": 1, "method": "tx",
                    "params": [tx_hash, ACCOUNT]
                })
            ]
            r = subprocess.run(cmd, capture_output=True, text=True, timeout=15)
            d = json.loads(r.stdout)
            if "error" in d:
                if "429" in str(d["error"]) or "Too Many" in str(d["error"]):
                    time.sleep(delay)
                    continue
            if "result" in d:
                outcomes = d["result"].get("receipts_outcome", [])
                if outcomes:
                    return outcomes
        except Exception as e:
            pass
        time.sleep(delay)
    return None

def build_n_ccall_code(n):
    lines = []
    for i in range(n):
        lines.append(f'(define r{i} (near/ccall-view "kampy.testnet" "get_owner" "{{}}"))')
    lines.append(f'(near/storage-write "sc_{n}" (to-string (near/ccall-count)))')
    return "\n".join(lines)

def test_n_ccalls(n, gas_tgas):
    code = build_n_ccall_code(n)
    tx = call_eval_async(code, gas_tgas)
    if not tx:
        return None, "no tx"
    
    time.sleep(8)
    receipts = fetch_receipts(tx)
    if not receipts:
        return None, "no receipts"
    
    results = []
    total_burn = 0
    resume_success = False
    out_of_gas = False
    
    for i, r in enumerate(receipts):
        status = r.get("outcome", {}).get("status", {})
        burn = r.get("outcome", {}).get("gas_burnt", 0)
        total_burn += burn
        sv = status.get("SuccessValue", "")
        fail = status.get("Failure", {})
        val = ""
        if sv:
            try:
                val = base64.b64decode(sv).decode()[:80]
            except:
                val = sv[:50]
        if fail:
            err = str(fail)
            if "Exceeded" in err:
                val = "OUT_OF_GAS"
                out_of_gas = True
            else:
                val = f"FAIL"
        
        if i == 0:
            eval_async_burn = burn
        if i == 1 and sv and "FAIL" not in val and "OUT_OF_GAS" not in val:
            resume_success = True
    
    return {
        "n": n,
        "gas_tgas": gas_tgas,
        "total_burn": total_burn,
        "eval_async_burn": eval_async_burn,
        "receipts": len(receipts),
        "resume_success": resume_success,
        "out_of_gas": out_of_gas,
    }, "ok"

if __name__ == "__main__":
    # Test matrix: N ccalls at various gas levels
    tests = [
        # (N, gas_tgas) 
        (1, 30), (1, 50), (1, 100),
        (2, 30), (2, 50),
        (3, 30), (3, 50),
        (5, 50), (5, 100), (5, 300),
        (6, 100), (6, 300),
        (10, 100), (10, 300),
        (20, 200), (20, 300),
        (30, 200), (30, 300),
        (40, 200), (40, 300),
        (50, 300),
        (60, 300),
        (70, 300),
        (75, 300),
    ]
    
    # If args provided, override
    if len(sys.argv) > 1:
        if sys.argv[1] == "full":
            pass  # use default tests
        else:
            n = int(sys.argv[1])
            gas = int(sys.argv[2]) if len(sys.argv) > 2 else 300
            tests = [(n, gas)]
    
    results = []
    print(f"{'N':>4} {'Gas':>5} | {'eval_async':>10} {'total':>8} {'rcpts':>5} | {'status':>8}")
    print("-" * 60)
    
    for n, gas in tests:
        r, status = test_n_ccalls(n, gas)
        if r is None:
            print(f"{n:4d} {gas:5d} | {'ERROR':>10} {'':>8} {'':>5} | {status}")
        else:
            s = "PASS" if r["resume_success"] else ("OOG" if r["out_of_gas"] else "FAIL")
            print(f"{n:4d} {gas:5d} | {r['eval_async_burn']/1e12:>9.3f}T {r['total_burn']/1e12:>7.1f}T {r['receipts']:>5d} | {s:>8}")
            results.append(r)
        time.sleep(12)  # Rate limit cooldown between tests
    
    # Summary
    print()
    passed = [r for r in results if r["resume_success"]]
    failed = [r for r in results if not r["resume_success"]]
    print(f"Passed: {len(passed)}, Failed: {len(failed)}")
    
    if passed:
        max_n = max(r["n"] for r in passed)
        print(f"Max working N: {max_n}")
    
    if failed:
        min_fail = min(r["n"] for r in failed)
        print(f"Min failing N: {min_fail}")
