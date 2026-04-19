use near_lisp::*;

fn eval_str(code: &str) -> String {
    let mut env = Vec::new();
    run_program(code, &mut env, 10_000).unwrap_or_else(|e| format!("ERROR: {}", e))
}

fn eval_str_gas(code: &str, gas: u64) -> String {
    let mut env = Vec::new();
    run_program(code, &mut env, gas).unwrap_or_else(|e| format!("ERROR: {}", e))
}

fn setup_test_vm() {
    let context = near_sdk::test_utils::VMContextBuilder::new().build();
    near_sdk::testing_env!(context);
}

fn setup_contract() -> LispContract {
    setup_test_vm();
    LispContract::new(10_000)
}

// ===========================================================================
// SECTION: Bytes builtin tests
// ===========================================================================

#[test]
fn test_hex_to_bytes() {
    assert_eq!(eval_str(r#"(hex->bytes "deadbeef")"#), "0xdeadbeef");
}

#[test]
fn test_hex_to_bytes_with_prefix() {
    assert_eq!(eval_str(r#"(hex->bytes "0xff")"#), "0xff");
}

#[test]
fn test_bytes_hex_alias() {
    assert_eq!(eval_str(r#"(bytes-hex "aa")"#), "0xaa");
}

#[test]
fn test_bytes_to_hex() {
    // bytes->hex returns a Lisp String, so Display wraps in quotes
    assert_eq!(eval_str(r#"(bytes->hex (hex->bytes "0xdeadbeef"))"#), r#""0xdeadbeef""#);
}

#[test]
fn test_bytes_len() {
    assert_eq!(eval_str(r#"(bytes-len (hex->bytes "0xdeadbeef"))"#), "4");
}

#[test]
fn test_bytes_len_empty() {
    assert_eq!(eval_str(r#"(bytes-len (hex->bytes ""))"#), "0");
}

#[test]
fn test_bytes_to_string() {
    assert_eq!(eval_str(r#"(bytes->string (string->bytes "hello"))"#), r#""hello""#);
}

#[test]
fn test_string_to_bytes() {
    assert_eq!(eval_str(r#"(bytes->hex (string->bytes "AB"))"#), r#""0x4142""#);
}

#[test]
fn test_bytes_concat() {
    assert_eq!(
        eval_str(r#"(bytes->hex (bytes-concat (hex->bytes "0xff") (hex->bytes "0xaa")))"#),
        r#""0xffaa""#
    );
}

#[test]
fn test_bytes_slice() {
    assert_eq!(
        eval_str(r#"(bytes->hex (bytes-slice (hex->bytes "deadbeef") 1 3))"#),
        r#""0xadbe""#
    );
}

#[test]
fn test_bytes_type() {
    assert_eq!(eval_str(r#"(type? (hex->bytes "0xff"))"#), r#""bytes""#);
}

#[test]
fn test_bytes_roundtrip() {
    assert_eq!(eval_str(r#"(bytes->hex (hex->bytes "0x0102030405"))"#), r#""0x0102030405""#);
}

// ===========================================================================
// SECTION: Custom modules contract API tests
// ===========================================================================

#[test]
fn test_contract_save_and_get_module() {
    let mut contract = setup_contract();
    contract.save_module("test-mod".to_string(), "(define (m-double x) (* x 2))".to_string());
    let m = contract.get_module("test-mod".to_string());
    assert_eq!(m, Some("(define (m-double x) (* x 2))".to_string()));
}

#[test]
fn test_contract_get_module_missing() {
    let contract = setup_contract();
    let m = contract.get_module("nonexistent".to_string());
    assert_eq!(m, None);
}

#[test]
fn test_contract_list_modules() {
    let mut contract = setup_contract();
    contract.save_module("mod-a".to_string(), "(define a 1)".to_string());
    contract.save_module("mod-b".to_string(), "(define b 2)".to_string());
    let mut names = contract.list_modules();
    names.sort();
    assert_eq!(names, vec!["mod-a".to_string(), "mod-b".to_string()]);
}

#[test]
fn test_contract_remove_module() {
    let mut contract = setup_contract();
    contract.save_module("to-remove".to_string(), "(define x 1)".to_string());
    assert!(contract.get_module("to-remove".to_string()).is_some());
    contract.remove_module("to-remove".to_string());
    assert!(contract.get_module("to-remove".to_string()).is_none());
    assert!(!contract.list_modules().contains(&"to-remove".to_string()));
}

#[test]
fn test_contract_save_module_invalid_parse() {
    let mut contract = setup_contract();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contract.save_module("bad".to_string(), "(((".to_string());
    }));
    assert!(result.is_err());
}

// ===========================================================================
// SECTION: require namespace prefix tests
// ===========================================================================

#[test]
fn test_require_math_with_prefix() {
    let code = r#"
        (require "math" "m")
        (m/abs -10)
    "#;
    assert_eq!(eval_str(code), "10");
}

#[test]
fn test_require_math_prefix_max() {
    let code = r#"
        (require "math" "m")
        (m/max 3 7)
    "#;
    assert_eq!(eval_str(code), "7");
}

#[test]
fn test_require_prefix_caching() {
    let code = r#"
        (require "math" "m")
        (require "math" "m")
        (m/min 5 3)
    "#;
    assert_eq!(eval_str(code), "3");
}

// ===========================================================================
// SECTION: require "crypto" stdlib tests
// ===========================================================================

#[test]
fn test_require_crypto_sha256_bytes() {
    setup_test_vm();
    let code = r#"
        (require "crypto")
        (hash/sha256-bytes "hello")
    "#;
    let result = eval_str(code);
    assert!(result.contains("2cf24dba"), "got: {}", result);
}

#[test]
fn test_require_crypto_keccak256_bytes() {
    setup_test_vm();
    let code = r#"
        (require "crypto")
        (hash/keccak256-bytes "hello")
    "#;
    let result = eval_str(code);
    assert!(!result.contains("ERROR"), "got: {}", result);
    assert!(result.len() > 10, "should return hex hash, got: {}", result);
}

// ===========================================================================
// SECTION: storage_usage / storage_balance contract view tests
// ===========================================================================

#[test]
fn test_contract_storage_usage() {
    let mut contract = setup_contract();
    let usage_before = contract.storage_usage();
    contract.save_policy("test-storage-policy".to_string(), "(= 1 1)".to_string());
    let usage_after = contract.storage_usage();
    assert!(usage_after > usage_before, "storage usage should increase after write");
}

#[test]
fn test_contract_storage_balance() {
    let contract = setup_contract();
    let balance = contract.storage_balance();
    assert!(balance.contains("total"), "got: {}", balance);
    assert!(balance.contains("available"), "got: {}", balance);
    assert!(balance.contains("locked"), "got: {}", balance);
}

// ===========================================================================
// SECTION: near/predecessor and near/signer raw getter tests
// ===========================================================================

#[test]
fn test_near_predecessor_returns_string() {
    setup_test_vm();
    let result = eval_str("(near/predecessor)");
    assert_eq!(result, r#""bob.near""#, "got: {}", result);
}

#[test]
fn test_near_signer_returns_string() {
    setup_test_vm();
    let result = eval_str("(near/signer)");
    assert_eq!(result, r#""bob.near""#, "got: {}", result);
}

#[test]
fn test_near_predecessor_and_equals_consistent() {
    setup_test_vm();
    let pred = eval_str("(near/predecessor)");
    let eq_result = eval_str(r#"(near/predecessor= "bob.near")"#);
    assert_eq!(eq_result, "true");
    assert!(pred.contains("bob.near"), "pred={}", pred);
}

// ===========================================================================
// SECTION: eval_script_async tests
// ===========================================================================

#[test]
fn test_contract_eval_script_async() {
    let mut contract = setup_contract();
    contract.save_script("async-test".to_string(), "(+ 1 2)".to_string());
    let result = contract.eval_script_async("async-test".to_string());
    // Simple expression with no ccalls returns result directly
    assert!(result == "3" || result == "YIELDING", "got: {}", result);
}

#[test]
fn test_contract_eval_script_async_missing() {
    let mut contract = setup_contract();
    let result = contract.eval_script_async("nonexistent".to_string());
    assert!(result.contains("not found"), "got: {}", result);
}

// ===========================================================================
// SECTION: near/log test (with VM context)
// ===========================================================================

#[test]
fn test_near_log_with_vm() {
    setup_test_vm();
    let mut e = Vec::new();
    let r = run_program(r#"(near/log "test message")"#, &mut e, 10_000);
    assert_eq!(r.unwrap(), "nil");
}

// ===========================================================================
// SECTION: NEP-297 event emission tests
// ===========================================================================

fn assert_event(emitted_event: &str, expected_event: &str, expected_name: &str) {
    assert!(
        emitted_event.starts_with("EVENT_JSON:"),
        "not an EVENT_JSON log: {}",
        emitted_event
    );
    let json_str = &emitted_event["EVENT_JSON:".len()..];
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .unwrap_or_else(|e| panic!("invalid JSON in event: {} — {}", json_str, e));
    assert_eq!(parsed["standard"], "near-lisp");
    assert_eq!(parsed["version"], "1.0.0");
    assert_eq!(parsed["event"], expected_event);
    assert_eq!(parsed["data"]["name"], expected_name);
}

fn get_event_logs() -> Vec<String> {
    near_sdk::test_utils::get_logs()
        .into_iter()
        .filter(|l| l.starts_with("EVENT_JSON:"))
        .collect()
}

#[test]
fn test_nep297_save_policy_event() {
    let mut contract = setup_contract();
    contract.save_policy("test-evt-policy".to_string(), "(= 1 1)".to_string());
    let events = get_event_logs();
    assert!(!events.is_empty(), "no events emitted");
    assert_event(&events[0], "save_policy", "test-evt-policy");
}

#[test]
fn test_nep297_remove_policy_event() {
    let mut contract = setup_contract();
    contract.save_policy("to-remove-pol".to_string(), "(= 1 1)".to_string());
    let save_events = get_event_logs();
    assert!(!save_events.is_empty(), "no save events");
    contract.remove_policy("to-remove-pol".to_string());
    let all_events = get_event_logs();
    // The remove event is the last EVENT_JSON after the save event
    let remove_evt = all_events.iter().find(|e| e.contains("\"remove_policy\""))
        .expect("no remove_policy event found");
    assert_event(remove_evt, "remove_policy", "to-remove-pol");
}

#[test]
fn test_nep297_save_script_event() {
    let mut contract = setup_contract();
    contract.save_script("test-evt-script".to_string(), "(+ 1 2)".to_string());
    let events = get_event_logs();
    assert!(!events.is_empty(), "no events emitted");
    assert_event(&events[0], "save_script", "test-evt-script");
}

#[test]
fn test_nep297_remove_script_event() {
    let mut contract = setup_contract();
    contract.save_script("to-remove-scr".to_string(), "(+ 1 2)".to_string());
    let save_events = get_event_logs();
    assert!(!save_events.is_empty(), "no save events");
    contract.remove_script("to-remove-scr".to_string());
    let all_events = get_event_logs();
    let remove_evt = all_events
        .iter()
        .find(|e| e.contains("\"remove_script\""))
        .expect("no remove_script event found");
    assert_event(remove_evt, "remove_script", "to-remove-scr");
}

#[test]
fn test_nep297_save_module_event() {
    let mut contract = setup_contract();
    contract.save_module("test-evt-module".to_string(), "(define x 1)".to_string());
    let events = get_event_logs();
    assert!(!events.is_empty(), "no events emitted");
    assert_event(&events[0], "save_module", "test-evt-module");
}

#[test]
fn test_nep297_remove_module_event() {
    let mut contract = setup_contract();
    contract.save_module("to-remove-mod".to_string(), "(define x 1)".to_string());
    let save_events = get_event_logs();
    assert!(!save_events.is_empty(), "no save events");
    contract.remove_module("to-remove-mod".to_string());
    let all_events = get_event_logs();
    let remove_evt = all_events
        .iter()
        .find(|e| e.contains("\"remove_module\""))
        .expect("no remove_module event found");
    assert_event(remove_evt, "remove_module", "to-remove-mod");
}

#[test]
fn test_nep297_transfer_ownership_event() {
    let mut contract = setup_contract();
    let new_owner = "new-owner.near".parse().unwrap();
    contract.transfer_ownership(new_owner);
    let events = get_event_logs();
    assert!(!events.is_empty(), "no events emitted");
    let evt = &events[0];
    assert!(evt.starts_with("EVENT_JSON:"), "not EVENT_JSON: {}", evt);
    let json_str = &evt["EVENT_JSON:".len()..];
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
    assert_eq!(parsed["standard"], "near-lisp");
    assert_eq!(parsed["version"], "1.0.0");
    assert_eq!(parsed["event"], "transfer_ownership");
    assert_eq!(parsed["data"]["new_owner"], "new-owner.near");
}
