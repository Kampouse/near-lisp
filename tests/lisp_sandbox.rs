use near_workspaces::network::Sandbox;
use near_workspaces::Worker;

/// Sandbox tests — run locally, fast, deterministic.
///
/// Build WASM first:
///   rustup override set 1.86.0
///   rustup target add wasm32-unknown-unknown --toolchain 1.86.0
///   cargo near build non-reproducible-wasm --no-abi
///   rustup override unset
///
/// Then: cargo test --test lisp_sandbox -- --nocapture

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

#[tokio::test]
async fn test_sandbox_arithmetic() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(+ 1 2)" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "3");
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(* 6 7)" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "42");
    println!("✓ arithmetic");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_lambda_and_closure() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(define square (lambda (n) (* n n))) (square 9)" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "81");
    let r: String = c.call("eval").args_json(serde_json::json!({ "code": "(define make-adder (lambda (n) (lambda (x) (+ n x)))) (define add10 (make-adder 10)) (add10 32)" })).max_gas().transact().await?.json()?;
    assert_eq!(r, "42");
    println!("✓ lambda + closure");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_fibonacci() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(100_000).await?;
    let r: String = c.call("eval").args_json(serde_json::json!({ "code": "(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 10)" })).max_gas().transact().await?.json()?;
    assert_eq!(r, "55");
    println!("✓ fib(10)=55");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_policy() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    let pass: bool = c
        .call("check_policy")
        .args_json(serde_json::json!({
            "policy": "(and (>= score 85) (<= duration 3600))",
            "input_json": "{\"score\": 90, \"duration\": 1200}"
        }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(pass);
    let fail: bool = c
        .call("check_policy")
        .args_json(serde_json::json!({
            "policy": "(and (>= score 85) (<= duration 3600))",
            "input_json": "{\"score\": 70, \"duration\": 1200}"
        }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(!fail);
    println!("✓ policy check");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_save_eval_policy() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    c.call("save_policy")
        .args_json(serde_json::json!({
            "name": "quality-gate",
            "policy": "(and (>= score 80) (str-contains status \"complete\"))"
        }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;

    let r: String = c
        .call("eval_policy")
        .args_json(serde_json::json!({
            "name": "quality-gate",
            "input_json": "{\"score\": 92, \"status\": \"complete\"}"
        }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "true");

    let r: String = c
        .call("eval_policy")
        .args_json(serde_json::json!({
            "name": "quality-gate",
            "input_json": "{\"score\": 60, \"status\": \"complete\"}"
        }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "false");
    println!("✓ save + eval policy");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_near_builtins() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(near/block-height)" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(r.parse::<i64>().is_ok());
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(near/timestamp)" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert!(r.parse::<i64>().is_ok());
    println!("✓ NEAR builtins");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_gas_exhaustion() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50).await?;
    let r: String = c.call("eval").args_json(serde_json::json!({ "code": "(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 20)" })).max_gas().transact().await?.json()?;
    assert!(r.starts_with("ERROR:") && r.contains("gas"));
    println!("✓ gas exhaustion");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_list_ops() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(car (list 1 2 3))" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "1");
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(cdr (list 1 2 3))" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "(2 3)");
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(append (list 1 2) (list 3 4))" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "(1 2 3 4)");
    println!("✓ list ops");
    Ok(())
}

#[tokio::test]
async fn test_sandbox_storage() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp(50_000).await?;
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(near/storage-write \"k\" \"v\")" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "true");
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(near/storage-read \"k\")" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "\"v\"");
    let r: String = c
        .call("eval")
        .args_json(serde_json::json!({ "code": "(near/storage-read \"missing\")" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    assert_eq!(r, "nil");
    println!("✓ storage");
    Ok(())
}
