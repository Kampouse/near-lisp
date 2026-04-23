
use crate::types::{Env, LispVal};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns true if the name is a dispatch_call builtin — these evaluate to
/// themselves as first-class values so they can be passed to map/reduce/etc.
pub fn is_builtin_name(name: &str) -> bool {
    matches!(
        name,
        "+" | "-"
            | "*"
            | "/"
            | "mod"
            | "="
            | "=="
            | "!="
            | "/="
            | "<"
            | ">"
            | "<="
            | ">="
            | "list"
            | "car"
            | "cdr"
            | "cons"
            | "len"
            | "append"
            | "nth"
            | "str-concat"
            | "str-contains"
            | "to-string"
            | "str-length"
            | "str-substring"
            | "str-split"
            | "str-trim"
            | "str-index-of"
            | "str-upcase"
            | "str-downcase"
            | "str-starts-with"
            | "str-ends-with"
            | "str="
            | "str!="
            | "nil?"
            | "list?"
            | "number?"
            | "string?"
            | "map?"
            | "bool?"
            | "to-float"
            | "to-int"
            | "to-num"
            | "type?"
            | "dict"
            | "dict/get"
            | "dict/set"
            | "dict/has?"
            | "dict/keys"
            | "dict/vals"
            | "dict/remove"
            | "dict/merge"
            | "error"
            | "empty?"
            | "range"
            | "reverse"
            | "sort"
            | "zip"
            | "map"
            | "filter"
            | "reduce"
            | "find"
            | "some"
            | "every"
    )
}

pub fn is_truthy(v: &LispVal) -> bool {
    !matches!(v, LispVal::Nil | LispVal::Bool(false))
}

pub fn as_num(v: &LispVal) -> Result<i64, String> {
    match v {
        LispVal::Num(n) => Ok(*n),
        _ => Err(format!("expected number, got {}", v)),
    }
}

pub fn as_float(v: &LispVal) -> Result<f64, String> {
    match v {
        LispVal::Float(f) => Ok(*f),
        LispVal::Num(n) => Ok(*n as f64),
        _ => Err(format!("expected number, got {}", v)),
    }
}

/// Returns true if any argument is a Float (triggering promotion).
pub fn any_float(args: &[LispVal]) -> bool {
    args.iter().any(|a| matches!(a, LispVal::Float(_)))
}

pub fn as_str(v: &LispVal) -> Result<String, String> {
    match v {
        LispVal::Str(s) => Ok(s.clone()),
        LispVal::Sym(s) => Ok(s.clone()),
        LispVal::Num(n) => Ok(n.to_string()),
        LispVal::Float(f) => Ok(f.to_string()),
        _ => Err(format!("expected string, got {}", v)),
    }
}

/// Prepend the storage sandbox prefix from the env (if any).
/// Uses a fast reverse scan — `__storage_prefix__` is typically near the end
/// (pushed early at setup), so `.rev().find()` is usually O(1).
pub fn sandbox_key(raw_key: &str, env: &Env) -> String {
    env.get("__storage_prefix__")
        .and_then(|v| match v {
            LispVal::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .map(|prefix| format!("{}{}", prefix, raw_key))
        .unwrap_or_else(|| raw_key.to_string())
}

pub fn do_arith(
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

pub fn parse_params(val: &LispVal) -> Result<(Vec<String>, Option<String>), String> {
    match val {
        LispVal::List(p) => {
            let mut params = Vec::new();
            let mut rest_param = None;
            let mut seen_rest = false;
            for v in p {
                match v {
                    LispVal::Sym(s) if s == "&rest" => {
                        seen_rest = true;
                    }
                    LispVal::Sym(s) if seen_rest => {
                        rest_param = Some(s.clone());
                        seen_rest = false;
                    }
                    LispVal::Sym(s) => {
                        params.push(s.clone());
                    }
                    _ => return Err("param must be sym".into()),
                }
            }
            Ok((params, rest_param))
        }
        _ => Err("params must be list".into()),
    }
}

// ---------------------------------------------------------------------------
// Apply lambda — core closure logic
// Push/pop optimization: instead of cloning both closed_env AND caller_env into
// a new vec, we push closed_env entries + params into caller_env directly, eval,
// then truncate. This eliminates the caller_env clone — the dominant cost for
// large envs. We still clone closed_env entries (unavoidable with Vec env), but
// closed_env is typically small (just the capture scope).
//
// Lookup ordering via .rev().find(): params > closed_env > original caller_env.
// This gives correct lexical scoping — closure bindings shadow original bindings,
// and caller's recursive definitions (like `(define fib ...)`) are still found.
// ---------------------------------------------------------------------------

// NOTE: apply_lambda moved to eval.rs (avoids circular dep: helpers→eval→helpers)

// ---------------------------------------------------------------------------
// Pattern matching helper
// ---------------------------------------------------------------------------

pub fn match_pattern(pattern: &LispVal, value: &LispVal) -> Option<Vec<(String, LispVal)>> {
    match pattern {
        LispVal::Sym(s) if s == "_" => Some(vec![]),
        LispVal::Sym(s) if s == "else" => Some(vec![]), // else is wildcard in match
        LispVal::Sym(s) if s.starts_with('?') => {
            // Binding variable — strip the '?' prefix
            Some(vec![(s[1..].to_string(), value.clone())])
        }
        // Any other symbol is a binding variable (a, b, c in (a (b c)))
        LispVal::Sym(s) => Some(vec![(s.clone(), value.clone())]),
        LispVal::Num(n) => {
            if value == &LispVal::Num(*n) {
                Some(vec![])
            } else {
                None
            }
        }
        LispVal::Float(f) => {
            if let LispVal::Float(vf) = value {
                if (*f - *vf).abs() < f64::EPSILON {
                    Some(vec![])
                } else {
                    None
                }
            } else {
                None
            }
        }
        LispVal::Str(s) => {
            if value == &LispVal::Str(s.clone()) {
                Some(vec![])
            } else {
                None
            }
        }
        LispVal::Bool(b) => {
            if value == &LispVal::Bool(*b) {
                Some(vec![])
            } else {
                None
            }
        }
        LispVal::List(pats) if !pats.is_empty() => {
            if let LispVal::Sym(s) = &pats[0] {
                if s == "list" {
                    // (list p1 p2 ...) — match list of same length
                    if let LispVal::List(vals) = value {
                        if vals.len() == pats.len() - 1 {
                            let mut all_bindings = vec![];
                            for (p, v) in pats[1..].iter().zip(vals.iter()) {
                                if let Some(bindings) = match_pattern(p, v) {
                                    all_bindings.extend(bindings);
                                } else {
                                    return None;
                                }
                            }
                            Some(all_bindings)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if s == "cons" && pats.len() == 3 {
                    // (cons head tail) — match non-empty list
                    if let LispVal::List(vals) = value {
                        if !vals.is_empty() {
                            let mut all_bindings = vec![];
                            if let Some(b1) = match_pattern(&pats[1], &vals[0]) {
                                all_bindings.extend(b1);
                            } else {
                                return None;
                            }
                            if let Some(b2) =
                                match_pattern(&pats[2], &LispVal::List(vals[1..].to_vec()))
                            {
                                all_bindings.extend(b2);
                            } else {
                                return None;
                            }
                            Some(all_bindings)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    // Bare list pattern: (a (b c)) treated as implicit list destructuring
                    if let LispVal::List(vals) = value {
                        if vals.len() == pats.len() {
                            let mut all_bindings = vec![];
                            for (p, v) in pats.iter().zip(vals.iter()) {
                                if let Some(bindings) = match_pattern(p, v) {
                                    all_bindings.extend(bindings);
                                } else {
                                    return None;
                                }
                            }
                            Some(all_bindings)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            } else {
                // Pattern list with non-symbol first element — treat as implicit list
                if let LispVal::List(vals) = value {
                    if vals.len() == pats.len() {
                        let mut all_bindings = vec![];
                        for (p, v) in pats.iter().zip(vals.iter()) {
                            if let Some(bindings) = match_pattern(p, v) {
                                all_bindings.extend(bindings);
                            } else {
                                return None;
                            }
                        }
                        Some(all_bindings)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
        _ => None,
    }
}
