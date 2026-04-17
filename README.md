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
# Build WASM (requires Rust 1.86.0 + cargo-near)
rustup override set 1.86.0
rustup target add wasm32-unknown-unknown --toolchain 1.86.0
cargo near build non-reproducible-wasm --no-abi
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

## Gas safety

The interpreter has an internal gas counter that decrements on every eval step. Storage operations cost 100 gas each. When gas hits zero, evaluation stops with `ERROR: out of gas`. This prevents infinite loops from burning real NEAR gas.

## Security

- **Storage prefix shadowing** — `__storage_prefix__` in user input cannot override the safe namespace
- **Eval whitelist** — owner can restrict who can call eval (empty = open access)
- **VM state GC** — serialized VM state is cleaned up after resume
- **Storage gas metering** — every storage op costs gas, preventing storage abuse

## Language reference

- **Types:** nil, bool, number (i64), float (f64), string, symbol, list, lambda, dict/map
- **Special forms:** quote, define, if, cond, let, lambda, progn, begin, and, or, not, loop/recur
- **Arithmetic:** +, -, *, /, mod
- **Comparison:** =, !=, <, >, <=, >= (works with mixed int/float)
- **List ops:** list, car, cdr, cons, len, append, nth
- **Dict ops:** dict, dict/get, dict/has?, dict/set, dict/remove, dict/keys, dict/vals, dict/merge
- **String ops:** str-concat, str-contains, str-length, str-substring, str-split, str-trim, str-index-of, str-upcase, str-downcase, str-starts-with, str-ends-with, to-string
- **JSON:** to-json, from-json
- **Crypto:** sha256, keccak256
- **Predicates:** nil?, list?, number?, string?, map?
- **Conversion:** to-float, to-int, to-string
- **NEAR storage:** near/storage-read, near/storage-write, near/storage-remove, near/storage-has?
- **NEAR chain:** near/block-height, near/predecessor, near/signer, near/signer=, near/predecessor=, near/timestamp, near/log, near/account-balance, near/account-locked-balance, near/attached-deposit, near/transfer
- **Cross-contract:** near/ccall, near/ccall-view, near/ccall-call, near/ccall-result, near/batch-result, near/ccall-count
- **Modules:** require "math", require "list", require "string", require "crypto"

## License

MIT
