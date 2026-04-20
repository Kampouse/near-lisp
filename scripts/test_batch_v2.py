#!/usr/bin/env python3
"""Test N ccalls using a more compact code generation approach."""
import subprocess, json, time, sys, base64

ACCOUNT = "kampy.testnet"
CONTRACT = "kampy.testnet"

def call_eval_async(code, gas_tgas, timeout=120):
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
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
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
            data = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tx", "params": [tx_hash, ACCOUNT]}).encode()
            import urllib.request
            req = urllib.request.Request("https://archival-rpc.testnet.near.org", data=data, headers={"Content-Type": "application/json"})
            resp = urllib.request.urlopen(req, timeout=15)
            d = json.loads(resp.read())
            if "result" in d:
                outcomes = d["result"].get("receipts_outcome", [])
                if outcomes:
                    return outcomes
        except:
            pass
        time.sleep(delay)
    return None

def build_compact_n_ccall_code(n):
    """Use a loop/recur to generate N ccalls more compactly.
    
    Actually, near/ccall-view can only appear at top level (pre-flight detection).
    So we must generate N explicit define expressions. But we can use shorter var names.
    """
    lines = []
    for i in range(n):
        lines.append(f'(define r{i} (near/ccall-view "kampy.testnet" "get_owner" "{{}}"))')
    lines.append(f'(near/storage-write "sc" (to-string (near/ccall-count)))')
    return "\n".join(lines)

def test_n_ccalls(n, gas_tgas):
    code = build_compact_n_ccall_code(n)
    timeout = max(120, n * 2)  # More time for larger JSON
    tx = call_eval_async(code, gas_tgas, timeout)
    if not tx:
        return None, "no tx"
    
    time.sleep(10)
    receipts = fetch_receipts(tx)
    if not receipts:
        return None, "no receipts"
    
    results = []
    total_burn = 0
    resume_success = False
    out_of_gas = False
    eval_async_burn = 0
    
    for i, r in enumerate(receipts):
        status = r.get("outcome", {}).get("status", {})
        burn = r.get("outcome", {}).get("gas_burnt", 0)
        total_burn += burn
        sv = status.get("SuccessValue", "")
        fail = status.get("Failure", {})
        logs = r.get("outcome", {}).get("logs", [])
        val = ""
        if sv:
            try:
                val = base64.b64decode(sv).decode()[:80]
            except:
                val = sv[:50]
        if fail:
            if "Exceeded" in str(fail):
                val = "OUT_OF_GAS"
                out_of_gas = True
            else:
                val = "FAIL"
        
        if i == 0:
            eval_async_burn = burn
        if i == 1 and sv and "FAIL" not in val and "OUT_OF_GAS" not in val:
            resume_success = True
        
        results.append({
            "idx": i,
            "burn": burn,
            "val": val,
            "logs": logs,
        })
    
    return {
        "n": n,
        "gas_tgas": gas_tgas,
        "total_burn": total_burn,
        "eval_async_burn": eval_async_burn,
        "receipts": len(results),
        "resume_success": resume_success,
        "out_of_gas": out_of_gas,
        "logs": results[0]["logs"] if results else [],
    }, "ok"

if __name__ == "__main__":
    tests = [(int(sys.argv[1]), int(sys.argv[2]))] if len(sys.argv) > 2 else [
        (1, 50), (5, 100), (10, 100), (20, 200), (50, 300), (75, 300), (100, 300),
    ]
    
    if len(sys.argv) > 1 and sys.argv[1] != "full":
        n = int(sys.argv[1])
        gas = int(sys.argv[2]) if len(sys.argv) > 2 else 300
        tests = [(n, gas)]
    
    print(f"{'N':>4} {'Gas':>5} | {'eval_async':>10} {'total':>8} {'rcpts':>5} | {'status':>8}")
    print("-" * 60)
    
    for n, gas in tests:
        r, status = test_n_ccalls(n, gas)
        if r is None:
            print(f"{n:4d} {gas:5d} | {'ERROR':>10} {'':>8} {'':>5} | {status}")
        else:
            s = "PASS" if r["resume_success"] else ("OOG" if r["out_of_gas"] else "FAIL")
            print(f"{n:4d} {gas:5d} | {r['eval_async_burn']/1e12:>9.3f}T {r['total_burn']/1e12:>7.1f}T {r['receipts']:>5d} | {s:>8}")
            for log in r.get("logs", []):
                print(f"      {log}")
        time.sleep(12)
