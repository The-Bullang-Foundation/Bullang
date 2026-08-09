//! `bullarchy init` — project scaffolding command.

use std::path::PathBuf;
use crate::init;
use crate::utils::current_dir;

pub fn cmd_init(
    name:      String,
    depth:     u8,
    blueprint: Option<PathBuf>,
    lang:      Option<String>,
    libs:      Vec<String>,
    path:      Option<PathBuf>,
) {
    let parent = path.unwrap_or_else(current_dir);

    // ── Input validation (decision 16) ────────────────────────────────────────
    //
    // None of this was checked. `--lang rust` wrote `#lang: rust;` into every
    // inventory, which the grammar does not accept — so `init` produced a
    // project that would not even parse, and the error surfaced later as a
    // parse failure with no hint that the flag caused it.
    if let Some(ref l) = lang {
        if bullang::ast::Backend::from_ext(l).is_none() {
            eprintln!(
                "error: '{}' is not a target language — use {}",
                l,
                bullang::ast::Backend::all_exts().join(" ")
            );
            eprintln!("       (the extension, not the language's name: 'rs', not 'rust')");
            std::process::exit(1);
        }
    }

    if !is_valid_identifier(&name) {
        eprintln!("error: '{}' is not a valid project name", name);
        eprintln!("       Use a letter or underscore, then letters, digits or underscores.");
        std::process::exit(1);
    }

    // In blueprint mode the tree comes from the file, so --depth is not
    // consulted. It used to be accepted and ignored in silence.
    if blueprint.is_some() && depth != 2 {
        eprintln!("warning: --depth is ignored in blueprint mode — the blueprint \
                   describes the tree");
    }

    // ── Blueprint mode ────────────────────────────────────────────────────────
    if let Some(ref bp_path) = blueprint {
        let bp_src = std::fs::read_to_string(bp_path).unwrap_or_else(|e| {
            eprintln!("error: cannot read blueprint file '{}': {}", bp_path.display(), e);
            std::process::exit(1);
        });

        let nodes = init::parse_blueprint(&bp_src).unwrap_or_else(|e| {
            eprintln!("error parsing blueprint: {}", e);
            std::process::exit(1);
        });

        println!("bullarchy init");
        println!("  name      : {}", name);
        println!("  blueprint : {}", bp_path.display());
        if let Some(ref l) = lang { println!("  lang      : {}", l); }
        println!();

        match init::init_from_blueprint(&parent, &name, &nodes, lang.as_deref(), &bp_src) {
            Ok(result) => {
                init::print_blueprint_tree(&result);
                println!();
                println!("project ready.");
                print_book_link();
            }
            Err(e) => { eprintln!("error: {}", e); std::process::exit(1); }
        }
        return;
    }

    // ── Standard depth-based mode ─────────────────────────────────────────────
    if depth < 1 || depth > 6 {
        eprintln!("error: --depth must be between 1 and 6");
        eprintln!();
        eprintln!("  depth 1 → skirmish");
        eprintln!("  depth 2 → tactic → skirmish");
        eprintln!("  depth 3 → strategy → tactic → skirmish");
        eprintln!("  depth 4 → battle → strategy → tactic → skirmish");
        eprintln!("  depth 5 → theater → battle → strategy → tactic → skirmish");
        eprintln!("  depth 6 → war → theater → battle → strategy → tactic → skirmish");
        std::process::exit(1);
    }

    let root_rank = init::rank_for_depth(depth).unwrap();
    println!("bullarchy init");
    println!("  name  : {}", name);
    println!("  depth : {} (root rank: {})", depth, root_rank.name());
    if let Some(ref l) = lang {
        println!("  lang  : {}", l);
    }
    if !libs.is_empty() {
        println!("  libs  : {}", libs.join(", "));
    }
    println!();

    match init::init(&parent, &name, depth, lang.as_deref(), &libs) {
        Ok(result) => {
            init::print_tree(&result);
            println!();
            println!("project ready.");
            print_book_link();
        }
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

/// `init` used to write a README that was a fifth copy of the language spec,
/// and `convert` deleted whatever README the project had in order to rewrite
/// it. Neither is `bullarchy`'s business: the spec lives in one place, and a
/// project's README belongs to whoever wrote it.
fn print_book_link() {
    println!();
    println!("  The Bullang Book: https://github.com/The-Bullang-Foundation/Bullang-Book");
}

/// A name that can be a directory, a module and an identifier in all six
/// target languages.
///
/// `--lib` is deliberately not validated: it is a native header or import of
/// the target language, whose spelling belongs to that language — `stdio.h`,
/// `os/exec`, `java.util.*` — and Bullarchy has no business ruling on it.
fn is_valid_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}
