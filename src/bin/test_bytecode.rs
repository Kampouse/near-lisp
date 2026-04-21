
//! Bytecode correctness tests + benchmark
use near_lisp::{lisp_eval, parse_all, Env, LispVal};

fn eval(code: &str) -> Result<LispVal, String> {
    let exprs = parse_all(code).map_err(|e| format!("parse error: {:?}", e))?;
    if exprs.len() != 1 {
        return Err(format!("expected 1 expr, got {}", exprs.len()));
    }
    let mut env = Env::new();
    let mut gas = 100_000_000_000_000u64;
    lisp_eval(&exprs[0], &mut env, &mut gas)
}

fn eval_with_gas(code: &str, gas_limit: u64) -> (Result<LispVal, String>, u64) {
    let exprs = match parse_all(code) {
        Ok(e) => e,
        Err(e) => return (Err(format!("parse error: {:?}", e)), 0),
    };
    if exprs.len() != 1 {
        return (Err("expected 1 expr".into()), 0);
    }
    let mut env = Env::new();
    let mut gas = gas_limit;
    let result = lisp_eval(&exprs[0], &mut env, &mut gas);
    (result, gas_limit - gas)
}

fn check(label: &str, code: &str, expected: i64) {
    match eval(code) {
        Ok(LispVal::Num(n)) if n == expected => println!("  PASS {} = {}", label, expected),
        Ok(LispVal::Num(n)) => { eprintln!("  FAIL {} got {} expected {}", label, n, expected); std::process::exit(1); }
        Ok(other) => { eprintln!("  FAIL {} got {:?} expected {}", label, other, expected); std::process::exit(1); }
        Err(e) => { eprintln!("  FAIL {} error: {}", label, e); std::process::exit(1); }
    }
}

fn main() {
    println!("=== CORRECTNESS ===\n");
    check("(>= i 10) sum 0..9", "(loop ((i 0) (sum 0)) (if (>= i 10) sum (recur (+ i 1) (+ sum i))))", 45);
    check("(> i 9) sum 0..9", "(loop ((i 0) (sum 0)) (if (> i 9) sum (recur (+ i 1) (+ sum i))))", 45);
    check("(> i 0) from i=0", "(loop ((i 0) (sum 0)) (if (> i 0) sum (recur (+ i 1) (+ sum i))))", 0);
    check("step=2", "(loop ((i 0) (sum 0)) (if (>= i 10) sum (recur (+ i 2) (+ sum i))))", 20);
    check("step=5", "(loop ((i 0) (sum 0)) (if (>= i 10) sum (recur (+ i 5) (+ sum i))))", 5);
    check("step=10", "(loop ((i 0) (sum 0)) (if (>= i 100) sum (recur (+ i 10) (+ sum i))))", 450);
    check("(> i 999)", "(loop ((i 0) (sum 0)) (if (> i 999) sum (recur (+ i 1) (+ sum i))))", 499500);
    check("(>= i 1000)", "(loop ((i 0) (sum 0)) (if (>= i 1000) sum (recur (+ i 1) (+ sum i))))", 499500);
    check("(>= i 10) from i=10", "(loop ((i 10) (sum 0)) (if (>= i 10) sum (recur (+ i 1) (+ sum i))))", 0);
    check("(> i 9) from i=10", "(loop ((i 10) (sum 0)) (if (> i 9) sum (recur (+ i 1) (+ sum i))))", 0);
    check("product 1..10", "(loop ((i 1) (prod 1)) (if (>= i 11) prod (recur (+ i 1) (* prod i))))", 3628800);
    check("count 100", "(loop ((i 0) (cnt 0)) (if (>= i 100) cnt (recur (+ i 1) (+ cnt 1))))", 100);
    check("generic sum 5", "(loop ((i 0) (sum 0)) (if (>= i 5) sum (recur (+ i 1) (+ sum 1))))", 5);
    check("from i=-5 to 5", "(loop ((i -5) (sum 0)) (if (>= i 5) sum (recur (+ i 1) (+ sum i))))", -5);
    check("zero iters", "(loop ((i 100) (sum 0)) (if (>= i 100) sum (recur (+ i 1) (+ sum i))))", 0);
    check("1 iter", "(loop ((i 0) (sum 0)) (if (>= i 1) sum (recur (+ i 1) (+ sum i))))", 0);
    check("2 iters", "(loop ((i 0) (sum 0)) (if (>= i 2) sum (recur (+ i 1) (+ sum i))))", 1);
    check("> 1 iter", "(loop ((i 0) (sum 0)) (if (> i 0) sum (recur (+ i 1) (+ sum i))))", 0);
    check("> 2 iters", "(loop ((i 0) (sum 0)) (if (> i 1) sum (recur (+ i 1) (+ sum i))))", 1);
    check("sum squares 0..4", "(loop ((i 0) (sum 0)) (if (>= i 5) sum (recur (+ i 1) (+ sum (* i i)))))", 30);
    check("sum i*2", "(loop ((i 0) (sum 0)) (if (>= i 5) sum (recur (+ i 1) (+ sum (* i 2)))))", 20);
    check("1-bind count", "(loop ((i 0)) (if (>= i 10) i (recur (+ i 1))))", 10);
    println!("\n  All 22 tests passed.\n");

    println!("=== PER-ITERATION GAS (internal gas accounting) ===\n");
    for n in [100, 1_000, 10_000, 100_000] {
        let code = format!("(loop ((i 0) (sum 0)) (if (>= i {}) sum (recur (+ i 1) (+ sum i))))", n);
        let (_, used) = eval_with_gas(&code, 10_000_000_000);
        println!("  >= sum {:>8} iters: {:>10} gas  {:.1} gas/iter", n, used, used as f64 / n as f64);
    }
    println!();
    for n in [100, 1_000, 10_000, 100_000] {
        let code = format!("(loop ((i 0) (sum 0)) (if (> i {}) sum (recur (+ i 1) (+ sum i))))", n);
        let (_, used) = eval_with_gas(&code, 10_000_000_000);
        println!("  > sum  {:>8} iters: {:>10} gas  {:.1} gas/iter", n, used, used as f64 / n as f64);
    }
    println!();
    for n in [100, 1_000, 10_000, 100_000] {
        let code = format!("(loop ((i 0)) (if (>= i {}) i (recur (+ i 1))))", n);
        let (_, used) = eval_with_gas(&code, 10_000_000_000);
        println!("  >= 1b  {:>8} iters: {:>10} gas  {:.1} gas/iter", n, used, used as f64 / n as f64);
    }
    println!("\n  --- Step variants (100K target) ---");
    for step in [1, 2, 5, 10] {
        let code = format!("(loop ((i 0) (sum 0)) (if (>= i 100000) sum (recur (+ i {}) (+ sum i))))", step);
        let (_, used) = eval_with_gas(&code, 10_000_000_000);
        let iters = 100000 / step;
        println!("  step={:<3} {:>6} iters: {:>10} gas  {:.1} gas/iter", step, iters, used, used as f64 / iters as f64);
    }
    println!("\n  --- Generic (no mega-fuse): (+ sum 1) ---");
    for n in [100, 1_000, 10_000] {
        let code = format!("(loop ((i 0) (sum 0)) (if (>= i {}) sum (recur (+ i 1) (+ sum 1))))", n);
        let (_, used) = eval_with_gas(&code, 10_000_000_000);
        println!("  generic {:>8} iters: {:>10} gas  {:.1} gas/iter", n, used, used as f64 / n as f64);
    }

    println!("\n=== BENCHMARK COMPLETE ===");
}
