/// Sandbox micro-benchmarks for near-lisp optimization.
///
/// Since sandbox can't process yield callbacks, these focus on:
/// 1. Sync eval overhead (parsing, check_ccall scanning)
/// 2. Storage write/read costs
/// 3. Borsh serialization costs
///
/// Build WASM first:
///   rustup override set 1.86.0
///   cargo near build non-reproducible-wasm --no-abi
///   rustup override unset
///
/// Run:
///   cargo test --test bench_micro -- --nocapture
use near_workspaces::network::Sandbox;
use near_workspaces::Worker;

async fn deploy_lisp(
    gas_limit: u64,
) -> anyhow::Result<(Worker<Sandbox>, near_workspaces::Contract)> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let wasm = std::fs::read("target/near/near_lisp/near_lisp.wasm")?;
    let contract = worker.dev_deploy(&wasm).await?;
    contract
        .call("new")
        .args_json(serde_json::json!({ "eval_gas_limit": gas_limit }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    Ok((worker, contract))
}

fn tgas(gas: near_workspaces::types::Gas) -> f64 {
    gas.as_gas() as f64 / 1_000_000_000_000.0
}

#[tokio::test]
async fn bench_sync_eval_scaling() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    // Baseline: just parsing overhead
    let tests = vec![
        ("(+ 1 2)", "1 expr"),
        ("(+ 1 2)\n(+ 3 4)", "2 exprs"),
        ("(+ 1 2)\n(+ 3 4)\n(+ 5 6)", "3 exprs"),
        ("(+ 1 2)\n(+ 3 4)\n(+ 5 6)\n(+ 7 8)", "4 exprs"),
        ("(+ 1 2)\n(+ 3 4)\n(+ 5 6)\n(+ 7 8)\n(+ 9 10)", "5 exprs"),
    ];

    println!("\n=== Sync eval scaling (expressions count) ===");
    println!("{:<12} {:>10} {:>10}", "Label", "R1 Gas", "Delta");
    let mut prev_gas = 0.0f64;
    for (code, label) in &tests {
        let res = c
            .call("eval")
            .args_json(serde_json::json!({ "code": code }))
            .max_gas()
            .transact()
            .await?;
        let gas = tgas(res.outcomes()[1].gas_burnt);
        let delta = if prev_gas > 0.0 { gas - prev_gas } else { 0.0 };
        println!("{:<12} {:>9.2}T {:>+9.2}T", label, gas, delta);
        prev_gas = gas;
    }

    Ok(())
}

#[tokio::test]
async fn bench_ccall_scan_overhead() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    // Measure how much gas the ccall-scanning adds vs normal eval
    // These will all fail (sandbox has no yield), but the gas burn
    // tells us the scanning overhead.
    let tests = vec![
        ("(+ 1 2)", "sync (no ccall)"),
        ("(near/ccall \"x.testnet\" \"foo\" \"{}\")", "1 ccall scan"),
        ("(near/ccall \"x.testnet\" \"foo\" \"{}\")\n(near/ccall \"x.testnet\" \"bar\" \"{}\")", "2 ccall scans"),
        ("(near/ccall \"x.testnet\" \"foo\" \"{}\")\n(near/ccall \"x.testnet\" \"bar\" \"{}\")\n(near/ccall \"x.testnet\" \"baz\" \"{}\")", "3 ccall scans"),
        ("(near/ccall \"x.testnet\" \"foo\" \"{}\")\n(near/ccall \"x.testnet\" \"bar\" \"{}\")\n(near/ccall \"x.testnet\" \"baz\" \"{}\")\n(near/ccall \"x.testnet\" \"qux\" \"{}\")", "4 ccall scans"),
        ("(near/ccall \"x.testnet\" \"foo\" \"{}\")\n(near/ccall \"x.testnet\" \"bar\" \"{}\")\n(near/ccall \"x.testnet\" \"baz\" \"{}\")\n(near/ccall \"x.testnet\" \"qux\" \"{}\")\n(near/ccall \"x.testnet\" \"quux\" \"{}\")", "5 ccall scans"),
    ];

    println!("\n=== Ccall scan overhead ===");
    println!("{:<16} {:>10} {:>10}", "Label", "R1 Gas", "Delta");
    let mut prev_gas = 0.0f64;
    for (code, label) in &tests {
        let res = c
            .call("eval_async")
            .args_json(serde_json::json!({ "code": code }))
            .gas(near_workspaces::types::Gas::from_tgas(100))
            .transact()
            .await?;
        let gas = tgas(res.outcomes()[1].gas_burnt);
        let delta = if prev_gas > 0.0 { gas - prev_gas } else { 0.0 };
        println!("{:<16} {:>9.2}T {:>+9.2}T", label, gas, delta);
        prev_gas = gas;
    }

    // Also measure define+ccall pattern
    let define_tests = vec![
        ("(define a (near/ccall \"x.testnet\" \"foo\" \"{}\"))\n(define b (near/ccall \"x.testnet\" \"bar\" \"{}\"))", "2 define+ccall"),
        ("(define a (near/ccall \"x.testnet\" \"foo\" \"{}\"))\n(define b (near/ccall \"x.testnet\" \"bar\" \"{}\"))\n(define c (near/ccall \"x.testnet\" \"baz\" \"{}\"))", "3 define+ccall"),
    ];

    println!("\n=== Define+ccall scan overhead ===");
    for (code, label) in &define_tests {
        let res = c
            .call("eval_async")
            .args_json(serde_json::json!({ "code": code }))
            .gas(near_workspaces::types::Gas::from_tgas(200))
            .transact()
            .await?;
        let gas = tgas(res.outcomes()[1].gas_burnt);
        println!("{:<20} {:>9.2}T", label, gas);
    }

    Ok(())
}

#[tokio::test]
async fn bench_storage_costs() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    // Measure storage write costs by doing evals with increasing state size
    let tests = vec![
        ("(define x \"hello\")", "1 small def"),
        ("(define x \"hello\")\n(define y \"world\")", "2 small defs"),
        (
            "(define x (near/ccall \"x.testnet\" \"foo\" \"{}\"))",
            "1 ccall def (scan only)",
        ),
    ];

    println!("\n=== Storage/define costs ===");
    for (code, label) in &tests {
        let res = c
            .call("eval")
            .args_json(serde_json::json!({ "code": code }))
            .max_gas()
            .transact()
            .await?;
        let gas = tgas(res.outcomes()[1].gas_burnt);
        println!("{:<25} {:>9.2}T", label, gas);
    }

    Ok(())
}
