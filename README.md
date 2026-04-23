# near-lisp

On-chain Lisp interpreter for NEAR Protocol. Eval Lisp expressions, define policies, cross-contract calls, crypto, storage — all on-chain.

## What it does

A NEAR smart contract that runs a Lisp interpreter inside the VM. Use it to:

- **Eval arbitrary Lisp** — arithmetic, lambdas, closures, recursion, loop/recur
- **Policy engine** — store rules on-chain, eval them against JSON input
- **Cross-contract calls** — yield/resume with near/ccall, multi-call chains
- **Crypto** — sha256, keccak256 on-chain
- **Storage** — namespaced read/write/remove with gas metering
- **Standard library** — require modules for math, list, string, crypto
- **NEAR interop** — balances, transfers, signer checks, batch tracking

## Quick start

```bash
# Build WASM (requires Rust 1.86.0 — NEAR rejects WASM from newer rustc)
rustup override set 1.86.0
rustup target add wasm32-unknown-unknown --toolchain 1.86.0
cargo build --target wasm32-unknown-unknown --release
wasm-opt -Oz --strip-debug --signext-lowering \
  -o target/near/near_lisp.wasm \
  target/wasm32-unknown-unknown/release/near_lisp.wasm
rustup override unset

# Or use the REPL locally
cargo run --bin repl
```

## Test

```bash
# All tests (203 tests)
cargo test

# Unit tests only (162 tests, fast)
cargo test --test lisp_unit

# Sandbox tests (real WASM on local sandbox)
cargo test --test lisp_sandbox -- --nocapture

# Fuzz tests (panic safety + arithmetic invariants)
cargo test --test fuzz_test

# Benchmark gas and max loop
cargo test --test bench_gas --test bench_max_loop

# Testnet (real NEAR testnet)
export TESTNET_ACCOUNT_ID=your-account.testnet
export TESTNET_SECRET_KEY=***
cargo test --test lisp_testnet -- --nocapture
```

## Usage examples

```lisp
;; Arithmetic
(+ 1 2)              ;; → 3
(* 6 7)              ;; → 42

;; Floats (f64)
(+ 1.5 2.5)          ;; → 4.0
(to-float 42)        ;; → 42.0
(to-int 3.7)         ;; → 3

;; Lambda + closure
(define square (lambda (n) (* n n)))
(square 9)           ;; → 81

(define make-adder (lambda (n) (lambda (x) (+ n x))))
(define add10 (make-adder 10))
(add10 32)           ;; → 42

;; Recursion
(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
(fib 10)             ;; → 55

;; Loop/recur (TCO-safe)
(loop for i in (list 1 2 3 4 5) sum i)   ;; → 15
(loop for i in (list 1 2 3) collect (* i i))  ;; → (1 4 9)

;; Dicts
(define d (dict "name" "alice" "score" 95))
(dict/get d "name")         ;; → "alice"
(dict/has? d "score")       ;; → true
(dict/set d "level" 2)      ;; → dict with 3 entries

;; String ops
(str-length "hello")        ;; → 5
(str-split "a,b,c" ",")     ;; → ("a" "b" "c")
(str-upcase "hello")        ;; → "HELLO"
(str-index-of "hello" "ll") ;; → 2

;; JSON
(to-json (list 1 "two" true))   ;; → [1,"two",true]
(from-json "{\"x\":42}")        ;; → map with x=42

;; Standard library
(require "math")
(abs -5)                ;; → 5
(even? 4)               ;; → true

(require "list")
(map (lambda (x) (* x 2)) (list 1 2 3))    ;; → (2 4 6)
(filter (lambda (x) (> x 2)) (list 1 2 3))  ;; → (3)
(reduce (lambda (a b) (+ a b)) 0 (list 1 2 3))  ;; → 6

(require "crypto")
(sha256 "hello")        ;; → "2cf24dba5fb..."

;; Comments
;; this is a line comment
(; this is a
   block comment ;)

;; Cross-contract call (yields to runtime)
(define price (near/ccall "oracle.near" "get_price" "{}"))
(+ price 10)

;; View vs call (with deposit + gas)
(near/ccall-view "ref.near" "get" "{}")
(near/ccall-call "ref.near" "set" "{}" "1000000" "100")

;; NEAR builtins
(near/block-height)          ;; → 246078789
(near/predecessor)           ;; → "alice.testnet"
(near/account-balance)       ;; → yoctoNEAR string
(near/account-locked-balance)
(near/attached-deposit)
(near/transfer "1000" "alice.near")

;; Storage (namespaced, gas-metered)
(near/storage-write "key" "value")  ;; → true
(near/storage-read "key")           ;; → "value"
(near/storage-remove "key")         ;; → true
(near/storage-has? "key")           ;; → true

;; Policy evaluation
(check_policy "(and (>= score 85) (<= duration 3600))" "{\"score\": 90, \"duration\": 1200}")
;; → true
```

## Contract API

| Method | Params | Description |
|--------|--------|-------------|
| `new` | `eval_gas_limit: u64` | Initialize contract |
| `eval` | `code: String` | Eval Lisp, return result as string |
| `eval_with_input` | `code: String, input_json: String` | Eval with JSON vars injected |
| `check_policy` | `policy: String, input_json: String` | Eval policy, return bool |
| `save_policy` / `get_policy` / `remove_policy` / `list_policies` | name-based CRUD | On-chain policy store |
| `save_script` / `get_script` / `remove_script` / `list_scripts` | name-based CRUD | On-chain script store |
| `eval_policy` | `name, input_json` | Eval a saved policy |
| `eval_script` / `eval_script_with_input` | name-based | Run a saved script |
| `set_gas_limit` / `get_gas_limit` | limit | Gas management |
| `transfer_ownership` / `get_owner` | AccountId | Owner management |
| `add_to_eval_whitelist` / `remove_from_eval_whitelist` / `get_eval_whitelist` | AccountId | Eval access control |

### Autonomous execution (callback pattern)

The callback methods enable fully autonomous on-chain computation — one call in, one callback out. No polling or orchestration needed.

| Method | Params | Description |
|--------|--------|-------------|
| `eval_async_with_callback` | `code, callback_account, callback_method` | Run Lisp, deliver result to `callback_account.callback_method` |
| `eval_script_async_with_callback` | `name, callback_account, callback_method` | Same, but runs a stored script |

**How it works:**

1. Agent calls `eval_async_with_callback("(define price ...)", "agent.testnet", "on_result")`
2. kampy runs the code — if ccalls are needed, it yields/resumes automatically
3. When done, kampy fires a cross-contract call to `agent.testnet.on_result(result_string)`

The callback fires immediately if no ccalls are needed, or after the yield/resume cycle completes for async computations. The caller's `on_result` method receives the result string as raw bytes.

```lisp
;; Agent kicks off autonomous computation
eval_async_with_callback(
  "(define p (near/ccall-view \"ref.near\" \"get_price\" \"{}\"))
   (if (> (to-num p) 1000) \"sell\" \"hold\")",
  "agent.testnet",
  "execute"
)
;; → kampy handles the ccall, computes, then calls agent.testnet.execute("sell")
```

## Gas safety

The interpreter has an internal gas counter that decrements on every eval step. Storage operations cost 100 gas each. When gas hits zero, evaluation stops with `ERROR: out of gas`. This prevents infinite loops from burning real NEAR gas.

## Security

- **Storage prefix shadowing** — `__storage_prefix__` in user input cannot override the safe namespace
- **Eval whitelist** — owner can restrict who can call eval (empty = open access)
- **VM state GC** — serialized VM state is cleaned up after resume
- **Storage gas metering** — every storage op costs gas, preventing storage abuse

## Language reference

### Types
`nil`, `true`/`false` (bool), number (i64), float (f64), string, symbol, list, lambda, dict/map

### Special forms
| Form | Description |
|------|-------------|
| `(quote x)` / `'x` | Return x unevaluated |
| `(define name expr)` | Bind name in current env |
| `(if cond then else?)` | Conditional |
| `(cond (test1 expr1) (test2 expr2) ... (else exprN))` | Multi-branch conditional |
| `(let ((x 1) (y 2)) body)` | Local bindings |
| `(lambda (params) body)` | Anonymous function |
| `(progn expr1 expr2 ...)` / `(begin ...)` | Evaluate sequence, return last |
| `(and a b)` / `(or a b)` | Short-circuit logic |
| `(not x)` | Logical negation |
| `(loop for x in lst collect expr)` | Loop with collect/sum/count |
| `(recur args...)` | Tail-call within loop |
| `(match expr (pattern body) ...)` | Pattern matching |

### Error handling
| Function | Description |
|----------|-------------|
| `(try expr (catch e fallback))` | Catch errors |
| `(catch expr handler)` | Catch errors (simpler form) |
| `(error msg)` | Raise error |

### Arithmetic
| Function | Description |
|----------|-------------|
| `(+ a b ...)` | Addition (2+ args) |
| `(- a b)` | Subtraction |
| `(* a b ...)` | Multiplication (2+ args) |
| `(/ a b)` | Division |
| `(mod a b)` | Modulo |

### Comparison
| Function | Description |
|----------|-------------|
| `(= a b)` | Equal (mixed int/float) |
| `(!= a b)` | Not equal |
| `(< a b)` / `(> a b)` / `(<= a b)` / `(>= a b)` | Ordering |

### List operations
| Function | Description |
|----------|-------------|
| `(list 1 2 3)` | Create list |
| `(car lst)` | First element |
| `(cdr lst)` | Rest of list |
| `(cons x lst)` | Prepend element |
| `(len lst)` | Length |
| `(append lst1 lst2)` | Concatenate lists |
| `(nth i lst)` | Element at index |
| `(range start end)` | List of integers |
| `(reverse lst)` | Reverse list |
| `(sort lst)` | Sort ascending |
| `(zip lst1 lst2)` | Interleave two lists |

### Higher-order functions
| Function | Description |
|----------|-------------|
| `(map fn lst)` | Apply fn to each element |
| `(filter fn lst)` | Keep elements where fn returns true |
| `(reduce fn init lst)` | Fold left |
| `(find fn lst)` | First element matching predicate |
| `(every fn lst)` | True if all elements match |

### Dict operations
| Function | Description |
|----------|-------------|
| `(dict "k1" v1 "k2" v2)` | Create dict |
| `(dict/get d "key")` | Get value (nil if missing) |
| `(dict/set d "key" val)` | Return new dict with key set |
| `(dict/remove d "key")` | Return new dict with key removed |
| `(dict/keys d)` | List of keys |
| `(dict/vals d)` | List of values |
| `(dict/merge d1 d2)` | Merge two dicts |

### String operations
| Function | Description |
|----------|-------------|
| `(str-concat a b ...)` | Concatenate strings (accepts numbers) |
| `(str-length s)` | String length |
| `(str-substring s start end)` | Substring |
| `(str-split s sep)` | Split by separator |
| `(str-trim s)` | Trim whitespace |
| `(str-contains s substr)` | Contains substring |
| `(str-index-of s substr)` | Index of substring |
| `(str-starts-with s prefix)` | Starts with prefix |
| `(str-ends-with s suffix)` | Ends with suffix |
| `(str-upcase s)` | Uppercase |
| `(str-downcase s)` | Lowercase |

### Type predicates & conversions
| Function | Description |
|----------|-------------|
| `(nil? x)` / `(list? x)` / `(number? x)` / `(string? x)` / `(map? x)` | Type checks |
| `(to-float x)` | Convert to float |
| `(to-int x)` | Convert to integer |
| `(to-num x)` | Convert to number (int or float) |
| `(to-string x)` | Convert to string |
| `(inspect x)` | Debug representation |

### JSON
| Function | Description |
|----------|-------------|
| `(from-json s)` / `(json-parse s)` | Parse JSON string → Lisp value |
| `(to-json val)` / `(json-build val)` | Lisp value → JSON string |
| `(json-get s key)` | Parse JSON string, extract key |
| `(json-get-in s k1 k2 ...)` | Parse JSON string, extract nested path |

### Crypto
| Function | Description |
|----------|-------------|
| `(sha256 s)` | SHA-256 hash |
| `(keccak256 s)` | Keccak-256 hash |
| `(ed25519-verify sig msg pubkey)` | Ed25519 signature verification |
| `(ecrecover sig msg)` | Ethereum address recovery |

### NEAR chain data
| Function | Description |
|----------|-------------|
| `(near/block-height)` | Current block height |
| `(near/timestamp)` | Block timestamp (nanoseconds) |
| `(near/predecessor)` | Calling account ID |
| `(near/signer)` | Signing account ID |
| `(near/predecessor= "alice.near")` | Check predecessor (gas-efficient) |
| `(near/signer= "alice.near")` | Check signer (gas-efficient) |
| `(near/account-balance)` | Contract balance in yoctoNEAR |
| `(near/account-locked-balance)` | Locked balance |
| `(near/attached-deposit)` | Attached deposit amount |
| `(near/log msg)` | Emit log event |
| `(near/log-debug msg)` | Emit debug log (only in debug builds) |

### NEAR storage
| Function | Description |
|----------|-------------|
| `(near/storage-read "key")` | Read from contract storage (namespaced) |
| `(near/storage-write "key" "val")` | Write to contract storage |
| `(near/storage-remove "key")` | Remove key from storage |
| `(near/storage-inc "key" delta)` | Atomic read-increment-write |

### NEAR cross-contract calls
| Function | Description |
|----------|-------------|
| `(near/ccall "acct" "method" "args")` | Cross-contract call (yields/resumes) |
| `(near/ccall-view "acct" "method" "args")` | View call (returns result or nil on error) |
| `(near/ccall-call "acct" "method" "args" deposit gas)` | Mutation call with deposit |
| `(near/ccall-result)` | Get result of last ccall |
| `(near/ccall-count)` | Number of pending ccalls |
| `(near/batch-result)` | Get batch execution result |
| `(near/batch-call "acct" "method" "args" deposit gas)` | Batched call |
| `(near/transfer amount "recipient")` | Transfer NEAR |

### Modules
```lisp
(require "math")    ;; abs, min, max, even?, odd?, gcd, pow, sqrt, floor, ceil, round
(require "list")    ;; map, filter, reduce, find, every, reverse, sort, zip, range
(require "string")  ;; enhanced string ops
(require "crypto")  ;; sha256, keccak256, hex helpers
```

## License

MIT
