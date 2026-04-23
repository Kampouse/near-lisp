use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use serde_json::json;

async fn setup() -> anyhow::Result<(Worker<Sandbox>, near_workspaces::Contract, near_workspaces::Contract)> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let dcl = worker.dev_deploy(&std::fs::read("/tmp/dcl_v2.wasm")?).await?;
    let lisp = worker.dev_deploy(&std::fs::read("target/near/near_lisp.wasm")?).await?;
    
    dcl.call("new").args_json(json!({
        "owner_id": dcl.id(), "wnear_id": "wrap.near",
        "farming_contract_id": dcl.id(), "exchange_fee": 10,
        "referral_fee": 5, "referrer_id": dcl.id()
    })).max_gas().transact().await?.into_result()?;
    
    lisp.call("new").args_json(json!({ "eval_gas_limit": 500_000 }))
        .max_gas().transact().await?.into_result()?;
    
    let root_acc = worker.root_account()?;
    root_acc.transfer_near(lisp.id(), near_workspaces::types::NearToken::from_near(100)).await?;
    
    Ok((worker, dcl, lisp))
}

#[tokio::test]
async fn test_full_rebalance_cycle() -> anyhow::Result<()> {
    let (_worker, dcl, lisp) = setup().await?;
    let dcl_id = dcl.id().as_str();
    let lisp_id = lisp.id().as_str();
    
    println!("DCL={}, Lisp={}\n", dcl_id, lisp_id);
    
    // ── INIT STORAGE ──
    let r: String = lisp.call("eval")
        .args_json(json!({ "code": r#"
(near/storage-write "lo" "410630")
(near/storage-write "hi" "411630")
(near/storage-write "count" "0")
"init""# }))
        .max_gas().transact().await?.json()?;
    println!("Init: {}", r);
    
    // Verify sync storage works
    let r: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "lo")"# }))
        .max_gas().transact().await?.json()?;
    println!("lo = {}", r);
    assert!(r.contains("410630"), "expected 410630, got {}", r);
    
    // ── STORE POLICY ──
    // Policy: no ccall, just storage ops (sync) to isolate the issue
    let sync_policy = r#"
(define in-range? (lambda (p lo hi) (and (>= p lo) (<= p hi))))
(if (in-range? cur lo hi)
  (begin
    (near/storage-write "last" "healthy")
    "HOLD")
  (begin
    (near/storage-write "lo" (to-string (- cur 300)))
    (near/storage-write "hi" (to-string (+ cur 300)))
    (near/storage-write "count" (to-string (+ 1 (to-num (near/storage-read "count")))))
    "REBALANCED"))
"#;
    lisp.call("save_script").args_json(json!({ "name": "sync_test", "code": sync_policy }))
        .max_gas().transact().await?.into_result()?;
    println!("\nSync policy stored");
    
    // Test sync: in range
    let r: String = lisp.call("eval_script_with_input")
        .args_json(json!({ "name": "sync_test", "input_json": json!({"cur": 411130, "lo": 410630, "hi": 411630}).to_string() }))
        .max_gas().transact().await?.json()?;
    println!("Sync HOLD: {}", r);
    let r: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "count")"# }))
        .max_gas().transact().await?.json()?;
    println!("count after HOLD = {} (should be 0)", r);
    
    // Test sync: out of range  
    let r: String = lisp.call("eval_script_with_input")
        .args_json(json!({ "name": "sync_test", "input_json": json!({"cur": 413000, "lo": 410630, "hi": 411630}).to_string() }))
        .max_gas().transact().await?.json()?;
    println!("Sync REBALANCE: {}", r);
    let lo: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "lo")"# }))
        .max_gas().transact().await?.json()?;
    let hi: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "hi")"# }))
        .max_gas().transact().await?.json()?;
    let count: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "count")"# }))
        .max_gas().transact().await?.json()?;
    println!("lo={} hi={} count={} (expect 412700/413300/1)", lo, hi, count);
    
    // ── NOW TEST WITH ASYNC + CCALL ──
    let async_policy = format!(r#"
(define in-range? (lambda (p lo hi) (and (>= p lo) (<= p hi))))
(define pool (near/ccall-view "{dcl_id}" "list_pools" "{{}}"))
(if (in-range? cur lo hi)
  (begin
    (near/storage-write "last" "healthy")
    "HOLD")
  (begin
    (near/storage-write "lo" (to-string (- cur 300)))
    (near/storage-write "hi" (to-string (+ cur 300)))
    (near/storage-write "count" (to-string (+ 1 (to-num (near/storage-read "count")))))
    "REBALANCED"))
"#);
    lisp.call("save_script").args_json(json!({ "name": "async_test", "code": async_policy }))
        .max_gas().transact().await?.into_result()?;
    println!("\nAsync policy stored");
    
    // Reset
    lisp.call("eval").args_json(json!({ "code": r#"(near/storage-write "lo" "410630")(near/storage-write "hi" "411630")"ok""# }))
        .max_gas().transact().await?;
    
    // Async: out of range
    println!("\n--- ASYNC REBALANCE ---");
    let res = lisp.call("eval_script_async_with_input")
        .args_json(json!({ "name": "async_test", "input_json": json!({"cur": 413000, "lo": 410630, "hi": 411630}).to_string() }))
        .max_gas().transact().await?;
    
    println!("Receipts: {}", res.receipt_outcomes().len());
    for (i, ro) in res.receipt_outcomes().iter().enumerate() {
        println!("  R{}: gas={} logs={:?}", i, ro.gas_burnt, ro.logs);
    }
    
    // Check storage AFTER the full transact completes
    let lo: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "lo")"# }))
        .max_gas().transact().await?.json()?;
    let hi: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "hi")"# }))
        .max_gas().transact().await?.json()?;
    let count: String = lisp.call("eval").args_json(json!({ "code": r#"(near/storage-read "count")"# }))
        .max_gas().transact().await?.json()?;
    println!("\nAfter async: lo={} hi={} count={} (expect 412700/413300/2)", lo, hi, count);
    
    println!("\n✅ DONE");
    Ok(())
}
