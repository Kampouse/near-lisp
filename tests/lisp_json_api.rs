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

// ═══════════════════════════════════════
// from-json / json-parse
// ═══════════════════════════════════════

#[tokio::test]
async fn test_from_json_primitives() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== from-json primitives ===");

    check!(ev(&lisp, "(from-json \"42\")").await?, "42");
    check!(ev(&lisp, "(from-json \"0\")").await?, "0");
    check!(ev(&lisp, "(from-json \"-7\")").await?, "-7");
    check!(ev(&lisp, "(from-json \"3.14\")").await?, "3.14");
    check!(ev(&lisp, "(from-json \"0.001\")").await?, "0.001");
    check!(ev(&lisp, "(from-json \"-1.5\")").await?, "-1.5");
    check!(ev(&lisp, "(from-json \"true\")").await?, "true");
    check!(ev(&lisp, "(from-json \"false\")").await?, "false");
    check!(ev(&lisp, "(from-json \"null\")").await?, "nil");

    // Alias equivalence
    let a = evj(&lisp, "(from-json s)", r#"{"x":1}"#).await?;
    let b = evj(&lisp, "(json-parse s)", r#"{"x":1}"#).await?;
    assert_eq!(a, b, "aliases differ: {} vs {}", a, b);

    println!("✅ from-json primitives (10 checks)");
    Ok(())
}

#[tokio::test]
async fn test_from_json_objects() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== from-json objects ===");

    // Empty
    let r = ev(&lisp, "(from-json \"{}\")").await?;
    check!(r, "{}");

    // Single/multi key
    let r = evj(&lisp, "(from-json s)", r#"{"a":1,"b":2}"#).await?;
    check!(r.clone(), "a");
    check!(r, "b");

    // Nested
    let r = evj(&lisp, "(from-json s)", r#"{"outer":{"inner":42}}"#).await?;
    check!(r.clone(), "outer");
    check!(r, "inner");

    // dict/get
    check!(evj(&lisp, "(dict/get (from-json s) \"name\")", r#"{"name":"alice"}"#).await?, "alice");
    check!(evj(&lisp, "(dict/get (from-json s) \"age\")", r#"{"age":30}"#).await?, "30");
    check!(evj(&lisp, "(dict/get (from-json s) \"missing\")", r#"{"x":1}"#).await?, "nil");

    // Mixed types
    let r = evj(&lisp, "(from-json s)", r#"{"s":"hi","n":42,"b":true,"x":null}"#).await?;
    check!(r.clone(), "hi");
    check!(r.clone(), "42");
    check!(r.clone(), "true");
    check!(r, "nil");

    println!("✅ from-json objects (12 checks)");
    Ok(())
}

#[tokio::test]
async fn test_from_json_arrays() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== from-json arrays ===");

    check!(ev(&lisp, "(from-json \"[]\")").await?, "()");
    check!(ev(&lisp, "(from-json \"[1,2,3]\")").await?, "1");
    check!(ev(&lisp, "(len (from-json \"[1,2,3,4]\"))").await?, "4");
    check!(ev(&lisp, "(nth (from-json \"[10,20,30]\") 0)").await?, "10");
    check!(ev(&lisp, "(nth (from-json \"[10,20,30]\") 2)").await?, "30");

    // Nested
    let r = ev(&lisp, "(from-json \"[[1,2],[3,4]]\")").await?;
    check!(r.clone(), "1");
    check!(r, "3");

    // Mixed
    let r = evj(&lisp, "(from-json s)", r#"[1,"hi",true,null]"#).await?;
    check!(r.clone(), "1");
    check!(r.clone(), "hi");
    check!(r.clone(), "true");
    check!(r, "nil");

    println!("✅ from-json arrays (10 checks)");
    Ok(())
}

#[tokio::test]
async fn test_json_get() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== json-get / json-get-in ===");

    check!(evj(&lisp, "(json-get s \"name\")", r#"{"name":"alice"}"#).await?, "alice");
    check!(evj(&lisp, "(json-get s \"count\")", r#"{"count":42}"#).await?, "42");
    check!(evj(&lisp, "(json-get s \"nope\")", r#"{"x":1}"#).await?, "nil");
    check!(evj(&lisp, "(json-get-in s \"a\" \"b\")", r#"{"a":{"b":99}}"#).await?, "99");
    check!(evj(&lisp, "(json-get-in s \"a\" \"b\" \"c\")", r#"{"a":{"b":{"c":"deep"}}}"#).await?, "deep");
    check!(evj(&lisp, "(json-get-in s \"a\" \"x\")", r#"{"a":{"b":1}}"#).await?, "nil");
    check!(evj(&lisp, "(json-get s \"price\")", r#"{"price":1.35}"#).await?, "1.35");

    println!("✅ json-get (7 checks)");
    Ok(())
}

// ═══════════════════════════════════════
// to-json / json-build
// ═══════════════════════════════════════

#[tokio::test]
async fn test_to_json_primitives() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== to-json primitives ===");

    check!(ev(&lisp, "(to-json 42)").await?, "42");
    check!(ev(&lisp, "(to-json 0)").await?, "0");
    check!(ev(&lisp, "(to-json -7)").await?, "-7");
    check!(ev(&lisp, "(to-json 3.14)").await?, "3.14");
    check!(ev(&lisp, "(to-json true)").await?, "true");
    check!(ev(&lisp, "(to-json false)").await?, "false");
    check!(ev(&lisp, "(to-json nil)").await?, "null");

    // Alias equivalence
    let a = ev(&lisp, "(to-json (dict \"x\" 1))").await?;
    let b = ev(&lisp, "(json-build (dict \"x\" 1))").await?;
    assert_eq!(a, b);

    println!("✅ to-json primitives (8 checks)");
    Ok(())
}

#[tokio::test]
async fn test_to_json_structures() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== to-json structures ===");

    // Dict
    let r = ev(&lisp, "(to-json (dict \"name\" \"alice\" \"age\" 30))").await?;
    check!(r.clone(), "name");
    check!(r.clone(), "alice");
    check!(r, "30");

    // List
    let r = ev(&lisp, "(to-json (list 1 2 3))").await?;
    check!(r, "1");

    // Nested dict
    let r = ev(&lisp, "(to-json (dict \"outer\" (dict \"x\" 1)))").await?;
    check!(r.clone(), "outer");
    check!(r, "x");

    // Computed
    check!(ev(&lisp, "(to-json (dict \"sum\" (+ 1 2)))").await?, "3");
    check!(ev(&lisp, "(to-json (dict \"price\" 1.35))").await?, "1.35");
    check!(ev(&lisp, "(to-json (dict \"active\" true))").await?, "true");
    check!(ev(&lisp, "(to-json (dict \"data\" nil))").await?, "null");

    // Empty
    check!(ev(&lisp, "(to-json (dict))").await?, "{}");
    check!(ev(&lisp, "(to-json (list))").await?, "[]");

    println!("✅ to-json structures (12 checks)");
    Ok(())
}

// ═══════════════════════════════════════
// Roundtrip & DCL simulation
// ═══════════════════════════════════════

#[tokio::test]
async fn test_json_roundtrip() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== roundtrip ===");

    check!(ev(&lisp, "(to-json (from-json \"42\"))").await?, "42");
    check!(ev(&lisp, "(to-json (from-json \"true\"))").await?, "true");
    check!(ev(&lisp, "(to-json (from-json \"null\"))").await?, "null");

    let r = evj(&lisp, "(to-json (from-json s))", r#"{"x":1,"y":2}"#).await?;
    check!(r.clone(), "x");
    check!(r, "y");

    // Parse→transform→build
    let r = evj(&lisp, r#"
(define d (from-json s))
(define items (dict/get d "items"))
(define total (dict/get d "total"))
(to-json (dict "total" total "count" (len items)))
"#, r#"{"items":[10,20,30],"total":60}"#).await?;
    check!(r.clone(), "total");
    check!(r, "count");

    // Error on bad input
    check!(ev(&lisp, "(from-json \"bad\")").await?, "from-json:");
    check!(ev(&lisp, "(from-json \"\")").await?, "from-json:");

    println!("✅ roundtrip (8 checks)");
    Ok(())
}

#[tokio::test]
async fn test_dcl_simulation() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== DCL simulation ===");

    // Parse pool response
    let pool = r#"{"pool_id":"usdt.tether-token.near|wrap.near|100","current_point":411130,"fee_rate":100,"liquidity":"5000000000000000000000","volume_24h":"123456789"}"#;

    let r = evj(&lisp, r#"
(define pool (from-json s))
(define cur (dict/get pool "current_point"))
(define liq (dict/get pool "liquidity"))
(to-json (dict "point" cur "liq" liq))
"#, pool).await?;
    check!(r.clone(), "411130");
    check!(r, "5000000");

    // Rebalance in-range
    let r = evj(&lisp, r#"
(define pool (from-json s))
(define cur (dict/get pool "current_point"))
(define in-range (and (>= cur 410630) (<= cur 411630)))
(if in-range (to-json (dict "action" "HOLD" "point" cur)) "NOPE")
"#, pool).await?;
    check!(r.clone(), "HOLD");
    check!(r, "411130");

    // Rebalance out-of-range
    let pool_out = r#"{"current_point":413000,"fee_rate":100}"#;
    let r = evj(&lisp, r#"
(define pool (from-json s))
(define cur (dict/get pool "current_point"))
(define vol 0.03)
(define width (cond ((<= vol 0.01) 100) ((<= vol 0.05) 300) (else 1000)))
(define in-range (and (>= cur 410630) (<= cur 411630)))
(if in-range
  (to-json (dict "action" "HOLD"))
  (to-json (dict "action" "REBALANCE" "lo" (- cur width) "hi" (+ cur width))))
"#, pool_out).await?;
    check!(r.clone(), "REBALANCE");
    check!(r.clone(), "412700");
    check!(r, "413300");

    // Quote simulation
    let quote = r#"{"amount":"714846670706717145976631","pool_id":"usdt.tether-token.near|wrap.near|100"}"#;
    check!(evj(&lisp, "(json-get s \"amount\")", quote).await?, "714846");

    println!("✅ DCL simulation (9 checks)");
    Ok(())
}
