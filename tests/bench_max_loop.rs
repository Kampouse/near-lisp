use near_workspaces::network::Sandbox;
use near_workspaces::Worker;

#[tokio::test]
async fn find_max_loop() -> anyhow::Result<()> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let wasm = std::fs::read("target/near/near_lisp/near_lisp.wasm")?;
    let contract = worker.dev_deploy(&wasm).await?;
    contract
        .call("new")
        .args_json(serde_json::json!({ "eval_gas_limit": 10_000_000 }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    println!("Testing loop limits with eval_gas_limit=10M, max_gas attached\n");
    println!(
        "{:<20} {:<12} {:>15} {:>10} {}",
        "iterations", "result", "gas_burnt", "Tgas", "status"
    );
    println!("{}", "-".repeat(75));

    let counts: Vec<u64> = vec![
        1000, 5000, 10000, 20000, 50000, 80000, 100000, 120000, 150000, 200000,
    ];

    for n in counts {
        let code = format!("(loop (i 0) (if (= i {}) i (recur (+ i 1))))", n);
        let res = contract
            .call("eval")
            .args_json(serde_json::json!({ "code": code }))
            .max_gas()
            .transact()
            .await;

        match res {
            Ok(outcome) => {
                let gas = outcome.total_gas_burnt;
                let gas_u64 = gas.as_gas();
                let tgas = gas_u64 as f64 / 1e12;
                let status = if outcome.clone().is_failure() {
                    "FAILED"
                } else {
                    "OK"
                };
                let result: String = outcome.json().unwrap_or_else(|e| format!("ERR:{}", e));
                println!(
                    "{:<20} {:<12} {:>15} {:.1}  {}",
                    n, result, gas_u64, tgas, status
                );
            }
            Err(e) => {
                println!(
                    "{:<20} {:<12} {:>15} {:>10}  {}",
                    n,
                    "TX_FAILED",
                    0,
                    0,
                    e.to_string().chars().take(60).collect::<String>()
                );
            }
        }
    }

    // Also test: what's the max gas a function call can use?
    println!("\n--- Raw gas probe (100K loop with different attached gas) ---");
    // Just use max_gas for all — the iteration count IS the probe

    Ok(())
}
