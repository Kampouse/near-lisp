use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::store::IterableSet;
use near_sdk::{
    env, near, AccountId, CryptoHash, Gas, GasWeight, NearToken, Promise, PromiseResult,
};
use std::collections::{BTreeMap, HashMap};

use crate::eval::lisp_eval;
use crate::helpers::*;
use crate::parser::parse_all;
use crate::types::{Env, LispVal};

// ---------------------------------------------------------------------------
// Public interface — synchronous eval (no ccall support)
// ---------------------------------------------------------------------------

pub fn run_program(code: &str, env: &mut Env, gas_limit: u64) -> Result<String, String> {
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
        serde_json::Value::Number(n) => {
            // Try i64 first (most common case, preserves exactness)
            if let Some(i) = n.as_i64() {
                LispVal::Num(i)
            } else if let Some(f) = n.as_f64() {
                LispVal::Float(f)
            } else if let Some(u) = n.as_u64() {
                LispVal::Num(u as i64)
            } else {
                LispVal::Num(0)
            }
        }
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
    /// Remaining top-level expressions to evaluate on resume
    /// (after all pending ccalls complete).
    pub remaining: Vec<LispVal>,
    /// Accumulated environment bindings.
    pub env: Env,
    /// Gas remaining.
    pub gas: u64,
    /// Variable names for each pending ccall.
    /// Each entry corresponds to one ccall in the batch.
    /// `Some("price")` for `(define price (near/ccall ...))`
    /// `None` for standalone `(near/ccall ...)`
    pub pending_vars: Vec<Option<String>>,
    /// Null-safe flags for each pending ccall.
    /// If true, failed promises return nil instead of aborting.
    pub null_safe_flags: Vec<bool>,
    /// Optional callback for cross-contract result delivery.
    /// When set, the final result is sent as a cross-contract call
    /// instead of being returned as a receipt value.
    pub callback: Option<CallbackInfo>,
}

/// Cross-contract callback info — persisted in VmState across yield cycles.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct CallbackInfo {
    pub account: String,
    pub method: String,
}

/// Result of running a program that may contain cross-contract calls.
pub enum RunResult {
    /// Evaluation completed synchronously.
    Done(String),
    /// Evaluation paused at one or more cross-contract calls.
    /// All ccalls found before a non-ccall expression are batched
    /// into a single yield cycle for parallel execution.
    Yield {
        yields: Vec<CcallYield>,
        state: VmState,
    },
}

/// Pending cross-contract call that requires a yield.
pub struct CcallYield {
    pub account: String,
    pub method: String,
    pub args_bytes: Vec<u8>,
    /// Deposit in yoctoNEAR (0 for view calls).
    pub deposit: u128,
    /// Gas in TeraGas (50 TGas default for view calls).
    pub gas_tgas: u64,
}

/// Internal: extracted cross-contract call info from an expression.
pub struct CcallInfo {
    pending_var: Option<String>,
    account: String,
    method: String,
    args_bytes: Vec<u8>,
    /// Deposit in yoctoNEAR (0 for view calls).
    deposit: u128,
    /// Gas in TeraGas (50 TGas default for view calls).
    gas_tgas: u64,
    /// If true, return nil on error instead of aborting the eval.
    null_safe: bool,
}

/// Helper: classify a ccall function name and return its mode.
/// Returns `None` if not a ccall function.
fn classify_ccall(name: &str) -> Option<CcallMode> {
    match name {
        "near/ccall" | "near/ccall-view" | "near/ccall-view*" => Some(CcallMode::View),
        "near/ccall-call" | "near/ccall-call*" => Some(CcallMode::Call),
        _ => None,
    }
}

/// Check if a ccall function name is the null-safe variant (returns nil on error instead of aborting).
fn is_null_safe_ccall(name: &str) -> bool {
    name.ends_with('*')
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
    env: &mut Env,
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
            CcallMode::View => {
                // Optional 4th arg: gas in Tgas. Default 10T.
                // 10T covers most view calls (get_owner burns 1.4T).
                // Users can override for heavy targets or to fit more ccalls in a batch.
                let gas = inner
                    .get(4)
                    .map(|a| as_str(a))
                    .transpose()?
                    .map(|s| s.parse::<u64>())
                    .transpose()
                    .map_err(|_| "near/ccall: invalid gas (must be number in Tgas)".to_string())?
                    .unwrap_or(10);
                (0u128, gas)
            }
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
            null_safe: is_null_safe_ccall(func),
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
pub fn check_ccall(
    expr: &LispVal,
    env: &mut Env,
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

/// Recursively walk an expression, replacing every `(near/ccall[-view|-call] ...)`
/// sub-expression with a synthetic temp variable `__ccall_tmp_N__`.
///
/// Returns the rewritten expression and a list of `(temp_var_name, ccall_list)` pairs
/// that should be prepended as `(define __ccall_tmp_N__ (near/ccall ...))` statements.
///
/// This lets the existing flat ccall scanner detect and batch ALL cross-contract calls,
/// even when they're nested inside other expressions like:
///   (dict/get (near/ccall "oracle" "get_data" "{}") "prices")
fn lift_nested_ccalls(
    expr: LispVal,
    counter: &mut usize,
    lifts: &mut Vec<(String, LispVal)>,
) -> LispVal {
    match expr {
        LispVal::List(ref list) if !list.is_empty() => {
            // Check if this list IS a ccall itself
            if let Some(LispVal::Sym(func)) = list.first() {
                if classify_ccall(func).is_some() {
                    // This IS a ccall — replace with temp var
                    let var_name = format!("__ccall_tmp_{}__", counter);
                    *counter += 1;
                    lifts.push((var_name.clone(), expr.clone()));
                    return LispVal::Sym(var_name);
                }
                // Skip (define var body) — the body is a single expression that
                // check_ccall Pattern 1 already handles when body is a ccall.
                // We only need to recurse into non-define forms.
                if func == "define" && list.len() == 3 {
                    // Don't lift ccalls from inside the body of a define.
                    // check_ccall handles (define var (near/ccall ...)) directly.
                    // But we DO need to lift if the body contains ccalls nested
                    // deeper — e.g. (define x (f (near/ccall ...) y)).
                    // So recurse into the body, but NOT if the body IS a ccall.
                    if let LispVal::List(body) = &list[2] {
                        if let Some(LispVal::Sym(f)) = body.first() {
                            if classify_ccall(f).is_some() {
                                // Body IS a ccall — leave it alone for check_ccall
                                return expr;
                            }
                        }
                    }
                }
            }
            // Not a ccall — recurse into children
            let mut new_list = Vec::with_capacity(list.len());
            for child in list.iter() {
                new_list.push(lift_nested_ccalls(child.clone(), counter, lifts));
            }
            LispVal::List(new_list)
        }
        _ => expr, // atoms pass through unchanged
    }
}

/// Pre-processing pass: walk all parsed expressions and hoist any nested
/// `(near/ccall ...)` calls into synthetic top-level `(define __ccall_tmp_N__ ...)`
/// statements. The original expressions are rewritten to reference the temp vars.
///
/// Example:
///   (define prices (dict/get (near/ccall "oracle" "get_data" "{}") "prices"))
/// becomes:
///   (define __ccall_tmp_0__ (near/ccall "oracle" "get_data" "{}"))
///   (define prices (dict/get __ccall_tmp_0__ "prices"))
fn lift_all_ccalls(exprs: Vec<LispVal>) -> Vec<LispVal> {
    let mut counter = 0usize;
    let mut result = Vec::new();

    for expr in exprs {
        let mut lifts = Vec::new();
        let rewritten = lift_nested_ccalls(expr, &mut counter, &mut lifts);

        // Emit synthetic defines for each lifted ccall
        for (var_name, ccall_expr) in lifts {
            result.push(LispVal::List(vec![
                LispVal::Sym("define".to_string()),
                LispVal::Sym(var_name),
                ccall_expr,
            ]));
        }

        result.push(rewritten);
    }

    result
}

/// Run a program that may contain cross-contract calls.
///
/// Loops: evaluates non-ccall expressions, then batch-scans for consecutive ccalls,
/// and yields if found. Repeats until all expressions are consumed.
pub fn run_program_with_ccall(
    code: &str,
    env: &mut Env,
    gas_limit: u64,
) -> Result<RunResult, String> {
    let exprs_raw = parse_all(code)?;
    let exprs = lift_all_ccalls(exprs_raw);
    let mut gas = gas_limit;

    let mut pos = 0;
    let mut last_result = LispVal::Nil;

    while pos < exprs.len() {
        // Phase 1: Evaluate all non-ccall expressions at the front
        while pos < exprs.len() {
            if check_ccall(&exprs[pos], env, &mut gas)?.is_some() {
                break; // hit a ccall — stop evaluating
            }
            last_result = lisp_eval(&exprs[pos], env, &mut gas)?;
            pos += 1;
        }

        // Phase 2: Batch-scan consecutive ccalls (single pass, no env clone)
        let mut batch = Vec::new();
        let mut first_after_batch = pos;

        while first_after_batch < exprs.len() {
            if let Some(ccall_info) = check_ccall(&exprs[first_after_batch], env, &mut gas)? {
                batch.push(ccall_info);
                first_after_batch += 1;
            } else {
                break;
            }
        }

        if batch.is_empty() {
            // No ccalls found, all done
            break;
        }

        let yields: Vec<CcallYield> = batch
            .iter()
            .map(|info| CcallYield {
                account: info.account.clone(),
                method: info.method.clone(),
                args_bytes: info.args_bytes.clone(),
                deposit: info.deposit,
                gas_tgas: info.gas_tgas,
            })
            .collect();

        // Extract pending_vars from the already-collected batch (no second scan)
        let pending_vars: Vec<Option<String>> =
            batch.iter().map(|info| info.pending_var.clone()).collect();
        let null_safe_flags: Vec<bool> =
            batch.iter().map(|info| info.null_safe).collect();

        let remaining = exprs[first_after_batch..].to_vec();

        return Ok(RunResult::Yield {
            yields,
            state: VmState {
                remaining,
                env: env.clone(),
                gas,
                pending_vars,
                null_safe_flags,
                callback: None,
            },
        });
    }

    // All expressions evaluated — return tracked last result (no re-evaluation)
    Ok(RunResult::Done(last_result.to_string()))
}

/// Run a list of already-parsed expressions that may contain cross-contract calls.
/// Like `run_program_with_ccall` but takes pre-parsed `Vec<LispVal>` instead of code string.
/// Used by `resume_eval` to continue evaluating remaining expressions after a yield.
///
/// Loops: evaluates non-ccall expressions, then batch-scans for consecutive ccalls,
/// and yields if found. Repeats until all expressions are consumed.
pub fn run_remaining_with_ccall(
    exprs: &[LispVal],
    env: &mut Env,
    gas: &mut u64,
) -> Result<RunResult, String> {
    let mut pos = 0;
    let mut last_result = LispVal::Nil;

    while pos < exprs.len() {
        // Phase 1: Evaluate all non-ccall expressions at the front
        while pos < exprs.len() {
            if check_ccall(&exprs[pos], env, gas)?.is_some() {
                break; // hit a ccall — stop evaluating
            }
            last_result = lisp_eval(&exprs[pos], env, gas)?;
            pos += 1;
        }

        // Phase 2: Batch-scan consecutive ccalls (single pass, no env clone)
        let mut batch = Vec::new();
        let mut first_after_batch = pos;

        while first_after_batch < exprs.len() {
            if let Some(ccall_info) = check_ccall(&exprs[first_after_batch], env, gas)? {
                batch.push(ccall_info);
                first_after_batch += 1;
            } else {
                break;
            }
        }

        if batch.is_empty() {
            // No ccalls found, all done
            break;
        }

        let yields: Vec<CcallYield> = batch
            .iter()
            .map(|info| CcallYield {
                account: info.account.clone(),
                method: info.method.clone(),
                args_bytes: info.args_bytes.clone(),
                deposit: info.deposit,
                gas_tgas: info.gas_tgas,
            })
            .collect();

        // Extract pending_vars from the already-collected batch (no second scan)
        let pending_vars: Vec<Option<String>> =
            batch.iter().map(|info| info.pending_var.clone()).collect();
        let null_safe_flags: Vec<bool> =
            batch.iter().map(|info| info.null_safe).collect();

        let remaining = exprs[first_after_batch..].to_vec();

        // If there are more expressions after this batch, yield to process them later.
        // If nothing remains, we still need to yield to execute the ccalls.
        return Ok(RunResult::Yield {
            yields,
            state: VmState {
                remaining,
                env: env.clone(),
                gas: *gas,
                pending_vars,
                null_safe_flags,
                callback: None,
            },
        });
    }

    // All expressions evaluated — return tracked last result (no re-evaluation)
    Ok(RunResult::Done(last_result.to_string()))
}

// ---------------------------------------------------------------------------
// Hex helpers (avoids adding hex crate dependency)
// ---------------------------------------------------------------------------

pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX_CHARS[(b >> 4) as usize] as char);
        s.push(HEX_CHARS[(b & 0xf) as usize] as char);
    }
    s
}

pub fn hex_decode(hex: &str) -> Vec<u8> {
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
