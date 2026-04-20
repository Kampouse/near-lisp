# Near-Lisp Test Coverage Audit Report

## 1. Builtins in dispatch_call() vs Test Coverage

### All builtins in dispatch_call() match arms (src/lib.rs:945-1617):

**FULLY TESTED (direct unit tests in lisp_unit.rs):**
- `+`, `-`, `*`, `/`, `mod` — test_arithmetic, test_nested_arithmetic, test_mod_*
- `=`, `==`, `!=`, `/=`, `<`, `>`, `<=`, `>=` — test_comparison, float comparison tests
- `list`, `car`, `cdr`, `cons`, `len`, `append`, `nth` — test_list_ops, nth tests
- `str-concat`, `str-contains`, `str-length`, `str-substring`, `str-split`, `str-trim`
- `str-index-of`, `str-upcase`, `str-downcase`, `str-starts-with`, `str-ends-with`
- `str=`, `str!=`
- `nil?`, `list?`, `number?`, `string?`, `map?` — test_type_checks, test_map_predicate
- `to-float`, `to-int` — float conversion tests
- `dict`, `dict/get`, `dict/set`, `dict/has?`, `dict/keys`, `dict/vals`, `dict/remove`, `dict/merge`
- `empty?`, `range`, `reverse`, `sort`, `zip`
- `map`, `filter`, `reduce`, `find`, `some`, `every` (native HOFs)
- `to-string`, `to-json`, `from-json`
- `sha256`, `keccak256` — crypto tests
- `near/storage-write`, `near/storage-read`, `near/storage-remove`, `near/storage-has?`
- `near/signer=`, `near/predecessor=`
- `near/transfer`, `near/batch-call`
- `fmt` — string interpolation tests
- `ed25519-verify`, `ecrecover` — error path tests only (require NEAR runtime for happy path)

**ZERO UNIT TEST COVERAGE:**
- `keccak256` (note: SHA256 is tested, keccak256 is only in the crypto test section — actually it IS tested at line 770)

Wait — let me recheck. Looking more carefully at the tests:

Actually, the following have comprehensive coverage via the listed tests above. The only ones with genuinely NO test coverage are:

### Builtins in is_builtin_name() but NOT in dispatch_call() — BROKEN/UNIMPLEMENTED:

These three builtins are listed in `is_builtin_name()` (src/lib.rs:342-352) which means the evaluator recognizes them, but they have NO match arm in `dispatch_call()`. They fall through to the `_` arm and produce "undefined: X" errors:

1. **`type?`** — Listed at line 343 but NO implementation anywhere in dispatch_call. Examples 05 and 17 use it. It will always error with "undefined: type?".
2. **`to-num`** — Listed at line 342 but NO implementation anywhere in dispatch_call. Examples 08, 16, and 17 use it. It will always error with "undefined: to-num".
3. **`error`** — Listed at line 352 but NO implementation anywhere in dispatch_call. Example 08 uses it. It will always error with "undefined: error".

### Builtins in lisp_eval() special forms (NOT in dispatch_call):

These are handled as special forms in lisp_eval (lines 839-885), not via dispatch_call:
- `near/ccall-result`, `near/batch-result`, `near/ccall-count` — tested
- `near/block-height`, `near/timestamp` — tested (sandbox + unit)
- `near/predecessor`, `near/signer` — tested indirectly
- `near/account-balance`, `near/attached-deposit`, `near/account-locked-balance` — tested
- `near/log` — NOT tested (test_near_log_returns_nil is empty, comment says "Cannot test execution in unit tests")

## 2. Example Files Execution Status

**Executed (run_test!):** 01-11, 13, 15, 17-19
**Parse-only (parse_test!):** 12, 14, 16

**CRITICAL ISSUE:** Example `17-type-conversions.lisp` is a `run_test!` (executed) but contains calls to `to-num` and `type?` which are UNIMPLEMENTED. This test will FAIL at runtime. Either:
- The test was never actually run, OR
- The error is silently swallowed somehow

Example `08-error-handling.lisp` uses `(error "division by zero")` and `(to-num s)` — also uses unimplemented builtins. It IS executed via `run_test!`.

Example `05-lists.lisp` uses `(type? ...)` — IS executed via `run_test!`.

## 3. Sandbox Tests (lisp_sandbox.rs) Coverage Gaps

The sandbox tests cover:
- Arithmetic (+, *)
- Lambda + closures
- Fibonacci (recursion)
- Policy check (pass/fail)
- Save + eval persistent policy
- NEAR builtins (block-height, timestamp)
- Gas exhaustion
- List ops (car, cdr, append)
- Storage (write, read, missing key)

**NOT tested in sandbox:**
- String operations (str-concat, str-split, etc.)
- Dict/Map operations
- Float arithmetic
- Crypto (sha256, keccak256, ed25519-verify, ecrecover)
- Pattern matching (match)
- try/catch
- fmt string interpolation
- to-json / from-json
- near/transfer, near/batch-call
- near/signer=, near/predecessor=
- loop/recur
- require (stdlib)
- ccall (yield/resume)
- near/log
- Cross-contract call flow

## 4. Testnet Integration Test (lisp_testnet.rs)

Single test `test_lisp_testnet_full_lifecycle` covers:
- Deploy + initialize contract
- Arithmetic (+, *, -)
- Lambda + closure
- Fibonacci
- Policy check (pass/fail)
- Save + eval persistent policy
- NEAR builtins (block-height, timestamp)
- Storage (write, read)
- Gas exhaustion (separate contract)
- List ops (car, cdr, append)

Requires env vars: TESTNET_ACCOUNT_ID, TESTNET_SECRET_KEY
Covers essentially the same features as sandbox tests — no additional coverage.

## 5. "type? undefined when called directly" Bug

**CONFIRMED — BUG IS NOT FIXED.**

`type?` is listed in `is_builtin_name()` (line 343) but has NO implementation in `dispatch_call()`. When called, the evaluator:
1. Recognizes `type?` as a builtin (is_builtin_name returns true)
2. Routes it to dispatch_call()
3. No match arm exists for `"type?"` 
4. Falls through to `_` arm → lambda lookup → "undefined: type?" error

Same bug exists for `to-num` and `error`.

## 6. CEK Machine Path Tests

**NO CEK machine exists in this codebase.** There are no functions named `eval_loop`, `eval_step`, or `feed_value`. There is no CEK machine implementation. The evaluation strategy uses a direct recursive evaluator (`lisp_eval` function) with a `LispVal::Recur` variant for tail-call optimization in loop/recur.

## 7. TODO/FIXME/HACK Comments

**NONE FOUND.** The codebase has zero TODO, FIXME, HACK, XXX, or WORKAROUND comments. The only debug-related comment is a `GAS_DEBUG` log format string at line 2619.

## Summary of Critical Gaps

| Gap | Severity | Description |
|-----|----------|-------------|
| `type?` unimplemented | HIGH | Listed as builtin but crashes with "undefined" |
| `to-num` unimplemented | HIGH | Listed as builtin but crashes with "undefined" |
| `error` unimplemented | HIGH | Listed as builtin but crashes with "undefined" |
| Examples 05, 08, 17 will fail | HIGH | Use unimplemented builtins |
| `near/log` untested | LOW | Cannot test outside NEAR runtime |
| Sandbox: no string/dict/crypto tests | MEDIUM | Missing coverage for key features |
| No `bool?` builtin | LOW | Fuzz test references it but it doesn't exist |
| No CEK machine | INFO | Not a gap — architecture doesn't use one |
