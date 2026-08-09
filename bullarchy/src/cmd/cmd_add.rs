//! `add` — Bullarchy package manager.
//!
//! bullarchy add                  → list all known packages from the registry
//! bullarchy add <name>           → install latest tagged version of a package
//! bullarchy add <name>@<ver>     → install a specific version
//! bullarchy add <https://...>    → install directly from a git URL (latest tag)
//! bullarchy add <https://...>@v1 → install specific version from a git URL

use std::fs;
use std::path::PathBuf;
use std::process::Command;

// ── Constants ─────────────────────────────────────────────────────────────────

const REGISTRY_URL: &str =
    "https://raw.githubusercontent.com/The-Bullang-Foundation/Bullarchy-registery/main/registry.json";

const BULLARCHY_REPO: &str =
    "https://github.com/The-Bullang-Foundation/Bullarchy.git";

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn cmd_add(args: &[&str]) {
    if args.is_empty() {
        list_packages();
        return;
    }

    let raw = args[0];

    let source = raw.to_string();

    if source.starts_with("https://") || source.starts_with("http://") {
        install_from_url(&source, None);
    } else {
        install_from_registry(&source);
    }
}

// ── List ──────────────────────────────────────────────────────────────────────

fn list_packages() {
    println!("  Fetching package list...\n");

    let registry = match fetch_registry() {
        Some(r) => r,
        None => {
            eprintln!("  Could not reach the Bullarchy registry.");
            eprintln!("  Check your internet connection or visit:");
            eprintln!("  https://github.com/The-Bullang-Foundation/Bullarchy-registery");
            return;
        }
    };

    let packages = match registry.as_object() {
        Some(p) => p,
        None => { eprintln!("  Registry format error."); return; }
    };

    if packages.is_empty() {
        println!("  No packages available yet.");
        return;
    }

    println!("  Available packages:\n");
    let max_name = packages.keys().map(|k| k.len()).max().unwrap_or(0);

    for (name, meta) in packages {
        let description = meta["description"].as_str().unwrap_or("No description.");
        println!(
            "    {:<width$}  {}",
            name, description,
            width = max_name
        );
    }

    println!();
    println!("  Install with:  add <name>");
    println!("  From URL:      add <https://github.com/...>");
    println!();
}

// ── Install from registry ─────────────────────────────────────────────────────

fn install_from_registry(name: &str) {
    let registry = match fetch_registry() {
        Some(r) => r,
        None => {
            eprintln!("  Could not reach the Bullarchy registry.");
            return;
        }
    };

    let meta = match registry.get(name) {
        Some(m) => m,
        None => {
            eprintln!("  Unknown package '{}'. Run 'add' to see available packages.", name);
            return;
        }
    };

    let git_url = match meta["git"].as_str() {
        Some(u) => u.to_string(),
        None    => { eprintln!("  Registry entry for '{}' has no git URL.", name); return; }
    };

    // Check if this package is a Cargo feature lib (has a "feature" field)
    let cargo_feature = meta["feature"].as_str().map(|s| s.to_string());

    install_from_url_with_feature(&git_url, Some(name), cargo_feature.as_deref());
}

// ── Install from URL ──────────────────────────────────────────────────────────

fn install_from_url(git_url: &str, package_name: Option<&str>) {
    install_from_url_with_feature(git_url, package_name, None);
}

fn install_from_url_with_feature(
    git_url:       &str,
    package_name:  Option<&str>,
    cargo_feature: Option<&str>,
) {
    let name = package_name.map(|s| s.to_string()).unwrap_or_else(|| {
        git_url
            .trim_end_matches('/')
            .trim_end_matches(".git")
            .split('/')
            .last()
            .unwrap_or("unknown")
            .to_string()
    });

    let install_dir = packages_dir().join(&name);

    if install_dir.exists() {
        println!("  '{}' is already installed.", name);
        // Still reinstall Bullarchy if this is a feature lib, in case it
        // was installed before but the feature wasn't compiled in.
        if let Some(feature) = cargo_feature {
            reinstall_bullarchy_with_feature(&name, feature);
        }
        return;
    }

    println!("  Installing {}...", name);

    fs::create_dir_all(&install_dir).expect("could not create package directory");

    let status = Command::new("git")
        .args([
            "clone", "--depth", "1",
            git_url, install_dir.to_str().unwrap(),
        ])
        .status();

    match status {
        Ok(s) if s.success() => {
            update_lockfile(&name, git_url, cargo_feature);
            println!("  Installed {} → {}", name, install_dir.display());

            if let Some(feature) = cargo_feature {
                println!();
                println!("  '{}' is a Cargo-feature library.", name);
                println!("  Reinstalling Bullarchy with --features {}...", feature);
                reinstall_bullarchy_with_feature(&name, feature);
            } else {
                println!();
                println!("  To use in a project, add to your inventory.bu:");
                println!("    #lib: {};", name);
            }
        }
        Ok(s) => {
            let _ = fs::remove_dir_all(&install_dir);
            eprintln!("  git clone failed (exit {}).", s);
        }
        Err(e) => {
            let _ = fs::remove_dir_all(&install_dir);
            eprintln!("  Failed to run git: {}.", e);
        }
    }
}

// ── Bullarchy reinstall with accumulated features ─────────────────────────────

fn reinstall_bullarchy_with_feature(new_lib: &str, new_feature: &str) {
    // Collect all currently installed feature libs from the lockfile
    let features = accumulated_features(new_lib, new_feature);
    let features_str = features.join(",");

    println!("  Active features: {}", features_str);
    println!("  Running: cargo install --git {} --features {} --force bullarchy", BULLARCHY_REPO, features_str);

    let status = Command::new("cargo")
        .args([
            "install",
            "--git", BULLARCHY_REPO,
            "--features", &features_str,
            "--force",
            "bullarchy",
        ])
        .status();

    match status {
        Ok(s) if s.success() => println!("  Bullarchy reinstalled with {}.", features_str),
        Ok(s)  => eprintln!("  Reinstall failed (exit {}).", s),
        Err(e) => eprintln!("  Failed to run cargo: {}.", e),
    }
}

/// Reads the lockfile and collects all feature names for installed libs,
/// adding the new one. Deduplicates.
fn accumulated_features(new_lib: &str, new_feature: &str) -> Vec<String> {
    let lock_path = bull_home().join("bull.lock");
    let mut features = vec![new_feature.to_string()];

    if lock_path.exists() {
        let content = fs::read_to_string(&lock_path).unwrap_or_default();
        if let Ok(lock) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(obj) = lock.as_object() {
                for (lib_name, meta) in obj {
                    if lib_name == new_lib { continue; }
                    if let Some(f) = meta["feature"].as_str() {
                        if !features.contains(&f.to_string()) {
                            features.push(f.to_string());
                        }
                    }
                }
            }
        }
    }

    features
}

// ── Registry fetching ─────────────────────────────────────────────────────────

fn fetch_registry() -> Option<serde_json::Value> {
    crate::utils::fetch_url(REGISTRY_URL).and_then(|t| serde_json::from_str(&t).ok())
}


// ── Lockfile ──────────────────────────────────────────────────────────────────

fn update_lockfile(name: &str, git_url: &str, cargo_feature: Option<&str>) {
    let lock_path = bull_home().join("bull.lock");

    let mut lock: serde_json::Value = if lock_path.exists() {
        let content = fs::read_to_string(&lock_path).unwrap_or_default();
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    lock[name] = serde_json::json!({
        "git":     git_url,
        "feature": cargo_feature.unwrap_or(""),
    });

    fs::write(&lock_path, serde_json::to_string_pretty(&lock).unwrap())
        .expect("could not write bull.lock");
}

// ── Paths ─────────────────────────────────────────────────────────────────────

pub fn bull_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let dir  = PathBuf::from(home).join(".bull");
    fs::create_dir_all(&dir).ok();
    dir
}

pub fn packages_dir() -> PathBuf {
    let dir = bull_home().join("packages");
    fs::create_dir_all(&dir).ok();
    dir
}
