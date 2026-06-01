# Bullang

Bullang defines the `.bu` language: grammar, parser, AST, type system, and standard library.

It is the foundation of the Bullang ecosystem. Bullarch and Bullscript depend on it as a library crate — but Bullang itself has no dependency on either. It can be installed and used alone.

---

## Prerequisite

Cargo v1.92.0 or later.

## Installation

### Language registry only

```bash
cargo install --git https://github.com/My-sidequests/Bullang.git
```

If you are reinstalling over an existing version, add `--force`:

```bash
cargo install --git https://github.com/My-sidequests/Bullang.git --force bullang
```

### Full suite

```bash
cargo install --git https://github.com/My-sidequests/Bullang.git --force bullang \
  && cargo install --git https://github.com/My-sidequests/Bullarchy.git --force bullarchy \
  && cargo install --git https://github.com/My-sidequests/Bullscript.git --force bullscript
```

### Update

```bash
bullang update
```

Already installed and want to force a reinstall regardless of version:

```bash
cargo install --git https://github.com/My-sidequests/Bullang.git --force bullang
```

---

## What Bullang provides

### The `bullang` binary

```bash
bullang stdlib   # list all builtins with signatures and descriptions
bullang update   # update to the latest version
```

### The interpreter

Bullang source files can be executed directly — no transpilation needed. The interpreter is exposed through `bullang::interpreter` and used by Bullscript's `run` command. See [Bullscript](https://github.com/My-sidequests/Bullscript) for the user-facing command.

The full standard library is supported, including file I/O (`open`, `close`, `in`, `out`), sorting algorithms, environment variables, and all math and string builtins.

Any bullet whose body is a native escape block (`@rust { ... }`, `@c { ... }`, etc.) cannot be interpreted — use `bullarchy convert` for those. The `bullang::checker` module validates this before execution.

### The `bullang` library crate

Bullarchy and Bullscript depend on this crate directly. It exports:

- `bullang::ast` — the full AST type hierarchy
- `bullang::parser` — `parse_file`, `parse_source`, `ParseError`
- `bullang::fmt` — `format_source`, `format_inventory`
- `bullang::stdlib` — `list_builtins`, builtin metadata
- `bullang::interpreter` — `run(&SourceFile)`, `Value`, `InterpError`
- `bullang::checker` — `check_no_escape(&SourceFile)`, `EscapeViolation`

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

For LSP support and editor configuration (Neovim, Helix, Emacs), use [Bullarchy](https://github.com/My-sidequests/Bullarchy) and run `editor-setup`.
