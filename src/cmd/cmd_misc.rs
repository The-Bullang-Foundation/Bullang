//! Miscellaneous commands: update, stdlib.

use bullang::stdlib;

/// The canonical source repository.
pub const DEFAULT_REPO: &str = "https://github.com/My-sidequests/Bullang.git";

// ── update ────────────────────────────────────────────────────────────────────

pub fn cmd_update() {
    println!("Updating bullang...");

    let remote = match remote_head(DEFAULT_REPO, "main") {
        Some(h) => h,
        None => {
            eprintln!("Could not reach repository. Check your internet connection.");
            return;
        }
    };

    let installed = installed_hash("bullang", DEFAULT_REPO, "main");

	if installed.map_or(false, |h| h == remote) {
        println!("Already up to date (commit: {}).", &remote[..8]);
        return;
    }

    let status = std::process::Command::new("cargo")
        .args(["install", "--git", DEFAULT_REPO, "--branch", "main", "--force", "bullang"])
        .status();

    match status {
        Ok(s) if s.success() => println!("Update complete."),
        Ok(s)  => eprintln!("cargo install exited with {}.", s),
        Err(e) => eprintln!("Failed to run cargo: {}.", e),
    }
}

/// Fetch the HEAD commit hash of `branch` from a remote git repository.
/// Returns the full 40-character SHA, or None if git is unavailable or the
/// repo cannot be reached.
pub fn remote_head(repo: &str, branch: &str) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["ls-remote", repo, &format!("refs/heads/{}", branch)])
        .output()
        .ok()?;

    let stdout = String::from_utf8(output.stdout).ok()?;
    let hash = stdout.split_whitespace().next()?;
    if hash.len() == 40 { Some(hash.to_string()) } else { None }
}

/// Read the commit hash for `package` as recorded in ~/.cargo/.crates2.json.
/// Returns the short hash stored by cargo (e.g. "aaec925f"), or None if not
/// found or the file cannot be parsed.
pub fn installed_hash(package: &str, repo: &str, branch: &str) -> Option<String> {
    let cargo_home = std::env::var("CARGO_installed_hashHOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".cargo")
        });

    let content = std::fs::read_to_string(
        cargo_home.join(".crates2.json")
    ).ok()?;

    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let installs = json["installs"].as_object()?;

    let repo_fragment = repo.trim_end_matches(".git");
    let branch_tag = format!("branch={}", branch);

    for key in installs.keys() {
        if key.contains(package)
            && key.contains(repo_fragment)
            && key.contains(&branch_tag)
        {
            // key = "bullang 1.0.0 (git+...?branch=main#e61e4db6c4c8...)"
            let hash = key.split('#').nth(1)?.trim_end_matches(')');
            return Some(hash.to_string());
        }
    }
    None
}

// ── stdlib ────────────────────────────────────────────────────────────────────

const MATH:       &[&str] = &["abs", "pow", "powf", "sqrt", "clamp", "min", "max", "log", "exp"];
const CONDITIONS: &[&str] = &["tern"];
const STRING:     &[&str] = &["to_upper", "to_lower", "trim", "starts_with", "ends_with",
                               "replace_str", "to_string", "parse_i64", "len"];
const ALGORITHMS: &[&str] = &["swap", "insertion_sort", "quick_sort", "merge_sort", "radix_sort"];
const IO:         &[&str] = &["in", "out", "open", "close", "time"];
const SYSTEM:     &[&str] = &["args", "exit", "env", "sleep"];

pub fn cmd_stdlib() {
    let builtins = stdlib::list_builtins();

    println!("Bullang standard library");
    println!("Available in every backend");
    println!();

    print_section("Math",       MATH,       &builtins);
    print_section("Conditions", CONDITIONS, &builtins);
    print_section("String",     STRING,     &builtins);
    print_section("Algorithms", ALGORITHMS, &builtins);
    print_section("I/O",        IO,         &builtins);
    print_section("System",     SYSTEM,     &builtins);

    println!("Usage in a source file:");
    println!();
    println!("  let upper(s: String) -> result: String {{");
    println!("      builtin::to_upper");
    println!("  }}");
    println!();
    println!("  let read_line(fd: i32) -> result: String {{");
    println!("      builtin::in");
    println!("  }}");
    println!();
    println!("The function's parameters are passed to the builtin.");
    println!("Parameter counts are enforced at build time.");
}

fn print_section(title: &str, names: &[&str], builtins: &[(&str, &str, &str)]) {
    println!("  {}", title);
    println!("  {}", "-".repeat(title.len()));
    for name in names {
        if let Some((_, sig, desc)) = builtins.iter().find(|(n, _, _)| n == name) {
            println!("    builtin::{:<14}  {}  — {}", name, sig, desc);
        }
    }
    println!();
}
