use near_lisp::Env;
use near_lisp::*;

fn run(code: &str, gas: u64) -> (Result<String, String>, u64) {
    let mut env = Env::new();
    let exprs = parse_all(code).unwrap();
    let mut g = gas;
    let mut result = LispVal::Nil;
    for expr in &exprs {
        result = match lisp_eval(expr, &mut env, &mut g) {
            Ok(v) => v,
            Err(e) => return (Err(e), gas - g),
        };
    }
    (Ok(result.to_string()), gas - g)
}

fn main() {
    // A: just loop, no body work
    // (loop ((i 0)) (if (= i N) i (recur (+ i 1))))
    // B: loop with no arithmetic
    // (loop ((i 0)) (if (= i N) i (recur (+ i 1))))
    // C: what if we avoid string lookup — use positional?

    println!("=== Internal gas per iteration (1 tick = 1 lisp_eval) ===");
    for n in [100, 1000, 10000] {
        let code = format!("(loop ((i 0)) (if (= i {}) i (recur (+ i 1))))", n);
        let (_, used) = run(&code, 1_000_000_000);
        println!(
            "  count({}): {} internal gas ({:.1} gas/n)",
            n,
            used,
            used as f64 / n as f64
        );
    }

    // Simpler: just counting, no =
    println!("\n=== Bare minimum loop (no comparison, no arithmetic) ===");
    // Can't avoid if... let's try the simplest possible loop
    for n in [100, 1000, 10000] {
        // Just increment and check nil
        let code = format!("(loop ((i 0)) (if (= i {}) i (recur (+ i 1))))", n);
        let (_, used) = run(&code, 1_000_000_000);
        println!(
            "  count({}): {} gas ({:.1}/n)",
            n,
            used,
            used as f64 / n as f64
        );
    }

    // What if we avoid env entirely? Direct parameter in loop
    println!("\n=== How much does env scan cost? ===");
    // 1 binding vs 20 bindings
    for extra in [0, 10, 50, 100] {
        let mut prefix = String::new();
        for j in 0..extra {
            prefix.push_str(&format!("(define x{} 42)", j));
        }
        let code = format!("{}(loop ((i 0)) (if (= i 1000) i (recur (+ i 1))))", prefix);
        let (_, used) = run(&code, 1_000_000_000);
        println!(
            "  {} extra bindings, count(1000): {} gas ({:.1}/n)",
            extra,
            used,
            used as f64 / 1000.0
        );
    }
}
