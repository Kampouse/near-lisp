---
name: near-lisp
description: On-chain Lisp interpreter for NEAR Protocol. Build, test, deploy, and write near-lisp programs — policies, rules, eval, cross-contract calls, storage, crypto. Use when working with the near-lisp project or writing on-chain Lisp for NEAR.
---

# near-lisp — On-Chain Lisp for NEAR Protocol

## When to use this skill

- Writing or debugging near-lisp programs (`.lisp` files)
- Building, testing, or deploying the near-lisp contract
- Creating on-chain policies or scripts
- Cross-contract call patterns with yield/resume
- Questions about near-lisp syntax, builtins, or semantics

## Project location

`~/.openclaw/workspace/near-lisp-clean/`

GitHub: `Kampouse/near-lisp`

## Build & Deploy

```bash
cd ~/.openclaw/workspace/near-lisp-clean

# One-command build + deploy (handles toolchain automatically)
make deploy

# Or build only
make build

# Local REPL
make repl
```

The Makefile uses `cargo near deploy --override-toolchain 1.86.0` which builds with the correct Rust version and deploys in one step. No manual toolchain switching needed.

### On-chain calls via make

```bash
make call CODE='(+ 1 2 3)'         # Eval Lisp on-chain
make view                          # View contract owner
make view-policies                 # List stored policies
make view-scripts                  # List stored scripts
make view-gas                      # View gas limit
make balance                       # Account balance
```

### All make targets

| Target | What it does |
|--------|-------------|
| `make build` | Build WASM only |
| `make deploy` | Build + deploy to testnet |
| `make test` | Run unit tests |
| `make test-sandbox` | Build + run sandbox tests |
| `make testnet` | Build + run testnet integration tests |
| `make call CODE=...` | Eval Lisp on-chain |
| `make repl` | Local REPL |
| `make clean` | Clean build artifacts |

## Testing

```bash
# All tests
cargo test

# Unit tests only (29, fast, ~3 min)
cargo test --lib

# Sandbox tests (23, real WASM on local sandbox, ~45 sec)
cargo test --test lisp_sandbox -- --nocapture

# Fuzz tests (7)
cargo test --test fuzz_test

# Benchmarks (2)
cargo test --test bench_gas --test bench_max_loop

# Testnet (needs live network)
export TESTNET_ACCOUNT_ID=your-account.testnet
export TESTNET_SECRET_KEY=***
cargo test --test lisp_testnet -- --nocapture
```

**Toolchain note**: Use `rustup override set 1.86.0` for WASM builds only. Unset (`rustup override unset`) before running tests — tests run with the default toolchain.

---

## Language Reference

### Types

| Type | Literal | Notes |
|------|---------|-------|
| Nil | `nil` | Falsy |
| Bool | `true`, `false` | `false` is falsy |
| Number | `42`, `-7` | i64 (64-bit signed integer) |
| Float | `3.14`, `0.5` | f64; triggers auto-promotion in mixed arithmetic |
| String | `"hello"` | UTF-8, double-quoted |
| Symbol | `foo`, `+` | Unevaluated identifier |
| List | `(1 2 3)` | Heterogeneous, `()` = nil |
| Lambda | `(lambda (x) body)` | Closure with captured env |
| Map/Dict | `(dict "k" v)` | BTreeMap, ordered |

**Truthiness**: Only `nil` and `false` are falsy. Zero, empty list, empty string are all truthy.

### Comments

```lisp
;; line comment (to end of line)
(; block comment ;)
```

### Special Forms

```lisp
;; Quote — return unevaluated
(quote expr)
;; ' shorthand: 'expr → (quote expr), '(1 2 3) → (quote (1 2 3))

;; Define — bind variable (evaluates expr)
(define name expr)
(define name)  ;; binds to Nil

;; If — conditional
(if cond then-expr else-expr)
(if cond then-expr)  ;; else defaults to Nil

;; Cond — multi-branch
(cond
  (test1 result1)
  (test2 result2)
  (else resultN))

;; Let — local bindings (evaluated sequentially)
(let ((x 1) (y (+ x 1))) (+ x y))

;; Lambda — create closure (captures current env)
(lambda (params...) body)

;; Progn / Begin — sequence, returns last
(progn e1 e2 e3)
(begin e1 e2 e3)

;; And / Or — short-circuit
(and e1 e2 e3)  ;; returns first falsy or last value
(or e1 e2 e3)   ;; returns first truthy or false

;; Not — boolean negation
(not expr)

;; Try/Catch — error handling
(try expr (catch error-var handler-body...))
;; On error, error-var binds to the error string

;; Match — pattern matching
(match expr
  (pattern1 result1)
  (pattern2 result2))

;; Loop/Recur — tail-call optimized iteration
(loop ((x 0) (y 1))
  (if (> x 10) y (recur (+ x 1) (+ x y))))
;; recur must be in tail position, arity must match loop bindings
```

**IMPORTANT syntax notes**:
- `(define (name params) body)` shorthand works — desugars to `(define name (lambda (params) body))`
- `'expr` quote shorthand works — `'foo` → `(quote foo)`, `'(1 2 3)` → `(quote (1 2 3))`
- `(loop for i in list sum i)` is NOT valid — only Clojure-style `(loop (bindings) body)` with `(recur args...)`

### Pattern Matching

```lisp
(match value
  (_ default-result)                  ;; wildcard
  (?x (* x 2))                        ;; binding (strips ? prefix)
  (42 "the answer")                   ;; literal match
  ("hello" greeting)                  ;; string literal match
  ((list ?a ?b) (list ?b ?a))         ;; list destructuring
  ((cons ?head ?tail) (len ?tail)))   ;; head/tail destructuring
```

Pattern types: `_` (wildcard), `?name` (binding), numeric/string/bool literals, `(list p1 p2 ...)`, `(cons head-pat tail-pat)`. No match returns `nil`.

---

### Built-in Functions

#### Arithmetic (2+ args, auto-promote to Float if any Float)

```lisp
(+ 1 2 3)       ;; → 6
(- 10 3 2)      ;; → 5
(* 6 7)         ;; → 42
(/ 10 3)        ;; → 3 (integer division with two ints)
(/ 10.0 3)      ;; → 3.333... (float if any float)
(mod 10 3)      ;; → 1
```

#### Comparison (2 args, auto-promote)

```lisp
(= a b)    (= = a b))  ;; structural equality
(!= a b)   (/= a b)    ;; inequality
(< > <= >=)            ;; numeric comparison
```

#### List Operations

```lisp
(list 1 2 3)         ;; → (1 2 3)
(car (list 1 2))     ;; → 1
(cdr (list 1 2))     ;; → (2)
(cons 0 (list 1 2))  ;; → (0 1 2)
(len (list 1 2 3))   ;; → 3
(append (list 1) (list 2))  ;; → (1 2)
(nth 1 (list 10 20 30))     ;; → 20 (zero-indexed)
```

#### Dict/Map Operations

```lisp
(define d (dict "name" "alice" "score" 95))
(dict/get d "name")         ;; → "alice"
(dict/has? d "score")       ;; → true
(dict/set d "level" 2)      ;; → new dict with 3 entries
(dict/keys d)               ;; → ("name" "score")
(dict/vals d)               ;; → ("alice" 95)
(dict/remove d "name")      ;; → new dict without "name"
(dict/merge d1 d2)          ;; → right-biased merge
```

#### String Operations

```lisp
(str-concat "hello" " " "world")  ;; → "hello world"
(str-contains "hello" "ell")      ;; → true
(str-length "hello")              ;; → 5
(str-substring "hello" 1 4)       ;; → "ell"
(str-split "a,b,c" ",")           ;; → ("a" "b" "c")
(str-trim "  hi  ")               ;; → "hi"
(str-index-of "hello" "ll")       ;; → 2
(str-upcase "hello")              ;; → "HELLO"
(str-downcase "HELLO")            ;; → "hello"
(str-starts-with "hello" "hel")   ;; → true
(str-ends-with "hello" "llo")     ;; → true
(str= a b)                        ;; string equality
(str!= a b)                       ;; string inequality
(to-string 42)                    ;; → "42"
```

#### Type Predicates

```lisp
(nil? x)      ;; → true if Nil
(list? x)     ;; → true if List
(number? x)   ;; → true if Num or Float
(string? x)   ;; → true if Str
(map? x)      ;; → true if Map
```

#### Type Conversions

```lisp
(to-float 42)     ;; → 42.0
(to-float "3.14") ;; → 3.14
(to-int 3.7)      ;; → 3
(to-int "42")     ;; → 42
(to-num "42")     ;; → 42 (alias for to-int)
```

#### Type Introspection

```lisp
(type? 42)              ;; → "number"
(type? "hello")         ;; → "string"
(type? true)            ;; → "boolean"
(type? nil)             ;; → "nil"
(type? (list 1 2))      ;; → "list"
(type? (dict "k" 1))    ;; → "map"
(type? (lambda (x) x))  ;; → "lambda"
```

#### Error Raising

```lisp
(error "something went wrong")  ;; → raises error (catchable with try/catch)
```

#### Crypto

```lisp
(sha256 "hello")             ;; → hex string
(keccak256 "hello")          ;; → hex string
(ed25519-verify sig msg pk)  ;; → bool
(ecrecover hash sig v flag)  ;; → hex pubkey or nil
```

#### JSON

```lisp
(to-json (list 1 "two" true))  ;; → [1,"two",true]
(from-json "{\"x\":42}")       ;; → map with x=42
;; null → Nil, object → Map, array → List
```

#### Formatting

```lisp
(fmt "Hello {name}, score: {score}" (dict "name" "alice" "score" 95))
;; → "Hello alice, score: 95"
```

---

### NEAR Builtins

#### Chain Info (special forms, no arg evaluation)

```lisp
(near/block-height)           ;; → Num (current block height)
(near/timestamp)              ;; → Num (nanoseconds)
(near/predecessor)            ;; → Str (caller account ID)
(near/signer)                 ;; → Str (signer account ID)
(near/account-balance)        ;; → Str (yoctoNEAR)
(near/account-locked-balance) ;; → Str (yoctoNEAR)
(near/attached-deposit)       ;; → Str (yoctoNEAR)
(near/log expr)               ;; logs to NEAR, returns Nil
```

#### Account Checks

```lisp
(near/signer= "alice.near")      ;; → bool
(near/predecessor= "alice.near") ;; → bool
```

#### Storage (100 gas per op, namespaced per caller)

```lisp
(near/storage-write "key" "value")  ;; → true
(near/storage-read "key")           ;; → "value" or nil
(near/storage-remove "key")         ;; → true
(near/storage-has? "key")           ;; → true/false
```

Keys are prefixed with `eval:{caller_account}:` for isolation.

#### Transfer

```lisp
(near/transfer "1000000" "recipient.near")  ;; yoctoNEAR string
```

#### Batch Calls

```lisp
(near/batch-call "contract.near"
  (list (list "method" "{\"arg\":1}" "0" "50")))
;; Each spec: (method args_json deposit_yocto gas_tgas)
```

#### Cross-Contract Calls (yield/resume)

```lisp
;; View call (read-only, default 0 deposit, 50 TGas)
(define price (near/ccall "oracle.near" "get_price" "{}"))

;; View call (explicit)
(near/ccall-view "ref.near" "get" "{}")

;; Call (mutable, requires deposit + gas)
(near/ccall-call "ref.near" "set" "{}" "1000000" "100")

;; Access results
(near/ccall-result)  ;; → last ccall result
(near/batch-result)  ;; → list of all accumulated results
(near/ccall-count)   ;; → count of results
```

**How ccalls work (batched)**: When `eval_async` encounters top-level ccalls, it pre-scans ALL consecutive ccalls and batches them into ONE yield cycle. N parallel cross-contract promises are created via `Promise::and()`, combined, then chained to a single `auto_resume_batch_ccall` callback. When all promises resolve, the callback borsh-serializes `Vec<Vec<u8>>` results and calls `promise_yield_resume`, which wakes `resume_eval` to inject all N results into the environment at once, then continues evaluating remaining expressions.

**Gas costs (batched, on testnet)**:
- 1 ccall: 100T minimum
- 2 ccalls: 152T minimum (~50T per extra ccall)
- 3 ccalls: 203T minimum
- Formula: ~100T base + N × ~50T per ccall
- Each ccall defaults to 10T gas (configurable in near/ccall-call args)

**Key constants**: `promise_yield_create` has ~40T fixed overhead per yield cycle. Auto-resume callback uses 5T. Reserve for current fn overhead is 10T.

---

### Standard Library Modules

Load with `(require "name")` — idempotent (skips if already loaded).

#### math

```lisp
(require "math")
(abs -5)        ;; → 5
(min 3 7)       ;; → 3
(max 3 7)       ;; → 7
(even? 4)       ;; → true
(odd? 3)        ;; → true
(gcd 12 8)      ;; → 4
(square 5)      ;; → 25
(pow 2 10)      ;; → 1024
(sqrt 16)       ;; → 4 (Newton's method via loop/recur)
(lcm 4 6)       ;; → 12
```

#### list

All functions below are **native builtins** — no `require "list"` needed. They work on-chain at near-zero gas cost.

```lisp
(empty? (list))              ;; → true
(map (lambda (x) (* x 2)) (list 1 2 3))    ;; → (2 4 6)
(filter (lambda (x) (> x 2)) (list 1 2 3)) ;; → (3)
(reduce + 0 (list 1 2 3))    ;; → 6  (raw builtin names work as values!)
(find (lambda (x) (> x 2)) (list 1 2 3))   ;; → 3
(some (lambda (x) (> x 2)) (list 1 2 3))   ;; → true
(every (lambda (x) (> x 0)) (list 1 2 3))  ;; → true
(reverse (list 1 2 3))       ;; → (3 2 1)
(sort (list 3 1 2))          ;; → (1 2 3)
(range 0 5)                  ;; → (0 1 2 3 4)
(zip (list 1 2) (list 3 4))  ;; → ((1 3) (2 4))
```

#### string

```lisp
(require "string")
(str-join ", " (list "a" "b" "c"))  ;; → "a, b, c"
(str-replace "hello" "l" "r")      ;; → "herro"
(str-repeat "ab" 3)                 ;; → "ababab"
(str-pad-left "5" 3 "0")            ;; → "005"
(str-pad-right "5" 3 "0")           ;; → "500"
```

#### crypto

```lisp
(require "crypto")
(hash/sha256-bytes "data")     ;; wraps sha256
(hash/keccak256-bytes "data")  ;; wraps keccak256
```

---

### Contract API

| Method | Type | Access | Description |
|--------|------|--------|-------------|
| `new(eval_gas_limit)` | init | private | Initialize contract (default gas limit: 10000) |
| `eval(code)` → String | call | whitelist | Eval Lisp, return result |
| `eval_with_input(code, input_json)` → String | call | whitelist | Eval with JSON vars injected |
| `eval_async(code)` → String | payable | whitelist | Async eval with ccall yield/resume |
| `check_policy(policy, input_json)` → bool | call | whitelist | Eval policy, return bool |
| `save_policy(name, policy)` | payable | owner | Store named policy |
| `get_policy(name)` → Option\<String\> | view | all | Retrieve policy |
| `list_policies()` → Vec\<String\> | view | all | List policy names |
| `remove_policy(name)` | call | owner | Delete policy |
| `eval_policy(name, input_json)` → String | call | whitelist | Eval stored policy |
| `save_script(name, code)` | payable | owner | Store named script |
| `get_script(name)` → Option\<String\> | view | all | Retrieve script |
| `list_scripts()` → Vec\<String\> | view | all | List script names |
| `remove_script(name)` | call | owner | Delete script |
| `eval_script(name)` → String | call | whitelist | Eval stored script |
| `eval_script_with_input(name, input_json)` → String | call | whitelist | Eval with input |
| `eval_script_async(name)` → String | payable | whitelist | Async eval with ccall |
| `save_module(name, code)` | payable | owner | Store custom module |
| `get_module(name)` → Option\<String\> | view | all | Retrieve module |
| `list_modules()` → Vec\<String\> | view | all | List module names |
| `remove_module(name)` | call | owner | Delete module |
| `set_gas_limit(limit)` | call | owner | Update eval gas limit |
| `get_gas_limit()` → u64 | view | all | Current gas limit |
| `get_owner()` → AccountId | view | all | Contract owner |
| `transfer_ownership(new_owner)` | call | owner | Transfer ownership |
| `add_to_eval_whitelist(account)` | call | owner | Whitelist account |
| `remove_from_eval_whitelist(account)` | call | owner | Remove from whitelist |
| `get_eval_whitelist()` → Vec\<AccountId\> | view | all | List whitelisted accounts |
| `storage_usage()` → u64 | view | all | Storage bytes |
| `storage_balance()` → String | view | all | JSON balance info |
| `resume_eval(yield_id)` → String | private | contract | Resume from yield |
| `auto_resume_batch_ccall(data_id_hex)` → String | private | contract | Batch ccall callback |

---

## Gas System

- Every `lisp_eval` call consumes **1 gas unit**
- Storage ops consume **additional 100 gas** each
- Default eval gas limit: **10,000** (configurable by owner)
- Out of gas → `ERROR: out of gas` (catchable via `try/catch`)
- `loop/recur` is more gas-efficient than naive recursion (no closure allocation per step)

## NEP-297 Events

All mutating owner actions emit NEP-297 standard events via `env::log_str`:

```
EVENT_JSON:{"standard":"near-lisp","version":"1.0.0","event":"<event>","data":{...}}
```

| Event | Data | Trigger |
|-------|------|---------|
| `save_policy` | `{"name":"..."}` | `save_policy()` |
| `remove_policy` | `{"name":"..."}` | `remove_policy()` |
| `save_script` | `{"name":"..."}` | `save_script()` |
| `remove_script` | `{"name":"..."}` | `remove_script()` |
| `save_module` | `{"name":"..."}` | `save_module()` |
| `remove_module` | `{"name":"..."}` | `remove_module()` |
| `transfer_ownership` | `{"old_owner":"...","new_owner":"..."}` | `transfer_ownership()` |

## Security Model

**Owner**: Set at init to `env::signer_account_id()`. Only owner can manage policies, scripts, modules, gas limit, whitelist, and ownership transfer.

**Eval Whitelist**: If empty (default), all callers can eval. If non-empty, only listed accounts can call eval methods.

**Storage Isolation**: Each caller's storage is prefixed with `eval:{caller_account}:`, preventing cross-caller access. The `__storage_prefix__` env var is pushed before user input so it can't be overridden.

**Private Methods**: `resume_eval` and `auto_resume_ccall` are `#[private]` — only the contract itself can call them.

## Use Cases

1. **Policy engine** — Store business rules on-chain, eval against JSON input
2. **Dynamic pricing** — Cross-contract calls to oracles, compute pricing logic
3. **Access control** — Evaluatable rules for who can do what
4. **Scriptable contracts** — Store reusable scripts, execute on demand
5. **On-chain computation** — Any evaluable Lisp with crypto, storage, and chain context

## Common Patterns

### Policy evaluation
```lisp
(check_policy
  "(and (>= score 85) (<= duration 3600))"
  "{\"score\": 90, \"duration\": 1200}")
;; → true
```

### Script with input
```lisp
;; Store: save_script("greet", "(fmt \"Hello {name}!\" input)")
;; Call: eval_script_with_input("greet", "{\"name\": \"world\"}")
;; → "Hello world!"
```

### Cross-contract oracle
```lisp
(define price (near/ccall "oracle.near" "get_price" "{\"pair\": \"NEAR/USD\"}"))
(define adjusted (+ (to-int price) 100))
(near/storage-write "adjusted_price" (to-string adjusted))
```

### Error-safe eval
```lisp
(try
  (/ 100 0)
  (catch e (near/log (str-concat "error: " e))))
```

## Implementation & Testing Notes

### How to test examples against the REPL
Pipe example files through `cargo run --bin repl`. NEAR env builtins (`near/block-height`, `near/transfer`, `near/storage-*`, etc.) will **panic** outside the contract runtime — filter those lines out before testing. The REPL uses `near_lisp::run_program()` directly with a persistent env across expressions.

### Adding new syntax to the interpreter
- **Tokenizer** (`tokenize()` ~line 150): single-char delimiters like `'` must be added to the `ch == '(' || ch == ')'` check, otherwise they get absorbed into the current token via `cur.push(ch)`
- **Parser** (`parse()` ~line 228): new token-level shorthands go in the match on `tok.as_str()` before the default case
- **Special forms** (in `lisp_eval()` ~line 508): add to the match on `name.as_str()` before the `_ => dispatch_call()` fallback
- **Builtins** (in `dispatch_call()` ~line 831): add to the match on `name.as_str()` before the `_ =>` lambda lookup fallback

### Key implementation detail: quote shorthand
Requires TWO changes: tokenizer must emit `'` as a standalone token (add to delimiter check), AND parser must handle `"'"` case to wrap in `(quote expr)`. Missing either one silently breaks.

### Key implementation detail: define shorthand
`(define (name params...) body)` desugars inline — creates the Lambda directly with `env.clone()` as `closed_env`, exactly matching what `lambda` does. Don't eval the body through `lisp_eval` first or you lose the closure.

### Key implementation detail: nil vs empty list
Empty list `()` is `LispVal::List(vec![])`, NOT `LispVal::Nil`. The `nil?` predicate now returns `true` for both Nil and empty List (fixed 2026-04). Stdlib functions that check for end-of-recursion with `(nil? lst)` now work correctly. Note for Rust code: can't use `LispVal::Nil | LispVal::List(ref v) if v.is_empty()` in a single matches! — Rust E0408 (variable not bound in all patterns). Use two separate `matches!` arms joined with `||`.

### Key implementation detail: IIFE / computed lambda heads
`((make-adder 5) 3)` now works. When `dispatch_call` encounters a `List` as the call head, it evaluates it via `lisp_eval(head, env, gas)` first, then passes the result to `call_val()`. This handles both literal inline lambdas and computed lambdas (function calls that return lambdas). The old code assumed any List head was a literal `(lambda ...)` form and rejected anything else with "inline lambda too short".

### Key implementation detail: match flat syntax
`match` now supports two syntaxes: clause form `(match expr (pat1 res1) (pat2 res2))` and flat form `(match expr pat1 res1 pat2 res2)`. Both the `lisp_eval` handler and the CEK `Frame::MatchExpr` handler use a while loop with index — List clauses advance by 1, bare atom clauses advance by 2 (pattern + result pair).

### Key implementation detail: stdlib lambda gas cost (RESOLVED)
Lisp-defined stdlib functions (via `require "list"`) were gas-prohibitive on-chain. Root cause: every lambda call clones the env, resolves the closure, and re-enters the full eval pipeline. **Fixed**: all list stdlib functions are now native Rust builtins in `dispatch_call`. `range`, `reverse`, `sort`, `zip`, `empty?` iterate with pure Rust loops (near-zero overhead). Higher-order functions (`map`, `filter`, `reduce`, `find`, `some`, `every`) iterate in Rust and call user lambdas via `call_val` per element — only the user lambda pays gas, the outer loop is free. Builtin names are first-class values via `is_builtin_name()` — `(reduce + 0 (list 1 2 3))` works without lambda wrapping. Env bindings shadow builtins (env checked first in `lisp_eval`).

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
- The ~50T per-ccall cost is ~80% NEAR runtime overhead (promise_yield_create ~40T, deferred receipt execution, promise scheduling) — NOT Lisp computation. Gas optimization should focus on reducing ccall_gas default (10T → 3-5T for views), auto_resume_gas (5T → 3T), and reserve_gas (10T → 5T). Sandbox benchmarks exist in `tests/bench_micro.rs` and `tests/bench_gas_sandbox.rs`.

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

Runs 129 tests against the live testnet contract. Uses `near contract call-function as-transaction` with inline JSON args. Returns pass/fail per test with summary.

**Important**: Use `prepaid-gas '100 Tgas'` minimum for tests involving `require` — 30 Tgas is too low for stdlib loading + computation. The `near` CLI does NOT support `file://` scheme for `json-args` — must use inline JSON strings.

## On-Chain Bugs — Status (2026-04)

**FIXED — `nil?` vs empty list**: `(nil? (list))` now returns `true`. The fix adds `LispVal::List(ref v) if v.is_empty()` as a second matches! arm (can't combine with Nil in one pattern due to Rust E0408 — variable not bound in all patterns). This fixes ALL recursive list functions that use `(nil? lst)` or `(empty? lst)` as base case.

**FIXED — `match` now supports flat syntax**: `(match 42 _ 99)` now works alongside the original clause syntax `(match 42 (_ 99))`. Both `lisp_eval` and the CEK trampoline `Frame::MatchExpr` handler were updated to walk clauses with a `while i < clauses.len()` loop — if a clause is a List, use clause form and advance by 1; if it's a bare atom, treat it as flat pattern-result pair and advance by 2.

**FIXED — IIFE**: `((make-adder 5) 3)` now works. The fix replaces the rigid "must be literal lambda form" check in `dispatch_call` with `lisp_eval(head, env, gas)?` + `call_val()`. This handles both literal inline lambdas `((lambda (x) ...) 5)` and computed lambdas `((make-adder 5) 3)` — lisp_eval resolves both to LispVal::Lambda, then call_val dispatches it.

**MEDIUM — `check_policy` not callable from eval**: `check_policy` is a contract method, not a Lisp function. Use `eval_policy(name, input_json)` for stored policies, or call `check_policy` as a contract method directly via `near contract call-function`.

**FIXED — List stdlib native builtins**: All list stdlib functions (map, filter, reduce, range, sort, reverse, zip, find, some, every, empty?) are now native Rust builtins in `dispatch_call`. Gas cost dropped from 100+ Tgas (Lisp lambda) to 0.309 Tgas (native Rust loop). `(range 0 100)` costs the same as `(range 0 3)`. No `require "list"` needed — they shadow the Lisp stdlib definitions automatically. Higher-order builtins (map, filter, reduce, find, some, every) call user-provided lambdas via `call_val` per element — the outer iteration is free, only the user's lambda pays gas per call.

**FIXED — First-class builtin functions**: `+`, `-`, `*`, `/`, `=`, `<`, `>`, `<=`, `>=`, `!=`, `str-concat`, and all other `dispatch_call` builtins are now first-class callable values. `lisp_eval` checks `is_builtin_name()` — if a symbol is a builtin and not in the env, it evaluates to itself (the Sym). `call_val` then dispatches it through `dispatch_call`. This enables `(reduce + 0 (list 1 2 3))` without wrapping in lambda. **Env bindings shadow builtins**: `(define + -)` then `(+ 5 3)` → `2`.

**FIXED — `Rc<Vec>` → `Box<Vec>` for closed_env**: `LispVal::Lambda::closed_env` was `Rc<Vec<...>>` which doesn't implement `BorshDeserialize`, blocking WASM builds. Changed to `Box<Vec<...>>` — Borsh works, WASM builds clean. `apply_lambda` takes `&Vec<...>` now (was `&Rc<Vec<...>>`). `call_val` passes `&**closed_env` via auto-deref coercion.

**FIXED — ccall result JSON double-encoding**: `resume_eval` was injecting raw `promise_result` bytes as `LispVal::Str`, so JSON-encoded returns (e.g. `"kampy.testnet"` with quotes) became double-quoted Lisp strings. Fixed by parsing through `serde_json::from_str` → `json_to_lisp`, which unwraps JSON types into proper Lisp types (strings without quotes, numbers as Num, arrays as List, objects as Map). Falls back to raw string if JSON parse fails.

**FIXED — Multi-ccall batched yield/resume (deployed)**: ccalls in `eval_async` only work at the TOP expression level or inside `(define var (near/ccall ...))`. They are pre-flight detected by `check_ccall()`, NOT handled at runtime in `dispatch_call`. Placing them inside `progn`, `let`, `if`, or any nested form results in `"undefined: near/ccall-view"`. Multiple consecutive top-level ccalls are now batched into a single yield cycle via `Promise::and()` — all N ccalls run in parallel, one callback collects all results. Gas: 100T base + ~50T per extra ccall. 2 ccalls = 152T, 3 ccalls = 203T. All 21 on-chain tests pass.

**MEDIUM — `type?` undefined when called directly**: `type?` is listed in `is_builtin_name()` (for first-class value support) but has NO handler in the `dispatch_call()` match arms. Calling `(type? 42)` goes through the dispatch fallback which looks up in env → fails → "undefined: type?". Fix: add `"type?"` to the `dispatch_call` match alongside other type predicates (`nil?`, `list?`, `number?`, `string?`, `map?`).

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

## Skill file sync
Two copies exist and must be kept in sync:
- `~/.hermes/skills/near-lisp/SKILL.md` (global, loaded by hermes)
- `~/.openclaw/workspace/near-lisp-clean/.agents/skills/near-lisp/SKILL.md` (project-local)
After editing either: `cp ~/.openclaw/workspace/near-lisp-clean/.agents/skills/near-lisp/SKILL.md ~/.hermes/skills/near-lisp/SKILL.md`

## Key implementation detail: why caller_env must stay in apply_lambda
The env uses Vec + `.rev().find()` (last-wins semantics). closed_env is pushed AFTER caller_env, so closed_env already shadows caller_env correctly. BUT: you can't remove caller_env entirely because:
- `(define fib (lambda ...))` — lambda captures env at creation time via `env.clone()`. At that point, `fib` is NOT in env yet (define pushes after eval). Recursion only works because caller_env leaks `fib` in.
- Placeholder pattern (push Nil first, create lambda, update) fails because lambda's `closed_env` copies `env.clone()` at creation time — it captures `fib = Nil`, not the real lambda. Updating the env after doesn't retroactively update the closed_env copy.
- Full fix requires either Rc<RefCell> for shared mutable env, or a letrec-style separate pass.

## Known Pitfalls
**Important notes**:
- `(define (f x) body)` shorthand desugars to `(define f (lambda (x) body))`
- `'expr` quote shorthand works — `'foo` → `(quote foo)`, `'(1 2 3)` → `(quote (1 2 3))`
- `(loop for i in list sum i)` NOT valid — only Clojure-style `(loop (bindings) body)` with `(recur ...)`
- Integer division with two ints: `(/ 10 3)` → `3` (truncated), not `3.333...`
- `require` is idempotent — safe to call multiple times for the same module
- Storage keys are auto-prefixed per caller — you can't access another caller's storage
- Cross-contract calls require `eval_async` or `eval_script_async`, not regular `eval`
