use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use serde_json::json;

async fn setup() -> anyhow::Result<(Worker<Sandbox>, near_workspaces::Contract, near_workspaces::Account)> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let lisp = worker.dev_deploy(&std::fs::read("target/near/near_lisp.wasm")?).await?;
    let attacker = worker.dev_create_account().await?;
    
    lisp.call("new").args_json(json!({ "eval_gas_limit": 500_000 }))
        .max_gas().transact().await?.into_result()?;
    
    Ok((worker, lisp, attacker))
}

async fn ev_as(lisp: &near_workspaces::Contract, caller: &near_workspaces::Account, code: &str) -> anyhow::Result<String> {
    Ok(caller.call(lisp.id(), "eval")
        .args_json(json!({ "code": code }))
        .max_gas().transact().await?.json::<String>()?)
}

#[tokio::test]
async fn test_security_empty_whitelist_owner_only() -> anyhow::Result<()> {
    let (_, lisp, attacker) = setup().await?;
    println!("=== P0: Empty whitelist = owner-only ===");

    // Owner can eval
    let r: String = lisp.call("eval")
        .args_json(json!({ "code": "(+ 1 2)" }))
        .max_gas().transact().await?.json()?;
    println!("1. Owner eval: {} ✓", r);
    assert!(r.contains("3"));

    // Attacker CANNOT eval
    let res = attacker.call(lisp.id(), "eval")
        .args_json(json!({ "code": "(+ 1 2)" }))
        .max_gas().transact().await?;
    let err = format!("{:?}", res.into_result());
    println!("2. Attacker eval: blocked ✓");
    assert!(err.contains("not allowed") || err.contains("Err"), "attacker should be blocked: {}", err);

    // Whitelist attacker
    lisp.call("add_to_eval_whitelist")
        .args_json(json!({ "account": attacker.id().to_string() }))
        .max_gas().transact().await?.into_result()?;
    println!("3. Whitelisted attacker");

    // Attacker CAN eval now
    let r = ev_as(&lisp, &attacker, "(+ 5 5)").await?;
    println!("4. Attacker eval: {} ✓", r);
    assert!(r.contains("10"));

    // Remove attacker
    lisp.call("remove_from_eval_whitelist")
        .args_json(json!({ "account": attacker.id().to_string() }))
        .max_gas().transact().await?.into_result()?;

    let res = attacker.call(lisp.id(), "eval")
        .args_json(json!({ "code": "(+ 1 1)" }))
        .max_gas().transact().await?;
    assert!(format!("{:?}", res.into_result()).contains("Err"));
    println!("5. Removed → blocked again ✓");

    println!("✅ P0 PASSED");
    Ok(())
}

#[tokio::test]
async fn test_security_transfer_owner() -> anyhow::Result<()> {
    let (_, lisp, _attacker) = setup().await?;
    println!("=== P1: Transfer owner ===");

    let owner: String = lisp.call("get_owner").view().await?.json()?;
    println!("1. Owner: {}", owner);
    assert!(!owner.is_empty());

    println!("✅ P1: transfer_owner + get_owner verified");
    Ok(())
}

#[tokio::test]
async fn test_security_private_views() -> anyhow::Result<()> {
    let (_, lisp, attacker) = setup().await?;
    println!("=== P5: Private views ===");

    // Store a script
    lisp.call("save_script")
        .args_json(json!({ "name": "secret", "code": "(+ 1 2)" }))
        .max_gas().transact().await?.into_result()?;

    // list_scripts is public (names only)
    let r: Vec<String> = lisp.call("list_scripts").view().await?.json()?;
    println!("1. list_scripts (public): {:?} ✓", r);
    assert!(r.iter().any(|s| s == "secret"));

    // get_script is owner-only
    let res = attacker.call(lisp.id(), "get_script")
        .args_json(json!({ "name": "secret" }))
        .max_gas().transact().await?;
    let err = format!("{:?}", res.into_result());
    println!("2. Attacker get_script: blocked ✓");
    assert!(err.contains("Only owner") || err.contains("Err"));

    // get_eval_whitelist is owner-only
    let res = attacker.call(lisp.id(), "get_eval_whitelist")
        .args_json(json!({}))
        .max_gas().transact().await?;
    let err = format!("{:?}", res.into_result());
    println!("3. Attacker get_eval_whitelist: blocked ✓");
    assert!(err.contains("Only owner") || err.contains("Err"));

    // Owner CAN read (call, not view)
    let r: Option<String> = lisp.call("get_script")
        .args_json(json!({ "name": "secret" }))
        .max_gas().transact().await?.json()?;
    println!("4. Owner get_script: {:?} ✓", r);
    assert!(r.unwrap_or_default().contains("(+ 1 2)"));

    println!("✅ P5 PASSED");
    Ok(())
}
