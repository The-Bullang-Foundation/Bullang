//! Unsaved editor buffers, standing in for files on disk.
//!
//! The language server validated from disk on every keystroke, while
//! `didChange` only updated its in-memory copy. So the diagnostics it showed
//! described the last *saved* version of the file: type an error and nothing
//! appeared, fix one and it stayed. It was simultaneously expensive — a full
//! project re-read per keypress — and wrong.
//!
//! The validator and type checker read files in five places. Rather than
//! thread a map of open buffers through every function between the server and
//! those five, the overlay is consulted at the read itself: `read_source` is
//! what they call, and it returns the buffer when one exists.
//!
//! Thread-local rather than a parameter because the server is single-threaded
//! and nothing else in Bullarchy sets it — `bullarchy check` and `convert`
//! leave it empty and read straight from disk, exactly as before.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

thread_local! {
    static OVERLAY: RefCell<HashMap<PathBuf, String>> = RefCell::new(HashMap::new());
}

/// Read `path`, preferring an open buffer's contents over what is on disk.
pub fn read_source(path: &Path) -> std::io::Result<String> {
    // An editor's URI and the tree walker's path can spell the same file
    // differently, so both are compared as resolved paths.
    let resolved = path.canonicalize().ok();
    let overlaid = OVERLAY.with(|o| {
        let map = o.borrow();
        if let Some(text) = map.get(path) {
            return Some(text.clone());
        }
        let target = resolved.as_ref()?;
        map.iter()
            .find(|(k, _)| k.canonicalize().as_ref().ok() == Some(target))
            .map(|(_, v)| v.clone())
    });
    match overlaid {
        Some(text) => Ok(text),
        None       => std::fs::read_to_string(path),
    }
}

/// Replace the set of open buffers.
pub fn set(docs: HashMap<PathBuf, String>) {
    OVERLAY.with(|o| *o.borrow_mut() = docs);
}

/// Forget every open buffer.
pub fn clear() {
    OVERLAY.with(|o| o.borrow_mut().clear());
}
