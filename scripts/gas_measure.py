#!/usr/bin/env python3
"""
Phase 1: Measure actual gas burn per receipt for N=1..5 ccalls.

For each N:
  - Build a Lisp program with N cross-contract view calls (self-call get_owner)
  - Call eval_async with 200T prepaid gas
  - Parse ALL receipts from RPC tx status
  - Record gas_burnt, gas_used, execution_status per receipt
  - Calculate total burned vs prepaid

Usage:
    python3 scripts/gas_measure.py
    python3 scripts/gas_measure.py --verbose
"""

import json
import subprocess
import sys
import time
import base64
import urllib.request

ACCOUNT = "kampy.testnet"
RPC = "https://archival-rpc.testnet.near.org"
PREPAID_TGAS = 200

verbose = "--verbose" in sys.argv or "-v" in sys.argv


def call_eval_async(code, gas_tgas=200):
    """Call eval_async on-chain, return tx_hash."""
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
    if verbose and tx_hash:
        print("    TX: " + tx_hash)
    return tx_hash


def get_full_tx_status(tx_hash):
    """Fetch full transaction status with gas_burnt per receipt."""
    for attempt in range(8):
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
        except Exception as e:
            if verbose:
                print("    retry %d: %s" % (attempt + 1, e))
            continue

        result = data.get("result")
        if not result:
            if verbose:
                print("    retry %d: no result yet" % (attempt + 1))
            continue

        # Transaction-level gas
        tx_gas_burnt = result.get("transaction_outcome", {}).get("outcome", {}).get("gas_burnt", 0)

        # Per-receipt gas
        receipts = []
        for r in result.get("receipts_outcome", []):
            receipt_id = r.get("id", "?")
            outcome = r.get("outcome", {})
            gas_burnt = outcome.get("gas_burnt", 0)
            tokens_burnt = outcome.get("tokens_burnt", 0)
            status = outcome.get("status", {})
            exec_status = "Unknown"
            return_val = None
            if "SuccessValue" in status:
                exec_status = "Success"
                sv = status["SuccessValue"]
                if sv:
                    try:
                        return_val = base64.b64decode(sv).decode()
                    except Exception:
                        return_val = sv
            elif "Failure" in status:
                exec_status = "Failure"
                return_val = status["Failure"].get("error", "unknown error")
            elif "SuccessReceiptId" in status:
                exec_status = "SuccessReceiptId"

            logs = outcome.get("logs", [])
            receiver = r.get("outcome", {}).get("executor_id", "?")

            receipts.append({
                "id": receipt_id[:12] + "...",
                "receiver": receiver,
                "gas_burnt": gas_burnt,
                "tokens_burnt": tokens_burnt,
                "status": exec_status,
                "return": return_val,
                "logs": logs,
            })

        return {
            "tx_gas_burnt": tx_gas_burnt,
            "receipts": receipts,
            "total_receipt_gas": sum(r["gas_burnt"] for r in receipts),
        }

    return None


def build_ccall_code(n):
    """Build Lisp code with N cross-contract calls to self (get_owner)."""
    lines = []
    for i in range(n):
        lines.append('(near/ccall "%s" "get_owner" "{}")' % ACCOUNT)
    # Summarize
    if n == 1:
        lines.append('(near/storage-write "result" (to-string (near/ccall-result)))')
        lines.append("(near/ccall-result)")
    else:
        lines.append("(near/batch-result)")
    return "\n".join(lines)


def fmt_tgas(gas_units):
    """Format gas units as Tgas."""
    return "%.3fT" % (gas_units / 1e12)


def measure_n_ccalls(n, prepaid_tgas=200):
    """Measure gas for N ccalls."""
    code = build_ccall_code(n)
    if verbose:
        print("  Code:\n" + code)

    tx_hash = call_eval_async(code, gas_tgas=prepaid_tgas)
    if not tx_hash:
        print("  N=%d: FAILED - no tx hash" % n)
        return None

    time.sleep(8)  # Wait for finality + avoid rate limit

    status = get_full_tx_status(tx_hash)
    if not status:
        print("  N=%d: FAILED - no tx status" % n)
        return None

    return {
        "n_ccalls": n,
        "prepaid_tgas": prepaid_tgas,
        "tx_hash": tx_hash,
        "tx_gas_burnt": status["tx_gas_burnt"],
        "total_receipt_gas": status["total_receipt_gas"],
        "total_gas": status["tx_gas_burnt"] + status["total_receipt_gas"],
        "receipts": status["receipts"],
    }


def main():
    results = []

    print("=" * 80)
    print("GAS MEASUREMENT: N=1..5 ccalls, 200T prepaid")
    print("=" * 80)

    for n in range(1, 6):
        print("\n--- N=%d ccalls ---" % n)
        result = measure_n_ccalls(n, prepaid_tgas=200)
        if result:
            results.append(result)
            total_tgas = result["total_gas"] / 1e12
            prepaid = result["prepaid_tgas"]
            waste_pct = (1 - total_tgas / prepaid) * 100
            print("  Total gas burned: %.3fT / %dT prepaid (%.1f%% unused)" % (total_tgas, prepaid, waste_pct))
            for i, r in enumerate(result["receipts"]):
                ret_short = (str(r["return"]) or "None")[:60]
                gas_str = fmt_tgas(r["gas_burnt"])
                print("  Receipt %d: %10s  %-18s  %-25s  %s" % (i, gas_str, r["status"], r["receiver"], ret_short))
        else:
            print("  N=%d: measurement failed" % n)

        time.sleep(8)  # Rate limit between tests

    # Also try N=6 to confirm it fails
    print("\n--- N=6 ccalls (expecting failure) ---")
    result6 = measure_n_ccalls(6, prepaid_tgas=300)
    if result6:
        results.append(result6)
        total_tgas = result6["total_gas"] / 1e12
        prepaid = result6["prepaid_tgas"]
        print("  Total gas burned: %.3fT / %dT prepaid" % (total_tgas, prepaid))
        for i, r in enumerate(result6["receipts"]):
            ret_short = (str(r["return"]) or "None")[:80]
            gas_str = fmt_tgas(r["gas_burnt"])
            print("  Receipt %d: %10s  %-18s  %s" % (i, gas_str, r["status"], ret_short))

    # Summary table
    print("\n" + "=" * 80)
    print("SUMMARY TABLE")
    print("=" * 80)
    print("%3s | %10s | %12s | %8s | %s" % ("N", "Prepaid", "Total Burned", "Waste %", "Receipts"))
    print("-" * 80)
    for r in results:
        total_tgas = r["total_gas"] / 1e12
        waste_pct = (1 - total_tgas / r["prepaid_tgas"]) * 100
        n_receipts = len(r["receipts"])
        print("%3d | %7dT   | %9.3fT  | %6.1f%% | %d" % (r["n_ccalls"], r["prepaid_tgas"], total_tgas, waste_pct, n_receipts))

    # Per-receipt gas analysis
    print("\n" + "=" * 80)
    print("PER-RECEIPT GAS ANALYSIS")
    print("=" * 80)
    for r in results:
        n = r["n_ccalls"]
        print("\nN=%d ccalls (%d receipts):" % (n, len(r["receipts"])))
        for i, rec in enumerate(r["receipts"]):
            ret_short = (str(rec["return"]) or "None")[:50]
            gas_str = fmt_tgas(rec["gas_burnt"])
            print("  [%d] %10s  %-25s  %-18s  %s" % (i, gas_str, rec["receiver"], rec["status"], ret_short))

    # Marginal cost analysis
    if len(results) >= 2:
        print("\n" + "=" * 80)
        print("MARGINAL COST (per additional ccall)")
        print("=" * 80)
        for i in range(1, len(results)):
            prev = results[i - 1]
            curr = results[i]
            if prev["n_ccalls"] + 1 == curr["n_ccalls"]:
                delta = (curr["total_gas"] - prev["total_gas"]) / 1e12
                print("  N=%d->%d: +%.3fT" % (prev["n_ccalls"], curr["n_ccalls"], delta))

    # Save raw results as JSON
    with open("/tmp/gas_measure_results.json", "w") as f:
        json.dump(results, f, indent=2, default=str)
    print("\nRaw results saved to /tmp/gas_measure_results.json")


if __name__ == "__main__":
    main()
