use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::store::IterableSet;
use near_sdk::{
    env, near, AccountId, CryptoHash, Gas, GasWeight, NearToken, Promise, PromiseResult,
};
use std::collections::{BTreeMap, HashMap};


use crate::types::{LispVal, Env, STORAGE_GAS_COST, consume_gas, get_stdlib_code};
use crate::helpers::*;
use crate::parser::parse_all;
use crate::eval::lisp_eval;
use crate::vm::*;


#[near(contract_state)]
pub struct LispContract {
    owner: AccountId,
    eval_gas_limit: u64,
    policy_names: IterableSet<String>,
    script_names: IterableSet<String>,
    module_names: IterableSet<String>,
    eval_whitelist: IterableSet<AccountId>,
}

impl Default for LispContract {
    fn default() -> Self {
        Self {
            owner: env::signer_account_id(),
            eval_gas_limit: 10_000,
            policy_names: IterableSet::new(b"p"),
            script_names: IterableSet::new(b"s"),
            module_names: IterableSet::new(b"m"),
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
            module_names: IterableSet::new(b"m"),
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
        let mut env = Env::new();
        env.push(
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        );
        run_program(&code, &mut env, self.eval_gas_limit)
            .unwrap_or_else(|e| format!("ERROR: {}", e))
    }

    pub fn eval_with_input(&self, code: String, input_json: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        let mut env = Env::new();
        // Push user-supplied vars first so they cannot shadow the prefix
        if let Ok(map) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input_json)
        {
            for (k, v) in &map {
                env.push(k.clone(), json_to_lisp(v.clone()));
            }
            // Also expose full input as a dict (consistent with eval_async_with_input)
            env.push(
                "input".to_string(),
                json_to_lisp(serde_json::Value::Object(map)),
            );
        }
        // Push __storage_prefix__ AFTER input vars so it takes precedence and
        // cannot be overwritten by an attacker-controlled input_json.
        env.push(
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        );
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
        self.policy_names.insert(name.clone());
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"save_policy\",\"data\":{{\"name\":\"{}\"}}}}",
            name
        ));
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

    /// Returns current storage usage in bytes.
    pub fn storage_usage(&self) -> u64 {
        env::storage_usage()
    }

    /// Returns JSON string with total/available/locked balance info.
    pub fn storage_balance(&self) -> String {
        let balance = env::account_balance();
        let locked = env::account_locked_balance();
        let available = balance.as_yoctonear().saturating_sub(locked.as_yoctonear());
        format!(
            "{{\"total\":\"{}\",\"available\":\"{}\",\"locked\":\"{}\"}}",
            balance.as_yoctonear(),
            available,
            locked.as_yoctonear()
        )
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
        self.script_names.insert(name.clone());
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"save_script\",\"data\":{{\"name\":\"{}\"}}}}",
            name
        ));
    }

    /// View: get a stored script by name
    pub fn get_script(&self, name: String) -> Option<String> {
        env::storage_read(format!("script:{}", name).as_bytes())
            .map(|b| String::from_utf8_lossy(&b).to_string())
    }

    /// View: read a key from eval-namespaced storage (caller-isolated).
    /// Lets other contracts read cached data written by Lisp scripts.
    /// Key is auto-prefixed with `eval:{caller}:`.
    pub fn get_data(&self, key: String) -> Option<String> {
        let storage_key = format!("eval:{}:{}", env::predecessor_account_id(), key);
        env::storage_read(storage_key.as_bytes())
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
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"remove_script\",\"data\":{{\"name\":\"{}\"}}}}",
            name
        ));
    }

    // --- Custom module management ---

    /// Store a custom module (owner-only). Modules can be loaded via require.
    pub fn save_module(&mut self, name: String, code: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can save modules"
        );
        // Validate: module must parse
        match parse_all(&code) {
            Ok(_) => {}
            Err(e) => panic!("Module parse error: {}", e),
        }
        env::storage_write(format!("module:{}", name).as_bytes(), code.as_bytes());
        self.module_names.insert(name.clone());
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"save_module\",\"data\":{{\"name\":\"{}\"}}}}",
            name
        ));
    }

    /// View: get a stored module by name
    pub fn get_module(&self, name: String) -> Option<String> {
        env::storage_read(format!("module:{}", name).as_bytes())
            .map(|b| String::from_utf8_lossy(&b).to_string())
    }

    /// View: list all stored module names
    pub fn list_modules(&self) -> Vec<String> {
        self.module_names.iter().cloned().collect()
    }

    /// Delete a stored module (owner-only)
    pub fn remove_module(&mut self, name: String) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can remove modules"
        );
        env::storage_remove(format!("module:{}", name).as_bytes());
        self.module_names.remove(&name);
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"remove_module\",\"data\":{{\"name\":\"{}\"}}}}",
            name
        ));
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
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"remove_policy\",\"data\":{{\"name\":\"{}\"}}}}",
            name
        ));
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

    /// Async eval of a stored script with JSON input variables.
    /// Combines `eval_script_async` (ccall/yield support) with `eval_with_input` (input injection).
    /// Single-call pattern: no need to write params to storage first.
    ///
    /// Lisp code can reference `input` or any key from input_json directly:
    ///   eval_script_async_with_input("oracle_query", r#"{"asset": "wrap.testnet"}"#)
    ///   → script sees `(dict/get input "asset")` → "wrap.testnet"
    pub fn eval_script_async_with_input(
        &mut self,
        name: String,
        input_json: String,
    ) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        match env::storage_read(format!("script:{}", name).as_bytes()) {
            Some(bytes) => {
                let code = String::from_utf8_lossy(&bytes).to_string();
                let mut eval_env = Env::new();
                // Inject user-supplied input vars (same as eval_with_input)
                if let Ok(map) =
                    serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input_json)
                {
                    // Inject each key as a top-level var
                    for (k, v) in &map {
                        eval_env.push(k.clone(), json_to_lisp(v.clone()));
                    }
                    // Also expose the full input as a dict for convenience
                    eval_env.push(
                        "input".to_string(),
                        json_to_lisp(serde_json::Value::Object(map)),
                    );
                }
                // Push storage prefix AFTER input vars (cannot be overwritten)
                eval_env.push(
                    "__storage_prefix__".to_string(),
                    LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
                );
                match run_program_with_ccall(&code, &mut eval_env, self.eval_gas_limit) {
                    Ok(RunResult::Done(result)) => result,
                    Ok(RunResult::Yield { yields, state }) => {
                        Self::setup_batch_yield_chain(yields, state)
                    }
                    Err(e) => format!("ERROR: {}", e),
                }
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
        let old_owner = self.owner.clone();
        env::log_str(&format!(
            "EVENT_JSON:{{\"standard\":\"near-lisp\",\"version\":\"1.0.0\",\"event\":\"transfer_ownership\",\"data\":{{\"old_owner\":\"{}\",\"new_owner\":\"{}\"}}}}",
            old_owner, new_owner
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
        let mut eval_env = Env::new();
        eval_env.push(
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        );
        match run_program_with_ccall(&code, &mut eval_env, self.eval_gas_limit) {
            Ok(RunResult::Done(result)) => result,
            Ok(RunResult::Yield { yields, state }) => Self::setup_batch_yield_chain(yields, state),
            Err(e) => format!("ERROR: {}", e),
        }
    }

    /// Async eval with cross-contract callback.
    ///
    /// Like `eval_async`, but when the computation completes, the result is
    /// delivered as a cross-contract function call to `callback_account.callback_method`
    /// instead of being returned as a receipt value. This lets external contracts
    /// receive Lisp computation results without polling.
    ///
    /// The callback receives the result string as its only argument (raw bytes).
    ///
    /// Example flow:
    ///   1. Agent contract calls: eval_async_with_callback(
    ///        "(define price (near/ccall \"ref.near\" \"get_price\" \"{}\")) (+ (to-num price) 10)",
    ///        "agent.testnet",
    ///        "on_result"
    ///      )
    ///   2. kampy runs eval, hits ccall → yields
    ///   3. ccall completes → resume_eval fires → continues eval
    ///   4. Final result: calls agent.testnet.on_result("42")
    pub fn eval_async_with_callback(
        &mut self,
        code: String,
        callback_account: String,
        callback_method: String,
    ) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        let mut eval_env = Env::new();
        eval_env.push(
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        );
        match run_program_with_ccall(&code, &mut eval_env, self.eval_gas_limit) {
            Ok(RunResult::Done(result)) => {
                // No ccalls needed — fire callback immediately
                let result_str = result.to_string();
                env::log_str(&format!(
                    "CALLBACK_IMMEDIATE: sending to {}.{}",
                    callback_account, callback_method
                ));
                Promise::new(callback_account.parse().unwrap()).function_call(
                    callback_method,
                    result_str.clone().into_bytes(),
                    NearToken::from_yoctonear(0),
                    Gas::from_tgas(50),
                );
                result_str
            }
            Ok(RunResult::Yield {
                yields,
                mut state,
            }) => {
                // Store callback in VmState so resume_eval can fire it later
                state.callback = Some(CallbackInfo {
                    account: callback_account,
                    method: callback_method,
                });
                Self::setup_batch_yield_chain(yields, state)
            }
            Err(e) => format!("ERROR: {}", e),
        }
    }

    /// Async eval with input injection — combines eval_async (ccall/yield) with
    /// eval_with_input (JSON input). Single call, no storage intermediary needed.
    ///
    /// Input keys become top-level vars AND are available via `(dict/get input "key")`.
    pub fn eval_async_with_input(&mut self, code: String, input_json: String) -> String {
        assert!(self.is_eval_allowed(), "Caller not allowed to eval");
        let mut eval_env = Env::new();
        if let Ok(map) =
            serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&input_json)
        {
            for (k, v) in &map {
                eval_env.push(k.clone(), json_to_lisp(v.clone()));
            }
            eval_env.push(
                "input".to_string(),
                json_to_lisp(serde_json::Value::Object(map)),
            );
        }
        // Push storage prefix AFTER input vars (cannot be overwritten)
        eval_env.push(
            "__storage_prefix__".to_string(),
            LispVal::Str(format!("eval:{}:", env::predecessor_account_id())),
        );
        match run_program_with_ccall(&code, &mut eval_env, self.eval_gas_limit) {
            Ok(RunResult::Done(result)) => result,
            Ok(RunResult::Yield { yields, state }) => Self::setup_batch_yield_chain(yields, state),
            Err(e) => format!("ERROR: {}", e),
        }
    }

    /// Yield callback — resumes evaluation after ALL batched cross-contract calls complete.
    ///
    /// Called automatically by NEAR's yield/resume mechanism when
    /// `promise_yield_resume` is invoked by `auto_resume_batch_ccall`.
    ///
    /// Flow:
    ///   1. auto_resume_batch_ccall collects all N ccall results,
    ///      borsh-serializes them, calls promise_yield_resume(data_id, results)
    ///   2. NEAR delivers the results to this deferred receipt
    ///   3. This method deserializes VmState, injects ALL results, continues eval
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

        // Preserve callback info before consuming state
        let callback = state.callback.clone();

        // Read batch results from yield_resume payload
        // auto_resume_batch_ccall borsh-serializes Vec<Vec<u8>>
        // SDK 5.6.0: promise_result is deprecated but we can't use promise_result_checked
        // without bumping SDK (requires rustc 1.88+, WASM target pinned to 1.86.0).
        #[allow(deprecated)]
        let ccall_results: Vec<Vec<u8>> = match env::promise_result(0) {
            PromiseResult::Successful(data) => borsh::from_slice(&data)
                .unwrap_or_else(|e| panic!("Failed to deserialize batch results: {}", e)),
            PromiseResult::Failed => {
                env::storage_remove(yield_id.as_bytes());
                return "ERROR: ccall batch failed".to_string();
            }
        };

        // Inject all results into environment
        let mut eval_env = state.env;

        for (i, result_bytes) in ccall_results.iter().enumerate() {
            let pending_var = state.pending_vars.get(i);

            let raw = String::from_utf8_lossy(result_bytes).to_string();
            let ccall_result_val = serde_json::from_str::<serde_json::Value>(&raw)
                .map(json_to_lisp)
                .unwrap_or(LispVal::Str(raw));

            if let Some(Some(var)) = pending_var {
                // (define var (near/ccall ...)) → inject result as the variable
                eval_env.push(var.clone(), ccall_result_val.clone());
            } else {
                // standalone (near/ccall ...) → inject as __ccall_result__
                eval_env.push("__ccall_result__".to_string(), ccall_result_val.clone());
            }

            // Append result to accumulated __ccall_results__ list (for near/batch-result)
            {
                let results_entry = eval_env.get_mut("__ccall_results__");
                match results_entry {
                    Some(LispVal::List(ref mut vals)) => {
                        vals.push(ccall_result_val.clone());
                    }
                    _ => eval_env.push(
                        "__ccall_results__".to_string(),
                        LispVal::List(vec![ccall_result_val.clone()]),
                    ),
                }
            }
        }

        // Cleanup stored state
        env::storage_remove(yield_id.as_bytes());

        // Continue evaluating remaining expressions using ccall-aware runner
        let mut gas = state.gas;
        match run_remaining_with_ccall(&state.remaining, &mut eval_env, &mut gas) {
            Ok(RunResult::Done(result)) => {
                // If a callback is registered, dispatch result via cross-contract call
                if let Some(cb) = callback {
                    let result_str = result.to_string();
                    env::log_str(&format!(
                        "CALLBACK: sending result to {}.{}",
                        cb.account, cb.method
                    ));
                    Promise::new(cb.account.parse().unwrap()).function_call(
                        cb.method,
                        result_str.clone().into_bytes(),
                        NearToken::from_yoctonear(0),
                        Gas::from_tgas(50),
                    );
                    return result_str;
                }
                result
            }
            Ok(RunResult::Yield {
                yields,
                state: mut new_state,
            }) => {
                // Propagate callback through yield cycles
                new_state.callback = callback;
                // More ccalls found — set up another batch yield chain
                Self::setup_batch_yield_chain(yields, new_state)
            }
            Err(e) => format!("ERROR: {}", e),
        }
    }

    /// Auto-resume callback — called when ALL parallel cross-contract promises complete.
    ///
    /// Reads all N promise results, borsh-serializes them into Vec<Vec<u8>>,
    /// and passes them to `promise_yield_resume` to wake up the deferred
    /// `resume_eval` receipt with all results at once.
    pub fn auto_resume_batch_ccall(&mut self, data_id_hex: String) {
        // Guard: must be called as cross-contract callback
        let count = env::promise_results_count();
        assert!(count > 0, "auto_resume_batch_ccall: must be called as callback");

        let data_id_bytes = hex_decode(&data_id_hex);
        let data_id: CryptoHash = data_id_bytes.try_into().expect("data_id must be 32 bytes");

        // Collect ALL promise results
        let mut results: Vec<Vec<u8>> = Vec::with_capacity(count as usize);
        #[allow(deprecated)] // SDK 5.6.0 pinned; can't use promise_result_checked (needs rustc 1.88+)
        for i in 0..count {
            match env::promise_result(i) {
                PromiseResult::Successful(data) => results.push(data),
                PromiseResult::Failed => results.push(vec![]),
            }
        }

        // Borsh-serialize the results and resume the yield
        let payload = borsh::to_vec(&results).expect("Failed to serialize batch results");
        env::promise_yield_resume(&data_id, &payload);
    }

    // -----------------------------------------------------------------------
    // Shared helpers for yield chain setup (used by eval_async & resume_eval)
    // -----------------------------------------------------------------------

    /// Set up a yield + cross-contract call + auto-resume callback chain.
    /// Used by both `eval_async` (first yield) and `resume_eval` (re-yield).
    /// Set up a batch yield + parallel cross-contract calls + auto-resume callback chain.
    ///
    /// Creates ONE yield, N parallel cross-contract promises combined via Promise::all(),
    /// and ONE auto_resume_batch_ccall callback that collects all N results.
    /// This uses a single yield cycle regardless of how many ccalls are batched,
    /// saving ~66T per additional ccall vs the old sequential approach.
    fn setup_batch_yield_chain(yields: Vec<CcallYield>, state: VmState) -> String {
        let n = yields.len();
        assert!(n > 0, "setup_batch_yield_chain: empty yields");

        // Save VM state to contract storage
        let yield_id = format!("vm:{}:{}", env::block_height(), env::block_timestamp());
        let state_bytes = borsh::to_vec(&state).expect("VmState serialization failed");
        env::storage_write(yield_id.as_bytes(), &state_bytes);

        // Read gas budget FIRST, before any promise operations.
        // Every Promise::new().function_call() deducts gas from prepaid immediately.
        let prepaid = env::prepaid_gas().as_gas();
        let used = env::used_gas().as_gas();
        let remaining = prepaid.saturating_sub(used);

        let total_ccall_gas: u64 = yields.iter().map(|y| y.gas_tgas * 1_000_000_000_000).sum();
        // auto_resume_batch_ccall iterates N promise results + borsh-serializes them
        // Base: ~2T, per-result: ~0.1T (promise_result read + push)
        let auto_resume_gas = Gas::from_tgas(2 + (n as u64 * 100_000_000_000 / 1_000_000_000_000).max(1));
        let yield_overhead: u64 = 5_000_000_000_000; // 5 Tgas (reduced from 40T→10T→5T)
        // Dynamic reserve: accounts for Promise::and() chain overhead (~0.25T per .and() call)
        // N promises → N-1 .and() calls + .then() callback (~0.3T) + misc overhead (~2T)
        let reserve: u64 = (n as u64).saturating_sub(1)
            .saturating_mul(300_000_000_000) // 0.3T per .and() call (measured 0.252T + margin)
            .saturating_add(3_000_000_000_000); // 3T base overhead

        // Debug: log gas values for gas optimization analysis
        env::log_str(&format!(
            "GAS_DEBUG: prepaid={}T used={}T remaining={}T n={} total_ccall_gas={}T",
            prepaid / 1_000_000_000_000,
            used / 1_000_000_000_000,
            remaining / 1_000_000_000_000,
            n,
            total_ccall_gas / 1_000_000_000_000,
        ));

        // Count future yield cycles in remaining expressions to right-size resume gas.
        // Each group of consecutive ccalls forms one yield cycle, separated by non-ccall exprs.
        let mut in_ccall_group = false;
        let mut future_yield_cycles: u64 = 0;
        let mut future_ccall_count: u64 = 0;
        for expr in state.remaining.iter() {
            let is_ccall = check_ccall(expr, &mut Env::new(), &mut 10000u64)
                .map(|r| r.is_some())
                .unwrap_or(false);
            if is_ccall {
                future_ccall_count += 1;
                if !in_ccall_group {
                    future_yield_cycles += 1;
                    in_ccall_group = true;
                }
            } else {
                in_ccall_group = false;
            }
        }

        // Base overhead for resume: deserialize VmState + inject results + eval remaining
        let resume_base: u64 = 5_000_000_000_000; // 5T
        // Per-ccall overhead in resume: promise_result read + JSON parse + env injection
        let per_ccall_resume: u64 = 500_000_000_000; // 0.5T per ccall result
        let current_batch_cost = resume_base.saturating_add(n as u64 * per_ccall_resume);

        let resume_gas_needed = if future_yield_cycles > 0 {
            // Each future yield cycle needs:
            //   setup_batch_yield_chain overhead: 5T (yield_overhead)
            //   auto_resume_batch_ccall: ~3T
            //   resume_eval: ~5T
            //   reserve: ~3T + 0.3T*(ccalls_in_batch - 1)
            // Plus the ccall gas itself
            let future_ccall_gas: u64 = future_ccall_count * 10_000_000_000_000; // 10T each
            let per_cycle_overhead: u64 = 20_000_000_000_000; // 20T per yield cycle
            current_batch_cost
                .saturating_add(future_yield_cycles * per_cycle_overhead)
                .saturating_add(future_ccall_gas)
        } else {
            current_batch_cost
        };

        let resume_effective = remaining
            .saturating_sub(yield_overhead)
            .saturating_sub(total_ccall_gas)
            .saturating_sub(auto_resume_gas.as_gas())
            .saturating_sub(reserve);

        // Cap resume gas at what we actually need — don't waste the rest
        let capped_effective = resume_effective.min(resume_gas_needed.saturating_sub(yield_overhead));
        let resume_gas = Gas::from_gas(capped_effective.saturating_add(yield_overhead));

        // Debug: log the full gas budget breakdown
        let total_deducted_tgas = resume_gas.as_gas() / 1_000_000_000_000
            + total_ccall_gas / 1_000_000_000_000
            + auto_resume_gas.as_gas() / 1_000_000_000_000;
        let surplus_tgas = (prepaid / 1_000_000_000_000)
            .saturating_sub(used / 1_000_000_000_000)
            .saturating_sub(total_deducted_tgas)
            .saturating_sub(reserve / 1_000_000_000_000);
        env::log_str(&format!(
            "GAS_BUDGET: resume={}T ccall_total={}T auto={}T reserve={}T deducted={}T surplus={surplus_tgas}T",
            resume_gas.as_gas() / 1_000_000_000_000,
            total_ccall_gas / 1_000_000_000_000,
            auto_resume_gas.as_gas() / 1_000_000_000_000,
            reserve / 1_000_000_000_000,
            total_deducted_tgas,
        ));

        // Step 1: Create yield — stores data_id in register 0
        let yield_args = serde_json::json!({"yield_id": &yield_id}).to_string();
        env::promise_yield_create(
            "resume_eval",
            yield_args.as_bytes(),
            resume_gas,
            GasWeight(0),
            0,
        );

        let data_id = env::read_register(0).expect("promise_yield_create should write data_id");
        let data_id_hex = hex_encode(&data_id);

        env::log_str(&format!(
            "GAS_AFTER_YIELD: used={}T",
            env::used_gas().as_gas() / 1_000_000_000_000,
        ));

        // Step 2: Create N parallel cross-contract call promises
        let self_id = env::current_account_id();

        let mut promises: Vec<Promise> = Vec::with_capacity(n);
        for (i, yi) in yields.iter().enumerate() {
            let account_id: near_sdk::AccountId = yi
                .account
                .parse()
                .expect("invalid account id in near/ccall");
            let cc_gas = Gas::from_tgas(yi.gas_tgas);
            let p = Promise::new(account_id).function_call(
                yi.method.clone(),
                yi.args_bytes.clone(),
                NearToken::from_yoctonear(yi.deposit),
                cc_gas,
            );
            promises.push(p);
            // Log every 25th promise and the last one
            if (i + 1) % 25 == 0 || i == n - 1 {
                env::log_str(&format!(
                    "GAS_AFTER_PROMISE_{}/{}: used={}T remaining={}T",
                    i + 1, n,
                    env::used_gas().as_gas() / 1_000_000_000_000,
                    (prepaid - env::used_gas().as_gas()) / 1_000_000_000_000,
                ));
            }
        }

        env::log_str(&format!(
            "GAS_ALL_PROMISES: used={}T remaining={}T resume_gas={}T",
            env::used_gas().as_gas() / 1_000_000_000_000,
            (prepaid - env::used_gas().as_gas()) / 1_000_000_000_000,
            resume_gas.as_gas() / 1_000_000_000_000,
        ));

        // Step 3: Combine all promises into one, then chain single callback
        let auto_args = serde_json::json!({"data_id_hex": data_id_hex}).to_string();

        let combined = if promises.len() == 1 {
            promises.into_iter().next().unwrap()
        } else {
            let mut iter = promises.into_iter();
            let first = iter.next().unwrap();
            let second = iter.next().unwrap();
            let mut combined = first.and(second);
            for p in iter {
                combined = combined.and(p);
            }
            combined
        };

        let _ = combined.then(Promise::new(self_id).function_call(
            "auto_resume_batch_ccall".to_string(),
            auto_args.into_bytes(),
            NearToken::from_yoctonear(0),
            auto_resume_gas,
        ));

        "YIELDING".to_string()
    }
}

