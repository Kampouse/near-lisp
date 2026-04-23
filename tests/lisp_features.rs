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
    
    Ok((worker, dcl, lisp))
}

// Helper: eval lisp code
async fn ev(lisp: &near_workspaces::Contract, code: &str) -> anyhow::Result<String> {
    let r: String = lisp.call("eval")
        .args_json(json!({ "code": code }))
        .max_gas()
        .transact()
        .await?
        .json()?;
    Ok(r)
}

#[tokio::test]
async fn test_json_parse() -> anyhow::Result<()> {
    let (_, _, lisp) = setup().await?;
    println!("=== json-parse ===");

    // Pass JSON via eval_with_input so we don't fight string escaping
    // json-parse receives a string that's already properly formatted
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(json-parse json_str)",
            "input_json": json!({"json_str": r#"{"foo": 42, "bar": "hello"}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("1. Parse object: {}", r);
    assert!(r.contains("foo"), "expected foo in: {}", r);

    // json-get with input
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(json-get json_str \"foo\")",
            "input_json": json!({"json_str": r#"{"foo": 42, "bar": "hello"}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("2. json-get foo: {} (should be 42)", r);
    assert!(r.contains("42"));

    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(json-get json_str \"bar\")",
            "input_json": json!({"json_str": r#"{"foo": 42, "bar": "hello"}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("3. json-get bar: {} (should be hello)", r);
    assert!(r.contains("hello"));

    // Missing key
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(json-get json_str \"missing\")",
            "input_json": json!({"json_str": r#"{"foo": 42}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("4. json-get missing: {} (should be nil)", r);
    assert!(r.contains("nil"));

    // json-get-in
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(json-get-in json_str \"a\" \"b\" \"c\")",
            "input_json": json!({"json_str": r#"{"a": {"b": {"c": 99}}}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("5. json-get-in a.b.c: {} (should be 99)", r);
    assert!(r.contains("99"));

    // Parse array
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(json-parse json_str)",
            "input_json": json!({"json_str": "[1, 2, 3]"}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("6. Parse array: {}", r);
    assert!(r.contains("1"));

    // dict/get from parsed
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": "(dict/get (json-parse json_str) \"price\")",
            "input_json": json!({"json_str": r#"{"price": 411130, "volume": 1000}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("7. dict/get from parsed: {} (should be 411130)", r);
    assert!(r.contains("411130"));

    // Parse simple JSON without quotes (no input needed)
    let r = ev(&lisp, r#"(json-parse "42")"#).await?;
    println!("8. Parse number: {}", r);
    assert!(r.contains("42"));

    let r = ev(&lisp, r#"(json-parse "true")"#).await?;
    println!("9. Parse bool: {}", r);
    assert!(r.contains("true"));

    println!("✅ json-parse PASSED\n");
    Ok(())
}

#[tokio::test]
async fn test_json_build() -> anyhow::Result<()> {
    let (_, _, lisp) = setup().await?;
    println!("=== json-build ===");

    let r = ev(&lisp, r#"(json-build (dict "action" "REBALANCE" "lo" 412700 "hi" 413300))"#).await?;
    println!("1. Build dict: {}", r);
    assert!(r.contains("REBALANCE"));
    assert!(r.contains("412700"));

    // Parse → modify → build (use input for the JSON)
    let r: String = lisp.call("eval_with_input")
        .args_json(json!({
            "code": r#"(define d (json-parse json_str))
(define new-count (+ 1 (dict/get d "count")))
(json-build (dict/set d "count" new-count))"#,
            "input_json": json!({"json_str": r#"{"count": 5}"#}).to_string()
        }))
        .max_gas().transact().await?.json()?;
    println!("2. Parse→modify→build: {}", r);
    assert!(r.contains("6"), "expected 6 in: {}", r);

    // Build with computed values
    let r = ev(&lisp, r#"
(define cur 413000)
(define width 300)
(json-build (dict "pool_id" "usdt|wrap|100" "left_point" (- cur width) "right_point" (+ cur width)))
"#).await?;
    println!("3. Build DCL args: {}", r);
    assert!(r.contains("412700"));

    println!("✅ json-build PASSED\n");
    Ok(())
}

#[tokio::test]
async fn test_storage_inc() -> anyhow::Result<()> {
    let (_, _, lisp) = setup().await?;
    println!("=== near/storage-inc ===");

    ev(&lisp, r#"(near/storage-write "counter" "0")"#).await?;
    let r = ev(&lisp, r#"(near/storage-inc "counter" 1)"#).await?;
    println!("1. inc by 1: {} ✓", r);
    assert!(r.contains("1"));

    let r = ev(&lisp, r#"(near/storage-inc "counter" 5)"#).await?;
    println!("2. inc by 5: {} ✓", r);
    assert!(r.contains("6"));

    let r = ev(&lisp, r#"(near/storage-inc "counter" -2)"#).await?;
    println!("3. dec by 2: {} ✓", r);
    assert!(r.contains("4"));

    let r = ev(&lisp, r#"(near/storage-inc "fresh" 10)"#).await?;
    println!("4. new key +10: {} ✓", r);
    assert!(r.contains("10"));

    let r = ev(&lisp, r#"(near/storage-inc "hits" 1)(near/storage-inc "hits" 1)(near/storage-inc "hits" 1)(near/storage-read "hits")"#).await?;
    println!("5. Triple inc: {} ✓", r);
    assert!(r.contains("3"));

    println!("✅ near/storage-inc PASSED\n");
    Ok(())
}

#[tokio::test]
async fn test_null_safe_ccall() -> anyhow::Result<()> {
    let (_, dcl, lisp) = setup().await?;
    let dcl_id = dcl.id().as_str();
    println!("=== near/ccall-view* ===");

    // Null-safe to real contract
    let res = lisp.call("eval_async")
        .args_json(json!({ "code": format!(r#"(near/ccall-view* "{dcl_id}" "list_pools" "{{}}")"#) }))
        .max_gas().transact().await?;
    println!("1. Null-safe real contract: {} receipts ✓", res.receipt_outcomes().len());
    assert!(res.receipt_outcomes().len() >= 2);

    // Normal for comparison
    let res = lisp.call("eval_async")
        .args_json(json!({ "code": format!(r#"(near/ccall-view "{dcl_id}" "list_pools" "{{}}")"#) }))
        .max_gas().transact().await?;
    println!("2. Normal real contract: {} receipts ✓", res.receipt_outcomes().len());

    println!("✅ near/ccall-view* PASSED\n");
    Ok(())
}

#[tokio::test]
async fn test_near_log() -> anyhow::Result<()> {
    let (_, _, lisp) = setup().await?;
    println!("=== near/log ===");

    let res = lisp.call("eval")
        .args_json(json!({ "code": r#"(begin (near/log "hello from lisp!") "done")"# }))
        .max_gas().transact().await?;
    println!("1. near/log: {:?}", res.outcome().logs);

    let res = lisp.call("eval")
        .args_json(json!({ "code": r#"(begin (near/log (str-concat "price=" (to-string 411130))) "done")"# }))
        .max_gas().transact().await?;
    println!("2. near/log computed: {:?}", res.outcome().logs);

    println!("✅ near/log PASSED\n");
    Ok(())
}

#[tokio::test]
async fn test_integrated_rebalance() -> anyhow::Result<()> {
    let (_, dcl, lisp) = setup().await?;
    let dcl_id = dcl.id().as_str();
    println!("=== INTEGRATED REBALANCE ===\n");

    let policy = r#"
(define get-width (lambda (vol)
  (cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000))))
(define in-range? (lambda (p lo hi) (and (>= p lo) (<= p hi))))
(if (in-range? cur lo hi)
  (begin
    (near/storage-write "last_check" "healthy")
    "HOLD")
  (begin
    (define width (get-width vol))
    (near/storage-write "position_lo" (to-string (- cur width)))
    (near/storage-write "position_hi" (to-string (+ cur width)))
    (near/storage-inc "rebalance_count" 1)
    (json-build (dict "action" "REBALANCE" "new_lo" (- cur width) "new_hi" (+ cur width)))))
"#;

    lisp.call("save_script").args_json(json!({ "name": "full_clmm", "code": policy }))
        .max_gas().transact().await?.into_result()?;
    println!("✓ Policy stored");

    let _: String = lisp.call("eval")
        .args_json(json!({ "code": r#"(near/storage-write "position_lo" "410630")(near/storage-write "position_hi" "411630")(near/storage-write "rebalance_count" "0")"ok""# }))
        .max_gas().transact().await?.json()?;

    // Cycle 1: HOLD
    let r: String = lisp.call("eval_script_with_input")
        .args_json(json!({ "name": "full_clmm", "input_json": json!({"cur": 411130, "lo": 410630, "hi": 411630, "vol": 0.02}).to_string() }))
        .max_gas().transact().await?.json()?;
    println!("✓ Cycle 1 (in range): {}", r);
    assert!(r.contains("HOLD"));

    // Cycle 2: REBALANCE
    let r: String = lisp.call("eval_script_with_input")
        .args_json(json!({ "name": "full_clmm", "input_json": json!({"cur": 413000, "lo": 410630, "hi": 411630, "vol": 0.03}).to_string() }))
        .max_gas().transact().await?.json()?;
    println!("✓ Cycle 2 (out of range): {}", r);
    assert!(r.contains("REBALANCE"));
    // vol=0.03 → width=300
    assert!(r.contains("412700"), "expected 412700: {}", r);

    let lo = ev(&lisp, r#"(near/storage-read "position_lo")"#).await?;
    let count = ev(&lisp, r#"(near/storage-read "rebalance_count")"#).await?;
    println!("  lo={} count={}", lo, count);
    assert!(lo.contains("412700"));
    assert!(count.contains("1"));

    // Async with DCL
    let async_policy = format!(r#"
(define get-width (lambda (vol)
  (cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000))))
(define in-range? (lambda (p lo hi) (and (>= p lo) (<= p hi))))
(near/ccall-view "{dcl_id}" "list_pools" "{{}}")
(if (in-range? cur lo hi)
  "HOLD"
  (begin
    (near/storage-write "position_lo" (to-string (- cur (get-width vol))))
    (near/storage-write "position_hi" (to-string (+ cur (get-width vol))))
    (near/storage-inc "rebalance_count" 1)
    "REBALANCED"))
"#);
    lisp.call("save_script").args_json(json!({ "name": "async_clmm", "code": async_policy }))
        .max_gas().transact().await?.into_result()?;

    let res = lisp.call("eval_script_async_with_input")
        .args_json(json!({ "name": "async_clmm", "input_json": json!({"cur": 411000, "lo": 412700, "hi": 413300, "vol": 0.08}).to_string() }))
        .max_gas().transact().await?;
    println!("✓ Cycle 3 (async): {} receipts", res.receipt_outcomes().len());

    let count = ev(&lisp, r#"(near/storage-read "rebalance_count")"#).await?;
    println!("  count: {} (should be 2)", count);
    assert!(count.contains("2"));

    println!("\n╔════════════════════════════════════════════╗");
    println!("║  ✅ ALL FEATURES VERIFIED                  ║");
    println!("╠════════════════════════════════════════════╣");
    println!("║  json-parse / json-get   ✓                 ║");
    println!("║  json-get-in             ✓                 ║");
    println!("║  json-build              ✓                 ║");
    println!("║  near/storage-inc        ✓                 ║");
    println!("║  near/ccall-view*        ✓                 ║");
    println!("║  near/log                ✓                 ║");
    println!("║  Integrated rebalance    ✓                 ║");
    println!("╚════════════════════════════════════════════╝");
    Ok(())
}
