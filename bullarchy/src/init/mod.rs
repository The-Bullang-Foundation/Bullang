//! `bullang init` — project scaffolding.
//!
//! `bullang init my_project --depth N` creates a properly structured
//! Bullang folder tree. Depth maps directly to rank from the bottom up:
//!
//!   depth 1 → skirmish
//!   depth 2 → tactic → skirmish
//!   depth 3 → strategy → tactic → skirmish
//!   depth 4 → battle → strategy → tactic → skirmish
//!   depth 5 → theater → battle → strategy → tactic → skirmish
//!   depth 6 → war → theater → battle → strategy → tactic → skirmish

mod blueprint;

pub use blueprint::{
    parse_blueprint, init_from_blueprint, print_blueprint_tree,
};

use std::path::{Path, PathBuf};
use std::fs;
use bullang::ast::Rank;

// ── Rank from depth ───────────────────────────────────────────────────────────

pub fn rank_for_depth(depth: u8) -> Option<Rank> {
    match depth {
        1 => Some(Rank::Skirmish),
        2 => Some(Rank::Tactic),
        3 => Some(Rank::Strategy),
        4 => Some(Rank::Battle),
        5 => Some(Rank::Theater),
        6 => Some(Rank::War),
        _ => None,
    }
}

/// The chain of ranks from root down to skirmish for a given depth.
/// depth 3 → [Strategy, Tactic, Skirmish]
fn rank_chain(depth: u8) -> Vec<Rank> {
    let all = [
        Rank::War, Rank::Theater, Rank::Battle,
        Rank::Strategy, Rank::Tactic, Rank::Skirmish,
    ];
    let start = (all.len() as u8).saturating_sub(depth) as usize;
    all[start..].to_vec()
}

// ── Public entry point ────────────────────────────────────────────────────────

pub struct InitResult {
    pub root:          PathBuf,
    pub files_created: Vec<PathBuf>,
}

pub fn init(parent: &Path, name: &str, depth: u8, lang: Option<&str>, libs: &[String]) -> Result<InitResult, String> {
    let root = parent.join(name);

    if root.exists() {
        return Err(format!(
            "'{}' already exists. Choose a different name or remove the existing folder.",
            root.display()
        ));
    }

    let ranks = rank_chain(depth);
    let total = ranks.len();
    let mut files_created: Vec<PathBuf> = Vec::new();

    create_level(&root, &ranks, name, total, true, lang, libs, &mut files_created)?;

    Ok(InitResult { root, files_created })
}

// ── Recursive folder creator ──────────────────────────────────────────────────

fn create_level(
    dir:     &Path,
    ranks:   &[Rank],
    name:    &str,
    total:   usize,
    is_root: bool,
    lang:    Option<&str>,
    libs:    &[String],
    created: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let rank = &ranks[0];

    fs::create_dir_all(dir)
        .map_err(|e| format!("Could not create '{}': {}", dir.display(), e))?;

    let mut inv_content = format!("#rank: {};\n", rank.name());
    if is_root {
        if let Some(l) = lang {
            // No blank line between the directives — that is what `fmt`
            // produces, and a scaffolded project that fails its own `check`
            // is a bad first impression.
            inv_content.push_str(&format!("#lang: {};\n", l));
        }
        if !libs.is_empty() {
            inv_content.push('\n');
            for lib in libs {
                inv_content.push_str(&format!("#lib: {};\n", lib));
            }
        }
    }
    // ── A source file to start from (decision 15) ─────────────────────────
    //
    // `init` used to create nothing but empty inventories, so the first thing
    // a new project did was fail `check` — there was no entry point, and
    // nothing to run. A `main.bu` at the root rank gives a project that
    // validates and transpiles the moment it is created.
    if is_root {
        write_file(&dir.join("main.bu"), MAIN_TEMPLATE, created)?;
    }

    write_file(&dir.join("inventory.bu"), &inv_content, created)?;

    if ranks.len() > 1 {
        let child_rank = &ranks[1];
        let child_name = format!("{}_{}", child_rank.name(), name);
        let child_dir  = dir.join(&child_name);
        create_level(&child_dir, &ranks[1..], name, total, false, None, &[], created)?;
    }

    Ok(())
}

/// The scaffolded entry point. Deliberately the smallest program that does
/// something observable: one bullet, one builtin, no imports to explain.
const MAIN_TEMPLATE: &str = "let main() {\n\
     \x20   (1, \"Hello from Bullang\\n\") : builtin::out -> {};\n\
     }\n";

// ── File writer ───────────────────────────────────────────────────────────────

pub(crate) fn write_file(path: &Path, content: &str, created: &mut Vec<PathBuf>) -> Result<(), String> {
    fs::write(path, content)
        .map_err(|e| format!("Could not write '{}': {}", path.display(), e))?;
    created.push(path.to_path_buf());
    Ok(())
}

// ── Display helper ────────────────────────────────────────────────────────────

pub fn print_tree(result: &InitResult) {
    println!("created: {}", result.root.display());
    println!();

    let root = &result.root;
    for path in &result.files_created {
        let rel       = path.strip_prefix(root).unwrap_or(path);
        let depth     = rel.components().count().saturating_sub(1);
        let indent    = "  ".repeat(depth);
        let file_name = rel.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");

        let label = match file_name {
            "inventory.bu" => format!("{} (inventory)", file_name),
            "main.bu"      => format!("{} (entry point)", file_name),
            _              => file_name.to_string(),
        };

        if file_name == "inventory.bu" && depth > 0 {
            let folder = rel.parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("?");
            let folder_indent = "  ".repeat(depth - 1);
            println!("  {}{}/", folder_indent, folder);
        }

        println!("  {}  {}", indent, label);
    }
}
