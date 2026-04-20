#!/usr/bin/env python3
"""Quick test for N batched ccalls at various gas levels."""
import subprocess, json, time, sys, base64

ACCOUNT = "kampy.testnet"
CONTRACT = "kampy.testnet"

def call_eval_async(code, gas_tgas="100"):
    """Call eval_async on-chain, return tx_hash."""
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
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
        output = r.stdout + r.stderr
        for line in output.splitlines():
            line = line.strip()
            if "Transaction ID:" in line:
                parts = line.split()
                return parts[-1]
        # Print output for debugging
        print(f"  CLI output: {output[:300]}")
        return None
    except Exception as e:
        print(f"  CLI error: {e}")
        return None

def call_view(method, args_json="{}"):
    """Call a view method."""
    cmd = [
        "near", "contract", "call-function", "as-read-only",
        CONTRACT, method,
        "json-args", args_json,
        "network-config", "testnet",
        "now",
    ]
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
        output = r.stdout.strip()
        if output:
            return output
        return None
    except:
        return None

def fetch_receipts(tx_hash, retries=8, delay=4):
    """Fetch all receipts for a tx."""
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
            if "result" in d:
                outcomes = d["result"].get("receipts_outcome", [])
                if outcomes:
                    return outcomes
        except:
            pass
        time.sleep(delay)
    return None

def test_n_ccalls(n, gas_tgas):
    """Test N batched ccalls."""
    # Build code: N ccalls to get_owner, then write count
    lines = []
    for i in range(n):
        lines.append(f'(define r{i} (near/ccall-view "kampy.testnet" "get_owner" "{{}}"))')
    lines.append(f'(near/storage-write "test_n_{n}" (to-string (near/ccall-count)))')
    code = "\n".join(lines)
    
    print(f"  Calling eval_async with {n} ccalls at {gas_tgas} Tgas...")
    tx_hash = call_eval_async(code, str(gas_tgas))
    if not tx_hash:
        return None, "no tx hash"
    
    print(f"  TX: {tx_hash}")
    time.sleep(8)
    receipts = fetch_receipts(tx_hash)
    if not receipts:
        return None, "no receipts"
    
    results = []
    for i, r in enumerate(receipts):
        status = r.get("outcome", {}).get("status", {})
        burn = r.get("outcome", {}).get("gas_burnt", 0)
        sv = status.get("SuccessValue", "")
        fail = status.get("Failure", {})
        val = ""
        if sv:
            try:
                val = base64.b64decode(sv).decode()[:100]
            except:
                val = sv[:50]
        if fail:
            err_msg = str(fail)[:200]
            if "Exceeded" in err_msg:
                val = f"OUT_OF_GAS"
            else:
                val = f"FAIL: {err_msg[:100]}"
        results.append((i, burn, val))
    
    # Check receipt 1 (resume_eval) for success
    if len(results) > 1:
        resume_val = results[1][2]
        if "FAIL" not in resume_val and "OUT_OF_GAS" not in resume_val:
            return results, "PASS"
        return results, f"resume: {resume_val}"
    return results, "no resume receipt"

if __name__ == "__main__":
    tests = []
    
    if len(sys.argv) > 2:
        # Specific test: N ccalls at GAS Tgas
        n = int(sys.argv[1])
        gas = int(sys.argv[2])
        tests = [(n, gas)]
    else:
        # Default: test 5, 6, 7, 8, 10 ccalls at 300 Tgas
        tests = [
            (1, 100),
            (5, 300),
            (6, 300),
            (7, 300),
            (8, 300),
            (10, 300),
        ]
    
    print(f"=== Batch Ccall Scaling Test ===")
    print(f"Contract: {CONTRACT}")
    print()
    
    for n, gas in tests:
        print(f"--- {n} ccalls at {gas} Tgas ---")
        results, status = test_n_ccalls(n, gas)
        
        if results is None:
            print(f"  ERROR: {status}")
        else:
            total_burn = sum(b for _, b, _ in results)
            print(f"  Status: {status}")
            print(f"  Total burned: {total_burn/1e12:.1f}T across {len(results)} receipts")
            for idx, burn, val in results[:5]:
                print(f"    Receipt {idx}: {burn/1e12:.3f}T - {val[:70]}")
            if len(results) > 5:
                print(f"    ... +{len(results)-5} more receipts")
        
        print()
        time.sleep(10)  # Rate limit cooldown
