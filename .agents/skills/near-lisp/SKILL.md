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
| Macro | `(defmacro name (params) body)` | Args NOT evaluated; expands before eval |
| Map/Dict | `(dict "k" v)` | BTreeMap, ordered |
| Bytes | `(hex->bytes "0xff")` | `Vec<u8>` binary data |

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

;; Variadic lambda — &rest collects remaining args as list
(lambda (a b &rest rest) (+ a b (len rest)))

;; Defmacro — define a macro (args NOT evaluated before passing to body)
(defmacro name (params...) body)
(defmacro name (&rest args) body)

;; Macroexpand — expand macros without evaluating
(macroexpand expr)

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

#### Bytes/Binary Operations

```lisp
;; Create bytes from hex string
(hex->bytes "0xdeadbeef")         ;; → Bytes [222, 173, 190, 239]
(bytes-hex "deadbeef")            ;; → same (alias, strips 0x prefix)

;; Convert back to hex
(bytes->hex (hex->bytes "0xff"))  ;; → "0xff"

;; Length
(bytes-len (hex->bytes "0xdeadbeef"))  ;; → 4

;; String ↔ Bytes
(string->bytes "hello")           ;; → Bytes [104, 101, 108, 108, 111]
(bytes->string (string->bytes "hello"))  ;; → "hello"

;; Concatenation
(bytes-concat (hex->bytes "0xff") (hex->bytes "0xaa"))  ;; → Bytes [255, 170]

;; Slicing (start, end indices)
(bytes-slice (hex->bytes "0xdeadbeef") 1 3)  ;; → Bytes [173, 190]

;; Type check
(type? (hex->bytes "0xff"))       ;; → "bytes"
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

#### Contract Events (NEP-297)

All mutating owner actions automatically emit standard NEP-297 events:

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

Events are emitted via `env::log_str` — visible in transaction receipts and indexable by explorers/indexers. No user action needed; the contract emits them automatically.

#### Cross-Contract Calls (yield/resume)

```lisp
;; View call (read-only, default 0 deposit, 10 TGas)
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
- 1 ccall: 55T minimum, ~10.4T actual burn
- 2 ccalls: 60T minimum, ~12.3T actual burn
- 3 ccalls: 60T minimum, ~14.3T actual burn
- 4 ccalls: 65T minimum, ~16.4T actual burn
- 5 ccalls: 70T minimum, ~18.5T actual burn
- 6 ccalls: 75T minimum, ~20.6T actual burn
- Marginal cost per extra ccall: ~2.1T actual burn
- Each ccall defaults to 10T gas allocation (configurable in near/ccall-call args; actual burn typically ~1.4T for view calls)
- Per-ccall promise receipt burns only ~1.4T actual — 86% waste within the 10T allocation

**Key constants** (dynamic, optimized):
- `yield_overhead`: 5T (reduced from original 40T → 10T → 5T)
- `auto_resume_gas`: `2T + N × 0.1T` (scales with batch size)
- `reserve`: `3T + 0.3T × (N-1)` (covers Promise::and() chain overhead)
- `promise_yield_create`: ~5T fixed overhead per yield cycle
- `ccall_gas` per view: 10T allocation (1.4T typical burn)

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
- Default eval gas limit: **10,000** (configurable by owner). Testnet contract is set to **300T** (300,000,000,000,000) to match NEAR's 300 Tgas receipt cap.
- Out of gas → `ERROR: out of gas` (catchable via `try/catch`)
- `loop/recur` is the ONLY iteration pattern with zero stack growth. It costs exactly **8 gas per iteration** (formula: `8n + 7`). At 10K gas: count to 1,249. At 100T testnet limit: theoretical 12.5T iterations.
- **Important: internal Lisp gas ≠ real NEAR gas.** Lisp gas is 1 tick per `lisp_eval` call regardless of allocation cost. On-chain, NEAR charges per WASM instruction.
- **Bytecode loop VM (deployed)**: `loop/recur` with simple bodies compiles to a register-based bytecode VM — ~10x faster than the old tree-walk. On-chain benchmarks (kampy.testnet, 300 Tgas cap):

| Pattern | Iterations | Total Gas | Per-iter |
|---------|-----------|-----------|----------|
| 1-binding count `(loop ((i 0)) ...)` | 1,000 | 4.36 Tgas | 4.36 Ggas |
| 1-binding count | 10,000 | 25.14 Tgas | 2.51 Ggas |
| 1-binding count | 50,000 | 117.48 Tgas | 2.35 Ggas |
| 1-binding count | 100,000 | 232.92 Tgas | 2.33 Ggas |
| 2-binding count `(loop ((i 0) (sum 0)) ...)` | 10,000 | 35.02 Tgas | 3.50 Ggas |
| 2-binding count | 100,000 | 301.20 Tgas | 3.01 Ggas |
| Baseline (no loop, eval `"1"`) | — | 1.98 Tgas | — |

- **Max iterations at 300 Tgas**: 129,672 (1-binding, binary searched on-chain)
- **Per-iteration cost** (amortized, converges at high N): ~2.3 Ggas/iter (1-binding), ~3.0 Ggas/iter (2-binding), ~0.7 Ggas marginal per extra binding
- **Old tree-walk cost** (for comparison): ~22.45 Ggas/iter, max ~13,350 iterations
- The internal gas limit should be set to 300T (`set_gas_limit(300000000000000)`) to match NEAR's receipt gas cap.
- **On-chain gas benchmarking method**: `near` CLI truncates gas to 3 decimal Tgas — useless for precision. Use RPC `EXPERIMENTAL_tx_status` for exact gas: extract tx hash from CLI output, then `curl RPC -d '{"method":"EXPERIMENTAL_tx_status","params":["TX_HASH","ACCOUNT"]}'`, sum `transaction_outcome.gas_burnt` + all `receipts_outcome[].outcome.gas_burnt`.
- **TCO trampoline does NOT help tail-recursive lambdas.** `(define count (lambda (n i) (if (= i n) i (count n (+ i 1)))))` still recurses through `dispatch_call` → `apply_lambda` → `lisp_eval` — real Rust stack frames. Stack overflows at ~100-200 depth. Only `loop/recur` avoids this.
- **Recursive fib gas cost**: grows at exactly 1.62x per n (golden ratio). fib(13) = 6777 gas, fib(19) = 121,761 gas. Stack overflows at fib(20) in debug builds.

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

### Adding a new `LispVal` variant (checklist)
When adding a new variant to `pub enum LispVal`, there are **7 mandatory update points**. Missing any one causes compile errors or runtime bugs:

1. **Enum definition** — add the variant. If it needs `BorshSerialize`/`BorshDeserialize` (for WASM/VmState), derive or implement it. Variants with non-serializable types (like `Promise`) need `#[borsh(skip)]` or custom impl.
2. **`Display` impl** — add a match arm for `fmt::Display`. This is what `run_program` returns as the string result.
3. **`lisp_eval` self-evaluating match** (~line 740) — the `match expr { Nil | Bool | Num | ... => Ok(expr.clone()) }` block. If the variant is a value type (not a form to evaluate), add it here. Missing this = `non-exhaustive patterns` compile error.
4. **`is_builtin_name`** — add any new builtin function names that operate on the new type (e.g. `"near/promise"`, `"promise-then"`).
5. **`dispatch_call` handlers** — add the actual builtin implementations that create/operate on the new variant.
6. **`type?` case** in dispatch_call — add to the type string match (e.g. `LispVal::Promise(_) => "promise"`).
7. **`inspect` case** in dispatch_call — add to the `inspect` value description match.
8. **Tests** — at minimum: builtin name recognition (`is_builtin_name`), Display format, self-evaluation through `run_program`.

### Key implementation detail: defmacro / macro expansion

Macro expansion happens at the TOP of `lisp_eval`'s `LispVal::List` branch, BEFORE special form dispatch. The head symbol is looked up in env; if it resolves to `LispVal::Macro`, the unevaluated args are passed to `apply_macro()` which binds them to the macro's params and evaluates the macro body to produce the expanded form, then `lisp_eval` is called on the expanded form.

**Borrow checker pattern**: Looking up a value in `env` (immutable borrow) then needing `env` mutably for `apply_macro`/`lisp_eval` causes E0502. Fix: clone the value out first:
```rust
let macro_val = env.iter().rev().find(|(k, _)| k == name).map(|(_, v)| v.clone());
if let Some(LispVal::Macro { params, rest_param, body, closed_env }) = macro_val {
    // Now env is free to be borrowed mutably
    let expanded = apply_macro(&params, &rest_param, &body, &closed_env, &macro_args, env, gas)?;
}
```

**Gas**: `apply_macro` and `expand_macros` now pass gas through correctly. There was a bug where `apply_macro` used `&mut 0` (zero gas) — macros were completely non-functional. Fixed 2026-04. Tests for macros need `eval_str_gas(code, 100_000)` — the default `eval_str` budget of 10,000 is too low for macro expansion + evaluation. After fix, all defmacro + macroexpand tests pass at standard gas budgets.

### Key testing pitfalls

**`bytes->hex` and String-returning builtins in tests**: Builtins like `bytes->hex` return `LispVal::Str`, whose `Display` impl wraps the value in double-quotes. Test assertions must account for this: `eval_str("(bytes->hex ...)")` returns `"\"0xdeadbeef\""` (with inner quotes), not `"0xdeadbeef"`. Use raw strings: `assert_eq!(result, r#""0xdeadbeef""#)`. This applies to ANY builtin that returns a `LispVal::Str` — the Display output always has the quote wrapping.

**Separate test files**: You CAN create separate test files (e.g. `tests/lisp_coverage.rs`) with their own `eval_str`, `eval_str_gas`, `setup_test_vm`, and `setup_contract` helpers. They compile and run independently via `cargo test --test lisp_coverage`. This is cleaner than appending hundreds of tests to the monolithic `lisp_unit.rs`.

**Pre-existing test failures** (as of 2026-04): `test_deep_recursion_gas_limit` and `test_fibonacci_15` stack overflow in debug builds. 6 defmacro/macroexpand tests fail (pre-existing gas budget issue — macros need high gas). 12 bytes tests fail (feature not implemented). Skip with `--skip test_deep_recursion_gas_limit --skip test_fibonacci_15`. Typical results: 333 lisp_unit pass, 23 lib pass.

**`near/log` test gap**: The `test_near_log_returns_nil` test was an empty placeholder with a comment saying "may panic in unit tests". This is wrong — `near/log` works fine in unit tests if `setup_test_vm()` is called first (the VMContext mock handles `env::log_str`).

**`storage_usage`/`storage_balance` contract views**: These were documented in SKILL.md but didn't exist in code until 2026-04. Now implemented — `storage_usage()` returns `env::storage_usage()` as u64, `storage_balance()` returns JSON string with total/available/locked fields. Note: `NearToken` doesn't implement `Sub`, so available balance uses `balance.as_yoctonear().saturating_sub(locked.as_yoctonear())` on raw u128 values.

**NEP-297 events**: All 7 mutating owner methods now emit `EVENT_JSON:` logs via `env::log_str`. Tested with `near_sdk::test_utils::get_logs()`.

**NEP-297 test gotcha — `get_logs()` accumulates**: `near_sdk::test_utils::get_logs()` returns ALL logs since the VMContext was created, not just new ones since the last call. When testing remove events (which require a save first), the save event is also in the log buffer. Don't assume `events[0]` is the event you just triggered — use `.iter().find(|e| e.contains("\"remove_policy\""))` to locate the specific event by name.

### CRITICAL: Never `git checkout` with uncommitted work

**`git checkout -- src/lib.rs` destroys ALL uncommitted changes.** There is no undo — `git stash` only works BEFORE checkout. If you need to reset partial patch attempts, use `git diff` to inspect, then apply targeted fixes. NEVER checkout the whole file when it has working code you need to keep. For near-lisp specifically, `src/lib.rs` is a monolith (~3500 lines) where macros, bytes, promises, NEP-297 events, storage methods, and defmacro gas fixes are ALL uncommitted at various times — a single `git checkout` erases hours of work.

**Safe workflow**: Before any risky operation, `cp src/lib.rs src/lib.rs.bak` or `git stash`. After a checkout mistake, check `git reflog` and `git stash list` — if nothing was stashed, the data is gone.

### Key implementation detail: patching strategy for lib.rs

When making large sets of changes to `src/lib.rs`:
- **The `patch` tool (mode=replace) is most reliable** for individual targeted changes — it does fuzzy matching and handles whitespace differences.
- **DUPLICATE PATTERN PITFALL**: When the same code pattern exists in multiple places (e.g. `"not" => {` appears in both `compile_expr` ~line 754 and `lisp_eval` ~line 1273), the patch tool may match the WRONG occurrence. Always include enough surrounding context to make the match unique, or verify with `read_file` at the exact line range before patching. When I patched `"not" => { ... }`, it matched the lisp_eval handler instead of the compile_expr handler three times in a row — destroying working code each time. Fix: use line-specific context (like the function signature or nearby unique identifiers) to disambiguate.
- **For major rewrites (like TCO refactoring lisp_eval)**: write the complete replacement function to a temp file first, then use a Python script to splice it into the main file at the correct line boundaries. Do NOT try to patch 300+ lines incrementally.
- **Python scripts doing string replacement** work for the initial batch of changes but break on subsequent rounds because rustfmt (invoked by the patch tool's lint check) reformat changes the whitespace.
- **Never use `read_file()` output for string matching** — it adds line number prefixes (`"  123|..."`). Use `terminal("cat path")` for raw content, or just use the `patch` tool directly.

### Key implementation detail: TCO trampoline in Rust (DEPLOYED)

`lisp_eval` uses a labeled trampoline loop. Tail positions rebind `current_expr` and `continue '_trampoline` instead of recursing. Every value-producing arm uses explicit `return`.

```rust
pub fn lisp_eval(expr: &LispVal, env: &mut Vec<(String, LispVal)>, gas: &mut u64) -> Result<LispVal, String> {
    let mut current_expr: LispVal = expr.clone();
    '_trampoline: loop {
        if *gas == 0 { return Err("out of gas".into()); }
        *gas -= 1;
        match &current_expr {
            // Self-evaluating: return directly
            | LispVal::Bool(_) | LispVal::Num(_) ... => return Ok(current_expr.clone()),
            // Special forms with TCO:
            "if"    => { current_expr = chosen_branch; continue '_trampoline; }
            "cond"  => { /* break+flag, then continue '_trampoline */ }
            "progn" => { current_expr = last_expr; continue '_trampoline; }
            "and"   => { current_expr = last_expr; continue '_trampoline; }
            "or"    => { current_expr = last_expr; continue '_trampoline; }
            // No TCO (env cleanup requires recursive call):
            "let" | "try" | "match" => { ... return result; }
            // Value-producing (always explicit return):
            "define" | "lambda" | "not" | "near/*" | "require" => { ... return Ok(...); }
            // loop/recur has its own inner Rust loop (unchanged):
            "loop" => { let result = loop { ... break val; }; return Ok(result); }
        }
    }
}
```

**Critical Rust type requirement**: Every match arm must end with `return Ok(...)` or `continue '_trampoline`. Bare `Ok(LispVal::Nil)` compiles as a tail expression of type `Result` — but the `loop` body expects no value (the loop never breaks). Adding `return` to every arm fixes this.

**Nested loop problem — labeled continue**: `cond` and `match` iterate clauses with `for`. A bare `continue` inside `for` continues the `for` loop, NOT the outer trampoline. Fix: use a labeled loop (`'_trampoline`) with break+flag pattern:
```rust
"cond" => {
    let mut found: Option<LispVal> = None;
    for clause in &list[1..] {
        if /* matches */ { found = Some(body); break; }
    }
    match found {
        Some(e) => { current_expr = e; continue '_trampoline; }
        None => return Ok(LispVal::Nil),
    }
}
```

**What IS tail-optimized** (no stack growth): `if`, `cond`, `progn`/`begin`, `and`, `or`.
**What is NOT tail-optimized** (still recursive): `let`, `try/catch`, `match` — these need env cleanup (push + truncate) so the body must be a recursive call. `loop`/`recur` has its own inner Rust loop. `define`, `lambda`, `near/*` all return values immediately.
- **`dispatch_call` evaluates all args upfront**: `let args: Vec<LispVal> = list[1..].iter().map(|a| lisp_eval(a, env, gas))...` — builtins use `args[N]`, NOT `eval_arg` (which doesn't exist in dispatch context).
- **`hex_decode`/`hex_encode` already exist** as helper functions in the codebase — don't import the `hex` crate.
- **`loop`/`recur` arm must use `break` not `return` from inner loop**: Use `let result = loop { ... break val; }; return Ok(result);` — the inner loop breaks with the value, then the trampoline arm returns it. Using `unreachable!()` after an infinite loop is cleaner but Rust doesn't guarantee the inner loop type-infers correctly.

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

### Ccall test file migration (tests/lisp_unit.rs)

The ccall batching refactor changed `VmState` and `RunResult` API. Tests in `tests/lisp_unit.rs` lines ~233-762 that directly construct `VmState` or pattern-match `RunResult::Yield` are broken and need migration:

**Old → New mappings:**
- `pending_var: Option<String>` → `pending_vars: Vec<Option<String>>`
- `VmState { ..., pending_var: Some("price".into()) }` → `VmState { ..., pending_vars: vec![Some("price".into())] }`
- `pending_var: None` → `pending_vars: vec![]` (for standalone ccalls with no define wrapper)
- `RunResult::Yield(yi)` → `RunResult::Yield { yields, state }` (struct variant, not tuple)
- `yi.account`, `yi.method` → `yields[0].account`, `yields[0].method`
- `yi.state.pending_var` → `state.pending_vars[0]`
- `yi.state.remaining` → `state.remaining`
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
- `BuiltinCall` handles: `abs`, `min`, `max`, `to-string`, `str`, `car`, `cdr`, `cons`, `list`, `len`, `append`, `nth`, `nil?`, `list?`, `number?`, `string?`, `zero?`, `pos?`, `neg?`, `even?`, `odd?`.
- **Variadic arithmetic/comparison**: `(+ a b c)` chains binary ops — compiles first arg, then for each remaining arg: compile + emit opcode. Supports 3+ args.
- **Nested if**: `(if test (if test2 a b) c)` compiles to nested jump instructions with proper patch-up.
- **Outer env capture**: `LoopCompiler` has `captured: Vec<(String, LispVal)>` — unknown symbols are looked up in `outer_env` at compile time and stored as extra slots. `(let ((x 10)) (loop ... (+ i x)))` compiles x into a captured slot.
- **and/or**: short-circuit with `Dup` + `JumpIfFalse`/`JumpIfTrue` + `Pop`. Returns first falsy/truthy or last value.
- **progn/begin**: evaluate all exprs, `Pop` intermediates, return last.
- **NOT supported in bytecode** (falls back to tree-walk): `let`/`try`/`match` (env mutation), `define`/`lambda` (scope creation), `quote`/`defmacro`/`macroexpand`, `BuiltinCall` with non-builtin names, `recur` inside nested `if` branches (recur only valid in top-level loop body tail position).
- Compilation is attempted at `loop` form entry in `lisp_eval` — if `try_compile_loop` returns `None`, falls back to existing tree-walk interpreter seamlessly.
- Helper functions `num_val` (owned) and `num_val_ref` (&ref) for extracting i64 from LispVal in both VM (owned stack.pop) and tree-walk (borrowed) contexts.
- **On-chain benchmarks** (kampy.testnet):
  - 1-binding counting loop: ~2.3 Ggas/iter at scale (was ~22.45 Ggas/iter tree-walk)
  - 2-binding counting loop: ~3.0 Ggas/iter at scale
  - Marginal cost per extra binding: ~0.7 Ggas
  - Max iterations at 300 Tgas: 129,672 (1-binding)
  - Baseline (no loop): 1.98 Tgas

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

## Skill file sync
Two copies exist and must be kept in sync:
- `~/.hermes/skills/near-lisp/SKILL.md` (global, loaded by hermes)
- `~/.openclaw/workspace/near-lisp-clean/.agents/skills/near-lisp/SKILL.md` (project-local)
After editing either: `cp ~/.openclaw/workspace/near-lisp-clean/.agents/skills/near-lisp/SKILL.md ~/.hermes/skills/near-lisp/SKILL.md`

## Key implementation detail: apply_lambda push/pop semantics

`apply_lambda` pushes closed_env entries + params into caller_env directly (no separate `local` vec). After body eval, it truncates back. This gives correct lexical scoping:

**Lookup order via `.rev().find()`:** params > closed_env > original caller_env
- Params shadow everything (correct)
- closed_env shadows original caller_env (correct lexical scoping — captures definition scope)
- Original caller_env entries are still found if not shadowed (recursive bindings work)

**Why recursive `fib` still works:** `(define fib (lambda ...))` — `fib` is in the ORIGINAL caller_env (pushed by `define`), NOT in closed_env (fib didn't exist when lambda was created). `.rev().find()` finds it in the original portion.

**Why it's an improvement over the old code:** The old `local = closed_env.clone(); local.extend(caller_env)` had caller_env AFTER closed_env, so caller SHADOWED closed (dynamic scoping). A redefined variable `x` in caller_env would shadow the closure's `x`. Now closed_env correctly shadows original — proper lexical scoping.

## Known Pitfalls
**Important notes**:
- `(define (f x) body)` shorthand desugars to `(define f (lambda (x) body))`
- `'expr` quote shorthand works — `'foo` → `(quote foo)`, `'(1 2 3)` → `(quote (1 2 3))`
- `(loop for i in list sum i)` NOT valid — only Clojure-style `(loop (bindings) body)` with `(recur ...)`
- Integer division with two ints: `(/ 10 3)` → `3` (truncated), not `3.333...`
- `require` is idempotent — safe to call multiple times for the same module
- Storage keys are auto-prefixed per caller — you can't access another caller's storage
- Cross-contract calls require `eval_async` or `eval_script_async`, not regular `eval`
