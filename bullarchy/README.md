# Bullarchy

Bullarchy is the unified toolchain for [Bullang](https://github.com/The-Bullang-Foundation/Bullang) projects. It handles project scaffolding, formatting, validation, transpilation, package management, and editor integration — available both as a graphical interface and a command-line tool.

It depends on Bullang as a library crate and can be installed independently — Bullang does not need to be installed as a binary for Bullarchy to work.

---

## Prerequisite

Cargo v1.92.0 or later.

## Installation

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullarchy.git
```

If you are reinstalling over an existing version, add `--force`:

```bash
cargo install --git https://github.com/The-Bullang-Foundation/Bullarchy.git --force bullarchy
```

After installation, `bullarchy` is available from anywhere.

---

## Usage

```bash
bullarchy
```

Launches the native graphical interface (`bullarchy-gui`, built with Go/Fyne). Bullarchy looks for the `bullarchy-gui` binary next to itself, then in `~/.local/bin`, `/usr/local/bin`, or the Windows `AppData\Local\Programs` folder. If it isn't found, Bullarchy falls back to the CLI REPL and prints install instructions.

```bash
bullarchy --cli
```

Launches the interactive terminal prompt:

```
command ->
```

```bash
bullarchy <command> [options]
```

Runs a command directly from the terminal and exits — no prompt, no GUI.

---

## Commands

All commands are available in the GUI, the terminal REPL, and as direct arguments.

### `init`

Scaffold a new Bullang project.

```bash
bullarchy init my_project
bullarchy init my_project --depth 4
bullarchy init my_project --lang c --lib stdio.h
bullarchy init my_project --blueprint blueprint.bu
bullarchy init my_project --blueprint blueprint.bu --lang go
```

Options:

- `--depth N` — hierarchy depth from 1 (skirmish only) to 6 (full war chain). Default: 2.
- `--lang ext` — target language (`rs`, `py`, `c`, `cpp`, `go`, `java`). Written to inventory as `#lang:`.
- `--lib header` — external library declaration. Can be repeated for multiple libraries.
- `--blueprint file` — initialize from a `blueprint.bu` file instead of a depth value. Depth is inferred from the blueprint.
- `--path dir` — where to create the project (default: current directory).

Depth reference:

```
depth 1 → skirmish
depth 2 → tactic → skirmish
depth 3 → strategy → tactic → skirmish
depth 4 → battle → strategy → tactic → skirmish
depth 5 → theater → battle → strategy → tactic → skirmish
depth 6 → war → theater → battle → strategy → tactic → skirmish
```

---

### `convert`

Transpile a Bullang project folder or a single `.bu` file.

```bash
bullarchy convert my_project
bullarchy convert my_project -e py
bullarchy convert path/to/file.bu
bullarchy convert path/to/file.bu -o out.rs
```

Options:

- `-n name` — output folder name (project mode).
- `-e ext` — target language (`rs`, `py`, `c`, `cpp`, `go`, `java`). Overrides `#lang` from inventory.
- `--out dir` — explicit output path (project mode).
- `-o file` — output file (single-file mode; omit to write to stdout).

---

### `add`

Browse and install Bullang packages from the registry.

```bash
bullarchy add                        # list all available packages
bullarchy add netlib                 # install latest version
bullarchy add netlib@v1.2.0          # install a specific version
bullarchy add https://github.com/... # install directly from a git URL
```

Packages are installed globally to `~/.bull/packages/` and are available to any Bullang project. To use a package, add `#lib: <name>;` to your project's `inventory.bu`.

---

### `fmt`

Format all `.bu` files in the project to canonical style. Rewrites files in place. Escape block contents are never modified.

```bash
bullarchy fmt
bullarchy fmt my_project
bullarchy fmt --dry-run
```

- With no argument, formats from the current directory.
- `--dry-run` shows which files would change without writing anything.

---

### `check`

Validate and type-check the project from the current directory. Also reports any files not in canonical format — run `fmt` to fix.

```bash
bullarchy check
```

Runs three passes in order:

1. Structural validation (rank hierarchy, inventory consistency, function declarations)
2. Type checking
3. Format drift check

---

### `editor-setup`

Write LSP configuration files for detected editors.

```bash
bullarchy editor-setup
```

Supports: Neovim (nvim-lspconfig), Helix, Emacs (eglot).
For VS Code: install the extension through the VS Code extension page.

The LSP server is built into Bullarchy and started directly by editors via `bullarchy lsp`.

---

### `update`

Clean-reinstall the whole Bullang toolset from the latest commit on each repo's main branch: Bullang, Bullarchy, and Bullscript via `cargo install --force`, plus a rebuild of `bullarchy-gui` from the `gui/` folder of the freshly cloned Bullarchy source (requires Go — this step mirrors what the GUI installer does, minus installing target-language toolchains).

```bash
bullarchy update
```

---

### `help`

Print the list of available commands.

```bash
bullarchy help
```

---

## LSP server

Bullarchy includes the Bullang language server. Editors invoke it directly:

```bash
bullarchy lsp
```

Capabilities: diagnostics, hover (function signatures), go-to-definition. Run `editor-setup` to have Bullarchy write the configuration for your editor automatically.
