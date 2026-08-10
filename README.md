# Bullang

Bullang is a language you write once and transpile into several target
languages. Its unit of code is the **bullet**: one input list, one operation,
one named result, on one line.

```
let calculus(a: i64, b: i64, c: i64) -> result: i64 {
    (a, b)  : a + b   -> {ab};
    (ab, c) : ab * c  -> {result};
}
```

That transpiles to Rust, Python, C, C++, Go or Java — real source you compile
with the target's own toolchain. **Bullang does not run your code.** It
generates code that something else runs.

This repository holds two crates:

| Crate | What it is |
|---|---|
| **`bullang/`** | The language: grammar, AST, parser, formatter, builtin catalogue |
| **`bullarchy/`** | The toolchain: project scaffolding, transpilation, validation, package management, LSP, GUI |

They are one repository because they move together, and two crates because a
package installed with `bullarchy add` needs `bullang` for
`ast::{Backend, Param}` — the signature every builtin emitter is written
against. One crate would make every package depend on six code generators, an
LSP and an HTTP client just to describe one function.

For a scripting language with its own interpreter, see
[BullScript](https://github.com/The-Bullang-Foundation/Bullscript) — a separate
language with a separate grammar, running `.busc` files. It borrows Bullang's
bullet syntax for familiarity and shares none of its code.

---

## Installation

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git bullang bullarchy
```

Add `--force` when reinstalling. Both binaries come from this one repository,
so they cannot end up a version apart.

There is also a
[graphical installer](https://github.com/The-Bullang-Foundation/bullang-installer)
that sets up Rust, Go, the target language toolchains and both binaries, and a
[sudo-free updater](https://github.com/The-Bullang-Foundation/bullang-installer-cli)
for machines where the toolchains are already present.

**Prerequisite:** Cargo 1.85 or later (edition 2024).

---

## Getting started

```bash
bullarchy init my_project --lang rs
cd my_project
bullarchy check
bullarchy convert .
```

`init` writes a `main.bu` that runs, so a new project validates and transpiles
without editing anything. `convert` writes its output beside the project, as
`_my_project`, with the target's own build file — a `Cargo.toml`, a `Makefile`,
a `go.mod`, whatever fits.

---

## The two binaries

```bash
bullarchy                 # the GUI
bullarchy --cli           # the terminal REPL
bullarchy <command>       # one command and exit
bullarchy --help          # every command; <command> --help explains one
```

| Command | |
|---|---|
| `init` | scaffold a project |
| `convert` | transpile a project, or a single `.bu` file |
| `check` | validate and type-check |
| `fmt` | reformat to canonical style |
| `add` / `remove` | install and uninstall packages |
| `stdlib` | list the core standard library |
| `editor-setup` | write LSP config for Vim, Neovim, Helix and Emacs |
| `update` | reinstall from this repository |
| `lsp` | run the language server on stdin/stdout |

```bash
bullang stdlib            # browse the core builtin catalogue
```

That is all the `bullang` binary does. Everything else is `bullarchy`.

---

## The language, briefly

Two ideas explain almost every rule.

**One operation per bullet.** `a + b` is a bullet; `a + b + c` is two. There is
no operator precedence to remember and no parentheses to trace, because the
shape of the code is the order of evaluation. A call cannot nest inside another
call for the same reason — write the inner call as its own bullet and pass its
binding.

**Only what translates faithfully.** A feature earns its place by having an
honest counterpart in all six backends. References, closures, function types
and fixed-size arrays did not, and were removed rather than approximated in five
languages and faked in the sixth. Collections will come back as one designed
feature — literals, indexing, length and iteration together — rather than as a
type with no way to build a value.

### Bullets

```
(inputs) : expression -> {binding};
```

| Part | Meaning |
|---|---|
| `(inputs)` | values passed in — identifiers or literals |
| `: expression` | the operation: arithmetic, a call, or a builtin |
| `-> {binding}` | names the result; `{}` discards it |

The last bullet binds the function's declared output.

```
(a, b)    : a + b              -> {sum};
(sum)     : builtin::i64_to_str -> {text};
(1, text) : builtin::out        -> {};
```

What the inputs *mean* depends on what the expression is:

| Bullet | Means |
|---|---|
| `(a, b) : a + b -> {sum};` | evaluate `a + b` |
| `(a, b) : add -> {sum};` | call `add(a, b)` |
| `(s) : builtin::to_upper -> {r};` | call the builtin with `s` |
| `(x) : some_fn(x, 2) -> {r};` | call it as written |

Operators: `+ - * / %`, `== != < <= > >=`, `&& ||`, unary `!` and `-`. Strings
interpolate with `{name}`.

### Types

| Type | |
|---|---|
| `i64` | 64-bit integer |
| `f64` | floating point |
| `bool` | true / false |
| `String` | UTF-8 text |
| `Tuple[A, B]` | a pair |
| `()` | no value |

Structs and enums are declared in `inventory.bu` and usable by name in the same
folder and one rank above.

### Escape blocks

When a function needs something only one target can express:

```
let fast_add(a: i64, b: i64) -> result: i64 {
@rust
    let result = a.wrapping_add(b);
    result
@end
}
```

An escape block is a macro. Bullang decides three things about it — where it
starts, where it ends, and which backend it names — and copies everything
between into the generated file **byte for byte**, indentation included. It is
never parsed, never reindented, never validated. One block per function, and a
function cannot mix bullets with a block.

Nothing is inferred from inside a block: whatever it needs, declare with
`#lib:`.

---

## Project structure

A project is a folder hierarchy, and every folder has a **rank**:

```
war → theater → battle → strategy → tactic → skirmish
```

Every folder carries an `inventory.bu` declaring its rank, its target language,
its dependencies, its types and its files:

```
#rank: tactic;
#lang: rs;
#lib: stdio.h;
#use: mathlib;

struct Point { x: i64, y: i64 }

math   : add, subtract;
shapes : area, perimeter;
```

`#lib` and `#use` are different things. `#lib` names a native header or import
**of the target language** — `stdio.h`, `os/exec`, `strings`. `#use` names a
**Bullang package**, installed with `bullarchy add`.

A function declared at a lower rank is callable one rank above. Each folder is
limited to five sub-folders, five source files, five functions per file and five
bullets per function — limits that keep a project readable at a glance.

### Language regions

A subtree may declare its own `#lang`. That subtree becomes a **region**: it is
transpiled to its own language, into its own output directory, with its own
build file. A call may not cross a region boundary, because Bullang generates no
FFI and there would be nothing to generate the call into.

---

## The standard library

The core is deliberately small — run `bullarchy stdlib` for the full list.
Anything beyond it is a package:

```bash
bullarchy add mathlib
```

```
#use: mathlib;
```

`bull-mathlib` supplies `abs`, `pow`, `powf`, `sqrt`, `clamp`, `log`, `exp` and
four sorting builtins. They are not core, and were never meant to be.

---

## Editor support

```bash
bullarchy editor-setup
```

Writes LSP configuration for Vim, Neovim, Helix and Emacs. A VS Code extension
lives in `bullang/bullang-vscode/`, and Vim syntax files in `bullang/vim/`.

The language server runs as `bullarchy lsp` over stdin/stdout, and provides
diagnostics, hover and go-to-definition. BullScript has its own, `bullscript
lsp`, for `.busc` files; `editor-setup` configures both when both are
installed.

Zed is the one editor that cannot be configured by writing a file — it needs an
extension, in `zed-bullang/`. See its README.

---

## Documentation

[The Bullang Book](docs/Bullang-Book.md) is the full reference: the language,
BullScript, the toolchain, and a quick-reference section.

---

## Repository layout

```
Bullang/
├── Cargo.toml              workspace
├── bullang/                the language
│   ├── src/                grammar, AST, parser, formatter, catalogue
│   ├── bullang-vscode/     VS Code extension
│   └── vim/                Vim and Neovim syntax files
├── bullarchy/              the toolchain
│   ├── src/                codegen, validator, LSP, CLI
│   └── gui/                the graphical interface (Go / Fyne)
├── tree-sitter-bullang/    grammar for Zed and Helix
├── zed-bullang/            Zed extension
└── docs/                   the Bullang Book
```

Editor support shares one token model across four files — the VS Code TextMate
grammar is the reference, and the tree-sitter queries and Vim syntax mirror it.
A change to the language's vocabulary is a change to all four.

Building both:

```bash
cargo build             # from the repository root
```

The GUI is Go rather than Rust and is built separately:

```bash
cd bullarchy/gui && go build ./...
```
