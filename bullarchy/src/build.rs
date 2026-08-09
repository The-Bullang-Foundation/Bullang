//! Tree-walk build pass — rank-agnostic, any rank as root.
//! Dispatches to Rust or Python codegen based on the target backend.

use std::path::Path;
use std::fs;

use bullang::ast::{BuFile, Rank, Backend};
use crate::codegen;
use bullang::parser;
use crate::validator::{
    ValidationError, collect_bu_files, collect_subdirs,
    read_inventory, main_bu_path,
};

pub struct BuildResult {
    pub errors:        Vec<ValidationError>,
    pub files_written: usize,
}

/// Which `.bu` file produced each output path.
///
/// C, C++, Go and Java have no module system that mirrors Bullang's folders,
/// so their output is flat: `a/util.bu` and `b/util.bu` both want to be
/// `util.c`. The second silently overwrote the first, and the only symptom was
/// functions mysteriously missing from the build.
///
/// Scoped per language region — two regions write separate directories, so the
/// same name in each is not a collision (decision 13).
type OutputOwners = std::collections::HashMap<std::path::PathBuf, std::path::PathBuf>;

/// Record `source` as the producer of `out_path`, or report a collision.
fn claim_output(
    out_path: &Path,
    source:   &Path,
    owners:   &mut OutputOwners,
    errors:   &mut Vec<ValidationError>,
) -> bool {
    if let Some(first) = owners.get(out_path) {
        errors.push(ValidationError {
            file: source.display().to_string(),
            line: 0,
            col:  0,
            message: format!(
                "'{}' and '{}' both generate '{}'. This backend writes one flat \
                 directory, so two source files cannot share a name — rename one \
                 of them.",
                first.display(),
                source.display(),
                out_path.display()
            ),
        });
        return false;
    }
    owners.insert(out_path.to_path_buf(), source.to_path_buf());
    true
}

// ── Public entry point ────────────────────────────────────────────────────────

pub fn build(root: &Path, out_dir: &Path, crate_name: &str, backend: &Backend) -> BuildResult {
    let mut errors        = Vec::new();
    let mut files_written = 0;

    let src_out = match backend {
        Backend::Python => out_dir.join(crate_name),
        Backend::Go | Backend::C | Backend::Cpp | Backend::Java => out_dir.to_path_buf(),
        _ => out_dir.join("src"),
    };
    fs::create_dir_all(&src_out).expect("could not create out/src");

    // The root's own main.bu, not "anywhere in the tree". The manifest and
    // Makefile declare an entry point at a fixed path, so anything deeper
    // named one that was never generated (decision 22).
    let has_main = main_bu_path(root).is_some();

    // Collect all structs and enums from all inventories in the tree.
    // Must happen before emit_folder so lower_enum_refs has the full EnumEnv.
    let all_structs  = collect_all_structs(root, &mut errors);
    let all_enums    = collect_all_enums(root, &mut errors);
    let all_natives  = collect_all_natives(root);
    let enum_env: bullang::ast::EnumEnv = all_enums.iter()
        .map(|e| (e.name.clone(), e.clone()))
        .collect();

    let mut owners: OutputOwners = OutputOwners::new();
    let (child_modules, _) = emit_folder(
        root, &src_out, backend, crate_name, has_main, &enum_env, &mut errors,
        &mut files_written, &mut owners,
    );

    match backend {
        Backend::Rust => {
            write_file(
                &src_out.join("lib.rs"),
                &codegen::emit_lib_rs(&child_modules, &all_structs, &all_enums),
                &mut files_written,
                &mut errors,
            );
            let cargo = if has_main {
                codegen::emit_cargo_toml_with_main(crate_name)
            } else {
                codegen::emit_cargo_toml(crate_name)
            };
            write_file(&out_dir.join("Cargo.toml"), &cargo, &mut files_written, &mut errors);
        }
        Backend::Python => {
            write_file(
                &src_out.join("__init__.py"),
                &codegen::emit_init_py(&child_modules, &all_structs, &all_enums),
                &mut files_written,
                &mut errors,
            );
        }
        Backend::C => {
            let header_name = format!("{}.h", crate_name);
            let all_sources = collect_all_sources(root, backend);
            let src_refs: Vec<(String, &bullang::ast::SourceFile)> =
                all_sources.iter().map(|(n, sf)| (n.clone(), sf)).collect();
            let libs   = collect_all_libs(root);
            let header = codegen::emit_header_c(crate_name, &src_refs, &libs, &all_structs, &all_enums);
            write_file(&out_dir.join(&header_name), &header, &mut files_written, &mut errors);

            let needs_ft = src_refs.iter().any(|(_, sf)| codegen::needs_foreign_types(sf));
            if needs_ft {
                write_file(
                    &out_dir.join("foreign_types.h"),
                    include_str!("foreign_types.h"),
                    &mut files_written,
                &mut errors,
                );
            }

            let needs_gen = src_refs.iter().any(|(_, sf)| codegen::needs_generic_types(sf));
            if needs_gen {
                write_file(
                    &out_dir.join("bu_generic.h"),
                    include_str!("bu_generic.h"),
                    &mut files_written,
                &mut errors,
                );
            }

            let mut all_c: Vec<String> = child_modules.iter()
                .map(|m| format!("{}.c", m)).collect();
            if has_main { all_c.push("main.c".to_string()); }
            let makefile = codegen::emit_makefile(crate_name, &all_c, has_main);
            write_file(&out_dir.join("Makefile"), &makefile, &mut files_written, &mut errors);
        }
        Backend::Cpp => {
            let header_name = format!("{}.hpp", crate_name);
            let all_sources = collect_all_sources(root, backend);
            let src_refs: Vec<(String, &bullang::ast::SourceFile)> =
                all_sources.iter().map(|(n, sf)| (n.clone(), sf)).collect();
            let libs   = collect_all_libs(root);
            let header = codegen::emit_header_cpp(crate_name, &src_refs, crate_name, &libs, &all_structs, &all_enums, &all_natives);
            write_file(&out_dir.join(&header_name), &header, &mut files_written, &mut errors);

            let mut all_cpp: Vec<String> = child_modules.iter()
                .map(|m| format!("{}.cpp", m)).collect();
            if has_main { all_cpp.push("main.cpp".to_string()); }
            let makefile = codegen::emit_makefile_cpp(crate_name, &all_cpp, has_main);
            write_file(&out_dir.join("Makefile"), &makefile, &mut files_written, &mut errors);
        }
        Backend::Go => {
            write_file(&out_dir.join("go.mod"), &codegen::emit_go_mod(crate_name), &mut files_written, &mut errors);

            // types.go — inventory structs + enums + Tuple foreign types
            let all_sources = collect_all_sources(root, backend);
            let src_refs: Vec<(String, &bullang::ast::SourceFile)> =
                all_sources.iter().map(|(n, sf)| (n.clone(), sf)).collect();
            let tuple_types = codegen::collect_tuple_types(&src_refs);
            if !all_structs.is_empty() || !all_enums.is_empty() || !tuple_types.is_empty() {
                let pkg = if has_main { "main" } else { crate_name };
                write_file(
                    &out_dir.join("types.go"),
                    &codegen::emit_types_go(pkg, &all_structs, &all_enums, &tuple_types),
                    &mut files_written,
                &mut errors,
                );
            }
        }
        Backend::Java => {
            if !all_structs.is_empty() || !all_enums.is_empty() {
                let types_class = codegen::to_pascal_case(crate_name);
                write_file(
                    &out_dir.join(format!("{}.java", types_class)),
                    &codegen::emit_types_java(&types_class, &all_structs, &all_enums),
                    &mut files_written,
                &mut errors,
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
                Backend::C | Backend::Cpp | Backend::Go | Backend::Java => out_dir.join("blueprint.md"),
                _ => src_out.join("blueprint.md"),
            };
            write_file(&out_path, &bp_content, &mut files_written, &mut errors);
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
    enum_env:   &bullang::ast::EnumEnv,
    errors:     &mut Vec<ValidationError>,
    written:    &mut usize,
    owners:     &mut OutputOwners,
) -> (Vec<String>, Vec<String>) {
    let inv = match read_inventory(src_dir) {
        Ok(i)  => i,
        Err(_) => return (vec![], vec![]),
    };

    let mut child_modules: Vec<String> = Vec::new();
    let mut all_fns:       Vec<String> = Vec::new();

    // A folder declaring its own `#lang` is a separate region, built into its
    // own output directory with its own build file (decision 13). Descending
    // into one here would emit its files a second time, in the wrong language,
    // inside this region's output.
    let subdirs_in_region = |dir: &Path| -> Vec<std::path::PathBuf> {
        collect_subdirs(dir).into_iter()
            .filter(|d| read_inventory(d).map(|i| i.lang.is_none()).unwrap_or(true))
            .collect()
    };

    // War: only sub-folders (+ optional main.bu)
    if inv.rank == Rank::War {
        for subdir in subdirs_in_region(src_dir) {
            let name      = dir_name(&subdir);
            let child_out = out_dir.join(&name);
            fs::create_dir_all(&child_out).ok();
            let (gc, fns) = emit_folder(&subdir, &child_out, backend, crate_name, has_main, enum_env, errors, written, owners);
            emit_mod_file(&child_out, &gc, backend, written, errors);
            merge(&fns, &mut all_fns);
            child_modules.push(name);
        }
        if let Some(mp) = main_bu_path(src_dir) {
            emit_main_file(&mp, out_dir, backend, crate_name, &collect_all_libs(src_dir), enum_env, errors, written);
        }
        emit_go_runtime(src_dir, out_dir, backend, crate_name, has_main, enum_env, written, errors);
        return (child_modules, all_fns);
    }

    // Sub-folders first (bottom-up)
    if inv.rank.has_sub_folders() {
        for subdir in subdirs_in_region(src_dir) {
            let name = dir_name(&subdir);
            let child_out = match backend {
                Backend::C | Backend::Cpp | Backend::Go | Backend::Java => out_dir.to_path_buf(),
                _ => {
                    let co = out_dir.join(&name);
                    fs::create_dir_all(&co).ok();
                    co
                }
            };
            let (gc, fns) = emit_folder(&subdir, &child_out, backend, crate_name, has_main, enum_env, errors, written, owners);
            if !matches!(backend, Backend::C | Backend::Cpp | Backend::Go | Backend::Java) {
                emit_mod_file(&child_out, &gc, backend, written, errors);
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
            bullang::ast::lower_enum_refs(&mut sf, enum_env);
            crate::sanitize::normalize(&mut sf, backend);

            merge(&entry.functions, &mut all_fns);

            let ext = match backend.ext() {
                Some(e) => e,
                None => {
                    errors.push(ValidationError {
                        file: bu_path.display().to_string(),
                        line: 0,
                        col:  0,
                        message: format!(
                            "'{}' is not a backend Bullarchy can generate for.",
                            backend.escape_keyword()
                        ),
                    });
                    continue;
                }
            };
            // Java requires a public class to live in a file named after it,
            // so the output file takes the class name rather than the .bu
            // stem: greet.bu -> Greet.java, not greet.java.
            let out_stem = match backend {
                Backend::Java => codegen::to_pascal_case(&entry.file),
                _             => entry.file.clone(),
            };
            let out_path = out_dir.join(format!("{}.{}", out_stem, ext));
            if !claim_output(&out_path, &bu_path, owners, errors) {
                continue;
            }
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
                Backend::Go          => codegen::emit_source_go(&sf, &go_pkg, &collect_all_libs(src_dir)),
                Backend::Java        => codegen::emit_source_java(&sf, &codegen::to_pascal_case(&entry.file)),
                Backend::Unknown(_)  => continue,
            };
            write_file(&out_path, &content, written, errors);
            child_modules.push(entry.file.clone());
        }
    }

    // main.bu at non-skirmish levels
    if inv.rank != Rank::Skirmish {
        if let Some(mp) = main_bu_path(src_dir) {
            emit_main_file(&mp, out_dir, backend, crate_name, &collect_all_libs(src_dir), enum_env, errors, written);
        }
    }

    emit_go_runtime(src_dir, out_dir, backend, crate_name, has_main, enum_env, written, errors);
    (child_modules, all_fns)
}

/// Go's builtin helpers, written once per package.
///
/// Every file in a Go package shares one namespace, so a helper hoisted into
/// each file that needs it is a redeclaration. `bu_runtime.go` holds them for
/// the whole package instead. No other backend needs this — see
/// `codegen_go::emit_runtime_go`.
fn emit_go_runtime(
    src_dir:    &Path,
    out_dir:    &Path,
    backend:    &Backend,
    crate_name: &str,
    has_main:   bool,
    enum_env:   &bullang::ast::EnumEnv,
    written:    &mut usize,
    errors:     &mut Vec<ValidationError>,
) {
    if !matches!(backend, Backend::Go) {
        return;
    }
    
    let mut owned: Vec<bullang::ast::SourceFile> = Vec::new();
    for path in bu_files_in(src_dir) {
        let Ok(source) = fs::read_to_string(&path) else { continue };
        if let Ok(BuFile::Source(mut sf)) = parser::parse_file(&source, false) {
            bullang::ast::lower_enum_refs(&mut sf, enum_env);
            crate::sanitize::normalize(&mut sf, backend);
            owned.push(sf);
        }
    }
    let refs: Vec<&bullang::ast::SourceFile> = owned.iter().collect();
    let package = if has_main { "main" } else { crate_name };
    if let Some(content) = codegen::emit_runtime_go(&refs, package) {
        write_file(&out_dir.join("bu_runtime.go"), &content, written, errors);
    }
}

/// Every `.bu` file directly in `dir`, including `main.bu`.
fn bu_files_in(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("bu")
            && path.file_name().and_then(|n| n.to_str()) != Some("inventory.bu")
        {
            out.push(path);
        }
    }
    out.sort();
    out
}

// ── Module file emitter ───────────────────────────────────────────────────────

fn emit_mod_file(dir: &Path, child_modules: &[String], backend: &Backend, written: &mut usize, errors: &mut Vec<ValidationError>) {
    match backend {
        Backend::Rust => {
            write_file(&dir.join("mod.rs"), &codegen::emit_mod_rs(child_modules), written, errors);
        }
        Backend::Python => {
            write_file(
                &dir.join("__init__.py"),
                &codegen::emit_init_py(child_modules, &[], &[]),
                written,
                errors,
            );
        }
        Backend::C | Backend::Cpp | Backend::Go | Backend::Java | Backend::Unknown(_) => {}
    }
}

// ── main.bu emitter ───────────────────────────────────────────────────────────

fn emit_main_file(
    main_path:  &Path,
    out_dir:    &Path,
    backend:    &Backend,
    crate_name: &str,
    // `#lib:` entries in scope. Go alone consumes these at source-emission
    // time — C and C++ put theirs in the shared header instead.
    libs:       &[String],
    enum_env:   &bullang::ast::EnumEnv,
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
    bullang::ast::lower_enum_refs(&mut sf, enum_env);
    crate::sanitize::normalize(&mut sf, backend);

    let header_name = format!("{}.h", crate_name);
    let hpp_name    = format!("{}.hpp", crate_name);
    match backend {
        Backend::Rust => {
            write_file(
                &out_dir.join("main.rs"),
                &codegen::emit_main(&sf, crate_name),
                written,
                errors,
            );
        }
        Backend::Python => {
            write_file(
                &out_dir.join("__main__.py"),
                &codegen::emit_main_py(&sf, crate_name),
                written,
                errors,
            );
        }
        Backend::C => {
            write_file(
                &out_dir.join("main.c"),
                &codegen::emit_main_c(&sf, &header_name),
                written,
                errors,
            );
        }
        Backend::Cpp => {
            write_file(
                &out_dir.join("main.cpp"),
                &codegen::emit_main_cpp(&sf, &hpp_name, crate_name),
                written,
                errors,
            );
        }
        Backend::Go => {
            write_file(
                &out_dir.join("main.go"),
                &codegen::emit_main_go(&sf, crate_name, libs),
                written,
                errors,
            );
        }
        Backend::Java => {
            write_file(
                &out_dir.join("Main.java"),
                &codegen::emit_main_java(&sf, crate_name),
                written,
                errors,
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
        if let bullang::ast::BulletBody::Natives(blocks) = &func.body {
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
/// Every struct in the tree, with the inventory that declared it.
///
/// Deduplicating by name was wrong twice over. Two folders can legitimately
/// declare unrelated types that happen to share a name, and the old code
/// silently kept the first and dropped the second — so a function compiled
/// against a `Point` with entirely different fields. That is a name clash the
/// author has to resolve, not something to paper over, because the generated
/// code puts them all in one namespace.
fn collect_all_structs(
    dir:    &Path,
    errors: &mut Vec<ValidationError>,
) -> Vec<bullang::ast::StructDef> {
    let mut result: Vec<(bullang::ast::StructDef, std::path::PathBuf)> = Vec::new();
    collect_structs_into(dir, &mut result, errors);
    result.into_iter().map(|(s, _)| s).collect()
}

fn collect_structs_into(
    dir:    &Path,
    out:    &mut Vec<(bullang::ast::StructDef, std::path::PathBuf)>,
    errors: &mut Vec<ValidationError>,
) {
    let inv_path = dir.join("inventory.bu");
    if let Ok(inv) = read_inventory(dir) {
        for s in inv.structs {
            match out.iter().find(|(existing, _)| existing.name == s.name) {
                Some((_, first)) => errors.push(ValidationError {
                    file: inv_path.display().to_string(),
                    line: 0, col: 0,
                    message: format!(
                        "struct '{}' is declared in both '{}' and '{}'. Generated \
                         code puts every type in one namespace, so the names must \
                         differ.",
                        s.name, first.display(), inv_path.display()
                    ),
                }),
                None => out.push((s, inv_path.clone())),
            }
        }
    }
    for subdir in collect_subdirs(dir) {
        collect_structs_into(&subdir, out, errors);
    }
}

/// Every enum in the tree. Same reasoning as `collect_all_structs`.
fn collect_all_enums(
    dir:    &Path,
    errors: &mut Vec<ValidationError>,
) -> Vec<bullang::ast::EnumDef> {
    let mut result: Vec<(bullang::ast::EnumDef, std::path::PathBuf)> = Vec::new();
    collect_enums_into(dir, &mut result, errors);
    result.into_iter().map(|(e, _)| e).collect()
}

fn collect_enums_into(
    dir:    &Path,
    out:    &mut Vec<(bullang::ast::EnumDef, std::path::PathBuf)>,
    errors: &mut Vec<ValidationError>,
) {
    let inv_path = dir.join("inventory.bu");
    if let Ok(inv) = read_inventory(dir) {
        for e in inv.enums {
            match out.iter().find(|(existing, _)| existing.name == e.name) {
                Some((_, first)) => errors.push(ValidationError {
                    file: inv_path.display().to_string(),
                    line: 0, col: 0,
                    message: format!(
                        "enum '{}' is declared in both '{}' and '{}'. Generated \
                         code puts every type in one namespace, so the names must \
                         differ.",
                        e.name, first.display(), inv_path.display()
                    ),
                }),
                None => out.push((e, inv_path.clone())),
            }
        }
    }
    for subdir in collect_subdirs(dir) {
        collect_enums_into(&subdir, out, errors);
    }
}

fn collect_all_natives(dir: &Path) -> Vec<bullang::ast::NativeBlock> {
    let mut result = Vec::new();
    let inv = match read_inventory(dir) {
        Ok(i) => i, Err(_) => return result,
    };
    for nb in inv.natives {
        // Escape blocks were the one collection never deduplicated, so the
        // same block declared in two folders was emitted twice into one file —
        // a redefinition in every target language.
        if !result.iter().any(|r: &bullang::ast::NativeBlock| {
            r.backend == nb.backend && r.code == nb.code
        }) {
            result.push(nb);
        }
    }
    for subdir in collect_subdirs(dir) {
        for nb in collect_all_natives(&subdir) {
            if !result.iter().any(|r: &bullang::ast::NativeBlock| {
                r.backend == nb.backend && r.code == nb.code
            }) {
                result.push(nb);
            }
        }
    }
    result
}

fn collect_all_sources(dir: &Path, backend: &Backend) -> Vec<(String, bullang::ast::SourceFile)> {
    let mut result = Vec::new();
    let inv = match read_inventory(dir) {
        Ok(i) => i, Err(_) => return result,
    };
    for entry in &inv.entries {
        let bu_path = dir.join(format!("{}.bu", entry.file));
        if let Ok(source) = std::fs::read_to_string(&bu_path) {
            if let Ok(bullang::ast::BuFile::Source(mut sf)) = parser::parse_file(&source, false) {
                crate::sanitize::normalize(&mut sf, backend);
                result.push((entry.file.clone(), sf));
            }
        }
    }
    for subdir in collect_subdirs(dir) {
        result.extend(collect_all_sources(&subdir, backend));
    }
    result
}

// ── Tree scan ─────────────────────────────────────────────────────────────────

// ── Utilities ─────────────────────────────────────────────────────────────────

/// Write one generated file, recording either the write or the reason it
/// failed.
///
/// The old version tested `fs::write(..).is_ok()` and, when it was not, did
/// nothing at all: no error, no count, no message. A `convert` into a
/// read-only directory or a full disk therefore reported success having
/// written nothing — the single most misleading outcome the tool could
/// produce.
///
/// A failed *formatter* is different in kind and stays a warning: the file is
/// written unformatted, which is correct output that merely reads worse, and
/// requiring `rustfmt` or `clang-format` to be installed in order to transpile
/// would be a surprising dependency.
fn write_file(
    path:    &Path,
    content: &str,
    written: &mut usize,
    errors:  &mut Vec<ValidationError>,
) {
    if let Some(parent) = path.parent() {
        if let Err(e) = fs::create_dir_all(parent) {
            errors.push(write_err(path, e));
            return;
        }
    }
    let formatted = crate::utils::format_source(path, content);
    match fs::write(path, formatted.as_deref().unwrap_or(content)) {
        Ok(()) => *written += 1,
        Err(e) => errors.push(write_err(path, e)),
    }
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

fn write_err(path: &Path, e: std::io::Error) -> ValidationError {
    ValidationError { file: path.display().to_string(), line: 0, col: 0,
        message: format!("Could not write: {}", e) }
}

fn parse_err(path: &Path, e: Box<dyn std::error::Error>) -> ValidationError {
    ValidationError { file: path.display().to_string(), line: 0, col: 0,
        message: format!("Parse error: {}", e) }
}
