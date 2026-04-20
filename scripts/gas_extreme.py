#!/usr/bin/env python3
"""Test very high N ccalls."""

import json
import subprocess
import time
import base64
import urllib.request

ACCOUNT = "kampy.testnet"
RPC = "https://archival-rpc.testnet.near.org"


def call_eval_async(code, gas_tgas=300):
    args = json.dumps({"code": code})
    cmd = [
        "near", "contract", "call-function", "as-transaction",
        ACCOUNT, "eval_async",
        "json-args", args,
        "prepaid-gas", str(gas_tgas) + " Tgas",
        "attached-deposit", "0 NEAR",
        "sign-as", ACCOUNT,
        "network-config", "testnet",
        "sign-with-legacy-keychain", "send",
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    output = r.stdout + r.stderr
    tx_hash = None
    for line in output.splitlines():
        line = line.strip()
        if "Transaction ID:" in line:
            tx_hash = line.split()[-1]
            break
    return tx_hash


def get_result(tx_hash):
    for attempt in range(8):
        time.sleep(3)
        payload = json.dumps({
            "jsonrpc": "2.0", "id": 1, "method": "tx",
            "params": [tx_hash, ACCOUNT]
        }).encode()
        req = urllib.request.Request(RPC, data=payload, headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                data = json.loads(resp.read())
        except Exception:
            continue
        result = data.get("result")
        if not result:
            continue
        total = result["transaction_outcome"]["outcome"].get("gas_burnt", 0)
        success = True
        resume_val = None
        n_receipts = 0
        for r in result.get("receipts_outcome", []):
            total += r["outcome"].get("gas_burnt", 0)
            n_receipts += 1
            status = r["outcome"].get("status", {})
            if "Failure" in status:
                success = False
            sv = status.get("SuccessValue", "")
            if sv:
                try:
                    val = json.loads(base64.b64decode(sv).decode())
                    if val != "YIELDING" and resume_val is None:
                        resume_val = val
                except Exception:
                    pass
        return {"total": total / 1e12, "success": success, "val": str(resume_val)[:50] if resume_val else None, "n_receipts": n_receipts}
    return None


for n in [50, 75, 100]:
    gas = 300
    code = "\n".join(['(near/ccall "%s" "get_owner" "{}")' % ACCOUNT] * n + ["(near/batch-result)"])
    print("N=%d @ %dT ... " % (n, gas), end="", flush=True)
    tx_hash = call_eval_async(code, gas_tgas=gas)
    if not tx_hash:
        print("no tx")
        continue
    time.sleep(5)
    result = get_result(tx_hash)
    if result:
        status = "OK" if result["success"] else "FAIL"
        print("%s burned=%.3fT %d receipts val=%s" % (status, result["total"], result["n_receipts"], result["val"]))
    else:
        print("no result")
    time.sleep(10)
