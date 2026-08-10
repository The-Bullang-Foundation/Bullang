; Bullang syntax highlighting.
;
; Capture names are Zed's, listed in its Language Extensions docs. Helix and
; Neovim use the same vocabulary for the captures used here.

; ── Keywords ────────────────────────────────────────────────────────────────
[
  "let"
  "struct"
  "enum"
] @keyword

; `builtin` is a keyword in `builtin::name`, not an ordinary identifier.
"builtin" @keyword

; ── Directives ──────────────────────────────────────────────────────────────
[
  "#rank"
  "#lang"
  "#lib"
  "#use"
] @keyword

(rank) @constant.builtin
(lang) @constant.builtin
(lib_name) @string.special

; ── Declarations ────────────────────────────────────────────────────────────
(bullet name: (identifier) @function)
(call name: (identifier) @function)
; Known builtins are highlighted as builtins; anything else is flagged.
; The list mirrors the one in the VS Code grammar, which mirrors
; `bullarchy stdlib`. All three have to change together.
((builtin_call name: (identifier) @function.builtin)
 (#any-of? @function.builtin
  "argc" "args" "close" "ends_with" "env" "exit" "i64_to_str" "in" "len"
  "max" "min" "open" "out" "replace_str" "run" "sleep" "starts_with"
  "str_to_i64" "swap" "tern" "time" "to_lower" "to_upper" "trim"))

((builtin_expr name: (identifier) @function.builtin)
 (#any-of? @function.builtin
  "argc" "args" "close" "ends_with" "env" "exit" "i64_to_str" "in" "len"
  "max" "min" "open" "out" "replace_str" "run" "sleep" "starts_with"
  "str_to_i64" "swap" "tern" "time" "to_lower" "to_upper" "trim"))

; A builtin outside that set — an unknown name is a useful thing to see.
((builtin_call name: (identifier) @comment.error)
 (#not-any-of? @comment.error
  "argc" "args" "close" "ends_with" "env" "exit" "i64_to_str" "in" "len"
  "max" "min" "open" "out" "replace_str" "run" "sleep" "starts_with"
  "str_to_i64" "swap" "tern" "time" "to_lower" "to_upper" "trim"))

(struct_def name: (identifier) @type)
(enum_def name: (identifier) @type)
(enum_variants (identifier) @variant)

(param name: (identifier) @variable.parameter)
(output_decl name: (identifier) @variable)
(binding (identifier) @variable)

(inventory_entry file: (identifier) @property)

; ── Types ───────────────────────────────────────────────────────────────────
(type (identifier) @type)
(tuple_type) @type
(unit_type) @type
"Tuple" @type.builtin
(type_params (identifier) @type)

; ── Fields ──────────────────────────────────────────────────────────────────
(field_access (identifier) @variable)
(struct_field name: (identifier) @property)

; ── Literals ────────────────────────────────────────────────────────────────
(string) @string
(integer) @number
(float) @number

; ── Operators and punctuation ───────────────────────────────────────────────
(operator) @operator
[
  "->"
  "::"
  ".."
  "!"
] @operator

[
  ":"
  ";"
  ","
  "."
] @punctuation.delimiter

[
  "("
  ")"
  "{"
  "}"
  "["
  "]"
] @punctuation.bracket

; ── Escape blocks ───────────────────────────────────────────────────────────
;
; One capture for the whole block: its contents are another language, and
; Bullang deliberately does not read them. Highlighting them as Bullang would
; be actively misleading.
(native_block) @embedded

(comment) @comment
