#!/usr/bin/env python3
"""
Phase 3b: Test higher N ccalls (7-10) with the new gas constants.
"""

import json
import subprocess
import sys
import time
import base64
import urllib.request

ACCOUNT = "kampy.testnet"
RPC = "https://archival-rpc.testnet.near.org"

verbose = "--verbose" in sys.argv or "-v" in sys.argv


def call_eval_async(code, gas_tgas=100):
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
            parts = line.split()
            tx_hash = parts[-1]
            break
    return tx_hash, output


def get_tx_result(tx_hash):
    for attempt in range(6):
        time.sleep(3)
        payload = json.dumps({
            "jsonrpc": "2.0", "id": 1, "method": "tx",
            "params": [tx_hash, ACCOUNT]
        }).encode()
        req = urllib.request.Request(
            RPC, data=payload,
            headers={"Content-Type": "application/json"}
        )
        try:
            with urllib.request.urlopen(req, timeout=15) as resp:
                data = json.loads(resp.read())
        except Exception:
            continue

        result = data.get("result")
        if not result:
            continue

        tx_gas_burnt = result.get("transaction_outcome", {}).get("outcome", {}).get("gas_burnt", 0)
        total_receipt_gas = 0
        success = True
        resume_val = None
        n_receipts = 0

        for r in result.get("receipts_outcome", []):
            outcome = r.get("outcome", {})
            total_receipt_gas += outcome.get("gas_burnt", 0)
            n_receipts += 1
            status = outcome.get("status", {})
            if "Failure" in status:
                success = False
                err = status["Failure"].get("error", "unknown")
                if resume_val is None:
                    resume_val = "FAIL: " + str(err)[:100]
            sv = status.get("SuccessValue", "")
            if sv:
                try:
                    val = json.loads(base64.b64decode(sv).decode())
                    if val != "YIELDING" and resume_val is None:
                        resume_val = val
                except Exception:
                    pass

        return {
            "tx_gas_burnt": tx_gas_burnt,
            "total_receipt_gas": total_receipt_gas,
            "total_gas": tx_gas_burnt + total_receipt_gas,
            "success": success,
            "resume_val": resume_val,
            "n_receipts": n_receipts,
        }

    return None


def build_ccall_code(n):
    lines = []
    for i in range(n):
        lines.append('(near/ccall "%s" "get_owner" "{}")' % ACCOUNT)
    lines.append("(near/batch-result)")
    return "\n".join(lines)


def test_n_ccalls(n, gas_tgas):
    code = build_ccall_code(n)
    tx_hash, output = call_eval_async(code, gas_tgas=gas_tgas)
    if not tx_hash:
        return None

    time.sleep(5)
    result = get_tx_result(tx_hash)
    if result:
        result["n"] = n
        result["prepaid"] = gas_tgas
    return result


def main():
    print("=" * 80)
    print("SCALING TEST: N=7..15 ccalls")
    print("=" * 80)

    # Test higher N with 100T prepaid
    for n in range(7, 16):
        print("\n--- N=%d ccalls, 100T prepaid ---" % n)
        result = test_n_ccalls(n, 100)
        if result:
            total = result["total_gas"] / 1e12
            status = "OK" if result["success"] else "FAIL"
            val = str(result["resume_val"])[:60] if result["resume_val"] else "None"
            print("  %s: burned %.3fT / %dT prepaid, %d receipts, val=%s" % (
                status, total, result["prepaid"], result["n_receipts"], val
            ))
        else:
            print("  FAILED: no result")

        time.sleep(8)

    # Now try N=10, 15, 20 with 200T
    print("\n\n=== Higher gas, higher N ===")
    for n in [10, 15, 20]:
        for gas in [100, 200, 300]:
            print("\n--- N=%d ccalls, %dT prepaid ---" % (n, gas))
            result = test_n_ccalls(n, gas)
            if result:
                total = result["total_gas"] / 1e12
                status = "OK" if result["success"] else "FAIL"
                val = str(result["resume_val"])[:80] if result["resume_val"] else "None"
                print("  %s: burned %.3fT / %dT prepaid, %d receipts, val=%s" % (
                    status, total, result["prepaid"], result["n_receipts"], val
                ))
                if result["success"]:
                    break  # Don't try higher gas if it worked
            else:
                print("  FAILED: no result")
            time.sleep(8)


if __name__ == "__main__":
    main()
