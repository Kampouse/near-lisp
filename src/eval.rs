use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::store::IterableSet;
use near_sdk::{
    env, near, AccountId, CryptoHash, Gas, GasWeight, NearToken, Promise, PromiseResult,
};
use std::collections::{BTreeMap, HashMap};


use crate::types::{LispVal, Env, check_gas, get_stdlib_code};
use crate::helpers::*;
use crate::bytecode::{try_compile_loop, exec_compiled_loop};
use crate::parser::parse_all;
use crate::vm::{json_to_lisp, lisp_to_json, hex_encode, hex_decode};

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

pub fn lisp_eval(
    expr: &LispVal,
    env: &mut Env,
    gas: &mut u64,
) -> Result<LispVal, String> {
    // Trampoline loop for TCO — tail positions rebind current_expr + continue.
    // Non-tail evaluations (args, conditions) still call lisp_eval recursively.
    let mut current_expr: LispVal = expr.clone();
    '_trampoline: loop {
        check_gas(gas)?;

        match &current_expr {
            LispVal::Nil
            | LispVal::Bool(_)
            | LispVal::Num(_)
            | LispVal::Float(_)
            | LispVal::Str(_)
            | LispVal::Lambda { .. }
            | LispVal::Map(_) => return Ok(current_expr.clone()),
            LispVal::Recur(_) => return Err("recur outside loop".into()),
            LispVal::Sym(name) => {
                if let Some(v) = env.get(name) {
                    return Ok(v.clone());
                }
                if is_builtin_name(name) {
                    return Ok(current_expr);
                }
                return Err(format!("undefined: {}", name));
            }
            LispVal::List(list) if list.is_empty() => return Ok(LispVal::Nil),
            LispVal::List(list) => {
                if let LispVal::Sym(name) = &list[0] {
                    match name.as_str() {
                        "quote" => return Ok(list.get(1).cloned().unwrap_or(LispVal::Nil)),
                        "define" => {
                            let var = match list.get(1) {
                                Some(LispVal::Sym(s)) => s.clone(),
                                _ => return Err("define: need symbol".into()),
                            };
                            let val = match list.get(2) {
                                Some(v) => lisp_eval(v, env, gas)?,
                                None => LispVal::Nil,
                            };
                            env.push(var, val);
                            return Ok(LispVal::Nil);
                        }
                        // TCO: if
                        "if" => {
                            let cond = lisp_eval(list.get(1).ok_or("if: need cond")?, env, gas)?;
                            current_expr = if is_truthy(&cond) {
                                list.get(2).ok_or("if: need then")?.clone()
                            } else {
                                list.get(3).cloned().unwrap_or(LispVal::Nil)
                            };
                            continue '_trampoline;
                        }
                        // TCO: cond
                        "cond" => {
                            let mut found: Option<LispVal> = None;
                            for clause in &list[1..] {
                                if let LispVal::List(parts) = clause {
                                    if parts.is_empty() {
                                        continue;
                                    }
                                    if let LispVal::Sym(kw) = &parts[0] {
                                        if kw == "else" {
                                            found = parts.get(1).cloned();
                                            break;
                                        }
                                    }
                                    let test = lisp_eval(&parts[0], env, gas)?;
                                    if is_truthy(&test) {
                                        found = Some(parts.get(1).cloned().unwrap_or(test));
                                        break;
                                    }
                                }
                            }
                            match found {
                                Some(e) => {
                                    current_expr = e;
                                    continue '_trampoline;
                                }
                                None => return Ok(LispVal::Nil),
                            }
                        }
                        // let: env cleanup requires recursive call (no TCO)
                        "let" => {
                            let bindings = match list.get(1) {
                                Some(LispVal::List(b)) => b,
                                _ => return Err("let: bindings must be list".into()),
                            };
                            let base_len = env.len();
                            for b in bindings {
                                if let LispVal::List(pair) = b {
                                    if pair.len() == 2 {
                                        if let LispVal::Sym(name) = &pair[0] {
                                            let val = lisp_eval(&pair[1], env, gas)?;
                                            env.push(name.clone(), val);
                                        }
                                    }
                                }
                            }
                            let result = list
                                .get(2)
                                .map(|e| lisp_eval(e, env, gas))
                                .unwrap_or(Ok(LispVal::Nil));
                            env.truncate(base_len);
                            return result;
                        }
                        "lambda" => {
                            let (params, rest_param) =
                                parse_params(list.get(1).ok_or("lambda: need params")?)?;
                            let body = list.get(2).ok_or("lambda: need body")?;
                            return Ok(LispVal::Lambda {
                                params,
                                rest_param,
                                body: Box::new(body.clone()),
                                closed_env: Box::new(env.clone().into_bindings()),
                            });
                        }
                        // TCO: progn/begin
                        "progn" | "begin" => {
                            let exprs = &list[1..];
                            if exprs.is_empty() {
                                return Ok(LispVal::Nil);
                            }
                            for e in &exprs[..exprs.len() - 1] {
                                lisp_eval(e, env, gas)?;
                            }
                            current_expr = exprs.last().unwrap().clone();
                            continue '_trampoline;
                        }
                        // TCO: and
                        "and" => {
                            if list.len() == 1 {
                                return Ok(LispVal::Bool(true));
                            }
                            let exprs = &list[1..];
                            for e in &exprs[..exprs.len() - 1] {
                                let r = lisp_eval(e, env, gas)?;
                                if !is_truthy(&r) {
                                    return Ok(r);
                                }
                            }
                            current_expr = exprs.last().unwrap().clone();
                            continue '_trampoline;
                        }
                        // TCO: or
                        "or" => {
                            if list.len() == 1 {
                                return Ok(LispVal::Bool(false));
                            }
                            let exprs = &list[1..];
                            for e in &exprs[..exprs.len() - 1] {
                                let r = lisp_eval(e, env, gas)?;
                                if is_truthy(&r) {
                                    return Ok(r);
                                }
                            }
                            current_expr = exprs.last().unwrap().clone();
                            continue '_trampoline;
                        }
                        "not" => {
                            let v = lisp_eval(list.get(1).ok_or("not: need arg")?, env, gas)?;
                            return Ok(LispVal::Bool(!is_truthy(&v)));
                        }
                        // try/catch: env cleanup, recursive call
                        "try" => {
                            let expr_to_try = list.get(1).ok_or("try: need expression")?;
                            let res = match lisp_eval(expr_to_try, env, gas) {
                                Ok(val) => return Ok(val),
                                Err(err_msg) => {
                                    let catch_clause =
                                        list.get(2).ok_or("try: need catch clause")?;
                                    if let LispVal::List(clause) = catch_clause {
                                        if clause.is_empty()
                                            || clause[0] != LispVal::Sym("catch".into())
                                        {
                                            return Err(
                                                "try: second arg must be (catch var body...)".into(),
                                            );
                                        }
                                        let error_var = match clause.get(1) {
                                            Some(LispVal::Sym(s)) => s.clone(),
                                            _ => {
                                                return Err(
                                                    "try: catch needs a variable name".into(),
                                                )
                                            }
                                        };
                                        env.push(error_var, LispVal::Str(err_msg));
                                        let base_len = env.len() - 1;
                                        let mut r = LispVal::Nil;
                                        for body_expr in &clause[2..] {
                                            r = lisp_eval(body_expr, env, gas)?;
                                        }
                                        env.truncate(base_len);
                                        r
                                    } else {
                                        return Err("try: catch clause must be a list".into());
                                    }
                                }
                            };
                            return Ok(res);
                        }
                        // match: env cleanup, recursive call
                        "match" => {
                            let val =
                                lisp_eval(list.get(1).ok_or("match: need expr")?, env, gas)?;
                            let mut matched: Option<(Vec<(String, LispVal)>, LispVal)> = None;
                            for clause in &list[2..] {
                                if let LispVal::List(parts) = clause {
                                    if parts.len() >= 2 {
                                        if let Some(bindings) = match_pattern(&parts[0], &val) {
                                            matched = Some((
                                                bindings,
                                                parts.get(1).cloned().unwrap_or(LispVal::Nil),
                                            ));
                                            break;
                                        }
                                    }
                                }
                            }
                            match matched {
                                Some((bindings, body)) => {
                                    let base_len = env.len();
                                    for (name, v) in bindings {
                                        env.push(name, v);
                                    }
                                    let result = lisp_eval(&body, env, gas);
                                    env.truncate(base_len);
                                    return result;
                                }
                                None => return Ok(LispVal::Nil),
                            }
                        }
                        // loop/recur: try bytecode compilation first, fall back to tree walk
                        "loop" => {
                            let bindings = match list.get(1) {
                                Some(LispVal::List(b)) => b,
                                _ => return Err("loop: bindings must be list".into()),
                            };
                            let body = list.get(2).ok_or("loop: need body")?;
                            let mut binding_names: Vec<String> = Vec::new();
                            let mut binding_vals: Vec<LispVal> = Vec::new();
                            let is_pair_style =
                                bindings.iter().all(|b| matches!(b, LispVal::List(_)));
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
                            // Try bytecode compilation for the loop body
                            if let Some(cl) = try_compile_loop(&binding_names, binding_vals.clone(), body, env) {
                                return exec_compiled_loop(&cl, gas, env);
                            }
                            // Fallback: tree-walk interpreter
                            let result = loop {
                                let base_len = env.len();
                                for (i, name) in binding_names.iter().enumerate() {
                                    env.push(name.clone(), binding_vals[i].clone());
                                }
                                let result = lisp_eval(body, env, gas);
                                env.truncate(base_len);
                                match result? {
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
                                    other => break other,
                                }
                            };
                            return Ok(result);
                        }
                        "recur" => {
                            let vals: Vec<LispVal> = list[1..]
                                .iter()
                                .map(|a| lisp_eval(a, env, gas))
                                .collect::<Result<_, _>>()?;
                            return Ok(LispVal::Recur(vals));
                        }
                        // near/ccall-result
                        "near/ccall-result" => {
                            return env
                                .iter()
                                .rev()
                                .find(|(k, _)| k == "__ccall_result__")
                                .map(|(_, v)| v.clone())
                                .ok_or_else(|| "near/ccall-result: no pending result".into());
                        }
                        // near/batch-result
                        "near/batch-result" => {
                            return env
                                .iter()
                                .rev()
                                .find(|(k, _)| k == "__ccall_results__")
                                .map(|(_, v)| v.clone())
                                .ok_or_else(|| "near/batch-result: no results yet".into());
                        }
                        // near/ccall-count
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
                            return Ok(LispVal::Num(count));
                        }
                        "near/block-height" => {
                            return Ok(LispVal::Num(env::block_height() as i64));
                        }
                        "near/predecessor" => {
                            return Ok(LispVal::Str(env::predecessor_account_id().to_string()));
                        }
                        "near/signer" => {
                            return Ok(LispVal::Str(env::signer_account_id().to_string()));
                        }
                        "near/timestamp" => {
                            return Ok(LispVal::Num(env::block_timestamp() as i64));
                        }
                        "near/account-balance" => {
                            return Ok(LispVal::Str(
                                env::account_balance().as_yoctonear().to_string(),
                            ));
                        }
                        "near/attached-deposit" => {
                            return Ok(LispVal::Str(
                                env::attached_deposit().as_yoctonear().to_string(),
                            ));
                        }
                        "near/account-locked-balance" => {
                            return Ok(LispVal::Str(
                                env::account_locked_balance().as_yoctonear().to_string(),
                            ));
                        }
                        "near/log" => {
                            let v =
                                lisp_eval(list.get(1).ok_or("near/log: need arg")?, env, gas)?;
                            env::log_str(&v.to_string());
                            return Ok(LispVal::Nil);
                        }
                        "require" => {
                            let module_name = match list.get(1) {
                                Some(LispVal::Str(s)) => s.as_str(),
                                _ => return Err("require: need string module name".into()),
                            };
                            let prefix: Option<&str> = match list.get(2) {
                                Some(LispVal::Str(s)) => Some(s.as_str()),
                                None => None,
                                _ => return Err("require: prefix must be string".into()),
                            };
                            let marker =
                                format!("__stdlib_{}__{}", module_name, prefix.unwrap_or(""));
                            if env.contains(&marker) {
                                return Ok(LispVal::Nil);
                            }
                            if let Some(code) = get_stdlib_code(module_name) {
                                if let Some(pfx) = prefix {
                                    let mut module_env = Env::new();
                                    let module_exprs = parse_all(code)?;
                                    for expr in &module_exprs {
                                        lisp_eval(expr, &mut module_env, gas)?;
                                    }
                                    for (k, v) in module_env.into_bindings() {
                                        env.push(format!("{}/{}", pfx, k), v);
                                    }
                                } else {
                                    let module_exprs = parse_all(code)?;
                                    for expr in &module_exprs {
                                        lisp_eval(expr, env, gas)?;
                                    }
                                }
                                env.push(marker, LispVal::Bool(true));
                                return Ok(LispVal::Nil);
                            }
                            let storage_key = format!("module:{}", module_name);
                            if let Some(bytes) = env::storage_read(storage_key.as_bytes()) {
                                let code = String::from_utf8(bytes)
                                    .map_err(|_| "require: module has invalid utf8")?;
                                if let Some(pfx) = prefix {
                                    let mut module_env = Env::new();
                                    let module_exprs = parse_all(&code)?;
                                    for expr in &module_exprs {
                                        lisp_eval(expr, &mut module_env, gas)?;
                                    }
                                    for (k, v) in module_env.into_bindings() {
                                        env.push(format!("{}/{}", pfx, k), v);
                                    }
                                } else {
                                    let module_exprs = parse_all(&code)?;
                                    for expr in &module_exprs {
                                        lisp_eval(expr, env, gas)?;
                                    }
                                }
                                env.push(marker, LispVal::Bool(true));
                                return Ok(LispVal::Nil);
                            }
                            return Err(format!(
                                "require: unknown module '{}'",
                                module_name
                            ));
                        }
                        _ => return dispatch_call(list, env, gas),
                    }
                } else {
                    return dispatch_call(list, env, gas);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Lambda application (here instead of helpers.rs to avoid circular dep)
// ---------------------------------------------------------------------------

pub fn apply_lambda(
    params: &[String],
    rest_param: &Option<String>,
    body: &LispVal,
    closed_env: &Vec<(String, LispVal)>,
    args: &[LispVal],
    caller_env: &mut Env,
    gas: &mut u64,
) -> Result<LispVal, String> {
    let base_len = caller_env.len();
    for (k, v) in closed_env {
        caller_env.push(k.clone(), v.clone());
    }
    for (i, p) in params.iter().enumerate() {
        caller_env.push(p.clone(), args.get(i).cloned().unwrap_or(LispVal::Nil));
    }
    if let Some(rest_name) = rest_param {
        let rest_args: Vec<LispVal> = args.get(params.len()..).unwrap_or(&[]).to_vec();
        caller_env.push(rest_name.clone(), LispVal::List(rest_args));
    }
    let result = lisp_eval(body, caller_env, gas);
    caller_env.truncate(base_len);
    result
}

// ---------------------------------------------------------------------------
// Function dispatch (builtins + lambda calls)
// ---------------------------------------------------------------------------

fn dispatch_call(
    list: &[LispVal],
    env: &mut Env,
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
            "str=" => {
                let a = as_str(args.get(0).ok_or("str=: need 2 args")?)?;
                let b = as_str(args.get(1).ok_or("str=: need 2 args")?)?;
                Ok(LispVal::Bool(a == b))
            }
            "str!=" => {
                let a = as_str(args.get(0).ok_or("str!=: need 2 args")?)?;
                let b = as_str(args.get(1).ok_or("str!=: need 2 args")?)?;
                Ok(LispVal::Bool(a != b))
            }
            "nil?" => Ok(LispVal::Bool(
                matches!(&args[0], LispVal::Nil)
                    || matches!(&args[0], LispVal::List(ref v) if v.is_empty()),
            )),
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
            "to-num" => match &args[0] {
                // Alias for to-int — converts to i64
                LispVal::Num(n) => Ok(LispVal::Num(*n)),
                LispVal::Float(f) => Ok(LispVal::Num(*f as i64)),
                LispVal::Str(s) => s
                    .parse::<i64>()
                    .map(LispVal::Num)
                    .map_err(|_| format!("to-num: cannot parse '{}'", s)),
                other => Err(format!("to-num: expected number, got {}", other)),
            },
            "type?" => Ok(LispVal::Str(
                match &args[0] {
                    LispVal::Nil => "nil",
                    LispVal::Bool(_) => "boolean",
                    LispVal::Num(_) => "number",
                    LispVal::Float(_) => "number",
                    LispVal::Str(_) => "string",
                    LispVal::List(_) => "list",
                    LispVal::Map(_) => "map",
                    LispVal::Lambda { .. } => "lambda",
                    LispVal::Sym(_) => "symbol",
                    _ => "unknown",
                }
                .to_string(),
            )),
            "bool?" => Ok(LispVal::Bool(matches!(&args[0], LispVal::Bool(_)))),
            "error" => {
                let msg = args
                    .get(0)
                    .map(|v| format!("{}", v))
                    .unwrap_or_else(|| "error".to_string());
                Err(msg)
            }
            // --- Debug builtins ---
            "debug" | "near/log-debug" => {
                // Log to NEAR runtime logs, return nil
                let msg = args
                    .get(0)
                    .map(|v| format!("{}", v))
                    .unwrap_or_else(|| "debug".to_string());
                #[cfg(not(test))]
                near_sdk::env::log_str(&format!("[DEBUG] {}", msg));
                Ok(LispVal::Nil)
            }
            "trace" => {
                // Log value to NEAR runtime logs, return the value unchanged (pass-through)
                let val = args.get(0).cloned().unwrap_or(LispVal::Nil);
                #[cfg(not(test))]
                near_sdk::env::log_str(&format!("[TRACE] {}", val));
                Ok(val)
            }
            "inspect" => {
                // Return detailed type+value info string
                let val = args.get(0).cloned().unwrap_or(LispVal::Nil);
                let type_str = match &val {
                    LispVal::Nil => "nil",
                    LispVal::Bool(_) => "boolean",
                    LispVal::Num(_) => "integer",
                    LispVal::Float(_) => "float",
                    LispVal::Str(_) => "string",
                    LispVal::List(items) => {
                        return Ok(LispVal::Str(format!("list[{}]: {}", items.len(), val)));
                    }
                    LispVal::Map(m) => {
                        return Ok(LispVal::Str(format!("map{{{} keys}}: {}", m.len(), val)));
                    }
                    LispVal::Lambda { params, .. } => {
                        return Ok(LispVal::Str(format!("lambda({}): <function>", params.len())));
                    }
                    LispVal::Sym(s) => {
                        return Ok(LispVal::Str(format!("symbol: {}", s)));
                    }
                    _ => "unknown",
                };
                Ok(LispVal::Str(format!("{}: {}", type_str, val)))
            }
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
            // Note: NEAR charges real gas for storage ops — no synthetic accounting needed.
            "near/storage-write" => {
                let raw_key = as_str(&args[0])?;
                let val = as_str(&args[1])?;
                let key = sandbox_key(&raw_key, env);
                env::storage_write(key.as_bytes(), val.as_bytes());
                Ok(LispVal::Bool(true))
            }
            "near/storage-read" => {
                let raw_key = as_str(&args[0])?;
                let key = sandbox_key(&raw_key, env);
                Ok(env::storage_read(key.as_bytes())
                    .map(|v| LispVal::Str(String::from_utf8_lossy(&v).to_string()))
                    .unwrap_or(LispVal::Nil))
            }
            "near/storage-remove" => {
                let raw_key = as_str(&args[0])?;
                let key = sandbox_key(&raw_key, env);
                env::storage_remove(key.as_bytes());
                Ok(LispVal::Bool(true))
            }
            "near/storage-has?" => {
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
            "near/batch-call" => {
                // (near/batch-call "recipient.near" (list (list "method" "{}" "0" "50") ...))
                // Each inner list is [method, args_json, deposit_yocto, gas_tgas]
                let recipient_str =
                    as_str(&args[0]).map_err(|_| "near/batch-call: recipient must be string")?;
                let recipient_id: AccountId = recipient_str
                    .parse()
                    .map_err(|_| "near/batch-call: invalid account id")?;
                let call_specs = match args.get(1) {
                    Some(LispVal::List(l)) => l,
                    _ => {
                        return Err("near/batch-call: second arg must be list of call specs".into())
                    }
                };
                if call_specs.is_empty() {
                    return Err("near/batch-call: need at least one call spec".into());
                }
                let mut promise = Promise::new(recipient_id);
                let mut count = 0u64;
                for spec in call_specs {
                    if let LispVal::List(parts) = spec {
                        if parts.len() < 4 {
                            return Err("near/batch-call: each spec needs [method args_json deposit_yocto gas_tgas]".into());
                        }
                        let method = as_str(&parts[0])?;
                        let args_json = as_str(&parts[1])?;
                        let deposit_str = as_str(&parts[2])?;
                        let gas_str = as_str(&parts[3])?;
                        let deposit_u128: u128 = deposit_str
                            .parse()
                            .map_err(|_| "near/batch-call: invalid deposit")?;
                        let gas_tgas: u64 = gas_str
                            .parse()
                            .map_err(|_| "near/batch-call: invalid gas")?;
                        promise = promise.function_call(
                            method,
                            args_json.into_bytes(),
                            NearToken::from_yoctonear(deposit_u128),
                            Gas::from_tgas(gas_tgas),
                        );
                        count += 1;
                    } else {
                        return Err("near/batch-call: each call spec must be a list".into());
                    }
                }
                let _ = promise;
                Ok(LispVal::Str(format!("batch:{}:{}", recipient_str, count)))
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

            "fmt" => {
                let template = match &args[0] {
                    LispVal::Str(s) => s.clone(),
                    _ => return Err("fmt: need template string".into()),
                };
                let data = &args[1];
                let mut result = String::new();
                let chars: Vec<char> = template.chars().collect();
                let mut i = 0;
                while i < chars.len() {
                    if chars[i] == '{' {
                        let mut key = String::new();
                        i += 1;
                        while i < chars.len() && chars[i] != '}' {
                            key.push(chars[i]);
                            i += 1;
                        }
                        if i < chars.len() {
                            i += 1;
                        } // skip '}'
                        let mut found = false;
                        if let LispVal::Map(map) = data {
                            if let Some(val) = map.get(&key) {
                                // For string values, use the raw content (no quotes)
                                match val {
                                    LispVal::Str(s) => result.push_str(s),
                                    _ => result.push_str(&val.to_string()),
                                }
                                found = true;
                            }
                        }
                        if !found {
                            result.push('{');
                            result.push_str(&key);
                            result.push('}');
                        }
                    } else {
                        result.push(chars[i]);
                        i += 1;
                    }
                }
                Ok(LispVal::Str(result))
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

            // --- List stdlib native builtins (zero stdlib gas overhead) ---
            "empty?" => Ok(LispVal::Bool(
                matches!(&args[0], LispVal::Nil)
                    || matches!(&args[0], LispVal::List(ref v) if v.is_empty()),
            )),

            "range" => {
                let start = as_num(args.get(0).ok_or("range: need 2 args")?)?;
                let end = as_num(args.get(1).ok_or("range: need 2 args")?)?;
                if start >= end {
                    return Ok(LispVal::List(vec![]));
                }
                let vals: Vec<LispVal> = (start..end).map(LispVal::Num).collect();
                Ok(LispVal::List(vals))
            }

            "reverse" => match &args[0] {
                LispVal::List(l) => Ok(LispVal::List(l.iter().rev().cloned().collect())),
                LispVal::Nil => Ok(LispVal::List(vec![])),
                other => Err(format!("reverse: expected list, got {}", other)),
            },

            "sort" => {
                let mut vals = match &args[0] {
                    LispVal::List(l) => l.clone(),
                    LispVal::Nil => vec![],
                    other => return Err(format!("sort: expected list, got {}", other)),
                };
                vals.sort_by(|a, b| {
                    let fa = match a {
                        LispVal::Num(n) => *n as f64,
                        LispVal::Float(f) => *f,
                        _ => 0.0,
                    };
                    let fb = match b {
                        LispVal::Num(n) => *n as f64,
                        LispVal::Float(f) => *f,
                        _ => 0.0,
                    };
                    fa.partial_cmp(&fb).unwrap_or(std::cmp::Ordering::Equal)
                });
                Ok(LispVal::List(vals))
            }

            "zip" => {
                let a = match &args[0] {
                    LispVal::List(l) => l.clone(),
                    LispVal::Nil => vec![],
                    other => return Err(format!("zip: expected list, got {}", other)),
                };
                let b = match args.get(1) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => vec![],
                    Some(other) => return Err(format!("zip: expected list, got {}", other)),
                    None => return Err("zip: need 2 args".into()),
                };
                let pairs: Vec<LispVal> = a
                    .iter()
                    .zip(b.iter())
                    .map(|(x, y)| LispVal::List(vec![x.clone(), y.clone()]))
                    .collect();
                Ok(LispVal::List(pairs))
            }

            "map" => {
                let func = args.get(0).ok_or("map: need (f list)")?;
                let lst = match args.get(1) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => return Ok(LispVal::List(vec![])),
                    Some(other) => return Err(format!("map: expected list, got {}", other)),
                    None => return Err("map: need (f list)".into()),
                };
                let mut result = Vec::with_capacity(lst.len());
                for elem in &lst {
                    result.push(call_val(func, &[elem.clone()], env, gas)?);
                }
                Ok(LispVal::List(result))
            }

            "filter" => {
                let func = args.get(0).ok_or("filter: need (pred list)")?;
                let lst = match args.get(1) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => return Ok(LispVal::List(vec![])),
                    Some(other) => return Err(format!("filter: expected list, got {}", other)),
                    None => return Err("filter: need (pred list)".into()),
                };
                let mut result = Vec::new();
                for elem in &lst {
                    if is_truthy(&call_val(func, &[elem.clone()], env, gas)?) {
                        result.push(elem.clone());
                    }
                }
                Ok(LispVal::List(result))
            }

            "reduce" => {
                let func = args.get(0).ok_or("reduce: need (f init list)")?;
                let mut acc = args.get(1).ok_or("reduce: need (f init list)")?.clone();
                let lst = match args.get(2) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => return Ok(acc),
                    Some(other) => return Err(format!("reduce: expected list, got {}", other)),
                    None => return Err("reduce: need (f init list)".into()),
                };
                for elem in &lst {
                    acc = call_val(func, &[acc.clone(), elem.clone()], env, gas)?;
                }
                Ok(acc)
            }

            "find" => {
                let func = args.get(0).ok_or("find: need (pred list)")?;
                let lst = match args.get(1) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => return Ok(LispVal::Nil),
                    Some(other) => return Err(format!("find: expected list, got {}", other)),
                    None => return Err("find: need (pred list)".into()),
                };
                for elem in &lst {
                    if is_truthy(&call_val(func, &[elem.clone()], env, gas)?) {
                        return Ok(elem.clone());
                    }
                }
                Ok(LispVal::Nil)
            }

            "some" => {
                let func = args.get(0).ok_or("some: need (pred list)")?;
                let lst = match args.get(1) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => return Ok(LispVal::Bool(false)),
                    Some(other) => return Err(format!("some: expected list, got {}", other)),
                    None => return Err("some: need (pred list)".into()),
                };
                for elem in &lst {
                    if is_truthy(&call_val(func, &[elem.clone()], env, gas)?) {
                        return Ok(LispVal::Bool(true));
                    }
                }
                Ok(LispVal::Bool(false))
            }

            "every" => {
                let func = args.get(0).ok_or("every: need (pred list)")?;
                let lst = match args.get(1) {
                    Some(LispVal::List(l)) => l.clone(),
                    Some(LispVal::Nil) => return Ok(LispVal::Bool(true)),
                    Some(other) => return Err(format!("every: expected list, got {}", other)),
                    None => return Err("every: need (pred list)".into()),
                };
                for elem in &lst {
                    if !is_truthy(&call_val(func, &[elem.clone()], env, gas)?) {
                        return Ok(LispVal::Bool(false));
                    }
                }
                Ok(LispVal::Bool(true))
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
        rest_param,
        body,
        closed_env,
    } = head
    {
        apply_lambda(params, &rest_param, body, closed_env, &args, env, gas)
    } else if let LispVal::List(ll) = head {
        // Inline lambda: ((lambda (x) (* x x)) 5)
        if ll.len() < 3 {
            return Err("inline lambda too short".into());
        }
        let (params, rest_param) = parse_params(&ll[1])?;
        apply_lambda(&params, &rest_param, &ll[2], &vec![], &args, env, gas)
    } else {
        Err("not callable".into())
    }
}

fn call_val(
    func: &LispVal,
    args: &[LispVal],
    env: &mut Env,
    gas: &mut u64,
) -> Result<LispVal, String> {
    match func {
        LispVal::Lambda {
            params,
            rest_param,
            body,
            closed_env,
        } => apply_lambda(params, rest_param, body, closed_env, args, env, gas),
        LispVal::List(ll) if ll.len() >= 3 => {
            let (params, rest_param) = parse_params(&ll[1])?;
            apply_lambda(&params, &rest_param, &ll[2], &vec![], args, env, gas)
        }
        LispVal::Sym(_) => {
            // Allow raw builtin names as first-class functions:
            // (reduce + 0 (list 1 2 3)) works because + is dispatched natively.
            let mut call = vec![func.clone()];
            call.extend(args.iter().cloned());
            dispatch_call(&call, env, gas)
        }
        _ => Err(format!("not callable: {}", func)),
    }
}

