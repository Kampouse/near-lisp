use near_lisp::Env;
fn bench(code: &str, gas: u64) -> Result<String, String> {
    let mut env = Env::new();
    near_lisp::run_program(code, &mut env, gas)
}

fn main() {
    // Binary search for max iterations at various gas budgets
    println!("=== simple counting loop (1 binding) ===");
    for gas in [10_000, 100_000, 1_000_000, 10_000_000, 100_000_000] {
        let mut lo: u64 = 0;
        let mut hi: u64 = gas;
        while lo < hi {
            let mid = lo + (hi - lo + 1) / 2;
            let code = format!("(loop ((i 0)) (if (>= i {}) i (recur (+ i 1))))", mid);
            match bench(&code, gas) {
                Ok(r) if !r.contains("out of gas") && !r.contains("ERROR") => lo = mid,
                _ => hi = mid - 1,
            }
        }
        println!("gas={:<12} max_iters={}", gas, lo);
    }

    // Fibonacci-ish with 2 bindings
    println!("\n=== fibonacci-style loop (2 bindings) ===");
    for gas in [10_000, 100_000, 1_000_000, 10_000_000, 100_000_000] {
        let mut lo: u64 = 0;
        let mut hi: u64 = gas;
        while lo < hi {
            let mid = lo + (hi - lo + 1) / 2;
            let code = format!(
                "(loop ((a 0) (b 1)) (if (>= a {}) a (recur b (+ a b))))",
                mid
            );
            match bench(&code, gas) {
                Ok(r) if !r.contains("out of gas") && !r.contains("ERROR") => lo = mid,
                _ => hi = mid - 1,
            }
        }
        println!("gas={:<12} max_iters={}", gas, lo);
    }

    // Per-iteration gas cost
    println!("\n=== per-iteration gas (binary search for min gas) ===");
    for iters in [100, 1000, 10000, 100000] {
        let code = format!("(loop ((i 0)) (if (>= i {}) i (recur (+ i 1))))", iters);
        let mut lo_gas: u64 = 1;
        let mut hi_gas: u64 = iters * 100;
        while lo_gas < hi_gas {
            let mid = lo_gas + (hi_gas - lo_gas) / 2;
            match bench(&code, mid) {
                Ok(r) if !r.contains("out of gas") && !r.contains("ERROR") => hi_gas = mid,
                _ => lo_gas = mid + 1,
            }
        }
        let overhead = lo_gas as f64 / iters as f64;
        println!(
            "iters={:<8} min_gas={:<8} per_iter={:.3}  formula_overhead={:.0}",
            iters,
            lo_gas,
            overhead,
            lo_gas as f64 - (iters as f64 * overhead)
        );
    }
}
