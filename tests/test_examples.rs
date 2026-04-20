use near_lisp::{parse_all, run_program, Env};

fn make_env() -> Env {
    Env::new()
}

fn parse_file(path: &str) -> Result<(), String> {
    let code = std::fs::read_to_string(path).map_err(|e| format!("read error: {}", e))?;
    parse_all(&code)?;
    Ok(())
}

fn run_file(path: &str, gas: u64) -> Result<String, String> {
    let code = std::fs::read_to_string(path).map_err(|e| format!("read error: {}", e))?;
    let mut env = make_env();
    run_program(&code, &mut env, gas)
}

// Files that should parse and run successfully in test mode
macro_rules! run_test {
    ($name:ident, $path:expr, $gas:expr) => {
        #[test]
        fn $name() {
            let result = run_file($path, $gas);
            if let Err(ref e) = result {
                eprintln!("FAIL {}: {}", stringify!($name), e);
            }
            assert!(result.is_ok(), "Example {} failed: {:?}", $path, result);
        }
    };
}

// Parse-only tests (for files with NEAR builtins that panic outside runtime)
macro_rules! parse_test {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let result = parse_file($path);
            if let Err(ref e) = result {
                eprintln!("FAIL {}: {}", stringify!($name), e);
            }
            assert!(result.is_ok(), "Parse of {} failed: {:?}", $path, result);
        }
    };
}

run_test!(test_ex01_basics, "examples/01-basics.lisp", 50_000);
run_test!(test_ex02_variables, "examples/02-variables.lisp", 50_000);
run_test!(test_ex03_conditionals, "examples/03-conditionals.lisp", 50_000);
run_test!(test_ex04_lambdas, "examples/04-lambdas.lisp", 50_000);
run_test!(test_ex05_lists, "examples/05-lists.lisp", 200_000);
run_test!(test_ex06_recursion, "examples/06-recursion.lisp", 200_000);
run_test!(test_ex07_pattern_matching, "examples/07-pattern-matching.lisp", 100_000);
run_test!(test_ex08_error_handling, "examples/08-error-handling.lisp", 100_000);
run_test!(test_ex09_stdlib_math, "examples/09-stdlib-math.lisp", 200_000);
run_test!(test_ex10_stdlib_string, "examples/10-stdlib-string.lisp", 200_000);
run_test!(test_ex11_stdlib_crypto, "examples/11-stdlib-crypto.lisp", 50_000);

// 12-near-context uses NEAR builtins — parse only
parse_test!(test_ex12_near_context, "examples/12-near-context.lisp");

// 13-modules now defines functions inline (no external module needed)
run_test!(test_ex13_modules, "examples/13-modules.lisp", 200_000);

// 14-policies is mostly comments — parse only
parse_test!(test_ex14_policies, "examples/14-policies.lisp");

run_test!(test_ex15_progn, "examples/15-progn.lisp", 50_000);

// 16-cross-contract uses ccall — parse only
parse_test!(test_ex16_cross_contract, "examples/16-cross-contract.lisp");

run_test!(test_ex17_type_conversions, "examples/17-type-conversions.lisp", 50_000);
run_test!(test_ex18_gas, "examples/18-gas.lisp", 5_000_000);
run_test!(test_ex19_real_world, "examples/19-real-world.lisp", 2_000_000);