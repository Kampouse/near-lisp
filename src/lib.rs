use near_sdk::{env, near};

// ---------------------------------------------------------------------------
// Lisp Value
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub enum LispVal {
    Nil,
    Bool(bool),
    Num(i64),
    Str(String),
    Sym(String),
    List(Vec<LispVal>),
    Lambda {
        params: Vec<String>,
        body: Box<LispVal>,
        closed_env: Vec<(String, LispVal)>,
    },
}

impl std::fmt::Display for LispVal {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            LispVal::Nil => write!(f, "nil"),
            LispVal::Bool(b) => write!(f, "{}", if *b { "true" } else { "false" }),
            LispVal::Num(n) => write!(f, "{}", n),
            LispVal::Str(s) => write!(f, "\"{}\"", s),
            LispVal::Sym(s) => write!(f, "{}", s),
            LispVal::List(vals) => {
                let parts: Vec<String> = vals.iter().map(|v| v.to_string()).collect();
                write!(f, "({})", parts.join(" "))
            }
            LispVal::Lambda { params, .. } => {
                write!(f, "#<lambda ({})>", params.join(" "))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tokenizer + Parser
// ---------------------------------------------------------------------------

fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    for ch in input.chars() {
        if in_str {
            cur.push(ch);
            if ch == '"' {
                tokens.push(cur.clone());
                cur.clear();
                in_str = false;
            }
        } else if ch == '"' {
            in_str = true;
            cur.push(ch);
        } else if ch == '(' || ch == ')' {
            if !cur.is_empty() { tokens.push(cur.clone()); cur.clear(); }
            tokens.push(ch.to_string());
        } else if ch.is_whitespace() {
            if !cur.is_empty() { tokens.push(cur.clone()); cur.clear(); }
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() { tokens.push(cur); }
    tokens
}

fn parse(tokens: &[String], pos: &mut usize) -> Result<LispVal, String> {
    if *pos >= tokens.len() { return Err("unexpected EOF".into()); }
    let tok = &tokens[*pos];
    *pos += 1;
    match tok.as_str() {
        "(" => {
            let mut list = Vec::new();
            while *pos < tokens.len() && tokens[*pos] != ")" {
                list.push(parse(tokens, pos)?);
            }
            if *pos >= tokens.len() { return Err("missing )".into()); }
            *pos += 1;
            Ok(LispVal::List(list))
        }
        ")" => Err("unexpected )".into()),
        "nil" => Ok(LispVal::Nil),
        "true" => Ok(LispVal::Bool(true)),
        "false" => Ok(LispVal::Bool(false)),
        s if s.starts_with('"') => Ok(LispVal::Str(s[1..s.len()-1].to_string())),
        s => s.parse::<i64>().map(LispVal::Num).or_else(|_| Ok(LispVal::Sym(s.to_string()))),
    }
}

fn parse_all(input: &str) -> Result<Vec<LispVal>, String> {
    let tokens = tokenize(input);
    let mut pos = 0;
    let mut exprs = Vec::new();
    while pos < tokens.len() { exprs.push(parse(&tokens, &mut pos)?); }
    Ok(exprs)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_truthy(v: &LispVal) -> bool { !matches!(v, LispVal::Nil | LispVal::Bool(false)) }

fn as_num(v: &LispVal) -> Result<i64, String> {
    match v { LispVal::Num(n) => Ok(*n), _ => Err(format!("expected number, got {}", v)) }
}

fn as_str(v: &LispVal) -> Result<String, String> {
    match v {
        LispVal::Str(s) => Ok(s.clone()),
        LispVal::Sym(s) => Ok(s.clone()),
        LispVal::Num(n) => Ok(n.to_string()),
        _ => Err(format!("expected string, got {}", v)),
    }
}

fn do_arith(args: &[LispVal], op: fn(i64, i64) -> i64) -> Result<LispVal, String> {
    if args.len() < 2 { return Err("arith needs 2+ args".into()); }
    let init = as_num(&args[0])?;
    let res: Result<i64, String> = args[1..].iter().try_fold(init, |a, b| Ok(op(a, as_num(b)?)));
    Ok(LispVal::Num(res?))
}

fn parse_params(val: &LispVal) -> Result<Vec<String>, String> {
    match val {
        LispVal::List(p) => p.iter().map(|v| match v {
            LispVal::Sym(s) => Ok(s.clone()), _ => Err("param must be sym".into())
        }).collect(),
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
    closed_env: &[(String, LispVal)],
    args: &[LispVal],
    caller_env: &mut Vec<(String, LispVal)>,
    gas: &mut u64,
) -> Result<LispVal, String> {
    // Merge: closed_env (lexical scope) + caller_env (has define bindings) + args
    let mut local = closed_env.to_vec();
    local.extend(caller_env.iter().cloned());
    for (i, p) in params.iter().enumerate() {
        local.push((p.clone(), args.get(i).cloned().unwrap_or(LispVal::Nil)));
    }
    lisp_eval(body, &mut local, gas)
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

pub fn lisp_eval(expr: &LispVal, env: &mut Vec<(String, LispVal)>, gas: &mut u64) -> Result<LispVal, String> {
    if *gas == 0 { return Err("out of gas".into()); }
    *gas -= 1;

    match expr {
        LispVal::Nil | LispVal::Bool(_) | LispVal::Num(_) | LispVal::Str(_) | LispVal::Lambda { .. } => Ok(expr.clone()),
        LispVal::Sym(name) => env.iter().rev()
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
                            list.get(3).map(|e| lisp_eval(e, env, gas)).unwrap_or(Ok(LispVal::Nil))
                        }
                    }
                    "cond" => {
                        for clause in &list[1..] {
                            if let LispVal::List(parts) = clause {
                                if parts.is_empty() { continue; }
                                if let LispVal::Sym(kw) = &parts[0] {
                                    if kw == "else" {
                                        return parts.get(1).map(|e| lisp_eval(e, env, gas)).unwrap_or(Ok(LispVal::Nil));
                                    }
                                }
                                let test = lisp_eval(&parts[0], env, gas)?;
                                if is_truthy(&test) {
                                    return parts.get(1).map(|e| lisp_eval(e, env, gas)).unwrap_or(Ok(test));
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
                        list.get(2).map(|e| lisp_eval(e, &mut local, gas)).unwrap_or(Ok(LispVal::Nil))
                    }
                    "lambda" => {
                        let params = parse_params(list.get(1).ok_or("lambda: need params")?)?;
                        let body = list.get(2).ok_or("lambda: need body")?;
                        Ok(LispVal::Lambda { params, body: Box::new(body.clone()), closed_env: env.clone() })
                    }
                    "progn" | "begin" => {
                        let mut r = LispVal::Nil;
                        for e in &list[1..] { r = lisp_eval(e, env, gas)?; }
                        Ok(r)
                    }
                    "and" => {
                        let mut r = LispVal::Bool(true);
                        for e in &list[1..] { r = lisp_eval(e, env, gas)?; if !is_truthy(&r) { return Ok(r); } }
                        Ok(r)
                    }
                    "or" => {
                        for e in &list[1..] { let r = lisp_eval(e, env, gas)?; if is_truthy(&r) { return Ok(r); } }
                        Ok(LispVal::Bool(false))
                    }
                    "not" => {
                        let v = lisp_eval(list.get(1).ok_or("not: need arg")?, env, gas)?;
                        Ok(LispVal::Bool(!is_truthy(&v)))
                    }
                    "near/block-height" => Ok(LispVal::Num(env::block_height() as i64)),
                    "near/predecessor" => Ok(LispVal::Str(env::predecessor_account_id().to_string())),
                    "near/signer" => Ok(LispVal::Str(env::signer_account_id().to_string())),
                    "near/timestamp" => Ok(LispVal::Num(env::block_timestamp() as i64)),
                    "near/log" => {
                        let v = lisp_eval(list.get(1).ok_or("near/log: need arg")?, env, gas)?;
                        env::log_str(&v.to_string());
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

fn dispatch_call(list: &[LispVal], env: &mut Vec<(String, LispVal)>, gas: &mut u64) -> Result<LispVal, String> {
    let head = &list[0];
    let args: Vec<LispVal> = list[1..].iter().map(|a| lisp_eval(a, env, gas)).collect::<Result<_,_>>()?;

    if let LispVal::Sym(name) = head {
        match name.as_str() {
            "+" => do_arith(&args, |a,b| a+b),
            "-" => do_arith(&args, |a,b| a-b),
            "*" => do_arith(&args, |a,b| a*b),
            "/" => {
                let b = as_num(args.get(1).ok_or("/ needs 2 args")?)?;
                if b == 0 { return Err("div by zero".into()); }
                Ok(LispVal::Num(as_num(&args[0])? / b))
            }
            "mod" => do_arith(&args, |a,b| a%b),
            "=" | "==" => Ok(LispVal::Bool(args.get(0) == args.get(1))),
            "!=" | "/=" => Ok(LispVal::Bool(args.get(0) != args.get(1))),
            "<" => Ok(LispVal::Bool(as_num(&args[0])? < as_num(&args[1])?)),
            ">" => Ok(LispVal::Bool(as_num(&args[0])? > as_num(&args[1])?)),
            "<=" => Ok(LispVal::Bool(as_num(&args[0])? <= as_num(&args[1])?)),
            ">=" => Ok(LispVal::Bool(as_num(&args[0])? >= as_num(&args[1])?)),
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
                    if let LispVal::List(l) = a { r.extend(l.iter().cloned()); } else { r.push(a.clone()); }
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
            "str-concat" => Ok(LispVal::Str(args.iter().map(|a| a.to_string()).collect::<Vec<_>>().join(""))),
            "str-contains" => Ok(LispVal::Bool(as_str(&args[0])?.contains(&as_str(&args[1])?))),
            "to-string" => Ok(LispVal::Str(args[0].to_string())),
            "nil?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Nil))),
            "list?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::List(_)))),
            "number?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Num(_)))),
            "string?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Str(_)))),
            "near/storage-write" => {
                env::storage_write(as_str(&args[0])?.as_bytes(), as_str(&args[1])?.as_bytes());
                Ok(LispVal::Bool(true))
            }
            "near/storage-read" => Ok(env::storage_read(as_str(&args[0])?.as_bytes())
                .map(|v| LispVal::Str(String::from_utf8_lossy(&v).to_string()))
                .unwrap_or(LispVal::Nil)),
            _ => {
                // Lambda lookup
                let func = env.iter().rev()
                    .find(|(k, _)| k == name)
                    .map(|(_, v)| v.clone())
                    .ok_or_else(|| format!("undefined: {}", name))?;
                call_val(&func, &args, env, gas)
            }
        }
    } else if let LispVal::Lambda { params, body, closed_env } = head {
        apply_lambda(params, body, closed_env, &args, env, gas)
    } else if let LispVal::List(ll) = head {
        // Inline lambda: ((lambda (x) (* x x)) 5)
        if ll.len() < 3 { return Err("inline lambda too short".into()); }
        let params = parse_params(&ll[1])?;
        apply_lambda(&params, &ll[2], &[], &args, env, gas)
    } else {
        Err("not callable".into())
    }
}

fn call_val(func: &LispVal, args: &[LispVal], env: &mut Vec<(String, LispVal)>, gas: &mut u64) -> Result<LispVal, String> {
    match func {
        LispVal::Lambda { params, body, closed_env } => apply_lambda(params, body, closed_env, args, env, gas),
        LispVal::List(ll) if ll.len() >= 3 => {
            let params = parse_params(&ll[1])?;
            apply_lambda(&params, &ll[2], &[], args, env, gas)
        }
        _ => Err(format!("not callable: {}", func)),
    }
}

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

pub fn run_program(code: &str, env: &mut Vec<(String, LispVal)>, gas_limit: u64) -> Result<String, String> {
    let exprs = parse_all(code)?;
    let mut gas = gas_limit;
    let mut result = LispVal::Nil;
    for expr in exprs { result = lisp_eval(&expr, env, &mut gas)?; }
    Ok(result.to_string())
}

pub fn json_to_lisp(val: serde_json::Value) -> LispVal {
    match val {
        serde_json::Value::Null => LispVal::Nil,
        serde_json::Value::Bool(b) => LispVal::Bool(b),
        serde_json::Value::Number(n) => LispVal::Num(n.as_i64().unwrap_or(0)),
        serde_json::Value::String(s) => LispVal::Str(s),
        serde_json::Value::Array(a) => LispVal::List(a.into_iter().map(json_to_lisp).collect()),
        serde_json::Value::Object(m) => LispVal::List(
            m.into_iter().map(|(k,v)| LispVal::List(vec![LispVal::Str(k), json_to_lisp(v)])).collect()
        ),
    }
}

// ---------------------------------------------------------------------------
// NEAR Contract
// ---------------------------------------------------------------------------

#[near(contract_state)]
pub struct LispContract { eval_gas_limit: u64 }

impl Default for LispContract {
    fn default() -> Self { Self { eval_gas_limit: 10_000 } }
}

#[near]
impl LispContract {
    #[init]
    pub fn new(eval_gas_limit: u64) -> Self {
        Self { eval_gas_limit: if eval_gas_limit == 0 { 10_000 } else { eval_gas_limit } }
    }

    pub fn eval(&self, code: String) -> String {
        let mut env = Vec::new();
        run_program(&code, &mut env, self.eval_gas_limit).unwrap_or_else(|e| format!("ERROR: {}", e))
    }

    pub fn eval_with_input(&self, code: String, input_json: String) -> String {
        let mut env = Vec::new();
        if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input_json) {
            for (k, v) in map { env.push((k, json_to_lisp(v))); }
        }
        run_program(&code, &mut env, self.eval_gas_limit).unwrap_or_else(|e| format!("ERROR: {}", e))
    }

    pub fn check_policy(&self, policy: String, input_json: String) -> bool {
        self.eval_with_input(policy, input_json) == "true"
    }

    pub fn save_policy(&mut self, name: String, policy: String) {
        env::storage_write(format!("policy:{}", name).as_bytes(), policy.as_bytes());
    }

    pub fn eval_policy(&self, name: String, input_json: String) -> String {
        match env::storage_read(format!("policy:{}", name).as_bytes()) {
            Some(bytes) => self.eval_with_input(String::from_utf8_lossy(&bytes).to_string(), input_json),
            None => format!("ERROR: policy '{}' not found", name),
        }
    }

    pub fn set_gas_limit(&mut self, limit: u64) { self.eval_gas_limit = limit; }
    pub fn get_gas_limit(&self) -> u64 { self.eval_gas_limit }
}
