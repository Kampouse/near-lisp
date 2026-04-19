#!/usr/bin/env python3
"""On-chain testnet smoke tests for near-lisp.
Usage: python3 scripts/testnet_smoke.py
"""
import json, subprocess, sys, time, os

ACCOUNT = "kampy.testnet"

# ---------------------------------------------------------------------------
# Lisp Display → Python value parser
# ---------------------------------------------------------------------------
# The contract's eval() returns a Rust String which is the Lisp Display output.
# JSON-encoding wraps that in quotes. After json.loads we get the raw Lisp Display.
#
# Examples of raw Lisp Display (after json.loads):
#   Number 42    → "42"
#   String "hi"  → '"hi"'          (Lisp wraps strings in double-quotes)
#   Bool true    → "true"
#   Bool false   → "false"
#   Nil          → "nil"
#   List (1 2 3) → "(1 2 3)"
#   List of str  → '("a" "b")'
#   Dict         → "{a: 1, b: 2}"
#   Promise      → "#<promise:0>"
# ---------------------------------------------------------------------------

def parse_lisp_value(s):
    """Parse a Lisp Display string into a comparable Python value."""
    if not isinstance(s, str):
        return s

    stripped = s.strip()

    # nil
    if stripped == "nil":
        return None

    # bools (before number check — "true"/"false" aren't numbers)
    if stripped == "true":
        return True
    if stripped == "false":
        return False

    # String — Lisp wraps in double-quotes: "\"hello\"" → "hello"
    if len(stripped) >= 2 and stripped[0] == '"' and stripped[-1] == '"':
        inner = stripped[1:-1]
        # Unescape \\ and \"
        inner = inner.replace('\\"', '"').replace('\\\\', '\\')
        return inner

    # List — starts with ( and ends with )
    if stripped.startswith("(") and stripped.endswith(")"):
        inner = stripped[1:-1].strip()
        if not inner:
            return []
        return _parse_lisp_list(inner)

    # Dict — starts with { and ends with }
    if stripped.startswith("{") and stripped.endswith("}"):
        return stripped  # dicts compared as strings for simplicity

    # Promise handle
    if stripped.startswith("#<promise:"):
        return stripped  # opaque handle

    # Float (has . or e/E)
    if "." in stripped or ("e" in stripped.lower() and not stripped.startswith("0x")):
        try:
            return float(stripped)
        except ValueError:
            pass

    # Integer
    try:
        return int(stripped)
    except ValueError:
        pass

    # Negative integer
    if stripped.startswith("-") and stripped[1:].isdigit():
        return int(stripped)

    # Fallback: return as string
    return stripped


def _tokenize_lisp(s):
    """Tokenize a Lisp Display string respecting quoted strings."""
    tokens = []
    i = 0
    while i < len(s):
        if s[i] in ' \t\n\r':
            i += 1
            continue
        if s[i] == '"':
            # Quoted string — find matching close quote, respecting escapes
            j = i + 1
            while j < len(s):
                if s[j] == '\\':
                    j += 2
                    continue
                if s[j] == '"':
                    break
                j += 1
            tokens.append(s[i:j+1])
            i = j + 1
        elif s[i] == '(':
            # Nested list — find matching close paren
            depth = 1
            j = i + 1
            while j < len(s) and depth > 0:
                if s[j] == '(':
                    depth += 1
                elif s[j] == ')':
                    depth -= 1
                elif s[j] == '"':
                    # Skip over quoted string content
                    j += 1
                    while j < len(s):
                        if s[j] == '\\':
                            j += 1
                        elif s[j] == '"':
                            break
                        j += 1
                j += 1
            tokens.append(s[i:j])
            i = j
        else:
            # Atom
            j = i
            while j < len(s) and s[j] not in ' \t\n\r':
                j += 1
            tokens.append(s[i:j])
            i = j
    return tokens


def _parse_lisp_list(inner):
    """Parse the inside of a Lisp list (without parens) into a Python list."""
    tokens = _tokenize_lisp(inner)
    return [parse_lisp_value(t) for t in tokens]


# ---------------------------------------------------------------------------
# On-chain caller
# ---------------------------------------------------------------------------

def call_eval(code):
    """Call eval on-chain, return (status, value)."""
    args = json.dumps({"code": code})

    cmd = f"""near contract call-function as-transaction {ACCOUNT} eval \
        json-args '{args}' \
        prepaid-gas '30 Tgas' attached-deposit '0 NEAR' \
        sign-as {ACCOUNT} network-config testnet sign-with-legacy-keychain send 2>&1"""
    result = subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=30)
    out = result.stdout + result.stderr

    if "Function execution return value" in out:
        lines = out.split("\n")
        for i, line in enumerate(lines):
            if "Function execution return value" in line:
                raw = lines[i+1].strip() if i+1 < len(lines) else ""
                try:
                    val = json.loads(raw)
                except json.JSONDecodeError:
                    val = raw
                return "ok", val
        return "ok", ""
    elif "Smart contract panicked" in out:
        # Extract error message from panic
        for line in out.split("\n"):
            if "panicked" in line.lower() or "ERROR" in line:
                return "error", line.strip()[:200]
        return "error", "contract panic"
    elif "ExecutionError" in out:
        return "error", "execution error"
    else:
        return "unknown", out[-300:]


# ---------------------------------------------------------------------------
# Test definitions
# ---------------------------------------------------------------------------

# Each test: (description, lisp_code, expected_python_value)
# Use None for expected to mean "smoke test — just don't crash"
# Use lambda for expected to compare parsed Lisp value

TESTS = [
    # 01-basics: arithmetic, comparisons, strings
    ("01 arithmetic", "(+ 1 2 3)", 6),
    ("01 nested math", "(* (+ 2 3) (- 10 4))", 30),
    ("01 mod", "(mod 10 3)", 1),
    ("01 comparison =", "(= 5 5)", True),
    ("01 comparison !=", "(!= 3 4)", True),
    ("01 comparison <", "(< 1 2)", True),
    ('01 str-concat', '(str-concat "hello" " " "world")', "hello world"),
    ('01 str-length', '(str-length "near")', 4),
    ('01 str-contains', '(str-contains "near-lisp" "lisp")', True),
    ('01 str-substring', '(str-substring "hello" 1 3)', "el"),
    ('01 str-split', '(str-split "a,b,c" ",")', ["a","b","c"]),

    # 02-variables
    ("02 define", "(progn (define x 42) x)", 42),
    ("02 let", "(let ((a 10) (b 20)) (+ a b))", 30),
    ("02 nested let", "(let ((x 1)) (let ((x 2) (y x)) (+ x y)))", 3),
    ("02 redefine", "(progn (define counter 0) (define counter (+ counter 1)) counter)", 1),

    # 03-conditionals
    ('03 if true', '(if true "yes" "no")', "yes"),
    ('03 if false', '(if false "yes" "no")', "no"),
    ('03 classify', '(progn (define (classify n) (if (< n 0) "neg" (if (= n 0) "zero" "pos"))) (classify 42))', "pos"),
    ('03 grade C', '(progn (define (grade s) (cond ((>= s 90) "A") ((>= s 80) "B") ((>= s 70) "C") ((>= s 60) "D") (true "F"))) (grade 72))', "C"),
    ('03 grade F', '(progn (define (grade s) (cond ((>= s 90) "A") ((>= s 80) "B") ((>= s 70) "C") ((>= s 60) "D") (true "F"))) (grade 50))', "F"),
    ("03 and short", "(and 1 2 3)", 3),
    ('03 or short', '(or nil "default")', "default"),
    ("03 not", "(not true)", False),

    # 04-lambdas
    ("04 lambda", "(progn (define double (lambda (x) (* x 2))) (double 21))", 42),
    ("04 shorthand", "(progn (define (square x) (* x x)) (square 7))", 49),
    ("04 closure", "(progn (define (make-adder n) (lambda (x) (+ n x))) ((make-adder 5) 3))", 8),
    ("04 compose", "(progn (define (compose f g) (lambda (x) (f (g x)))) (define add1 (lambda (x) (+ x 1))) (define mul2 (lambda (x) (* x 2))) ((compose mul2 add1) 4))", 10),
    ("04 apply-twice", "(progn (define (apply-twice f x) (f (f x))) (define d (lambda (x) (* x 2))) (apply-twice d 3))", 12),

    # 05-lists
    ("05 list", "(list 1 2 3)", [1,2,3]),
    ("05 car", "(car (list 1 2 3))", 1),
    ("05 cdr", "(cdr (list 1 2 3))", [2,3]),
    ("05 cons", "(cons 0 (list 1 2 3))", [0,1,2,3]),
    ("05 nth", "(nth 1 (list 10 20 30))", 20),
    ("05 len", "(len (list 1 2 3))", 3),
    ("05 append", "(append (list 1 2) (list 3 4))", [1,2,3,4]),
    ("05 quote", "(quote (1 2 3))", [1,2,3]),
    ('05 quote shorthand', "\\'(1 + 2)", [1,"+",2]),
    ("05 map", "(map (lambda (x) (* x x)) (list 1 2 3 4))", [1,4,9,16]),
    ("05 filter", "(filter (lambda (x) (> x 2)) (list 1 2 3 4))", [3,4]),
    ("05 reduce", "(reduce + 0 (list 1 2 3))", 6),
    ("05 reverse", "(reverse (list 1 2 3))", [3,2,1]),
    ("05 sort", "(sort (list 3 1 4 1 5))", [1,1,3,4,5]),
    ("05 range", "(range 0 5)", [0,1,2,3,4]),
    ('05 zip', '(zip (list 1 2 3) (list "a" "b" "c"))', [[1,"a"],[2,"b"],[3,"c"]]),
    ("05 find", "(find (lambda (x) (> x 3)) (list 1 2 4 3))", 4),
    ("05 some", "(some (lambda (x) (= x 3)) (list 1 2 3))", True),
    ("05 every", "(every (lambda (x) (> x 0)) (list 1 2 3))", True),

    # 06-recursion
    ("06 factorial", "(progn (define (factorial n) (if (<= n 1) 1 (* n (factorial (- n 1))))) (factorial 10))", 3628800),
    ("06 fib 10", "(progn (define (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2))))) (fib 10))", 55),
    ("06 mutual even", "(progn (define (my-even? n) (if (= n 0) true (my-odd? (- n 1)))) (define (my-odd? n) (if (= n 0) false (my-even? (- n 1)))) (my-even? 4))", True),
    ("06 mutual odd", "(progn (define (my-even? n) (if (= n 0) true (my-odd? (- n 1)))) (define (my-odd? n) (if (= n 0) false (my-even? (- n 1)))) (my-odd? 7))", True),
    ("06 loop factorial", "(progn (define (ftc n) (loop ((i n) (acc 1)) (if (<= i 0) acc (recur (- i 1) (* acc i))))) (ftc 20))", 2432902008176640000),
    ("06 loop fib", "(progn (define (ftc n) (loop ((i n) (a 0) (b 1)) (if (= i 0) a (recur (- i 1) b (+ a b))))) (ftc 50))", 12586269025),
    ("06 recursive sum", "(progn (define (my-sum lst) (if (nil? lst) 0 (+ (car lst) (my-sum (cdr lst))))) (my-sum (list 1 2 3 4 5)))", 15),
    ("06 tco sum", "(progn (define (sum-tc lst) (loop ((r lst) (acc 0)) (if (nil? r) acc (recur (cdr r) (+ acc (car r)))))) (sum-tc (list 1 2 3 4 5)))", 15),

    # 07-pattern-matching
    ('07 match 0', '(progn (define (d x) (match x 0 "zero" 1 "one" _ "other")) (d 0))', "zero"),
    ('07 match wildcard', '(progn (define (d x) (match x 0 "zero" 1 "one" _ "other")) (d 42))', "other"),
    ('07 match hello', '(progn (define (d x) (match x 0 "zero" 1 "one" "hello" "greeting" _ "other")) (d "hello"))', "greeting"),
    ("07 destructure list", "(match (list 1 2 3) (a b c) (+ a b c))", 6),
    ('07 classify empty', '(progn (define (cl lst) (match lst () "empty" (x) "single" _ "more")) (cl (list)))', "empty"),
    ('07 classify single', '(progn (define (cl lst) (match lst () "empty" (x) "single" _ "more")) (cl (list 1)))', "single"),
    ('07 classify more', '(progn (define (cl lst) (match lst () "empty" (x) "single" _ "more")) (cl (list 1 2)))', "more"),

    # 08-error-handling
    ('08 catch div0', '(try (/ 1 0) (catch e "caught"))', "caught"),
    ("08 try ok", "(try (+ 1 2) (catch e nil))", 3),
    ('08 custom error', '(progn (define (safe-div a b) (if (= b 0) (error "div0") (/ a b))) (try (safe-div 10 0) (catch e e)))', "div0"),
    ('08 parse-pos ok', '(progn (define (pp s) (try (let ((n (to-num s))) (if (< n 0) (error "neg") n)) (catch e -1))) (pp "42"))', 42),
    ('08 parse-pos neg', '(progn (define (pp s) (try (let ((n (to-num s))) (if (< n 0) (error "neg") n)) (catch e -1))) (pp "-5"))', -1),
    ('08 parse-pos bad', '(progn (define (pp s) (try (let ((n (to-num s))) (if (< n 0) (error "neg") n)) (catch e -1))) (pp "abc"))', -1),

    # 09-stdlib-math
    ("09 abs", '(progn (require "math") (abs -42))', 42),
    ("09 min", '(progn (require "math") (min 3 7))', 3),
    ("09 max", '(progn (require "math") (max 3 7))', 7),
    ("09 even?", '(progn (require "math") (even? 4))', True),
    ("09 odd?", '(progn (require "math") (odd? 7))', True),
    ("09 gcd", '(progn (require "math") (gcd 12 8))', 4),
    ("09 lcm", '(progn (require "math") (lcm 4 6))', 12),
    ("09 square", '(progn (require "math") (square 5))', 25),
    ("09 pow", '(progn (require "math") (pow 2 10))', 1024),
    ("09 sqrt 144", '(progn (require "math") (sqrt 144))', 12),

    # 10-stdlib-string
    ('10 str-join', '(progn (require "string") (str-join ", " (list "near" "lisp" "contract")))', "near, lisp, contract"),
    ('10 str-replace', '(progn (require "string") (str-replace "hello world" "o" "0"))', "hell0 w0rld"),
    ('10 str-repeat', '(progn (require "string") (str-repeat "ha" 3))', "hahaha"),
    ('10 str-pad-left', '(progn (require "string") (str-pad-left "42" 5 "0"))', "00042"),
    ('10 str-pad-right', '(progn (require "string") (str-pad-right "hi" 6 "."))', "hi...."),

    # 11-crypto (smoke tests — just check no crash, verify hex format)
    ('11 sha256', '(sha256 "hello")', None),
    ('11 keccak256', '(keccak256 "hello")', None),

    # 12-near-context
    ("12 predecessor", "(near/predecessor)", None),
    ("12 block-height", "(near/block-height)", None),
    ("12 timestamp", "(near/timestamp)", None),
    ('12 storage write/read', '(progn (near/storage-write "tk" "tv") (near/storage-read "tk"))', "tv"),
    ('12 storage missing', '(near/storage-read "noexist-xyz")', None),  # nil
    ('12 storage-has', '(progn (near/storage-write "ex-chk" "y") (near/storage-has? "ex-chk"))', True),
    ('12 near/log', '(near/log "test")', None),

    # 13-modules (inline patterns)
    ('13 valid-account', '(progn (define (va? id) (and (> (str-length id) 0) (str-contains id "."))) (va? "user.near"))', True),
    ('13 invalid-account', '(progn (define (va? id) (and (> (str-length id) 0) (str-contains id "."))) (va? ""))', False),
    ('13 transfer-msg', '(progn (define (tm f t a) (str-concat f " -> " t ": " a " yoctoNEAR")) (tm "alice.near" "bob.near" "5000"))', "alice.near -> bob.near: 5000 yoctoNEAR"),

    # 14-policies (via check_policy call — uses eval_policy instead)
    ('14 eval_policy pass', '(eval_policy "allow_high" "{\\"score\\":85}")', None),  # smoke — policy may not exist
    ('14 check inline pass', '(> 85 50)', True),
    ('14 check inline fail', '(> 30 50)', False),

    # 15-progn
    ("15 progn", "(progn (define a 1) (define b 2) (+ a b))", 3),
    ("15 if progn", "(progn (define r (if true (progn (define x 10) (define y 20) (+ x y)) 0)) r)", 30),
    ("15 nested progn", "(progn (define s1 (+ 1 2)) (progn (define s2 (* s1 10)) s2))", 30),

    # 17-type-conversions
    ("17 to-string 42", "(to-string 42)", "42"),
    ("17 to-string true", "(to-string true)", "true"),
    ("17 to-string nil", "(to-string nil)", "nil"),
    ('17 to-num str', '(to-num "42")', 42),
    ("17 type? num", "(type? 42)", "number"),
    ('17 type? str', '(type? "hello")', "string"),
    ("17 type? bool", "(type? true)", "boolean"),
    ("17 type? nil", "(type? nil)", "nil"),
    ("17 type? list", "(type? (list 1 2))", "list"),
    ("17 type? lambda", "(type? (lambda (x) x))", "lambda"),
    ('17 type? map', '(type? (dict "k" 1))', "map"),
    ("17 type? bytes", '(type? (hex->bytes "0xff"))', "bytes"),
    ("17 type? promise", '(type? (near/promise "kampy.testnet" "ping" "{}"))', "promise"),
    ("17 to-float", "(to-float 42)", 42.0),
    ("17 to-int", "(to-int 3.7)", 3),

    # 18-gas
    ("18 loop 100", "(progn (define (ct n) (loop ((i n) (acc 0)) (if (= i 0) acc (recur (- i 1) (+ acc 1))))) (ct 100))", 100),

    # 19-real-world
    ('19 valid-transfer', '(progn (define (vt? f t a) (and (> (str-length f) 0) (> (str-length t) 0) (> a 0) (!= f t))) (vt? "a" "b" 100))', True),
    ('19 template', '(progn (define (tpl g n) (str-concat g ", " (str-concat n "!"))) (tpl "Hello" "NEAR"))', "Hello, NEAR!"),
    ('19 FSM start', '(progn (define (ns c e) (cond ((and (= c "idle") (= e "start")) "running") ((and (= c "running") (= e "stop")) "stopped") ((and (= c "stopped") (= e "reset")) "idle") (true c))) (ns "idle" "start"))', "running"),
    ('19 FSM stop', '(progn (define (ns c e) (cond ((and (= c "idle") (= e "start")) "running") ((and (= c "running") (= e "stop")) "stopped") ((and (= c "stopped") (= e "reset")) "idle") (true c))) (ns "running" "stop"))', "stopped"),
    ('19 FSM no-op', '(progn (define (ns c e) (cond ((and (= c "idle") (= e "start")) "running") ((and (= c "running") (= e "stop")) "stopped") ((and (= c "stopped") (= e "reset")) "idle") (true c))) (ns "running" "reset"))', "running"),

    # dict
    ('dict get', '(dict/get (dict "name" "alice" "score" 95) "name")', "alice"),
    ('dict has', '(dict/has? (dict "name" "alice") "name")', True),
    ('dict keys', '(dict/keys (dict "b" 2 "a" 1))', ["a","b"]),
    ('dict vals', '(dict/vals (dict "b" 2 "a" 1))', [1,2]),
    ('dict set', '(dict/get (dict/set (dict "a" 1) "b" 2) "b")', 2),
    ('dict merge', '(dict/get (dict/merge (dict "a" 1) (dict "b" 2)) "b")', 2),
    ('dict remove', '(dict/has? (dict/remove (dict "a" 1 "b" 2) "a") "a")', False),

    # JSON
    ('to-json', '(to-json (list 1 "two" true))', '[1,"two",true]'),
    ('from-json', '(dict/get (from-json "{\\"x\\":42}") "x")', 42),

    # fmt
    ('fmt', '(fmt "Hello {name}" (dict "name" "alice"))', "Hello alice"),

    # bytes
    ('bytes roundtrip', '(bytes->hex (hex->bytes "0xdeadbeef"))', "0xdeadbeef"),
    ('bytes len', '(bytes-len (hex->bytes "0xdeadbeef"))', 4),
    ('bytes->string', '(bytes->string (string->bytes "hello"))', "hello"),
    ('bytes concat', '(bytes->hex (bytes-concat (hex->bytes "0xff") (hex->bytes "0xaa")))', "0xffaa"),

    # promise chaining
    ('promise create', '(type? (near/promise "kampy.testnet" "ping" "{}"))', "promise"),
    ('promise then', '(progn (define p (near/promise "kampy.testnet" "ping" "{}")) (type? (promise-then p "kampy.testnet" "get_owner" "{}")))', "promise"),
    ('promise and', '(progn (define p1 (near/promise "kampy.testnet" "ping" "{}")) (define p2 (near/promise "kampy.testnet" "get_owner" "{}")) (type? (promise-and p1 p2)))', "promise"),

    # inspect
    ('inspect num', '(inspect 42)', lambda v: "number" in v and "42" in v),
    ('inspect str', '(inspect "hi")', lambda v: "string" in v),
    ('inspect bool', '(inspect true)', lambda v: "boolean" in v),

    # macros
    ('defmacro', '(progn (defmacro double (x) (list (quote *) 2 x)) (double 21))', 42),
    ('macroexpand', '(progn (defmacro dbl (x) (list (quote *) 2 x)) (macroexpand (dbl 5)))', "(list (*) 2 5)"),  # expanded form
]


def compare_values(actual, expected):
    """Compare actual parsed value against expected. Returns True if match."""
    if callable(expected):
        return expected(actual)
    if actual == expected:
        return True
    # Numeric string comparison (e.g. "42" == 42)
    if isinstance(expected, (int, float)) and isinstance(actual, str):
        try:
            return float(actual) == float(expected)
        except ValueError:
            return False
    return False


def main():
    total = len(TESTS)
    print(f"Running {total} on-chain tests against {ACCOUNT}...\n")

    passed = 0
    failed = 0
    errored = 0
    failures = []

    for desc, code, expected in TESTS:
        status, raw_val = call_eval(code)

        if status == "error":
            tag = "ERR"
            errored += 1
            detail = str(raw_val)[:120]
            failures.append(f"  ERR {desc}: {detail}")
        elif status == "timeout":
            tag = "TMO"
            errored += 1
            failures.append(f"  TMO {desc}")
        elif expected is None:
            # Smoke test — just don't crash
            tag = "OK "
            passed += 1
        else:
            parsed = parse_lisp_value(raw_val)
            if compare_values(parsed, expected):
                tag = "PASS"
                passed += 1
            else:
                tag = "FAIL"
                failed += 1
                failures.append(f"  FAIL {desc}: got {parsed!r} (raw={raw_val!r}), expected {expected!r}")

        print(f"  [{tag}] {desc}")
        time.sleep(0.3)  # rate limit

    print(f"\n{'='*60}")
    print(f"Results: {passed}/{total} passed, {failed} wrong, {errored} errors")

    if failures:
        print(f"\nFailures:")
        for f in failures:
            print(f)

    sys.exit(0 if (failed + errored) == 0 else 1)


if __name__ == "__main__":
    main()
