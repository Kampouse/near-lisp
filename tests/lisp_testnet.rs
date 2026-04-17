use near_workspaces::network::Testnet;
use near_workspaces::{Account, Worker};

/// Full lifecycle on NEAR testnet — requires a funded account.
///
/// Set environment variables:
///   export TESTNET_ACCOUNT_ID=your-account.testnet
///   export TESTNET_SECRET_KEY=ed25519:...
///
/// Build WASM first:
///   rustup override set 1.86.0
///   rustup target add wasm32-unknown-unknown --toolchain 1.86.0
///   cargo near build non-reproducible-wasm --no-abi
///   rustup override unset
///
/// Then: cargo test --test lisp_testnet -- --nocapture

fn get_testnet_creds() -> anyhow::Result<(String, String)> {
    let account_id = std::env::var("TESTNET_ACCOUNT_ID")
        .map_err(|_| anyhow::anyhow!("set TESTNET_ACCOUNT_ID env var"))?;
    let secret_key = std::env::var("TESTNET_SECRET_KEY")
        .map_err(|_| anyhow::anyhow!("set TESTNET_SECRET_KEY env var"))?;
    Ok((account_id, secret_key))
}

#[tokio::test]
async fn test_lisp_testnet_full_lifecycle() -> anyhow::Result<()> {
    let (account_id, secret_key) = get_testnet_creds()?;

    let worker: Worker<Testnet> = near_workspaces::testnet()
        .rpc_addr("https://test.rpc.fastnear.com")
        .await?;

    let root = Account::from_secret_key(
        account_id.parse()?,
        secret_key.parse()?,
        &worker,
    );

    let wasm = std::fs::read("target/near/near_lisp/near_lisp.wasm")?;
    println!("wasm size: {} bytes", wasm.len());

    // Create unique subaccount
    let uid = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis();
    let sub_id = format!("lisp{}", uid);
    let sub = root
        .create_subaccount(&sub_id)
        .initial_balance(near_workspaces::types::NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;
    println!("created: {}", sub.id());

    let contract = sub.deploy(&wasm).await?.into_result()?;
    println!("deployed: {}", contract.id());

    contract.call("new").args_json(serde_json::json!({ "eval_gas_limit": 100000 }))
        .max_gas().transact().await?.into_result()?;
    println!("initialized OK");

    // --- Arithmetic ---
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(+ 1 2)" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "3"); println!("✓ (+ 1 2) = {}", r);

    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(* 6 7)" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "42"); println!("✓ (* 6 7) = {}", r);

    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(- 10 3)" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "7"); println!("✓ (- 10 3) = {}", r);

    // --- Lambda + Closure ---
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(define square (lambda (n) (* n n))) (square 9)" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "81"); println!("✓ (square 9) = {}", r);

    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(define make-adder (lambda (n) (lambda (x) (+ n x)))) (define add10 (make-adder 10)) (add10 32)" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "42"); println!("✓ closure (add10 32) = {}", r);

    // --- Fibonacci ---
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 10)" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "55"); println!("✓ (fib 10) = {}", r);

    // --- Policy check ---
    let pass: bool = contract.call("check_policy")
        .args_json(serde_json::json!({
            "policy": "(and (>= score 85) (<= duration 3600))",
            "input_json": "{\"score\": 90, \"duration\": 1200}"
        }))
        .max_gas().transact().await?.json()?;
    assert!(pass); println!("✓ policy pass = {}", pass);

    let fail: bool = contract.call("check_policy")
        .args_json(serde_json::json!({
            "policy": "(and (>= score 85) (<= duration 3600))",
            "input_json": "{\"score\": 70, \"duration\": 1200}"
        }))
        .max_gas().transact().await?.json()?;
    assert!(!fail); println!("✓ policy fail = {}", fail);

    // --- Save + eval persistent policy ---
    contract.call("save_policy")
        .args_json(serde_json::json!({
            "name": "quality-gate",
            "policy": "(and (>= score 80) (str-contains status \"complete\"))"
        }))
        .max_gas().transact().await?.into_result()?;
    println!("✓ policy saved on-chain");

    let r: String = contract.call("eval_policy")
        .args_json(serde_json::json!({
            "name": "quality-gate",
            "input_json": "{\"score\": 92, \"status\": \"complete\"}"
        }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "true"); println!("✓ eval_policy(92) = {}", r);

    let r: String = contract.call("eval_policy")
        .args_json(serde_json::json!({
            "name": "quality-gate",
            "input_json": "{\"score\": 60, \"status\": \"complete\"}"
        }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "false"); println!("✓ eval_policy(60) = {}", r);

    // --- NEAR builtins ---
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(near/block-height)" }))
        .max_gas().transact().await?.json()?;
    assert!(r.parse::<i64>().is_ok()); println!("✓ block_height = {}", r);

    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(near/timestamp)" }))
        .max_gas().transact().await?.json()?;
    assert!(r.parse::<i64>().is_ok()); println!("✓ timestamp = {}", r);

    // --- Storage ---
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(near/storage-write \"k\" \"v\")" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "true");
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(near/storage-read \"k\")" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "\"v\""); println!("✓ storage OK");

    // --- Gas exhaustion ---
    let sub2_id = format!("gas{}", uid);
    let sub2 = root
        .create_subaccount(&sub2_id)
        .initial_balance(near_workspaces::types::NearToken::from_near(5))
        .transact()
        .await?
        .into_result()?;
    let c2 = sub2.deploy(&wasm).await?.into_result()?;
    c2.call("new").args_json(serde_json::json!({ "eval_gas_limit": 50 }))
        .max_gas().transact().await?.into_result()?;
    let r: String = c2.call("eval")
        .args_json(serde_json::json!({ "code": "(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 20)" }))
        .max_gas().transact().await?.json()?;
    assert!(r.starts_with("ERROR:") && r.contains("gas"));
    println!("✓ gas exhaustion: {}", r);

    // --- List ops ---
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(car (list 1 2 3))" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "1");
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(cdr (list 1 2 3))" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "(2 3)");
    let r: String = contract.call("eval")
        .args_json(serde_json::json!({ "code": "(append (list 1 2) (list 3 4))" }))
        .max_gas().transact().await?.json()?;
    assert_eq!(r, "(1 2 3 4)"); println!("✓ list ops OK");

    println!("\n========================================");
    println!("ALL TESTS PASSED ON NEAR TESTNET");
    println!("contract: {}", contract.id());
    println!("========================================");

    Ok(())
}
