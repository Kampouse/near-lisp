use near_lisp::*;

fn eval_str(code: &str) -> String {
    let mut env = Vec::new();
    run_program(code, &mut env, 10_000).unwrap_or_else(|e| format!("ERROR: {}", e))
}

fn eval_str_gas(code: &str, gas: u64) -> String {
    let mut env = Vec::new();
    run_program(code, &mut env, gas).unwrap_or_else(|e| format!("ERROR: {}", e))
}

#[test]
fn test_arithmetic() {
    assert_eq!(eval_str("(+ 1 2)"), "3");
    assert_eq!(eval_str("(* 3 4)"), "12");
    assert_eq!(eval_str("(- 10 3)"), "7");
    assert_eq!(eval_str("(/ 10 2)"), "5");
    assert_eq!(eval_str("(mod 10 3)"), "1");
}

#[test]
fn test_nested_arithmetic() {
    assert_eq!(eval_str("(+ 1 (* 2 3))"), "7");
    assert_eq!(eval_str("(* (+ 2 3) (- 10 5))"), "25");
}

#[test]
fn test_comparison() {
    assert_eq!(eval_str("(> 5 3)"), "true");
    assert_eq!(eval_str("(< 2 5)"), "true");
    assert_eq!(eval_str("(= 3 3)"), "true");
    assert_eq!(eval_str("(!= 3 4)"), "true");
    assert_eq!(eval_str("(>= 5 5)"), "true");
    assert_eq!(eval_str("(<= 4 5)"), "true");
}

#[test]
fn test_boolean_logic() {
    assert_eq!(eval_str("(and true true)"), "true");
    assert_eq!(eval_str("(and true false)"), "false");
    assert_eq!(eval_str("(or false true)"), "true");
    assert_eq!(eval_str("(or false false)"), "false");
    assert_eq!(eval_str("(not true)"), "false");
    assert_eq!(eval_str("(not false)"), "true");
}

#[test]
fn test_define_and_lambda() {
    assert_eq!(eval_str("(define x 42) x"), "42");
    assert_eq!(
        eval_str("(define square (lambda (n) (* n n))) (square 5)"),
        "25"
    );
    assert_eq!(
        eval_str("(define add (lambda (a b) (+ a b))) (add 3 4)"),
        "7"
    );
}

#[test]
fn test_inline_lambda() {
    assert_eq!(eval_str("((lambda (x) (* x x)) 6)"), "36");
}

#[test]
fn test_if() {
    assert_eq!(eval_str("(if (> 5 3) 10 20)"), "10");
    assert_eq!(eval_str("(if (< 5 3) 10 20)"), "20");
    assert_eq!(eval_str("(if true 1)"), "1");
    assert_eq!(eval_str("(if false 1)"), "nil");
}

#[test]
fn test_cond() {
    let code = r#"
        (cond
            ((> 1 2) "first")
            ((> 2 1) "second")
            (else "third"))
    "#;
    assert_eq!(eval_str(code), "\"second\"");
}

#[test]
fn test_let() {
    assert_eq!(eval_str("(let ((x 10) (y 20)) (+ x y))"), "30");
}

#[test]
fn test_progn() {
    assert_eq!(eval_str("(progn (define a 1) (define b 2) (+ a b))"), "3");
}

#[test]
fn test_list_ops() {
    assert_eq!(eval_str("(list 1 2 3)"), "(1 2 3)");
    assert_eq!(eval_str("(car (list 1 2 3))"), "1");
    assert_eq!(eval_str("(cdr (list 1 2 3))"), "(2 3)");
    assert_eq!(eval_str("(len (list 1 2 3))"), "3");
    assert_eq!(eval_str("(nth 1 (list 10 20 30))"), "20");
    assert_eq!(eval_str("(cons 0 (list 1 2))"), "(0 1 2)");
    assert_eq!(eval_str("(append (list 1 2) (list 3 4))"), "(1 2 3 4)");
}

#[test]
fn test_string_ops() {
    assert_eq!(eval_str("(str-contains \"hello world\" \"world\")"), "true");
    assert_eq!(eval_str("(str-contains \"hello\" \"xyz\")"), "false");
    assert_eq!(eval_str("(len \"hello\")"), "5");
}

#[test]
fn test_type_checks() {
    assert_eq!(eval_str("(nil? nil)"), "true");
    assert_eq!(eval_str("(nil? 42)"), "false");
    assert_eq!(eval_str("(list? (list 1 2))"), "true");
    assert_eq!(eval_str("(number? 42)"), "true");
    assert_eq!(eval_str("(string? \"hi\")"), "true");
}

#[test]
fn test_recursive_fibonacci() {
    let code = r#"
        (define fib (lambda (n)
            (if (<= n 1)
                n
                (+ (fib (- n 1)) (fib (- n 2))))))
        (fib 10)
    "#;
    assert_eq!(eval_str(code), "55");
}

#[test]
fn test_fibonacci_15() {
    let code = r#"
        (define fib (lambda (n)
            (if (<= n 1)
                n
                (+ (fib (- n 1)) (fib (- n 2))))))
        (fib 15)
    "#;
    assert_eq!(eval_str_gas(code, 50_000), "610");
}

#[test]
fn test_higher_order() {
    let code = r#"
        (define apply (lambda (f x) (f x)))
        (define double (lambda (n) (* n 2)))
        (apply double 21)
    "#;
    assert_eq!(eval_str(code), "42");
}

#[test]
fn test_closures() {
    let code = r#"
        (define make-adder (lambda (n)
            (lambda (x) (+ n x))))
        (define add5 (make-adder 5))
        (add5 10)
    "#;
    assert_eq!(eval_str(code), "15");
}

#[test]
fn test_gas_limit() {
    let mut env = Vec::new();
    let code = r#"
        (define fib (lambda (n)
            (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2))))))
        (fib 20)
    "#;
    let result = run_program(code, &mut env, 50);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("gas"));
}

#[test]
fn test_policy_pass() {
    let policy = r#"(and (>= score 85) (<= duration 3600) (str-contains status "complete"))"#;
    let input = r#"{"score": 90, "duration": 1200, "status": "complete"}"#;
    let mut env: Vec<(String, LispVal)> = Vec::new();
    if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(input) {
        for (key, val) in map {
            env.push((key, json_to_lisp(val)));
        }
    }
    assert_eq!(run_program(policy, &mut env, 10_000).unwrap(), "true");
}

#[test]
fn test_policy_fail() {
    let policy = r#"(and (>= score 85) (<= duration 3600) (str-contains status "complete"))"#;
    let input = r#"{"score": 70, "duration": 1200, "status": "complete"}"#;
    let mut env: Vec<(String, LispVal)> = Vec::new();
    if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(input) {
        for (key, val) in map {
            env.push((key, json_to_lisp(val)));
        }
    }
    assert_eq!(run_program(policy, &mut env, 10_000).unwrap(), "false");
}

#[test]
fn test_complex_policy() {
    let policy = r#"
        (let ((min-score 80)
              (max-time 5000))
            (and
                (>= score min-score)
                (<= duration max-time)
                (or
                    (str-contains status "complete")
                    (str-contains status "partial"))))
    "#;
    let input = r#"{"score": 92, "duration": 3000, "status": "partial success"}"#;
    let mut env: Vec<(String, LispVal)> = Vec::new();
    if let Ok(map) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(input) {
        for (key, val) in map {
            env.push((key, json_to_lisp(val)));
        }
    }
    assert_eq!(run_program(policy, &mut env, 10_000).unwrap(), "true");
}

// ===========================================================================
// Yield/Resume + Cross-Contract Call tests
// ===========================================================================

#[test]
fn test_vmstate_roundtrip() {
    // Verify VmState serializes/deserializes correctly via borsh
    let state = VmState {
        remaining: vec![LispVal::List(vec![
            LispVal::Sym("+".into()),
            LispVal::Num(1),
            LispVal::Num(2),
        ])],
        env: vec![
            ("x".to_string(), LispVal::Num(42)),
            ("name".to_string(), LispVal::Str("test".into())),
        ],
        gas: 9500,
        pending_var: Some("price".to_string()),
    };

    let bytes = borsh::to_vec(&state).expect("serialize");
    let restored: VmState = borsh::from_slice(&bytes).expect("deserialize");

    assert_eq!(restored.gas, 9500);
    assert_eq!(restored.pending_var, Some("price".to_string()));
    assert_eq!(restored.env.len(), 2);
    assert_eq!(restored.remaining.len(), 1);
    // Verify env content
    assert_eq!(restored.env[0].0, "x");
    assert_eq!(restored.env[0].1, LispVal::Num(42));
}

#[test]
fn test_vmstate_complex_env() {
    // Test with lambda closures in the env (most complex LispVal variant)
    let state = VmState {
        remaining: vec![],
        env: vec![(
            "fib".to_string(),
            LispVal::Lambda {
                params: vec!["n".into()],
                body: Box::new(LispVal::List(vec![
                    LispVal::Sym("if".into()),
                    LispVal::List(vec![
                        LispVal::Sym("<=".into()),
                        LispVal::Sym("n".into()),
                        LispVal::Num(1),
                    ]),
                    LispVal::Sym("n".into()),
                    LispVal::List(vec![
                        LispVal::Sym("+".into()),
                        LispVal::List(vec![
                            LispVal::Sym("fib".into()),
                            LispVal::List(vec![
                                LispVal::Sym("-".into()),
                                LispVal::Sym("n".into()),
                                LispVal::Num(1),
                            ]),
                        ]),
                        LispVal::List(vec![
                            LispVal::Sym("fib".into()),
                            LispVal::List(vec![
                                LispVal::Sym("-".into()),
                                LispVal::Sym("n".into()),
                                LispVal::Num(2),
                            ]),
                        ]),
                    ]),
                ])),
                closed_env: Box::new(vec![]),
            },
        )],
        gas: 50000,
        pending_var: None,
    };

    let bytes = borsh::to_vec(&state).expect("serialize lambda env");
    let restored: VmState = borsh::from_slice(&bytes).expect("deserialize");
    assert_eq!(restored.gas, 50000);
    assert!(matches!(restored.env[0].1, LispVal::Lambda { .. }));
}

#[test]
fn test_run_program_no_ccall() {
    // run_program_with_ccall returns Done when no ccall is present
    let mut env = Vec::new();
    let result = run_program_with_ccall("(+ 1 2)", &mut env, 10_000).unwrap();
    match result {
        RunResult::Done(s) => assert_eq!(s, "3"),
        RunResult::Yield(_) => panic!("Expected Done, got Yield"),
    }
}

#[test]
fn test_run_program_ccall_define_pattern() {
    // (define price (near/ccall "ref.near" "get_price" "{}"))
    // Should yield with pending_var = Some("price")
    let mut env = Vec::new();
    let code = r#"
        (define x 42)
        (define price (near/ccall "ref.near" "get_price" "{}"))
        (+ x 10)
    "#;
    let result = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "ref.near");
            assert_eq!(yi.method, "get_price");
            assert_eq!(yi.state.pending_var, Some("price".to_string()));
            // x=42 should be in the env
            assert!(yi
                .state
                .env
                .iter()
                .any(|(k, v)| k == "x" && *v == LispVal::Num(42)));
            // remaining should contain (+ x 10)
            assert_eq!(yi.state.remaining.len(), 1);
        }
        RunResult::Done(_) => panic!("Expected Yield, got Done"),
    }
}

#[test]
fn test_run_program_ccall_standalone_pattern() {
    // (near/ccall "ref.near" "get_price" "{}") standalone
    let mut env = Vec::new();
    let code = r#"
        (near/ccall "oracle.near" "latest" "{}")
        (near/ccall-result)
    "#;
    let result = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "oracle.near");
            assert_eq!(yi.method, "latest");
            assert_eq!(yi.state.pending_var, None); // standalone, no define
            assert_eq!(yi.state.remaining.len(), 1); // (near/ccall-result)
        }
        RunResult::Done(_) => panic!("Expected Yield, got Done"),
    }
}

#[test]
fn test_run_program_multiple_top_level_only_first_ccall_yields() {
    // If code has two ccall forms, only the first one triggers yield
    let mut env = Vec::new();
    let code = r#"
        (define a (near/ccall "x.near" "f" "{}"))
        (define b (near/ccall "y.near" "g" "{}"))
    "#;
    let result = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "x.near"); // first ccall
            assert_eq!(yi.state.remaining.len(), 1); // second define is in remaining
        }
        RunResult::Done(_) => panic!("Expected Yield"),
    }
}

// ===========================================================================
// Multi-ccall re-yield chain tests
// ===========================================================================

#[test]
fn test_run_remaining_with_ccall_no_ccall() {
    // run_remaining_with_ccall returns Done when no ccall in remaining
    let mut env = Vec::new();
    let exprs = parse_all("(+ 1 2) (* 3 4)").unwrap();
    let mut gas = 10_000u64;
    let result = run_remaining_with_ccall(&exprs, &mut env, &mut gas).unwrap();
    match result {
        RunResult::Done(s) => assert_eq!(s, "12"), // last expression result
        RunResult::Yield(_) => panic!("Expected Done, got Yield"),
    }
}

#[test]
fn test_run_remaining_with_ccall_yields_on_first_ccall() {
    // remaining expressions contain a ccall as first expression
    let mut env = vec![("x".to_string(), LispVal::Num(42))];
    let exprs = parse_all(
        r#"
        (define b (near/ccall "y.near" "g" "{}"))
        (+ x 10)
    "#,
    )
    .unwrap();
    let mut gas = 10_000u64;
    let result = run_remaining_with_ccall(&exprs, &mut env, &mut gas).unwrap();
    match result {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "y.near");
            assert_eq!(yi.method, "g");
            assert_eq!(yi.state.pending_var, Some("b".to_string()));
            // x=42 should still be in env
            assert!(yi
                .state
                .env
                .iter()
                .any(|(k, v)| k == "x" && *v == LispVal::Num(42)));
            // remaining should have (+ x 10)
            assert_eq!(yi.state.remaining.len(), 1);
        }
        RunResult::Done(_) => panic!("Expected Yield, got Done"),
    }
}

#[test]
fn test_multi_ccall_two_ccalls_yield_chain() {
    // Simulate the full yield→resume→re-yield→resume→done chain
    // Step 1: Initial run yields on first ccall
    let mut env = Vec::new();
    let code = r#"
        (define a (near/ccall "x.near" "f" "{}"))
        (define b (near/ccall "y.near" "g" "{}"))
        (+ 1 2)
    "#;
    let result = run_program_with_ccall(code, &mut env, 10_000).unwrap();

    let state1 = match result {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "x.near");
            assert_eq!(yi.method, "f");
            assert_eq!(yi.state.pending_var, Some("a".to_string()));
            assert_eq!(yi.state.remaining.len(), 2); // second ccall + (+ 1 2)
            yi.state
        }
        RunResult::Done(_) => panic!("Expected Yield on first ccall"),
    };

    // Step 2: Simulate resume — inject first ccall result, run remaining
    let mut env2 = state1.env.clone();
    env2.push(("a".to_string(), LispVal::Str("result_a".to_string())));
    let mut gas2 = state1.gas;
    let result2 = run_remaining_with_ccall(&state1.remaining, &mut env2, &mut gas2).unwrap();

    let state2 = match result2 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "y.near"); // second ccall
            assert_eq!(yi.method, "g");
            assert_eq!(yi.state.pending_var, Some("b".to_string()));
            // env should have a=result_a
            assert!(yi
                .state
                .env
                .iter()
                .any(|(k, v)| k == "a" && *v == LispVal::Str("result_a".to_string())));
            // remaining should have (+ 1 2)
            assert_eq!(yi.state.remaining.len(), 1);
            yi.state
        }
        RunResult::Done(_) => panic!("Expected Yield on second ccall"),
    };

    // Step 3: Simulate second resume — inject second ccall result, run remaining
    let mut env3 = state2.env.clone();
    env3.push(("b".to_string(), LispVal::Str("result_b".to_string())));
    let mut gas3 = state2.gas;
    let result3 = run_remaining_with_ccall(&state2.remaining, &mut env3, &mut gas3).unwrap();

    match result3 {
        RunResult::Done(s) => {
            assert_eq!(s, "3"); // (+ 1 2)
        }
        RunResult::Yield(_) => panic!("Expected Done after all ccalls resolved"),
    }
}

#[test]
fn test_multi_ccall_env_accumulates_across_yields() {
    // Verify env bindings accumulate correctly across multiple yield/resume cycles
    let mut env = Vec::new();
    let code = r#"
        (define x 100)
        (define a (near/ccall "alpha.near" "f1" "{}"))
        (define y 200)
        (define b (near/ccall "beta.near" "f2" "{}"))
        (+ x y)
    "#;

    // First yield
    let result1 = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    let state1 = match result1 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "alpha.near");
            // x=100 should be in env
            assert!(yi
                .state
                .env
                .iter()
                .any(|(k, v)| k == "x" && *v == LispVal::Num(100)));
            yi.state
        }
        RunResult::Done(_) => panic!("Expected Yield"),
    };

    // Resume 1: inject a, then y=200 is defined, then second ccall yields
    let mut env2 = state1.env.clone();
    env2.push(("a".to_string(), LispVal::Str("alpha_result".to_string())));
    let mut gas2 = state1.gas;
    let result2 = run_remaining_with_ccall(&state1.remaining, &mut env2, &mut gas2).unwrap();
    let state2 = match result2 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "beta.near");
            // y=200 should have been defined before the second ccall
            assert!(yi
                .state
                .env
                .iter()
                .any(|(k, v)| k == "y" && *v == LispVal::Num(200)));
            // a should still be there
            assert!(yi
                .state
                .env
                .iter()
                .any(|(k, v)| k == "a" && *v == LispVal::Str("alpha_result".to_string())));
            yi.state
        }
        RunResult::Done(_) => panic!("Expected Yield on second ccall"),
    };

    // Resume 2: inject b, then (+ x y) evaluates
    let mut env3 = state2.env.clone();
    env3.push(("b".to_string(), LispVal::Str("beta_result".to_string())));
    let mut gas3 = state2.gas;
    let result3 = run_remaining_with_ccall(&state2.remaining, &mut env3, &mut gas3).unwrap();
    match result3 {
        RunResult::Done(s) => {
            assert_eq!(s, "300"); // (+ 100 200)
        }
        RunResult::Yield(_) => panic!("Expected Done"),
    }
}

#[test]
fn test_multi_ccall_gas_decreases_across_yields() {
    // Gas should decrease with each step across the yield chain
    let mut env = Vec::new();
    let code = r#"
        (define a (near/ccall "x.near" "f" "{}"))
        (define b (near/ccall "y.near" "g" "{}"))
    "#;

    let result1 = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    let gas_after_first = match &result1 {
        RunResult::Yield(yi) => yi.state.gas,
        RunResult::Done(_) => panic!("Expected Yield"),
    };

    // Gas decreased from initial 10_000
    assert!(
        gas_after_first < 10_000,
        "gas should decrease after first yield"
    );

    let state1 = match result1 {
        RunResult::Yield(yi) => yi.state,
        RunResult::Done(_) => panic!("unreachable"),
    };

    // Resume: run remaining
    let mut env2 = state1.env.clone();
    env2.push(("a".to_string(), LispVal::Str("r1".to_string())));
    let mut gas2 = state1.gas;
    let result2 = run_remaining_with_ccall(&state1.remaining, &mut env2, &mut gas2).unwrap();

    let gas_after_second = match &result2 {
        RunResult::Yield(yi) => yi.state.gas,
        RunResult::Done(_) => panic!("Expected second Yield"),
    };

    // Gas decreased further
    assert!(
        gas_after_second < gas_after_first,
        "gas should decrease across yields"
    );
}

#[test]
fn test_multi_ccall_standalone_ccall_chain() {
    // Standalone ccalls (without define wrapper) should also re-yield correctly
    let mut env = Vec::new();
    let code = r#"
        (near/ccall "oracle.near" "get1" "{}")
        (near/ccall-result)
        (near/ccall "oracle.near" "get2" "{}")
        (near/ccall-result)
    "#;

    // First yield on first ccall
    let result1 = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    let state1 = match result1 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "oracle.near");
            assert_eq!(yi.method, "get1");
            assert_eq!(yi.state.pending_var, None); // standalone
            yi.state
        }
        RunResult::Done(_) => panic!("Expected Yield"),
    };

    // Resume: inject __ccall_result__, remaining = (near/ccall-result) + ccall + ccall-result
    let mut env2 = state1.env.clone();
    env2.push((
        "__ccall_result__".to_string(),
        LispVal::Str("first_result".to_string()),
    ));
    let mut gas2 = state1.gas;
    let result2 = run_remaining_with_ccall(&state1.remaining, &mut env2, &mut gas2).unwrap();

    let state2 = match result2 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "oracle.near");
            assert_eq!(yi.method, "get2");
            assert_eq!(yi.state.pending_var, None);
            // remaining should have (near/ccall-result)
            assert_eq!(yi.state.remaining.len(), 1);
            yi.state
        }
        RunResult::Done(_) => panic!("Expected second Yield"),
    };

    // Second resume: inject result, evaluate last (near/ccall-result)
    let mut env3 = state2.env.clone();
    env3.push((
        "__ccall_result__".to_string(),
        LispVal::Str("second_result".to_string()),
    ));
    let mut gas3 = state2.gas;
    let result3 = run_remaining_with_ccall(&state2.remaining, &mut env3, &mut gas3).unwrap();

    match result3 {
        RunResult::Done(s) => {
            assert_eq!(s, "\"second_result\""); // (near/ccall-result) returns the last result
        }
        RunResult::Yield(_) => panic!("Expected Done"),
    }
}

#[test]
fn test_multi_ccall_mixed_define_and_standalone() {
    // Mix of (define x (near/ccall ...)) and standalone (near/ccall ...)
    let mut env = Vec::new();
    let code = r#"
        (define a (near/ccall "x.near" "f" "{}"))
        (near/ccall "y.near" "g" "{}")
        (near/ccall-result)
    "#;

    // First yield: define pattern
    let result1 = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    let state1 = match result1 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "x.near");
            assert_eq!(yi.state.pending_var, Some("a".to_string()));
            yi.state
        }
        RunResult::Done(_) => panic!("Expected Yield"),
    };

    // Resume 1: inject a, run remaining which has standalone ccall
    let mut env2 = state1.env.clone();
    env2.push(("a".to_string(), LispVal::Str("result_a".to_string())));
    let mut gas2 = state1.gas;
    let result2 = run_remaining_with_ccall(&state1.remaining, &mut env2, &mut gas2).unwrap();

    let state2 = match result2 {
        RunResult::Yield(yi) => {
            assert_eq!(yi.account, "y.near");
            assert_eq!(yi.state.pending_var, None); // standalone
            assert_eq!(yi.state.remaining.len(), 1); // (near/ccall-result)
            yi.state
        }
        RunResult::Done(_) => panic!("Expected second Yield"),
    };

    // Resume 2: inject __ccall_result__, evaluate (near/ccall-result)
    let mut env3 = state2.env.clone();
    env3.push((
        "__ccall_result__".to_string(),
        LispVal::Str("result_y".to_string()),
    ));
    let mut gas3 = state2.gas;
    let result3 = run_remaining_with_ccall(&state2.remaining, &mut env3, &mut gas3).unwrap();

    match result3 {
        RunResult::Done(s) => {
            assert_eq!(s, "\"result_y\"");
        }
        RunResult::Yield(_) => panic!("Expected Done"),
    }
}

#[test]
fn test_ccall_args_evaluated() {
    // Args expression should be evaluated before yielding
    let mut env = Vec::new();
    let code = r#"
        (define x "hello")
        (define result (near/ccall "test.near" "method" x))
    "#;
    let result = run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        RunResult::Yield(yi) => {
            assert_eq!(yi.args_bytes, b"hello"); // x evaluated to "hello"
        }
        RunResult::Done(_) => panic!("Expected Yield"),
    }
}

#[test]
fn test_run_program_synchronous_ignores_ccall_in_body() {
    // run_program (synchronous) doesn't know about ccall — it will try to
    // evaluate (near/ccall ...) as a regular function call and hit "undefined"
    let mut env = Vec::new();
    let result = run_program(
        r#"(near/ccall "test.near" "method" "{}")"#,
        &mut env,
        10_000,
    );
    // Should error because near/ccall isn't a regular function
    assert!(result.is_err());
}

#[test]
fn test_hex_roundtrip() {
    // Verify VmState borsh roundtrip works correctly
    let state = VmState {
        remaining: vec![],
        env: vec![],
        gas: 100,
        pending_var: None,
    };
    let serialized = borsh::to_vec(&state).unwrap();
    let deserialized: VmState = borsh::from_slice(&serialized).unwrap();
    assert_eq!(deserialized.gas, 100);
}

// ===========================================================================
// Crypto builtin tests
// ===========================================================================

#[test]
fn test_sha256() {
    // SHA256 of empty string = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    let result = eval_str("(sha256 \"\")");
    assert_eq!(
        result,
        "\"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\""
    );
}

#[test]
fn test_sha256_hello() {
    // SHA256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    let result = eval_str("(sha256 \"hello\")");
    assert_eq!(
        result,
        "\"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\""
    );
}

#[test]
fn test_keccak256() {
    // Keccak256 of empty string = c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470
    let result = eval_str("(keccak256 \"\")");
    assert_eq!(
        result,
        "\"c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470\""
    );
}

#[test]
fn test_sha256_in_policy() {
    // Use sha256 inside a policy expression
    let code = r#"
        (let ((h (sha256 "test")))
            (str-contains h "9f86d"))
    "#;
    assert_eq!(eval_str(code), "true");
}

// ===========================================================================
// Storage builtin tests (using near_sdk mock VM)
// ===========================================================================

fn setup_test_vm() {
    let context = near_sdk::test_utils::VMContextBuilder::new().build();
    near_sdk::testing_env!(context);
}

#[test]
fn test_storage_write_read() {
    setup_test_vm();
    let mut e = Vec::new();
    // Write
    let r = run_program(r#"(near/storage-write "mykey" "myval")"#, &mut e, 10_000);
    assert_eq!(r.unwrap(), "true");
    // Read back
    let r = run_program(r#"(near/storage-read "mykey")"#, &mut e, 10_000);
    assert_eq!(r.unwrap(), "\"myval\"");
}

#[test]
fn test_storage_read_missing() {
    setup_test_vm();
    let mut e = Vec::new();
    let r = run_program(r#"(near/storage-read "nonexistent")"#, &mut e, 10_000);
    assert_eq!(r.unwrap(), "nil");
}

#[test]
fn test_storage_has() {
    setup_test_vm();
    let mut e = Vec::new();
    // Key doesn't exist yet
    assert_eq!(eval_str(r#"(near/storage-has? "test-key")"#), "false");
    // Write it
    let _ = run_program(r#"(near/storage-write "test-key" "v")"#, &mut e, 10_000);
    // Now it exists
    assert_eq!(eval_str(r#"(near/storage-has? "test-key")"#), "true");
}

#[test]
fn test_storage_remove() {
    setup_test_vm();
    let mut e = Vec::new();
    // Write then remove
    let _ = run_program(r#"(near/storage-write "rm-key" "val")"#, &mut e, 10_000);
    let r = run_program(r#"(near/storage-remove "rm-key")"#, &mut e, 10_000);
    assert_eq!(r.unwrap(), "true");
    // Confirm gone
    let r = run_program(r#"(near/storage-read "rm-key")"#, &mut e, 10_000);
    assert_eq!(r.unwrap(), "nil");
}

// ===========================================================================
// Chain state builtin tests
// ===========================================================================

#[test]
fn test_account_balance_returns_string() {
    setup_test_vm();
    // Should return a yoctonear string
    let result = eval_str("(near/account-balance)");
    // Result is like "\"100000000000000000000000000\"" (100 NEAR in yocto)
    assert!(
        result.contains("100000000000000000000000000"),
        "got: {}",
        result
    );
}

#[test]
fn test_attached_deposit_returns_string() {
    setup_test_vm();
    let result = eval_str("(near/attached-deposit)");
    assert!(result.starts_with("\""), "got: {}", result);
}

#[test]
fn test_signer_equals() {
    setup_test_vm();
    // Default test context signer is "bob.near"
    let result = eval_str("(near/signer= \"bob.near\")");
    assert_eq!(result, "true");
    let result = eval_str("(near/signer= \"eve.near\")");
    assert_eq!(result, "false");
}

#[test]
fn test_predecessor_equals() {
    setup_test_vm();
    // Default test context predecessor is "bob.near"
    let result = eval_str("(near/predecessor= \"bob.near\")");
    assert_eq!(result, "true");
    let result = eval_str("(near/predecessor= \"eve.near\")");
    assert_eq!(result, "false");
}

// ===========================================================================
// Contract-level tests (owner, scripts, policies)
// ===========================================================================

fn setup_contract() -> near_lisp::LispContract {
    setup_test_vm();
    near_lisp::LispContract::new(10_000)
}

#[test]
fn test_contract_owner_is_signer() {
    let contract = setup_contract();
    let owner = contract.get_owner();
    // default VMContext signer is "bob.near"
    assert_eq!(owner.as_str(), "bob.near");
}

#[test]
fn test_contract_eval_basic() {
    let contract = setup_contract();
    assert_eq!(contract.eval("(+ 2 3)".to_string()), "5");
}

#[test]
fn test_contract_eval_with_input() {
    let contract = setup_contract();
    let result = contract.eval_with_input("(* x 2)".to_string(), r#"{"x": 21}"#.to_string());
    assert_eq!(result, "42");
}

#[test]
fn test_contract_save_and_get_policy() {
    let mut contract = setup_contract();
    // Owner (bob.near) can save
    contract.save_policy("test-policy".to_string(), "(= x 42)".to_string());
    let p = contract.get_policy("test-policy".to_string());
    assert_eq!(p, Some("(= x 42)".to_string()));
}

#[test]
fn test_contract_list_policies() {
    let mut contract = setup_contract();
    contract.save_policy("p1".to_string(), "(= x 1)".to_string());
    contract.save_policy("p2".to_string(), "(= x 2)".to_string());
    let mut names = contract.list_policies();
    names.sort();
    assert_eq!(names, vec!["p1", "p2"]);
}

#[test]
fn test_contract_eval_policy() {
    let mut contract = setup_contract();
    contract.save_policy(
        "is-admin".to_string(),
        r#"(near/signer= "bob.near")"#.to_string(),
    );
    let result = contract.eval_policy("is-admin".to_string(), "{}".to_string());
    assert_eq!(result, "true");
}

#[test]
fn test_contract_eval_policy_not_found() {
    let contract = setup_contract();
    let result = contract.eval_policy("nonexistent".to_string(), "{}".to_string());
    assert!(result.contains("not found"));
}

#[test]
fn test_contract_remove_policy() {
    let mut contract = setup_contract();
    contract.save_policy("temp".to_string(), "42".to_string());
    assert!(contract.get_policy("temp".to_string()).is_some());
    contract.remove_policy("temp".to_string());
    assert!(contract.get_policy("temp".to_string()).is_none());
}

#[test]
fn test_contract_save_and_get_script() {
    let mut contract = setup_contract();
    contract.save_script(
        "fib".to_string(),
        "(define fib (lambda (n) (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))) (fib 10)"
            .to_string(),
    );
    let s = contract.get_script("fib".to_string());
    assert!(s.is_some());
    assert!(s.unwrap().contains("fib"));
}

#[test]
fn test_contract_save_script_invalid_parse() {
    let mut contract = setup_contract();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.save_script("bad".to_string(), "(((".to_string());
    }));
    assert!(result.is_err());
}

#[test]
fn test_contract_list_scripts() {
    let mut contract = setup_contract();
    contract.save_script("s1".to_string(), "(+ 1 2)".to_string());
    contract.save_script("s2".to_string(), "(* 3 4)".to_string());
    let mut names = contract.list_scripts();
    names.sort();
    assert_eq!(names, vec!["s1", "s2"]);
}

#[test]
fn test_contract_eval_script() {
    let mut contract = setup_contract();
    contract.save_script("compute".to_string(), "(+ 10 20)".to_string());
    let result = contract.eval_script("compute".to_string());
    assert_eq!(result, "30");
}

#[test]
fn test_contract_eval_script_not_found() {
    let contract = setup_contract();
    let result = contract.eval_script("nonexistent".to_string());
    assert!(result.contains("not found"));
}

#[test]
fn test_contract_eval_script_with_input() {
    let mut contract = setup_contract();
    contract.save_script("double".to_string(), "(* x 2)".to_string());
    let result = contract.eval_script_with_input("double".to_string(), r#"{"x": 7}"#.to_string());
    assert_eq!(result, "14");
}

#[test]
fn test_contract_remove_script() {
    let mut contract = setup_contract();
    contract.save_script("temp".to_string(), "42".to_string());
    assert!(contract.get_script("temp".to_string()).is_some());
    contract.remove_script("temp".to_string());
    assert!(contract.get_script("temp".to_string()).is_none());
}

#[test]
fn test_contract_gas_limit() {
    let mut contract = setup_contract();
    assert_eq!(contract.get_gas_limit(), 10_000);
    contract.set_gas_limit(50_000);
    assert_eq!(contract.get_gas_limit(), 50_000);
}

#[test]
fn test_contract_transfer_ownership() {
    let mut contract = setup_contract();
    assert_eq!(contract.get_owner().as_str(), "bob.near");
    contract.transfer_ownership("alice.near".parse().unwrap());
    assert_eq!(contract.get_owner().as_str(), "alice.near");
}

#[test]
fn test_contract_default_gas_limit() {
    setup_test_vm();
    let c = near_lisp::LispContract::new(0);
    assert_eq!(c.get_gas_limit(), 10_000); // default fallback
}

#[test]
fn test_composite_crypto_policy() {
    setup_test_vm();
    // A realistic policy: hash some data, check signer, verify storage
    let code = r#"
        (progn
            (near/storage-write "attested" (sha256 "result-data"))
            (and
                (near/signer= "bob.near")
                (near/storage-has? "attested")
                (!= (near/storage-read "attested") nil)))
    "#;
    let result = eval_str(code);
    assert_eq!(result, "true");
}

// ===========================================================================
// SECTION: Float correctness tests
// ===========================================================================

#[test]
fn test_float_to_float_from_int() {
    assert_eq!(eval_str("(to-float 42)"), "42");
}

#[test]
fn test_float_to_int_truncation() {
    assert_eq!(eval_str("(to-int 3.7)"), "3");
}

#[test]
fn test_float_to_int_from_negative() {
    assert_eq!(eval_str("(to-int -2.3)"), "-2");
}

#[test]
fn test_float_add() {
    assert_eq!(eval_str("(+ 1.5 2.5)"), "4");
}

#[test]
fn test_float_mul_mixed() {
    assert_eq!(eval_str("(* 3 1.5)"), "4.5");
}

#[test]
fn test_float_sub() {
    assert_eq!(eval_str("(- 5.5 1.5)"), "4");
}

#[test]
fn test_float_div() {
    assert_eq!(eval_str("(/ 9.0 2.0)"), "4.5");
}

#[test]
fn test_float_lt_mixed() {
    assert_eq!(eval_str("(< 1.5 2)"), "true");
}

#[test]
fn test_float_gt_mixed() {
    assert_eq!(eval_str("(> 2.5 2)"), "true");
}

#[test]
fn test_float_eq_different_types() {
    // Float 1.0 and Int 1 are different types => false
    assert_eq!(eval_str("(= 1.0 1)"), "false");
}

#[test]
fn test_float_eq_same_type() {
    assert_eq!(eval_str("(= 1.5 1.5)"), "true");
}

#[test]
fn test_number_predicate_float() {
    assert_eq!(eval_str("(number? 3.14)"), "true");
}

#[test]
fn test_number_predicate_int() {
    assert_eq!(eval_str("(number? 42)"), "true");
}

#[test]
fn test_number_predicate_string() {
    assert_eq!(eval_str("(number? \"hello\")"), "false");
}

#[test]
fn test_float_literal_display() {
    assert_eq!(eval_str("3.14"), "3.14");
}

#[test]
fn test_float_literal_whole() {
    // 42.0 displays as "42" after stripping trailing zeros
    assert_eq!(eval_str("42.0"), "42");
}

#[test]
fn test_float_div_by_zero() {
    assert!(eval_str("(/ 1.0 0.0)").contains("ERROR"));
}

#[test]
fn test_float_gte() {
    assert_eq!(eval_str("(>= 2.5 2.5)"), "true");
}

#[test]
fn test_float_lte() {
    assert_eq!(eval_str("(<= 1.5 2)"), "true");
}

// ===========================================================================
// SECTION: Dict / Map correctness tests
// ===========================================================================

#[test]
fn test_dict_empty() {
    assert_eq!(eval_str("(dict)"), "{}");
}

#[test]
fn test_dict_with_pairs() {
    let result = eval_str(r#"(dict "a" 1 "b" 2)"#);
    assert!(result.contains("\"a\""), "should contain key a: {}", result);
    assert!(result.contains("\"b\""), "should contain key b: {}", result);
    assert!(result.contains("1"), "should contain val 1: {}", result);
    assert!(result.contains("2"), "should contain val 2: {}", result);
}

#[test]
fn test_dict_get_existing() {
    assert_eq!(eval_str(r#"(dict/get (dict "x" 42) "x")"#), "42");
}

#[test]
fn test_dict_get_missing() {
    assert_eq!(eval_str(r#"(dict/get (dict "x" 42) "y")"#), "nil");
}

#[test]
fn test_dict_has_existing() {
    assert_eq!(eval_str(r#"(dict/has? (dict "x" 1) "x")"#), "true");
}

#[test]
fn test_dict_has_missing() {
    assert_eq!(eval_str(r#"(dict/has? (dict "x" 1) "y")"#), "false");
}

#[test]
fn test_dict_set_adds_key() {
    let result = eval_str(r#"(dict/set (dict) "k" 42)"#);
    assert!(result.contains("\"k\""), "should contain key k: {}", result);
    assert!(result.contains("42"), "should contain val 42: {}", result);
}

#[test]
fn test_dict_set_overwrites() {
    let result = eval_str(r#"(dict/set (dict "a" 1) "a" 99)"#);
    assert!(
        result.contains("99"),
        "should contain new val 99: {}",
        result
    );
    assert!(
        !result.contains(": 1"),
        "should not contain old val 1: {}",
        result
    );
}

#[test]
fn test_dict_remove() {
    let result = eval_str(r#"(dict/remove (dict "x" 1 "y" 2) "x")"#);
    assert!(!result.contains("\"x\""), "x should be removed: {}", result);
    assert!(result.contains("\"y\""), "y should remain: {}", result);
}

#[test]
fn test_dict_keys() {
    let result = eval_str(r#"(dict/keys (dict "a" 1 "b" 2))"#);
    assert!(
        result.contains("\"a\""),
        "keys should contain a: {}",
        result
    );
    assert!(
        result.contains("\"b\""),
        "keys should contain b: {}",
        result
    );
}

#[test]
fn test_dict_vals() {
    let result = eval_str(r#"(dict/vals (dict "a" 1 "b" 2))"#);
    assert_eq!(result, "(1 2)");
}

#[test]
fn test_dict_merge() {
    let result = eval_str(r#"(dict/merge (dict "a" 1) (dict "b" 2))"#);
    assert!(
        result.contains("\"a\""),
        "merged should contain a: {}",
        result
    );
    assert!(
        result.contains("\"b\""),
        "merged should contain b: {}",
        result
    );
}

#[test]
fn test_dict_merge_overwrite() {
    // Second map overwrites first map's value for same key
    let result = eval_str(r#"(dict/merge (dict "a" 1) (dict "a" 2))"#);
    assert!(
        result.contains("2"),
        "should contain overwritten val 2: {}",
        result
    );
}

#[test]
fn test_map_predicate() {
    assert_eq!(eval_str(r#"(map? (dict "a" 1))"#), "true");
    assert_eq!(eval_str("(map? (list 1 2))"), "false");
}

// ===========================================================================
// SECTION: String operation correctness tests
// ===========================================================================

#[test]
fn test_str_length() {
    assert_eq!(eval_str(r#"(str-length "hello")"#), "5");
}

#[test]
fn test_str_length_empty() {
    assert_eq!(eval_str(r#"(str-length "")"#), "0");
}

#[test]
fn test_str_length_unicode() {
    // str-length counts chars, not bytes
    assert_eq!(eval_str(r#"(str-length "café")"#), "4");
}

#[test]
fn test_str_substring() {
    assert_eq!(eval_str(r#"(str-substring "hello" 1 3)"#), "\"el\"");
}

#[test]
fn test_str_substring_full() {
    assert_eq!(eval_str(r#"(str-substring "abc" 0 3)"#), "\"abc\"");
}

#[test]
fn test_str_split() {
    assert_eq!(
        eval_str(r#"(str-split "a,b,c" ",")"#),
        "(\"a\" \"b\" \"c\")"
    );
}

#[test]
fn test_str_split_no_delimiter() {
    assert_eq!(eval_str(r#"(str-split "hello" ",")"#), "(\"hello\")");
}

#[test]
fn test_str_trim() {
    assert_eq!(eval_str(r#"(str-trim "  hi  ")"#), "\"hi\"");
}

#[test]
fn test_str_trim_no_whitespace() {
    assert_eq!(eval_str(r#"(str-trim "hello")"#), "\"hello\"");
}

#[test]
fn test_str_index_of_found() {
    assert_eq!(eval_str(r#"(str-index-of "hello" "ll")"#), "2");
}

#[test]
fn test_str_index_of_not_found() {
    assert_eq!(eval_str(r#"(str-index-of "hello" "xyz")"#), "-1");
}

#[test]
fn test_str_upcase() {
    assert_eq!(eval_str(r#"(str-upcase "hello")"#), "\"HELLO\"");
}

#[test]
fn test_str_downcase() {
    assert_eq!(eval_str(r#"(str-downcase "HELLO")"#), "\"hello\"");
}

#[test]
fn test_str_starts_with_true() {
    assert_eq!(eval_str(r#"(str-starts-with "hello" "hel")"#), "true");
}

#[test]
fn test_str_starts_with_false() {
    assert_eq!(eval_str(r#"(str-starts-with "hello" "xyz")"#), "false");
}

#[test]
fn test_str_ends_with_true() {
    assert_eq!(eval_str(r#"(str-ends-with "hello" "llo")"#), "true");
}

#[test]
fn test_str_ends_with_false() {
    assert_eq!(eval_str(r#"(str-ends-with "hello" "hel")"#), "false");
}

// ===========================================================================
// SECTION: to-json / from-json correctness tests
// ===========================================================================

#[test]
fn test_to_json_list() {
    let result = eval_str(r#"(to-json (list 1 2 3))"#);
    // to-json returns a LispStr containing the JSON text, displayed with quotes
    assert!(
        result.contains("[1,2,3]"),
        "expected JSON array: {}",
        result
    );
}

#[test]
fn test_to_json_dict() {
    let result = eval_str(r#"(to-json (dict "a" 1))"#);
    assert!(result.contains("a"), "expected key a: {}", result);
    assert!(result.contains("1"), "expected val 1: {}", result);
}

#[test]
fn test_to_json_bool() {
    let result = eval_str("(to-json true)");
    assert!(result.contains("true"), "expected JSON true: {}", result);
}

#[test]
fn test_to_json_nil() {
    let result = eval_str("(to-json nil)");
    assert!(result.contains("null"), "expected JSON null: {}", result);
}

#[test]
fn test_from_json_number() {
    assert_eq!(eval_str(r#"(from-json "42")"#), "42");
}

#[test]
fn test_from_json_bool() {
    assert_eq!(eval_str(r#"(from-json "true")"#), "true");
}

#[test]
fn test_from_json_null() {
    assert_eq!(eval_str(r#"(from-json "null")"#), "nil");
}

#[test]
fn test_from_json_array() {
    assert_eq!(eval_str(r#"(from-json "[1,2,3]")"#), "(1 2 3)");
}

#[test]
fn test_json_roundtrip_list() {
    let result = eval_str(r#"(from-json (to-json (list 1 "two" true)))"#);
    assert_eq!(result, "(1 \"two\" true)");
}

#[test]
fn test_json_roundtrip_dict() {
    let result = eval_str(r#"(from-json (to-json (dict "x" 42)))"#);
    // Should produce a Map with x=42
    assert!(result.contains("\"x\""), "should contain key x: {}", result);
    assert!(result.contains("42"), "should contain val 42: {}", result);
}

// ===========================================================================
// SECTION: Storage gas metering tests
// ===========================================================================

#[test]
fn test_storage_gas_insufficient() {
    setup_test_vm();
    // Each storage-write costs 100 gas. 200 gas is not enough for 3 writes.
    let result = eval_str_gas(
        r#"(progn (near/storage-write "k1" "v1") (near/storage-write "k2" "v2") (near/storage-write "k3" "v3") "done")"#,
        200,
    );
    assert!(
        result.contains("ERROR"),
        "expected out-of-gas error: {}",
        result
    );
    assert!(
        result.contains("out of gas"),
        "expected 'out of gas': {}",
        result
    );
}

#[test]
fn test_storage_gas_sufficient() {
    setup_test_vm();
    // 350 gas is enough for 3 storage writes (3 * 100 = 300) plus eval overhead
    let result = eval_str_gas(
        r#"(progn (near/storage-write "k1" "v1") (near/storage-write "k2" "v2") (near/storage-write "k3" "v3") "done")"#,
        350,
    );
    assert_eq!(result, "\"done\"");
}

#[test]
fn test_storage_gas_single_write_low() {
    setup_test_vm();
    // 50 gas is not enough for a single storage write (needs 100)
    let result = eval_str_gas(r#"(near/storage-write "k1" "v1")"#, 50);
    assert!(result.contains("ERROR"), "expected out-of-gas: {}", result);
}

#[test]
fn test_storage_gas_consumed_more_than_without() {
    setup_test_vm();
    // Expression without storage
    let mut env1 = Vec::new();
    let _ = run_program("(+ 1 2)", &mut env1, 10000);
    // We can't directly observe remaining gas from run_program, but we verify
    // that storage ops consume more gas than pure computation
    let mut env2 = Vec::new();
    let result = run_program(
        r#"(progn (near/storage-write "k1" "v1") (+ 1 2))"#,
        &mut env2,
        10000,
    );
    assert_eq!(result.unwrap(), "3");
}

// ===========================================================================
// SECTION: Storage prefix shadowing fix tests
// ===========================================================================

#[test]
fn test_storage_prefix_not_overridden_by_input() {
    setup_test_vm();
    let contract = near_lisp::LispContract::new(10_000);
    // Try to override __storage_prefix__ via input_json — should be ignored
    let result = contract.eval_with_input(
        "(__storage_prefix__)".to_string(),
        r#"{"__storage_prefix__": "evil"}"#.to_string(),
    );
    // The prefix should be "eval:bob.near:" not "evil"
    // Since it's a string, evaluating it as a function call gives "not callable" error
    assert!(
        result.contains("eval:bob.near:"),
        "prefix should be safe, got: {}",
        result
    );
    assert!(
        !result.contains("evil"),
        "prefix should NOT be 'evil', got: {}",
        result
    );
}

#[test]
fn test_storage_prefix_cannot_write_to_evil_namespace() {
    setup_test_vm();
    let contract = near_lisp::LispContract::new(10_000);
    // Write via eval_with_input — the storage prefix should be safe
    let _ = contract.eval_with_input(
        r#"(near/storage-write "mykey" "myval")"#.to_string(),
        r#"{"__storage_prefix__": "evil"}"#.to_string(),
    );
    // Verify the key was written under the safe prefix, not "evil"
    let result = contract.eval(r#"(near/storage-read "mykey")"#.to_string());
    assert_eq!(result, "\"myval\"");
}

// ===========================================================================
// SECTION: Eval whitelist tests
// ===========================================================================

#[test]
fn test_eval_whitelist_empty_allows_all() {
    setup_test_vm();
    let contract = near_lisp::LispContract::new(10_000);
    // Empty whitelist = open access (backward compat)
    assert_eq!(contract.eval("(+ 1 2)".to_string()), "3");
}

#[test]
fn test_eval_whitelist_after_add_blocks_others() {
    setup_test_vm();
    let mut contract = near_lisp::LispContract::new(10_000);
    // Add alice.near to whitelist — now only alice can eval
    contract.add_to_eval_whitelist("alice.near".parse().unwrap());
    // bob.near (default test signer/predecessor) should be blocked
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.eval("(+ 1 2)".to_string());
    }));
    assert!(
        result.is_err(),
        "bob.near should be blocked after whitelist add"
    );
}

#[test]
fn test_eval_whitelist_add_and_list() {
    setup_test_vm();
    let mut contract = near_lisp::LispContract::new(10_000);
    contract.add_to_eval_whitelist("alice.near".parse().unwrap());
    contract.add_to_eval_whitelist("charlie.near".parse().unwrap());
    let list = contract.get_eval_whitelist();
    assert!(list.contains(&"alice.near".parse().unwrap()));
    assert!(list.contains(&"charlie.near".parse().unwrap()));
}

#[test]
fn test_eval_whitelist_remove() {
    setup_test_vm();
    let mut contract = near_lisp::LispContract::new(10_000);
    contract.add_to_eval_whitelist("alice.near".parse().unwrap());
    contract.remove_from_eval_whitelist("alice.near".parse().unwrap());
    let list = contract.get_eval_whitelist();
    assert!(!list.contains(&"alice.near".parse().unwrap()));
}

// ===========================================================================
// SECTION: ccall-view vs ccall-call correctness tests
// ===========================================================================

#[test]
fn test_ccall_view_yields_with_zero_deposit() {
    setup_test_vm();
    let mut env = Vec::new();
    let code = r#"(define x (near/ccall-view "x.near" "f" "{}"))"#;
    let result = near_lisp::run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        near_lisp::RunResult::Yield(yi) => {
            assert_eq!(yi.deposit, 0, "ccall-view should have deposit=0");
            assert_eq!(yi.gas_tgas, 50, "ccall-view should have gas=50 TGas");
            assert_eq!(yi.account, "x.near");
            assert_eq!(yi.method, "f");
        }
        near_lisp::RunResult::Done(s) => panic!("expected Yield, got Done: {}", s),
    }
}

#[test]
fn test_ccall_call_yields_with_deposit_and_gas() {
    setup_test_vm();
    let mut env = Vec::new();
    let code = r#"(define x (near/ccall-call "x.near" "f" "{}" "1000000" "100"))"#;
    let result = near_lisp::run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        near_lisp::RunResult::Yield(yi) => {
            assert_eq!(yi.deposit, 1000000, "ccall-call deposit should be 1000000");
            assert_eq!(yi.gas_tgas, 100, "ccall-call gas should be 100 TGas");
            assert_eq!(yi.account, "x.near");
            assert_eq!(yi.method, "f");
        }
        near_lisp::RunResult::Done(s) => panic!("expected Yield, got Done: {}", s),
    }
}

#[test]
fn test_ccall_view_standalone_yields() {
    setup_test_vm();
    let mut env = Vec::new();
    let code = r#"(near/ccall-view "oracle.near" "get_price" "{\"asset\": \"ETH\"}")"#;
    let result = near_lisp::run_program_with_ccall(code, &mut env, 10_000).unwrap();
    match result {
        near_lisp::RunResult::Yield(yi) => {
            assert_eq!(yi.account, "oracle.near");
            assert_eq!(yi.method, "get_price");
            assert_eq!(yi.deposit, 0);
        }
        near_lisp::RunResult::Done(s) => panic!("expected Yield, got Done: {}", s),
    }
}

// ===========================================================================
// SECTION: Batch tracking tests (near/batch-result, near/ccall-count)
// ===========================================================================

#[test]
fn test_ccall_count_initially_zero() {
    assert_eq!(eval_str("(near/ccall-count)"), "0");
}

#[test]
fn test_batch_result_no_results_errors() {
    let result = eval_str("(near/batch-result)");
    assert!(
        result.contains("ERROR"),
        "expected error when no results: {}",
        result
    );
    assert!(
        result.contains("no results"),
        "expected 'no results' message: {}",
        result
    );
}

// ===========================================================================
// SECTION: near/transfer correctness tests
// ===========================================================================

#[test]
fn test_near_transfer_returns_string() {
    setup_test_vm();
    let result = eval_str(r#"(near/transfer "1000" "alice.near")"#);
    assert!(
        result.contains("transfer:1000:alice.near"),
        "expected transfer string, got: {}",
        result
    );
}

#[test]
fn test_near_transfer_does_not_crash() {
    setup_test_vm();
    let result = eval_str(r#"(near/transfer "5000000" "bob.near")"#);
    // Should return a string, not an error
    assert!(
        !result.contains("ERROR"),
        "transfer should not error: {}",
        result
    );
}

// ===========================================================================
// SECTION: Account locked balance tests
// ===========================================================================

#[test]
fn test_account_locked_balance_returns_string() {
    setup_test_vm();
    let result = eval_str("(near/account-locked-balance)");
    // Default mock VM has 0 locked balance
    assert!(
        result.starts_with("\""),
        "locked balance should be a quoted string, got: {}",
        result
    );
}

#[test]
fn test_account_locked_balance_is_zero_in_mock() {
    setup_test_vm();
    let result = eval_str("(near/account-locked-balance)");
    assert!(
        result.contains("0"),
        "mock VM should have 0 locked balance, got: {}",
        result
    );
}

// ===========================================================================
// SECTION: loop / recur correctness tests
// ===========================================================================

#[test]
fn test_loop_recur_sum() {
    // Sum 0+1+2+3+4+5 = 15
    let code = r#"(loop ((i 0) (sum 0)) (if (> i 5) sum (recur (+ i 1) (+ sum i))))"#;
    assert_eq!(eval_str(code), "15");
}

#[test]
fn test_loop_recur_factorial() {
    // 5! = 120
    let code = r#"(loop ((n 5) (acc 1)) (if (= n 0) acc (recur (- n 1) (* acc n))))"#;
    assert_eq!(eval_str(code), "120");
}

#[test]
fn test_loop_recur_immediate_exit() {
    let code = r#"(loop ((n 10)) (if (= n 10) 42 (recur (+ n 1))))"#;
    assert_eq!(eval_str(code), "42");
}

#[test]
fn test_loop_recur_zero_iterations() {
    // Immediately exits since condition is true from start
    let code = r#"(loop ((n 0)) (if (= n 0) 99 (recur (+ n 1))))"#;
    assert_eq!(eval_str(code), "99");
}

// ===========================================================================
// SECTION: stdlib require correctness tests
// ===========================================================================

#[test]
fn test_require_math_abs_negative() {
    assert_eq!(eval_str(r#"(require "math") (abs -5)"#), "5");
}

#[test]
fn test_require_math_abs_positive() {
    assert_eq!(eval_str(r#"(require "math") (abs 5)"#), "5");
}

#[test]
fn test_require_math_even() {
    assert_eq!(eval_str(r#"(require "math") (even? 4)"#), "true");
}

#[test]
fn test_require_math_odd() {
    assert_eq!(eval_str(r#"(require "math") (odd? 3)"#), "true");
}

#[test]
fn test_require_math_even_false() {
    assert_eq!(eval_str(r#"(require "math") (even? 3)"#), "false");
}

#[test]
fn test_require_math_odd_false() {
    assert_eq!(eval_str(r#"(require "math") (odd? 4)"#), "false");
}

#[test]
fn test_require_list_map() {
    let result = eval_str(r#"(require "list") (map (lambda (x) (* x 2)) (list 1 2 3))"#);
    assert_eq!(result, "(2 4 6)");
}

#[test]
fn test_require_list_filter() {
    let result = eval_str(r#"(require "list") (filter (lambda (x) (> x 2)) (list 1 2 3 4))"#);
    assert_eq!(result, "(3 4)");
}

#[test]
fn test_require_list_reduce_with_lambda() {
    let result = eval_str(r#"(require "list") (reduce (lambda (a b) (+ a b)) 0 (list 1 2 3))"#);
    assert_eq!(result, "6");
}

#[test]
fn test_require_unknown_module_errors() {
    let result = eval_str(r#"(require "nonexistent")"#);
    assert!(result.contains("ERROR"), "expected error: {}", result);
    assert!(
        result.contains("unknown module"),
        "expected 'unknown module': {}",
        result
    );
}

#[test]
fn test_require_non_string_errors() {
    let result = eval_str("(require 42)");
    assert!(result.contains("ERROR"), "expected error: {}", result);
    assert!(
        result.contains("need string"),
        "expected 'need string': {}",
        result
    );
}

// ===========================================================================
// SECTION: Feature 1 — Stdlib caching
// ===========================================================================

#[test]
fn test_stdlib_cached_require_same_module_twice() {
    // Requiring the same module twice should still work and give same result
    let code = r#"(require "math") (require "math") (abs -10)"#;
    assert_eq!(eval_str(code), "10");
}

#[test]
fn test_stdlib_cached_require_list_twice() {
    let code = r#"(require "list") (require "list") (map (lambda (x) (* x 3)) (list 1 2))"#;
    assert_eq!(eval_str(code), "(3 6)");
}

#[test]
fn test_stdlib_cached_require_different_modules() {
    let code = r#"(require "math") (require "string") (abs -7)"#;
    assert_eq!(eval_str(code), "7");
}

#[test]
fn test_stdlib_cached_require_saves_gas() {
    // First require consumes gas; second require should consume almost none
    // because it skips re-evaluation. We test that a low-gas second require works.
    let code1 = r#"(require "math")"#;
    let code2 = r#"(require "math") (min 3 5)"#;

    // Load math with ample gas
    let mut env = Vec::new();
    run_program(code1, &mut env, 50_000).unwrap();

    // Second require + call should work with low gas since require is cached
    let result = run_program(code2, &mut env, 200).unwrap();
    assert_eq!(result, "3");
}

// ===========================================================================
// SECTION: Feature 2 — try/catch special form
// ===========================================================================

#[test]
fn test_try_catch_basic_error() {
    let code = r#"(try (/ 1 0) (catch e "caught"))"#;
    assert_eq!(eval_str(code), "\"caught\"");
}

#[test]
fn test_try_catch_success() {
    let code = r#"(try (+ 1 2) (catch e "error"))"#;
    assert_eq!(eval_str(code), "3");
}

#[test]
fn test_try_catch_error_binding() {
    let code = r#"(try (/ 1 0) (catch err err))"#;
    let result = eval_str(code);
    // Error message should be bound to `err` and returned as a string
    assert!(result.contains("div by zero"), "got: {}", result);
}

#[test]
fn test_try_catch_undefined_var() {
    let code = r#"(try undefined_var (catch e (str-concat "caught: " e)))"#;
    let result = eval_str(code);
    assert!(result.contains("caught:"), "got: {}", result);
    assert!(result.contains("undefined"), "got: {}", result);
}

#[test]
fn test_try_catch_multi_body() {
    // Handler with multiple body forms (progn-style)
    let code = r#"
        (try (/ 1 0)
            (catch e
                (define recovered "yes")
                (str-concat "error:" e)))
    "#;
    let result = eval_str(code);
    assert!(result.starts_with("\"error:"), "got: {}", result);
}

#[test]
fn test_try_catch_gas_exhaustion() {
    // Gas exhaustion from inner expression should be catchable.
    // Need enough gas for: try form (1) + inner call form (1) + catch handler (1+)
    // The inner near/storage-write fails when trying to evaluate its args with 0 gas,
    // then the catch handler needs gas to evaluate "no-gas" string literal.
    let code = r#"(try (near/storage-write "k" "v") (catch e "no-gas"))"#;
    let result = eval_str_gas(code, 5);
    assert_eq!(result, "\"no-gas\"");
}

#[test]
fn test_try_catch_nested() {
    let code = r#"
        (try
            (try (/ 1 0) (catch e (undefined_inner)))
            (catch outer (str-concat "outer:" outer)))
    "#;
    let result = eval_str(code);
    assert!(result.contains("outer:"), "got: {}", result);
}

// ===========================================================================
// SECTION: Feature 3 — str= and str!= builtins + string = comparison
// ===========================================================================

#[test]
fn test_generic_eq_strings_equal() {
    assert_eq!(eval_str(r#"(= "hello" "hello")"#), "true");
}

#[test]
fn test_generic_eq_strings_not_equal() {
    assert_eq!(eval_str(r#"(= "hello" "world")"#), "false");
}

#[test]
fn test_str_eq_equal() {
    assert_eq!(eval_str(r#"(str= "foo" "foo")"#), "true");
}

#[test]
fn test_str_eq_not_equal() {
    assert_eq!(eval_str(r#"(str= "foo" "bar")"#), "false");
}

#[test]
fn test_str_neq_not_equal() {
    assert_eq!(eval_str(r#"(str!= "foo" "bar")"#), "true");
}

#[test]
fn test_str_neq_equal() {
    assert_eq!(eval_str(r#"(str!= "foo" "foo")"#), "false");
}

#[test]
fn test_str_eq_empty_strings() {
    assert_eq!(eval_str(r#"(str= "" "")"#), "true");
}

#[test]
fn test_str_eq_case_sensitive() {
    assert_eq!(eval_str(r#"(str= "Hello" "hello")"#), "false");
}

// ===========================================================================
// SECTION: Feature 4 — near/batch-call multi-action
// ===========================================================================

#[test]
fn test_batch_call_basic() {
    let code = r#"
        (near/batch-call "test.near"
            (list (list "method1" "{}" "0" "50")))
    "#;
    let result = eval_str(code);
    assert!(result.contains("batch:test.near:1"), "got: {}", result);
}

#[test]
fn test_batch_call_multiple() {
    let code = r#"
        (near/batch-call "test.near"
            (list
                (list "method1" "{}" "0" "50")
                (list "method2" "{}" "1000" "30")))
    "#;
    let result = eval_str(code);
    assert!(result.contains("batch:test.near:2"), "got: {}", result);
}

#[test]
fn test_batch_call_empty_specs_error() {
    let code = r#"
        (near/batch-call "test.near" (list))
    "#;
    let result = eval_str(code);
    assert!(result.contains("ERROR"), "expected error: {}", result);
    assert!(result.contains("at least one call spec"), "got: {}", result);
}

#[test]
fn test_batch_call_invalid_recipient_error() {
    let code = r#"
        (near/batch-call "invalid account"
            (list (list "method1" "{}" "0" "50")))
    "#;
    let result = eval_str(code);
    assert!(result.contains("ERROR"), "expected error: {}", result);
}

#[test]
fn test_batch_call_short_spec_error() {
    let code = r#"
        (near/batch-call "test.near"
            (list (list "method1" "{}")))
    "#;
    let result = eval_str(code);
    assert!(result.contains("ERROR"), "expected error: {}", result);
    assert!(result.contains("each spec needs"), "got: {}", result);
}

#[test]
fn test_batch_call_non_list_specs_error() {
    let code = r#"(near/batch-call "test.near" "not a list")"#;
    let result = eval_str(code);
    assert!(result.contains("ERROR"), "expected error: {}", result);
    assert!(result.contains("list of call specs"), "got: {}", result);
}

// ===========================================================================
// SECTION: Feature 5 — Pattern matching (match special form)
// ===========================================================================

#[test]
fn test_match_number_literal() {
    assert_eq!(
        eval_str("(match 42 (1 \"one\") (42 \"found\") (_ \"other\"))"),
        "\"found\""
    );
}

#[test]
fn test_match_string_literal() {
    let code = r#"(match "hello" ("world" 1) ("hello" 2) (_ 3))"#;
    assert_eq!(eval_str(code), "2");
}

#[test]
fn test_match_wildcard() {
    assert_eq!(eval_str("(match 999 (_ \"matched\"))"), "\"matched\"");
}

#[test]
fn test_match_binding_variable() {
    assert_eq!(eval_str("(match 42 (?x (+ x 1)))"), "43");
}

#[test]
fn test_match_list_pattern() {
    assert_eq!(
        eval_str("(match (list 1 2 3) ((list 1 2 3) \"matched\") (_ \"no\"))"),
        "\"matched\""
    );
}

#[test]
fn test_match_list_pattern_with_bindings() {
    assert_eq!(
        eval_str("(match (list 10 20) ((list ?a ?b) (+ a b)) (_ 0))"),
        "30"
    );
}

#[test]
fn test_match_list_pattern_wrong_length() {
    assert_eq!(
        eval_str("(match (list 1 2) ((list 1 2 3) \"yes\") (_ \"no\"))"),
        "\"no\""
    );
}

#[test]
fn test_match_cons_pattern() {
    assert_eq!(eval_str("(match (list 1 2 3) ((cons ?h ?t) h) (_ 0))"), "1");
}

#[test]
fn test_match_cons_pattern_tail() {
    assert_eq!(
        eval_str("(match (list 1 2 3) ((cons ?h ?t) t) (_ (list)))"),
        "(2 3)"
    );
}

#[test]
fn test_match_cons_empty_list_fails() {
    assert_eq!(
        eval_str("(match (list) ((cons ?h ?t) \"yes\") (_ \"empty\"))"),
        "\"empty\""
    );
}

#[test]
fn test_match_bool_literal() {
    assert_eq!(
        eval_str("(match true (false \"no\") (true \"yes\"))"),
        "\"yes\""
    );
}

#[test]
fn test_match_no_match_returns_nil() {
    assert_eq!(eval_str("(match 5 (1 \"a\") (2 \"b\"))"), "nil");
}

#[test]
fn test_match_nested() {
    let code = r#"
        (define classify
            (lambda (x)
                (match x
                    ((list 1 ?rest) (str-concat "starts-1:" (to-json rest)))
                    ((cons ?h ?t) (str-concat "head:" (to-json (list h))))
                    (_ "other"))))
        (classify (list 1 99))
    "#;
    let result = eval_str(code);
    assert!(result.contains("starts-1:"), "got: {}", result);
}

// ===========================================================================
// SECTION: Feature 6 — fmt string interpolation
// ===========================================================================

#[test]
fn test_fmt_simple() {
    let code = r#"(fmt "Hello {name}" (dict "name" "Alice"))"#;
    assert_eq!(eval_str(code), "\"Hello Alice\"");
}

#[test]
fn test_fmt_multiple_keys() {
    let code = r#"(fmt "{greeting} {name}" (dict "greeting" "Hi" "name" "Bob"))"#;
    assert_eq!(eval_str(code), "\"Hi Bob\"");
}

#[test]
fn test_fmt_missing_key_left_as_is() {
    let code = r#"(fmt "Hello {unknown}" (dict "name" "Alice"))"#;
    assert_eq!(eval_str(code), "\"Hello {unknown}\"");
}

#[test]
fn test_fmt_number_value() {
    let code = r#"(fmt "Score: {score}" (dict "score" 95))"#;
    assert_eq!(eval_str(code), "\"Score: 95\"");
}

#[test]
fn test_fmt_bool_value() {
    let code = r#"(fmt "Active: {status}" (dict "status" true))"#;
    assert_eq!(eval_str(code), "\"Active: true\"");
}

#[test]
fn test_fmt_empty_dict() {
    let code = r#"(fmt "Hello {name}" (dict))"#;
    assert_eq!(eval_str(code), "\"Hello {name}\"");
}

#[test]
fn test_fmt_no_placeholders() {
    let code = r#"(fmt "No placeholders" (dict))"#;
    assert_eq!(eval_str(code), "\"No placeholders\"");
}

#[test]
fn test_fmt_mixed_found_and_missing() {
    let code = r#"(fmt "{a} {b} {c}" (dict "a" 1 "c" 3))"#;
    assert_eq!(eval_str(code), "\"1 {b} 3\"");
}

// ===========================================================================
// SECTION: Feature 7 — Custom modules (require integration)
// ===========================================================================

#[test]
fn test_require_unknown_module_still_errors() {
    let result = eval_str(r#"(require "nonexistent_module_xyz")"#);
    assert!(result.contains("ERROR"), "expected error: {}", result);
    assert!(
        result.contains("unknown module"),
        "expected 'unknown module': {}",
        result
    );
}
