//! `remove` — uninstall a Bullang package.
//!
//! bullarchy remove <name>  → uninstall a package and rebuild if it was a feature lib

use std::fs;
use std::process::Command;

use crate::cmd::cmd_add::{bull_home, packages_dir};

const BULLARCHY_REPO: &str =
    "https://github.com/The-Bullang-Foundation/Bullarchy.git";

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn cmd_remove(args: &[&str]) {
    let name = match args.first() {
        Some(n) => n.to_string(),
        None => {
            eprintln!("  Usage: remove <package-name>");
            return;
        }
    };

    let lock_path = bull_home().join("bull.lock");

    // Read lockfile
    let mut lock: serde_json::Value = if lock_path.exists() {
        let content = fs::read_to_string(&lock_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Check the package is actually installed
    let entry = match lock.get(&name) {
        Some(e) => e.clone(),
        None => {
            eprintln!("  '{}' is not installed.", name);
            return;
        }
    };

    let cargo_feature = entry["feature"].as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());

    // Remove package files
    let pkg_dir = packages_dir().join(&name);
    if pkg_dir.exists() {
        match fs::remove_dir_all(&pkg_dir) {
            Ok(_)  => println!("  Removed {}", pkg_dir.display()),
            Err(e) => {
                eprintln!("  Failed to remove package files: {}", e);
                return;
            }
        }
    }

    // Remove from lockfile
    if let Some(obj) = lock.as_object_mut() {
        obj.remove(&name);
    }
    fs::write(&lock_path, serde_json::to_string_pretty(&lock).unwrap())
        .expect("could not update bull.lock");

    println!("  '{}' removed from bull.lock.", name);

    // If this was a Cargo feature lib, rebuild Bullarchy without it
    if let Some(feature) = cargo_feature {
        let remaining = remaining_features(&lock, &feature);

        if remaining.is_empty() {
            println!("  No feature libs remaining — reinstalling Bullarchy without features...");
            let status = Command::new("cargo")
                .args([
                    "install",
                    "--git", BULLARCHY_REPO,
                    "--force",
                    "bullarchy",
                ])
                .status();
            report(status);
        } else {
            let features_str = remaining.join(",");
            println!("  Reinstalling Bullarchy with remaining features: {}...", features_str);
            let status = Command::new("cargo")
                .args([
                    "install",
                    "--git", BULLARCHY_REPO,
                    "--features", &features_str,
                    "--force",
                    "bullarchy",
                ])
                .status();
            report(status);
        }
    } else {
        println!("  Done. '{}' has been uninstalled.", name);
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Collect all feature names still in the lockfile, excluding the removed one.
fn remaining_features(lock: &serde_json::Value, removed_feature: &str) -> Vec<String> {
    let mut features = Vec::new();
    if let Some(obj) = lock.as_object() {
        for (_name, meta) in obj {
            if let Some(f) = meta["feature"].as_str() {
                if !f.is_empty() && f != removed_feature && !features.contains(&f.to_string()) {
                    features.push(f.to_string());
                }
            }
        }
    }
    features
}

fn report(status: std::io::Result<std::process::ExitStatus>) {
    match status {
        Ok(s) if s.success() => println!("  Bullarchy reinstalled successfully."),
        Ok(s)  => eprintln!("  Reinstall failed (exit {}).", s),
        Err(e) => eprintln!("  Failed to run cargo: {}.", e),
    }
}
