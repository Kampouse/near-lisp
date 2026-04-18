/// Sandbox gas profiling for near-lisp batch ccall optimization.
///
/// Measures exact gas burns for each receipt in the yield/ccall chain
/// to find tight constants for auto_resume_gas, reserve_gas, and ccall_gas.
///
/// Build WASM first:
///   rustup override set 1.86.0
///   cargo near build non-reproducible-wasm --no-abi
///   rustup override unset
///
/// Run:
///   cargo test --test bench_gas_sandbox -- --nocapture
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

/// Profile sync eval to see baseline overhead
#[tokio::test]
async fn profile_sync_eval() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    let tests = vec![
        ("(+ 1 2)", "simple add"),
        ("(+ 1 2 3 4 5)", "5-arg add"),
        ("(define x 42) (+ x 1)", "define + add"),
        ("(str-concat \"hello\" \" \" \"world\")", "str-concat"),
    ];

    for (code, label) in tests {
        let res = c
            .call("eval")
            .args_json(serde_json::json!({ "code": code }))
            .max_gas()
            .transact()
            .await?;

        println!("\n=== Sync: {} ===", label);
        for (i, outcome) in res.outcomes().iter().enumerate() {
            let gas = tgas(outcome.gas_burnt);
            let status = if outcome.is_success() { "OK" } else { "FAIL" };
            println!("  R{}: {} | {:.2} Tgas", i, status, gas);
            for log in &outcome.logs {
                println!("    LOG: {}", log);
            }
        }
    }

    Ok(())
}

/// Profile single ccall at various gas levels
#[tokio::test]
async fn profile_single_ccall() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    let code = r#"(define owner (near/ccall "near.testnet" "get_owner" "{}"))
(str-concat "ok=" owner)"#;

    for gas_tgas in [50u64, 60, 70, 80, 90, 100] {
        let res = c
            .call("eval_async")
            .args_json(serde_json::json!({ "code": code }))
            .gas(near_workspaces::types::Gas::from_tgas(gas_tgas))
            .transact()
            .await?;

        println!("\n=== 1 ccall @ {} Tgas ===", gas_tgas);
        for (i, outcome) in res.outcomes().iter().enumerate() {
            let gas = tgas(outcome.gas_burnt);
            let status = if outcome.is_success() { "OK" } else { "FAIL" };
            println!("  R{}: {} | {:.2} Tgas", i, status, gas);
            for log in &outcome.logs {
                println!("    LOG: {}", log);
            }
        }
    }

    Ok(())
}

/// Profile 2 batched ccalls at various gas levels
#[tokio::test]
async fn profile_two_ccalls() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    let code = r#"(define a (near/ccall "near.testnet" "get_owner" "{}"))
(define b (near/ccall "near.testnet" "get_gas_limit" "{}"))
(+ (len (list a b)) 0)"#;

    for gas_tgas in [100u64, 120, 140, 150, 160, 180, 200] {
        let res = c
            .call("eval_async")
            .args_json(serde_json::json!({ "code": code }))
            .gas(near_workspaces::types::Gas::from_tgas(gas_tgas))
            .transact()
            .await?;

        println!("\n=== 2 ccalls @ {} Tgas ===", gas_tgas);
        for (i, outcome) in res.outcomes().iter().enumerate() {
            let gas = tgas(outcome.gas_burnt);
            let status = if outcome.is_success() { "OK" } else { "FAIL" };
            println!("  R{}: {} | {:.2} Tgas", i, status, gas);
            for log in &outcome.logs {
                println!("    LOG: {}", log);
            }
        }
    }

    Ok(())
}

/// Profile 3 batched ccalls
#[tokio::test]
async fn profile_three_ccalls() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;

    let code = r#"(near/ccall "near.testnet" "get_owner" "{}")
(near/ccall "near.testnet" "get_gas_limit" "{}")
(near/ccall "near.testnet" "get_owner" "{}")
(near/ccall-count)"#;

    for gas_tgas in [180u64, 200, 210, 230, 250] {
        let res = c
            .call("eval_async")
            .args_json(serde_json::json!({ "code": code }))
            .gas(near_workspaces::types::Gas::from_tgas(gas_tgas))
            .transact()
            .await?;

        println!("\n=== 3 ccalls @ {} Tgas ===", gas_tgas);
        for (i, outcome) in res.outcomes().iter().enumerate() {
            let gas = tgas(outcome.gas_burnt);
            let status = if outcome.is_success() { "OK" } else { "FAIL" };
            println!("  R{}: {} | {:.2} Tgas", i, status, gas);
            for log in &outcome.logs {
                println!("    LOG: {}", log);
            }
        }
    }

    Ok(())
}
