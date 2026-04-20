#!/usr/bin/env python3
"""
On-chain gas benchmark for NEAR contracts.
Binary searches for max iterations or measures exact gas via RPC.

Usage:
  python3 bench_onchain.py --code '(loop ((i 0)) (if (>= i N) i (recur (+ i 1))))' --range 1000 200000
  python3 bench_onchain.py --code '(reduce + 0 (range 0 N))' --range 100 10000
  python3 bench_onchain.py --tests  # run predefined benchmark suite

Requirements: near CLI configured with credentials.
"""
import subprocess, json, re, time, argparse, sys

ACCOUNT = "kampy.testnet"
RPC = "https://archival-rpc.testnet.near.org"
DEFAULT_GAS = 300  # Tgas
RPC_DELAY = 3  # seconds between tx and RPC query
BENCH_DELAY = 1.5  # seconds between benchmarks

def run(code, gas_tgas=DEFAULT_GAS):
    """Call eval on-chain, return total gas burned (Tgas) or None."""
    cmd = [
        "near", "contract", "call-function", "as-transaction",
        ACCOUNT, "eval",
        "json-args", json.dumps({"code": code}),
        "prepaid-gas", f"{gas_tgas}Tgas", "attached-deposit", "0NEAR",
        "sign-as", ACCOUNT, "network-config", "testnet",
        "sign-with-legacy-keychain", "send"
    ]
    r = subprocess.run(cmd, capture_output=True, text=True, timeout=120)
    output = r.stdout + r.stderr

    if "out of gas" in output.lower() or "Exceeded the prepaid gas" in output:
        return None, "OUT_OF_GAS"

    tx_match = re.search(r'Transaction ID: (\w+)', output)
    if not tx_match:
        return None, output[:300]

    tx_hash = tx_match.group(1)
    time.sleep(RPC_DELAY)

    for attempt in range(3):
        payload = {
            "jsonrpc": "2.0", "id": 1,
            "method": "EXPERIMENTAL_tx_status",
            "params": [tx_hash, ACCOUNT]
        }
        r2 = subprocess.run(
            ["curl", "-s", "-H", "Content-Type: application/json", RPC, "-d", json.dumps(payload)],
            capture_output=True, text=True, timeout=30
        )
        try:
            d = json.loads(r2.stdout)
            if "error" in d:
                if attempt < 2:
                    time.sleep(5)
                    continue
                return None, f"RPC: {d['error']}"
            total = d["result"]["transaction_outcome"]["outcome"]["gas_burnt"]
            for receipt in d["result"]["receipts_outcome"]:
                total += receipt["outcome"]["gas_burnt"]
            return total / 1e12, tx_hash
        except Exception as e:
            if attempt < 2:
                time.sleep(5)
                continue
            return None, f"Parse error: {e}"
    return None, "All retries failed"


def bench(name, code, n=None):
    """Run one benchmark, print result, return gas."""
    gas, info = run(code)
    if gas is not None:
        per = (gas * 1000) / n if n else None
        per_s = f"{per:>10.3f} Ggas{'/elem' if n else ''}" if per else ""
        print(f"  {name:<40} {gas:>8.2f} Tgas {per_s}")
        return gas
    else:
        print(f"  {name:<40}  FAIL  {str(info)[:60]}")
        return None


def binary_search_max(code_template, lo=1000, hi=200000, placeholder="N"):
    """Binary search for max N where code_template (with N placeholder) succeeds."""
    best_n, best_gas = 0, 0
    print(f"  Binary search: {lo} – {hi}")
    while lo <= hi:
        mid = (lo + hi) // 2
        code = code_template.replace(placeholder, str(mid))
        gas, info = run(code)
        if gas is not None:
            print(f"    N={mid:>8}  OK  {gas:.2f} Tgas")
            best_n, best_gas = mid, gas
            lo = mid + 1
        else:
            print(f"    N={mid:>8}  FAIL ({info})")
            hi = mid - 1
        time.sleep(BENCH_DELAY)
    if best_n:
        print(f"  → Max: {best_n} iterations ({best_gas:.2f} Tgas, {(best_gas*1000)/best_n:.3f} Ggas/iter)")
    return best_n, best_gas


def predefined_suite():
    """Run a standard benchmark suite."""
    print("═══ LOOP BENCHMARKS ═══\n")
    bench("1-binding count 1K", "(loop ((i 0)) (if (>= i 1000) i (recur (+ i 1))))", 1000)
    time.sleep(BENCH_DELAY)
    bench("1-binding count 10K", "(loop ((i 0)) (if (>= i 10000) i (recur (+ i 1))))", 10000)
    time.sleep(BENCH_DELAY)
    bench("1-binding count 50K", "(loop ((i 0)) (if (>= i 50000) i (recur (+ i 1))))", 50000)
    time.sleep(BENCH_DELAY)
    bench("2-binding count 10K", "(loop ((i 0) (sum 0)) (if (>= i 10000) sum (recur (+ i 1) (+ sum i))))", 10000)
    time.sleep(BENCH_DELAY)

    print("\n═══ LIST BENCHMARKS ═══\n")
    for n in [100, 1000, 5000, 10000]:
        bench(f"range(0,{n})", f"(range 0 {n})", n)
        time.sleep(BENCH_DELAY)
    print()
    for n in [100, 1000, 5000]:
        bench(f"map (* x 2) n={n}", f"(map (lambda (x) (* x 2)) (range 0 {n}))", n)
        time.sleep(BENCH_DELAY)
    print()
    for n in [100, 1000, 5000]:
        bench(f"reduce + n={n}", f"(reduce + 0 (range 0 {n}))", n)
        time.sleep(BENCH_DELAY)

    print("\n═══ BINARY SEARCH: MAX ITERATIONS ═══\n")
    binary_search_max("(loop ((i 0)) (if (>= i N) i (recur (+ i 1))))")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="On-chain NEAR gas benchmarking")
    parser.add_argument("--tests", action="store_true", help="Run predefined suite")
    parser.add_argument("--code", help="Lisp code with N as placeholder")
    parser.add_argument("--range", nargs=2, type=int, default=[1000, 200000], help="Binary search range")
    parser.add_argument("--account", default=ACCOUNT, help="NEAR account")
    parser.add_argument("--gas", type=int, default=DEFAULT_GAS, help="Prepaid gas (Tgas)")
    args = parser.parse_args()

    if args.account != ACCOUNT:
        ACCOUNT = args.account

    if args.tests:
        predefined_suite()
    elif args.code:
        lo, hi = args.range
        binary_search_max(args.code, lo, hi)
    else:
        parser.print_help()
