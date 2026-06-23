# Bullang

Bullang is a functionnal interpreted programming language, meant to simplify writing secure code.
It's main attributes is that it can be transpiled into multiple target languages.

The Bullang language is the foundation of the Bullang ecosystem. 
Bullarchy and Bullscript depend on it, but Bullang itself has no dependency on either. 
It can be installed and used alone, also we advise using those tools to enhance your workflow.

To learn the language and tools, go to https://github.com/The-Bullang-Foundation/Bullang-Book

---

## Prerequisite

Cargo v1.92.0 or later.

## Installation

### Language registry only

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git
```

If you are reinstalling over an existing version, add `--force`:

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git --force bullang
```

### Full suite

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git --force bullang \
  && cargo install --git https://github.com/The-Bullang-Foundation/Bullarchy.git --force bullarchy \
  && cargo install --git https://github.com/The-Bullang-Foundation/Bullscript.git --force bullscript
```

### Update

```bash
bullang update
```

## What Bullang provides

### The `bullang` binary

```bash
bullang stdlib   # list all builtins with signatures and descriptions
bullang update   # update to the latest version
```

### The interpreter

Bullang source files can be executed directly using Bullscript. See [Bullscript](https://github.com/The-Bullang-Foundation/Bullscript) for the user-facing command.

The full standard library is supported, including file I/O (`open`, `close`, `in`, `out`), sorting algorithms, environment variables, and all math and string builtins.

Any bullet whose body is a native escape block (`@rust { ... }`, `@c { ... }`, etc.) cannot be interpreted — use `bullarchy convert` for those. The `bullang::checker` module validates this before execution.

---

## Language overview

### Function syntax

```
let add(a: i32, b: i32) -> result: i32 {
    (a, b) : a + b -> {result};
}
```

### Entry point convention

When a file is run directly, Bullscript looks for a zero-argument `main` bullet:

```
let main() -> result: Unit {
    (1, 2) : a + b -> {sum};
    (sum)  : builtin::out(1, "{sum}\n") -> {result};
}
```

Use `builtin::out` to print — there is no implicit output.

Each line inside a function is a **pipe**: inputs on the left, expression in the middle, named binding on the right. The last binding is the return value.

### Standard library

Builtins are available in every backend and declared using the `builtin::` prefix:

```
let upper(s: String) -> result: String {
    builtin::to_upper
}

let read_line(fd: i32) -> result: String {
    builtin::in
}
```

Run `bullang stdlib` for the full catalogue with signatures and descriptions.

---

## Project structure

Every folder holds an `inventory.bu` — its rank declaration, optional language and library directives, struct and enum definitions, and the list of source files with their functions.
This is enforced throught Bullarchy when transpilling Bullang into a target language. That strict rank system allows for both abstraction and clarity.

Folders nest from `war` down to `skirmish`. Functions and structs defined at a lower rank are available one rank above. Build below to use above.

```
war
└── theater
    └── battle
        └── strategy
            └── tactic
                └── skirmish
```

---

## Editor support

VS Code: install the extension through the VS Code extension page.

For LSP support and editor configuration (Neovim, Helix, Emacs), use [Bullarchy](https://github.com/The-Bullang-Foundation/Bullarchy) and run `editor-setup`.
