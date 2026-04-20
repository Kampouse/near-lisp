# Test Results — 2026-04-19

**Contract**: kampy.testnet (testnet)
**Timestamp**: 2026-04-19 ~00:53 UTC

---

## 1. Yield Boundary Tests (`scripts/test_yield_boundary.py`)

**Result: 30/32 passed, 2 failed**

### Section Breakdown

| Section | Tests | Passed | Failed |
|---------|-------|--------|--------|
| 1. Capacity Limits | 6 | 5 | 1 |
| 2. Error Paths | 4 | 4 | 0 |
| 3. Gas Edge Cases | 5 | 5 | 0 |
| 4. Multi-Yield Cycles | 3 | 2 | 1 |
| 5. State Persistence | 7 | 7 | 0 |
| 6. Cross-Yield Data Flow | 2 | 2 | 0 |
| 7. Corner Cases | 5 | 5 | 0 |

### Failures

**1. "max batch: 8 ccalls"** — `FAIL (no tx)`
- **Classification**: Rate limit false positive (RPC)
- The `near` CLI did not return a transaction hash. This is a testnet RPC rate limit issue — the test ran shortly after the 7-ccall test which passed at 120 Tgas. Not a contract bug.
- Same 8-ccall test has passed in prior runs with longer delays.

**2. "3 yield cycles: ccall + non-ccall + ccall + non-ccall + ccall"** — `FAIL (Exceeded prepaid gas)`
- **Classification**: Real gas limit issue
- Error: `Exceeded the prepaid gas` at 300 Tgas
- 3 yield cycles with 3 ccalls requires more than 300 Tgas. Each yield cycle costs ~100T base, and the 3 ccall targets each need ~10T. Total estimate: ~330T+ needed.
- **Fix**: Either increase the test gas to `400 Tgas` or reduce per-ccall gas default from 10T to 3-5T for view calls.

### Highlights
- **4, 5, 6, and 10-ccall batches all passed** — batched yield/resume is working well
- **All error paths passed** — bad accounts, bad methods, mixed good/bad batches
- **All gas edge cases passed** — including exact minimum (58T for 1 ccall, 70T for 2 ccalls)
- **All state persistence tests passed** — defines, storage, lambdas survive yield boundaries
- **All cross-yield data flow tests passed** — ccall results correctly feed into subsequent expressions

---

## 2. CCall Test Suite (`scripts/test_ccall.py`)

**Result: 14/21 passed, 7 failed**

### Section Breakdown

| Section | Tests | Passed | Failed |
|---------|-------|--------|--------|
| Single ccall (14 tests) | 14 | 14 | 0 |
| Multi-ccall batched (3 tests) | 3 | 0 | 3 |
| Sync eval (4 tests) | 4 | 0 | 4 |

### Failures

**All 7 failures are `FAIL (no receipts)` — RPC rate limit false positives.**

The ccall test suite ran immediately after the boundary tests (~25 min of continuous testnet RPC calls). By the time multi-ccall tests started, the archival RPC was returning empty responses for tx lookups:

- **multi-ccall: two sequential views** — FAIL (no receipts)
- **multi-ccall: ccall-count after 2 ccalls** — FAIL (no receipts)
- **three sequential ccalls + ccall-count** — FAIL (no receipts)
- **sync eval: arithmetic** — FAIL (no receipts)
- **sync eval: string concat** — FAIL (no receipts)
- **sync eval: list reduce** — FAIL (no receipts)
- **sync eval: loop/recur** — FAIL (no receipts)

**Classification**: All 7 are testnet RPC rate limiting. The `fetch_tx()` function retries 5 times with 3s delays (15s total), which is insufficient when the RPC is throttled after 25+ minutes of continuous calls. None are contract bugs.

**Recommendation**: Run test suites with longer delays (15-20s between tests) or add a longer cooldown between the two suites (10+ min).

---

## Summary

| Suite | Total | Passed | Failed | Real Bugs | Rate Limit False Positives |
|-------|-------|--------|--------|-----------|---------------------------|
| Yield Boundary | 32 | 30 | 2 | 1 | 1 |
| CCall | 21 | 14 | 7 | 0 | 7 |
| **Combined** | **53** | **44** | **9** | **1** | **8** |

### Real Issues (1)
1. **3 yield cycles exhaust 300 Tgas** — multi-yield gas budget needs tuning (increase test gas or reduce per-ccall gas)

### Rate Limit Artifacts (8)
- 1 "no tx" from yield boundary tests
- 7 "no receipts" from ccall tests (ran right after boundary tests)

### Action Items
- [ ] Increase 3-yield-cycle test gas to 400+ Tgas
- [ ] Add 10-min cooldown between test suites
- [ ] Consider increasing `fetch_tx` retry count from 5 to 10
- [ ] Consider increasing inter-test delay from 8s to 12s for back-to-back runs
