# near-lisp

On-chain Lisp interpreter for NEAR Protocol. 158KB WASM. Eval Lisp expressions, define policies, read/write storage — all on-chain.

## What it does

A NEAR smart contract that runs a Lisp interpreter inside the VM. Use it to:

- **Eval arbitrary Lisp** — arithmetic, lambdas, closures, recursion
- **Policy engine** — store rules on-chain, eval them against JSON input
- **NEAR builtins** — access block height, predecessor, signer, timestamp
- **Storage** — read/write contract storage from Lisp

## Quick start

```bash
# Build WASM (requires Rust 1.86.0 + cargo-near)
rustup override set 1.86.0
rustup target add wasm32-unknown-unknown --toolchain 1.86.0
cargo near build non-reproducible-wasm --no-abi
rustup override unset
```

## Test

```bash
# Unit tests (fast, no network)
cargo test --test lisp_unit

# Sandbox tests (local near-workspaces sandbox)
cargo test --test lisp_sandbox -- --nocapture

# Testnet tests (real NEAR testnet)
export TESTNET_ACCOUNT_ID=your-account.testnet
export TESTNET_SECRET_KEY=ed25519:...
cargo test --test lisp_testnet -- --nocapture
```

## Usage examples

```lisp
;; Arithmetic
(+ 1 2)              ;; → 3
(* 6 7)              ;; → 42

;; Lambda + closure
(define square (lambda (n) (* n n)))
(square 9)           ;; → 81

(define make-adder (lambda (n) (lambda (x) (+ n x))))
(define add10 (make-adder 10))
(add10 32)           ;; → 42

;; Recursion
(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
(fib 10)             ;; → 55

;; Policy evaluation
(check_policy "(and (>= score 85) (<= duration 3600))" "{\"score\": 90, \"duration\": 1200}")
;; → true

;; NEAR builtins
(near/block-height)  ;; → 246078789
(near/predecessor)   ;; → "alice.testnet"
(near/timestamp)     ;; → 1776386554691532289

;; Storage
(near/storage-write "key" "value")  ;; → true
(near/storage-read "key")           ;; → "value"
```

## Contract API

| Method | Params | Description |
|--------|--------|-------------|
| `new` | `eval_gas_limit: u64` | Initialize contract |
| `eval` | `code: String` | Eval Lisp, return result as string |
| `eval_with_input` | `code: String, input_json: String` | Eval with JSON vars injected into env |
| `check_policy` | `policy: String, input_json: String` | Eval policy, return bool |
| `save_policy` | `name: String, policy: String` | Store named policy on-chain |
| `eval_policy` | `name: String, input_json: String` | Eval a saved policy |
| `set_gas_limit` | `limit: u64` | Change the Lisp gas limit |
| `get_gas_limit` | | Get current gas limit |

## Gas safety

The interpreter has an internal gas counter that decrements on every eval step. When it hits zero, evaluation stops with `ERROR: out of gas`. This prevents infinite loops from burning real NEAR gas.

## Language features

- Types: nil, bool, number, string, symbol, list, lambda
- Special forms: quote, define, if, cond, let, lambda, progn, begin, and, or, not
- Arithmetic: +, -, *, /, mod
- Comparison: =, !=, <, >, <=, >=
- List ops: list, car, cdr, cons, len, append, nth
- String ops: str-concat, str-contains, to-string
- Predicates: nil?, list?, number?, string?
- NEAR: near/block-height, near/predecessor, near/signer, near/timestamp, near/log, near/storage-read, near/storage-write

## License

MIT
