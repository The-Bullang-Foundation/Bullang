use std::collections::HashMap;

// ── Source location ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub line: usize,
    pub col:  usize,
}

impl Span {
    pub fn new(line: usize, col: usize) -> Self { Self { line, col } }
}

// ── Backend ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum Backend {
    Rust,
    Python,
    C,
    Cpp,
    Go,
    /// An unrecognised backend name written in an escape block.
    Unknown(String),
}

impl Backend {
    pub fn from_ext(ext: &str) -> Option<Self> {
        match ext {
            "rs"  => Some(Backend::Rust),
            "py"  => Some(Backend::Python),
            "c"   => Some(Backend::C),
            "cpp" | "cc" | "cxx" => Some(Backend::Cpp),
            "go"  => Some(Backend::Go),
            _     => None,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Backend::Rust       => "rust",
            Backend::Python     => "python",
            Backend::C          => "c",
            Backend::Cpp        => "cpp",
            Backend::Go         => "go",
            Backend::Unknown(_) => "unknown",
        }
    }
    pub fn ext(&self) -> &'static str {
        match self {
            Backend::Rust       => "rs",
            Backend::Python     => "py",
            Backend::C          => "c",
            Backend::Cpp        => "cpp",
            Backend::Go         => "go",
            Backend::Unknown(_) => "?",
        }
    }
    pub fn escape_keyword(&self) -> String {
        match self {
            Backend::Rust        => "rust".to_string(),
            Backend::Python      => "python".to_string(),
            Backend::C           => "c".to_string(),
            Backend::Cpp         => "cpp".to_string(),
            Backend::Go          => "go".to_string(),
            Backend::Unknown(s)  => s.clone(),
        }
    }
}

// ── Rank ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Skirmish,
    Tactic,
    Strategy,
    Battle,
    Theater,
    War,
}

impl Rank {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "war"      => Some(Rank::War),
            "theater"  => Some(Rank::Theater),
            "battle"   => Some(Rank::Battle),
            "strategy" => Some(Rank::Strategy),
            "tactic"   => Some(Rank::Tactic),
            "skirmish" => Some(Rank::Skirmish),
            _          => None,
        }
    }

    pub fn child_rank(&self) -> Option<Rank> {
        match self {
            Rank::War      => Some(Rank::Theater),
            Rank::Theater  => Some(Rank::Battle),
            Rank::Battle   => Some(Rank::Strategy),
            Rank::Strategy => Some(Rank::Tactic),
            Rank::Tactic   => Some(Rank::Skirmish),
            Rank::Skirmish => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Rank::War      => "war",
            Rank::Theater  => "theater",
            Rank::Battle   => "battle",
            Rank::Strategy => "strategy",
            Rank::Tactic   => "tactic",
            Rank::Skirmish => "skirmish",
        }
    }

    pub fn has_own_files(&self) -> bool  { *self != Rank::War }
    pub fn has_sub_folders(&self) -> bool { *self != Rank::Skirmish }
}

// ── Type system ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum BuType {
    Named(String),
    Tuple(Vec<BuType>),
    Array(Box<BuType>, usize),
    Unknown,
}

impl BuType {
    pub fn to_rust(&self) -> String {
        match self {
            BuType::Named(s)     => s.clone(),
            BuType::Tuple(inner) => format!(
                "Tuple[{}]",
                inner.iter().map(|t| t.to_rust()).collect::<Vec<_>>().join(", ")
            ),
            BuType::Array(ty, n) => format!("[{}; {}]", ty.to_rust(), n),
            BuType::Unknown      => "_".to_string(),
        }
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            BuType::Named(s) => matches!(
                s.as_str(),
                "i8"|"i16"|"i32"|"i64"|"i128"|"isize"|
                "u8"|"u16"|"u32"|"u64"|"u128"|"usize"|
                "f32"|"f64"
            ),
            _ => false,
        }
    }
}

// ── Type environment ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BulletSig {
    pub params:  Vec<BuType>,
    pub returns: BuType,
}

pub type TypeEnv   = HashMap<String, BulletSig>;
pub type StructEnv = HashMap<String, StructDef>;
pub type EnumEnv   = HashMap<String, EnumDef>;

// ── Expressions ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum CallArg {
    Value(String),
    BulletRef(String),
}

#[derive(Debug, Clone)]
pub enum Atom {
    Ident(String),
    Integer(i64),
    Float(f64),
    /// A plain string literal with no interpolation: `"hello"`
    StringLit(String),
    /// A string template with `{var}` placeholders: `"hello {name}!"`
    /// Stored as the raw template content (quotes stripped).
    /// Each codegen resolves the placeholders into its own format mechanism.
    Interp(String),
    Call { name: String, args: Vec<CallArg> },
    /// Unary expression: `!b` or `-x`
    Unary { op: String, rhs: Box<Atom> },
    /// Struct field access: `point.x` or `player.position.y`
    FieldAccess { base: String, fields: Vec<String> },
    /// String character index: `s[i]` → char
    Index { base: String, idx: Box<Expr> },
    /// String slice: `s[i..j]` → String
    Slice { base: String, from: Box<Expr>, to: Box<Expr> },
    /// Inline builtin call usable as a pipe expression: `builtin::assert(cond)`
    /// Distinct from BulletBody::Builtin (whole-function-body form).
    BuiltinExpr { name: String, args: Vec<Expr> },
    /// Enum variant access: `Direction.North`
    /// Produced by the lowering pass from FieldAccess when the base is a known enum.
    EnumVariant { ty: String, variant: String },
    /// Inline anonymous function: `|a: i32, b: i32| -> i32 { a + b }`
    Closure { params: Vec<ClosureParam>, ret: BuType, body: Box<Expr> },
}

#[derive(Debug, Clone)]
pub struct ClosureParam {
    pub name: String,
    pub ty:   BuType,
}

#[derive(Debug, Clone)]
pub struct BinExpr {
    pub lhs: Atom,
    pub op:  String,
    pub rhs: Atom,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Atom(Atom),
    BinOp(BinExpr),
    Tuple(Vec<Expr>),
}

// ── Pipe ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Pipe {
    pub inputs:    Vec<String>,
    pub expr:      Expr,
    pub binding:   String,
    pub propagate: bool,   // true when the bullet ends with `?`
    pub span:      Span,
}

// ── Bullet body ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NativeBlock {
    pub backend: Backend,
    pub code:    String,
}

#[derive(Debug, Clone)]
pub enum BulletBody {
    /// Pure Bullang pipe chain.
    Pipes(Vec<Pipe>),
    /// One or more native escape blocks — each targets a specific backend.
    /// At codegen time the matching block is selected; others are ignored.
    Natives(Vec<NativeBlock>),
    /// Reference to a stdlib builtin.
    Builtin(String),
}

// ── Output declaration ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct OutputDecl {
    pub name: String,
    pub ty:   BuType,
}

// ── Parameter ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Param {
    pub name: String,
    pub ty:   BuType,
}

// ── Struct definitions ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct StructField {
    pub name: String,
    pub ty:   BuType,
}

/// A struct type definition. Structs are always public.
#[derive(Debug, Clone)]
pub struct StructDef {
    pub name:   String,
    pub fields: Vec<StructField>,
}

// ── Enum definitions ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct EnumVariant {
    pub name: String,
}

/// A C-style enum. Variants are integer tags — no data payloads.
#[derive(Debug, Clone)]
pub struct EnumDef {
    pub name:     String,
    pub variants: Vec<EnumVariant>,
}

// ── Bullet ────────────────────────────────────────────────────────────────────

/// A single function. All bullets are always public — there is no private code.
#[derive(Debug, Clone)]
pub struct Bullet {
    pub name:        String,
    pub type_params: Vec<String>,   // e.g. ["T"] for let max[T](...)
    pub params:      Vec<Param>,
    pub output:      OutputDecl,
    pub body:        BulletBody,
    pub span:        Span,
}

// ── Inventory entry ───────────────────────────────────────────────────────────

/// One line in inventory.bu: `filename : fn1, fn2, fn3;`
#[derive(Debug, Clone)]
pub struct InventoryEntry {
    pub file:      String,        // filename without .bu extension
    pub functions: Vec<String>,   // all functions declared in that file
}

// ── File types ────────────────────────────────────────────────────────────────

/// A source .bu file — only bullet declarations. Structs live in inventory.bu.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub bullets: Vec<Bullet>,
}

/// An inventory.bu file — rank, directives, struct definitions, and file manifest.
#[derive(Debug, Clone)]
pub struct InventoryFile {
    pub rank:    Rank,
    pub lang:    Option<Backend>,      // #lang: ext;
    pub libs:    Vec<String>,          // #lib: header; (C/C++ only)
    pub structs: Vec<StructDef>,       // struct definitions for this folder
    pub enums:   Vec<EnumDef>,         // enum definitions for this folder
    pub entries: Vec<InventoryEntry>,  // one per source file in this folder
}

#[derive(Debug, Clone)]
pub enum BuFile {
    Source(SourceFile),
    Inventory(InventoryFile),
}

// ── Lowering pass: FieldAccess → EnumVariant ──────────────────────────────────

/// Convert `Type.Variant` (parsed as FieldAccess) to `Atom::EnumVariant`
/// for all single-field accesses where the base name is a known enum.
/// Run this before typechecking and before codegen.
pub fn lower_enum_refs(sf: &mut SourceFile, enum_env: &EnumEnv) {
    for bullet in &mut sf.bullets {
        if let BulletBody::Pipes(pipes) = &mut bullet.body {
            for pipe in pipes {
                lower_expr(&mut pipe.expr, enum_env);
            }
        }
    }
}

fn lower_expr(expr: &mut Expr, env: &EnumEnv) {
    match expr {
        Expr::Atom(a)      => lower_atom(a, env),
        Expr::BinOp(b)     => { lower_atom(&mut b.lhs, env); lower_atom(&mut b.rhs, env); }
        Expr::Tuple(exprs) => { for e in exprs { lower_expr(e, env); } }
    }
}

fn lower_atom(atom: &mut Atom, env: &EnumEnv) {
    match atom {
        Atom::FieldAccess { base, fields } if fields.len() == 1 && env.contains_key(base) => {
            let ty      = base.clone();
            let variant = fields[0].clone();
            *atom = Atom::EnumVariant { ty, variant };
        }
        Atom::Unary { rhs, .. }         => lower_atom(rhs, env),
        Atom::Index { idx, .. }         => lower_expr(idx, env),
        Atom::Slice { from, to, .. }    => { lower_expr(from, env); lower_expr(to, env); }
        Atom::BuiltinExpr { args, .. }  => { for a in args { lower_expr(a, env); } }
        Atom::Closure { body, .. }      => lower_expr(body, env),
        // Call args are strings — no AST nodes to lower.
        // FieldAccess with 2+ fields, Ident, Literal, EnumVariant: no-op.
        _ => {}
    }
}
