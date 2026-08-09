//! `update` — clean and reinstall the full Bullang ecosystem.
//!
//! Steps:
//!   1. Clear cargo git cache for all Bullang repos
//!   2. cargo install --force for Bullang, Bullarchy, Bullscript
//!   3. Rebuild bullarchy-gui (Go/Fyne) from the gui/ folder of the
//!      Bullarchy checkout cargo just cloned, and reinstall it next to
//!      the bullarchy binary. Assumes Go is already installed (this
//!      mirrors what the GUI installer does, minus the target-language
//!      toolchain setup).

use std::path::{Path, PathBuf};

pub const DEFAULT_REPO: &str = "https://github.com/The-Bullang-Foundation/Bullarchy.git";

const BULLANG_REPO:    &str = "https://github.com/The-Bullang-Foundation/Bullang.git";
const BULLARCHY_REPO:  &str = "https://github.com/The-Bullang-Foundation/Bullarchy.git";
const BULLSCRIPT_REPO: &str = "https://github.com/The-Bullang-Foundation/Bullscript.git";

pub fn cmd_update() {
    println!("  Updating Bullang ecosystem...\n");

    // The version check that used to run on every REPL start now runs here,
    // where the user asked for it. It is advisory: if the network is
    // unreachable the reinstall below still proceeds.
    report_available_update();

    // ── Step 1: Clear cargo git caches ───────────────────────────────────────

    println!("  Clearing cargo git cache...");
    let cargo_home = cargo_home();

    for dir in &["bullang", "bullarchy", "bullscript"] {
        for subfolder in &["checkouts", "db"] {
            let pattern = cargo_home.join("git").join(subfolder);
            if let Ok(entries) = std::fs::read_dir(&pattern) {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().to_lowercase();
                    if name.contains(dir) {
                        let path = entry.path();
                        match std::fs::remove_dir_all(&path) {
                            Ok(_)  => println!("    removed {}", path.display()),
                            Err(e) => eprintln!("    warning: could not remove {}: {}", path.display(), e),
                        }
                    }
                }
            }
        }
    }

    // Also remove old installed binaries so cargo reinstalls cleanly
    for bin in &["bullang", "bullarchy", "bullscript", "bullarchy-gui"] {
        let bin_path = cargo_home.join("bin").join(bin);
        if bin_path.exists() {
            let _ = std::fs::remove_file(&bin_path);
            println!("    removed binary: {}", bin_path.display());
        }
        // Windows
        let bin_exe = cargo_home.join("bin").join(format!("{}.exe", bin));
        if bin_exe.exists() {
            let _ = std::fs::remove_file(&bin_exe);
            println!("    removed binary: {}", bin_exe.display());
        }
    }

    println!();

    // ── Step 2: Reinstall all three ───────────────────────────────────────────

    let installs = [
        ("Bullang",    BULLANG_REPO,    "bullang"),
        ("Bullarchy",  BULLARCHY_REPO,  "bullarchy"),
        ("Bullscript", BULLSCRIPT_REPO, "bullscript"),
    ];

    let mut failed = false;

    for (name, repo, bin) in &installs {
        println!("  Installing {} from {}...", name, repo);
        let status = std::process::Command::new("cargo")
            .args(["install", "--git", repo, "--force", bin])
            .status();

        match status {
            Ok(s) if s.success() => println!("  ✓ {} updated.\n", name),
            Ok(s) => {
                eprintln!("  ✗ {} failed (exit {}).\n", name, s);
                failed = true;
            }
            Err(e) => {
                eprintln!("  ✗ Failed to run cargo for {}: {}.\n", name, e);
                failed = true;
            }
        }
    }

    // ── Step 3: Rebuild and reinstall the GUI ───────────────────────────────

    println!("  Rebuilding bullarchy-gui...");
    match update_gui(&cargo_home) {
        Ok(path) => println!("  ✓ bullarchy-gui updated -> {}\n", path.display()),
        Err(e) => {
            eprintln!("  ✗ bullarchy-gui update failed: {}\n", e);
            failed = true;
        }
    }

    if failed {
        eprintln!("  Some updates failed. Check the output above.");
    } else {
        println!("  ✓ All Bullang tools updated successfully.");
        println!("  Restart your terminal to ensure the new binaries are active.");
    }
}

// ── GUI rebuild ──────────────────────────────────────────────────────────────

/// Find the gui/ folder inside cargo's own checkout of Bullarchy (the one
/// `cargo install` just cloned in step 2), build it with `go build`, and
/// install the resulting binary into the cargo bin dir — the same folder
/// `bullarchy` itself lives in, which is the first place `launch_gui()`
/// looks. Returns the installed binary's path on success.
fn update_gui(cargo_home: &Path) -> Result<PathBuf, String> {
    let gui_src = find_bullarchy_gui_dir(cargo_home)
        .ok_or_else(|| "could not find gui/ in cargo's Bullarchy checkout".to_string())?;

    let out_name = if cfg!(windows) { "bullarchy-gui.exe" } else { "bullarchy-gui" };
    let built_path = gui_src.join(out_name);

    // The GUI is written in Go, so building it needs a Go toolchain. That was
    // never declared or checked: without one, `update` failed partway through
    // with a raw "No such file or directory" from the process spawn.
    if std::process::Command::new("go").arg("version").output().is_err() {
        return Err(
            "the GUI is written in Go and no `go` toolchain was found on PATH. \
             Install Go (https://go.dev/dl/) and run `bullarchy update` again, \
             or use the CLI, which needs no Go."
                .to_string()
        );
    }

    println!("    building from {}", gui_src.display());
    let status = std::process::Command::new("go")
        .args(["build", "-o", out_name, "."])
        .current_dir(&gui_src)
        .status()
        .map_err(|e| format!("failed to run `go build`: {}", e))?;

    if !status.success() {
        return Err(format!("`go build` exited with {}", status));
    }

    let target = cargo_home.join("bin").join(out_name);
    if target.exists() {
        std::fs::remove_file(&target)
            .map_err(|e| format!("could not remove old {}: {}", target.display(), e))?;
    }
    std::fs::copy(&built_path, &target)
        .map_err(|e| format!("could not install {}: {}", target.display(), e))?;
    let _ = std::fs::remove_file(&built_path);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = std::fs::metadata(&target) {
            let mut perms = meta.permissions();
            perms.set_mode(perms.mode() | 0o755);
            let _ = std::fs::set_permissions(&target, perms);
        }
    }

    Ok(target)
}

/// Locate `gui/` inside the most recently checked-out Bullarchy source
/// under `~/.cargo/git/checkouts/`.
fn find_bullarchy_gui_dir(cargo_home: &Path) -> Option<PathBuf> {
    let checkouts = cargo_home.join("git").join("checkouts");
    let repo_dir = std::fs::read_dir(&checkouts).ok()?
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().to_lowercase().contains("bullarchy"))
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?
        .path();

    // Inside <repo_dir>/, each checked-out commit lives in its own subfolder.
    let commit_dir = std::fs::read_dir(&repo_dir).ok()?
        .flatten()
        .filter(|e| e.path().is_dir())
        .max_by_key(|e| e.metadata().and_then(|m| m.modified()).ok())?
        .path();

    let gui_dir = commit_dir.join("gui");
    if gui_dir.join("main.go").exists() { Some(gui_dir) } else { None }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn cargo_home() -> PathBuf {
    std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let home = std::env::var("HOME")
                .or_else(|_| std::env::var("USERPROFILE"))
                .unwrap_or_default();
            PathBuf::from(home).join(".cargo")
        })
}

/// Fetch the HEAD commit hash of `branch` from a remote git repository.
/// Say whether the installed Bullarchy is behind its repository.
fn report_available_update() {
    match (remote_head(DEFAULT_REPO, "main"), installed_hash("bullarchy", DEFAULT_REPO, "main")) {
        (Some(remote), Some(installed)) if remote == installed =>
            println!("  Already at the latest revision. Reinstalling anyway.\n"),
        (Some(_), Some(_)) =>
            println!("  A newer revision is available.\n"),
        // Either call can fail without the network, which is not a reason to
        // refuse to reinstall.
        _ => {}
    }
}

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
pub fn installed_hash(package: &str, repo: &str, branch: &str) -> Option<String> {
    let content = std::fs::read_to_string(
        cargo_home().join(".crates2.json")
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
            let hash = key.split('#').nth(1)?.trim_end_matches(')');
            return Some(hash.to_string());
        }
    }
    None
}
