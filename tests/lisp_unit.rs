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
    assert_eq!(eval_str("(define square (lambda (n) (* n n))) (square 5)"), "25");
    assert_eq!(eval_str("(define add (lambda (a b) (+ a b))) (add 3 4)"), "7");
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
