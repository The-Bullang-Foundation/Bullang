; The outline shows what a file declares: its functions, and — in an
; inventory — its types and file entries.
(bullet
  "let" @context
  name: (identifier) @name) @item

(struct_def
  "struct" @context
  name: (identifier) @name) @item

(enum_def
  "enum" @context
  name: (identifier) @name) @item

(inventory_entry
  file: (identifier) @name) @item
