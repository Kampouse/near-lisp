# Near-Lisp Interpreter Gas Optimization Audit

## Executive Summary

The interpreter has several systemic gas waste patterns that compound on-chain. The most impactful issues are: (1) environment as Vec with O(n) linear scans happening on every variable lookup, (2) pervasive `.clone()` on every value construction and env operation, (3) no TCO except loop/recur (recursive lambdas blow the stack and burn gas), (4) triple evaluation in run_program_with_ccall completion path, and (5) Lambda closures capturing the entire env on every creation. Combined, these likely account for 40-60% of gas burn in typical on-chain programs.

---

## 1. Gas/Performance Bottlenecks

### CRITICAL: Environment is Vec<(String, LispVal)> with O(n) linear scan

**Location:** Lines 637-638, 658, 406-414, 723, 745, 873, 959, 1774-1778, 2711-2714

The environment `Vec<(String, LispVal)>` is scanned with `.iter().rev().find()` on every variable lookup. This is O(n) where n = total bindings ever pushed (shadowing just appends). Every symbol resolution in `lisp_eval` (line 658), every `sandbox_key` call (line 406), every lambda lookup in `dispatch_call` (line 1774), and the `__ccall_results__` mutation in `resume_eval` (line 2711) all do linear scans.

**Impact:** For a program with 50 defines that reads variables 100 times, that's ~5000 string comparisons. On NEAR, string comparison is WASM memcmp — not free.

**Fix:** Replace with `BTreeMap<String, LispVal>` or `HashMap<String, LispVal>` (or a scope-chain of small maps). O(log n) or O(1) lookups. This is the single highest-impact change.

```rust
// Before: env.iter().rev().find(|(k, _)| k == name)
// After:  env.get(name)  // HashMap
```

### CRITICAL: Pervasive cloning of LispVal

**Location:** Lines 652, 659, 482-483, 723, 745, 798, 819, 873, 875, 1028, 1086, 1090, etc.

Almost every operation clones. Key hot spots:

- **lisp_eval atom branch (line 652):** `Ok(expr.clone())` — every literal, every self-evaluating value gets cloned on every eval. For strings and lists this is O(n) allocation.
- **apply_lambda (line 482-483):** `let mut local = closed_env.clone(); local.extend(caller_env.iter().cloned())` — clones the ENTIRE closure env PLUS the caller env on EVERY function call. If env has 100 bindings, that's 100 String+LispVal clones per call.
- **let binding (line 723):** `let mut local = env.clone()` — clones entire env for every let block.
- **lambda creation (line 745):** `closed_env: Box::new(env.clone())` — captures full env snapshot.
- **loop (line 873):** `let mut local = env.clone()` inside the tight loop — clones env EVERY iteration.
- **dispatch_call (line 1028-1031):** All args are evaluated and collected into a new Vec via clone.
- **sandbox_key (line 406-414):** Linear scan + clone of env just to find the prefix.

**Impact:** For a recursive function called 50 times with 30 env bindings, that's 1500 LispVal clones, each potentially containing heap-allocated Strings and Vecs.

**Fix:** Use `Rc<LispVal>` or `Arc<LispVal>` for shared immutable data. Use references where possible. For env, use a persistent data structure (im::HashMap) or parent-pointer scope chain so `let`/`lambda` only allocate the new frame, not copy the entire chain.

### HIGH: sandbox_key does O(n) scan on every storage operation

**Location:** Lines 405-415, 1397, 1404, 1412, 1419

Every `near/storage-read`, `near/storage-write`, `near/storage-remove`, `near/storage-has?` calls `sandbox_key()` which does a full env linear scan for `__storage_prefix__`. This prefix is set once and never changes.

**Fix:** Extract `__storage_prefix__` once at eval start and pass it as a separate parameter, or cache it on first access.

### MEDIUM: Hex encoding is O(n) with per-byte format!()

**Location:** Lines 2267-2269

```rust
fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}
```

Each byte allocates a separate String via `format!()`, then they're joined. For a 32-byte hash, that's 32 heap allocations + 1 join.

**Fix:** Pre-allocate a String of capacity `bytes.len() * 2` and write hex chars directly with a lookup table. Single allocation.

### MEDIUM: str-length counts Unicode chars

**Location:** Line 1140

```rust
Ok(LispVal::Num(s.chars().count() as i64))
```

This scans the entire string to count chars. On-chain storage keys/values are typically ASCII, making this wasteful.

**Fix:** For ASCII strings (which is the common case on-chain), `s.len()` is sufficient. Could add a fast path.

---

## 2. Language Design Gaps (Gas-Relevant)

### Missing: Mutable data structures (set!, mutable lists/arrays)

Currently every dict/set/remove/merge clones the entire BTreeMap (lines 1336-1339, 1368-1371). Dict updates are O(n) because they clone the entire map. On-chain state management is a primary use case — forcing functional-style copy-on-write for every mutation burns gas.

**Fix:** Add `dict/update!` that mutates in place, or use a `RefCell`-style approach for mutable references.

### Missing: Efficient numeric for-loop

The `range` builtin creates a full Vec in memory (line 1625):
```rust
let vals: Vec<LispVal> = (start..end).map(LispVal::Num).collect();
```

For `(range 0 1000)`, this allocates 1000 LispVal::Num heap objects. Then `map`/`reduce`/`filter` iterate over them with more allocations.

**Fix:** Add a native `(for i start end body)` construct that iterates without materializing the list. Or make `map`/`reduce` accept a range directly without intermediate list allocation.

### Missing: Batch storage operations

Every storage read/write is a separate builtin call with O(n) env lookup + O(n) sandbox_key scan. A `(near/storage-batch-read (list "k1" "k2"))` that returns a map would halve the per-key overhead.

### Missing: Compact binary values

All storage values are strings. All numbers are i64 or f64 (8 bytes) but get stringified for storage. A `Bytes` variant in LispVal would allow raw binary storage without UTF-8 encode/decode overhead.

---

## 3. Interpreter Architecture Issues

### CRITICAL: Recursive eval blows the stack — no general TCO

**Location:** Lines 635-1016

`lisp_eval` is fully recursive. Only `loop/recur` has TCO (lines 872-890). Recursive Lisp programs using `(define fib (lambda (n) ...))` will:
1. Burn gas proportional to recursion depth
2. Risk stack overflow (NEAR WASM stack is limited)
3. On each recursive call, `apply_lambda` clones the entire env (line 482)

The stdlib itself defines recursive functions: `gcd`, `map`, `filter`, `reduce`, `sort`, `range`, `reverse`, etc. — all using recursive lambdas, not loop/recur. Each call pays the full env clone + stack frame cost.

**Impact:** `(sort (range 0 100))` via stdlib creates ~100 LispVals for range, then sort does O(n log n) recursive calls, each cloning the env. A conservative estimate is 10,000+ env entries cloned.

**Fix:** Implement general tail-call optimization in `lisp_eval`. Detect when a call is in tail position (last expression in body) and reuse the stack frame. This is a larger change but dramatically reduces gas for recursive programs.

### HIGH: Triple evaluation in run_program_with_ccall completion

**Location:** Lines 2156-2166, 2248-2260

When `run_program_with_ccall` or `run_remaining_with_ccall` reaches the end without yielding, it RE-EVALUATES all expressions just to get the final result:

```rust
// Lines 2158-2166
let mut result = LispVal::Nil;
let mut final_env = env.clone();
let mut final_gas = gas_limit;
for expr in exprs.iter() {
    result = lisp_eval(expr, &mut final_env, &mut final_gas)?;
}
```

This triples the gas cost of the non-ccall path (once in Phase 1 eval, once in the `env2` clone scan for pending_vars, once here).

**Fix:** Track the last result during Phase 1 evaluation instead of re-evaluating. This is a straightforward refactor — store `Option<LispVal>` for the last evaluated result.

### HIGH: check_ccall + extract_ccall_info double-scan with env cloning

**Location:** Lines 2130-2141, 2222-2231

For batch ccall detection, the code:
1. Scans expressions with `check_ccall` (calling `lisp_eval` for args)
2. Then clones the env and rescans with `check_ccall` AGAIN to get `pending_vars`
3. Then copies the env back

```rust
// Lines 2132-2141
let mut gas2 = gas_limit;
let mut env2 = env.clone();  // FULL ENV CLONE
for i in pos..first_after_batch {
    if let Some(ccall_info) = check_ccall(&exprs[i], &mut env2, &mut gas2)? {
        pending_vars.push(ccall_info.pending_var);
    }
}
*env = env2;
gas = gas2;
```

**Fix:** Collect `pending_vars` during the first scan in Phase 2. The second scan is completely redundant.

### MEDIUM: Tokenizer allocates Vec<String> for all tokens

**Location:** Lines 171-244

`tokenize()` collects all tokens into a `Vec<String>`, then `parse()` iterates over them. For a large program, this doubles memory usage (source string + token vec).

**Fix:** Use a cursor-based tokenizer that yields tokens on demand.

### MEDIUM: parse_all is called redundantly for modules

**Location:** Lines 967-1001

When `require` loads a stdlib module, it calls `parse_all(code)` every time. The stdlib code is `const` — it never changes. The parsed ASTs could be pre-built at compile time.

**Fix:** Use a `once_cell` or `lazy_static` to cache parsed stdlib ASTs, or build them as const expressions.

---

## 4. Storage Inefficiency

### HIGH: VmState serialization includes full env with all bindings

**Location:** Lines 1896-1910, 2789-2790

```rust
pub struct VmState {
    pub env: Vec<(String, LispVal)>,
    pub remaining: Vec<LispVal>,
    pub gas: u64,
    pub pending_vars: Vec<Option<String>>,
}
```

When yielding for a ccall, the ENTIRE env (all bindings, including stdlib functions with their full closure environments) is Borsh-serialized and written to contract storage. The stdlib `list` module alone defines 12 functions, each capturing the entire env in their closure.

**Impact:** A typical yield could serialize 50+ bindings with deeply nested Lambda values. Each Lambda contains a `closed_env: Box<Vec<(String, LispVal)>>` which is a full env snapshot. This creates exponential storage growth: Lambda{env: [Lambda{env: [...]}]}.

**Fix:** 
1. Don't store stdlib functions in the serialized env — they can be re-loaded on resume.
2. Use env compression: only serialize bindings that differ from the initial state.
3. Strip closures from Lambda values before serialization (keep params + body only, rebuild env on resume).

### HIGH: Lambda closed_env is exponential in nesting depth

**Location:** Lines 120-125

```rust
Lambda {
    params: Vec<String>,
    rest_param: Option<String>,
    body: Box<LispVal>,
    closed_env: Box<Vec<(String, LispVal)>>,
}
```

Every Lambda captures the full env at creation time. If you define fn1, then fn2 (which captures env containing fn1), then fn3 (captures env containing fn1 + fn2), each successive closure is larger. The `closed_env` of fn3 contains fn2's closed_env which contains fn1's closed_env — exponential duplication.

**Fix:** Use an env ID/reference system. Store envs in a separate vec and reference by index. Or use persistent data structures (structural sharing via Rc/Arc).

### MEDIUM: Storage keys are all string-prefixed

**Location:** Lines 984, 2408, 2465, 2513

All keys use format strings: `"module:{}", "policy:{}", "script:{}"`. Each requires a heap allocation for the format string. On NEAR, storage key length affects gas cost.

**Fix:** Use shorter, fixed-length key encoding. `"m:{}"` instead of `"module:{}"`.

---

## 5. Low-Hanging Fruit (Easy Wins, Big Impact)

### #1: Replace env Vec with HashMap (ESTIMATED: 20-40% gas reduction for variable-heavy programs)

```rust
type Env = HashMap<String, LispVal>;
```

Single biggest win. Every variable lookup, every define, every lambda call benefits. The `push` + `rev().find()` pattern becomes `insert` + `get`. Shadowing works naturally (later inserts overwrite).

**Scope chain for let/lambda:** Use a parent-pointer pattern:
```rust
struct Env {
    bindings: HashMap<String, LispVal>,
    parent: Option<Box<Env>>,
}
```

This makes `let` and `lambda` O(1) to create (no clone), and lookup is O(depth * log n) instead of O(total_bindings).

### #2: Eliminate the triple-eval in run_program_with_ccall completion (ESTIMATED: 30-50% gas reduction for non-ccall completion path)

Track `last_result` during Phase 1:
```rust
let mut last_result = LispVal::Nil;
while pos < exprs.len() {
    if check_ccall(&exprs[pos], env, &mut gas)?.is_some() { break; }
    last_result = lisp_eval(&exprs[pos], env, &mut gas)?;
    pos += 1;
}
// Use last_result directly instead of re-evaluating
```

### #3: Eliminate the double ccall scan (ESTIMATED: 15-25% gas reduction for ccall-heavy scripts)

Collect `pending_vars` during the first batch scan instead of rescanning with a cloned env.

### #4: Avoid cloning atoms in lisp_eval (ESTIMATED: 10-15% gas reduction)

```rust
// Before (line 652):
LispVal::Nil | LispVal::Bool(_) | LispVal::Num(_) | ... => Ok(expr.clone()),

// After: use Copy for primitive types
LispVal::Nil => Ok(LispVal::Nil),
LispVal::Bool(b) => Ok(LispVal::Bool(*b)),
LispVal::Num(n) => Ok(LispVal::Num(*n)),
LispVal::Float(f) => Ok(LispVal::Float(*f)),
// Strings/Lists still need cloning, but can be deferred
```

### #5: Pre-allocate hex_encode buffer (ESTIMATED: saves ~32 heap allocs per hash)

```rust
fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0xf) as usize] as char);
    }
    s
}
```

### #6: Cache __storage_prefix__ outside env (ESTIMATED: eliminates O(n) scan per storage op)

Store the prefix as a separate field in the eval context rather than in the env vec. Currently every `near/storage-*` call does an O(n) scan for it.

### #7: Stdlib functions as native builtins instead of parsed Lisp (ESTIMATED: 30-50% gas reduction for stdlib-heavy programs)

`map`, `filter`, `reduce`, `sort`, `reverse`, `range`, etc. are already native builtins (lines 1677-1770). But the stdlib Lisp definitions (lines 42-54) shadow them with recursive lambda versions when `(require "list")` is called. The Lisp versions are slower because they use recursive calls with env cloning.

**Fix:** Either don't load the stdlib versions for functions that already have native builtins, or make `require` skip redefining native builtins.

### #8: Add `Rc<LispVal>` to eliminate deep cloning (ESTIMATED: 20-30% gas reduction overall)

Replace `LispVal::List(Vec<LispVal>)` with `LispVal::List(Rc<Vec<LispVal>>)` and `LispVal::Str(Rc<String>)`. Cloning becomes `Rc::clone()` (increment counter, no heap alloc). This is safe because LispVal is never mutated in place.

---

## Priority Ranking

| Priority | Fix | Impact | Effort |
|----------|-----|--------|--------|
| P0 | Replace env Vec with HashMap/scope-chain | 20-40% gas | Medium |
| P0 | Eliminate triple-eval in completion path | 30-50% gas | Easy |
| P0 | Eliminate double ccall scan | 15-25% gas | Easy |
| P1 | Add Rc<LispVal> for shared data | 20-30% gas | Medium |
| P1 | Strip Lambda closures from serialized VmState | Large storage savings | Medium |
| P1 | Stdlib: use native builtins, don't load Lisp versions | 30-50% for stdlib | Easy |
| P2 | General TCO for recursive lambdas | Unlocks deep recursion | Hard |
| P2 | Cache storage_prefix outside env | Eliminates per-storage scan | Easy |
| P2 | Pre-allocate hex_encode | Minor but trivial | Trivial |
| P3 | Native for-loop (no intermediate list) | Useful for iteration | Medium |
| P3 | Batch storage operations | Reduces per-key overhead | Medium |
| P3 | Pre-parse stdlib at compile time | Eliminates redundant parsing | Easy |

---

## Architecture Diagram of Current Hot Path

```
lisp_eval (Sym lookup)
  └── env.iter().rev().find()     ← O(n) linear scan, every lookup
  
apply_lambda  
  ├── closed_env.clone()          ← O(n) full env clone
  ├── caller_env.cloned()         ← O(n) full env clone  
  └── lisp_eval(body, local, gas) ← recursive call

dispatch_call
  ├── args: map(lisp_eval)        ← O(n) eval + clone each arg
  ├── env.iter().rev().find()     ← O(n) lookup for lambda
  └── call_val(func, args)
      └── apply_lambda(...)       ← another env clone

let / match / try
  └── env.clone()                 ← O(n) full env clone per scope

loop/recur (TCO — the only optimized path)
  └── env.clone()                 ← still O(n) per iteration!
```
