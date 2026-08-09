//! `convert` — transpile a project folder or a single .bu file.
//!
//! Usage:
//!   convert my_project          — auto-detect lang from #lang: directive
//!   convert my_project py       — override lang for the whole project
//!   convert file.bu             — auto-detect lang from nearest inventory
//!   convert file.bu rs          — override lang, write next to source
//!   convert file.bu out.rs      — convert and write to explicit output path
//!
//! The second positional argument is interpreted as:
//!   - a known short extension (see `Backend::ALL`)  → language override
//!   - a filename ending in a known extension     → output path (single-file only)
//!   - absent                                     → auto-detect

use std::path::{Path, PathBuf};
use bullang::ast::{self, Backend};
use crate::validator::{self, AllErrors};
use crate::{build, codegen, typecheck};
use bullang::parser;
use crate::utils::{current_dir, read_file, find_root_from, find_root_from_probe,
                   print_all_errors, print_type_errors, format_source};

// ── Short extension set ───────────────────────────────────────────────────────

/// Derived from `Backend`, not written out again — the hand-written list this
/// replaces was missing "java", so `bullarchy convert . java` rejected a
/// backend the rest of the toolchain fully supported.
fn is_known_ext(s: &str) -> bool { Backend::from_ext(s).is_some() }

/// The backends a user could have named, for an error message.
fn known_exts() -> String { Backend::all_exts().join(" ") }

// ── Public entry point ────────────────────────────────────────────────────────

/// `lang` is `-e/--lang`, `out` is `-o/--out`.
///
/// These used to be one positional argument that `resolve_single_second`
/// guessed at: a known extension meant a language override, anything else
/// meant an output path. So `convert x.bu out.txt` failed with "cannot infer
/// language" rather than "unknown extension", and there was no way to say
/// "this language, that file" at all — which is exactly what the GUI's convert
/// panel asks for.
pub fn cmd_convert(target: Option<PathBuf>, lang: Option<String>, out: Option<PathBuf>) {
    let path = match target {
        Some(p) => p,
        None    => current_dir(),
    };

    if path.extension().map(|e| e == "bu").unwrap_or(false) {
        // ── Single-file mode ──────────────────────────────────────────────────
        if !path.exists() {
            eprintln!("error: '{}' not found", path.display());
            std::process::exit(1);
        }
        let input = path.canonicalize().unwrap_or(path);

        let lang = match lang {
            Some(l) if is_known_ext(&l) => l,
            Some(l) => {
                eprintln!("error: '{}' is not a recognised backend — use {}", l, known_exts());
                std::process::exit(1);
            }
            // No override: take the language from the output file's extension
            // if one was given, otherwise from the nearest inventory's `#lang`.
            None => match out.as_ref().and_then(|o| o.extension()).and_then(|e| e.to_str()) {
                Some(ext) if is_known_ext(ext) => ext.to_string(),
                Some(ext) => {
                    eprintln!(
                        "error: cannot infer a language from the output file's '.{}' \
                         extension — pass -e/--lang, or name the output with a known \
                         extension ({})",
                        ext, known_exts()
                    );
                    std::process::exit(1);
                }
                None => detect_lang_for_file(&input),
            },
        };
        cmd_convert_file(input, lang, out);
    } else {
        // ── Project mode ──────────────────────────────────────────────────────
        let source_dir = if path.exists() && path.is_dir() {
            path.canonicalize().unwrap_or(path)
        } else {
            eprintln!("error: '{}' is not a directory or .bu file", path.display());
            std::process::exit(1);
        };

        if out.is_some() {
            eprintln!("error: -o/--out names a single output file, which only applies \
                       when converting one .bu file. A project writes a directory.");
            std::process::exit(1);
        }
        let lang_override = lang.map(|s| {
            if is_known_ext(&s) { s }
            else {
                eprintln!("error: '{}' is not a recognised backend — use {}", s, known_exts());
                std::process::exit(1);
            }
        });

        cmd_convert_project(source_dir, lang_override);
    }
}

fn detect_lang_for_file(input: &Path) -> String {
    if let Some(dir) = input.parent() {
        if let Ok(inv) = validator::read_inventory(dir) {
            if let Some(ext) = inv.lang.as_ref().and_then(|b| b.ext()) { return ext.to_string(); }
        }
        let probe = find_root_from_probe(dir);
        if let Ok(inv) = validator::read_inventory(&probe) {
            if let Some(ext) = inv.lang.as_ref().and_then(|b| b.ext()) { return ext.to_string(); }
        }
    }
    "rs".to_string()
}

// ── Single-file conversion ────────────────────────────────────────────────────

fn cmd_convert_file(input: PathBuf, lang: String, explicit_out: Option<PathBuf>) {
    let source = read_file(&input);
    let is_inv = input.file_name().and_then(|n| n.to_str())
        .map(|n| n == "inventory.bu").unwrap_or(false);

    let mut bu = parser::parse_file(&source, is_inv).unwrap_or_else(|e| {
        eprintln!("parse error in {}:\n  {}", input.display(), e);
        std::process::exit(1);
    });

    let backend = Backend::from_ext(&lang).unwrap_or(Backend::Rust);
    let stem    = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let out_dir = input.parent().unwrap_or_else(|| Path::new("."));

    let out_path = |ext: &str| -> PathBuf {
        explicit_out.clone()
            .unwrap_or_else(|| out_dir.join(format!("{}.{}", stem, ext)))
    };

    // Identifiers that are keywords in the target language are escaped here,
    // the same as in a project build.
    if let ast::BuFile::Source(ref mut sf) = bu {
        crate::sanitize::normalize(sf, &backend);
    }

    match bu {
        ast::BuFile::Source(ref sf) => {
            // Check for escape block conflicts
            check_escape_compat(sf, &backend, &input);

            let (content, ext) = match backend {
                Backend::Rust        => (codegen::emit_bare_rs(sf),   "rs"),
                Backend::Python      => (codegen::emit_bare_py(sf),   "py"),
                Backend::C           => (codegen::emit_bare_c(sf),    "c"),
                Backend::Cpp         => (codegen::emit_bare_cpp(sf),  "cpp"),
                Backend::Go          => (codegen::emit_bare_go(sf),   "go"),
                Backend::Java        => (codegen::emit_bare_java(sf), "java"),
                Backend::Unknown(_)  => (codegen::emit_bare_rs(sf),   "rs"),
            };
            let out = out_path(ext);
            write_or_exit(&out, content);
            println!("wrote {}", out.display());
        }
        ast::BuFile::Inventory(_) => {
            let out = out_path("rs");
            write_or_exit(&out, codegen::emit_mod_rs(&[]));
            println!("wrote {}", out.display());
        }
    }
}

fn check_escape_compat(sf: &ast::SourceFile, backend: &Backend, path: &Path) {
    for bullet in &sf.bullets {
        if let ast::BulletBody::Natives(blocks) = &bullet.body {
            if let Some(block) = blocks.first() {
                if block.backend != *backend
                    && !matches!(block.backend, ast::Backend::Unknown(_))
                {
                    eprintln!(
                        "error: '{}': function '{}' has a @{} escape block \
                         but target is {}. Remove the override or match the backend.",
                        path.display(), bullet.name,
                        block.backend.escape_keyword(), backend.escape_keyword()
                    );
                    std::process::exit(1);
                }
            }
        }
    }
}

// ── Project conversion ────────────────────────────────────────────────────────

fn cmd_convert_project(source_dir: PathBuf, lang_override: Option<String>) {
    let root = find_root_from(&source_dir);

    let langs = collect_folder_langs(&root);
    let unique_langs: std::collections::HashSet<String> = langs.values()
        .filter_map(|l| l.as_ref().and_then(|b| b.ext()).map(|e| e.to_string()))
        .collect();

    let is_multi = unique_langs.len() > 1;

    if is_multi && lang_override.is_some() {
        eprintln!("error: this project uses multiple languages ({}).",
            unique_langs.into_iter().collect::<Vec<_>>().join(", "));
        eprintln!("       Omit the language argument to convert each folder independently.");
        std::process::exit(1);
    }

    if is_multi {
        cmd_convert_multi(&root, &source_dir);
        return;
    }

    // Single-language project
    let resolved_lang = lang_override.unwrap_or_else(|| {
        let probe = find_root_from_probe(&source_dir);
        validator::read_inventory(&probe)
            .ok()
            .and_then(|inv| inv.lang.and_then(|b| b.ext()).map(|e| e.to_string()))
            .unwrap_or_else(|| "rs".to_string())
    });

    let backend = Backend::from_ext(&resolved_lang).unwrap_or_else(|| {
        eprintln!("error: unknown backend '{}' — use {}", resolved_lang, known_exts());
        std::process::exit(1);
    });

    let source_name = source_dir.file_name()
        .and_then(|n| n.to_str()).unwrap_or("bullang_project");
    let out_dir = source_dir.parent()
        .unwrap_or(&source_dir)
        .join(format!("_{}", source_name));

    if out_dir.starts_with(&root) || root.starts_with(&out_dir) {
        eprintln!("error: output must be outside the source tree");
        std::process::exit(1);
    }

        let crate_name = crate_name_from(&out_dir);
    let root_rank = match validator::read_folder_rank(&root) {
        Some(r) => r,
        None => {
            eprintln!("error: no readable '#rank' in '{}/inventory.bu'", root.display());
            std::process::exit(1);
        }
    };

    println!("convert");
    println!("  source  : {} ({})", root.display(), root_rank.name());
    println!("  output  : {}", out_dir.display());
    println!("  backend : {}", backend.escape_keyword());
    println!();

    let all_errors = validator::validate_tree(&root);
    if !all_errors.is_empty() { print_all_errors(&all_errors); std::process::exit(1); }
    println!("structural validation ... ok");

    let compat_errors = build::validate_backend_compatibility(&root, &backend);
    if !compat_errors.is_empty() {
        let all = AllErrors { parse: vec![], structural: compat_errors };
        print_all_errors(&all);
        std::process::exit(1);
    }

    let type_errors = typecheck::typecheck_tree(&root);
    if !type_errors.is_empty() { print_type_errors(&type_errors); std::process::exit(1); }
    println!("type checking         ... ok");

    let result = build::build(&root, &out_dir, &crate_name, &backend);
    if !result.errors.is_empty() {
        let all = AllErrors { parse: vec![], structural: result.errors };
        print_all_errors(&all);
        eprintln!("\nconvert failed");
        std::process::exit(1);
    }

    println!("code generation       ... ok\n");
    println!("wrote {} file(s) to {}", result.files_written, out_dir.display());
    println!();
    print_next_steps(&backend, &out_dir, &crate_name);
}

fn print_next_steps(backend: &Backend, out_dir: &Path, crate_name: &str) {
    match backend {
        Backend::Rust   => println!("to compile:\n  cd {} && cargo build", out_dir.display()),
        Backend::Python => println!("to run:\n  cd {} && python3 -m {}", out_dir.display(), crate_name),
        Backend::C      => println!("to compile:\n  cd {} && make", out_dir.display()),
        Backend::Cpp    => println!("to compile:\n  cd {} && make", out_dir.display()),
        Backend::Go     => println!("to run:\n  cd {} && go run .", out_dir.display()),
        Backend::Java   => {
            if out_dir.join("BuNative.java").exists() {
                println!(
                    "to compile:\n  cd {} && make -f Makefile.native && javac *.java && java -Djava.library.path=. Main",
                    out_dir.display()
                );
            } else {
                println!("to compile:\n  cd {} && javac *.java && java Main", out_dir.display());
            }
        }
        Backend::Unknown(kw) => eprintln!("error: unknown backend '{}'", kw),
    }
}

// ── Multi-language project ────────────────────────────────────────────────────

fn collect_folder_langs(root: &Path) -> std::collections::HashMap<PathBuf, Option<Backend>> {
    let mut map = std::collections::HashMap::new();
    collect_langs_recursive(root, None, &mut map);
    map
}

fn collect_langs_recursive(
    dir: &Path, parent_lang: Option<&Backend>,
    map: &mut std::collections::HashMap<PathBuf, Option<Backend>>,
) {
    let inv      = validator::read_inventory(dir).ok();
    let own_lang = inv.as_ref().and_then(|i| i.lang.as_ref());
    let effective = own_lang.or(parent_lang);
    map.insert(dir.to_path_buf(), effective.cloned());
    for subdir in validator::collect_subdirs(dir) {
        collect_langs_recursive(&subdir, effective, map);
    }
}

/// Every language region in the tree, outermost first.
///
/// The root is always a region. Below it, any folder declaring its own
/// `#lang` starts another (decision 13). This used to look only at the root's
/// *direct* children and skip the root itself, so a project whose regions were
/// nested deeper produced nothing for them, and the root's own files were
/// never converted at all.
fn collect_regions(root: &Path) -> Vec<(PathBuf, Backend)> {
    fn walk(dir: &Path, inherited: Option<&Backend>, out: &mut Vec<(PathBuf, Backend)>, is_root: bool) {
        let inv = validator::read_inventory(dir).ok();
        let own = inv.as_ref().and_then(|i| i.lang.clone());
        let effective = own.clone().or_else(|| inherited.cloned());

        if is_root || own.is_some() {
            if let Some(ref b) = effective {
                out.push((dir.to_path_buf(), b.clone()));
            }
        }
        for subdir in validator::collect_subdirs(dir) {
            walk(&subdir, effective.as_ref(), out, false);
        }
    }
    let mut out = Vec::new();
    walk(root, None, &mut out, true);
    out
}

fn cmd_convert_multi(root: &Path, source_dir: &Path) {
    println!("convert (multi-language)");
    println!("  source : {}\n", root.display());

    let mut total = 0usize;
    let mut converted = Vec::new();

    for (subdir, backend) in collect_regions(root) {
        // Each region is self-contained: its own directory, its own build
        // file, and no references across the boundary — a call that crossed
        // one was rejected by the validator before we got here.
        let folder_name = if subdir == root {
            root.file_name().and_then(|n| n.to_str()).unwrap_or("out").to_string()
        } else {
            subdir.file_name().and_then(|n| n.to_str()).unwrap_or("out").to_string()
        };
        // Beside the project, not inside it — the same convention the
        // single-region path uses. Writing generated code into the source tree
        // made the *next* `check` fail on it: the walker found `_deep/` with no
        // inventory.bu and reported a malformed folder.
        let out_dir = source_dir.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(format!("_{}", folder_name));
        let crate_name = crate_name_from(&out_dir);

        println!("  [{} → {}]", backend.escape_keyword(), out_dir.display());

        let all_errors = validator::validate_tree(&subdir);
        if !all_errors.is_empty() { print_all_errors(&all_errors); eprintln!("  skipped {}\n", folder_name); continue; }

        let type_errors = typecheck::typecheck_tree(&subdir);
        if !type_errors.is_empty() { print_type_errors(&type_errors); eprintln!("  skipped {}\n", folder_name); continue; }

        let result = build::build(&subdir, &out_dir, &crate_name, &backend);
        if !result.errors.is_empty() {
            let all = AllErrors { parse: vec![], structural: result.errors };
            print_all_errors(&all);
            eprintln!("  skipped {}\n", folder_name);
            continue;
        }

        total += result.files_written;
        converted.push((folder_name.clone(), backend.escape_keyword().to_string(), out_dir));
        println!("  wrote {} file(s)\n", result.files_written);
    }


    println!("done — {} converted:", converted.len());
    for (name, lang, out) in &converted {
        println!("  [{}] {} → {}", lang, name, out.display());
    }
    println!("\ntotal files written: {}", total);
}

fn write_or_exit(path: &Path, content: String) {
    let formatted = format_source(path, &content)
        .unwrap_or(content);
    std::fs::write(path, &formatted).unwrap_or_else(|e| {
        eprintln!("error writing {}: {}", path.display(), e);
        std::process::exit(1);
    });
}

/// A module name derived from an output directory.
///
/// The directory name comes from the user's folder, which may contain
/// characters no target language accepts in an identifier. A project in
/// `example-project/` produced a crate called `_example-project`, and the
/// hyphen made the generated `Cargo.toml` unparseable, the Python package
/// unimportable and the C++ namespace a syntax error — three backends broken
/// by a character that is perfectly ordinary in a folder name.
///
/// `bullarchy init` validates the name it is given, but a project can be
/// converted without ever having been through `init`.
fn crate_name_from(out_dir: &Path) -> String {
    let raw = out_dir.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("bullang_out");

    let mut name: String = raw.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect();

    // A leading digit is legal in a directory name and in none of the six.
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if name.is_empty() {
        name.push_str("bullang_out");
    }
    name
}
