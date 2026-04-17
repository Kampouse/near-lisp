use near_workspaces::network::Sandbox;
use near_workspaces::Worker;

/// Gas benchmark — run in sandbox to measure computational cost.
/// cargo test --test bench_gas -- --nocapture

#[tokio::test]
async fn bench_gas() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let wasm = std::fs::read("target/near/near_lisp/near_lisp.wasm")?;

    println!("WASM size: {} bytes ({} KB)", wasm.len(), wasm.len() / 1024);

    let contract = worker.dev_deploy(&wasm).await?;
    contract
        .call("new")
        .args_json(serde_json::json!({ "eval_gas_limit": 100000 }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    let tests: Vec<(&str, &str)> = vec![
        // Original tests
        ("(+ 1 2)", "addition"),
        ("(* 6 7)", "multiplication"),
        ("(define square (lambda (n) (* n n))) (square 9)", "lambda square"),
        ("(define make-adder (lambda (n) (lambda (x) (+ n x)))) (define add5 (make-adder 5)) (add5 37)", "closure"),
        ("(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 10)", "fib(10)-recursive"),
        ("(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 15)", "fib(15)-recursive"),
        ("(near/block-height)", "block-height"),
        ("(list 1 2 3 4 5 6 7 8 9 10)", "list-10"),
        // NEW: loop/recur tests
        ("(loop (a 0 b 1 cnt 10) (if (= cnt 0) a (recur b (+ a b) (- cnt 1))))", "fib(10)-loop"),
        ("(loop (a 0 b 1 cnt 50) (if (= cnt 0) a (recur b (+ a b) (- cnt 1))))", "fib(50)-loop"),
        ("(loop (i 1 sum 0) (if (> i 1000) sum (recur (+ i 1) (+ sum i))))", "sum(1..1000)"),
        ("(loop (n 20 acc 1) (if (= n 0) acc (recur (- n 1) (* acc n))))", "factorial(20)"),
        ("(loop (i 0) (if (= i 5000) i (recur (+ i 1))))", "count-5000"),
    ];

    println!(
        "\n{:<30} {:<15} {:>15} {:>10}",
        "operation", "result", "gas_burnt", "Tgas"
    );
    println!("{}", "-".repeat(75));

    for (code, label) in tests {
        let outcome = contract
            .call("eval")
            .args_json(serde_json::json!({ "code": code }))
            .max_gas()
            .transact()
            .await?;

        let gas = outcome.total_gas_burnt;
        let gas_u64 = gas.as_gas();
        let tgas = gas_u64 as f64 / 1e12;
        let result: String = outcome.json().unwrap_or_else(|e| format!("ERR:{}", e));
        println!("{:<30} {:<15} {:>15} {:.1}", label, result, gas_u64, tgas);
    }

    println!("\n--- Comparison ---");
    println!("Recursive fib(10) vs loop fib(10): same O(n) but recursive uses closures");
    println!("Loop fib(50): impossible with naive recursion (O(2^n) eval steps)");
    println!("Loop count-5000: pure iteration benchmark");

    Ok(())
}
