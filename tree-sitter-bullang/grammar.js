/**
 * Tree-sitter grammar for Bullang.
 *
 * Transcribed from bullang/src/grammar.pest, which stays the authority: this
 * one exists because Zed requires a tree-sitter grammar for any language an
 * extension defines, and it is used for highlighting and structure only. The
 * pest grammar is what actually decides whether a file is valid.
 *
 * Two consequences of that split are worth stating:
 *
 *   - This grammar is deliberately more permissive. Tree-sitter runs on every
 *     keystroke, including on text that is mid-edit and not yet valid; a
 *     grammar that refused such input would leave the buffer unhighlighted
 *     exactly when a reader most wants the help. Real errors come from the
 *     language server, which runs `bullarchy`'s own parser.
 *   - Both files handle `.bu` for source *and* inventory, because Bullang uses
 *     one extension for both. The parser accepts either shape.
 */

module.exports = grammar({
  name: "bullang",

  extras: ($) => [/\s/, $.comment],

  // A bare `builtin::name` and an inline `builtin::name(...)` share a prefix,
  // as do slice and index. Tree-sitter resolves those with lookahead rather
  // than the ordered choice pest uses.
  conflicts: ($) => [],

  rules: {
    source_file: ($) => repeat($._item),

    _item: ($) =>
      choice(
        $.bullet,
        $.struct_def,
        $.enum_def,
        $.native_block,
        $.directive,
        $.inventory_entry,
      ),

    comment: (_) => token(seq("//", /[^\n]*/)),

    // ── Function declaration ────────────────────────────────────────────────
    bullet: ($) =>
      seq(
        "let",
        field("name", $.identifier),
        optional($.type_params),
        $.param_list,
        optional($.output_decl),
        field("body", $.block),
      ),

    type_params: ($) =>
      seq("[", $.identifier, repeat(seq(",", $.identifier)), "]"),

    param_list: ($) =>
      seq("(", optional(seq($.param, repeat(seq(",", $.param)))), ")"),

    param: ($) => seq(field("name", $.identifier), ":", field("type", $.type)),

    output_decl: ($) =>
      seq("->", field("name", $.identifier), ":", field("type", $.type)),

    block: ($) => seq("{", repeat(choice($.pipe, $.builtin_call, $.native_block)), "}"),

    // ── Types ───────────────────────────────────────────────────────────────
    type: ($) => choice($.unit_type, $.tuple_type, $.identifier),

    unit_type: (_) => seq("(", ")"),

    tuple_type: ($) =>
      seq("Tuple", "[", $.type, repeat1(seq(",", $.type)), "]"),

    // ── Bullets ─────────────────────────────────────────────────────────────
    pipe: ($) =>
      seq($.input_list, ":", field("value", $._pipe_value), "->", $.binding, ";"),

    input_list: ($) =>
      seq("(", optional(seq($._input, repeat(seq(",", $._input)))), ")"),

    _input: ($) =>
      choice($.float, $.integer, $.string, $.slice, $.index, $.field_access, $.identifier),

    _pipe_value: ($) => choice($.tuple_expr, $.builtin_call, $._expression),

    tuple_expr: ($) =>
      seq("(", $._expression, repeat1(seq(",", $._expression)), ")"),

    binding: ($) => seq("{", optional($.identifier), "}"),

    // ── Expressions: exactly one operation ──────────────────────────────────
    _expression: ($) => choice($.binary_expr, $._atom),

    binary_expr: ($) =>
      seq(field("left", $._atom), field("operator", $.operator), field("right", $._atom)),

    operator: (_) =>
      choice("&&", "||", "==", "!=", "<=", ">=", "+", "-", "*", "/", "%", "<", ">"),

    _atom: ($) =>
      choice(
        $.builtin_expr,
        $.call,
        $.float,
        $.integer,
        $.string,
        $.slice,
        $.index,
        $.field_access,
        $.unary_expr,
        $.identifier,
      ),

    unary_expr: ($) => prec(2, seq(choice("!", "-"), $._atom)),

    // `builtin::name(...)` — inline, with its own arguments.
    builtin_expr: ($) =>
      prec(
        2,
        seq(
          "builtin",
          "::",
          field("name", $.identifier),
          "(",
          optional(seq($._expression, repeat(seq(",", $._expression)))),
          ")",
        ),
      ),

    // `builtin::name` — bare, taking the bullet's inputs as its arguments.
    builtin_call: ($) => prec(1, seq("builtin", "::", field("name", $.identifier))),

    call: ($) =>
      prec(
        1,
        seq(
          field("name", $.identifier),
          "(",
          optional(seq($._call_arg, repeat(seq(",", $._call_arg)))),
          ")",
        ),
      ),

    _call_arg: ($) =>
      choice($.float, $.integer, $.string, $.slice, $.index, $.field_access, $.identifier),

    field_access: ($) => prec(3, seq($.identifier, repeat1(seq(".", $.identifier)))),

    slice: ($) =>
      prec(4, seq($.identifier, "[", $._expression, "..", $._expression, "]")),

    index: ($) => prec(3, seq($.identifier, "[", $._expression, "]")),

    // ── Escape block: opaque by design ──────────────────────────────────────
    //
    // Matched as a single token so nothing inside is parsed or highlighted as
    // Bullang — which is the whole point of an escape block.
    native_block: (_) =>
      token(seq("@", /[a-zA-Z][a-zA-Z0-9_]*/, /[ \t]*\r?\n/, repeat(/[^@]|@[^e]|@e[^n]|@en[^d]/), "@end")),

    // ── inventory.bu ────────────────────────────────────────────────────────
    directive: ($) =>
      choice(
        seq("#rank", ":", $.rank, ";"),
        seq("#lang", ":", $.lang, ";"),
        seq("#lib", ":", $.lib_name, ";"),
        seq("#use", ":", $.identifier, ";"),
      ),

    rank: (_) => choice("war", "theater", "battle", "strategy", "tactic", "skirmish"),

    lang: (_) => choice("rs", "py", "cpp", "c", "go", "java"),

    lib_name: (_) => token.immediate(/[^;\n]+/),

    inventory_entry: ($) =>
      seq(
        field("file", $.identifier),
        ":",
        $.identifier,
        repeat(seq(",", $.identifier)),
        ";",
      ),

    struct_def: ($) =>
      seq("struct", field("name", $.identifier), "{", optional($.struct_fields), "}"),

    struct_fields: ($) =>
      seq($.struct_field, repeat(seq(",", $.struct_field)), optional(",")),

    struct_field: ($) => seq(field("name", $.identifier), ":", field("type", $.type)),

    enum_def: ($) =>
      seq("enum", field("name", $.identifier), "{", optional($.enum_variants), "}"),

    enum_variants: ($) =>
      seq($.identifier, repeat(seq(",", $.identifier)), optional(",")),

    // ── Terminals ───────────────────────────────────────────────────────────
    identifier: (_) => /[a-zA-Z_][a-zA-Z0-9_]*/,

    float: (_) => token(seq(optional("-"), /\d+/, ".", /\d+/)),

    integer: (_) => token(seq(optional("-"), /\d+/)),

    // A backslash escapes the next character; a raw newline cannot appear.
    string: (_) => token(seq('"', repeat(choice(/\\./, /[^"\\\n]/)), '"')),
  },
});
