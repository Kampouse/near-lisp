use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use serde_json::json;

async fn setup() -> anyhow::Result<(Worker<Sandbox>, near_workspaces::Contract)> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let lisp = worker.dev_deploy(&std::fs::read("target/near/near_lisp.wasm")?).await?;
    lisp.call("new").args_json(json!({ "eval_gas_limit": 500_000 }))
        .max_gas().transact().await?.into_result()?;
    Ok((worker, lisp))
}

async fn ev(lisp: &near_workspaces::Contract, code: &str) -> anyhow::Result<String> {
    Ok(lisp.call("eval").args_json(json!({ "code": code }))
        .max_gas().transact().await?.json::<String>()?)
}

async fn ev_input(lisp: &near_workspaces::Contract, code: &str, input: serde_json::Value) -> anyhow::Result<String> {
    Ok(lisp.call("eval_with_input")
        .args_json(json!({ "code": code, "input_json": input.to_string() }))
        .max_gas().transact().await?.json::<String>()?)
}

fn jq(json: &str) -> String {
    // Escape JSON for embedding inside a Lisp string literal
    json.replace('\\', "\\\\").replace('"', "\\\"")
}

macro_rules! check {
    ($val:expr, $needle:expr) => {
        let v = &$val;
        assert!(v.contains($needle), "expected '{}' in: {}", $needle, v);
    };
}

// ==================== PARSING EDGE CASES ====================

#[tokio::test]
async fn test_parse_unicode_escaped() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Parse: Unicode & Escaped Strings ===");

    // Unicode via eval_with_input (avoids escaping hell)
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "\"日本語\""})).await?;
    println!("1. Unicode (日本語): {}", r);
    check!(r, "日本語");

    let r = ev_input(&lisp, "(from-json s)", json!({"s": "\"🎉🚀\""})).await?;
    println!("2. Emoji (🎉🚀): {}", r);
    check!(r, "🎉");

    // Escaped strings via input
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "\"hello\\nworld\""})).await?;
    println!("3. Newline in string: {}", r);
    check!(r, "hello");

    let r = ev_input(&lisp, "(from-json s)", json!({"s": "\"quotes\\\"inside\""})).await?;
    println!("4. Quotes inside: {}", r);
    check!(r, "quotes");

    println!("✅ Unicode/Escaped parsing PASSED");
    Ok(())
}

#[tokio::test]
async fn test_parse_extreme_numbers() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Parse: Extreme Numbers ===");

    // Very large integer
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "99999999999999999"})).await?;
    println!("1. Large int: {}", r);
    check!(r, "9999999999");

    // Very small decimal
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "0.0000000001"})).await?;
    println!("2. Tiny decimal: {}", r);

    // Scientific notation
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "1.5e10"})).await?;
    println!("3. Scientific (1.5e10): {}", r);
    check!(r, "15000000000");

    // Negative zero
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "-0"})).await?;
    println!("4. Negative zero: {}", r);
    // -0 or 0 both acceptable
    assert!(r == "0" || r == "-0" || r.contains("0"), "negative zero should be 0-ish: {}", r);

    // Max safe integer
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "9007199254740991"})).await?;
    println!("5. MAX_SAFE_INTEGER: {}", r);
    check!(r, "9007199");

    println!("✅ Extreme numbers PASSED");
    Ok(())
}

#[tokio::test]
async fn test_parse_deep_nesting() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Parse: Deep Nesting ===");

    // 15-level nested JSON via input
    let mut val = serde_json::json!(42);
    for i in (1..=15).rev() {
        val = serde_json::json!({ format!("level{}", i): val });
    }
    let r = ev_input(&lisp, "(dict/get (from-json s) \"level1\")", json!({"s": val.to_string()})).await?;
    println!("1. 15-deep nesting → level1: {}", &r[..r.len().min(80)]);
    check!(r, "level2");

    // Deep array nesting
    let mut arr = serde_json::json!(42);
    for _ in 0..10 {
        arr = serde_json::json!([arr]);
    }
    let r = ev_input(&lisp, "(from-json s)", json!({"s": arr.to_string()})).await?;
    println!("2. 10-deep array: {}", &r[..r.len().min(80)]);
    check!(r, "42");

    println!("✅ Deep nesting PASSED");
    Ok(())
}

#[tokio::test]
async fn test_parse_duplicates_whitespace_special() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Parse: Duplicates, Whitespace, Special Floats ===");

    // Duplicate keys — last wins (serde default)
    let r = ev_input(&lisp, "(dict/get (from-json s) \"a\")", json!({"s": "{\"a\":1,\"a\":2}"})).await?;
    println!("1. Duplicate key 'a': {} (expect 2)", r);
    check!(r, "2");

    // Whitespace variations
    let r = ev_input(&lisp, "(dict/get (from-json s) \"x\")", json!({"s": "  {  \"x\"  :  42  }  "})).await?;
    println!("2. Extra whitespace: {}", r);
    check!(r, "42");

    // NaN — serde returns error
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "NaN"})).await?;
    println!("3. NaN: {}", &r[..r.len().min(60)]);
    assert!(r.contains("ERROR") || r.contains("expected"), "NaN should error");

    // Infinity
    let r = ev_input(&lisp, "(from-json s)", json!({"s": "Infinity"})).await?;
    println!("4. Infinity: {}", &r[..r.len().min(60)]);
    assert!(r.contains("ERROR") || r.contains("expected"), "Infinity should error");

    println!("✅ Duplicates/Whitespace/Special PASSED");
    Ok(())
}

// ==================== GETTING EDGE CASES ====================

#[tokio::test]
async fn test_get_numeric_index_special_keys() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Get: Numeric Index, Special Keys ===");

    // nth works on arrays from JSON (nth index list)
    let r = ev_input(&lisp, "(nth 1 (from-json s))", json!({"s": "[10,20,30]"})).await?;
    println!("1. nth 1 of [10,20,30]: {} (expect 20)", r);
    check!(r, "20");

    // Key with special characters (hyphen)
    let r = ev_input(&lisp, "(dict/get (from-json s) \"my-key\")", json!({"s": "{\"my-key\":42}"})).await?;
    println!("2. Key 'my-key': {}", r);
    check!(r, "42");

    // Key that looks like a number (string key in JSON)
    let r = ev_input(&lisp, "(dict/get (from-json s) \"123\")", json!({"s": "{\"123\":\"value\"}"})).await?;
    println!("3. Numeric key '123': {}", r);
    check!(r, "value");

    // Empty key
    let r = ev_input(&lisp, "(dict/get (from-json s) \"\")", json!({"s": "{\"\":42}"})).await?;
    println!("4. Empty key: {}", r);
    check!(r, "42");

    // Key with dot (json-get-in would try nested)
    let r = ev_input(&lisp, "(dict/get (from-json s) \"a.b\")", json!({"s": "{\"a.b\":99}"})).await?;
    println!("5. Key 'a.b': {}", r);
    check!(r, "99");

    println!("✅ Numeric Index/Special Keys PASSED");
    Ok(())
}

// ==================== BUILDING EDGE CASES ====================

#[tokio::test]
async fn test_build_symbols_lambdas_large() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Build: Symbols, Lambdas, Large Dicts ===");

    // Symbol — to-json on a bare symbol
    let r = ev(&lisp, "(to-json (quote hello))").await?;
    println!("1. Symbol 'hello: {}", r);
    // Should be some string representation or error

    // Lambda → to-json (lambda is (fn ...) or lambda)
    let r = ev(&lisp, "(to-json (lambda (x) (+ x 1)))").await?;
    println!("2. Lambda: {}", r);
    // Whatever it returns, shouldn't crash

    // Large dict (12 keys)
    let code = r#"(to-json (dict "k1" 1 "k2" 2 "k3" 3 "k4" 4 "k5" 5 "k6" 6 "k7" 7 "k8" 8 "k9" 9 "k10" 10 "k11" 11 "k12" 12))"#;
    let r = ev(&lisp, code).await?;
    println!("3. 12-key dict length: {}", r.len());
    check!(r, "k1");
    check!(r, "k12");

    // Special chars in string values (quotes embedded via input)
    let r = ev_input(&lisp, "(to-json (dict \"msg\" s))", json!({"s": "hello\"world"})).await?;
    println!("4. Quotes in value: {}", r);
    check!(r, "hello");

    println!("✅ Symbols/Lambdas/Large PASSED");
    Ok(())
}

// ==================== ROUNDTRIP EDGE CASES ====================

#[tokio::test]
async fn test_roundtrip_precision_order() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Roundtrip: Precision, Order, Large ===");

    // Float precision: 0.1 + 0.2
    let r = ev(&lisp, "(to-json (dict \"v\" (+ 0.1 0.2)))").await?;
    println!("1. 0.1+0.2: {}", r);
    check!(r, "0.3");

    // String roundtrip via input
    let r = ev_input(&lisp, "(to-json (from-json s))", json!({"s": "{\"msg\":\"hello\"}"})).await?;
    println!("2. String roundtrip: {}", r);
    check!(r, "msg");
    check!(r, "hello");

    // Key order preserved (dict uses BTreeMap = sorted)
    let r = ev_input(&lisp, "(to-json (from-json s))", json!({"s": "{\"z\":1,\"a\":2,\"m\":3}"})).await?;
    println!("3. Key order: {}", r);
    let a_pos = r.find("\"a\"").unwrap_or(999);
    let m_pos = r.find("\"m\"").unwrap_or(999);
    let z_pos = r.find("\"z\"").unwrap_or(999);
    assert!(a_pos < m_pos && m_pos < z_pos, "Keys should be sorted: a < m < z, got: {}", r);

    // Large structure roundtrip (20 keys)
    let large: std::collections::BTreeMap<String, i32> = (1..=20).map(|i| (format!("k{}", i), i)).collect();
    let r = ev_input(&lisp, "(to-json (from-json s))", json!({"s": serde_json::to_string(&large)?})).await?;
    println!("4. 20-key roundtrip length: {}", r.len());
    check!(r, "k1");
    check!(r, "k20");

    println!("✅ Roundtrip Precision/Order PASSED");
    Ok(())
}

// ==================== DCL-SPECIFIC EDGE CASES ====================

#[tokio::test]
async fn test_dcl_position_user_assets() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== DCL: Position, User Assets, Limit Orders ===");

    // Position data parsing via input
    let pos = json!({
        "lpt_id": "123",
        "owner": "kampouse.near",
        "pool_id": 100,
        "token_ids": ["usdt.tether-token.near", "wrap.near"],
        "lower_tick": 412000,
        "upper_tick": 414000,
        "current_point": 413000,
        "liquidity": "5000000000"
    });
    let r = ev_input(&lisp, "(dict/get (from-json s) \"lpt_id\")", json!({"s": pos.to_string()})).await?;
    println!("1. Position lpt_id: {}", r);
    check!(r, "123");

    let r = ev_input(&lisp, "(dict/get (from-json s) \"lower_tick\")", json!({"s": pos.to_string()})).await?;
    println!("2. Position lower_tick: {}", r);
    check!(r, "412000");

    let r = ev_input(&lisp, "(dict/get (from-json s) \"pool_id\")", json!({"s": pos.to_string()})).await?;
    println!("3. Position pool_id: {}", r);
    check!(r, "100");

    // User assets parsing
    let assets = json!({
        "account_id": "kampouse.near",
        "assets": [
            {"token_id": "usdt.tether-token.near", "balance": "1000000000"},
            {"token_id": "wrap.near", "balance": "5000000000000000000"}
        ]
    });
    let r = ev_input(&lisp, "(dict/get (from-json s) \"account_id\")", json!({"s": assets.to_string()})).await?;
    println!("4. User assets account: {}", r);
    check!(r, "kampouse.near");

    // Limit order parsing
    let order = json!({
        "order_id": 456,
        "status": "PENDING",
        "token_in": "usdt.tether-token.near",
        "token_out": "wrap.near",
        "amount_in": "2000000",
        "amount_out": "0"
    });
    let r = ev_input(&lisp, "(dict/get (from-json s) \"status\")", json!({"s": order.to_string()})).await?;
    println!("5. Limit order status: {}", r);
    check!(r, "PENDING");

    let r = ev_input(&lisp, "(dict/get (from-json s) \"order_id\")", json!({"s": order.to_string()})).await?;
    println!("6. Limit order_id: {}", r);
    check!(r, "456");

    // Real DCL pool response format
    let pool = json!({
        "pool_id": "usdt.tether-token.near|wrap.near|100",
        "token_ids": ["usdt.tether-token.near", "wrap.near"],
        "fee_rate": 3000,
        "current_point": 413500,
        "liquidity": "1234567890",
        "tick_spacing": 1
    });
    let r = ev_input(&lisp, "(dict/get (from-json s) \"fee_rate\")", json!({"s": pool.to_string()})).await?;
    println!("7. DCL fee_rate: {}", r);
    check!(r, "3000");

    let r = ev_input(&lisp, "(dict/get (from-json s) \"current_point\")", json!({"s": pool.to_string()})).await?;
    println!("8. DCL current_point: {}", r);
    check!(r, "413500");

    println!("✅ DCL Position/User Assets/Limit Orders PASSED");
    Ok(())
}
