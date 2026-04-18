#!/usr/bin/env python3
"""Find minimum gas for N ccalls on testnet."""
import json, base64, time, subprocess, urllib.request, sys

ACCOUNT = "kampy.testnet"
RPC = "https://archival-rpc.testnet.near.org"

def run_test(code, gas_tgas):
    args = json.dumps({"code": code})
    cmd = ["near", "contract", "call-function", "as-transaction",
           ACCOUNT, "eval_async", "json-args", args,
           "prepaid-gas", f"{gas_tgas} Tgas", "attached-deposit", "0 NEAR",
           "sign-as", ACCOUNT, "network-config", "testnet",
           "sign-with-legacy-keychain", "send"]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=60)
    output = r.stdout + r.stderr
    tx_hash = None
    for line in output.splitlines():
        if "Transaction ID:" in line:
            tx_hash = line.split()[-1]
            break
    if not tx_hash:
        return None

    time.sleep(4)
    for attempt in range(3):
        try:
            payload = json.dumps({"jsonrpc":"2.0","id":1,"method":"tx",
                "params":[tx_hash, ACCOUNT]}).encode()
            req = urllib.request.Request(RPC, data=payload,
                headers={"Content-Type":"application/json"})
            with urllib.request.urlopen(req, timeout=10) as resp:
                data = json.loads(resp.read())
            for r in data['result']['receipts_outcome']:
                s = r['outcome']['status']
                if 'Failure' in s:
                    return 'FAIL'
                if 'SuccessValue' in s and s['SuccessValue']:
                    v = base64.b64decode(s['SuccessValue']).decode()
                    if v != '"YIELDING"':
                        return f'OK={v}'
            return '?'
        except:
            time.sleep(3)
    return '?'

def find_min(code, lo, hi):
    while lo < hi:
        mid = (lo + hi) // 2
        time.sleep(1)
        result = run_test(code, mid)
        print(f"  {mid}T → {result}")
        if result and result.startswith('OK'):
            hi = mid
        else:
            lo = mid + 1
    return lo

# 1 ccall
code1 = '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n(str-concat "ok=" a)'
# 2 ccalls
code2 = '(define a (near/ccall "kampy.testnet" "get_owner" "{}"))\n(define b (near/ccall "kampy.testnet" "get_gas_limit" "{}"))\n(+ (len (list a b)) 0)'
# 3 ccalls
code3 = '(near/ccall "kampy.testnet" "get_owner" "{}")\n(near/ccall "kampy.testnet" "get_gas_limit" "{}")\n(near/ccall "kampy.testnet" "get_owner" "{}")\n(near/ccall-count)'

print("Finding minimum gas per ccall count...")
print()

print("=== 1 ccall ===")
min1 = find_min(code1, 60, 100)
print(f"Minimum for 1 ccall: {min1}T")
print()

print("=== 2 ccalls ===")
min2 = find_min(code2, 100, 170)
print(f"Minimum for 2 ccalls: {min2}T")
print()

print("=== 3 ccalls ===")
min3 = find_min(code3, 170, 240)
print(f"Minimum for 3 ccalls: {min3}T")
print()

# Summary
print("=" * 40)
print(f"1 ccall:  {min1}T")
print(f"2 ccalls: {min2}T")
print(f"3 ccalls: {min3}T")
d1 = min2 - min1
d2 = min3 - min2
print(f"Delta 1→2: {d1}T")
print(f"Delta 2→3: {d2}T")
print(f"Avg per extra ccall: {(d1+d2)//2}T")
print(f"Base cost (1 ccall): {min1}T")
print(f"Formula: {min1}T + N*{(d1+d2)//2}T")
