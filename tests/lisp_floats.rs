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

macro_rules! check {
    ($r:expr, $expect:expr) => {
        let r = $r;
        assert!(r.contains($expect), "expected '{}' in: {}", $expect, r);
    };
}

#[tokio::test]
async fn test_float_arithmetic() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== FLOAT ARITHMETIC ===");

    check!(ev(&lisp, "(+ 1.5 2.5)").await?, "4");
    check!(ev(&lisp, "(- 10.5 3.2)").await?, "7.3");
    check!(ev(&lisp, "(* 3.0 2.0)").await?, "6");
    check!(ev(&lisp, "(/ 10.0 3.0)").await?, "3.33");
    check!(ev(&lisp, "(+ 1 0.5)").await?, "1.5");
    check!(ev(&lisp, "(* 100 0.03)").await?, "3");
    check!(ev(&lisp, "(* 0.03 100)").await?, "3");
    check!(ev(&lisp, "(* 2.5 4)").await?, "10");
    check!(ev(&lisp, "(- 0.1 0.05)").await?, "0.05");
    check!(ev(&lisp, "(/ 1.0 3.0)").await?, "0.33");
    println!("✅ Arithmetic\n");
    Ok(())
}

#[tokio::test]
async fn test_float_comparisons() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== FLOAT COMPARISONS ===");

    check!(ev(&lisp, "(<= 0.03 0.05)").await?, "true");
    check!(ev(&lisp, "(<= 0.03 0.01)").await?, "false");
    check!(ev(&lisp, "(> 0.03 0.01)").await?, "true");
    check!(ev(&lisp, "(= 0.03 0.03)").await?, "true");
    check!(ev(&lisp, "(< 0.01 0.03)").await?, "true");
    check!(ev(&lisp, "(>= 2 1.5)").await?, "true");
    check!(ev(&lisp, "(> 0.1 0.01)").await?, "true");
    check!(ev(&lisp, "(= 0.5 0.5)").await?, "true");
    // FIXED: (= 1 1.0) now promotes to float
    check!(ev(&lisp, "(= 1 1.0)").await?, "true");
    println!("✓ (= 1 1.0) = true (fixed!)");
    check!(ev(&lisp, "(= 1 1.5)").await?, "false");
    check!(ev(&lisp, "(!= 1 1.5)").await?, "true");
    check!(ev(&lisp, "(!= 1 1.0)").await?, "false");
    println!("✅ Comparisons\n");
    Ok(())
}

#[tokio::test]
async fn test_float_from_json() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== FLOAT FROM JSON ===");

    check!(ev_input(&lisp, "x", json!({"x": 0.03})).await?, "0.03");
    check!(ev_input(&lisp, "x", json!({"x": 1.5})).await?, "1.5");
    check!(ev_input(&lisp, "x", json!({"x": 0.001})).await?, "0.001");
    check!(ev_input(&lisp, "x", json!({"x": 99.99})).await?, "99.99");
    check!(ev_input(&lisp, "x", json!({"x": -3.14})).await?, "-3.14");
    check!(ev_input(&lisp, "x", json!({"x": 0.0001})).await?, "0.0001");
    check!(ev_input(&lisp, "x", json!({"x": 999999.99})).await?, "999999.99");

    // Int stays int
    let r = ev_input(&lisp, "x", json!({"x": 42})).await?;
    assert!(r.contains("42") && !r.contains("42.0"), "int should stay int: {}", r);

    // FIXED: large scientific notation
    check!(ev_input(&lisp, "x", json!({"x": 1e10})).await?, "10000000000");
    println!("✓ 1e10 → 10000000000 (fixed!)");

    check!(ev_input(&lisp, "x", json!({"x": 1e-6})).await?, "0.000001");

    // Zero stays int
    let r = ev_input(&lisp, "x", json!({"x": 0})).await?;
    assert!(!r.contains("0.0"), "zero should be int: {}", r);
    println!("✅ JSON input\n");
    Ok(())
}

#[tokio::test]
async fn test_float_string_ops() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== FLOAT STRING OPS ===");

    // FIXED: str-concat with float now works
    let r = ev(&lisp, r#"(str-concat "price=" (to-string 3.14))"#).await?;
    println!("1. str-concat float: {} (expect price=3.14)", r);
    check!(r, "3.14");

    let r = ev(&lisp, r#"(str-concat "vol=" (to-string 0.03))"#).await?;
    println!("2. str-concat vol: {}", r);
    check!(r, "0.03");

    // to-string
    check!(ev(&lisp, "(to-string 3.14)").await?, "3.14");
    check!(ev(&lisp, "(to-string 0.03)").await?, "0.03");
    check!(ev(&lisp, "(to-string 1.0)").await?, "1");
    println!("3. to-string 1.0 → \"1\" (strips trailing zeros)");

    // to-float from string
    check!(ev(&lisp, "(to-float \"3.14\")").await?, "3.14");
    check!(ev(&lisp, "(to-float \"0.03\")").await?, "0.03");

    // Conversions
    check!(ev(&lisp, "(to-int 3.9)").await?, "3");
    check!(ev(&lisp, "(to-int -3.9)").await?, "-3");
    check!(ev(&lisp, "(to-num 3.14)").await?, "3");
    println!("✅ String ops\n");
    Ok(())
}

#[tokio::test]
async fn test_float_cond_logic() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== FLOAT COND/LOGIC ===");

    check!(ev_input(&lisp, "(cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000))", json!({"vol": 0.03})).await?, "300");
    check!(ev_input(&lisp, "(cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000))", json!({"vol": 0.005})).await?, "100");
    check!(ev_input(&lisp, "(cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000))", json!({"vol": 0.08})).await?, "500");
    check!(ev_input(&lisp, "(cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000))", json!({"vol": 0.20})).await?, "1000");
    // Boundaries
    check!(ev_input(&lisp, "(cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) (else 500))", json!({"vol": 0.01})).await?, "100");
    check!(ev_input(&lisp, "(cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) (else 500))", json!({"vol": 0.05})).await?, "300");
    // and/or/not
    check!(ev(&lisp, "(and (>= 0.03 0.01) (<= 0.03 0.05))").await?, "true");
    check!(ev(&lisp, "(not (> 0.03 0.05))").await?, "true");
    // CLMM in-range
    check!(ev(&lisp, "(and (>= 411130 410630) (<= 411130 411630))").await?, "true");
    check!(ev(&lisp, "(and (>= 413000 410630) (<= 413000 411630))").await?, "false");
    println!("✅ Cond/logic\n");
    Ok(())
}

#[tokio::test]
async fn test_float_json_roundtrip() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== FLOAT JSON ROUNDTRIP ===");

    // Parse → compute → build
    let r = ev_input(&lisp, r#"
(define d (json-parse json_str))
(define price (dict/get d "price"))
(define qty (dict/get d "qty"))
(json-build (dict "price" price "qty" qty "total" (* price qty)))
"#, json!({"json_str": r#"{"price": 1.35, "qty": 100}"#})).await?;
    println!("1. price*qty: {}", r);
    check!(r, "135");

    // Pool parse
    let r = ev_input(&lisp, r#"
(define pool (json-parse json_str))
(define point (dict/get pool "current_point"))
(define fee (dict/get pool "fee"))
(str-concat "point=" (to-string point) " fee=" (to-string fee))
"#, json!({"json_str": r#"{"current_point": 411130, "fee": 0.01, "volume": 5000.75}"#})).await?;
    println!("2. Pool: {}", r);
    check!(r.clone(), "411130");
    check!(r, "0.01");

    // Compute range from pool
    let r = ev_input(&lisp, r#"
(define pool (json-parse json_str))
(define cur (dict/get pool "current_point"))
(define vol 0.03)
(define width (cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) ((<= vol 0.10) 500) (else 1000)))
(json-build (dict "lo" (- cur width) "hi" (+ cur width)))
"#, json!({"json_str": r#"{"current_point": 413000}"#})).await?;
    println!("3. Range: {}", r);
    check!(r.clone(), "412700");
    check!(r, "413300");

    // Negative
    let r = ev_input(&lisp, r#"(dict/get (json-parse json_str) "change")"#, json!({"json_str": r#"{"change": -0.05}"#})).await?;
    println!("4. Negative: {}", r);
    check!(r, "-0.05");

    println!("✅ Roundtrip\n");
    Ok(())
}

// The float tests above already test json-parse/json-build.
// This test verifies the aliases work identically.

#[tokio::test]
async fn test_json_aliases() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== JSON ALIASES ===");

    // from-json == json-parse
    let a = ev_input(&lisp, "(json-parse json_str)", json!({"json_str": r#"{"x": 42}"#})).await?;
    let b = ev_input(&lisp, "(from-json json_str)", json!({"json_str": r#"{"x": 42}"#})).await?;
    assert_eq!(a, b, "json-parse != from-json: {} vs {}", a, b);
    println!("✓ from-json == json-parse: {}", a);

    // to-json == json-build
    let a = ev(&lisp, r#"(json-build (dict "price" 1.35 "qty" 100))"#).await?;
    let b = ev(&lisp, r#"(to-json (dict "price" 1.35 "qty" 100))"#).await?;
    assert_eq!(a, b, "json-build != to-json: {} vs {}", a, b);
    println!("✓ to-json == json-build: {}", a);

    // Roundtrip: from-json → to-json
    let r = ev_input(&lisp, r#"(to-json (from-json json_str))"#, json!({"json_str": r#"{"hello":"world","num":42}"#})).await?;
    println!("✓ from-json→to-json roundtrip: {}", r);
    check!(r.clone(), "hello");
    check!(r, "42");

    // Parse with from-json, extract, build with to-json
    let r = ev_input(&lisp, r#"
(define d (from-json json_str))
(define price (dict/get d "price"))
(to-json (dict "total" (* price 2)))
"#, json!({"json_str": r#"{"price": 1.35}"#})).await?;
    println!("✓ from-json + to-json pipeline: {}", r);
    check!(r, "2.7");

    println!("✅ JSON aliases PASSED\n");
    Ok(())
}
