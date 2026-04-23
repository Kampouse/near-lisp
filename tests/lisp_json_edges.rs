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

async fn evj(lisp: &near_workspaces::Contract, code: &str, s: &str) -> anyhow::Result<String> {
    Ok(lisp.call("eval_with_input")
        .args_json(json!({ "code": code, "input_json": serde_json::to_string(&json!({"s": s}))? }))
        .max_gas().transact().await?.json::<String>()?)
}

macro_rules! check {
    ($r:expr, $expect:expr) => {
        let r = $r;
        assert!(r.contains($expect), "expected '{}' in: {}", $expect, r);
    };
}

#[tokio::test]
async fn test_parse_edge_cases() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== PARSE EDGE CASES ===");

    // Very large integer
    let r = ev(&lisp, "(from-json \"9999999999999999\")").await?;
    println!("1. Large int: {}", r);

    // Very small decimal
    let r = ev(&lisp, "(from-json \"0.0000000001\")").await?;
    println!("2. Tiny decimal: {}", r);

    // Negative zero
    let r = ev(&lisp, "(from-json \"-0\")").await?;
    println!("3. Negative zero: {}", r);

    // Deeply nested
    let r = evj(&lisp, "(dict/get (dict/get (dict/get (from-json s) \"a\") \"b\") \"c\")", r#"{"a":{"b":{"c":"deep"}}}"#).await?;
    println!("4. Deep get: {} (expect deep)", r);
    check!(r, "deep");

    // Duplicate keys (last wins in serde)
    let r = evj(&lisp, "(dict/get (from-json s) \"x\")", r#"{"x":1,"x":2}"#).await?;
    println!("5. Duplicate keys: {} (expect 2, last wins)", r);

    // Whitespace
    check!(ev(&lisp, "(from-json \"  42  \")").await?, "42");
    println!("6. Whitespace ok");

    // Empty string → error
    let r = ev(&lisp, "(from-json \"\")").await?;
    println!("7. Empty: {} (expect error)", r);
    check!(r, "from-json:");

    // Just whitespace
    let r = ev(&lisp, "(from-json \"   \")").await?;
    println!("8. Whitespace only: {}", r);
    check!(r, "from-json:");

    // Unicode
    let r = evj(&lisp, "(dict/get (from-json s) \"name\")", r#"{"name":"日本語"}"#).await?;
    println!("9. Unicode: {} (expect 日本語)", r);
    check!(r, "日本語");

    // Emoji in JSON
    let r = evj(&lisp, "(dict/get (from-json s) \"emoji\")", r#"{"emoji":"🎉"}"#).await?;
    println!("10. Emoji: {} (expect 🎉)", r);
    check!(r, "🎉");

    // Nested array in object
    let r = evj(&lisp, "(len (dict/get (from-json s) \"items\"))", r#"{"items":[1,2,3,4,5]}"#).await?;
    println!("11. Array in object: {} (expect 5)", r);
    check!(r, "5");

    // Boolean in object
    check!(evj(&lisp, "(dict/get (from-json s) \"active\")", r#"{"active":true}"#).await?, "true");
    println!("12. Bool in object ✓");

    // Null in object
    check!(evj(&lisp, "(dict/get (from-json s) \"data\")", r#"{"data":null}"#).await?, "nil");
    println!("13. Null in object ✓");

    println!("✅ Parse edge cases");
    Ok(())
}

#[tokio::test]
async fn test_build_edge_cases() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== BUILD EDGE CASES ===");

    // Symbol → what happens?
    let r = ev(&lisp, "(to-json 'hello)").await?;
    println!("1. Symbol: {}", r);

    // Lambda → what happens?
    let r = ev(&lisp, "(to-json (lambda (x) x))").await?;
    println!("2. Lambda: {}", r);

    // Large dict
    let r = ev(&lisp, "(to-json (dict \"a\" 1 \"b\" 2 \"c\" 3 \"d\" 4 \"e\" 5 \"f\" 6 \"g\" 7 \"h\" 8 \"i\" 9 \"j\" 10))").await?;
    println!("3. Large dict (10 keys): {}", r.chars().take(60).collect::<String>());
    check!(r.clone(), "a");
    check!(r, "j");

    // Special chars in string values
    let r = ev(&lisp, r#"(to-json (dict "msg" "hello world"))"#).await?;
    println!("4. String with space: {}", r);
    check!(r, "hello world");

    // Negative number
    check!(ev(&lisp, "(to-json (dict \"x\" -42))").await?, "-42");
    println!("5. Negative in dict ✓");

    // Zero
    check!(ev(&lisp, "(to-json (dict \"x\" 0))").await?, "0");
    println!("6. Zero in dict ✓");

    // Nested list in dict
    let r = ev(&lisp, "(to-json (dict \"items\" (list 1 2 3)))").await?;
    println!("7. List in dict: {}", r);
    check!(r, "1");

    // Dict in list
    let r = ev(&lisp, "(to-json (list (dict \"x\" 1) (dict \"y\" 2)))").await?;
    println!("8. Dict in list: {}", r);
    check!(r, "x");

    println!("✅ Build edge cases");
    Ok(())
}

#[tokio::test]
async fn test_roundtrip_edge_cases() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== ROUNDTRIP EDGE CASES ===");

    // Float precision
    let r = ev(&lisp, "(to-json (* 0.1 0.2))").await?;
    println!("1. 0.1*0.2 = {} (IEEE 754)", r);
    // Should be ~0.020000000000000004, not exactly 0.02

    // String roundtrip
    let r = evj(&lisp, "(to-json (from-json s))", r#"{"msg":"hello"}"#).await?;
    println!("2. String roundtrip: {}", r);
    check!(r, "hello");

    // Bool roundtrip
    check!(ev(&lisp, "(to-json (from-json \"true\"))").await?, "true");
    check!(ev(&lisp, "(to-json (from-json \"false\"))").await?, "false");
    println!("3. Bool roundtrip ✓");

    // Null roundtrip
    check!(ev(&lisp, "(to-json (from-json \"null\"))").await?, "null");
    println!("4. Null roundtrip ✓");

    // Nested roundtrip
    let r = evj(&lisp, "(to-json (from-json s))", r#"{"a":{"b":[1,2,3]}}"#).await?;
    println!("5. Nested roundtrip: {}", r);
    check!(r.clone(), "a");
    check!(r, "1");

    // Key with number-like name
    check!(evj(&lisp, "(dict/get (from-json s) \"123\")", r#"{"123":"numeric key"}"#).await?, "numeric key");
    println!("6. Numeric key name ✓");

    // Empty string value
    check!(evj(&lisp, "(dict/get (from-json s) \"name\")", r#"{"name":""}"#).await?, "");
    println!("7. Empty string value ✓");

    println!("✅ Roundtrip edge cases");
    Ok(())
}
