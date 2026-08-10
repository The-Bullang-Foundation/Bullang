# tree-sitter-bullang

A [tree-sitter](https://tree-sitter.github.io) grammar for Bullang, used for
syntax highlighting and code structure.

**`bullang/src/grammar.pest` remains the authority on what Bullang is.** This
grammar exists because Zed requires a tree-sitter grammar for any language an
extension defines, and it is deliberately more permissive: tree-sitter runs on
every keystroke, including on text that is mid-edit and not yet valid, and a
grammar that refused such input would leave the buffer unhighlighted exactly
when a reader most wants the help. Real errors come from the language server,
which runs Bullarchy's own parser.

Both `grammar.pest` and this grammar accept source files and inventory files,
because Bullang uses one extension for both.

An escape block (`@rust … @end`) is matched as a single opaque token. Its
contents are another language and Bullang does not read them, so highlighting
them as Bullang would be actively misleading.

## Building

```bash
npm install -g tree-sitter-cli
tree-sitter generate
tree-sitter parse path/to/file.bu
```

`src/` is committed, as is conventional for tree-sitter grammars — Zed builds
the grammar from this repository without running the CLI itself.
