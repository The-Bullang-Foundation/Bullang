use pest::iterators::Pair;
use crate::ast::*;

#[derive(pest_derive::Parser)]
#[grammar = "grammar.pest"]
pub struct BulParser;

// ── Parse error type ──────────────────────────────────────────────────────────

/// A parse error with file path, line, col and message.
/// Separate from ValidationError so the display layer can format them
/// consistently alongside structural/type errors.
#[derive(Debug)]
pub struct ParseError {
    pub file:    String,
    pub line:    usize,
    pub col:     usize,
    pub message: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line > 0 {
            write!(f, "[{}:{}:{}] {}", self.file, self.line, self.col, self.message)
        } else {
            write!(f, "[{}] {}", self.file, self.message)
        }
    }
}

/// Result of a tolerant parse: successfully parsed bullets + any parse errors.
pub struct ParseResult {
    pub file:   BuFile,
    pub errors: Vec<ParseError>,
}

// ── Public entry points ───────────────────────────────────────────────────────

/// Strict parse — used for inventory files where there is no meaningful
/// way to recover from a broken #rank or entry line.
pub fn parse_file(
    source:       &str,
    is_inventory: bool,
) -> Result<BuFile, Box<dyn std::error::Error>> {
    use pest::Parser;
    if is_inventory {
        let mut pairs = BulParser::parse(Rule::inventory_file, source)?;
        Ok(BuFile::Inventory(parse_inventory(pairs.next().unwrap())))
    } else {
        let mut pairs = BulParser::parse(Rule::source_file, source)?;
        Ok(BuFile::Source(parse_source(pairs.next().unwrap())))
    }
}

/// Tolerant parse for source files.
///
/// Splits the source into individual `let …` function chunks by scanning
/// for top-level `let` keywords. Each chunk is parsed independently so
/// one broken function does not prevent the others from being validated.
///
/// Returns a ParseResult containing:
///   - every bullet that parsed successfully (possibly empty)
///   - every parse error encountered (possibly empty)
pub fn parse_file_tolerant(source: &str, file_path: &str) -> ParseResult {
    use pest::Parser;

    // Fast path: try a strict parse first. If it succeeds there is nothing
    // to recover from and we avoid the extra work of splitting.
    if let Ok(mut pairs) = BulParser::parse(Rule::source_file, source) {
        return ParseResult {
            file:   BuFile::Source(parse_source(pairs.next().unwrap())),
            errors: vec![],
        };
    }

    // Recovery path: split at top-level `let` boundaries and parse each
    // function independently.
    let chunks = split_into_function_chunks(source);
    let mut bullets = Vec::new();
    let mut errors  = Vec::new();

    for (chunk_src, line_offset) in chunks {
        match BulParser::parse(Rule::source_file, &chunk_src) {
            Ok(mut pairs) => {
                let sf = parse_source(pairs.next().unwrap());
                bullets.extend(sf.bullets);
            }
            Err(e) => {
                let (line, col) = pest_error_location(&e);
                let adjusted_line = line + line_offset.saturating_sub(1);
                errors.push(ParseError {
                    file:    file_path.to_string(),
                    line:    adjusted_line,
                    col,
                    message: pest_error_message(&e),
                });
            }
        }
    }

    ParseResult {
        file:   BuFile::Source(SourceFile { bullets }),
        errors,
    }
}

// ── Function chunk splitter ───────────────────────────────────────────────────

/// Split a source file into individual `let` function chunks.
///
/// Each chunk is a complete `let name(...) -> ... { ... }` block.
/// We scan line by line looking for lines that start with `let ` (after
/// trimming whitespace) — these mark the start of a new function.
/// The chunk text for each function runs from its `let` line up to
/// (but not including) the next `let` line.
///
/// Returns Vec<(chunk_source, start_line_number_1indexed)>.
fn split_into_function_chunks(source: &str) -> Vec<(String, usize)> {
    let lines: Vec<&str> = source.lines().collect();
    let mut chunk_starts: Vec<usize> = Vec::new(); // 0-indexed line numbers

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("let ") || trimmed == "let" {
            chunk_starts.push(i);
        }
    }

    if chunk_starts.is_empty() {
        // No `let` found at all — return the whole source as one chunk
        // so the error message points to the actual problem.
        return vec![(source.to_string(), 1)];
    }

    let mut chunks = Vec::new();
    for (idx, &start) in chunk_starts.iter().enumerate() {
        let end = if idx + 1 < chunk_starts.len() {
            chunk_starts[idx + 1]
        } else {
            lines.len()
        };

        let chunk_lines = &lines[start..end];
        let chunk_src   = chunk_lines.join("\n");
        // line_offset is 1-indexed for error reporting
        chunks.push((chunk_src, start + 1));
    }

    chunks
}

// ── Pest error helpers ────────────────────────────────────────────────────────

fn pest_error_location(e: &pest::error::Error<Rule>) -> (usize, usize) {
    match e.line_col {
        pest::error::LineColLocation::Pos((line, col)) => (line, col),
        pest::error::LineColLocation::Span((line, col), _) => (line, col),
    }
}

fn pest_error_message(e: &pest::error::Error<Rule>) -> String {
    // Pest error Display includes the full source snippet — we just want
    // the description line.
    let full = format!("{}", e);
    full.lines()
        .find(|l| l.trim_start().starts_with('='))
        .map(|l| l.trim_start_matches(|c| c == '=' || c == ' ').to_string())
        .unwrap_or_else(|| "Syntax error".to_string())
}

// ── Block indent normalisation ───────────────────────────────────────────────

/// Strip common leading whitespace from a native block body.
///
/// When a developer writes:
///   @rust
///       values.iter().sum()
///   @end
///
/// The captured string is "\n    values.iter().sum()\n    " (4 spaces each line).
/// We strip those 4 spaces so the emitted code is just "values.iter().sum()".
/// The code generator then adds back exactly 4 spaces for function-body indentation.
fn normalise_block_indent(raw: &str) -> String {
    let lines: Vec<&str> = raw.split('\n').collect();

    // Find minimum indentation across all non-empty lines
    let min_indent = lines.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start_matches(' ').len()) // only strip spaces, not tabs
        .min()
        .unwrap_or(0);

    // Strip exactly min_indent leading spaces from each line
    lines.iter()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else if l.len() >= min_indent {
                l[min_indent..].to_string()
            } else {
                l.trim_start().to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        // Trim a single leading/trailing newline that comes from the block delimiters
        .trim_matches('\n')
        .to_string()
}

// ── Span extraction ───────────────────────────────────────────────────────────

fn span_of(pair: &Pair<Rule>) -> Span {
    let (line, col) = pair.as_span().start_pos().line_col();
    Span::new(line, col)
}

// ── Source file ───────────────────────────────────────────────────────────────

fn parse_source(pair: Pair<Rule>) -> SourceFile {
    let bullets = pair.into_inner()
        .filter(|p| p.as_rule() == Rule::bullet)
        .map(parse_bullet)
        .collect();
    SourceFile { bullets }
}

// ── Struct definition ─────────────────────────────────────────────────────────

fn parse_struct_def(pair: Pair<Rule>) -> crate::ast::StructDef {
    let mut inner = pair.into_inner();
    let name      = inner.next().unwrap().as_str().to_string();
    // struct_fields contains struct_field* children
    let fields_pair = inner.next().unwrap();
    let fields = fields_pair.into_inner()
        .filter(|p| p.as_rule() == Rule::struct_field)
        .map(|p| {
            let mut fi = p.into_inner();
            crate::ast::StructField {
                name: fi.next().unwrap().as_str().to_string(),
                ty:   parse_ty(fi.next().unwrap()),
            }
        })
        .collect();
    crate::ast::StructDef { name, fields }
}

fn parse_enum_def(pair: Pair<Rule>) -> crate::ast::EnumDef {
    let mut inner = pair.into_inner();
    let name      = inner.next().unwrap().as_str().to_string();
    // enum_variants contains enum_variant* children
    let variants_pair = inner.next().unwrap();
    let variants = variants_pair.into_inner()
        .filter(|p| p.as_rule() == Rule::enum_variant)
        .map(|p| crate::ast::EnumVariant { name: p.as_str().to_string() })
        .collect();
    crate::ast::EnumDef { name, variants }
}

// ── Inventory file ────────────────────────────────────────────────────────────

fn parse_inventory(pair: Pair<Rule>) -> InventoryFile {
    let mut rank    = None;
    let mut lang    = None;
    let mut libs    = Vec::new();
    let mut structs = Vec::new();
    let mut enums   = Vec::new();
    let mut entries = Vec::new();

    for inner in pair.into_inner() {
        match inner.as_rule() {
            Rule::dir_rank => {
                rank = Rank::from_str(inner.into_inner().next().unwrap().as_str());
            }
            Rule::dir_lang => {
                let ext = inner.into_inner().next().unwrap().as_str();
                lang = Backend::from_ext(ext);
            }
            Rule::dir_lib => {
                let name = inner.into_inner().next().unwrap().as_str().trim().to_string();
                libs.push(name);
            }
            Rule::struct_def => {
                structs.push(parse_struct_def(inner));
            }
            Rule::enum_def => {
                enums.push(parse_enum_def(inner));
            }
            Rule::inv_entry => {
                let mut ci    = inner.into_inner();
                let file      = ci.next().unwrap().as_str().to_string();
                let functions = ci.map(|p| p.as_str().to_string()).collect();
                entries.push(InventoryEntry { file, functions });
            }
            Rule::EOI => {}
            _         => {}
        }
    }

    InventoryFile {
        rank: rank.expect("inventory.bu is missing #rank"),
        lang,
        libs,
        structs,
        enums,
        entries,
    }
}

// ── Bullet ────────────────────────────────────────────────────────────────────

fn parse_bullet(pair: Pair<Rule>) -> Bullet {
    let bullet_span = span_of(&pair);
    let mut inner   = pair.into_inner();
    let name        = inner.next().unwrap().as_str().to_string();

    // Optional type_params: "[T]" or "[K, V]"
    let type_params = match inner.peek().map(|p| p.as_rule()) {
        Some(Rule::type_params) => {
            inner.next().unwrap().into_inner()
                .map(|p| p.as_str().to_string())
                .collect()
        }
        _ => vec![],
    };

    let params = parse_param_list(inner.next().unwrap());
    let output = parse_output_decl(inner.next().unwrap());
    let body   = parse_bullet_body(inner.next().unwrap());
    Bullet { name, type_params, params, output, body, span: bullet_span }
}

fn parse_param_list(pair: Pair<Rule>) -> Vec<Param> {
    pair.into_inner()
        .filter(|p| p.as_rule() == Rule::param)
        .map(|p| {
            let mut pi = p.into_inner();
            Param {
                name: pi.next().unwrap().as_str().to_string(),
                ty:   parse_ty(pi.next().unwrap()),
            }
        })
        .collect()
}

fn parse_output_decl(pair: Pair<Rule>) -> OutputDecl {
    let mut inner = pair.into_inner();
    OutputDecl {
        name: inner.next().unwrap().as_str().to_string(),
        ty:   parse_ty(inner.next().unwrap()),
    }
}

fn parse_bullet_body(pair: Pair<Rule>) -> BulletBody {
    let children: Vec<Pair<Rule>> = pair.into_inner().collect();
    if children.is_empty() { panic!("bullet body is empty"); }

    match children[0].as_rule() {
        Rule::native_block => {
            // Collect ALL native blocks — a function may have one per backend
            let blocks: Vec<NativeBlock> = children.iter()
                .filter(|c| c.as_rule() == Rule::native_block)
                .map(|c| parse_native_block(c.as_str()))
                .collect();
            BulletBody::Natives(blocks)
        }
        Rule::builtin_call => {
            let name = children[0].clone().into_inner()
                .next().unwrap().as_str().to_string();
            BulletBody::Builtin(name)
        }
        Rule::pipe => {
            BulletBody::Pipes(children.into_iter().map(parse_pipe).collect())
        }
        other => unreachable!("unexpected bullet_body child: {:?}", other),
    }
}

fn parse_native_block(raw: &str) -> NativeBlock {
    // raw is "@rust\n    code\n    @end"
    let raw = &raw[1..]; // strip leading @
    let name_end = raw.find(|c: char| c.is_whitespace()).unwrap_or(raw.len());
    let backend_str = &raw[..name_end];
    let after_name  = &raw[name_end..];
    let code_end    = after_name.rfind("@end").unwrap_or(after_name.len());
    let code_raw    = &after_name[..code_end];
    let code        = normalise_block_indent(code_raw);
    let backend = match backend_str {
        "rust"   => Backend::Rust,
        "python" => Backend::Python,
        "c"      => Backend::C,
        "cpp"    => Backend::Cpp,
        "go"     => Backend::Go,
        other    => Backend::Unknown(other.to_string()),
    };
    NativeBlock { backend, code }
}

// ── Pipe ──────────────────────────────────────────────────────────────────────

fn parse_pipe(pair: Pair<Rule>) -> Pipe {
    let pipe_span = span_of(&pair);
    let mut inner = pair.into_inner();
    let inputs: Vec<String> = inner.next().unwrap().into_inner()
        .map(|p| p.as_str().to_string()).collect();
    let expr      = parse_pipe_val(inner.next().unwrap());
    let binding   = inner.next().unwrap().into_inner()
        .next().unwrap().as_str().to_string();
    // Optional propagate_op `?`
    let propagate = inner.next()
        .map(|p| p.as_rule() == Rule::propagate_op)
        .unwrap_or(false);
    Pipe { inputs, expr, binding, propagate, span: pipe_span }
}

fn parse_pipe_val(pair: Pair<Rule>) -> Expr {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::tuple_expr => Expr::Tuple(inner.into_inner().map(parse_expr).collect()),
        Rule::expr       => parse_expr(inner),
        other => unreachable!("unexpected pipe_val: {:?}", other),
    }
}

fn parse_expr(pair: Pair<Rule>) -> Expr {
    let mut inner = pair.into_inner();
    let lhs       = parse_atom(inner.next().unwrap());
    match inner.next() {
        Some(op_pair) => {
            let op  = op_pair.as_str().trim().to_string();
            let rhs = parse_atom(inner.next().unwrap());
            Expr::BinOp(BinExpr { lhs, op, rhs })
        }
        None => Expr::Atom(lhs),
    }
}

fn parse_atom(pair: Pair<Rule>) -> Atom {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::builtin_expr => {
            let mut parts = inner.into_inner();
            let name = parts.next().unwrap().as_str().to_string();
            let args = parts.map(parse_expr).collect();
            Atom::BuiltinExpr { name, args }
        }
        Rule::closure => {
            let mut parts = inner.into_inner();
            let mut params = Vec::new();
            // Consume closure_param pairs until we hit the return type (ty rule)
            // then the body expr. We peek at the rule to distinguish.
            let mut next = parts.next();
            while let Some(ref p) = next {
                if p.as_rule() == Rule::closure_param {
                    let mut ci = p.clone().into_inner();
                    let name = ci.next().unwrap().as_str().to_string();
                    let ty   = parse_ty(ci.next().unwrap());
                    params.push(crate::ast::ClosureParam { name, ty });
                    next = parts.next();
                } else {
                    break;
                }
            }
            // next is now the return ty
            let ret  = parse_ty(next.unwrap());
            let body = parse_expr(parts.next().unwrap());
            Atom::Closure { params, ret, body: Box::new(body) }
        }
        Rule::call => {
            let mut ci = inner.into_inner();
            let name   = ci.next().unwrap().as_str().to_string();
            let args   = ci.map(parse_call_arg).collect();
            Atom::Call { name, args }
        }
        Rule::float        => Atom::Float(inner.as_str().parse().unwrap()),
        Rule::integer      => Atom::Integer(inner.as_str().parse().unwrap()),
        Rule::ident        => Atom::Ident(inner.as_str().to_string()),
        Rule::string_lit   => parse_string_atom(inner.as_str()),
        Rule::field_access => {
            let mut parts = inner.into_inner();
            let base   = parts.next().unwrap().as_str().to_string();
            let fields = parts.map(|p| p.as_str().to_string()).collect();
            Atom::FieldAccess { base, fields }
        }
        Rule::index_expr => {
            let mut parts = inner.into_inner();
            let base = parts.next().unwrap().as_str().to_string();
            let idx  = parse_expr(parts.next().unwrap());
            Atom::Index { base, idx: Box::new(idx) }
        }
        Rule::slice_expr => {
            let mut parts = inner.into_inner();
            let base = parts.next().unwrap().as_str().to_string();
            let from = parse_expr(parts.next().unwrap());
            let to   = parse_expr(parts.next().unwrap());
            Atom::Slice { base, from: Box::new(from), to: Box::new(to) }
        }
        Rule::unary_expr => {
            let mut ui   = inner.into_inner();
            let op       = ui.next().unwrap().as_str().to_string();
            let rhs_pair = ui.next().unwrap();
            let rhs      = parse_atom(rhs_pair);
            Atom::Unary { op, rhs: Box::new(rhs) }
        }
        other => unreachable!("unexpected atom: {:?}", other),
    }
}

/// Strip the outer quotes from a string literal and decide whether it contains
/// `{ident}` interpolation placeholders.
fn parse_string_atom(raw: &str) -> Atom {
    // raw includes the surrounding quotes: "hello {name}"
    let content = &raw[1..raw.len()-1];
    if has_interp_vars(content) {
        Atom::Interp(content.to_string())
    } else {
        Atom::StringLit(content.to_string())
    }
}

/// True if the string contains at least one `{ident}` placeholder.
fn has_interp_vars(s: &str) -> bool {
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            let ident_chars: String = chars.by_ref()
                .take_while(|&x| x != '}')
                .collect();
            if !ident_chars.is_empty()
                && ident_chars.chars().next().map(|c| c.is_alphabetic()).unwrap_or(false)
                && ident_chars.chars().all(|c| c.is_alphanumeric() || c == '_')
            {
                return true;
            }
        }
    }
    false
}

fn parse_call_arg(pair: Pair<Rule>) -> CallArg {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::bullet_ref => CallArg::BulletRef(
            inner.into_inner().next().unwrap().as_str().to_string()
        ),
        Rule::float        => CallArg::Value(inner.as_str().to_string()),
        Rule::integer      => CallArg::Value(inner.as_str().to_string()),
        Rule::ident        => CallArg::Value(inner.as_str().to_string()),
        Rule::string_lit   => CallArg::Value(inner.as_str().to_string()),
        Rule::field_access => CallArg::Value(inner.as_str().to_string()),
        Rule::index_expr   => CallArg::Value(inner.as_str().to_string()),
        Rule::slice_expr   => CallArg::Value(inner.as_str().to_string()),
        // Closures as call args are parsed fully; store raw text for now
        // (the AST node is used; CallArg::Value carries the source text).
        Rule::closure      => CallArg::Value(inner.as_str().to_string()),
        other => unreachable!("unexpected call_arg: {:?}", other),
    }
}

// ── Type ──────────────────────────────────────────────────────────────────────

fn parse_ty(pair: Pair<Rule>) -> BuType {
    let inner = pair.into_inner().next().unwrap();
    match inner.as_rule() {
        Rule::ty_unit  => BuType::Named("()".to_string()),
        Rule::ty_tuple => {
            // Tuple[T, U, ...] — walk ty_tuple_args inner types
            let types: Vec<BuType> = inner.into_inner()
                .flat_map(|p| p.into_inner())   // ty_tuple_args → its ty children
                .filter(|p| p.as_rule() == Rule::ty)
                .map(parse_ty)
                .collect();
            BuType::Tuple(types)
        }
        Rule::ty_array => {
            let mut ai      = inner.into_inner();
            let elem        = parse_ty(ai.next().unwrap());
            let size: usize = ai.next().unwrap().as_str().parse().unwrap();
            BuType::Array(Box::new(elem), size)
        }
        // Fn[...], &T, &mut T, and plain atoms stored verbatim — codegen handles
        Rule::ty_fn      => BuType::Named(inner.as_str().trim().to_string()),
        Rule::ty_ref     => BuType::Named(inner.as_str().trim().to_string()),
        Rule::ty_ref_mut => BuType::Named(inner.as_str().trim().to_string()),
        Rule::ty_atom    => BuType::Named(inner.as_str().trim().to_string()),
        other => unreachable!("unexpected ty rule: {:?}", other),
    }
}
