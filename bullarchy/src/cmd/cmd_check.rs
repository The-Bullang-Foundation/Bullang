//! `bullarchy check` — validate and type-check the project from the current directory.
//! Also reports any files not in canonical format — run `bullarchy fmt` to fix.

use crate::{validator, typecheck};
use crate::utils::{current_dir, find_root_from, print_all_errors, print_type_errors};

pub fn cmd_check() {
    let root = find_root_from(&current_dir());
    let rank = match validator::read_folder_rank(&root) {
        Some(r) => r,
        None => {
            eprintln!(
                "error: no readable '#rank' in '{}/inventory.bu'.\n       \
                 Every folder needs one — run `bullarchy init` to scaffold a \
                 project, or add '#rank: tactic;' as the file's first line.",
                root.display()
            );
            std::process::exit(1);
        }
    };

    println!("bullarchy check");
    println!("  root : {} ({})", root.display(), rank.name());
    println!();

    let all_errors = validator::validate_tree(&root);
    if !all_errors.is_empty() {
        print_all_errors(&all_errors);
        std::process::exit(1);
    }

    let type_errors = typecheck::typecheck_tree(&root);
    if !type_errors.is_empty() {
        print_type_errors(&type_errors);
        std::process::exit(1);
    }

    // Format check — report drift without writing anything
    let unformatted = crate::cmd::cmd_fmt::check_formatting(&root);
    if !unformatted.is_empty() {
        eprintln!();
        eprintln!("formatting errors — run `bullarchy fmt` to fix:");
        for path in &unformatted {
            eprintln!("  {}", path.display());
        }
        eprintln!();
        eprintln!("{} file(s) not in canonical format", unformatted.len());
        std::process::exit(1);
    }

    println!("ok -- no errors found");
}
