#!/usr/bin/env python3
"""
Phase 1b: Find minimum prepaid gas for N=1..6 ccalls.
Starts at a low value and increments until success.

Also measures: what's the minimum gas where N=6 works?
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
    """Get total gas burned and whether it succeeded."""
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

        for r in result.get("receipts_outcome", []):
            outcome = r.get("outcome", {})
            total_receipt_gas += outcome.get("gas_burnt", 0)
            status = outcome.get("status", {})
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

        return {
            "tx_gas_burnt": tx_gas_burnt,
            "total_receipt_gas": total_receipt_gas,
            "total_gas": tx_gas_burnt + total_receipt_gas,
            "success": success,
            "resume_val": resume_val,
        }

    return None


def build_ccall_code(n):
    lines = []
    for i in range(n):
        lines.append('(near/ccall "%s" "get_owner" "{}")' % ACCOUNT)
    if n == 1:
        lines.append('(near/storage-write "result" (to-string (near/ccall-result)))')
        lines.append("(near/ccall-result)")
    else:
        lines.append("(near/batch-result)")
    return "\n".join(lines)


def find_min_gas(n, start=30, step=5, max_gas=150):
    """Binary-ish search for minimum gas that works for N ccalls."""
    code = build_ccall_code(n)

    # First, scan upward from start
    for gas in range(start, max_gas + 1, step):
        if verbose:
            print("    trying %dT ..." % gas, end=" ", flush=True)
        tx_hash, output = call_eval_async(code, gas_tgas=gas)
        if not tx_hash:
            if verbose:
                print("no tx hash")
            # Check if there was an explicit gas error
            if "GasLimitExceeded" in output or "exceeded the prepaid" in output:
                if verbose:
                    print("(gas limit)")
                continue
            time.sleep(3)
            continue

        time.sleep(5)

        result = get_tx_result(tx_hash)
        if result is None:
            if verbose:
                print("no result")
            continue

        if result["success"] and result["resume_val"] is not None:
            burned = result["total_gas"] / 1e12
            if verbose:
                print("OK (burned %.3fT)" % burned)
            return gas, result
        else:
            if verbose:
                print("FAILED (success=%s val=%s)" % (result["success"], result["resume_val"]))
            continue

    return None, None


def main():
    print("=" * 80)
    print("MINIMUM PREPAID GAS SEARCH: N=1..6 ccalls")
    print("=" * 80)

    results = {}

    for n in range(1, 7):
        print("\n--- N=%d ccalls ---" % n)
        min_gas, result = find_min_gas(n, start=30, step=5, max_gas=150)
        if min_gas:
            burned = result["total_gas"] / 1e12
            print("  Min prepaid: %dT (burned %.3fT, %.1f%% utilization)" % (
                min_gas, burned, burned / min_gas * 100
            ))
            results[n] = {"min_prepaid": min_gas, "burned": burned}
        else:
            # Try higher
            print("  Failed up to 150T, trying 150-300T range...")
            min_gas, result = find_min_gas(n, start=150, step=10, max_gas=300)
            if min_gas:
                burned = result["total_gas"] / 1e12
                print("  Min prepaid: %dT (burned %.3fT, %.1f%% utilization)" % (
                    min_gas, burned, burned / min_gas * 100
                ))
                results[n] = {"min_prepaid": min_gas, "burned": burned}
            else:
                print("  FAILED at all gas levels!")
                results[n] = {"min_prepaid": None, "burned": None}

        time.sleep(8)

    print("\n" + "=" * 80)
    print("MINIMUM GAS SUMMARY")
    print("=" * 80)
    print("%3s | %12s | %12s | %10s" % ("N", "Min Prepaid", "Actual Burn", "Util %"))
    print("-" * 60)
    for n in sorted(results.keys()):
        r = results[n]
        if r["min_prepaid"]:
            util = r["burned"] / r["min_prepaid"] * 100
            print("%3d | %10dT   | %9.3fT  | %8.1f%%" % (n, r["min_prepaid"], r["burned"], util))
        else:
            print("%3d | %10s   | %10s  | %8s" % (n, "N/A", "N/A", "N/A"))

    # Save
    with open("/tmp/gas_min_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print("\nSaved to /tmp/gas_min_results.json")


if __name__ == "__main__":
    main()
