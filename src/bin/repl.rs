use std::io::{self, Write};
use std::panic::{self, AssertUnwindSafe};

fn count_open_parens(line: &str) -> i64 {
    let mut depth: i64 = 0;
    let mut in_string = false;
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if in_string {
            if ch == '\\' && i + 1 < chars.len() {
                i += 2; // skip escaped char
                continue;
            }
            if ch == '"' {
                in_string = false;
            }
        } else if ch == '"' {
            in_string = true;
        } else if ch == ';' && i + 1 < chars.len() && chars[i + 1] == ';' {
            break; // line comment — rest of line ignored
        } else if ch == '(' {
            depth += 1;
        } else if ch == ')' {
            depth -= 1;
        }
        i += 1;
    }
    depth
}

fn main() {
    let mut env: Vec<(String, near_lisp::LispVal)> = Vec::new();
    let mut gas_limit: u64 = 10_000_000;
    let mut input_buf = String::new();

    println!("near-lisp REPL v0.2.0");
    println!("Type :help for commands, :quit to exit.");

    let stdin = io::stdin();
    loop {
        // Prompt
        if input_buf.is_empty() {
            print!("lisp> ");
        } else {
            print!("  ... ");
        }
        io::stdout().flush().unwrap();

        let mut line = String::new();
        match stdin.read_line(&mut line) {
            Ok(0) => {
                // EOF
                println!();
                break;
            }
            Ok(_) => {}
            Err(e) => {
                eprintln!("Input error: {}", e);
                break;
            }
        }

        let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');

        // Handle commands only at the top level (no open parens)
        if input_buf.is_empty() {
            match trimmed {
                ":quit" | ":q" => {
                    println!("Bye!");
                    break;
                }
                ":help" | ":h" => {
                    println!("Commands:");
                    println!("  :help, :h    Show this help");
                    println!("  :quit, :q    Exit the REPL");
                    println!("  :gas N       Set gas limit (current: {})", gas_limit);
                    println!("  :env         Show the current environment bindings");
                    println!("  :reset       Clear the environment");
                    println!();
                    println!("Enter Lisp expressions. Multi-line input is supported");
                    println!("by leaving open parentheses — the REPL will continue");
                    println!("reading until all parens are closed.");
                    continue;
                }
                ":env" => {
                    if env.is_empty() {
                        println!("; environment is empty");
                    } else {
                        for (k, v) in &env {
                            println!("  {} = {}", k, v);
                        }
                    }
                    continue;
                }
                ":reset" => {
                    env.clear();
                    println!("; environment cleared");
                    continue;
                }
                "" => continue,
                _ => {}
            }

            // Handle :gas N command
            if trimmed.starts_with(":gas ") {
                if let Ok(n) = trimmed[5..].trim().parse::<u64>() {
                    gas_limit = n;
                    println!("; gas limit set to {}", gas_limit);
                } else {
                    eprintln!("; invalid gas value: {}", &trimmed[5..]);
                }
                continue;
            }
        }

        // Skip blank lines inside multi-line (do nothing)
        if trimmed.is_empty() {
            continue;
        }

        // Accumulate multi-line input
        if !input_buf.is_empty() {
            input_buf.push('\n');
        }
        input_buf.push_str(trimmed);

        // Check if parens are balanced
        let net_depth = count_open_parens(&input_buf);
        if net_depth > 0 {
            // More open parens — continue reading
            continue;
        }

        // We have a complete expression(s). Evaluate.
        let code = std::mem::take(&mut input_buf);

        let result = panic::catch_unwind(AssertUnwindSafe(|| {
            near_lisp::run_program(&code, &mut env, gas_limit)
        }));

        match result {
            Ok(Ok(val)) => {
                println!("{}", val);
            }
            Ok(Err(e)) => {
                eprintln!("Error: {}", e);
            }
            Err(panic_payload) => {
                let msg = if let Some(s) = panic_payload.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_payload.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic".to_string()
                };
                eprintln!("Panic (NEAR builtins not available in REPL): {}", msg);
            }
        }
    }
}
