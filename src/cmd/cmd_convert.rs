//! `bullang convert` — transpiles a project folder or a single .bu file.
//!
//! Multi-language projects: if the project tree contains folders with different
//! #lang: directives (set via blueprint language prefixes), `bullang convert`
//! without `-e` converts each folder to its declared language independently.
//! Passing `-e` on a multi-language project is refused.

use std::path::{Path, PathBuf};
use crate::ast::{self, Backend};
use crate::validator::{self, AllErrors};
use crate::{build, codegen, parser, typecheck};
use crate::utils::{current_dir, read_file, find_root_from, find_root_from_probe, print_all_errors, print_type_errors};
use crate::readme::delete_project_readme;

// ── Project / single-file dispatch ────────────────────────────────────────────

pub fn cmd_convert(
    folder: Option<PathBuf>,
    name:   Option<String>,
    ext:    Option<String>,
    out:    Option<PathBuf>,
    output: Option<PathBuf>,
) {
    // ── Single-file mode ──────────────────────────────────────────────────────
    if let Some(ref p) = folder {
        let is_bu = p.extension().map(|e| e == "bu").unwrap_or(false);
        if is_bu {
            let resolved = if p.exists() {
                p.canonicalize().unwrap_or_else(|_| p.clone())
            } else {
                eprintln!("error: '{}' not found", p.display());
                std::process::exit(1);
            };
            cmd_convert_file(resolved, ext.unwrap_or_else(|| "rs".to_string()), output);
            return;
        }
    }

    let source_dir = match folder {
        Some(ref p) => {
            let c = p.canonicalize().unwrap_or_else(|_| p.clone());
            if !c.is_dir() {
                eprintln!("error: '{}' is not a directory", p.display());
                std::process::exit(1);
            }
            c
        }
        None => current_dir(),
    };

    let root = find_root_from(&source_dir);

    // ── Multi-language detection ──────────────────────────────────────────────
    let langs = collect_folder_langs(&root);
    let unique_langs: std::collections::HashSet<String> = langs.values()
        .filter_map(|l| l.as_ref().map(|b| b.ext().to_string()))
        .collect();

    let is_multi_lang = unique_langs.len() > 1;
    let ext_explicitly_set = ext.is_some();

    if is_multi_lang && ext_explicitly_set {
        eprintln!("error: this project uses multiple languages ({}).",
            unique_langs.into_iter().collect::<Vec<_>>().join(", "));
        eprintln!("       Run 'bullang convert' without -e to convert each folder");
        eprintln!("       to its declared language independently.");
        std::process::exit(1);
    }

    if is_multi_lang {
        cmd_convert_multi(&root, &source_dir, out);
        return;
    }

    // ── Single-language project ───────────────────────────────────────────────
    let resolved_ext = match ext {
        Some(e) => e,
        None => {
            // Auto-detect from root inventory #lang
            let probe_root = find_root_from_probe(&source_dir);
            if let Ok(inv) = validator::read_inventory(&probe_root) {
                if let Some(ref backend) = inv.lang {
                    backend.ext().to_string()
                } else {
                    "rs".to_string()
                }
            } else {
                "rs".to_string()
            }
        }
    };

    let backend = Backend::from_ext(&resolved_ext).unwrap_or_else(|| {
        eprintln!("error: unknown extension '{}' — supported: rs, py, c, cpp, go", resolved_ext);
        std::process::exit(1);
    });

    let source_name = source_dir.file_name()
        .and_then(|n| n.to_str()).unwrap_or("bullang_project").to_string();

    let out_dir = match out {
        Some(p) => p,
        None => {
            let out_name = name.unwrap_or_else(|| format!("_{}", source_name));
            source_dir.parent().unwrap_or(&source_dir).join(out_name)
        }
    };

    if out_dir.starts_with(&root) || root.starts_with(&out_dir) {
        eprintln!("error: output must be outside the source tree");
        std::process::exit(1);
    }

    let crate_name = out_dir.file_name()
        .and_then(|n| n.to_str()).unwrap_or("bullang_out").to_string();
    let root_rank  = validator::read_folder_rank(&root).expect("root has no rank");

    println!("bullang convert");
    println!("  source  : {} ({})", root.display(), root_rank.name());
    println!("  output  : {}", out_dir.display());
    println!("  crate   : {}", crate_name);
    println!("  backend : {}", backend.name());
    println!();

    let all_errors = validator::validate_tree(&root);
    if !all_errors.is_empty() {
        print_all_errors(&all_errors);
        std::process::exit(1);
    }
    println!("structural validation ... ok");

    // Backend compatibility: reject escape blocks targeting a different backend
    let compat_errors = build::validate_backend_compatibility(&root, &backend);
    if !compat_errors.is_empty() {
        let all = AllErrors { parse: vec![], structural: compat_errors };
        print_all_errors(&all);
        std::process::exit(1);
    }

    let type_errors = typecheck::typecheck_tree(&root);
    if !type_errors.is_empty() {
        print_type_errors(&type_errors);
        std::process::exit(1);
    }
    println!("type checking         ... ok");

    let result = build::build(&root, &out_dir, &crate_name, &backend);
    if !result.errors.is_empty() {
        let all = AllErrors { parse: vec![], structural: result.errors };
        print_all_errors(&all);
        eprintln!("\nconvert failed");
        std::process::exit(1);
    }

    println!("code generation       ... ok");
    println!();
    delete_project_readme(&root);
    println!("wrote {} file(s) to {}", result.files_written, out_dir.display());
    println!();
    match backend {
        Backend::Rust => {
            println!("to compile:");
            println!("  cd {} && cargo build", out_dir.display());
        }
        Backend::Python => {
            println!("to run:");
            println!("  cd {} && python3 -m {}", out_dir.display(), crate_name);
        }
        Backend::C => {
            println!("to compile:");
            println!("  cd {} && make", out_dir.display());
        }
        Backend::Cpp => {
            println!("to compile:");
            println!("  cd {} && make", out_dir.display());
        }
        Backend::Go => {
            println!("to run:");
            println!("  cd {} && go run .", out_dir.display());
        }
        Backend::Unknown(kw) => {
            eprintln!("error: unknown backend '{}'", kw);
        }
    }
}

// ── Single-file conversion ────────────────────────────────────────────────────
// `bullang convert path/to/file.bu [-e lang] [-o out]`
// Transpiles one source file without tree context.

pub fn cmd_convert_file(input: PathBuf, ext: String, output: Option<PathBuf>) {
    let source = read_file(&input);
    let is_inv = input.file_name().and_then(|n| n.to_str())
        .map(|n| n == "inventory.bu").unwrap_or(false);

    let bu = parser::parse_file(&source, is_inv).unwrap_or_else(|e| {
        eprintln!("parse error in {}:\n  {}", input.display(), e);
        std::process::exit(1);
    });

    let backend = Backend::from_ext(&ext).unwrap_or(Backend::Rust);

    // Derive output path: same directory as the input file, stem unchanged,
    // extension replaced with the target language extension.
    // If the user passed -o explicitly, honour that instead.
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let out_dir = input.parent().unwrap_or_else(|| std::path::Path::new("."));

    let derive_output = |target_ext: &str| -> PathBuf {
        match &output {
            Some(p) => p.clone(),
            None    => out_dir.join(format!("{}.{}", stem, target_ext)),
        }
    };

    match bu {
        ast::BuFile::Source(ref sf) => {
            use std::collections::HashSet;
            let path   = input.display().to_string();
            let errors = validator::validate_source_direct(
                sf, &path, &HashSet::new(), &ast::Rank::Skirmish,
            );
            if !errors.is_empty() {
                let all = AllErrors { parse: vec![], structural: errors };
                print_all_errors(&all);
                std::process::exit(1);
            }
            let type_errors = typecheck::typecheck_file(sf, &path);
            if !type_errors.is_empty() {
                print_type_errors(&type_errors);
                std::process::exit(1);
            }

            match backend {
                Backend::Rust => {
                    let out = derive_output("rs");
                    let content = codegen::emit_source(sf);
                    write_file_or_exit(&out, content);
                }
                Backend::Python => {
                    let out = derive_output("py");
                    let content = codegen::emit_source_py(sf);
                    write_file_or_exit(&out, content);
                }
                Backend::C => {
                    let hdr_name = format!("{}.h", stem);
                    let hdr_out  = out_dir.join(&hdr_name);
                    let src_out  = derive_output("c");
                    let content  = codegen::emit_source_c(sf, &hdr_name);
                    // For C, also write the header alongside if no explicit -o
                    if output.is_none() {
                        let (hdr_content, src_content) = split_c_output(&content, &hdr_name);
                        write_file_or_exit(&hdr_out, hdr_content);
                        write_file_or_exit(&src_out, src_content);
                    } else {
                        write_file_or_exit(&src_out, content);
                    }
                }
                Backend::Cpp => {
                    let hdr_name = format!("{}.hpp", stem);
                    let hdr_out  = out_dir.join(&hdr_name);
                    let src_out  = derive_output("cpp");
                    let content  = codegen::emit_source_cpp(sf, &hdr_name);
                    if output.is_none() {
                        let (hdr_content, src_content) = split_c_output(&content, &hdr_name);
                        write_file_or_exit(&hdr_out, hdr_content);
                        write_file_or_exit(&src_out, src_content);
                    } else {
                        write_file_or_exit(&src_out, content);
                    }
                }
                Backend::Go => {
                    let out = derive_output("go");
                    let content = codegen::emit_source_go(sf, "main");
                    write_file_or_exit(&out, content);
                }
                Backend::Unknown(_) => {
                    let out = derive_output("rs");
                    let content = codegen::emit_source(sf);
                    write_file_or_exit(&out, content);
                }
            }

            println!("wrote {}.{}", stem, ext);
        }
        ast::BuFile::Inventory(_) => {
            let out = derive_output("rs");
            write_file_or_exit(&out, codegen::emit_mod_rs(&[]));
            println!("wrote {}", out.display());
        }
    }
}

fn write_file_or_exit(path: &std::path::Path, content: String) {
    std::fs::write(path, &content).unwrap_or_else(|e| {
        eprintln!("error writing {}: {}", path.display(), e);
        std::process::exit(1);
    });
}

/// Split a combined C/C++ codegen output (header + source concatenated) into
/// two separate strings.  The codegen emits the header block first, separated
/// from the source block by a blank line.  If no clear split is found the
/// whole content goes into the source file.
fn split_c_output(content: &str, hdr_name: &str) -> (String, String) {
    // The generated output starts with the header guard / #pragma once block.
    // Look for the first occurrence of `#include "hdr_name"` which marks the
    // beginning of the .c / .cpp section.
    let marker = format!("#include \"{}\"", hdr_name);
    if let Some(pos) = content.find(&marker) {
        let hdr = content[..pos].trim_end().to_string() + "\n";
        let src = content[pos..].to_string();
        (hdr, src)
    } else {
        (String::new(), content.to_string())
    }
}

// ── Multi-language helpers ────────────────────────────────────────────────────

/// Walk the tree and collect (folder_path → Option<Backend>) for every folder
/// that has a #lang: directive or inherits one.
fn collect_folder_langs(root: &Path) -> std::collections::HashMap<PathBuf, Option<Backend>> {
    let mut map = std::collections::HashMap::new();
    collect_langs_recursive(root, None, &mut map);
    map
}

fn collect_langs_recursive(
    dir:         &Path,
    parent_lang: Option<&Backend>,
    map:         &mut std::collections::HashMap<PathBuf, Option<Backend>>,
) {
    let inv     = validator::read_inventory(dir).ok();
    let own_lang = inv.as_ref().and_then(|i| i.lang.as_ref());
    let effective = own_lang.or(parent_lang);
    map.insert(dir.to_path_buf(), effective.cloned());

    let subdirs = validator::collect_subdirs(dir);
    for subdir in subdirs {
        collect_langs_recursive(&subdir, effective, map);
    }
}

/// Convert a multi-language project: each top-level language boundary
/// is converted independently to `_foldername` next to the source folder.
fn cmd_convert_multi(root: &Path, source_dir: &Path, _out: Option<PathBuf>) {
    println!("bullang convert (multi-language)");
    println!("  source : {}", root.display());
    println!();

    let subdirs = validator::collect_subdirs(root);
    let mut total_written = 0usize;
    let mut converted = Vec::new();

    for subdir in &subdirs {
        let inv = match validator::read_inventory(subdir) {
            Ok(i)  => i,
            Err(_) => continue,
        };

        let backend = match &inv.lang {
            Some(b) => b.clone(),
            None    => continue, // no lang — skip
        };

        let folder_name = subdir.file_name()
            .and_then(|n| n.to_str()).unwrap_or("out");
        let out_dir = source_dir.join(format!("_{}", folder_name));
        let crate_name = format!("_{}", folder_name);

        println!("  [{} → {}]", backend.ext(), out_dir.display());

        // Validate this sub-tree
        let all_errors = validator::validate_tree(subdir);
        if !all_errors.is_empty() {
            print_all_errors(&all_errors);
            eprintln!("  skipped {} (validation errors)", folder_name);
            println!();
            continue;
        }

        let type_errors = typecheck::typecheck_tree(subdir);
        if !type_errors.is_empty() {
            print_type_errors(&type_errors);
            eprintln!("  skipped {} (type errors)", folder_name);
            println!();
            continue;
        }

        let result = build::build(subdir, &out_dir, &crate_name, &backend);
        if !result.errors.is_empty() {
            let all = AllErrors { parse: vec![], structural: result.errors };
            print_all_errors(&all);
            eprintln!("  skipped {} (codegen errors)", folder_name);
            println!();
            continue;
        }

        total_written += result.files_written;
        converted.push((folder_name.to_string(), backend.ext().to_string(), out_dir.clone()));
        println!("  wrote {} file(s)", result.files_written);
        println!();
    }

    delete_project_readme(root);

    println!("conversion complete — {} output(s):", converted.len());
    for (name, lang, out) in &converted {
        println!("  [{}] {} → {}", lang, name, out.display());
    }
    println!();
    println!("total files written: {}", total_written);
}
