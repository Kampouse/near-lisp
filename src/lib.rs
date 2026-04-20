// near-lisp — on-chain Lisp interpreter for NEAR Protocol
// Split into modules — compiled into single WASM binary

mod types;
mod parser;
mod helpers;
mod bytecode;
mod eval;
mod vm;
mod contract;

// Re-exports
pub use types::{LispVal, Env, STORAGE_GAS_COST, consume_gas, get_stdlib_code};
pub use parser::parse_all;
pub use eval::lisp_eval;
pub use vm::{run_program, run_program_with_ccall, run_remaining_with_ccall, VmState, RunResult, CallbackInfo, CcallYield};
pub use vm::{json_to_lisp, lisp_to_json};
pub use bytecode::{try_compile_loop, exec_compiled_loop};
pub use helpers::{is_builtin_name, is_truthy};
pub use contract::LispContract;
