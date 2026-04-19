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

## Gas & Performance

See `references/GAS_REFERENCE.md` for detailed on-chain benchmarks.

Quick reference (bytecode VM, 300 Tgas cap):
- Pure compute loop: ~2.3 Ggas/iter, max ~130K iterations
- Reduce on list: ~4.3 Ggas/elem, max ~70K elements
- Map on list: ~9.2 Ggas/elem, max ~32K elements
- List creation: ~1.6 Ggas/elem, max ~190K elements
- Sort: ~2.1 Ggas/elem (O(n log n))
- Extra `if` in loop: +0.3 Ggas/iter (constant) to +0.8 Ggas/iter (with comparison)
- Outer `if` around loop: ~0.02 Tgas flat (one-time)

## Security Model

**Owner**: Set at init to `env::signer_account_id()`. Only owner can manage policies, scripts, modules, gas limit, whitelist, and ownership transfer.

**Eval Whitelist**: If empty (default), all callers can eval. If non-empty, only listed accounts can call eval methods.

**Storage Isolation**: Each caller's storage is prefixed with `eval:{caller_account}:`, preventing cross-caller access. The `__storage_prefix__` env var is pushed before user input so it can't be overridden.

**Private Methods**: `resume_eval` and `auto_resume_batch_ccall` are `#[private]` — only the contract itself can call them.

## Common Patterns

```lisp
;; Policy evaluation
(check_policy
  "(and (>= score 85) (<= duration 3600))"
  "{\"score\": 90, \"duration\": 1200}")
;; → true
```

```lisp
;; Script with input
;; Store: save_script("greet", "(fmt \"Hello {name}!\" input)")
;; Call: eval_script_with_input("greet", "{\"name\": \"world\"")
;; → "Hello world!"
```

```lisp
;; Cross-contract oracle
(define price (near/ccall "oracle.near" "get_price" "{\"pair\": \"NEAR/USD\"}"))
(define adjusted (+ (to-int price) 100))
(near/storage-write "adjusted_price" (to-string adjusted))
```

```lisp
;; Error-safe eval
(try
  (/ 100 0)
  (catch e (near/log (str-concat "error: " e))))
```

## Known Pitfalls

- `(define (f x) body)` shorthand desugars to `(define f (lambda (x) body))`
- `'expr` quote shorthand works — `'foo` → `(quote foo)`, `'(1 2 3)` → `(quote (1 2 3))`
- `(loop for i in list sum i)` NOT valid — only Clojure-style `(loop (bindings) body)` with `(recur ...)`
- Integer division with two ints: `(/ 10 3)` → `3` (truncated), not `3.333...`
- `require` is idempotent — safe to call multiple times for the same module
- Storage keys are auto-prefixed per caller — you can't access another caller's storage
- Cross-contract calls require `eval_async` or `eval_script_async`, not regular `eval`
- `loop/recur` is the ONLY iteration pattern with zero stack growth. Recursive lambdas overflow at ~100-200 depth.

## Implementation & Testing

See `references/IMPLEMENTATION_NOTES.md` for Rust internals, bug history, and testing pitfalls.

## Skill file sync

Two copies exist and must be kept in sync:
- `~/.hermes/skills/near-lisp/SKILL.md` (global, loaded by hermes)
- `~/.openclaw/workspace/near-lisp-clean/.agents/skills/near-lisp/SKILL.md` (project-local)
After editing either: `cp ~/.openclaw/workspace/near-lisp-clean/.agents/skills/near-lisp/SKILL.md ~/.hermes/skills/near-lisp/SKILL.md`
