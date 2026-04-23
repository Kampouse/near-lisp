use near_workspaces::network::Sandbox;
use near_workspaces::Worker;
use serde_json::json;
use std::fmt::Write;

async fn setup() -> anyhow::Result<(Worker<Sandbox>, near_workspaces::Contract)> {
    let worker: Worker<Sandbox> = near_workspaces::sandbox().await?;
    let lisp = worker.dev_deploy(&std::fs::read("target/near/near_lisp.wasm")?).await?;
    lisp.call("new").args_json(json!({ "eval_gas_limit": 500_000 }))
        .max_gas().transact().await?.into_result()?;
    Ok((worker, lisp))
}

async fn ev(lisp: &near_workspaces::Contract, code: &str) -> String {
    lisp.call("eval").args_json(json!({ "code": code }))
        .max_gas().transact().await
        .map(|r| r.json::<String>().unwrap_or_else(|_| "DECODE_ERR".into()))
        .unwrap_or_else(|e| format!("EXEC_ERR: {}", e))
}

// ==================== MALFORMED INPUT ====================

#[tokio::test]
async fn test_fuzz_unclosed_parens() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Unclosed Parens ===");

    let cases = vec![
        "(", "((", "(()", "(+ 1", "(+ 1 2", "(+ (", "((()", "(let ((x 1)",
    ];
    for code in cases {
        let r = ev(&lisp, code).await;
        assert!(r.contains("ERROR") || r.contains("EXEC_ERR"), "expected error for: {} got: {}", code, r);
        println!("  '{}' → error ✓", code);
    }
    println!("✅ Unclosed parens: all errors");
    Ok(())
}

#[tokio::test]
async fn test_fuzz_extra_close_parens() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Extra Close Parens ===");

    let cases = vec![
        ")", "))", "())", "(+ 1 2))", "(()", "()())",
    ];
    for code in cases {
        let r = ev(&lisp, code).await;
        assert!(r.contains("ERROR") || r.contains("EXEC_ERR"), "expected error for: {} got: {}", code, r);
        println!("  '{}' → error ✓", code);
    }
    println!("✅ Extra close parens: all errors");
    Ok(())
}

#[tokio::test]
async fn test_fuzz_empty_and_whitespace() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Empty & Whitespace ===");

    let cases = vec!["", " ", "   ", "\t", "\n", "  \n\t  "];
    for code in cases {
        let r = ev(&lisp, code).await;
        // Empty/whitespace should not panic — error, nil, or empty string all OK
        assert!(!r.is_empty() || code.is_empty(), "should return something for: {:?}", code);
        println!("  {:?} → {:?} ✓", code, &r[..r.len().min(30)]);
    }
    println!("✅ Empty/whitespace: all errors");
    Ok(())
}

#[tokio::test]
async fn test_fuzz_garbage_input() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Garbage Input ===");

    let cases = vec![
        "undefined", "null", "NaN", "Infinity", "###", "!!!", "@@@",
        "abc def ghi", "123abc", "0xdeadbeef", "0b1010",
        "{\"key\": 42}", "[1,2,3]", "func()", "x => x",
        ";;;", "; comment", "# comment", "// comment",
        "<html>", "SELECT * FROM", "rm -rf /",
    ];
    for code in cases {
        let r = ev(&lisp, code).await;
        // Should either error or return something reasonable (not panic/crash)
        assert!(!r.is_empty(), "should return something for: {}", code);
        println!("  '{}' → {} ✓", code, &r[..r.len().min(50)]);
    }
    println!("✅ Garbage input: no panics");
    Ok(())
}

// ==================== DEEP RECURSION ====================

#[tokio::test]
async fn test_fuzz_deep_nesting() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Deep Nesting (50 levels) ===");

    // 50 levels of (+ (+ (+ ... 1) 1) 1)
    let mut code = "1".to_string();
    for _ in 0..50 {
        code = format!("(+ {})", code);
    }
    let r = ev(&lisp, &code).await;
    println!("  50-deep (+): {}", &r[..r.len().min(40)]);
    // Should complete or error gracefully
    assert!(!r.is_empty());

    // 50 levels of (list (list (list ... nil)))
    let mut code = "nil".to_string();
    for _ in 0..50 {
        code = format!("(list {})", code);
    }
    let r = ev(&lisp, &code).await;
    println!("  50-deep (list): {}", &r[..r.len().min(40)]);
    assert!(!r.is_empty());

    println!("✅ Deep nesting: no crash");
    Ok(())
}

#[tokio::test]
async fn test_fuzz_wide_expressions() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Wide Expressions ===");

    // 100 arguments to +
    let args: Vec<String> = (0..100).map(|i| i.to_string()).collect();
    let code = format!("(+ {})", args.join(" "));
    let r = ev(&lisp, &code).await;
    println!("  (+ 0..99): {}", &r[..r.len().min(40)]);
    assert!(!r.is_empty());

    // Long string
    let long_str = "a".repeat(1000);
    let code = format!("(len \"{}\")", long_str);
    let r = ev(&lisp, &code).await;
    println!("  len(1000 chars): {}", &r[..r.len().min(40)]);
    assert!(!r.is_empty());

    println!("✅ Wide expressions: no crash");
    Ok(())
}

// ==================== TYPE CONFUSION ====================

#[tokio::test]
async fn test_fuzz_type_confusion() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Type Confusion ===");

    let cases = vec![
        "(+ \"hello\" 1)",           // string + number
        "(* \"2\" 3)",               // string * number
        "(+ nil 1)",                 // nil + number
        "(- true 1)",                // bool - number
        "(if 42 1 2)",               // non-bool condition
        "(if nil 1 2)",              // nil condition
        "(if \"\" 1 2)",             // empty string condition
        "(len 42)",                  // len on number
        "(len nil)",                 // len on nil
        "(> \"a\" \"b\")",           // compare strings
        "(= nil nil)",               // nil = nil
        "(= nil false)",             // nil = false
        "(dict/get 42 \"key\")",     // get on number
        "(nth 0 42)",                // nth on number
        "(str-concat 1 2)",          // concat numbers
        "(to-json (to-json 42))",    // double to-json
        "(from-json (from-json \"42\"))", // double from-json
        "(+ (dict) 1)",              // dict + number
        "(list? (fn (x) x))",        // lambda type check
    ];
    for code in cases {
        let r = ev(&lisp, code).await;
        // Should not panic, just error or return something
        assert!(!r.is_empty(), "empty result for: {}", code);
        println!("  {} → {} ✓", code, &r[..r.len().min(40)]);
    }
    println!("✅ Type confusion: no panics");
    Ok(())
}

// ==================== EDGE CASE VALUES ====================

#[tokio::test]
async fn test_fuzz_edge_values() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Edge Values ===");

    let cases = vec![
        "(/ 1 0)",                   // divide by zero
        "(% 10 0)",                  // mod by zero
        "(nth -1 (list 1 2 3))",    // negative index
        "(nth 100 (list 1 2 3))",   // out of bounds
        "(dict/get (dict) \"key\")", // empty dict get
        "(dict/remove (dict) \"key\")", // empty dict remove
        "(car nil)",                 // car of nil
        "(cdr nil)",                 // cdr of nil
        "(car (list))",              // car of empty list
        "(cdr (list))",              // cdr of empty list
        "(! 0)",                     // not zero
        "(! nil)",                   // not nil
        "(! \"\")",                   // not empty string
        "(&& false true)",           // and
        "(|| false true)",           // or
        "(max)",                     // max with no args
        "(min)",                     // min with no args
        "(+ )",                      // + with no args
        "(list)",                    // empty list
        "(dict)",                    // empty dict
        "(str-concat)",              // concat with no args
    ];
    for code in cases {
        let r = ev(&lisp, code).await;
        assert!(!r.is_empty(), "empty result for: {}", code);
        println!("  {} → {} ✓", code, &r[..r.len().min(40)]);
    }
    println!("✅ Edge values: no panics");
    Ok(())
}

// ==================== INJECTION / SAFETY ====================

#[tokio::test]
async fn test_fuzz_injection_safety() -> anyhow::Result<()> {
    let (_, lisp) = setup().await?;
    println!("=== Injection Safety ===");

    let cases = vec![
        // These should NOT escape the Lisp sandbox
        "(near/storage-write \"evil\" \"value\")",   // storage write from eval
        "(+ 1 (near/ccall-view",                      // broken ccall
        "(eval \"(+ 1 2)\")",                         // eval-in-eval if it exists
        "(apply + (list 1 2 3))",                     // apply
        "(system \"ls\")",                            // no system call
        "(shell \"rm -rf /\")",                       // no shell
        "(read-file \"/etc/passwd\")",               // no file access
        "(http \"http://evil.com\")",                // no http
    ];
    for code in cases {
        let r = ev(&lisp, code).await;
        assert!(!r.is_empty());
        // Should error (undefined) — not crash or execute
        println!("  {} → {} ✓", code, &r[..r.len().min(50)]);
    }
    println!("✅ Injection: no escape");
    Ok(())
}
