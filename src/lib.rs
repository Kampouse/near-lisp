// near-lisp — on-chain Lisp interpreter for NEAR Protocol
// Split into modules — compiled into single WASM binary

mod bytecode;
mod contract;
mod eval;
mod helpers;
mod parser;
mod types;
mod vm;

// Re-exports
pub use bytecode::{exec_compiled_loop, try_compile_loop};
pub use contract::LispContract;
pub use eval::lisp_eval;
pub use helpers::{is_builtin_name, is_truthy};
pub use parser::parse_all;
pub use types::{check_gas, get_stdlib_code, Env, LispVal, DEFAULT_EVAL_GAS_LIMIT};
pub use vm::{json_to_lisp, lisp_to_json};
pub use vm::{
    run_program, run_program_with_ccall, run_remaining_with_ccall, CallbackInfo, CcallYield,
    RunResult, VmState,
};
