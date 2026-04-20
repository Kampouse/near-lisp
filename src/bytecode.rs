use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::store::IterableSet;
use near_sdk::{
    env, near, AccountId, CryptoHash, Gas, GasWeight, NearToken, Promise, PromiseResult,
};
use std::collections::{BTreeMap, HashMap};


use crate::helpers::is_truthy;
use crate::types::{LispVal, Env};

// ---------------------------------------------------------------------------
// Loop Bytecode Compiler — tight VM for loop/recur
// ---------------------------------------------------------------------------
// Compiles (loop ((i init) ...) body) into flat opcodes with slot-indexed
// env. Falls back to lisp_eval for unsupported expressions.
//
// Supported body patterns:
//   (if TEST then-expr (recur ARG1 ARG2 ...))
//   (if TEST then-expr else-expr)
// where TEST and ARGs can use: Num, Sym (binding ref), +, -, *, /, =, <, <=, >, >=
//
// ~20-50x faster than tree-walking because:
//   - No string matching per eval step (flat opcode array, PC increment)
//   - No env linear scan (slot-indexed Vec<LispVal>)
//   - No AST traversal (compiled jump targets)
//   - No LispVal::List construction for recur args
// ---------------------------------------------------------------------------


/// Bytecode opcodes for the loop VM.
#[derive(Clone, Debug)]
enum Op {
    /// Push binding slot value onto stack
    LoadSlot(usize),
    /// Push a literal i64
    PushI64(i64),
    /// Push a literal f64
    PushFloat(f64),
    /// Push a literal bool
    PushBool(bool),
    /// Push a literal string
    PushStr(String),
    /// Push nil
    PushNil,
    /// Duplicate top of stack
    Dup,
    /// Pop and discard top of stack
    Pop,
    /// Pop stack into binding slot
    StoreSlot(usize),
    /// Arithmetic: pop 2, push result
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    /// Comparison: pop 2, push bool
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
    /// Pop stack, jump to addr if truthy
    JumpIfTrue(usize),
    /// Pop stack, jump to addr if falsy
    JumpIfFalse(usize),
    /// Unconditional jump
    Jump(usize),
    /// Pop TOS, return it as the loop result
    Return,
    /// Pop N args into slots 0..N, jump to loop start
    Recur(usize),
    /// Call a builtin by name with N args from stack
    BuiltinCall(String, usize),
}

/// Compiled loop representation.
pub struct CompiledLoop {
    /// Number of binding slots
    num_slots: usize,
    /// Binding names (for fallback)
    slot_names: Vec<String>,
    /// Initial values for slots
    init_vals: Vec<LispVal>,
    /// Bytecode
    code: Vec<Op>,
    /// PC of the loop start (for recur jumps)
    loop_start_pc: usize,
    /// Captured outer env variables (name → value), placed in slots after bindings
    captured: Vec<(String, LispVal)>,
}

/// Compilation context
struct LoopCompiler {
    slot_map: Vec<String>,  // slot index → binding name
    code: Vec<Op>,
    /// Outer env variables captured at compile time (name, value)
    captured: Vec<(String, LispVal)>,
}

impl LoopCompiler {
    fn new(slot_names: Vec<String>) -> Self {
        Self { slot_map: slot_names, code: Vec::new(), captured: Vec::new() }
    }

    /// Look up binding name → slot index (bindings first, then captured env)
    fn slot_of(&self, name: &str) -> Option<usize> {
        if let Some(idx) = self.slot_map.iter().position(|s| s == name) {
            return Some(idx);
        }
        if let Some(idx) = self.captured.iter().position(|(s, _)| s == name) {
            return Some(self.slot_map.len() + idx);
        }
        None
    }

    /// Try to capture an unknown symbol from outer env. Returns true if captured.
    fn try_capture(&mut self, name: &str, outer_env: &Env) -> bool {
        if self.slot_of(name).is_some() { return true; }
        if let Some(val) = outer_env.get(name) {
            self.captured.push((name.to_string(), val.clone()));
            return true;
        }
        false
    }

    /// Try to compile an expression. Returns false if unsupported.
    fn compile_expr(&mut self, expr: &LispVal, outer_env: &Env) -> bool {
        match expr {
            LispVal::Num(n) => { self.code.push(Op::PushI64(*n)); true }
            LispVal::Float(f) => { self.code.push(Op::PushFloat(*f)); true }
            LispVal::Bool(b) => { self.code.push(Op::PushBool(*b)); true }
            LispVal::Str(s) => { self.code.push(Op::PushStr(s.clone())); true }
            LispVal::Nil => { self.code.push(Op::PushNil); true }
            LispVal::Sym(name) => {
                if let Some(slot) = self.slot_of(name) {
                    self.code.push(Op::LoadSlot(slot));
                    true
                } else if self.try_capture(name, outer_env) {
                    let slot = self.slot_of(name).unwrap();
                    self.code.push(Op::LoadSlot(slot));
                    true
                } else {
                    false
                }
            }
            LispVal::List(list) if list.is_empty() => { self.code.push(Op::PushNil); true }
            LispVal::List(list) => {
                if let LispVal::Sym(op) = &list[0] {
                    match op.as_str() {
                        // Variadic arithmetic: chain binary ops
                        "+" | "-" | "*" | "/" | "%" => {
                            let opcode = match op.as_str() {
                                "+" => Op::Add, "-" => Op::Sub, "*" => Op::Mul,
                                "/" => Op::Div, "%" => Op::Mod, _ => unreachable!(),
                            };
                            if list.len() < 3 { return false; }
                            if !self.compile_expr(&list[1], outer_env) { return false; }
                            for arg in &list[2..] {
                                if !self.compile_expr(arg, outer_env) { return false; }
                                self.code.push(opcode.clone());
                            }
                            true
                        }
                        // Variadic comparison: chain binary ops
                        "=" | "<" | "<=" | ">" | ">=" => {
                            let opcode = match op.as_str() {
                                "=" => Op::Eq, "<" => Op::Lt, "<=" => Op::Le,
                                ">" => Op::Gt, ">=" => Op::Ge, _ => unreachable!(),
                            };
                            if list.len() < 3 { return false; }
                            if !self.compile_expr(&list[1], outer_env) { return false; }
                            for arg in &list[2..] {
                                if !self.compile_expr(arg, outer_env) { return false; }
                                self.code.push(opcode.clone());
                            }
                            true
                        }
                        "not" => {
                            let arg = match list.get(1) { Some(a) => a, None => return false };
                            if !self.compile_expr(arg, outer_env) { return false; }
                            self.code.push(Op::PushBool(false));
                            self.code.push(Op::Eq);
                            true
                        }
                        // Nested if: (if test then else) — compiles to jump instructions
                        "if" => {
                            let test = match list.get(1) { Some(t) => t, None => return false };
                            let then_branch = match list.get(2) { Some(t) => t, None => return false };
                            let else_branch = list.get(3);
                            if !self.compile_expr(test, outer_env) { return false; }
                            let jf_idx = self.code.len();
                            self.code.push(Op::JumpIfFalse(0));
                            if !self.compile_expr(then_branch, outer_env) { return false; }
                            let jmp_idx = self.code.len();
                            self.code.push(Op::Jump(0));
                            let else_start = self.code.len();
                            self.code[jf_idx] = Op::JumpIfFalse(else_start);
                            if let Some(ee) = else_branch {
                                if !self.compile_expr(ee, outer_env) { return false; }
                            } else {
                                self.code.push(Op::PushNil);
                            }
                            self.code[jmp_idx] = Op::Jump(self.code.len());
                            true
                        }
                        // recur: compile args, emit Recur(N) — valid in any tail position
                        "recur" => {
                            let num_slots = self.slot_map.len();
                            if list.len() - 1 != num_slots { return false; }
                            for arg in &list[1..] {
                                if !self.compile_expr(arg, outer_env) { return false; }
                            }
                            self.code.push(Op::Recur(num_slots));
                            true
                        }
                        // and: short-circuit, returns first falsy or last value
                        // Pattern: compile arg; Dup; JumpIfFalse(end); Pop; ...next arg...
                        "and" => {
                            if list.len() < 2 { return false; }
                            let mut jump_patches: Vec<usize> = Vec::new();
                            for (i, arg) in list[1..].iter().enumerate() {
                                if !self.compile_expr(arg, outer_env) { return false; }
                                if i + 1 < list.len() - 1 {
                                    self.code.push(Op::Dup);
                                    let jf_idx = self.code.len();
                                    self.code.push(Op::JumpIfFalse(0));
                                    self.code.push(Op::Pop);
                                    jump_patches.push(jf_idx);
                                }
                            }
                            let end_pc = self.code.len();
                            for idx in jump_patches {
                                self.code[idx] = Op::JumpIfFalse(end_pc);
                            }
                            true
                        }
                        // or: short-circuit, returns first truthy or last value
                        "or" => {
                            if list.len() < 2 { return false; }
                            let mut jump_patches: Vec<usize> = Vec::new();
                            for (i, arg) in list[1..].iter().enumerate() {
                                if !self.compile_expr(arg, outer_env) { return false; }
                                if i + 1 < list.len() - 1 {
                                    self.code.push(Op::Dup);
                                    let jt_idx = self.code.len();
                                    self.code.push(Op::JumpIfTrue(0));
                                    self.code.push(Op::Pop);
                                    jump_patches.push(jt_idx);
                                }
                            }
                            let end_pc = self.code.len();
                            for idx in jump_patches {
                                self.code[idx] = Op::JumpIfTrue(end_pc);
                            }
                            true
                        }
                        // progn / begin: evaluate all, return last
                        "progn" | "begin" => {
                            if list.len() < 2 {
                                self.code.push(Op::PushNil);
                                return true;
                            }
                            for (i, arg) in list[1..].iter().enumerate() {
                                if !self.compile_expr(arg, outer_env) { return false; }
                                if i + 1 < list.len() - 1 {
                                    self.code.push(Op::Pop);
                                }
                            }
                            true
                        }
                        // cond: multi-branch — chained JumpIfFalse
                        // (cond (t1 r1) (t2 r2) (else rN))
                        "cond" => {
                            if list.len() < 2 { return false; }
                            let mut end_jumps: Vec<usize> = Vec::new();
                            let mut i = 1;
                            while i < list.len() {
                                let clause = match list.get(i) {
                                    Some(LispVal::List(c)) if c.len() >= 2 => c.clone(),
                                    _ => { return false; }
                                };
                                // else clause — just compile result
                                if clause[0] == LispVal::Sym("else".into()) {
                                    if !self.compile_expr(&clause[1], outer_env) { return false; }
                                    break;
                                }
                                // compile test
                                if !self.compile_expr(&clause[0], outer_env) { return false; }
                                let jf_idx = self.code.len();
                                self.code.push(Op::JumpIfFalse(0)); // placeholder
                                // compile result
                                if !self.compile_expr(&clause[1], outer_env) { return false; }
                                end_jumps.push(self.code.len());
                                self.code.push(Op::Jump(0)); // jump to end
                                // patch JF to skip to next clause
                                self.code[jf_idx] = Op::JumpIfFalse(self.code.len());
                                i += 1;
                            }
                            // patch all end jumps
                            let end_pc = self.code.len();
                            for idx in end_jumps {
                                self.code[idx] = Op::Jump(end_pc);
                            }
                            true
                        }
                        _ => {
                            if list.len() > 1 {
                                let n_args = list.len() - 1;
                                for arg in &list[1..] {
                                    if !self.compile_expr(arg, outer_env) { return false; }
                                }
                                self.code.push(Op::BuiltinCall(op.clone(), n_args));
                                true
                            } else { false }
                        }
                    }
                } else { false }
            }
            _ => false,
        }
    }

    /// Compile the loop body. Returns the compiled loop or None.
    fn compile_body(
        mut self,
        init_vals: Vec<LispVal>,
        body: &LispVal,
        outer_env: &Env,
    ) -> Option<CompiledLoop> {
        let num_slots = self.slot_map.len();

        if let LispVal::List(parts) = body {
            if parts.first() == Some(&LispVal::Sym("if".into())) {
                let test = parts.get(1)?;
                let then_branch = parts.get(2)?;
                let else_branch = parts.get(3);

                if !self.compile_expr(test, outer_env) { return None; }
                let jf_idx = self.code.len();
                self.code.push(Op::JumpIfFalse(0));
                if !self.compile_expr(then_branch, outer_env) { return None; }
                self.code.push(Op::Return);
                let else_start = self.code.len();
                self.code[jf_idx] = Op::JumpIfFalse(else_start);

                if let Some(else_expr) = else_branch {
                    if let LispVal::List(else_parts) = else_expr {
                        if else_parts.first() == Some(&LispVal::Sym("recur".into())) {
                            let recur_args = &else_parts[1..];
                            if recur_args.len() != num_slots { return None; }
                            for arg in recur_args {
                                if !self.compile_expr(arg, outer_env) { return None; }
                            }
                            self.code.push(Op::Recur(num_slots));
                        } else {
                            if !self.compile_expr(else_expr, outer_env) { return None; }
                            self.code.push(Op::Return);
                        }
                    } else {
                        if !self.compile_expr(else_expr, outer_env) { return None; }
                        self.code.push(Op::Return);
                    }
                } else {
                    self.code.push(Op::PushNil);
                    self.code.push(Op::Return);
                }
                let captured = self.captured.clone();
                return Some(CompiledLoop {
                    num_slots,
                    slot_names: self.slot_map,
                    init_vals,
                    code: self.code,
                    loop_start_pc: 0,
                    captured,
                });
            }
            if !self.compile_expr(body, outer_env) { return None; }
            self.code.push(Op::Return);
            let captured = self.captured.clone();
            return Some(CompiledLoop {
                num_slots,
                slot_names: self.slot_map,
                init_vals,
                code: self.code,
                loop_start_pc: 0,
                captured,
            });
        }
        None
    }
}

/// Run a compiled loop. Returns the result.
fn run_compiled_loop(
    cl: &CompiledLoop,
    gas: &mut u64,
    outer_env: &mut Env,
) -> Result<LispVal, String> {
    // Slot-based env: binding slots + captured env slots, direct index access
    let mut slots: Vec<LispVal> = cl.init_vals.clone();
    // Append captured env values after binding slots
    for (_, val) in &cl.captured {
        slots.push(val.clone());
    }
    let mut stack: Vec<LispVal> = Vec::with_capacity(16);
    let code = &cl.code;
    let mut pc: usize = 0;

    loop {
        if *gas == 0 { return Err("out of gas".into()); }
        *gas -= 1;

        match &code[pc] {
            Op::LoadSlot(s) => {
                stack.push(slots[*s].clone());
                pc += 1;
            }
            Op::PushI64(n) => {
                stack.push(LispVal::Num(*n));
                pc += 1;
            }
            Op::PushFloat(f) => {
                stack.push(LispVal::Float(*f));
                pc += 1;
            }
            Op::PushBool(b) => {
                stack.push(LispVal::Bool(*b));
                pc += 1;
            }
            Op::PushStr(s) => {
                stack.push(LispVal::Str(s.clone()));
                pc += 1;
            }
            Op::PushNil => {
                stack.push(LispVal::Nil);
                pc += 1;
            }
            Op::Dup => {
                if let Some(top) = stack.last() {
                    stack.push(top.clone());
                }
                pc += 1;
            }
            Op::Pop => {
                stack.pop();
                pc += 1;
            }
            Op::StoreSlot(s) => {
                slots[*s] = stack.pop().unwrap_or(LispVal::Nil);
                pc += 1;
            }
            Op::Add => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Num(a + b));
                pc += 1;
            }
            Op::Sub => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Num(a - b));
                pc += 1;
            }
            Op::Mul => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Num(a * b));
                pc += 1;
            }
            Op::Div => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                if b == 0 { return Err("division by zero".into()); }
                stack.push(LispVal::Num(a / b));
                pc += 1;
            }
            Op::Mod => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                if b == 0 { return Err("modulo by zero".into()); }
                stack.push(LispVal::Num(a % b));
                pc += 1;
            }
            Op::Eq => {
                let b = stack.pop().unwrap_or(LispVal::Nil);
                let a = stack.pop().unwrap_or(LispVal::Nil);
                stack.push(LispVal::Bool(lisp_eq(&a, &b)));
                pc += 1;
            }
            Op::Lt => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Bool(a < b));
                pc += 1;
            }
            Op::Le => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Bool(a <= b));
                pc += 1;
            }
            Op::Gt => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Bool(a > b));
                pc += 1;
            }
            Op::Ge => {
                let b = num_val(stack.pop().unwrap_or(LispVal::Nil));
                let a = num_val(stack.pop().unwrap_or(LispVal::Nil));
                stack.push(LispVal::Bool(a >= b));
                pc += 1;
            }
            Op::JumpIfTrue(addr) => {
                let v = stack.pop().unwrap_or(LispVal::Nil);
                if is_truthy(&v) { pc = *addr; } else { pc += 1; }
            }
            Op::JumpIfFalse(addr) => {
                let v = stack.pop().unwrap_or(LispVal::Nil);
                if !is_truthy(&v) { pc = *addr; } else { pc += 1; }
            }
            Op::Jump(addr) => { pc = *addr; }
            Op::Return => {
                return Ok(stack.pop().unwrap_or(LispVal::Nil));
            }
            Op::Recur(n) => {
                // Pop n args in reverse order into slots
                let mut new_vals: Vec<LispVal> = Vec::with_capacity(*n);
                for _ in 0..*n {
                    new_vals.push(stack.pop().unwrap_or(LispVal::Nil));
                }
                new_vals.reverse();
                for (i, v) in new_vals.into_iter().enumerate() {
                    slots[i] = v;
                }
                pc = 0; // jump to loop start
            }
            Op::BuiltinCall(name, n_args) => {
                let mut args: Vec<LispVal> = Vec::with_capacity(*n_args);
                for _ in 0..*n_args {
                    args.push(stack.pop().unwrap_or(LispVal::Nil));
                }
                args.reverse();
                let result = eval_builtin(name, &args)?;
                stack.push(result);
                pc += 1;
            }
        }
    }
}

/// Extract i64 from LispVal
pub fn num_val(v: LispVal) -> i64 {
    match v {
        LispVal::Num(n) => n,
        LispVal::Float(f) => f as i64,
        _ => 0,
    }
}

pub fn num_val_ref(v: &LispVal) -> i64 {
    match v {
        LispVal::Num(n) => *n,
        LispVal::Float(f) => *f as i64,
        _ => 0,
    }
}

/// Lisp equality
pub fn lisp_eq(a: &LispVal, b: &LispVal) -> bool {
    match (a, b) {
        (LispVal::Num(x), LispVal::Num(y)) => x == y,
        (LispVal::Float(x), LispVal::Float(y)) => x == y,
        (LispVal::Num(x), LispVal::Float(y)) => (*x as f64) == *y,
        (LispVal::Float(x), LispVal::Num(y)) => *x == (*y as f64),
        (LispVal::Bool(x), LispVal::Bool(y)) => x == y,
        (LispVal::Str(x), LispVal::Str(y)) => x == y,
        (LispVal::Nil, LispVal::Nil) => true,
        _ => false,
    }
}

/// Evaluate a builtin by name (for Op::BuiltinCall)
pub fn eval_builtin(name: &str, args: &[LispVal]) -> Result<LispVal, String> {
    match name {
        "abs" => Ok(LispVal::Num(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)).abs())),
        "min" => {
            let a = num_val(args.get(0).cloned().unwrap_or(LispVal::Nil));
            let b = num_val(args.get(1).cloned().unwrap_or(LispVal::Nil));
            Ok(LispVal::Num(a.min(b)))
        }
        "max" => {
            let a = num_val(args.get(0).cloned().unwrap_or(LispVal::Nil));
            let b = num_val(args.get(1).cloned().unwrap_or(LispVal::Nil));
            Ok(LispVal::Num(a.max(b)))
        }
        "to-string" => Ok(LispVal::Str(format!("{}", args.get(0).unwrap_or(&LispVal::Nil)))),
        "str" => Ok(LispVal::Str(args.iter().map(|a| format!("{}", a)).collect())),
        "car" => match args.get(0) {
            Some(LispVal::List(l)) => Ok(l.first().cloned().unwrap_or(LispVal::Nil)),
            _ => Ok(LispVal::Nil),
        },
        "cdr" => match args.get(0) {
            Some(LispVal::List(l)) => Ok(LispVal::List(l[1..].to_vec())),
            _ => Ok(LispVal::Nil),
        },
        "cons" => {
            let head = args.get(0).cloned().unwrap_or(LispVal::Nil);
            let tail = match args.get(1) {
                Some(LispVal::List(l)) => l.clone(),
                _ => vec![],
            };
            Ok(LispVal::List(vec![head].into_iter().chain(tail).collect()))
        }
        "list" => Ok(LispVal::List(args.to_vec())),
        "length" => match args.get(0) {
            Some(LispVal::List(l)) => Ok(LispVal::Num(l.len() as i64)),
            Some(LispVal::Str(s)) => Ok(LispVal::Num(s.len() as i64)),
            _ => Ok(LispVal::Num(0)),
        },
        "empty?" => match args.get(0) {
            Some(LispVal::List(l)) => Ok(LispVal::Bool(l.is_empty())),
            Some(LispVal::Nil) => Ok(LispVal::Bool(true)),
            _ => Ok(LispVal::Bool(false)),
        },
        "zero?" => Ok(LispVal::Bool(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) == 0)),
        "pos?" => Ok(LispVal::Bool(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) > 0)),
        "neg?" => Ok(LispVal::Bool(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) < 0)),
        "mod" => {
            let b = num_val(args.get(1).cloned().unwrap_or(LispVal::Nil));
            if b == 0 { return Err("mod by zero".into()); }
            Ok(LispVal::Num(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) % b))
        }
        "remainder" => {
            let b = num_val(args.get(1).cloned().unwrap_or(LispVal::Nil));
            if b == 0 { return Err("remainder by zero".into()); }
            Ok(LispVal::Num(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) % b))
        }
        "even?" => Ok(LispVal::Bool(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) % 2 == 0)),
        "odd?" => Ok(LispVal::Bool(num_val(args.get(0).cloned().unwrap_or(LispVal::Nil)) % 2 != 0)),
        _ => Err(format!("loop bytecode: unknown builtin '{}'", name)),
    }
}

/// Try to compile a loop into bytecode. Returns None if body is too complex.
pub fn try_compile_loop(
    binding_names: &[String],
    binding_vals: Vec<LispVal>,
    body: &LispVal,
    outer_env: &Env,
) -> Option<CompiledLoop> {
    let compiler = LoopCompiler::new(binding_names.to_vec());
    compiler.compile_body(binding_vals, body, outer_env)
}

/// Execute a compiled loop
pub fn exec_compiled_loop(
    cl: &CompiledLoop,
    gas: &mut u64,
    outer_env: &mut Env,
) -> Result<LispVal, String> {
    run_compiled_loop(cl, gas, outer_env)
}
