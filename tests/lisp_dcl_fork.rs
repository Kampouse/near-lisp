use near_workspaces::Worker;
use serde_json::json;

async fn deploy_lisp_forked() -> anyhow::Result<(Worker<near_workspaces::network::Mainnet>, near_workspaces::Contract)> {
    let worker: Worker<near_workspaces::network::Mainnet> = near_workspaces::mainnet().await?;
    let wasm = std::fs::read("target/near/near_lisp.wasm")?;
    let contract = worker.dev_deploy(&wasm).await?;
    contract
        .call("new")
        .args_json(json!({ "eval_gas_limit": 300_000 }))
        .max_gas()
        .transact()
        .await?
        .into_result()?;
    println!("✓ near-lisp deployed");
    Ok((worker, contract))
}

#[tokio::test]
async fn test_dcl_basic_eval() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp_forked().await?;
    let r: String = c.call("eval")
        .args_json(json!({ "code": "(+ 1 2)" }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    println!("✓ eval (+ 1 2) = {}", r);
    assert_eq!(r, "3");
    Ok(())
}

#[tokio::test]
async fn test_dcl_ccall_view_pool() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp_forked().await?;
    let code = r#"(define pool (near/ccall-view "dclv2.ref-labs.near" "get_pool" "{\"pool_id\":\"usdt.tether-token.near|wrap.near|100\"}"))"#;
    let r: String = c.call("eval_async")
        .args_json(json!({ "code": code }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    println!("✓ ccall-view get_pool: {}", &r[..r.len().min(500)]);
    Ok(())
}

#[tokio::test]
async fn test_dcl_parse_pool() -> anyhow::Result<()> {
    let (_, c) = deploy_lisp_forked().await?;
    let code = r#"
(define raw (near/ccall-view "dclv2.ref-labs.near" "get_pool" "{\"pool_id\":\"usdt.tether-token.near|wrap.near|100\"}"))
(define parsed (dict/from-json raw))
(dict/get parsed "current_point")
"#;
    let r: String = c.call("eval_async")
        .args_json(json!({ "code": code }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    println!("✓ current_point: {}", r);
    Ok(())
}
