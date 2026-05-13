//! Tree-walk build pass — rank-agnostic, any rank as root.
//! Dispatches to Rust or Python codegen based on the target backend.

use std::path::Path;
use std::fs;

use crate::ast::{BuFile, Rank, Backend};
use crate::codegen;
use crate::parser;
use crate::validator::{
    ValidationError, collect_bu_files, collect_subdirs,
    read_inventory, main_bu_path,
};

pub struct BuildResult {
    pub errors:        Vec<ValidationError>,
    pub files_written: usize,
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn build(root: &Path, out_dir: &Path, crate_name: &str, backend: &Backend) -> BuildResult {
    let mut errors        = Vec::new();
    let mut files_written = 0;

    let src_out = match backend {
        Backend::Python => out_dir.join(crate_name),
        Backend::Go | Backend::C | Backend::Cpp => out_dir.to_path_buf(),
        _ => out_dir.join("src"),
    };
    fs::create_dir_all(&src_out).expect("could not create out/src");

    let has_main = tree_has_main(root);

    // Collect all structs and enums from all inventories in the tree.
    // Must happen before emit_folder so lower_enum_refs has the full EnumEnv.
    let all_structs = collect_all_structs(root);
    let all_enums   = collect_all_enums(root);
    let enum_env: crate::ast::EnumEnv = all_enums.iter()
        .map(|e| (e.name.clone(), e.clone()))
        .collect();

    let (child_modules, _) = emit_folder(
        root, &src_out, backend, crate_name, has_main, &enum_env, &mut errors, &mut files_written,
    );

    match backend {
        Backend::Rust => {
            write_file(
                &src_out.join("lib.rs"),
                &codegen::emit_lib_rs(&child_modules, &all_structs, &all_enums),
                &mut files_written,
            );
            let cargo = if has_main {
                codegen::emit_cargo_toml_with_main(crate_name)
            } else {
                codegen::emit_cargo_toml(crate_name)
            };
            write_file(&out_dir.join("Cargo.toml"), &cargo, &mut files_written);
        }
        Backend::Python => {
            write_file(
                &src_out.join("__init__.py"),
                &codegen::emit_init_py(&child_modules, &all_structs, &all_enums),
                &mut files_written,
            );
        }
        Backend::C => {
            let header_name = format!("{}.h", crate_name);
            let all_sources = collect_all_sources(root);
            let src_refs: Vec<(String, &crate::ast::SourceFile)> =
                all_sources.iter().map(|(n, sf)| (n.clone(), sf)).collect();
            let libs   = collect_all_libs(root);
            let header = codegen::emit_header_c(crate_name, &src_refs, &libs, &all_structs, &all_enums);
            write_file(&out_dir.join(&header_name), &header, &mut files_written);

            let needs_ft = src_refs.iter().any(|(_, sf)| codegen::needs_foreign_types(sf));
            if needs_ft {
                write_file(
                    &out_dir.join("foreign_types.h"),
                    include_str!("foreign_types.h"),
                    &mut files_written,
                );
            }

            let needs_gen = src_refs.iter().any(|(_, sf)| codegen::needs_generic_types(sf));
            if needs_gen {
                write_file(
                    &out_dir.join("bu_generic.h"),
                    include_str!("bu_generic.h"),
                    &mut files_written,
                );
            }

            let mut all_c: Vec<String> = child_modules.iter()
                .map(|m| format!("{}.c", m)).collect();
            if has_main { all_c.push("main.c".to_string()); }
            let makefile = codegen::emit_makefile(crate_name, &all_c, has_main);
            write_file(&out_dir.join("Makefile"), &makefile, &mut files_written);
        }
        Backend::Cpp => {
            let header_name = format!("{}.hpp", crate_name);
            let all_sources = collect_all_sources(root);
            let src_refs: Vec<(String, &crate::ast::SourceFile)> =
                all_sources.iter().map(|(n, sf)| (n.clone(), sf)).collect();
            let libs   = collect_all_libs(root);
            let header = codegen::emit_header_cpp(crate_name, &src_refs, crate_name, &libs, &all_structs, &all_enums);
            write_file(&out_dir.join(&header_name), &header, &mut files_written);

            let mut all_cpp: Vec<String> = child_modules.iter()
                .map(|m| format!("{}.cpp", m)).collect();
            if has_main { all_cpp.push("main.cpp".to_string()); }
            let makefile = codegen::emit_makefile_cpp(crate_name, &all_cpp, has_main);
            write_file(&out_dir.join("Makefile"), &makefile, &mut files_written);
        }
        Backend::Go => {
            write_file(&out_dir.join("go.mod"), &codegen::emit_go_mod(crate_name), &mut files_written);

            // types.go — inventory structs + enums + Tuple foreign types
            let all_sources = collect_all_sources(root);
            let src_refs: Vec<(String, &crate::ast::SourceFile)> =
                all_sources.iter().map(|(n, sf)| (n.clone(), sf)).collect();
            let tuple_types = codegen::collect_tuple_types(&src_refs);
            if !all_structs.is_empty() || !all_enums.is_empty() || !tuple_types.is_empty() {
                let pkg = if has_main { "main" } else { crate_name };
                write_file(
                    &out_dir.join("types.go"),
                    &codegen::emit_types_go(pkg, &all_structs, &all_enums, &tuple_types),
                    &mut files_written,
                );
            }
        }
        Backend::Unknown(_) => {}
    }

    // ── blueprint.md ─────────────────────────────────────────────────────────
    // If the project root contains a blueprint.bu, copy it as blueprint.md
    // into the output so the architecture is documented alongside the code.
    let bp_src = root.join("blueprint.bu");
    if bp_src.exists() {
        if let Ok(bp_content) = fs::read_to_string(&bp_src) {
            let out_path = match backend {
                Backend::Python => out_dir.join(crate_name).join("blueprint.md"),
                Backend::C | Backend::Cpp | Backend::Go => out_dir.join("blueprint.md"),
                _ => src_out.join("blueprint.md"),
            };
            write_file(&out_path, &bp_content, &mut files_written);
        }
    }

    BuildResult { errors, files_written }
}

// ── Recursive folder emitter ──────────────────────────────────────────────────

fn emit_folder(
    src_dir:    &Path,
    out_dir:    &Path,
    backend:    &Backend,
    crate_name: &str,
    has_main:   bool,
    enum_env:   &crate::ast::EnumEnv,
    errors:     &mut Vec<ValidationError>,
    written:    &mut usize,
) -> (Vec<String>, Vec<String>) {
    let inv = match read_inventory(src_dir) {
        Ok(i)  => i,
        Err(_) => return (vec![], vec![]),
    };

    let mut child_modules: Vec<String> = Vec::new();
    let mut all_fns:       Vec<String> = Vec::new();

    // War: only sub-folders (+ optional main.bu)
    if inv.rank == Rank::War {
        for subdir in collect_subdirs(src_dir) {
            let name      = dir_name(&subdir);
            let child_out = out_dir.join(&name);
            fs::create_dir_all(&child_out).ok();
            let (gc, fns) = emit_folder(&subdir, &child_out, backend, crate_name, has_main, enum_env, errors, written);
            emit_mod_file(&child_out, &gc, backend, written);
            merge(&fns, &mut all_fns);
            child_modules.push(name);
        }
        if let Some(mp) = main_bu_path(src_dir) {
            emit_main_file(&mp, out_dir, backend, crate_name, enum_env, errors, written);
        }
        return (child_modules, all_fns);
    }

    // Sub-folders first (bottom-up)
    if inv.rank.has_sub_folders() {
        for subdir in collect_subdirs(src_dir) {
            let name = dir_name(&subdir);
            let child_out = match backend {
                Backend::C | Backend::Cpp | Backend::Go => out_dir.to_path_buf(),
                _ => {
                    let co = out_dir.join(&name);
                    fs::create_dir_all(&co).ok();
                    co
                }
            };
            let (gc, fns) = emit_folder(&subdir, &child_out, backend, crate_name, has_main, enum_env, errors, written);
            if !matches!(backend, Backend::C | Backend::Cpp | Backend::Go) {
                emit_mod_file(&child_out, &gc, backend, written);
                child_modules.push(name);
            } else {
                child_modules.extend(gc);
            }
            merge(&fns, &mut all_fns);
        }
    }

    // Source files in inventory order
    if inv.rank.has_own_files() {
        for entry in &inv.entries {
            let bu_path = src_dir.join(format!("{}.bu", entry.file));
            let source  = match fs::read_to_string(&bu_path) {
                Ok(s)  => s,
                Err(e) => { errors.push(io_err(&bu_path, e)); continue; }
            };
            let mut sf = match parser::parse_file(&source, false) {
                Ok(BuFile::Source(s))    => s,
                Ok(BuFile::Inventory(_)) => continue,
                Err(e) => { errors.push(parse_err(&bu_path, e)); continue; }
            };

            // Lower FieldAccess → EnumVariant before codegen
            crate::ast::lower_enum_refs(&mut sf, enum_env);

            merge(&entry.functions, &mut all_fns);

            let ext      = backend.ext();
            let out_path = out_dir.join(format!("{}.{}", entry.file, ext));
            let header_name = format!("{}.h", crate_name);
            let hpp_name    = format!("{}.hpp", crate_name);
            let go_pkg = if has_main && matches!(backend, Backend::Go) {
                "main".to_string()
            } else {
                crate_name.to_string()
            };
            let content = match backend {
                Backend::Rust        => codegen::emit_source(&sf),
                Backend::Python      => codegen::emit_source_py(&sf),
                Backend::C           => codegen::emit_source_c(&sf, &header_name),
                Backend::Cpp         => codegen::emit_source_cpp(&sf, &hpp_name),
                Backend::Go          => codegen::emit_source_go(&sf, &go_pkg),
                Backend::Unknown(_)  => continue,
            };
            write_file(&out_path, &content, written);
            child_modules.push(entry.file.clone());
        }
    }

    // main.bu at non-skirmish levels
    if inv.rank != Rank::Skirmish {
        if let Some(mp) = main_bu_path(src_dir) {
            emit_main_file(&mp, out_dir, backend, crate_name, enum_env, errors, written);
        }
    }

    (child_modules, all_fns)
}

// ── Module file emitter ───────────────────────────────────────────────────────

fn emit_mod_file(dir: &Path, child_modules: &[String], backend: &Backend, written: &mut usize) {
    match backend {
        Backend::Rust => {
            write_file(&dir.join("mod.rs"), &codegen::emit_mod_rs(child_modules), written);
        }
        Backend::Python => {
            write_file(
                &dir.join("__init__.py"),
                &codegen::emit_init_py(child_modules, &[], &[]),
                written,
            );
        }
        Backend::C | Backend::Cpp | Backend::Go | Backend::Unknown(_) => {}
    }
}

// ── main.bu emitter ───────────────────────────────────────────────────────────

fn emit_main_file(
    main_path:  &Path,
    out_dir:    &Path,
    backend:    &Backend,
    crate_name: &str,
    enum_env:   &crate::ast::EnumEnv,
    errors:     &mut Vec<ValidationError>,
    written:    &mut usize,
) {
    let source = match fs::read_to_string(main_path) {
        Ok(s)  => s,
        Err(e) => { errors.push(io_err(main_path, e)); return; }
    };
    let mut sf = match parser::parse_file(&source, false) {
        Ok(BuFile::Source(s)) => s,
        Ok(BuFile::Inventory(_)) => return,
        Err(e) => { errors.push(parse_err(main_path, e)); return; }
    };

    // Lower FieldAccess → EnumVariant before codegen
    crate::ast::lower_enum_refs(&mut sf, enum_env);

    let header_name = format!("{}.h", crate_name);
    let hpp_name    = format!("{}.hpp", crate_name);
    match backend {
        Backend::Rust => {
            write_file(
                &out_dir.join("main.rs"),
                &codegen::emit_main(&sf, crate_name),
                written,
            );
        }
        Backend::Python => {
            write_file(
                &out_dir.join("__main__.py"),
                &codegen::emit_main_py(&sf, crate_name),
                written,
            );
        }
        Backend::C => {
            write_file(
                &out_dir.join("main.c"),
                &codegen::emit_main_c(&sf, &header_name),
                written,
            );
        }
        Backend::Cpp => {
            write_file(
                &out_dir.join("main.cpp"),
                &codegen::emit_main_cpp(&sf, &hpp_name, crate_name),
                written,
            );
        }
        Backend::Go => {
            write_file(
                &out_dir.join("main.go"),
                &codegen::emit_main_go(&sf, crate_name),
                written,
            );
        }
        Backend::Unknown(_) => {}
    }
}

// ── Backend mismatch validation ───────────────────────────────────────────────

/// Validate that all escape blocks in the tree match the target backend.
/// Returns errors for any mismatch found.
pub fn validate_backend_compatibility(
    root:    &Path,
    backend: &Backend,
) -> Vec<ValidationError> {
    let mut errors = Vec::new();
    check_folder_backend(root, backend, &mut errors);
    errors
}

fn check_folder_backend(dir: &Path, backend: &Backend, errors: &mut Vec<ValidationError>) {
    for bu in collect_bu_files(dir) {
        check_file_backend(&bu, backend, errors);
    }
    if let Some(mp) = main_bu_path(dir) {
        check_file_backend(&mp, backend, errors);
    }
    for subdir in collect_subdirs(dir) {
        check_folder_backend(&subdir, backend, errors);
    }
}

fn check_file_backend(path: &Path, backend: &Backend, errors: &mut Vec<ValidationError>) {
    let source = match fs::read_to_string(path) {
        Ok(s)  => s,
        Err(_) => return,
    };
    let sf = match parser::parse_file(&source, false) {
        Ok(BuFile::Source(s)) => s,
        _                     => return,
    };

    let path_str = path.display().to_string();
    for func in &sf.bullets {
        if let crate::ast::BulletBody::Natives(blocks) = &func.body {
            // With multi-block functions, compatibility means at least ONE block
            // matches the target backend. If none match, report an error.
            let has_match = blocks.iter().any(|b| {
                match (&b.backend, backend) {
                    (Backend::C, Backend::Cpp)   => true,
                    (Backend::Cpp, Backend::Cpp) => true,
                    (a, b) => a == b,
                }
            });
            if !has_match && !blocks.is_empty() {
                let available: Vec<String> = blocks.iter()
                    .map(|b| b.backend.escape_keyword().to_string())
                    .collect();
                errors.push(ValidationError {
                    file:    path_str.clone(),
                    line:    func.span.line,
                    col:     func.span.col,
                    message: format!(
                        "Function '{}': no '@{}' escape block provided. \
                         Available blocks: @{}. Add a '@{}' block for this backend.",
                        func.name, backend.escape_keyword(),
                        available.join(", @"), backend.escape_keyword()
                    ),
                });
            }
        }
    }
}

// ── Library collector (for header #include directives) ───────────────────────

/// Walk the entire source tree and collect all unique #lib declarations.
/// Libs from all inventories are merged — deeper inventories can add to
/// the global set. Order is deterministic (tree walk order, deduped).
fn collect_all_libs(dir: &Path) -> Vec<String> {
    let mut libs: Vec<String> = Vec::new();
    if let Ok(inv) = read_inventory(dir) {
        for lib in &inv.libs {
            if !libs.contains(lib) {
                libs.push(lib.clone());
            }
        }
    }
    for subdir in collect_subdirs(dir) {
        for lib in collect_all_libs(&subdir) {
            if !libs.contains(&lib) {
                libs.push(lib);
            }
        }
    }
    libs
}

// ── Source file collector (for header generation) ────────────────────────────

/// Walk the entire source tree and collect (stem_name, SourceFile) for every
/// .bu source file. Used by C/C++ header generation to produce forward decls.
fn collect_all_structs(dir: &Path) -> Vec<crate::ast::StructDef> {
    let mut result = Vec::new();
    let inv = match read_inventory(dir) {
        Ok(i) => i, Err(_) => return result,
    };
    for s in inv.structs {
        if !result.iter().any(|r: &crate::ast::StructDef| r.name == s.name) {
            result.push(s);
        }
    }
    for subdir in collect_subdirs(dir) {
        for s in collect_all_structs(&subdir) {
            if !result.iter().any(|r: &crate::ast::StructDef| r.name == s.name) {
                result.push(s);
            }
        }
    }
    result
}

fn collect_all_enums(dir: &Path) -> Vec<crate::ast::EnumDef> {
    let mut result = Vec::new();
    let inv = match read_inventory(dir) {
        Ok(i) => i, Err(_) => return result,
    };
    for e in inv.enums {
        if !result.iter().any(|r: &crate::ast::EnumDef| r.name == e.name) {
            result.push(e);
        }
    }
    for subdir in collect_subdirs(dir) {
        for e in collect_all_enums(&subdir) {
            if !result.iter().any(|r: &crate::ast::EnumDef| r.name == e.name) {
                result.push(e);
            }
        }
    }
    result
}

fn collect_all_sources(dir: &Path) -> Vec<(String, crate::ast::SourceFile)> {
    let mut result = Vec::new();
    let inv = match read_inventory(dir) {
        Ok(i) => i, Err(_) => return result,
    };
    for entry in &inv.entries {
        let bu_path = dir.join(format!("{}.bu", entry.file));
        if let Ok(source) = std::fs::read_to_string(&bu_path) {
            if let Ok(crate::ast::BuFile::Source(sf)) = parser::parse_file(&source, false) {
                result.push((entry.file.clone(), sf));
            }
        }
    }
    for subdir in collect_subdirs(dir) {
        result.extend(collect_all_sources(&subdir));
    }
    result
}

// ── Tree scan ─────────────────────────────────────────────────────────────────

fn tree_has_main(dir: &Path) -> bool {
    if main_bu_path(dir).is_some() { return true; }
    for subdir in collect_subdirs(dir) {
        if tree_has_main(&subdir) { return true; }
    }
    false
}

// ── Utilities ─────────────────────────────────────────────────────────────────

fn write_file(path: &Path, content: &str, written: &mut usize) {
    if let Some(p) = path.parent() { fs::create_dir_all(p).ok(); }
    if fs::write(path, content).is_ok() { *written += 1; }
}

fn dir_name(path: &Path) -> String {
    path.file_name().and_then(|s| s.to_str()).unwrap_or("unknown").to_string()
}

fn merge(src: &[String], dst: &mut Vec<String>) {
    for name in src { if !dst.contains(name) { dst.push(name.clone()); } }
}

fn io_err(path: &Path, e: std::io::Error) -> ValidationError {
    ValidationError { file: path.display().to_string(), line: 0, col: 0,
        message: format!("Could not read: {}", e) }
}

fn parse_err(path: &Path, e: Box<dyn std::error::Error>) -> ValidationError {
    ValidationError { file: path.display().to_string(), line: 0, col: 0,
        message: format!("Parse error: {}", e) }
}
