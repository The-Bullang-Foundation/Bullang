# Bullang for Zed

Syntax highlighting and language-server support for `.bu` files.

## Why this exists as an extension

Every other editor `bullarchy editor-setup` handles is configured by writing a
file — Neovim, Vim, Helix and Emacs all accept "run this command for this file
type". Zed does not: it will not recognise a new language without an extension,
and an extension must name a tree-sitter grammar. So Zed needs a compiled
artifact where the others need three lines of config.

The extension itself does almost nothing. It tells Zed to run `bullarchy lsp`,
found on your PATH. The diagnostics come from Bullarchy's own validator and
type checker; the highlighting comes from the grammar.

## Installing it

Until it is in the extension registry:

1. Open the command palette and run **zed: install dev extension**.
2. Point it at this directory.

`bullarchy` must be on your PATH. If it is not, the extension says so rather
than failing silently.

## Layout

```
zed-bullang/
├── extension.toml           grammar source and language-server registration
├── Cargo.toml               compiled to wasm32-wasip1 by Zed
├── src/lib.rs               finds bullarchy, runs `bullarchy lsp`
└── languages/bullang/
    ├── config.toml          file suffixes, comments, brackets
    ├── highlights.scm       syntax highlighting
    ├── brackets.scm         bracket matching
    ├── indents.scm          auto-indentation
    └── outline.scm          the outline panel
```

The grammar lives in `../tree-sitter-bullang`. Before publishing, set the `rev`
in `extension.toml` to a commit of the grammar repository — Zed fetches it from
there, pinned, rather than from this tree.
