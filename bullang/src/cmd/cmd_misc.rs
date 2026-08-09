//! `bullang stdlib` — browse the core builtin catalogue.

use bullang::stdlib::{self, Category};

pub fn cmd_stdlib() {
    println!("Bullang core standard library");
    println!("Available in every backend.");
    println!();

    for category in Category::ALL {
        let entries: Vec<_> = stdlib::by_category(*category).collect();
        if entries.is_empty() {
            continue;
        }
        let title = category.title();
        println!("  {}", title);
        println!("  {}", "-".repeat(title.len()));
        for b in entries {
            println!("    builtin::{:<14}  {}", b.name, b.signature);
            println!("    {:<14}            {}", "", b.description);
        }
        println!();
    }

    println!("Usage in a source file:");
    println!();
    println!("  let upper(s: String) -> result: String {{");
    println!("      (s) : builtin::to_upper -> {{result}};");
    println!("  }}");
    println!();
    println!("A builtin can also be a whole bullet body, taking the function's");
    println!("own parameters as its arguments:");
    println!();
    println!("  let upper(s: String) -> result: String {{");
    println!("      builtin::to_upper");
    println!("  }}");
    println!();
    println!("Anything beyond this core set lives in a package. Install one with");
    println!("`bullarchy add <name>`, then declare it in inventory.bu:");
    println!();
    println!("  #use: mathlib;");
}
