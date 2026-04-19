# Gas & Performance Reference — near-lisp on-chain

All benchmarks run on kampy.testnet (300 Tgas receipt cap). Measured via RPC `EXPERIMENTAL_tx_status` for exact gas.

## Bytecode Loop VM

`loop/recur` with simple bodies compiles to a register-based bytecode VM — ~10x faster than the old tree-walk.

### Loop benchmarks (1-binding counting)

```
(loop ((i 0)) (if (>= i N) i (recur (+ i 1))))

Iterations    Total Gas    Per-iter
1,000         4.36 Tgas    4.36 Ggas
10,000       25.14 Tgas    2.51 Ggas
50,000      117.48 Tgas    2.35 Ggas
100,000     232.92 Tgas    2.33 Ggas
```

Baseline (no loop, eval `"1"`): 1.98 Tgas

### Multi-binding loops

```
2-binding (loop ((i 0) (sum 0)) ...):
  10K = 35.02 Tgas (3.50 Ggas/iter)
  100K = 301.20 Tgas (3.01 Ggas/iter)

3-binding + mod + if (odd/even):
  Max: 45,755 iterations at 301.42 Tgas (6.59 Ggas/iter)
```

### Per-iteration cost breakdown

```
Component                         Ggas/iter
Base loop overhead (1 binding)    ~2.33
Extra binding                     +0.70
Extra if(true) in body            +0.30
Extra if(>comparison) in body     +0.79
mod operation                     ~included in if+comp
```

### Outer if around loop

An `if` wrapping a loop is evaluated once, not per iteration. Cost: ~0.02-0.04 Tgas flat (one-time). Per-iter cost identical to bare loop.

### Max iterations at 300 Tgas

```
Pattern                         Max iters    Ggas/iter
1-binding count                 129,672      2.33
2-binding count                 ~99,600      3.01
2-bind + mod + if(sum)          38,310       6.20
3-bind + mod + if branch        45,755       6.59
```

### Old tree-walk (for comparison)

~22.45 Ggas/iter, max ~13,350 iterations. Bytecode VM is ~10x faster.

### What the bytecode VM compiles

**Supported** (compiled to bytecode): `+` `-` `*` `/` `%`, `=` `<` `<=` `>` `>=`, `if`, `and`, `or`, `not`, `progn`/`begin`, `cond`, `recur` (any nesting depth), all literals, 22 builtins.

**Falls back to tree-walk**: `let`/`try`/`match` (env mutation), `define`/`lambda`/`quote`/`defmacro`.

---

## List Operations

### Creation (range)

```
N=100:     2.14 Tgas   (21 Ggas/elem)
N=1,000:   3.40 Tgas   (3.4 Ggas/elem)
N=5,000:   9.24 Tgas   (1.85 Ggas/elem)
N=10,000: 16.57 Tgas   (1.66 Ggas/elem)
N=20,000: 32.04 Tgas   (1.60 Ggas/elem)
N=50,000: 78.46 Tgas   (1.57 Ggas/elem)
```

Amortized: ~1.57 Ggas/elem at scale. O(n) confirmed.

### Higher-order functions (native Rust outer loop + lambda per element)

```
Operation     Ggas/elem (at scale)   Note
map           9.2                    Allocates output list
filter        9.0                    Allocates output list
every         8.1                    Short-circuits on false
find          4.4                    Short-circuits, single value
reduce        4.3                    No list allocation — cheapest
```

Key insight: `reduce` is 2x cheaper than `map` because it doesn't build a new list. If you can express your problem as a reduction (sums, counts, min/max), prefer it over map-then-aggregate.

### Structural operations

```
Operation     Ggas/elem (at scale)   Complexity
reverse       1.95                   O(n)
sort          2.14                   O(n log n)
append        3.57                   O(n)
zip           6.61                   O(n) — creates pairs
nth           ~0 overhead            O(n) traversal
len           ~0 overhead            O(1) — basically free
```

All structural ops are native Rust implementations — no Lisp evaluator overhead.

### Manual loop with car/cdr

```
(loop ((lst (range 0 N)) (sum 0)) (if (nil? lst) sum (recur (cdr lst) (+ sum (car lst)))))

N=100:     21.4 Ggas/elem
N=1,000:    2.5 Ggas/elem
N=5,000:    0.8 Ggas/elem
N=10,000:   0.6 Ggas/elem
```

Cheaper than `reduce` at scale because `car`/`cdr`/`nil?` compile as `BuiltinCall` in the bytecode VM — no lambda dispatch overhead.

### Max practical list sizes at 300 Tgas

```
Operation         Max elements
range creation    ~190,000
sort              ~140,000
reduce +          ~70,000
map (* x 2)       ~32,000
zip               ~45,000
```

### Scaling analysis (doubling test)

**reduce + (O(n) confirmed)**:
```
N→2N: gas ratio 1.17→1.76 (converging toward 2.0x, expected for O(n) with fixed overhead)
```

**sort reversed range (O(n log n) confirmed)**:
```
N→2N: gas ratio 1.08→1.41, NlogN ratio ~2.2-2.3x
```

---

## Cross-Contract Calls (ccall)

### Batching gas costs (on testnet)

```
N ccalls    Prepaid    Actual burn
1           55T        10.4T
2           60T        12.3T
3           60T        14.3T
4           65T        16.4T
5           70T        18.5T
6           75T        20.6T
```

Marginal: ~2.1T actual burn per extra ccall. Each ccall defaults to 10T allocation (1.4T typical burn for view calls — 86% waste).

### Key constants

- `yield_overhead`: 5T
- `auto_resume_gas`: `2T + N × 0.1T`
- `reserve`: `3T + 0.3T × (N-1)`
- `promise_yield_create`: ~5T fixed per yield cycle
- `ccall_gas` per view: 10T allocation

### Ccall placement restriction (deployed contract)

`near/ccall` only works at TOP expression level or inside `(define var (near/ccall ...))`. Inside `progn`, `let`, `if`, or nested forms → `"undefined: near/ccall-view"`. (CEK machine exists in code but not deployed — will fix this.)

---

## On-chain gas benchmarking method

`near` CLI truncates gas to 3 decimal Tgas — useless for precision. Use RPC:

```bash
# Get tx hash from CLI output, then:
curl -s -H 'Content-Type: application/json' https://archival-rpc.testnet.near.org \
  -d '{"jsonrpc":"2.0","id":1,"method":"EXPERIMENTAL_tx_status","params":["TX_HASH","ACCOUNT"]}' \
  -o /tmp/tx.json

# Sum all gas: transaction_outcome + all receipts_outcome
python3 -c "
import json
d = json.load(open('/tmp/tx.json'))
total = d['result']['transaction_outcome']['outcome']['gas_burnt']
for r in d['result']['receipts_outcome']:
    total += r['outcome']['gas_burnt']
print(f'Total: {total/1e12:.2f} Tgas')
"
```

---

## Summary: Practical limits

```
Pattern                         Max capacity (300 Tgas)
Pure compute loop               ~130,000 iterations
Odd/even + mod + branch         ~45,000 iterations
Reduce on list                  ~70,000 elements
Map on list                     ~32,000 elements
List creation (range)           ~190,000 elements
Sort                            ~140,000 elements
Single ccall                    55T minimum
6 parallel ccalls               75T minimum

Rule of thumb: if it fits in a SQL WHERE clause, it fits on-chain.
If it needs a full table scan, it doesn't.
Data > compute: list allocation is the bottleneck, not the loop body.
```
