# near-lisp examples

Examples covering all language features. Run on-chain via `eval` or `eval_with_input`.

| # | File | Topics |
|---|------|--------|
| 01 | [basics.lisp](01-basics.lisp) | Numbers, arithmetic, comparisons, strings |
| 02 | [variables.lisp](02-variables.lisp) | define, let, scoping |
| 03 | [conditionals.lisp](03-conditionals.lisp) | if, cond, and, or, not |
| 04 | [lambdas.lisp](04-lambdas.lisp) | lambda, closures, higher-order functions, compose |
| 05 | [lists.lisp](05-lists.lisp) | list, car, cdr, cons, quote, map, filter, reduce |
| 06 | [recursion.lisp](06-recursion.lisp) | recursion, loop/recur (tail-call optimization) |
| 07 | [pattern-matching.lisp](07-pattern-matching.lisp) | match, destructuring, rest patterns |
| 08 | [error-handling.lisp](08-error-handling.lisp) | try/catch, error, safe parsing |
| 09 | [stdlib-math.lisp](09-stdlib-math.lisp) | abs, min, max, gcd, sqrt, pow |
| 10 | [stdlib-string.lisp](10-stdlib-string.lisp) | str-join, str-replace, str-repeat, padding |
| 11 | [stdlib-crypto.lisp](11-stdlib-crypto.lisp) | sha256, keccak256 |
| 12 | [near-context.lisp](12-near-context.lisp) | block info, accounts, storage, logging |
| 13 | [modules.lisp](13-modules.lisp) | custom modules, require, module composition |
| 14 | [policies.lisp](14-policies.lisp) | save/eval policies, check_policy, input JSON |
| 15 | [progn.lisp](15-progn.lisp) | progn, begin, expression sequencing |
| 16 | [cross-contract.lisp](16-cross-contract.lisp) | ccall, batch calls, async yield/resume |
| 17 | [type-conversions.lisp](17-type-conversions.lisp) | to-string, to-num, type?, nil? |
| 18 | [gas.lisp](18-gas.lisp) | gas limits, efficiency, catchable exhaustion |
| 19 | [real-world.lisp](19-real-world.lisp) | validation, FSM, list pipelines, compound policies |

## Quick start

```bash
# Eval a single expression
near call contract.near eval '{"code": "(+ 1 2)"}' --accountId you.near

# Eval with input data
near call contract.near eval_with_input '{"code": "(+ x y)", "input_json": "{\"x\": 3, \"y\": 4}"}' --accountId you.near

# Save and run a script
near call contract.near save_script '{"name": "hello", "code": "(+ 1 2)"}' --accountId you.near
near call contract.near eval_script '{"name": "hello"}' --accountId you.near
```
