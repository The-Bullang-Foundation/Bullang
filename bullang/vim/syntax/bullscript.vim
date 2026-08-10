" BullScript syntax highlighting.
"
" The same token model as bullang.vim, because the two languages deliberately
" share the bullet shape. Where they differ, the colouring shows it:
"   - a BullScript binding carries its type: -> {sum: i64}
"   - bag::name calls a saved script; builtin::name calls a builtin
"   - no let, no struct, no enum, no directives, no escape blocks
"
" The builtin list comes from Bullscript's lang/builtins.rs and is not the same
" list as Bullang's.

if exists("b:current_syntax")
  finish
endif

syn case match

" ── Comments ─────────────────────────────────────────────────────────────────
syn match bullscriptComment "//.*$"

" ── Types ────────────────────────────────────────────────────────────────────
syn keyword bullscriptType i64 f64 bool String
syn keyword bullscriptBool true false

" ── Builtins and bag entries ─────────────────────────────────────────────────
syn match bullscriptBuiltinNs "builtin::"
syn keyword bullscriptBuiltinFn contained add capture close in open out run
      \ to_lower to_upper trim
syn match bullscriptBuiltinBad "builtin::\zs[a-z_][a-z_0-9]*\>"
      \ contains=bullscriptBuiltinFn

syn match bullscriptBagNs "bag::"
syn match bullscriptBagFn "bag::\zs[a-zA-Z_][a-zA-Z0-9_]*\>"

" ── Bindings: -> {name: type} ────────────────────────────────────────────────
syn match bullscriptArrow "->"
syn match bullscriptBinding "->\s*{\s*\zs[a-zA-Z_][a-zA-Z0-9_]*"

" ── Literals ─────────────────────────────────────────────────────────────────
syn region bullscriptString start=+"+ skip=+\\.+ end=+"+ contains=bullscriptEscape
syn match bullscriptEscape contained "\\."
syn match bullscriptNumber "\<-\=\d\+\(\.\d\+\)\=\>"

" ── Operators ────────────────────────────────────────────────────────────────
syn match bullscriptOperator "==\|!=\|<=\|>=\|&&\|||\|[-+*/%<>!]"

hi def link bullscriptComment     Comment
hi def link bullscriptType        Type
hi def link bullscriptBool        Boolean
hi def link bullscriptBuiltinNs   PreProc
hi def link bullscriptBuiltinFn   Function
hi def link bullscriptBuiltinBad  Error
hi def link bullscriptBagNs       PreProc
hi def link bullscriptBagFn       Function
hi def link bullscriptArrow       Operator
hi def link bullscriptBinding     Identifier
hi def link bullscriptString      String
hi def link bullscriptEscape      SpecialChar
hi def link bullscriptNumber      Number
hi def link bullscriptOperator    Operator

let b:current_syntax = "bullscript"
