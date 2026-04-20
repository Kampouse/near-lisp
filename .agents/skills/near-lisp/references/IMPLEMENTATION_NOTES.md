# Implementation Notes — near-lisp

Reference for modifying the near-lisp interpreter (Rust). Not needed for writing Lisp programs — see SKILL.md and `references/GAS_REFERENCE.md` for that.

## Contents

1. Gas System (internal Lisp gas)
2. Testing & Coverage
3. On-Chain Bugs Status
4. Gas Optimizations (bytecode VM, push/pop, etc.)
5. Known Code Issues & CEK Machine
6. apply_lambda Semantics

---

## Gas System

- Every `lisp_eval` call consumes **1 gas unit**
- Storage ops consume **additional 100 gas** each
- Default eval gas limit: **10,000** (configurable by owner). Testnet contract is set to **300T** (300,000,000,000,000) to match NEAR's 300 Tgas receipt cap.
- Out of gas → `ERROR: out of gas` (catchable via `try/catch`)
- `loop/recur` is the ONLY iteration pattern with zero stack growth. It costs exactly **8 gas per iteration** (formula: `8n + 7`). At 10K gas: count to 1,249. At 100T testnet limit: theoretical 12.5T iterations.
- **Important: internal Lisp gas ≠ real NEAR gas.** Lisp gas is 1 tick per `lisp_eval` call regardless of allocation cost. On-chain, NEAR charges per WASM instruction.
- **Bytecode loop VM (deployed)**: `loop/recur` with simple bodies compiles to a register-based bytecode VM — ~10x faster than the old tree-walk. On-chain benchmarks (kampy.testnet, 300 Tgas cap):

| Pattern | Iterations | Total Gas | Per-iter |
|---------|-----------|-----------|----------|
| 1-binding count | 1,000 | 2.30 Tgas | 0.80 Ggas |
| 1-binding count | 10,000 | 8.99 Tgas | 0.75 Ggas |
| 1-binding count | 50,000 | 38.71 Tgas | 0.74 Ggas |
| 1-binding count | 100,000 | 75.86 Tgas | 0.74 Ggas |
| 2-binding count | 10,000 | 16.96 Tgas | 1.55 Ggas |
| 2-binding count | 100,000 | 155.28 Tgas | 1.54 Ggas |
| Baseline (no loop, eval `\"1\"`) | — | 1.50 Tgas | — |

- **Max iterations at 300 Tgas**: ~401K (1-binding), ~194K (2-binding) — binary searched on-chain
- **Per-iteration cost** (marginal, converges at high N): ~0.74 Ggas/iter (1-binding), ~1.54 Ggas/iter (2-binding), ~0.79 Ggas marginal per extra binding
- **Old tree-walk cost** (for comparison): ~22.45 Ggas/iter, max ~13,350 iterations
- **Peephole optimizer** fuses LoadSlot + PushI64 + Arith/Cmp into single ops (SlotAddImm, SlotGeImm, etc.) and converts small Recur→RecurDirect. This provides ~3x improvement over non-fused bytecode. CRITICAL: jump targets must be remapped after fusion (see On-Chain Bugs section).
- **Old tree-walk cost** (for comparison): ~22.45 Ggas/iter, max ~13,350 iterations
- The internal gas limit should be set to 300T (`set_gas_limit(300000000000000)`) to match NEAR's receipt gas cap.
- **On-chain gas benchmarking method**: `near` CLI truncates gas to 3 decimal Tgas — useless for precision. Use RPC `EXPERIMENTAL_tx_status` for exact gas: extract tx hash from CLI output, then `curl RPC -d '{"method":"EXPERIMENTAL_tx_status","params":["TX_HASH","ACCOUNT"]}'`, sum `transaction_ou

... [OUTPUT TRUNCATED - 21249 chars omitted out of 71249 total] ...

te.remaining` → `state.remaining`
- `yi.state.env` → `state.env`
- `yi.state.gas` → `state.gas`

**Multi-ccall tests now batch**: Old tests expected two ccalls to yield separately (first yields, resume, second yields, resume). Now consecutive ccalls batch into ONE yield with `yields.len() == N`. Tests like `test_multi_ccall_two_ccalls_yield_chain` need rewriting: both ccalls appear in `yields`, both var names in `pending_vars`, resume injects all N results at once.

**`test_run_program_no_ccall`**: This one is easy — `RunResult::Yield(_)` → `RunResult::Yield { .. }` (wildcard struct).

**VmState roundtrip tests** (`test_vmstate_roundtrip`, `test_vmstate_complex_env`, `test_hex_roundtrip`): Change `pending_var: Some(...)` / `pending_var: None` to `pending_vars: vec![...]` / `pending_vars: vec![]`.

**Tests that DON'T need migration**: All `eval_str()` / `eval_str_gas()` tests — these call `run_program()` which returns `Result<String, String>`, completely unaffected by the VmState/RunResult changes.

### Sandbox test coverage gap

The example files (examples/01-19) are not integration-tested in sandbox. They run as unit tests or parse-only. The critical gap: examples 12 (near-context), 13 (modules), 14 (policies), 16 (cross-contract) are parse-only — never executed on-chain. The sandbox tests in `tests/lisp_sandbox.rs` cover individual features inline with explicit assertions, but don't load the example files.

### Testing yield/resume on-chain (ccall)

**ccall placement restriction**: `near/ccall`, `near/ccall-view`, `near/ccall-call` are ONLY detected at the TOP expression level (or inside a top-level `(define var (near/ccall ...))`). They are pre-flight detected by `check_ccall()`, NOT handled at runtime in `dispatch_call`. Placing them inside `progn`, `let`, `if`, or any nested form results in `"undefined: near/ccall-view"`.

**CLI hides deferred receipt results**: `near-cli-rs` only shows the return value from the FIRST receipt. Yield/resume produces deferred receipts — the CLI shows `"YIELDING"` from `eval_async` but NOT the actual result from `resume_eval`. To see the full result chain:
```bash
curl -s -H 'Content-Type: application/json' 'https://archival-rpc.testnet.near.org' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tx","params":["<TX_HASH>","<account>.testnet"]}' \
  -o /tmp/tx_result.json
python3 -c "
import json, base64
with open('/tmp/tx_result.json') as f:
    d = json.load(f)
for i,r in enumerate(d['result']['receipts_outcome']):
    sv = r['outcome']['status'].get('SuccessValue','')
    val = base64.b64decode(sv).decode() if sv else '(empty)'
    print(f'Receipt {i}: {val}')
"
```
Receipt 0 = "YIELDING", Receipt 1 = actual resume_eval result, Receipt 3 = ccall target return.

**JSON double-encoding**: View call results come back JSON-encoded. `get_owner` returns `"kampy.testnet"` (with quotes). Concatenating with `str-concat` produces `"The owner is: "kampy.testnet""`. Strip outer quotes if needed.

**Cross-contract (ccall yield/resume) in sandbox**: CONFIRMED — `promise_yield_create` does NOT execute in near-workspaces sandbox (v0.22). Calls to `eval_async` always fail at ~1.78T gas (the yield never fires). The ccall yield path can ONLY be tested on testnet. Sandbox CAN measure non-yield costs:
- Sync eval: ~1.5T per call
- Ccall scanning: ~0.03T per ccall scanned (negligible — the double-scan for pending_vars is not worth optimizing)
- Sandbox benchmarks exist in `tests/bench_micro.rs` and `tests/bench_gas_sandbox.rs`.

### Build pitfalls

**WASM output path mismatch**: `cargo near build` outputs to `target/near/near_lisp.wasm` but sandbox tests read from `target/near/near_lisp/near_lisp.wasm`. After building WASM, copy: `cp target/near/near_lisp.wasm target/near/near_lisp/near_lisp.wasm`

**SDK version pinning**: near-sdk 5.6.0 is pinned because 5.24.1+ requires rustc 1.88+ (via `time` crate). The wasm32-unknown-unknown target requires rustup override to 1.86.0. Do NOT run `cargo update` — it pulls incompatible deps. If accidentally updated: `git checkout -- Cargo.lock`.

**Deploy cost**: WASM is ~402KB. NEAR charges ~1 NEAR per 100KB storage. Deployment costs ~2.6-3.5 NEAR (~$1.50-$2.00 USD). Free on sandbox/testnet.

**`is_multiple_of` is unstable**: The `unsigned_is_multiple_of` feature is nightly-only. Use `% n != 0` for parity checks — works on stable.

**rustfmt + chained closures**: rustfmt reformatting multi-line `.and_then()` chains can cause type mismatches. If the first `.ok_or("literal")` returns `Result<_, &str>` but a later `.map_err(|_| format!(...))` returns `Result<_, String>`, the collect fails. Fix: annotate the first error type explicitly with `.ok_or::<String>("msg".into())`.

**Sandbox view methods**: View methods returning `u64` (like `storage_usage()`) can trap with "Memory out of bounds" in sandbox. This is a near-workspaces serialization quirk, not a contract bug.

## On-Chain Smoke Test

```bash
cd ~/.openclaw/workspace/near-lisp-clean
python3 scripts/testnet_smoke.py
```

Runs 144 tests against the live testnet contract. Uses `near contract call-function as-transaction` with inline JSON args. Returns pass/fail per test with summary.

**Important**: Some tests involving `require` may need higher gas. The script uses 30 Tgas per call which works for most tests. Increase to `100 Tgas` if stdlib-loading tests fail with out-of-gas errors. The `near` CLI does NOT support `file://` scheme for `json-args` — must use inline JSON strings.

## On-Chain Bugs — Status (2026-04)

**FIXED — Peephole optimizer jump target remap**: `peephole_optimize()` fuses 3 ops (LoadSlot + PushI64 + Arith) into 1 fused op (e.g. SlotAddImm), shrinking the bytecode. But jump targets (JumpIfFalse, JumpIfTrue, Jump) were compiled against the original indices — after fusion they pointed past the end of the shorter optimized code, causing `index out of bounds` panics at runtime. Fix: build an `index_map: Vec<usize>` mapping old_pc → new_pc during fusion, then remap all jump targets in a second pass via `remap_jump_target()`. The `Recur`/`RecurDirect` ops don't need remapping since they always jump to pc=0 (loop start).

**FIXED — `nil?` vs empty list**: `(nil? (list))` now returns `true`. The fix adds `LispVal::List(ref v) if v.is_empty()` as a second matches! arm (can't combine with Nil in one pattern due to Rust E0408 — variable not bound in all patterns). This fixes ALL recursive list functions that use `(nil? lst)` or `(empty? lst)` as base case.

**FIXED — `match` now supports flat syntax**: `(match 42 _ 99)` now works alongside the original clause syntax `(match 42 (_ 99))`. Both `lisp_eval` and the CEK trampoline `Frame::MatchExpr` handler were updated to walk clauses with a `while i < clauses.len()` loop — if a clause is a List, use clause form and advance by 1; if it's a bare atom, treat it as flat pattern-result pair and advance by 2.

**FIXED — IIFE**: `((make-adder 5) 3)` now works. The fix replaces the rigid "must be literal lambda form" check in `dispatch_call` with `lisp_eval(head, env, gas)?` + `call_val()`. This handles both literal inline lambdas `((lambda (x) ...) 5)` and computed lambdas `((make-adder 5) 3)` — lisp_eval resolves both to LispVal::Lambda, then call_val dispatches it.

**MEDIUM — `check_policy` not callable from eval**: `check_policy` is a contract method, not a Lisp function. Use `eval_policy(name, input_json)` for stored policies, or call `check_policy` as a contract method directly via `near contract call-function`.

**FIXED — List stdlib native builtins**: All list stdlib functions (map, filter, reduce, range, sort, reverse, zip, find, some, every, empty?) are now native Rust builtins in `dispatch_call`. Gas cost dropped from 100+ Tgas (Lisp lambda) to 0.309 Tgas (native Rust loop). `(range 0 100)` costs the same as `(range 0 3)`. No `require "list"` needed — they shadow the Lisp stdlib definitions automatically. Higher-order builtins (map, filter, reduce, find, some, every) call user-provided lambdas via `call_val` per element — the outer iteration is free, only the user's lambda pays gas per call.

**FIXED — First-class builtin functions**: `+`, `-`, `*`, `/`, `=`, `<`, `>`, `<=`, `>=`, `!=`, `str-concat`, and all other `dispatch_call` builtins are now first-class callable values. `lisp_eval` checks `is_builtin_name()` — if a symbol is a builtin and not in the env, it evaluates to itself (the Sym). `call_val` then dispatches it through `dispatch_call`. This enables `(reduce + 0 (list 1 2 3))` without wrapping in lambda. **Env bindings shadow builtins**: `(define + -)` then `(+ 5 3)` → `2`.

**FIXED — `Rc<Vec>` → `Box<Vec>` for closed_env**: `LispVal::Lambda::closed_env` was `Rc<Vec<...>>` which doesn't implement `BorshDeserialize`, blocking WASM builds. Changed to `Box<Vec<...>>` — Borsh works, WASM builds clean. `apply_lambda` takes `&Vec<...>` now (was `&Rc<Vec<...>>`). `call_val` passes `&**closed_env` via auto-deref coercion.

**FIXED — ccall result JSON double-encoding**: `resume_eval` was injecting raw `promise_result` bytes as `LispVal::Str`, so JSON-encoded returns (e.g. `"kampy.testnet"` with quotes) became double-quoted Lisp strings. Fixed by parsing through `serde_json::from_str` → `json_to_lisp`, which unwraps JSON types into proper Lisp types (strings without quotes, numbers as Num, arrays as List, objects as Map). Falls back to raw string if JSON parse fails.

**FIXED — Multi-ccall batched yield/resume (deployed)**: ccalls in `eval_async` only work at the TOP expression level or inside `(define var (near/ccall ...))`. They are pre-flight detected by `check_ccall()`, NOT handled at runtime in `dispatch_call`. Placing them inside `progn`, `let`, `if`, or any nested form results in `"undefined: near/ccall-view"`. Multiple consecutive top-level ccalls are now batched into a single yield cycle via `Promise::and()` — all N ccalls run in parallel, one callback collects all results. Gas (optimized): 55T base + ~5T per extra ccall. 6 ccalls at 75T prepaid, actual burn ~20.6T. All 21 on-chain tests pass.

**FIXED — `type?`, `to-num`, `error`, `bool?` builtins**: All four were listed in `is_builtin_name()` but had no handler in `dispatch_call()`. Now implemented. `type?` returns type string, `to-num` alias for to-int, `error` raises catchable errors, `bool?` checks for Bool type.

**FIXED — nested match destructuring**: `match_pattern()` now treats any bare symbol as a binding variable (not just `?x`). Nested patterns like `(match (list 1 (list 2 3)) ((a (b c)) (+ a b c)))` work. `else` is also a wildcard.

**NEW — variadic lambdas (&rest)**: `(lambda (a b &rest rest) ...)` collects remaining args into `rest` as a list. `rest_param` field on `LispVal::Lambda`.

**NEW — require namespace prefix**: `(require "math" "m")` loads all module defs with `m/` prefix (m/abs, m/max etc). Without prefix, old flat behavior preserved.

**NEW — defmacro system**: `(defmacro name (params) body)` defines macros — args are NOT evaluated before being passed to the macro body. `(macroexpand expr)` expands macros without evaluating. Supports `&rest` params. Macro expansion happens before special form dispatch in `lisp_eval`. Macros close over their definition env (like lambdas). Gas is passed through — macro expansion costs gas.

**NEW — inspect builtin**: `(inspect x)` returns type+value description string for debugging.

**NOT IMPLEMENTED — Bytes type**: `LispVal::Bytes(Vec<u8>)` and its 8 builtins (`hex->bytes`, `bytes-hex`, `bytes->hex`, `bytes-len`, `bytes->string`, `string->bytes`, `bytes-concat`, `bytes-slice`) are documented in the Language Reference section of SKILL.md but do NOT exist in the current code. 12 tests in `tests/lisp_coverage.rs` fail with `"undefined: hex->bytes"`. The `hex_decode`/`hex_encode` helpers DO exist in the codebase — just no `LispVal::Bytes` variant or dispatch_call handlers.

**Testing ccall yield/resume on-chain**: The `near` CLI only shows the first receipt's return value ("YIELDING"). The actual result from `resume_eval` is in a deferred receipt. Use `scripts/test_ccall.py` for automated testing, or manually query RPC:
```bash
curl -s -H 'Content-Type: application/json' https://archival-rpc.testnet.near.org \
  -d '{"jsonrpc":"2.0","id":1,"method":"tx","params":["TX_HASH","kampy.testnet"]}' \
  -o /tmp/tx.json
python3 -c "
import json, base64
d = json.load(open('/tmp/tx.json'))
for i, r in enumerate(d['result']['receipts_outcome']):
    sv = r['outcome']['status'].get('SuccessValue', '')
    val = json.loads(base64.b64decode(sv)) if sv else None
    print(f'Receipt {i}: {val!r}')
"
```
**Receipt decoding**: base64 decode → `json.loads()` → Lisp Display output. For `LispVal::Str`, Display wraps in quotes: `'"hello"'`. For `LispVal::Num`, no wrapping: `'42'`. Receipt 0 = "YIELDING", Receipt 1 = resume_eval result, Receipt 3 = raw ccall target return.

**On-chain ccall test script**:
```bash
python3 scripts/test_ccall.py           # 21 tests (14 single-ccall + 3 multi-ccall batched + 4 sync)
python3 scripts/test_ccall.py --verbose # show receipt details on failure
```

**Working on-chain**: arithmetic, comparisons, strings, define/let, if/cond/and/or/not, lambdas, closures (with binding), loop/recur (native), storage, crypto, dict, JSON, fmt, type conversions, try/catch, progn, near-context builtins, match, IIFE, nil?/empty?, type predicates (except `type?`), native list builtins (map/filter/reduce/range/sort/reverse/zip/find/some/every/empty?), first-class builtins as values, single and multi-ccall batched yield/resume with proper JSON typing. All 21 test_ccall.py tests pass.

## Test Coverage (as of 2026-04)

**Test files:**
- `tests/lisp_unit.rs` — 334+ unit tests (core eval, builtins, contract methods)
- `tests/lisp_coverage.rs` — 37 tests (bytes, modules, require prefix, crypto stdlib, storage views, near/predecessor, near/signer, eval_script_async, near/log, NEP-297 events)
- `tests/lisp_sandbox.rs` — sandbox integration tests
- `tests/test_examples.rs` — parse-only validation for example files
- `scripts/test_ccall.py` — 21 on-chain ccall tests
- `scripts/testnet_smoke.py` — 144 on-chain smoke tests

**NOW COVERED (added in test coverage fix):**
- Bytes type: 12 tests in lisp_coverage.rs but ALL FAIL — builtins not implemented yet (see "NOT IMPLEMENTED" in bugs section)
- Custom modules: save/get/list/remove + invalid parse (5 tests)
- require namespace prefix: (require "math" "m") → m/abs, m/max, caching (3 tests)
- require "crypto" stdlib: hash/sha256-bytes, hash/keccak256-bytes (2 tests)
- storage_usage/storage_balance contract views (2 tests, methods added to lib.rs)
- near/predecessor + near/signer raw string getters (3 tests)
- eval_script_async: found + missing (2 tests)
- near/log: returns nil with VM context (1 test)
- NEP-297 events: all 7 mutating methods emit standard EVENT_JSON (7 tests — save/remove policy/script/module + transfer_ownership)

**STILL NOT IMPLEMENTED (code doesn't exist):**
- Nothing — all documented features now have code + tests.

**STILL NOT TESTED ON-CHAIN (unit-only):**
- variadic lambdas (&rest), near/batch-call, near/transfer

## Gas Optimizations (2026-04)

The following optimizations were applied to reduce on-chain gas cost:

**P0 — Eliminated re-evaluation at ccall completion** (run_program_with_ccall, run_remaining_with_ccall):
- Both functions re-evaluated ALL expressions from scratch just to get the final result after the last expression was consumed by the eval loop. Now tracks `last_result` during evaluation — returns it directly with zero re-evaluation.
- Impact: 30-50% gas reduction for non-ccall scripts, 15-25% for ccall scripts.

**P0 — Eliminated double ccall scan** (run_program_with_ccall, run_remaining_with_ccall):
- Both functions scanned ccall expressions TWICE: once to build the yield batch, once with a full env clone to extract pending_vars. Now extracts pending_vars from the already-collected batch in a single pass.
- Impact: eliminates one full env clone + N re-evaluations of ccall expressions per yield cycle.

**P0 — loop/recur bytecode VM (DEPLOYED, ~10x faster than tree-walk)**:
- `loop/recur` with simple bodies (arith, comparisons, builtins, if/else branching) compiles to a register-based bytecode VM instead of tree-walking.
- Opcodes: `PushI64`, `PushFloat`, `PushBool`, `PushStr`, `PushNil`, `Dup`, `Pop`, `LoadSlot`, `StoreSlot`, `Add`, `Sub`, `Mul`, `Div`, `Mod`, `Eq`, `Lt`, `Le`, `Gt`, `Ge`, `JumpIfFalse`, `JumpIfTrue`, `Jump`, `Return`, `Recur`, `BuiltinCall(name, nargs)`.
- `LoopCompiler` struct builds slot map (binding name → slot index) and emits ops. Also captures outer env variables at compile time into extra slots (placed after binding slots). `try_compile_loop()` attempts compilation; returns `None` for unsupported expressions (falls back to tree-walk). Signature: `try_compile_loop(binding_names, binding_vals, body, outer_env)`.
- `run_compiled_loop()` executes the bytecode with a value stack + slot array (binding slots + captured env slots). `Recur(N)` pops N args from stack into binding slots and jumps to loop start.
- `BuiltinCall` handles: `abs`, `min`, `max`, `to-string`, `str`, `car`, `cdr`, `cons`, `list`, `len`, `append`, `nth`, `nil?`, `list?`, `number?`, `string?`, `zero?`, `pos?`, `neg?`, `even?`, `odd?`, `mod`, `remainder`.
- **Variadic arithmetic/comparison**: `(+ a b c)` chains binary ops — compiles first arg, then for each remaining arg: compile + emit opcode. Supports 3+ args.
- **Nested if**: `(if test (if test2 a b) c)` compiles to nested jump instructions with proper patch-up.
- **Outer env capture**: `LoopCompiler` has `captured: Vec<(String, LispVal)>` — unknown symbols are looked up in `outer_env` at compile time and stored as extra slots. `(let ((x 10)) (loop ... (+ i x)))` compiles x into a captured slot.
- **and/or**: short-circuit with `Dup` + `JumpIfFalse`/`JumpIfTrue` + `Pop`. Returns first falsy/truthy or last value.
- **progn/begin**: evaluate all exprs, `Pop` intermediates, return last.
- **cond**: multi-branch via chained `JumpIfFalse`, each result jumps to end. Supports `(else ...)` final clause.
- **NOT supported in bytecode** (falls back to tree-walk): `let`/`try`/`match` (env mutation), `define`/`lambda` (scope creation), `quote`/`defmacro`/`macroexpand`.
- Compilation is attempted at `loop` form entry in `lisp_eval` — if `try_compile_loop` returns `None`, falls back to existing tree-walk interpreter seamlessly.
- Helper functions `num_val` (owned) and `num_val_ref` (&ref) for extracting i64 from LispVal in both VM (owned stack.pop) and tree-walk (borrowed) contexts.
- **On-chain benchmarks** (kampy.testnet):
  - 1-binding counting loop: ~0.74 Ggas/iter at scale (was ~22.45 Ggas/iter tree-walk)
  - 2-binding counting loop: ~1.54 Ggas/iter at scale
  - Marginal cost per extra binding: ~0.79 Ggas
  - Max iterations at 300 Tgas: ~401K (1-binding), ~194K (2-binding)
  - Baseline (no loop): 1.50 Tgas
  - **Peephole optimizer** fuses LoadSlot+PushI64+Arith/Cmp into SlotAddImm etc. (~3x speedup over unfused bytecode). Must remap jump targets after fusion — see "peephole jump remap" in On-Chain Bugs.

**P1 — loop/recur: push/pop instead of env clone** (tree-walk fallback):
- Every loop iteration cloned the entire env. Now pushes bindings directly into env, evaluates body, then truncates (pops) the bindings. Zero allocation per iteration.
- Impact: loop with 100 iterations on an env with 20 bindings goes from 100 × clone(20 entries) to 100 × push/pop.

**P1 — let/try/match: push/pop instead of env clone**:
- `let`, `try/catch`, and `match` all cloned env to create a local scope. Now uses push + truncate pattern — pushes bindings into env, evaluates body, then truncates back.
- Impact: eliminates env clone for every let, try, and match expression.

**P1 — hex_encode: lookup table instead of format! per byte**:
- Old: `bytes.iter().map(|b| format!("{:02x}", b)).collect()` — allocates a String per byte.
- New: pre-allocated String with capacity + lookup table push — one allocation total.
- Impact: significant for any code that serializes bytes (ccall args, crypto results).

**P1 — sandbox_key: documented as already fast**:
- `sandbox_key` uses `.rev().find()` to locate `__storage_prefix__`. Since it's pushed early at setup (near the end of the env vector), the reverse scan hits it in O(1) in practice.

**P0 — apply_lambda: push/pop instead of double-env-clone (DEPLOYED):**
- Old: `let mut local = closed_env.clone(); local.extend(caller_env.iter().cloned())` — clones BOTH envs every function call.
- New: pushes closed_env entries + params into caller_env, evals body, truncates. Only clones closed_env (typically small — just the capture scope), NOT caller_env (which grows unboundedly).
- Side effect: fixes lexical scoping — closed_env entries now shadow original caller_env entries (via `.rev().find()` order), instead of caller_env shadowing closed_env (dynamic scoping bug). Recursive bindings like `(define fib ...)` still work because `fib` is in the original caller_env portion, not in closed_env.
- Impact: `(map (lambda (x) (* x 2)) (list 1..100))` with 30-entry env goes from 100 × (30+30) = 6000 entry clones to 100 × 30 = 3000.

**NOT changed (and why):**
- `env` is still `Vec<(String, LispVal)>` with linear scan — switching to `HashMap` would break Borsh serialization for `VmState` (used in yield/resume). Would need a custom serializer.
- `apply_lambda` still clones `closed_env` entries — `Rc` doesn't implement BorshDeserialize. `Box<Vec<>>` is required for WASM. Could be eliminated with a two-env approach (threading a closure_env reference through lisp_eval) but that's a 77-call-site refactor.
- `range` already allocates a `Vec` but this is the standard pattern for on-chain iteration — lazy iterators don't help in a Lisp interpreter that materializes lists.

## Known Code Issues (as of audit 2025-04)

**RESOLVED — Lexical scoping in `apply_lambda` (line ~395):**
- closed_env is pushed AFTER caller_env, so `.rev().find()` correctly finds closed_env first
- Closures see their definition scope, not the call site — works as expected
- Note: caller_env IS still visible in the lambda body (all caller bindings leak in), but closed_env entries shadow them. Full fix would need removing caller_env, but that breaks recursive `(define fib (lambda ...))` without letrec-style support

**RESOLVED — `args[0]` panic in `\"error\"` handler:**
- Fixed: now uses `args.get(0)` with a safe fallback to `"error".to_string()` if no args provided
- Note: most builtins already used the safe `arg!(args, N, name)` macro. Only the `error` handler had the direct index panic

**RESOLVED — CEK machine for nested ccall support (code exists, NOT yet deployed)**:
- `run_program_with_ccall` and `run_remaining_with_ccall` now use `eval_loop` (CEK machine)
- Old `check_ccall` / `extract_ccall_info` / `CcallInfo` pre-flight scanner removed (~100 lines of dead code)
- `lisp_eval` still exists for synchronous evaluation (tests, REPL, require, loop body) — returns `Result<LispVal, String>`
- `eval_loop` returns `Result<EvalResult, String>` where `EvalResult` is `Done(LispVal)` or `Yield(CcallYieldInfo)`
- `VmState` has `kont: Vec<Frame>` field for continuation stack — populated on yield
- `CcallYieldInfo` has `kont: Vec<Frame>` — `eval_loop` pushes outer frames when yield bubbles up
- Old `pending_var` field is always `None` (Frame::Define in kont replaces it)
- **Resume via kont**: `resume_from_kont(ccall_result, kont, env, gas)` feeds ccall result through saved frames using `feed_value`, chaining through nested frames. `resume_eval` calls this before running remaining top-level exprs.
- **eval_args_loop arg preservation**: All yield points in `eval_args_loop` push `Frame::ArgEval { head, evaluated, remaining, saved_env }` onto kont. On resume, already-evaluated args are preserved and evaluation continues from where it stopped.
- **NOTE**: The CEK machine is in the repo but the currently deployed contract (`kampy.testnet`) still uses the old `check_ccall` pre-flight scanner. Deployed contract has top-level-only ccall restriction. CEK needs deploy + testing before it replaces the current flow.
- **SDK 5.6.0** (pinned): CANNOT bump to 5.24.1 — it requires rustc 1.88+ via `time` crate, but wasm32 target is pinned to 1.86.0. `promise_result` is NOT deprecated in 5.6.0 with `legacy` feature — zero warnings. When upgrading becomes possible: use `promise_result_checked(idx, max_len) → Result<Vec<u8>, PromiseError>`, `PromiseError` is `#[non_exhaustive]` — match `Failed`, `TooLong(usize)`, and wildcard `Err(_)`. `is_multiple_of()` is nightly-only — use `% n != 0` instead.

### CEK Machine Architecture (implemented)

The CEK (Control, Environment, Kontinuation) machine replaces the recursive evaluator for ccall-aware paths.

**Types:**
```rust
enum Step {
    Done(LispVal),
    EvalThen { expr: Box<LispVal>, env: Vec<(String, LispVal)>, frame: Box<Frame> },
    EvalArgs { head: Box<LispVal>, evaluated: Vec<LispVal>, remaining: Vec<LispVal>, env: Vec<(String, LispVal)> },
    YieldCcall(CcallYieldInfo),
}
pub enum Frame {
    If { then_branch: Box<LispVal>, else_branch: Option<Box<LispVal>> },
    Progn { remaining: Vec<LispVal> },
    Define { var_name: String },
    SetVar { var_name: String },
    Cond { body_forms: Vec<LispVal>, remaining_clauses: Vec<LispVal> },
    CondTestPassed { body_forms: Vec<LispVal>, test_val: Box<LispVal> },
    LetEvalBindings { binding_names: Vec<String>, remaining: Vec<LispVal>, evaluated: Vec<LispVal>, body_exprs: Vec<LispVal> },
    And { remaining: Vec<LispVal> },
    Or { remaining: Vec<LispVal> },
    TryExpr { catch_var: String, catch_body: Vec<LispVal> },
    MatchExpr { clauses: Vec<LispVal> },
    LoopEvalBindings { binding_names: Vec<String>, ... },
    LoopBody { binding_names: Vec<String>, body: Box<LispVal> },
    RecurEvalArgs { binding_count: usize, evaluated: Vec<LispVal>, remaining: Vec<LispVal> },
    ArgEval { head: Box<LispVal>, evaluated: Vec<LispVal>, remaining: Vec<LispVal>, saved_env: Vec<(String, LispVal)> },
    RequireModule,
}
enum EvalResult { Done(LispVal), Yield(CcallYieldInfo) }
struct CcallYieldInfo { account, method, args_bytes, deposit, gas_tgas, kont: Vec<Frame> }
```

**Key functions:**
- `eval_step(expr, env, gas)` → `Result<Step, String>` — one step of CEK evaluation
- `feed_value(val, frame, env, gas)` → `Result<Step, String>` — feed computed value into a continuation frame
- `eval_loop(expr, env, gas)` → `Result<EvalResult, String>` — main driver, chains steps until Done or Yield. On yield, pushes outer frames onto `info.kont`
- `eval_args_loop(head, evaluated, remaining, env, gas)` → `Result<Step, String>` — left-to-right arg evaluation

**Kont propagation on yield:** When `eval_loop` catches a `YieldCcall` bubbling up through an `EvalThen { frame }`, it pushes that frame onto `info.kont`. For nested yields (yield inside recursive eval_loop call), the inner call already pushed its frames, and the outer call appends its own frame. This builds the full continuation chain.

**What this enables:** ccalls can now appear at ANY nesting depth — inside `if`, `let`, `cond`, `progn`, `and`, `or`, lambda args, etc. The kont captures enough context to resume through those frames. `resume_from_kont` feeds the ccall result through saved frames, and `eval_args_loop` preserves already-evaluated args on yield.

**RESOLVED — `json_to_lisp` truncates floats:**
- Fixed: now checks the JSON number's string form for `.` or `e`/`E` to detect float syntax
- `3.0` → `LispVal::Float(3.0)` (preserves intent), `3` → `LispVal::Num(3)` (integer)
- Numbers too large for i64 that lack float syntax fall through to f64, then to string

**HIGH — String inconsistencies:**
- No escape sequences in strings (`\"`, `\n`, `\t` unsupported)
- `str-index-of` returns byte offset, `str-substring` uses char offset — breaks on UTF-8 (e.g. "café")
- `mod` inconsistent: int uses `rem_euclid` (non-negative), float uses `%` (can be negative)

**HIGH — `(=)` with zero args returns `true`** (None == None)
**MEDIUM — `mod` panics on division by zero** — FIXED: now returns error "mod by zero"
**MEDIUM — `let`/`cond` only evaluate single body expression** — FIXED: lambda/define shorthand now wrap multiple body forms in progn
**MEDIUM — `check_policy` only matches string `"true"`** — FIXED: now accepts any truthy value (numbers, non-empty lists, etc.)
**MEDIUM — `save_policy()` missing NEP-297 event** — FIXED: event emission was already present in code
**MEDIUM — `mod` panics on division by zero** — FIXED: now returns error "mod by zero"
**MEDIUM — inline lambda empty closed_env** — FIXED: now captures current env via `env.clone()` for lexical scoping
