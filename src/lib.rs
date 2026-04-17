use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::store::IterableSet;
use near_sdk::{
    env, near, AccountId, CryptoHash, Gas, GasWeight, NearToken, Promise, PromiseResult,
};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Gas cost for each storage operation (read/write/remove/has).
const STORAGE_GAS_COST: u64 = 100;

/// Helper: check and consume `cost` gas units. Returns Err if insufficient.
fn consume_gas(gas: &mut u64, cost: u64, op: &str) -> Result<(), String> {
    if *gas < cost {
        return Err(format!("{}: out of gas (need {}, have {})", op, cost, *gas));
    }
    *gas -= cost;
    Ok(())
}

// ---------------------------------------------------------------------------
// Standard Library Modules
// ---------------------------------------------------------------------------

const MATH_STDLIB: &str = r#"
(define abs (lambda (x) (if (< x 0) (- 0 x) x)))
(define min (lambda (a b) (if (< a b) a b)))
(define max (lambda (a b) (if (> a b) a b)))
(define even? (lambda (n) (= (mod n 2) 0)))
(define odd? (lambda (n) (= (mod n 2) 1)))
(define gcd (lambda (a b) (if (= b 0) (abs a) (gcd b (mod a b)))))
(define square (lambda (x) (* x x)))
(define identity (lambda (x) x))
(define pow (lambda (base exp) (if (<= exp 0) 1 (* base (pow base (- exp 1))))))
(define sqrt (lambda (n) (if (< n 0) nil (if (< n 2) n (loop ((x (/ n 2))) (let ((x1 (/ (+ x (/ n x)) 2))) (if (>= x1 x) x (recur x1))))))))
(define lcm (lambda (a b) (if (or (= a 0) (= b 0)) 0 (/ (* (abs a) (abs b)) (gcd a b)))))
"#;

const STDLIB_LIST: &str = r#"
(define empty? (lambda (lst) (if (nil? lst) true (= (len lst) 0))))
(define map (lambda (f lst) (if (empty? lst) (list) (cons (f (car lst)) (map f (cdr lst))))))
(define filter (lambda (pred lst) (if (empty? lst) (list) (if (pred (car lst)) (cons (car lst) (filter pred (cdr lst))) (filter pred (cdr lst))))))
(define reduce (lambda (f init lst) (if (empty? lst) init (reduce f (f init (car lst)) (cdr lst)))))
(define find (lambda (pred lst) (if (empty? lst) nil (if (pred (car lst)) (car lst) (find pred (cdr lst))))))
(define some (lambda (pred lst) (if (empty? lst) false (if (pred (car lst)) true (some pred (cdr lst))))))
(define every (lambda (pred lst) (if (empty? lst) true (if (pred (car lst)) (every pred (cdr lst)) false))))
(define reverse (lambda (lst) (if (empty? lst) (list) (loop ((acc (list)) (cur lst)) (if (empty? cur) acc (recur (cons (car cur) acc) (cdr cur)))))))
(define sort (lambda (lst) (if (empty? lst) (list) (if (empty? (cdr lst)) lst (let ((pivot (car lst)) (rest (cdr lst))) (append (sort (filter (lambda (x) (< x pivot)) rest)) (cons pivot (sort (filter (lambda (x) (>= x pivot)) rest)))))))))
(define range (lambda (start end) (if (>= start end) (list) (cons start (range (+ start 1) end)))))
(define zip (lambda (a b) (if (or (empty? a) (empty? b)) (list) (cons (list (car a) (car b)) (zip (cdr a) (cdr b))))))
"#;

const STDLIB_STRING: &str = r#"
(define str-join (lambda (sep lst) (if (or (nil? lst) (= (len lst) 0)) "" (if (nil? (cdr lst)) (car lst) (str-concat (car lst) (str-concat sep (str-join sep (cdr lst))))))))
(define str-replace (lambda (s old new) (str-join new (str-split s old))))
(define str-repeat (lambda (s n) (if (<= n 0) "" (if (= n 1) s (str-concat s (str-repeat s (- n 1)))))))
(define str-pad-left (lambda (s len pad) (if (>= (str-length s) len) s (str-pad-left (str-concat pad s) len pad))))
(define str-pad-right (lambda (s len pad) (if (>= (str-length s) len) s (str-pad-right (str-concat s pad) len pad))))
"#;

const STDLIB_CRYPTO: &str = r#"
(define hash/sha256-bytes (lambda (s) (sha256 s)))
(define hash/keccak256-bytes (lambda (s) (keccak256 s)))
"#;

fn get_stdlib_code(name: &str) -> Option<&'static str> {
    match name {
        "math" => Some(MATH_STDLIB),
        "list" => Some(STDLIB_LIST),
        "string" => Some(STDLIB_STRING),
        "crypto" => Some(STDLIB_CRYPTO),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Source Location Tracking
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct SourceLoc {
    line: u32,
    col: u32,
}

impl std::fmt::Display for SourceLoc {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

#[derive(Clone, Debug)]
struct Token {
    text: String,
    loc: SourceLoc,
}

/// A parsed expression annotated with its source line number.
struct SpannedExpr {
    expr: LispVal,
    line: u32,
}

// ---------------------------------------------------------------------------
// Lisp Value
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, BorshSerialize, BorshDeserialize)]
pub enum LispVal {
    Nil,
    Bool(bool),
    Num(i64),
    Float(f64),
    Str(String),
    Sym(String),
    List(Vec<LispVal>),
    Lambda {
        params: Vec<String>,
        body: Box<LispVal>,
        closed_env: std::rc::Rc<Vec<(String, LispVal)>>,
    },
    /// Internal: recur signal — loop/recur tail-call optimization
    Recur(Vec<LispVal>),
    /// Map / dictionary — ordered key-value pairs
    Map(BTreeMap<String, LispVal>),
}

impl std::fmt::Display for LispVal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LispVal::Nil => write!(f, "nil"),
            LispVal::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            LispVal::Num(n) => write!(f, "{}", n),
            LispVal::Float(fl) => {
                // Format with enough precision, strip trailing zeros
                let s = format!("{:.10}", fl);
                let s = s.trim_end_matches('0');
                let s = s.trim_end_matches('.');
                write!(f, "{}", s)
            }
            LispVal::Str(s) => write!(f, "\"{}\"", s),
            LispVal::Sym(s) => write!(f, "{}", s),
            LispVal::List(vals) => {
                let parts: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
                write!(f, "({})", parts.join(" "))
            }
            LispVal::Lambda { params, .. } => {
                write!(f, "#<lambda ({})>", params.join(" "))
            }
            LispVal::Recur(vals) => {
                let parts: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
                write!(f, "#<recur ({})>", parts.join(" "))
            }
            LispVal::Map(m) => {
                let entries: Vec<String> =
                    m.iter().map(|(k, v)| format!("\"{}\": {}", k, v)).collect();
                write!(f, "{{{}}}", entries.join(", "))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tokenizer + Parser
// ---------------------------------------------------------------------------

fn tokenize(input: &str) -> Vec<String> {
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        if in_str {
            cur.push(ch);
            if ch == '"' {
                tokens.push(cur.clone());
                cur.clear();
                in_str = false;
            }
            i += 1;
        } else if ch == '"' && !in_str {
            in_str = true;
            cur.push(ch);
            i += 1;
        } else if ch == ';' && i + 1 < len && chars[i + 1] == ';' {
            // ;; line comment — skip to end of line
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
            i += 2;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            // skip the newline itself
            if i < len {
                i += 1;
            }
        } else if ch == '(' && i + 1 < len && chars[i + 1] == ';' {
            // (; block comment ;) — skip until matching ;)
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
            i += 2;
            while i + 1 < len {
                if chars[i] == ';' && chars[i + 1] == ')' {
                    i += 2;
                    break;
                }
                i += 1;
            }
        } else if ch == '(' || ch == ')' {
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
            tokens.push(ch.to_string());
            i += 1;
        } else if ch.is_whitespace() {
            if !cur.is_empty() {
                tokens.push(cur.clone());
                cur.clear();
            }
            i += 1;
        } else {
            cur.push(ch);
            i += 1;
        }
    }

    if !cur.is_empty() {
        tokens.push(cur);
    }
    tokens
}

fn parse(tokens: &[String], pos: &mut usize) -> Result<LispVal, String> {
    if *pos >= tokens.len() {
        return Err("unexpected EOF".into());
    }
    let tok = &tokens[*pos];
    *pos += 1;
    match tok.as_str() {
        "(" => {
            let mut list = Vec::new();
            while *pos < tokens.len() && tokens[*pos] != ")" {
                list.push(parse(tokens, pos)?);
            }
            if *pos >= tokens.len() {
                return Err("missing )".into());
            }
            *pos += 1;
            Ok(LispVal::List(list))
        }
        ")" => Err("unexpected )".into()),
        "nil" => Ok(LispVal::Nil),
        "true" => Ok(LispVal::Bool(true)),
        "false" => Ok(LispVal::Bool(false)),
        s if s.starts_with('"') => Ok(LispVal::Str(s[1..s.len() - 1].to_string())),
        s => {
            if let Ok(n) = s.parse::<i64>() {
                Ok(LispVal::Num(n))
            } else if s.contains('.') {
                s.parse::<f64>()
                    .map(LispVal::Float)
                    .or_else(|_| Ok(LispVal::Sym(s.to_string())))
            } else {
                Ok(LispVal::Sym(s.to_string()))
            }
        }
    }
}

pub fn parse_all(input: &str) -> Result<Vec<LispVal>, String> {
    let tokens = tokenize(input);
    let mut pos = 0;
    let mut exprs = Vec::new();
    while pos < tokens.len() {
        exprs.push(parse(&tokens, &mut pos)?);
    }
    Ok(exprs)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_truthy(v: &LispVal) -> bool {
    !matches!(v, LispVal::Nil | LispVal::Bool(false))
}

fn as_num(v: &LispVal) -> Result<i64, String> {
    match v {
        LispVal::Num(n) => Ok(*n),
        _ => Err(format!("expected number, got {}", v)),
    }
}

fn as_float(v: &LispVal) -> Result<f64, String> {
    match v {
        LispVal::Float(f) => Ok(*f),
        LispVal::Num(n) => Ok(*n as f64),
        _ => Err(format!("expected number, got {}", v)),
    }
}

/// Returns true if any argument is a Float (triggering promotion).
fn any_float(args: &[LispVal]) -> bool {
    args.iter().any(|a| matches!(a, LispVal::Float(_)))
}

fn as_str(v: &LispVal) -> Result<String, String> {
    match v {
        LispVal::Str(s) => Ok(s.clone()),
        LispVal::Sym(s) => Ok(s.clone()),
        LispVal::Num(n) => Ok(n.to_string()),
        _ => Err(format!("expected string, got {}", v)),
    }
}

/// Prepend the storage sandbox prefix from the env (if any).
/// If `__storage_prefix__` is set in env, keys become `{prefix}{key}`.
/// Otherwise, the raw key is used as-is (backward compatible).
fn sandbox_key(raw_key: &str, env: &[(String, LispVal)]) -> String {
    env.iter()
        .rev()
        .find(|(k, _)| k == "__storage_prefix__")
        .and_then(|(_, v)| match v {
            LispVal::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .map(|prefix| format!("{}{}", prefix, raw_key))
        .unwrap_or_else(|| raw_key.to_string())
}

fn do_arith(
    args: &[LispVal],
    op_int: fn(i64, i64) -> i64,
    op_float: fn(f64, f64) -> f64,
) -> Result<LispVal, String> {
    if args.len() < 2 {
        return Err("arith needs 2+ args".into());
    }
    if any_float(args) {
        let init = as_float(&args[0])?;
        let res: Result<f64, String> = args[1..]
            .iter()
            .try_fold(init, |a, b| Ok(op_float(a, as_float(b)?)));
        Ok(LispVal::Float(res?))
    } else {
        let init = as_num(&args[0])?;
        let res: Result<i64, String> = args[1..]
            .iter()
            .try_fold(init, |a, b| Ok(op_int(a, as_num(b)?)));
        Ok(LispVal::Num(res?))
    }
}

fn parse_params(val: &LispVal) -> Result<Vec<String>, String> {
    match val {
        LispVal::List(p) => p
            .iter()
            .map(|v| match v {
                LispVal::Sym(s) => Ok(s.clone()),
                _ => Err("param must be sym".into()),
            })
            .collect(),
        _ => Err("params must be list".into()),
    }
}

// ---------------------------------------------------------------------------
// Apply lambda — core closure logic
// closed_env: env captured at lambda creation
// caller_env: env at the call site (has recursive bindings like `fib`)
// ---------------------------------------------------------------------------

fn apply_lambda(
    params: &[String],
    body: &LispVal,
    closed_env: &std::rc::Rc<Vec<(String, LispVal)>>,
    args: &[LispVal],
    caller_env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<LispVal, String> {
    let mut local = (**closed_env).clone();
    local.extend(caller_env.iter().cloned());
    for (i, p) in params.iter().enumerate() {
        local.push((p.clone(), args.get(i).cloned().unwrap_or(LispVal::Nil)));
    }
    lisp_eval(body, &mut local, gas)
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

pub fn lisp_eval(
    expr: &LispVal,
    env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<LispVal, String> {
    if *gas == 0 {
        return Err("out of gas".into());
    }
    *gas -= 1;

    match expr {
        LispVal::Nil
        | LispVal::Bool(_)
        | LispVal::Num(_)
        | LispVal::Float(_)
        | LispVal::Str(_)
        | LispVal::Lambda { .. }
        | LispVal::Map(_) => Ok(expr.clone()),
        LispVal::Recur(_) => Err("recur outside loop".into()),
        LispVal::Sym(name) => env
            .iter()
            .rev()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.clone())
            .ok_or_else(|| format!("undefined: {}", name)),
        LispVal::List(list) if list.is_empty() => Ok(LispVal::Nil),
        LispVal::List(list) => {
            if let LispVal::Sym(name) = &list[0] {
                match name.as_str() {
                    "quote" => Ok(list.get(1).cloned().unwrap_or(LispVal::Nil)),
                    "define" => {
                        let var = match list.get(1) {
                            Some(LispVal::Sym(s)) => s.clone(),
                            _ => return Err("define: need symbol".into()),
                        };
                        let val = match list.get(2) {
                            Some(v) => lisp_eval(v, env, gas)?,
                            None => LispVal::Nil,
                        };
                        env.push((var, val));
                        Ok(LispVal::Nil)
                    }
                    "if" => {
                        let cond = lisp_eval(list.get(1).ok_or("if: need cond")?, env, gas)?;
                        if is_truthy(&cond) {
                            lisp_eval(list.get(2).ok_or("if: need then")?, env, gas)
                        } else {
                            list.get(3)
                                .map(|e| lisp_eval(e, env, gas))
                                .unwrap_or(Ok(LispVal::Nil))
                        }
                    }
                    "cond" => {
                        for clause in &list[1..] {
                            if let LispVal::List(parts) = clause {
                                if parts.is_empty() {
                                    continue;
                                }
                                if let LispVal::Sym(kw) = &parts[0] {
                                    if kw == "else" {
                                        return parts
                                            .get(1)
                                            .map(|e| lisp_eval(e, env, gas))
                                            .unwrap_or(Ok(LispVal::Nil));
                                    }
                                }
                                let test = lisp_eval(&parts[0], env, gas)?;
                                if is_truthy(&test) {
                                    return parts
                                        .get(1)
                                        .map(|e| lisp_eval(e, env, gas))
                                        .unwrap_or(Ok(test));
                                }
                            }
                        }
                        Ok(LispVal::Nil)
                    }
                    "let" => {
                        let bindings = match list.get(1) {
                            Some(LispVal::List(b)) => b,
                            _ => return Err("let: bindings must be list".into()),
                        };
                        let mut local = env.clone();
                        for b in bindings {
                            if let LispVal::List(pair) = b {
                                if pair.len() == 2 {
                                    if let LispVal::Sym(name) = &pair[0] {
                                        let val = lisp_eval(&pair[1], &mut local, gas)?;
                                        local.push((name.clone(), val));
                                    }
                                }
                            }
                        }
                        list.get(2)
                            .map(|e| lisp_eval(e, &mut local, gas))
                            .unwrap_or(Ok(LispVal::Nil))
                    }
                    "lambda" => {
                        let params = parse_params(list.get(1).ok_or("lambda: need params")?)?;
                        let body = list.get(2).ok_or("lambda: need body")?;
                        Ok(LispVal::Lambda {
                            params,
                            body: Box::new(body.clone()),
                            closed_env: std::rc::Rc::new(env.clone()),
                        })
                    }
                    "progn" | "begin" => {
                        let mut r = LispVal::Nil;
                        for e in &list[1..] {
                            r = lisp_eval(e, env, gas)?;
                        }
                        Ok(r)
                    }
                    "and" => {
                        let mut r = LispVal::Bool(true);
                        for e in &list[1..] {
                            r = lisp_eval(e, env, gas)?;
                            if !is_truthy(&r) {
                                return Ok(r);
                            }
                        }
                        Ok(r)
                    }
                    "or" => {
                        for e in &list[1..] {
                            let r = lisp_eval(e, env, gas)?;
                            if is_truthy(&r) {
                                return Ok(r);
                            }
                        }
                        Ok(LispVal::Bool(false))
                    }
                    "not" => {
                        let v = lisp_eval(list.get(1).ok_or("not: need arg")?, env, gas)?;
                        Ok(LispVal::Bool(!is_truthy(&v)))
                    }
                    // Clojure-style loop/recur — tail-call optimization
                    "loop" => {
                        let bindings = match list.get(1) {
                            Some(LispVal::List(b)) => b,
                            _ => return Err("loop: bindings must be list".into()),
                        };
                        let body = list.get(2).ok_or("loop: need body")?;
                        let mut binding_names: Vec<String> = Vec::new();
                        let mut binding_vals: Vec<LispVal> = Vec::new();
                        let is_pair_style = bindings.iter().all(|b| matches!(b, LispVal::List(_)));
                        if is_pair_style {
                            for b in bindings {
                                if let LispVal::List(pair) = b {
                                    if pair.len() == 2 {
                                        if let LispVal::Sym(name) = &pair[0] {
                                            binding_names.push(name.clone());
                                            binding_vals.push(lisp_eval(&pair[1], env, gas)?);
                                        }
                                    }
                                }
                            }
                        } else {
                            if bindings.len() % 2 != 0 {
                                return Err("loop: flat bindings need even count".into());
                            }
                            let mut i = 0;
                            while i < bindings.len() {
                                if let LispVal::Sym(name) = &bindings[i] {
                                    binding_names.push(name.clone());
                                    binding_vals.push(lisp_eval(&bindings[i + 1], env, gas)?);
                                } else {
                                    return Err(format!(
                                        "loop: binding name must be sym, got {}",
                                        bindings[i]
                                    ));
                                }
                                i += 2;
                            }
                        }
                        loop {
                            let mut local = env.clone();
                            for (i, name) in binding_names.iter().enumerate() {
                                local.push((name.clone(), binding_vals[i].clone()));
                            }
                            match lisp_eval(body, &mut local, gas)? {
                                LispVal::Recur(new_vals) => {
                                    if new_vals.len() != binding_names.len() {
                                        return Err(format!(
                                            "recur: expected {} args, got {}",
                                            binding_names.len(),
                                            new_vals.len()
                                        ));
                                    }
                                    binding_vals = new_vals;
                                }
                                other => return Ok(other),
                            }
                        }
                    }
                    "recur" => {
                        let vals: Vec<LispVal> = list[1..]
                            .iter()
                            .map(|a| lisp_eval(a, env, gas))
                            .collect::<Result<_, _>>()?;
                        Ok(LispVal::Recur(vals))
                    }
                    // near/ccall-result: returns the last cross-contract call result
                    "near/ccall-result" => env
                        .iter()
                        .rev()
                        .find(|(k, _)| k == "__ccall_result__")
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| "near/ccall-result: no pending result".into()),
                    // near/batch-result: returns ALL accumulated ccall results as a list
                    "near/batch-result" => env
                        .iter()
                        .rev()
                        .find(|(k, _)| k == "__ccall_results__")
                        .map(|(_, v)| v.clone())
                        .ok_or_else(|| "near/batch-result: no results yet".into()),
                    // near/ccall-count: returns how many ccall results have been accumulated
                    "near/ccall-count" => {
                        let count = env
                            .iter()
                            .rev()
                            .find(|(k, _)| k == "__ccall_results__")
                            .map(|(_, v)| match v {
                                LispVal::List(vals) => vals.len() as i64,
                                _ => 0,
                            })
                            .unwrap_or(0);
                        Ok(LispVal::Num(count))
                    }
                    "near/block-height" => Ok(LispVal::Num(env::block_height() as i64)),
                    "near/predecessor" => {
                        Ok(LispVal::Str(env::predecessor_account_id().to_string()))
                    }
                    "near/signer" => Ok(LispVal::Str(env::signer_account_id().to_string())),
                    "near/timestamp" => Ok(LispVal::Num(env::block_timestamp() as i64)),
                    "near/account-balance" => Ok(LispVal::Str(
                        env::account_balance().as_yoctonear().to_string(),
                    )),
                    "near/attached-deposit" => Ok(LispVal::Str(
                        env::attached_deposit().as_yoctonear().to_string(),
                    )),
                    "near/account-locked-balance" => Ok(LispVal::Str(
                        env::account_locked_balance().as_yoctonear().to_string(),
                    )),
                    "near/log" => {
                        let v = lisp_eval(list.get(1).ok_or("near/log: need arg")?, env, gas)?;
                        env::log_str(&v.to_string());
                        Ok(LispVal::Nil)
                    }
                    "require" => {
                        let module_name = match list.get(1) {
                            Some(LispVal::Str(s)) => s.as_str(),
                            _ => return Err("require: need string module name".into()),
                        };
                        let code = get_stdlib_code(module_name)
                            .ok_or_else(|| format!("require: unknown module '{}'", module_name))?;
                        let module_exprs = parse_all(code)?;
                        for expr in &module_exprs {
                            lisp_eval(expr, env, gas)?;
                        }
                        Ok(LispVal::Nil)
                    }
                    _ => dispatch_call(list, env, gas),
                }
            } else {
                dispatch_call(list, env, gas)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Function dispatch (builtins + lambda calls)
// ---------------------------------------------------------------------------

fn dispatch_call(
    list: &[LispVal],
    env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<LispVal, String> {
    let head = &list[0];
    let args: Vec<LispVal> = list[1..]
        .iter()
        .map(|a| lisp_eval(a, env, gas))
        .collect::<Result<_, _>>()?;

    if let LispVal::Sym(name) = head {
        match name.as_str() {
            "+" => do_arith(&args, |a, b| a + b, |a, b| a + b),
            "-" => do_arith(&args, |a, b| a - b, |a, b| a - b),
            "*" => do_arith(&args, |a, b| a * b, |a, b| a * b),
            "/" => {
                if any_float(&args) {
                    let b = as_float(args.get(1).ok_or("/ needs 2 args")?)?;
                    if b == 0.0 {
                        return Err("div by zero".into());
                    }
                    Ok(LispVal::Float(as_float(&args[0])? / b))
                } else {
                    let b = as_num(args.get(1).ok_or("/ needs 2 args")?)?;
                    if b == 0 {
                        return Err("div by zero".into());
                    }
                    Ok(LispVal::Num(as_num(&args[0])? / b))
                }
            }
            "mod" => do_arith(&args, |a, b| i64::rem_euclid(a, b), |a, b| a % b),
            "=" | "==" => Ok(LispVal::Bool(args.get(0) == args.get(1))),
            "!=" | "/=" => Ok(LispVal::Bool(args.get(0) != args.get(1))),
            "<" => {
                if any_float(&args) {
                    Ok(LispVal::Bool(as_float(&args[0])? < as_float(&args[1])?))
                } else {
                    Ok(LispVal::Bool(as_num(&args[0])? < as_num(&args[1])?))
                }
            }
            ">" => {
                if any_float(&args) {
                    Ok(LispVal::Bool(as_float(&args[0])? > as_float(&args[1])?))
                } else {
                    Ok(LispVal::Bool(as_num(&args[0])? > as_num(&args[1])?))
                }
            }
            "<=" => {
                if any_float(&args) {
                    Ok(LispVal::Bool(as_float(&args[0])? <= as_float(&args[1])?))
                } else {
                    Ok(LispVal::Bool(as_num(&args[0])? <= as_num(&args[1])?))
                }
            }
            ">=" => {
                if any_float(&args) {
                    Ok(LispVal::Bool(as_float(&args[0])? >= as_float(&args[1])?))
                } else {
                    Ok(LispVal::Bool(as_num(&args[0])? >= as_num(&args[1])?))
                }
            }
            "list" => Ok(LispVal::List(args)),
            "car" => match args.get(0) {
                Some(LispVal::List(l)) if !l.is_empty() => Ok(l[0].clone()),
                _ => Ok(LispVal::Nil),
            },
            "cdr" => match args.get(0) {
                Some(LispVal::List(l)) if l.len() > 1 => Ok(LispVal::List(l[1..].to_vec())),
                _ => Ok(LispVal::Nil),
            },
            "cons" => match args.get(1) {
                Some(LispVal::List(l)) => {
                    let mut n = vec![args[0].clone()];
                    n.extend(l.iter().cloned());
                    Ok(LispVal::List(n))
                }
                _ => Ok(LispVal::List(args)),
            },
            "len" => match args.get(0) {
                Some(LispVal::List(l)) => Ok(LispVal::Num(l.len() as i64)),
                Some(LispVal::Str(s)) => Ok(LispVal::Num(s.len() as i64)),
                _ => Err("len: need list or string".into()),
            },
            "append" => {
                let mut r = Vec::new();
                for a in &args {
                    if let LispVal::List(l) = a {
                        r.extend(l.iter().cloned());
                    } else {
                        r.push(a.clone());
                    }
                }
                Ok(LispVal::List(r))
            }
            "nth" => {
                let i = as_num(args.get(0).ok_or("nth: need index")?)? as usize;
                match args.get(1) {
                    Some(LispVal::List(l)) => l.get(i).cloned().ok_or("index out of range".into()),
                    _ => Err("nth: need list".into()),
                }
            }
            "str-concat" => {
                let parts: Vec<String> = args
                    .iter()
                    .map(|a| match a {
                        LispVal::Str(s) => s.clone(),
                        _ => a.to_string(),
                    })
                    .collect();
                Ok(LispVal::Str(parts.join("")))
            }
            "str-contains" => Ok(LispVal::Bool(
                as_str(&args[0])?.contains(&as_str(&args[1])?),
            )),
            "to-string" => Ok(LispVal::Str(args[0].to_string())),
            "str-length" => {
                let s = as_str(&args[0])?;
                Ok(LispVal::Num(s.chars().count() as i64))
            }
            "str-substring" => {
                // (str-substring s start end) — char-based indices
                let s = as_str(&args[0])?;
                let start = as_num(args.get(1).ok_or("str-substring: need start")?)? as usize;
                let end = as_num(args.get(2).ok_or("str-substring: need end")?)? as usize;
                let chars: Vec<char> = s.chars().collect();
                if start > end || end > chars.len() {
                    return Err(format!(
                        "str-substring: indices out of range ({}..{} for len {})",
                        start,
                        end,
                        chars.len()
                    ));
                }
                Ok(LispVal::Str(chars[start..end].iter().collect()))
            }
            "str-split" => {
                // (str-split s delimiter) => list of strings
                let s = as_str(&args[0])?;
                let delim = as_str(args.get(1).ok_or("str-split: need delimiter")?)?;
                let parts: Vec<LispVal> = s
                    .split(&delim)
                    .map(|p| LispVal::Str(p.to_string()))
                    .collect();
                Ok(LispVal::List(parts))
            }
            "str-trim" => {
                let s = as_str(&args[0])?;
                Ok(LispVal::Str(s.trim().to_string()))
            }
            "str-index-of" => {
                // (str-index-of haystack needle) => index or -1
                let haystack = as_str(&args[0])?;
                let needle = as_str(args.get(1).ok_or("str-index-of: need needle")?)?;
                let idx = haystack.find(&needle).map(|i| i as i64).unwrap_or(-1);
                Ok(LispVal::Num(idx))
            }
            "str-upcase" => {
                let s = as_str(&args[0])?;
                Ok(LispVal::Str(s.to_uppercase()))
            }
            "str-downcase" => {
                let s = as_str(&args[0])?;
                Ok(LispVal::Str(s.to_lowercase()))
            }
            "str-starts-with" => {
                let s = as_str(&args[0])?;
                let prefix = as_str(args.get(1).ok_or("str-starts-with: need prefix")?)?;
                Ok(LispVal::Bool(s.starts_with(&prefix)))
            }
            "str-ends-with" => {
                let s = as_str(&args[0])?;
                let suffix = as_str(args.get(1).ok_or("str-ends-with: need suffix")?)?;
                Ok(LispVal::Bool(s.ends_with(&suffix)))
            }
            "nil?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Nil))),
            "list?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::List(_)))),
            "number?" => Ok(LispVal::Bool(matches!(
                &args[0],
                LispVal::Num(_) | LispVal::Float(_)
            ))),
            "to-float" => match &args[0] {
                LispVal::Float(f) => Ok(LispVal::Float(*f)),
                LispVal::Num(n) => Ok(LispVal::Float(*n as f64)),
                LispVal::Str(s) => s
                    .parse::<f64>()
                    .map(LispVal::Float)
                    .map_err(|_| format!("to-float: cannot parse '{}'", s)),
                other => Err(format!("to-float: expected number, got {}", other)),
            },
            "to-int" => match &args[0] {
                LispVal::Num(n) => Ok(LispVal::Num(*n)),
                LispVal::Float(f) => Ok(LispVal::Num(*f as i64)),
                LispVal::Str(s) => s
                    .parse::<i64>()
                    .map(LispVal::Num)
                    .map_err(|_| format!("to-int: cannot parse '{}'", s)),
                other => Err(format!("to-int: expected number, got {}", other)),
            },
            "string?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Str(_)))),
            "map?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Map(_)))),

            // --- Dict / Map builtins ---
            "dict" => {
                // (dict) => empty map
                // (dict "k1" v1 "k2" v2 ...) => map with pairs
                let mut m = BTreeMap::new();
                let mut i = 0;
                while i + 1 < args.len() {
                    let key = as_str(&args[i]).map_err(|_| "dict: keys must be strings")?;
                    m.insert(key, args[i + 1].clone());
                    i += 2;
                }
                Ok(LispVal::Map(m))
            }
            "dict/get" => {
                let m = match &args[0] {
                    LispVal::Map(m) => m,
                    _ => return Err("dict/get: expected map".into()),
                };
                let key = as_str(&args[1]).map_err(|_| "dict/get: key must be string")?;
                Ok(m.get(&key).cloned().unwrap_or(LispVal::Nil))
            }
            "dict/set" => {
                let mut m = match &args[0] {
                    LispVal::Map(m) => m.clone(),
                    _ => return Err("dict/set: expected map".into()),
                };
                let key = as_str(&args[1]).map_err(|_| "dict/set: key must be string")?;
                m.insert(key, args.get(2).cloned().unwrap_or(LispVal::Nil));
                Ok(LispVal::Map(m))
            }
            "dict/has?" => {
                let m = match &args[0] {
                    LispVal::Map(m) => m,
                    _ => return Err("dict/has?: expected map".into()),
                };
                let key = as_str(&args[1]).map_err(|_| "dict/has?: key must be string")?;
                Ok(LispVal::Bool(m.contains_key(&key)))
            }
            "dict/keys" => {
                let m = match &args[0] {
                    LispVal::Map(m) => m,
                    _ => return Err("dict/keys: expected map".into()),
                };
                Ok(LispVal::List(
                    m.keys().map(|k| LispVal::Str(k.clone())).collect(),
                ))
            }
            "dict/vals" => {
                let m = match &args[0] {
                    LispVal::Map(m) => m,
                    _ => return Err("dict/vals: expected map".into()),
                };
                Ok(LispVal::List(m.values().cloned().collect()))
            }
            "dict/remove" => {
                let mut m = match &args[0] {
                    LispVal::Map(m) => m.clone(),
                    _ => return Err("dict/remove: expected map".into()),
                };
                let key = as_str(&args[1]).map_err(|_| "dict/remove: key must be string")?;
                m.remove(&key);
                Ok(LispVal::Map(m))
            }
            "dict/merge" => {
                let mut m = match &args[0] {
                    LispVal::Map(m) => m.clone(),
                    _ => return Err("dict/merge: first arg must be map".into()),
                };
                match &args[1] {
                    LispVal::Map(m2) => {
                        for (k, v) in m2 {
                            m.insert(k.clone(), v.clone());
                        }
                    }
                    _ => return Err("dict/merge: second arg must be map".into()),
                }
                Ok(LispVal::Map(m))
            }

            // --- Storage (namespaced by __storage_prefix__) ---
            "near/storage-write" => {
                consume_gas(gas, STORAGE_GAS_COST, "near/storage-write")?;
                let raw_key = as_str(&args[0])?;
                let val = as_str(&args[1])?;
                let key = sandbox_key(&raw_key, env);
                env::storage_write(key.as_bytes(), val.as_bytes());
                Ok(LispVal::Bool(true))
            }
            "near/storage-read" => {
                consume_gas(gas, STORAGE_GAS_COST, "near/storage-read")?;
                let raw_key = as_str(&args[0])?;
                let key = sandbox_key(&raw_key, env);
                Ok(env::storage_read(key.as_bytes())
                    .map(|v| LispVal::Str(String::from_utf8_lossy(&v).to_string()))
                    .unwrap_or(LispVal::Nil))
            }
            "near/storage-remove" => {
                consume_gas(gas, STORAGE_GAS_COST, "near/storage-remove")?;
                let raw_key = as_str(&args[0])?;
                let key = sandbox_key(&raw_key, env);
                env::storage_remove(key.as_bytes());
                Ok(LispVal::Bool(true))
            }
            "near/storage-has?" => {
                consume_gas(gas, STORAGE_GAS_COST, "near/storage-has?")?;
                let raw_key = as_str(&args[0])?;
                let key = sandbox_key(&raw_key, env);
                Ok(LispVal::Bool(env::storage_has_key(key.as_bytes())))
            }

            // --- Cryptographic hashing ---
            "sha256" => {
                let data = as_str(&args[0])?;
                let hash = env::sha256(data.as_bytes());
                Ok(LispVal::Str(hex_encode(&hash)))
            }
            "keccak256" => {
                let data = as_str(&args[0])?;
                let hash = env::keccak256(data.as_bytes());
                Ok(LispVal::Str(hex_encode(&hash)))
            }

            // --- Signature verification ---
            "ed25519-verify" => {
                // (ed25519-verify sig-hex msg-hex pk-hex)
                let sig_bytes = hex_decode(&as_str(&args[0])?);
                let msg = as_str(&args[1])?;
                let pk_bytes = hex_decode(&as_str(&args[2])?);
                let sig: [u8; 64] = sig_bytes
                    .try_into()
                    .map_err(|_| "ed25519-verify: signature must be 64 bytes (128 hex chars)")?;
                let pk: [u8; 32] = pk_bytes
                    .try_into()
                    .map_err(|_| "ed25519-verify: public key must be 32 bytes (64 hex chars)")?;
                Ok(LispVal::Bool(env::ed25519_verify(
                    &sig,
                    msg.as_bytes(),
                    &pk,
                )))
            }
            "ecrecover" => {
                // (ecrecover hash-hex sig-hex v malleability_flag)
                // Returns 64-byte public key as hex, or nil on failure
                let hash_bytes = hex_decode(&as_str(&args[0])?);
                let sig_bytes = hex_decode(&as_str(&args[1])?);
                let v = as_num(&args[2])? as u8;
                let malleability = match args.get(3) {
                    Some(LispVal::Bool(b)) => *b,
                    _ => true,
                };
                if hash_bytes.len() != 32 {
                    return Err("ecrecover: hash must be 32 bytes (64 hex chars)".into());
                }
                if sig_bytes.len() != 65 {
                    return Err("ecrecover: signature must be 65 bytes (130 hex chars)".into());
                }
                match env::ecrecover(&hash_bytes, &sig_bytes, v, malleability) {
                    Some(pk) => Ok(LispVal::Str(hex_encode(&pk))),
                    None => Ok(LispVal::Nil),
                }
            }

            // --- Chain state ---
            "near/transfer" => {
                // (near/transfer amount_yocto recipient)
                // Creates a Promise transfer. Only works in async context.
                let amount_str = as_str(&args[0])
                    .map_err(|_| "near/transfer: amount must be string (yoctoNEAR)")?;
                let amount_u128: u128 = amount_str
                    .parse()
                    .map_err(|_| "near/transfer: invalid amount")?;
                let recipient_str =
                    as_str(&args[1]).map_err(|_| "near/transfer: recipient must be string")?;
                let recipient_id: AccountId = recipient_str
                    .parse()
                    .map_err(|_| "near/transfer: invalid account id")?;
                let _ = Promise::new(recipient_id).transfer(NearToken::from_yoctonear(amount_u128));
                Ok(LispVal::Str(format!(
                    "transfer:{}:{}",
                    amount_str, recipient_str
                )))
            }
            "near/signer=" => {
                let expected = as_str(&args[0])?;
                Ok(LispVal::Bool(
                    env::signer_account_id().to_string() == expected,
                ))
            }
            "near/predecessor=" => {
                let expected = as_str(&args[0])?;
                Ok(LispVal::Bool(
                    env::predecessor_account_id().to_string() == expected,
                ))
            }

            "to-json" => {
                let json_val = lisp_to_json(&args[0]);
                serde_json::to_string(&json_val)
                    .map(LispVal::Str)
                    .map_err(|e| format!("to-json: {}", e))
            }
            "from-json" => {
                let s = as_str(&args[0]).map_err(|_| "from-json: expected string")?;
                let parsed: serde_json::Value =
                    serde_json::from_str(&s).map_err(|e| format!("from-json: {}", e))?;
                Ok(json_to_lisp(parsed))
            }

            _ => {
                // Lambda lookup
                let func = env
                    .iter()
                    .rev()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| format!("undefined: {}", name))?;
                call_val(&func, &args, env, gas)
            }
        }
    } else if let LispVal::Lambda {
        params,
        body,
        closed_env,
    } = head
    {
        apply_lambda(params, body, closed_env, &args, env, gas)
    } else if let LispVal::List(ll) = head {
        // Inline lambda: ((lambda (x) (* x x)) 5)
        if ll.len() < 3 {
            return Err("inline lambda too short".into());
        }
        let params = parse_params(&ll[1])?;
        apply_lambda(&params, &ll[2], &std::rc::Rc::new(vec![]), &args, env, gas)
    } else {
        Err("not callable".into())
    }
}

fn call_val(
    func: &LispVal,
    args: &[LispVal],
    env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<LispVal, String> {
    match func {
        LispVal::Lambda {
            params,
            body,
            closed_env,
        } => apply_lambda(params, body, closed_env, args, env, gas),
        LispVal::List(ll) if ll.len() >= 3 => {
            let params = parse_params(&ll[1])?;
            apply_lambda(&params, &ll[2], &std::rc::Rc::new(vec![]), args, env, gas)
        }
        _ => Err(format!("not callable: {}", func)),
    }
}

// ---------------------------------------------------------------------------
// Public interface — synchronous eval (no ccall support)
// ---------------------------------------------------------------------------

pub fn run_program(
    code: &str,
    env: &mut Vec<(String, LispVal)>,
    gas_limit: u64,
) -> Result<String, String> {
    let exprs = parse_all(code)?;
    let mut gas = gas_limit;
    let mut result = LispVal::Nil;
    for expr in exprs {
        result = lisp_eval(&expr, env, &mut gas)?;
    }
    Ok(result.to_string())
}

pub fn json_to_lisp(val: serde_json::Value) -> LispVal {
    match val {
        serde_json::Value::Null => LispVal::Nil,
        serde_json::Value::Bool(b) => LispVal::Bool(b),
        serde_json::Value::Number(n) => LispVal::Num(n.as_i64().unwrap_or(0)),
        serde_json::Value::String(s) => LispVal::Str(s),
        serde_json::Value::Array(a) => LispVal::List(a.into_iter().map(json_to_lisp).collect()),
        serde_json::Value::Object(m) => {
            let map: BTreeMap<String, LispVal> =
                m.into_iter().map(|(k, v)| (k, json_to_lisp(v))).collect();
            LispVal::Map(map)
        }
    }
}

pub fn lisp_to_json(val: &LispVal) -> serde_json::Value {
    match val {
        LispVal::Nil => serde_json::Value::Null,
        LispVal::Bool(b) => serde_json::Value::Bool(*b),
        LispVal::Num(n) => serde_json::Value::Number(serde_json::Number::from(*n)),
        LispVal::Float(f) => {
            if let Some(n) = serde_json::Number::from_f64(*f) {
                serde_json::Value::Number(n)
            } else {
                serde_json::Value::Null
            }
        }
        LispVal::Str(s) => serde_json::Value::String(s.clone()),
        LispVal::List(items) => serde_json::Value::Array(items.iter().map(lisp_to_json).collect()),
        LispVal::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), lisp_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        // Symbols, lambdas, recur — convert to string representation
        other => serde_json::Value::String(other.to_string()),
    }
}

// ===========================================================================
// LAYER 1: Yield/Resume — VM state serialization
// ===========================================================================

/// Serializable VM state — captures everything needed to resume evaluation.
/// Stored in contract storage between yield and resume.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug)]
pub struct VmState {
    /// Remaining top-level expressions to evaluate on resume.
    pub remaining: Vec<LispVal>,
    /// Accumulated environment bindings.
    pub env: Vec<(String, LispVal)>,
    /// Gas remaining.
    pub gas: u64,
    /// Variable name being defined when yield happened.
    /// `Some("price")` for `(define price (near/ccall ...))`
    /// `None` for standalone `(near/ccall ...)`
    pub pending_var: Option<String>,
}

/// Result of running a program that may contain cross-contract calls.
pub enum RunResult {
    /// Evaluation completed synchronously.
    Done(String),
    /// Evaluation paused at a cross-contract call. Contains everything needed
    /// to create the promise, yield, and resume later.
    Yield(CcallYield),
}

/// Pending cross-contract call that requires a yield.
pub struct CcallYield {
    pub account: String,
    pub method: String,
    pub args_bytes: Vec<u8>,
    pub state: VmState,
    /// Deposit in yoctoNEAR (0 for view calls).
    pub deposit: u128,
    /// Gas in TeraGas (50 TGas default for view calls).
    pub gas_tgas: u64,
}

/// Internal: extracted cross-contract call info from an expression.
struct CcallInfo {
    pending_var: Option<String>,
    account: String,
    method: String,
    args_bytes: Vec<u8>,
    /// Deposit in yoctoNEAR (0 for view calls).
    deposit: u128,
    /// Gas in TeraGas (50 TGas default for view calls).
    gas_tgas: u64,
}

/// Helper: classify a ccall function name and return its mode.
/// Returns `None` if not a ccall function.
fn classify_ccall(name: &str) -> Option<CcallMode> {
    match name {
        "near/ccall" | "near/ccall-view" => Some(CcallMode::View),
        "near/ccall-call" => Some(CcallMode::Call),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum CcallMode {
    View,
    Call,
}

/// Helper: extract ccall info from the inner argument list of a ccall expression.
/// `func_name` is the original function name (e.g. "near/ccall-call").
/// `inner` is [func_sym, account, method, args_json, (deposit, gas)?]
fn extract_ccall_info(
    inner: &[LispVal],
    env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
    pending_var: Option<String>,
) -> Result<Option<CcallInfo>, String> {
    if inner.len() < 3 {
        return Ok(None);
    }
    if let LispVal::Sym(func) = &inner[0] {
        let mode = match classify_ccall(func) {
            Some(m) => m,
            None => return Ok(None),
        };

        let account = as_str(&inner[1])?;
        let method = as_str(&inner[2])?;
        let args_val = inner
            .get(3)
            .map(|a| lisp_eval(a, env, gas))
            .transpose()?
            .unwrap_or(LispVal::Str("{}".to_string()));
        let args_bytes = match &args_val {
            LispVal::Str(s) => s.clone().into_bytes(),
            other => other.to_string().into_bytes(),
        };

        let (deposit, gas_tgas) = match mode {
            CcallMode::View => (0u128, 50u64),
            CcallMode::Call => {
                let deposit_str =
                    inner
                        .get(4)
                        .map(|a| as_str(a))
                        .transpose()?
                        .ok_or_else(|| {
                            "near/ccall-call: need deposit (yoctonear string)".to_string()
                        })?;
                let deposit: u128 = deposit_str
                    .parse()
                    .map_err(|_| "near/ccall-call: invalid deposit".to_string())?;
                let gas_str = inner
                    .get(5)
                    .map(|a| as_str(a))
                    .transpose()?
                    .ok_or_else(|| "near/ccall-call: need gas (tgas)".to_string())?;
                let gas_tgas: u64 = gas_str
                    .parse()
                    .map_err(|_| "near/ccall-call: invalid gas".to_string())?;
                (deposit, gas_tgas)
            }
        };

        return Ok(Some(CcallInfo {
            pending_var,
            account,
            method,
            args_bytes,
            deposit,
            gas_tgas,
        }));
    }
    Ok(None)
}

/// Check if a top-level expression is a `(near/ccall[-view|-call] ...)` that requires yielding.
/// Supports two patterns:
///   (define var (near/ccall "account" "method" "args_json"))
///   (near/ccall "account" "method" "args_json")
///   (near/ccall-view "account" "method" "args_json")
///   (near/ccall-call "account" "method" "args_json" "deposit_yocto" "gas_tgas")
fn check_ccall(
    expr: &LispVal,
    env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<Option<CcallInfo>, String> {
    let list = match expr {
        LispVal::List(l) if l.len() >= 3 => l,
        _ => return Ok(None),
    };

    // Pattern 1: (define var (near/ccall[-view|-call] account method args [deposit gas]))
    if let [LispVal::Sym(define), LispVal::Sym(var), LispVal::List(inner)] = &list[..] {
        if define == "define" && inner.len() >= 3 {
            if let Some(info) = extract_ccall_info(inner, env, gas, Some(var.clone()))? {
                return Ok(Some(info));
            }
        }
    }

    // Pattern 2: (near/ccall[-view|-call] account method args [deposit gas]) standalone
    if let Some(info) = extract_ccall_info(list, env, gas, None)? {
        return Ok(Some(info));
    }

    Ok(None)
}

/// Run a program that may contain cross-contract calls.
/// Returns `RunResult::Yield` if a `(near/ccall ...)` is encountered at the
/// top level, with the full VM state captured for later resumption.
pub fn run_program_with_ccall(
    code: &str,
    env: &mut Vec<(String, LispVal)>,
    gas_limit: u64,
) -> Result<RunResult, String> {
    let exprs = parse_all(code)?;
    let mut gas = gas_limit;
    let mut result = LispVal::Nil;

    for (i, expr) in exprs.iter().enumerate() {
        // Before evaluating, check if this expression requires a ccall yield
        if let Some(ccall_info) = check_ccall(expr, env, &mut gas)? {
            return Ok(RunResult::Yield(CcallYield {
                account: ccall_info.account,
                method: ccall_info.method,
                args_bytes: ccall_info.args_bytes,
                deposit: ccall_info.deposit,
                gas_tgas: ccall_info.gas_tgas,
                state: VmState {
                    remaining: exprs[i + 1..].to_vec(),
                    env: env.clone(),
                    gas,
                    pending_var: ccall_info.pending_var,
                },
            }));
        }
        result = lisp_eval(expr, env, &mut gas)?;
    }

    Ok(RunResult::Done(result.to_string()))
}

/// Run a list of already-parsed expressions that may contain cross-contract calls.
/// Like `run_program_with_ccall` but takes pre-parsed `Vec<LispVal>` instead of code string.
/// Used by `resume_eval` to continue evaluating remaining expressions after a yield.
pub fn run_remaining_with_ccall(
    exprs: &[LispVal],
    env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<RunResult, String> {
    let mut result = LispVal::Nil;

    for (i, expr) in exprs.iter().enumerate() {
        // Before evaluating, check if this expression requires a ccall yield
        if let Some(ccall_info) = check_ccall(expr, env, gas)? {
            return Ok(RunResult::Yield(CcallYield {
                account: ccall_info.account,
                method: ccall_info.method,
                args_bytes: ccall_info.args_bytes,
                deposit: ccall_info.deposit,
                gas_tgas: ccall_info.gas_tgas,
                state: VmState {
                    remaining: exprs[i + 1..].to_vec(),
                    env: env.clone(),
                    gas: *gas,
                    pending_var: ccall_info.pending_var,
                },
            }));
        }
        result = lisp_eval(expr, env, gas)?;
    }

    Ok(RunResult::Done(result.to_string()))
}

// ---------------------------------------------------------------------------
// Hex helpers (avoids adding hex crate dependency)
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hex_decode(hex: &str) -> Vec<u8> {
    (0..hex.len())
        .step_by(2)
        .filter_map(|i| {
            hex.get(i..i + 2)
                .and_then(|s| u8::from_str_radix(s, 16).ok())
        })
        .collect()
}

// ===========================================================================
// LAYER 2 + 3: Contract methods — cross-contract call + resume
// ===========================================================================

#[near(contract_state)]
pub struct LispContract {
    owner: AccountId,
    eval_gas_limit: u64,
    policy_names: IterableSet<String>,
    script_names: IterableSet<String>,
    eval_whitelist: IterableSet<AccountId>,
}

impl Default for LispContract {
    fn default() -> Self {
        Self {
            owner: env::signer_account_id(),
            eval_gas_limit: 10_000,
            policy_names: IterableSet::new(b"p"),
            script_names: IterableSet::new(b"s"),
            eval_whitelist: IterableSet::new(b"w"),
        }
    }
}

#[near]
impl LispContract {
    #[init]
    pub fn new(eval_gas_limit: u64) -> Self {
        Self {
            owner: env::signer_account_id(),
            eval_gas_limit: if eval_gas_limit == 0 {
                10_000
            } else {
                eval_gas_limit
            },
            policy_names: IterableSet::new(b"p"),
            script_names: IterableSet::new(b"s"),
            eval_whitelist: IterableSet::new(b"w"),
        }
    }

    // --- Eval access control (whitelist) ---

    /// Returns true if the caller is allowed to eval.
    /// If the whitelist is empty, all callers are allowed (backward compatible).
    /// If the whitelist has entries, only whitelisted accounts may eval.
    fn is_eval_allowed(&self) -> bool {
        if self.eval_whitelist.is_empty() {
            return true;
        }
        let caller = env::predecessor_account_id();
        self.eval_whitelist.contains(&caller)
    }

    /// Add an account to the eval whitelist (owner-only).
    pub fn add_to_eval_whitelist(&mut self, account: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can manage eval whitelist"
        );
        self.eval_whitelist.insert(account);
    }

    /// Remove an account from the eval whitelist (owner-only).
    pub fn remove_from_eval_whitelist(&mut self, account: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can manage eval whitelist"
        );
        self.eval_whitelist.remove(&account);
    }

    /// View: list all accounts in the eval whitelist
    pub fn get_eval_whitelist(&self) -> Vec<AccountId> {
        self.eval_whitelist.iter().cloned().collect()
    }

    // --- Existing synchronous eval API ---

    pub fn eval(&self, code: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        let mut env = Vec::new();
        env.push((
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        ));
        run_program(&code, &mut env, self.eval_gas_limit)
            .unwrap_or_else(|e| format!("ERROR: {}", e))
    }

    pub fn eval_with_input(&self, code: String, input_json: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        let mut env = Vec::new();
        // Push user-supplied vars first so they cannot shadow the prefix
        if let Ok(map) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input_json)
        {
            for (k, v) in map {
                env.push((k, json_to_lisp(v)));
            }
        }
        // Push __storage_prefix__ AFTER input vars so it takes precedence and
        // cannot be overwritten by an attacker-controlled input_json.
        env.push((
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        ));
        run_program(&code, &mut env, self.eval_gas_limit)
            .unwrap_or_else(|e| format!("ERROR: {}", e))
    }

    pub fn check_policy(&self, policy: String, input_json: String) -> bool {
        self.eval_with_input(policy, input_json) == "true"
    }

    pub fn save_policy(&mut self, name: String, policy: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can save policies"
        );
        env::storage_write(format!("policy:{}", name).as_bytes(), policy.as_bytes());
        self.policy_names.insert(name);
    }

    pub fn eval_policy(&self, name: String, input_json: String) -> String {
        match env::storage_read(format!("policy:{}", name).as_bytes()) {
            Some(bytes) => {
                self.eval_with_input(String::from_utf8_lossy(&bytes).to_string(), input_json)
            }
            None => format!("ERROR: policy '{}' not found", name),
        }
    }

    /// View: get a stored policy by name
    pub fn get_policy(&self, name: String) -> Option<String> {
        env::storage_read(format!("policy:{}", name).as_bytes())
            .map(|b| String::from_utf8_lossy(&b).to_string())
    }

    /// View: list all stored policy names
    pub fn list_policies(&self) -> Vec<String> {
        self.policy_names.iter().cloned().collect()
    }

    pub fn set_gas_limit(&mut self, limit: u64) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can set gas limit"
        );
        self.eval_gas_limit = limit;
    }
    pub fn get_gas_limit(&self) -> u64 {
        self.eval_gas_limit
    }

    // -----------------------------------------------------------------------
    // Script storage (multi-ccall programs)
    // -----------------------------------------------------------------------

    /// Store a named script (owner-only). Scripts can contain near/ccall
    /// and are evaluated via eval_script / eval_script_async.
    pub fn save_script(&mut self, name: String, code: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can save scripts"
        );
        // Validate: script must parse
        match parse_all(&code) {
            Ok(_) => {}
            Err(e) => panic!("Script parse error: {}", e),
        }
        env::storage_write(format!("script:{}", name).as_bytes(), code.as_bytes());
        self.script_names.insert(name);
    }

    /// View: get a stored script by name
    pub fn get_script(&self, name: String) -> Option<String> {
        env::storage_read(format!("script:{}", name).as_bytes())
            .map(|b| String::from_utf8_lossy(&b).to_string())
    }

    /// View: list all stored script names
    pub fn list_scripts(&self) -> Vec<String> {
        self.script_names.iter().cloned().collect()
    }

    /// Delete a stored script (owner-only)
    pub fn remove_script(&mut self, name: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can remove scripts"
        );
        env::storage_remove(format!("script:{}", name).as_bytes());
        self.script_names.remove(&name);
    }

    /// Delete a stored policy (owner-only)
    pub fn remove_policy(&mut self, name: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can remove policies"
        );
        env::storage_remove(format!("policy:{}", name).as_bytes());
        self.policy_names.remove(&name);
    }

    /// Evaluate a stored script synchronously (no ccall support)
    pub fn eval_script(&self, name: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        match env::storage_read(format!("script:{}", name).as_bytes()) {
            Some(bytes) => {
                let code = String::from_utf8_lossy(&bytes).to_string();
                self.eval(code)
            }
            None => format!("ERROR: script '{}' not found", name),
        }
    }

    /// Evaluate a stored script with input data (synchronous, no ccall)
    pub fn eval_script_with_input(&self, name: String, input_json: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        match env::storage_read(format!("script:{}", name).as_bytes()) {
            Some(bytes) => {
                let code = String::from_utf8_lossy(&bytes).to_string();
                self.eval_with_input(code, input_json)
            }
            None => format!("ERROR: script '{}' not found", name),
        }
    }

    /// Evaluate a stored script asynchronously (with ccall support)
    pub fn eval_script_async(&mut self, name: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        match env::storage_read(format!("script:{}", name).as_bytes()) {
            Some(bytes) => {
                let code = String::from_utf8_lossy(&bytes).to_string();
                self.eval_async(code)
            }
            None => format!("ERROR: script '{}' not found", name),
        }
    }

    // -----------------------------------------------------------------------
    // Ownership
    // -----------------------------------------------------------------------

    /// View: get the current owner
    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }

    /// Transfer ownership to a new account (owner-only)
    pub fn transfer_ownership(&mut self, new_owner: AccountId) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can transfer ownership"
        );
        env::log_str(&format!(
            "Ownership transferred from {} to {}",
            self.owner, new_owner
        ));
        self.owner = new_owner;
    }

    // -----------------------------------------------------------------------
    // Async eval with yield/resume + cross-contract calls
    // -----------------------------------------------------------------------

    /// Evaluate code with async cross-contract call support.
    ///
    /// If the code contains `(near/ccall "account" "method" "args")` at the
    /// top level, creates a cross-contract promise, yields execution, and
    /// saves the full VM state. The result is delivered via `resume_eval`
    /// callback which fires automatically when the cross-contract call completes.
    ///
    /// Lisp usage:
    ///   (define price (near/ccall "ref.near" "get_price" "{}"))
    ///   (+ (to-num price) 10)
    ///
    /// Or standalone:
    ///   (near/ccall "ref.near" "get_price" "{}")
    ///   (near/ccall-result)  ;; returns the result on resume
    pub fn eval_async(&mut self, code: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        let mut eval_env = Vec::new();
        eval_env.push((
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        ));
        match run_program_with_ccall(&code, &mut eval_env, self.eval_gas_limit) {
            Ok(RunResult::Done(result)) => result,
            Ok(RunResult::Yield(yi)) => Self::setup_yield_chain(yi),
            Err(e) => format!("ERROR: {}", e),
        }
    }

    /// Yield callback — resumes evaluation after the cross-contract call completes.
    ///
    /// Called automatically by NEAR's yield/resume mechanism when
    /// `promise_yield_resume` is invoked by `auto_resume_ccall`.
    ///
    /// Flow:
    ///   1. auto_resume_ccall calls promise_yield_resume(data_id, result)
    ///   2. NEAR delivers the result to this deferred receipt
    ///   3. This method deserializes VmState, injects the result, continues eval
    ///
    /// Re-yielding: if the remaining expressions contain another ccall, this
    /// method will yield AGAIN — creating a new yield+ccall chain. This enables
    /// multiple cross-contract calls in sequence.
    pub fn resume_eval(&mut self, yield_id: String) -> String {
        // Guard: must be called as yield callback (has promise results)
        assert!(
            env::promise_results_count() > 0,
            "resume_eval: must be called as yield callback"
        );

        // Restore VM state from storage
        let state_bytes = env::storage_read(yield_id.as_bytes())
            .unwrap_or_else(|| panic!("VM state not found: {}", yield_id));
        let state: VmState =
            borsh::from_slice(&state_bytes).unwrap_or_else(|e| panic!("Corrupt VM state: {}", e));

        // Read cross-contract result from yield_resume payload
        let ccall_result = match env::promise_result(0) {
            PromiseResult::Successful(data) => String::from_utf8_lossy(&data).to_string(),
            PromiseResult::Failed => {
                env::storage_remove(yield_id.as_bytes());
                return "ERROR: ccall failed".to_string();
            }
        };

        // Inject result into environment
        let mut eval_env = state.env;
        if let Some(var) = &state.pending_var.clone() {
            // (define var (near/ccall ...)) → inject result as the variable
            eval_env.push((var.clone(), LispVal::Str(ccall_result.clone())));
        } else {
            // standalone (near/ccall ...) → inject as __ccall_result__
            eval_env.push((
                "__ccall_result__".to_string(),
                LispVal::Str(ccall_result.clone()),
            ));
        }

        // Append result to accumulated __ccall_results__ list (for near/batch-result)
        {
            let results_entry = eval_env
                .iter_mut()
                .rev()
                .find(|(k, _)| k == "__ccall_results__");
            match results_entry {
                Some((_, LispVal::List(ref mut vals))) => {
                    vals.push(LispVal::Str(ccall_result));
                }
                _ => {
                    // First result — create the list
                    eval_env.push((
                        "__ccall_results__".to_string(),
                        LispVal::List(vec![LispVal::Str(ccall_result)]),
                    ));
                }
            }
        }

        // Cleanup stored state (will be re-saved if we yield again)
        env::storage_remove(yield_id.as_bytes());

        // Continue evaluating remaining expressions using ccall-aware runner
        let mut gas = state.gas;
        match run_remaining_with_ccall(&state.remaining, &mut eval_env, &mut gas) {
            Ok(RunResult::Done(result)) => result,
            Ok(RunResult::Yield(yi)) => {
                // Re-yield: another ccall found in remaining expressions.
                // Set up a new yield+ccall chain.
                Self::setup_yield_chain(yi)
            }
            Err(e) => format!("ERROR: {}", e),
        }
    }

    /// Auto-resume callback — called when the cross-contract promise completes.
    ///
    /// Reads the cross-contract result from `promise_result(0)` and passes it
    /// to `promise_yield_resume` to wake up the deferred `resume_eval` receipt.
    pub fn auto_resume_ccall(&mut self, data_id_hex: String) {
        // Guard: must be called as cross-contract callback
        assert!(
            env::promise_results_count() > 0,
            "auto_resume_ccall: must be called as callback"
        );

        let data_id_bytes = hex_decode(&data_id_hex);
        let data_id: CryptoHash = data_id_bytes.try_into().expect("data_id must be 32 bytes");

        // Read the cross-contract call result
        let result = match env::promise_result(0) {
            PromiseResult::Successful(data) => data,
            PromiseResult::Failed => vec![],
        };

        // Resume the yield, passing the result as payload
        env::promise_yield_resume(&data_id, &result);
    }

    // -----------------------------------------------------------------------
    // Shared helpers for yield chain setup (used by eval_async & resume_eval)
    // -----------------------------------------------------------------------

    /// Set up a yield + cross-contract call + auto-resume callback chain.
    /// Used by both `eval_async` (first yield) and `resume_eval` (re-yield).
    fn setup_yield_chain(yi: CcallYield) -> String {
        // Save VM state to contract storage
        let yield_id = format!("vm:{}:{}", env::block_height(), env::block_timestamp());
        let state_bytes = borsh::to_vec(&yi.state).expect("VmState serialization failed");
        env::storage_write(yield_id.as_bytes(), &state_bytes);

        // Step 1: Create yield — stores data_id in register 0
        env::promise_yield_create(
            "resume_eval",
            yield_id.as_bytes(),
            Gas::from_tgas(200),
            GasWeight(0),
            0,
        );

        // Read data_id from register
        let data_id = env::read_register(0).expect("promise_yield_create should write data_id");
        let data_id_hex = hex_encode(&data_id);

        // Step 2: Create cross-contract call promise
        let account_id: near_sdk::AccountId = yi
            .account
            .parse()
            .expect("invalid account id in near/ccall");

        // Step 3: Chain callback
        let auto_args = serde_json::json!({
            "data_id_hex": data_id_hex
        })
        .to_string();

        let cc_promise = Promise::new(account_id).function_call(
            yi.method,
            yi.args_bytes,
            NearToken::from_yoctonear(yi.deposit),
            Gas::from_tgas(yi.gas_tgas),
        );

        let _ = cc_promise.then(Promise::new(env::current_account_id()).function_call(
            "auto_resume_ccall".to_string(),
            auto_args.into_bytes(),
            NearToken::from_yoctonear(0),
            Gas::from_tgas(50),
        ));

        "YIELDING".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_basic() {
        assert_eq!(tokenize("(+ 1 2)"), vec!["(", "+", "1", "2", ")"]);
    }

    #[test]
    fn test_tokenize_line_comment() {
        // ;; comment after code
        assert_eq!(
            tokenize("(+ 1 2) ;; this is a comment\n(* 3 4)"),
            vec!["(", "+", "1", "2", ")", "(", "*", "3", "4", ")"]
        );
    }

    #[test]
    fn test_tokenize_line_comment_only() {
        // entire line is a comment
        assert_eq!(
            tokenize(";; just a comment\n(+ 1 2)"),
            vec!["(", "+", "1", "2", ")"]
        );
    }

    #[test]
    fn test_tokenize_line_comment_at_eof() {
        // comment with no trailing newline
        assert_eq!(tokenize("(+ 1 2);;comment"), vec!["(", "+", "1", "2", ")"]);
    }

    #[test]
    fn test_tokenize_block_comment() {
        assert_eq!(
            tokenize("(+ 1 (; hidden ;) 2)"),
            vec!["(", "+", "1", "2", ")"]
        );
    }

    #[test]
    fn test_tokenize_block_comment_multiline() {
        assert_eq!(
            tokenize("(+ (; block\ncomment\n;) 1)"),
            vec!["(", "+", "1", ")"]
        );
    }

    #[test]
    fn test_tokenize_block_comment_standalone() {
        assert_eq!(
            tokenize("(; entire block comment ;) (+ 1 2)"),
            vec!["(", "+", "1", "2", ")"]
        );
    }

    #[test]
    fn test_tokenize_string_with_semicolons() {
        // semicolons inside strings must NOT be treated as comments
        assert_eq!(
            tokenize("(+ \"hello;;world\")"),
            vec!["(", "+", "\"hello;;world\"", ")"]
        );
    }

    #[test]
    fn test_tokenize_preserves_original_behavior() {
        assert_eq!(
            tokenize("(define (fact n) (if (<= n 1) 1 (* n (fact (- n 1)))))"),
            vec![
                "(", "define", "(", "fact", "n", ")", "(", "if", "(", "<=", "n", "1", ")", "1",
                "(", "*", "n", "(", "fact", "(", "-", "n", "1", ")", ")", ")", ")", ")"
            ]
        );
    }

    #[test]
    fn test_batch_result_no_results() {
        // near/batch-result should error when no __ccall_results__ in env
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 1000u64;
        let expr = LispVal::List(vec![LispVal::Sym("near/batch-result".into())]);
        let result = lisp_eval(&expr, &mut env, &mut gas);
        assert!(result.is_err());
    }

    #[test]
    fn test_ccall_count_no_results() {
        // near/ccall-count should return 0 when no __ccall_results__ in env
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 1000u64;
        let expr = LispVal::List(vec![LispVal::Sym("near/ccall-count".into())]);
        let result = lisp_eval(&expr, &mut env, &mut gas);
        assert_eq!(result.unwrap(), LispVal::Num(0));
    }

    #[test]
    fn test_batch_result_with_results() {
        // near/batch-result should return the accumulated list
        let mut env: Vec<(String, LispVal)> = vec![(
            "__ccall_results__".to_string(),
            LispVal::List(vec![
                LispVal::Str("result1".into()),
                LispVal::Str("result2".into()),
            ]),
        )];
        let mut gas = 1000u64;
        let expr = LispVal::List(vec![LispVal::Sym("near/batch-result".into())]);
        let result = lisp_eval(&expr, &mut env, &mut gas);
        assert_eq!(
            result.unwrap(),
            LispVal::List(vec![
                LispVal::Str("result1".into()),
                LispVal::Str("result2".into()),
            ])
        );
    }

    #[test]
    fn test_ccall_count_with_results() {
        // near/ccall-count should return the count of accumulated results
        let mut env: Vec<(String, LispVal)> = vec![(
            "__ccall_results__".to_string(),
            LispVal::List(vec![
                LispVal::Str("result1".into()),
                LispVal::Str("result2".into()),
                LispVal::Str("result3".into()),
            ]),
        )];
        let mut gas = 1000u64;
        let expr = LispVal::List(vec![LispVal::Sym("near/ccall-count".into())]);
        let result = lisp_eval(&expr, &mut env, &mut gas);
        assert_eq!(result.unwrap(), LispVal::Num(3));
    }

    // -----------------------------------------------------------------------
    // Tests for (require "module") and standard library modules
    // -----------------------------------------------------------------------

    #[test]
    fn test_require_unknown_module() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 1000u64;
        let expr = parse_all("(require \"nonexistent\")").unwrap();
        let result = lisp_eval(&expr[0], &mut env, &mut gas);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("unknown module"));
    }

    #[test]
    fn test_require_non_string() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 1000u64;
        let expr = parse_all("(require 42)").unwrap();
        let result = lisp_eval(&expr[0], &mut env, &mut gas);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("need string module name"));
    }

    #[test]
    fn test_require_math_module() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 50000u64;
        // Load the math module
        let req = parse_all("(require \"math\")").unwrap();
        for e in &req {
            lisp_eval(e, &mut env, &mut gas).unwrap();
        }

        // abs
        let r = lisp_eval(&parse_all("(abs -5)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(5));

        // abs positive
        let r = lisp_eval(&parse_all("(abs 5)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(5));

        // min
        let r = lisp_eval(&parse_all("(min 3 7)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(3));

        // max
        let r = lisp_eval(&parse_all("(max 3 7)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(7));

        // pow
        let r = lisp_eval(&parse_all("(pow 2 10)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(1024));

        // pow base case
        let r = lisp_eval(&parse_all("(pow 5 0)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(1));

        // gcd
        let r = lisp_eval(&parse_all("(gcd 12 8)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(4));

        // lcm
        let r = lisp_eval(&parse_all("(lcm 4 6)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(12));

        // even?
        let r = lisp_eval(&parse_all("(even? 4)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Bool(true));
        let r = lisp_eval(&parse_all("(even? 3)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Bool(false));

        // odd?
        let r = lisp_eval(&parse_all("(odd? 3)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Bool(true));
        let r = lisp_eval(&parse_all("(odd? 4)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Bool(false));

        // sqrt
        let r = lisp_eval(&parse_all("(sqrt 25)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(5));
        let r = lisp_eval(&parse_all("(sqrt 16)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(4));
        let r = lisp_eval(&parse_all("(sqrt 0)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(0));
        let r = lisp_eval(&parse_all("(sqrt 1)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(r, LispVal::Num(1));
    }

    #[test]
    fn test_require_list_module() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 80000u64;
        let req = parse_all("(require \"list\")").unwrap();
        for e in &req {
            lisp_eval(e, &mut env, &mut gas).unwrap();
        }

        // map
        let r = lisp_eval(
            &parse_all("(map (lambda (x) (* x x)) (list 1 2 3))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(
            r,
            LispVal::List(vec![LispVal::Num(1), LispVal::Num(4), LispVal::Num(9)])
        );

        // filter
        let r = lisp_eval(
            &parse_all("(filter (lambda (x) (> x 2)) (list 1 2 3 4))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::List(vec![LispVal::Num(3), LispVal::Num(4)]));

        // reduce
        let r = lisp_eval(
            &parse_all("(reduce (lambda (a b) (+ a b)) 0 (list 1 2 3 4))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Num(10));

        // find
        let r = lisp_eval(
            &parse_all("(find (lambda (x) (> x 3)) (list 1 2 4 3))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Num(4));

        // some
        let r = lisp_eval(
            &parse_all("(some (lambda (x) (= x 3)) (list 1 2 3 4))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Bool(true));
        let r = lisp_eval(
            &parse_all("(some (lambda (x) (= x 5)) (list 1 2 3))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Bool(false));

        // every
        let r = lisp_eval(
            &parse_all("(every (lambda (x) (> x 0)) (list 1 2 3))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Bool(true));
        let r = lisp_eval(
            &parse_all("(every (lambda (x) (> x 2)) (list 1 2 3))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Bool(false));

        // reverse
        let r = lisp_eval(
            &parse_all("(reverse (list 1 2 3))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(
            r,
            LispVal::List(vec![LispVal::Num(3), LispVal::Num(2), LispVal::Num(1)])
        );

        // range
        let r = lisp_eval(&parse_all("(range 0 5)").unwrap()[0], &mut env, &mut gas).unwrap();
        assert_eq!(
            r,
            LispVal::List(vec![
                LispVal::Num(0),
                LispVal::Num(1),
                LispVal::Num(2),
                LispVal::Num(3),
                LispVal::Num(4),
            ])
        );

        // zip
        let r = lisp_eval(
            &parse_all("(zip (list 1 2 3) (list 4 5 6))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(
            r,
            LispVal::List(vec![
                LispVal::List(vec![LispVal::Num(1), LispVal::Num(4)]),
                LispVal::List(vec![LispVal::Num(2), LispVal::Num(5)]),
                LispVal::List(vec![LispVal::Num(3), LispVal::Num(6)]),
            ])
        );

        // sort
        let r = lisp_eval(
            &parse_all("(sort (list 3 1 4 1 5 9 2 6))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(
            r,
            LispVal::List(vec![
                LispVal::Num(1),
                LispVal::Num(1),
                LispVal::Num(2),
                LispVal::Num(3),
                LispVal::Num(4),
                LispVal::Num(5),
                LispVal::Num(6),
                LispVal::Num(9),
            ])
        );
    }

    #[test]
    fn test_require_string_module() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 80000u64;
        let req = parse_all("(require \"string\")").unwrap();
        for e in &req {
            lisp_eval(e, &mut env, &mut gas).unwrap();
        }

        // str-join
        let r = lisp_eval(
            &parse_all("(str-join \", \" (list \"a\" \"b\" \"c\"))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("a, b, c".to_string()));

        // str-join single element
        let r = lisp_eval(
            &parse_all("(str-join \", \" (list \"only\"))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("only".to_string()));

        // str-join empty list
        let r = lisp_eval(
            &parse_all("(str-join \", \" (list))").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("".to_string()));

        // str-repeat
        let r = lisp_eval(
            &parse_all("(str-repeat \"ab\" 3)").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("ababab".to_string()));

        // str-repeat zero
        let r = lisp_eval(
            &parse_all("(str-repeat \"ab\" 0)").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("".to_string()));

        // str-replace
        let r = lisp_eval(
            &parse_all("(str-replace \"hello world\" \"world\" \"there\")").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("hello there".to_string()));

        // str-pad-left
        let r = lisp_eval(
            &parse_all("(str-pad-left \"42\" 5 \"0\")").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("00042".to_string()));

        // str-pad-right
        let r = lisp_eval(
            &parse_all("(str-pad-right \"hi\" 6 \".\")").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        assert_eq!(r, LispVal::Str("hi....".to_string()));
    }

    #[test]
    fn test_require_crypto_module() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let mut gas = 50000u64;
        let req = parse_all("(require \"crypto\")").unwrap();
        for e in &req {
            lisp_eval(e, &mut env, &mut gas).unwrap();
        }

        // hash/sha256-bytes should wrap sha256
        let r = lisp_eval(
            &parse_all("(hash/sha256-bytes \"hello\")").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        // Should be a string (hex output)
        match r {
            LispVal::Str(s) => assert_eq!(s.len(), 64), // SHA256 is 32 bytes = 64 hex chars
            other => panic!("expected string, got {:?}", other),
        }

        // hash/keccak256-bytes should wrap keccak256
        let r = lisp_eval(
            &parse_all("(hash/keccak256-bytes \"hello\")").unwrap()[0],
            &mut env,
            &mut gas,
        )
        .unwrap();
        match r {
            LispVal::Str(s) => assert_eq!(s.len(), 64),
            other => panic!("expected string, got {:?}", other),
        }
    }

    #[test]
    fn test_run_program_with_require() {
        // Test that require works through the run_program interface
        let mut env: Vec<(String, LispVal)> = vec![];
        let result = run_program(
            "(require \"math\") (define x (abs -42)) x",
            &mut env,
            100000,
        )
        .unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_require_multiple_modules() {
        // Require both math and list, use functions from both
        let mut env: Vec<(String, LispVal)> = vec![];
        let result = run_program(
            "(require \"math\") (require \"list\") (+ (abs -1) (abs -2))",
            &mut env,
            100000,
        );
        assert_eq!(result.unwrap(), "3");
    }

    #[test]
    fn test_require_map_abs() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let result = run_program(
            "(require \"math\") (require \"list\") (map abs (list -1 -2 -3))",
            &mut env,
            100000,
        );
        match result {
            Ok(v) => assert_eq!(v, "(1 2 3)"),
            Err(e) => panic!("map abs failed: {}", e),
        }
    }

    #[test]
    fn test_require_reduce_sum() {
        let mut env: Vec<(String, LispVal)> = vec![];
        let result = run_program(
            "(require \"list\") (reduce (lambda (a b) (+ a b)) 0 (list 1 2 3))",
            &mut env,
            100000,
        );
        assert_eq!(result.unwrap(), "6");
    }
}
