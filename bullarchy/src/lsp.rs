//! Bullang Language Server — hand-rolled JSON-RPC over stdin/stdout.
//!
//! No external LSP framework is used. The LSP wire protocol is straightforward:
//!   Content-Length: <N>\r\n\r\n<N bytes of JSON>
//!
//! Capabilities:
//!   • Diagnostics   — parse / validation / type errors pushed on every save
//!   • Hover         — function signature shown on hover
//!   • Go-to-def     — jump to the file+line where a function is declared

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use bullang::ast::{BuFile, BuType, Bullet};
use crate::{validator, typecheck};
use bullang::parser;

// ── Wire protocol ─────────────────────────────────────────────────────────────

fn read_message(reader: &mut impl BufRead) -> Option<Value> {
    // Read headers until blank line
    let mut content_length: Option<usize> = None;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok()?;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() { break; }
        // Header names are case-insensitive in the LSP base protocol, and
        // the separator may or may not have a space after the colon. Matching
        // the exact string "Content-Length: " meant a client sending
        // "content-length:" was read as having no body at all, and the server
        // silently stopped responding.
        if let Some((name, rest)) = line.split_once(':') {
            if name.trim().eq_ignore_ascii_case("content-length") {
                content_length = rest.trim().parse().ok();
            }
        }
    }
    let len = content_length?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf).ok()?;
    serde_json::from_slice(&buf).ok()
}

fn send_message(writer: &mut impl Write, value: &Value) {
    let body = value.to_string();
    let _ = write!(writer, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = writer.flush();
}

fn response(id: &Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error_response(id: &Value, code: i32, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

fn notification(method: &str, params: Value) -> Value {
    json!({ "jsonrpc": "2.0", "method": method, "params": params })
}

// ── Server state ──────────────────────────────────────────────────────────────

/// In-memory snapshot of an open document.
struct Document { content: String }

/// Index of every function visible in a project root.
/// Key = function name, Value = (Bullet, absolute file path)
type FnIndex = HashMap<String, (Bullet, String)>;

struct Server {
    /// URI string → document content
    docs:     HashMap<String, Document>,
    /// Project root path string → function index
    fn_index: HashMap<String, FnIndex>,
    /// Set by `shutdown`. `exit` after it is a clean stop; `exit` without it
    /// means the client went away unexpectedly, and the spec asks for a
    /// non-zero status so a supervisor can tell the two apart. The old code
    /// exited 0 either way.
    shutdown_requested: bool,
}

impl Server {
    /// Hand the open buffers to the validator for the duration of one run.
    fn publish_overlay(&self) {
        let docs = self.docs.iter()
            .filter_map(|(uri, doc)| uri_to_path(uri).map(|p| (p, doc.content.clone())))
            .collect();
        crate::overlay::set(docs);
    }

    fn new() -> Self {
        Self { docs: HashMap::new(), fn_index: HashMap::new(), shutdown_requested: false }
    }
}

// ── Path helpers ──────────────────────────────────────────────────────────────

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    // file:///absolute/path → /absolute/path
    uri.strip_prefix("file://").map(|p| {
        // URL-decode the minimal set we need (%20 etc.)
        let decoded = percent_decode(p);
        PathBuf::from(decoded)
    })
}

fn path_to_uri(path: &Path) -> String {
    format!("file://{}", path.display())
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' {
            let h1 = chars.next().unwrap_or('0');
            let h2 = chars.next().unwrap_or('0');
            if let Ok(b) = u8::from_str_radix(&format!("{}{}", h1, h2), 16) {
                out.push(b as char);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Walk up from `path` to find the topmost directory containing `inventory.bu`.
fn find_root(path: &Path) -> Option<PathBuf> {
    let dir = if path.is_file() { path.parent()? } else { path };
    let mut root: Option<PathBuf> = None;
    let mut cur = dir;
    loop {
        if cur.join("inventory.bu").exists() { root = Some(cur.to_path_buf()); }
        match cur.parent() { Some(p) => cur = p, None => break }
    }
    root
}

// ── Diagnostics ───────────────────────────────────────────────────────────────

/// Run the full validation pipeline and return a map of URI → diagnostics array.
fn validate_project(root: &Path) -> HashMap<String, Vec<Value>> {
    let mut by_uri: HashMap<String, Vec<Value>> = HashMap::new();

    let all = validator::validate_tree(root);

    for e in &all.parse {
        let uri = path_to_uri(Path::new(&e.file));
        by_uri.entry(uri).or_default().push(make_diag(e.line, e.col, &e.message, 1));
    }
    for e in &all.structural {
        let uri = path_to_uri(Path::new(&e.file));
        by_uri.entry(uri).or_default().push(make_diag(e.line, e.col, &e.message, 1));
    }

    if all.is_empty() {
        for e in typecheck::typecheck_tree(root) {
            let uri = path_to_uri(Path::new(&e.file));
            by_uri.entry(uri).or_default().push(make_diag(e.line, e.col, &e.message, 1));
        }
    }

    by_uri
}

/// Build an LSP diagnostic object.
/// `severity`: 1=Error 2=Warning 3=Info 4=Hint
/// Lines and cols are 1-based from our side, 0-based for LSP.
fn make_diag(line: usize, col: usize, msg: &str, severity: u8) -> Value {
    let l = if line == 0 { 0 } else { line.saturating_sub(1) } as u64;
    let c = if col  == 0 { 0 } else { col.saturating_sub(1)  } as u64;
    json!({
        "range": {
            "start": { "line": l, "character": c },
            "end":   { "line": l, "character": c + 80 }
        },
        "severity": severity,
        "source":   "bullang",
        "message":  msg
    })
}

// ── Function index ────────────────────────────────────────────────────────────

fn build_fn_index(root: &Path) -> FnIndex {
    let mut index = FnIndex::new();
    collect_fns(root, &mut index);
    index
}

fn collect_fns(dir: &Path, index: &mut FnIndex) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fns(&path, index);
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.ends_with(".bu")
                || name == "inventory.bu"
                || name == "blueprint.bu" { continue; }
            let Ok(src) = std::fs::read_to_string(&path) else { continue };
            let path_str = path.display().to_string();
            let result = parser::parse_file_tolerant(&src, &path_str);
            if let BuFile::Source(sf) = result.file {
                for bullet in sf.bullets {
                    index.insert(bullet.name.clone(), (bullet, path_str.clone()));
                }
            }
        }
    }
}

// ── Hover helpers ─────────────────────────────────────────────────────────────

fn format_hover(bullet: &Bullet, file: &str) -> String {
    let params: Vec<String> = bullet.params.iter()
        .map(|p| format!("{}: {}", p.name, fmt_type(&p.ty)))
        .collect();
    let display_file = Path::new(file)
        .file_name().and_then(|n| n.to_str()).unwrap_or(file);
    format!(
        "```\nlet {}({}) -> {}: {}\n```\n*{}*",
        bullet.name,
        params.join(", "),
        bullet.output.as_ref().map(|o| o.name.as_str()).unwrap_or("_"),
        bullet.output.as_ref().map(|o| fmt_type(&o.ty)).unwrap_or_default(),
        display_file,
    )
}

fn fmt_type(ty: &BuType) -> String {
    match ty {
        BuType::Named(s)     => s.clone(),
        BuType::Tuple(inner) => format!(
            "Tuple[{}]", inner.iter().map(fmt_type).collect::<Vec<_>>().join(", ")
        ),
        BuType::Unknown      => "_".to_string(),
    }
}

/// Return the identifier token at a (0-based) line/col position.
fn word_at(source: &str, line: u64, col: u64) -> Option<String> {
    let text_line = source.lines().nth(line as usize)?;
    let col = col as usize;
    if col > text_line.len() { return None; }
    let start = text_line[..col]
        .rfind(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|i| i + 1).unwrap_or(0);
    let end = col + text_line[col..]
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(text_line.len() - col);
    let word = text_line[start..end].trim();
    if word.is_empty() { None } else { Some(word.to_string()) }
}

// ── Main LSP loop ─────────────────────────────────────────────────────────────

/// How long the buffer must be quiet before diagnostics are recomputed.
///
/// Validation walks and parses the whole project, so doing it per keystroke
/// made a fast typist queue up work faster than the server could finish it.
/// A pause is also when diagnostics are actually wanted: mid-word, every
/// identifier is briefly undefined.
const DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(300);

pub fn run() {
    use std::sync::mpsc;

    let stdout = io::stdout();
    let mut writer = io::BufWriter::new(stdout.lock());
    let mut server = Server::new();

    // Reading moves to its own thread so the main loop can wait with a
    // timeout — which is what makes debouncing possible at all. A single
    // blocking `read_message` has no way to notice that the typing stopped.
    let (tx, rx) = mpsc::channel::<Value>();
    std::thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = io::BufReader::new(stdin.lock());
        while let Some(msg) = read_message(&mut reader) {
            if tx.send(msg).is_err() {
                break;
            }
        }
    });

    // The document whose diagnostics are owed, once the typing settles.
    let mut pending: Option<String> = None;

    loop {
        let timeout = if pending.is_some() { DEBOUNCE } else { std::time::Duration::from_secs(3600) };
        match rx.recv_timeout(timeout) {
            Ok(msg) => {
                if let Some(uri) = handle(&mut server, &mut writer, msg) {
                    pending = Some(uri);
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if let Some(uri) = pending.take() {
                    push_diagnostics(&mut server, &mut writer, &uri);
                }
            }
            // The reader thread ended: stdin closed.
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

/// Returns the URI whose diagnostics are now stale, if any. The caller
/// decides when to recompute them.
fn handle(server: &mut Server, writer: &mut impl Write, msg: Value) -> Option<String> {
    let method = msg["method"].as_str().unwrap_or("");
    let id     = &msg["id"];
    let params = &msg["params"];

    match method {
        // ── Lifecycle ─────────────────────────────────────────────────────────
        "initialize" => {
            let caps = json!({
                "textDocumentSync": 1,          // 1 = Full sync
                "hoverProvider": true,
                "definitionProvider": true,
                // `bullarchy fmt` already produces canonical Bullang, so the
                // editor may as well be able to ask for it.
                "documentFormattingProvider": true
            });
            send_message(writer, &response(id, json!({
                "capabilities": caps,
                "serverInfo": { "name": "bullang-lsp", "version": env!("CARGO_PKG_VERSION") }
            })));
        }
        "initialized" => {}   // notification, no reply needed
        "shutdown" => {
            server.shutdown_requested = true;
            send_message(writer, &response(id, Value::Null));
        }
        "exit" => std::process::exit(if server.shutdown_requested { 0 } else { 1 }),

        // ── Document lifecycle ────────────────────────────────────────────────
        "textDocument/didOpen" => {
            let uri  = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
            let text = params["textDocument"]["text"].as_str().unwrap_or("").to_string();
            server.docs.insert(uri.clone(), Document { content: text });
            return Some(uri);
        }
        "textDocument/didChange" => {
            let uri = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
            if let Some(change) = params["contentChanges"].as_array().and_then(|a| a.last()) {
                let text = change["text"].as_str().unwrap_or("").to_string();
                server.docs.insert(uri.clone(), Document { content: text });
            }
            return Some(uri);
        }
        "textDocument/didSave" => {
            let uri = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
            return Some(uri);
        }
        "textDocument/didClose" => {
            let uri = params["textDocument"]["uri"].as_str().unwrap_or("").to_string();
            server.docs.remove(&uri);
            // Clear diagnostics for the closed file
            send_message(writer, &notification("textDocument/publishDiagnostics", json!({
                "uri": uri, "diagnostics": []
            })));
        }

        // ── Hover ─────────────────────────────────────────────────────────────
        "textDocument/hover" => {
            let uri  = params["textDocument"]["uri"].as_str().unwrap_or("");
            let line = params["position"]["line"].as_u64().unwrap_or(0);
            let col  = params["position"]["character"].as_u64().unwrap_or(0);

            let result = (|| -> Option<Value> {
                let doc  = server.docs.get(uri)?;
                let word = word_at(&doc.content, line, col)?;

                let root_str = uri_to_path(uri)
                    .and_then(|p| find_root(&p))
                    .map(|r| r.display().to_string())?;

                let (bullet, file) = server.fn_index.get(&root_str)?.get(&word)?;
                let md = format_hover(bullet, file);
                Some(json!({
                    "contents": { "kind": "markdown", "value": md }
                }))
            })();

            send_message(writer, &response(id, result.unwrap_or(Value::Null)));
        }

        // ── Go-to-definition ──────────────────────────────────────────────────
        "textDocument/definition" => {
            let uri  = params["textDocument"]["uri"].as_str().unwrap_or("");
            let line = params["position"]["line"].as_u64().unwrap_or(0);
            let col  = params["position"]["character"].as_u64().unwrap_or(0);

            let result = (|| -> Option<Value> {
                let doc  = server.docs.get(uri)?;
                let word = word_at(&doc.content, line, col)?;

                let root_str = uri_to_path(uri)
                    .and_then(|p| find_root(&p))
                    .map(|r| r.display().to_string())?;

                let (bullet, file) = server.fn_index.get(&root_str)?.get(&word)?;

                let def_uri  = path_to_uri(Path::new(file));
                let def_line = if bullet.span.line > 0 { bullet.span.line - 1 } else { 0 };
                let def_col  = if bullet.span.col  > 0 { bullet.span.col  - 1 } else { 0 };

                Some(json!({
                    "uri": def_uri,
                    "range": {
                        "start": { "line": def_line, "character": def_col },
                        "end":   { "line": def_line, "character": def_col }
                    }
                }))
            })();

            send_message(writer, &response(id, result.unwrap_or(Value::Null)));
        }

        // ── Unknown request ───────────────────────────────────────────────────
        _ => {
            // Only respond if this is a request (has an id), not a notification
            if !id.is_null() {
                send_message(writer, &error_response(id, -32601, "method not found"));
            }
        }
    }

    None
}

// ── Diagnostics push ──────────────────────────────────────────────────────────

fn push_diagnostics(server: &mut Server, writer: &mut impl Write, trigger_uri: &str) {
    let Some(root_path) = uri_to_path(trigger_uri).and_then(|p| find_root(&p)) else {
        // No project root — publish empty diagnostics to clear any stale ones
        send_message(writer, &notification("textDocument/publishDiagnostics", json!({
            "uri": trigger_uri, "diagnostics": []
        })));
        return;
    };

    let root_str = root_path.display().to_string();

    // Rebuild function index
    let fn_index = build_fn_index(&root_path);
    server.fn_index.insert(root_str.clone(), fn_index);

    // Validate what the editor is showing, not what is on disk. `didChange`
    // updates `docs`; without this the diagnostics described the last saved
    // version, so a fresh error stayed invisible until save and a fixed one
    // stayed on screen.
    server.publish_overlay();
    let diags_by_uri = validate_project(&root_path);
    crate::overlay::clear();

    // Publish for every open document in this project, sending [] if no errors
    let open_uris: Vec<String> = server.docs.keys().cloned().collect();
    for uri in &open_uris {
        let Some(path) = uri_to_path(uri) else { continue };
        let Some(root) = find_root(&path) else { continue };
        if root.display().to_string() != root_str { continue }
        let empty = vec![];
        let diags = diags_by_uri.get(uri).unwrap_or(&empty);
        send_message(writer, &notification("textDocument/publishDiagnostics", json!({
            "uri": uri, "diagnostics": diags
        })));
    }

    // The triggering file may not be in `docs` yet — on the very first
    // `didOpen`, for instance. Publishing for it unconditionally sent the same
    // diagnostics twice for every file that *was* open, which some editors
    // render as duplicate entries in their problem list.
    if !open_uris.iter().any(|u| u == trigger_uri) {
        let empty = vec![];
        let trigger_diags = diags_by_uri.get(trigger_uri).unwrap_or(&empty);
        send_message(writer, &notification("textDocument/publishDiagnostics", json!({
            "uri": trigger_uri, "diagnostics": trigger_diags
        })));
    }
}
