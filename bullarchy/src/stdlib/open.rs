//! `builtin::open(path: String, mode: String) -> i64`
//!
//! Returns a descriptor: an index into a table the generated program keeps for
//! itself, not a raw operating-system descriptor. 0, 1 and 2 are seeded with
//! stdin, stdout and stderr; `open` allocates from 3 upward.
//!
//! That indirection is the whole point. A raw POSIX descriptor is an `int`; a
//! Windows native handle is a pointer-sized opaque value, so code written
//! against `open(2)`, `<unistd.h>` or `FromRawFd` cannot build there at all —
//! which is what five of the six backends used to emit. Going through a table
//! lets every backend use its own ordinary, portable file type instead:
//! `FILE*`, `std::fstream`, `os.File`, `RandomAccessFile`, `std::fs::File`.
//! No `#ifdef`, no JNI, and generated code a reader can follow.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "open";

/// Which part of the shim a builtin needs.
///
/// C and C++ compile generated code with `-Wall -Werror`, where a `static`
/// function that is never called is a build failure. Emitting the whole shim
/// because a program printed one line would break every such program, so
/// those two backends take only the piece they use plus the shared table.
///
/// The other four have no equivalent rule — an uncalled function is at worst
/// a warning there — so they take the shim whole, which keeps it readable as
/// one unit.
pub enum Shim { Open, Close, Out, In }

pub fn requirements_for(shim: Shim, backend: &Backend) -> Requirements {
    match backend {
        Backend::Rust   => Requirements::new(RUST_IMPORTS, &[RUST_TABLE]),
        Backend::Go     => Requirements::helper(&[GO_TABLE], GO_IMPORTS),
        Backend::Java   => Requirements::new(&[], &[JAVA_TABLE]),
        Backend::Python => Requirements::new(&[], &[PY_TABLE]),
        Backend::C => Requirements::new(C_IMPORTS, match shim {
            Shim::Open  => &[C_CORE, C_OPEN],
            Shim::Close => &[C_CORE, C_CLOSE],
            Shim::Out   => &[C_CORE, C_OUT],
            Shim::In    => &[C_CORE, C_IN],
        }),
        Backend::Cpp => Requirements::new(CPP_IMPORTS, match shim {
            Shim::Open  => &[CPP_CORE, CPP_OPEN],
            Shim::Close => &[CPP_CORE, CPP_CLOSE],
            Shim::Out   => &[CPP_CORE, CPP_OUT],
            Shim::In    => &[CPP_CORE, CPP_IN],
        }),
        Backend::Unknown(_) => Requirements::NONE,
    }
}

pub fn requirements(backend: &Backend) -> Requirements {
    requirements_for(Shim::Open, backend)
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 2)?;
    let (path, mode) = (p[0], p[1]);
    Ok(match backend {
        Backend::Rust   => format!("bu_open({}, {})", path, mode),
        Backend::Python => format!("bu_open({}, {})", path, mode),
        Backend::C      => format!("bu_open({}, {})", path, mode),
        Backend::Cpp    => format!("bu_open({}, {})", path, mode),
        Backend::Go     => format!("buOpen({}, {})", path, mode),
        Backend::Java   => format!("BuIo.open({}, {})", path, mode),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

// ── The table, one shim per backend ───────────────────────────────────────
//
// Every shim keeps the same shape: a growable list of open files, indices 0-2
// reserved for the standard streams, `open` appending and returning the new
// index. Modes are r, w, a, rw; an unknown mode is an error rather than a
// silent fall back to read-only, which is what the old code did in two
// separate places.

pub const RUST_IMPORTS: &[&str] = &[
    "use std::cell::RefCell;",
    "use std::fs::{File, OpenOptions};",
    "use std::io::{BufRead, BufReader, Read, Write};",
];

pub const RUST_TABLE: &str = r#"thread_local! {
    static BU_FILES: RefCell<Vec<Option<File>>> = RefCell::new(vec![None, None, None]);
}

fn bu_open(path: &str, mode: &str) -> i64 {
    let opened = match mode {
        "r"  => OpenOptions::new().read(true).open(path),
        "w"  => OpenOptions::new().write(true).create(true).truncate(true).open(path),
        "a"  => OpenOptions::new().append(true).create(true).open(path),
        "rw" => OpenOptions::new().read(true).write(true).create(true).open(path),
        _    => {
            eprintln!("open: unknown mode '{}' — use r, w, a or rw", mode);
            return -1;
        }
    };
    match opened {
        Ok(f) => BU_FILES.with(|t| {
            let mut t = t.borrow_mut();
            t.push(Some(f));
            (t.len() - 1) as i64
        }),
        Err(e) => {
            eprintln!("open: {}: {}", path, e);
            -1
        }
    }
}

fn bu_close(fd: i64) {
    if fd < 3 { return; }
    BU_FILES.with(|t| {
        let mut t = t.borrow_mut();
        if let Some(slot) = t.get_mut(fd as usize) { *slot = None; }
    });
}

fn bu_out(fd: i64, content: &str) -> i64 {
    let bytes = content.as_bytes();
    let written = match fd {
        1 => std::io::stdout().write_all(bytes).and_then(|_| std::io::stdout().flush()),
        2 => std::io::stderr().write_all(bytes).and_then(|_| std::io::stderr().flush()),
        _ => BU_FILES.with(|t| {
            let mut t = t.borrow_mut();
            match t.get_mut(fd as usize).and_then(|s| s.as_mut()) {
                Some(f) => f.write_all(bytes).and_then(|_| f.flush()),
                None => Ok(()),
            }
        }),
    };
    if written.is_ok() { bytes.len() as i64 } else { -1 }
}

fn bu_in(fd: i64) -> String {
    let mut line = String::new();
    if fd == 0 {
        let _ = std::io::stdin().read_line(&mut line);
    } else {
        BU_FILES.with(|t| {
            let mut t = t.borrow_mut();
            if let Some(f) = t.get_mut(fd as usize).and_then(|s| s.as_mut()) {
                let mut byte = [0u8; 1];
                while let Ok(1) = f.read(&mut byte) {
                    if byte[0] == b'\n' { break; }
                    line.push(byte[0] as char);
                }
            }
        });
    }
    while line.ends_with('\n') || line.ends_with('\r') { line.pop(); }
    line
}
"#;

pub const C_IMPORTS: &[&str] = &[
    "#include <stdio.h>",
    "#include <stdlib.h>",
    "#include <string.h>",
];

pub const C_CORE: &str = r#"#define BU_MAX_FILES 64
/* A line read by bu_in has no length known before it is read, so the
   destination the caller declares is fixed rather than computed. */
#define BU_LINE_MAX 4096

static FILE	*bu_files[BU_MAX_FILES];
static int	bu_files_init = 0;

static inline void	bu_io_init(void)
{
	if (bu_files_init)
		return ;
	bu_files[0] = stdin;
	bu_files[1] = stdout;
	bu_files[2] = stderr;
	bu_files_init = 1;
}
"#;

pub const C_OPEN: &str = r#"static inline long long	bu_open(const char *path, const char *mode)
{
	const char	*m;
	int			i;

	bu_io_init();
	if (strcmp(mode, "r") == 0)
		m = "r";
	else if (strcmp(mode, "w") == 0)
		m = "w";
	else if (strcmp(mode, "a") == 0)
		m = "a";
	else if (strcmp(mode, "rw") == 0)
		m = "r+";
	else
	{
		fprintf(stderr, "open: unknown mode '%s' - use r, w, a or rw\n", mode);
		return (-1);
	}
	i = 3;
	while (i < BU_MAX_FILES && bu_files[i])
		i++;
	if (i == BU_MAX_FILES)
		return (-1);
	bu_files[i] = fopen(path, m);
	if (!bu_files[i])
		return (-1);
	return ((long long)i);
}
"#;

pub const C_CLOSE: &str = r#"static inline void	bu_close(long long fd)
{
	bu_io_init();
	if (fd < 3 || fd >= BU_MAX_FILES || !bu_files[fd])
		return ;
	fclose(bu_files[fd]);
	bu_files[fd] = NULL;
}
"#;

pub const C_OUT: &str = r#"static inline long long	bu_out(long long fd, const char *content)
{
	size_t	len;

	bu_io_init();
	if (fd < 0 || fd >= BU_MAX_FILES || !bu_files[fd])
		return (-1);
	len = strlen(content);
	if (fwrite(content, 1, len, bu_files[fd]) != len)
		return (-1);
	fflush(bu_files[fd]);
	return ((long long)len);
}
"#;

pub const C_IN: &str = r#"static inline char	*bu_in(char *dest, long long fd)
{
	int		c;
	size_t	i;

	bu_io_init();
	dest[0] = '\0';
	if (fd < 0 || fd >= BU_MAX_FILES || !bu_files[fd])
		return (dest);
	i = 0;
	while (i + 1 < BU_LINE_MAX)
	{
		c = fgetc(bu_files[fd]);
		if (c == EOF || c == '\n')
			break ;
		dest[i++] = (char)c;
	}
	dest[i] = '\0';
	return (dest);
}
"#;

pub const CPP_IMPORTS: &[&str] = &[
    "#include <cstdio>",
    "#include <fstream>",
    "#include <iostream>",
    "#include <memory>",
    "#include <string>",
    "#include <vector>",
];

pub const CPP_CORE: &str = r#"static std::vector<std::unique_ptr<std::fstream>> bu_files(3);
"#;

pub const CPP_OPEN: &str = r#"static long long bu_open(const std::string &path, const std::string &mode) {
	std::ios::openmode m;
	if (mode == "r") m = std::ios::in;
	else if (mode == "w") m = std::ios::out | std::ios::trunc;
	else if (mode == "a") m = std::ios::out | std::ios::app;
	else if (mode == "rw") m = std::ios::in | std::ios::out;
	else {
		std::cerr << "open: unknown mode '" << mode << "' - use r, w, a or rw\n";
		return -1;
	}
	auto f = std::make_unique<std::fstream>(path, m);
	if (!f->is_open()) return -1;
	bu_files.push_back(std::move(f));
	return (long long)(bu_files.size() - 1);
}
"#;

pub const CPP_CLOSE: &str = r#"static void bu_close(long long fd) {
	if (fd < 3 || (size_t)fd >= bu_files.size()) return;
	bu_files[fd].reset();
}
"#;

pub const CPP_OUT: &str = r#"static long long bu_out(long long fd, const std::string &content) {
	if (fd == 1) { std::cout << content << std::flush; return (long long)content.size(); }
	if (fd == 2) { std::cerr << content << std::flush; return (long long)content.size(); }
	if (fd < 0 || (size_t)fd >= bu_files.size() || !bu_files[fd]) return -1;
	*bu_files[fd] << content;
	bu_files[fd]->flush();
	return (long long)content.size();
}
"#;

pub const CPP_IN: &str = r#"static std::string bu_in(long long fd) {
	std::string line;
	if (fd == 0) { std::getline(std::cin, line); return line; }
	if (fd < 0 || (size_t)fd >= bu_files.size() || !bu_files[fd]) return line;
	std::getline(*bu_files[fd], line);
	return line;
}
"#;

pub const GO_IMPORTS: &[&str] = &["bufio", "fmt", "os", "strings"];

pub const GO_TABLE: &str = r#"var buFiles = []*os.File{os.Stdin, os.Stdout, os.Stderr}
var buReaders = map[int64]*bufio.Reader{}

func buOpen(path string, mode string) int64 {
	var f *os.File
	var err error
	switch mode {
	case "r":
		f, err = os.Open(path)
	case "w":
		f, err = os.Create(path)
	case "a":
		f, err = os.OpenFile(path, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0644)
	case "rw":
		f, err = os.OpenFile(path, os.O_RDWR|os.O_CREATE, 0644)
	default:
		fmt.Fprintf(os.Stderr, "open: unknown mode %q — use r, w, a or rw\n", mode)
		return -1
	}
	if err != nil {
		return -1
	}
	buFiles = append(buFiles, f)
	return int64(len(buFiles) - 1)
}

func buClose(fd int64) {
	if fd < 3 || fd >= int64(len(buFiles)) || buFiles[fd] == nil {
		return
	}
	buFiles[fd].Close()
	buFiles[fd] = nil
	delete(buReaders, fd)
}

func buOut(fd int64, content string) int64 {
	if fd < 0 || fd >= int64(len(buFiles)) || buFiles[fd] == nil {
		return -1
	}
	n, err := buFiles[fd].WriteString(content)
	if err != nil {
		return -1
	}
	return int64(n)
}

func buIn(fd int64) string {
	if fd < 0 || fd >= int64(len(buFiles)) || buFiles[fd] == nil {
		return ""
	}
	r, ok := buReaders[fd]
	if !ok {
		r = bufio.NewReader(buFiles[fd])
		buReaders[fd] = r
	}
	line, _ := r.ReadString('\n')
	return strings.TrimRight(line, "\r\n")
}
"#;

pub const JAVA_TABLE: &str = r#"static final class BuIo {
    private static final java.util.List<java.io.RandomAccessFile> files =
        new java.util.ArrayList<>(java.util.Arrays.asList(null, null, null));
    private static java.io.BufferedReader stdin;

    static long open(String path, String mode) {
        String m;
        switch (mode) {
            case "r":  m = "r";  break;
            case "w":  m = "rw"; break;
            case "a":  m = "rw"; break;
            case "rw": m = "rw"; break;
            default:
                System.err.println("open: unknown mode '" + mode + "' — use r, w, a or rw");
                return -1;
        }
        try {
            java.io.RandomAccessFile f = new java.io.RandomAccessFile(path, m);
            if (mode.equals("w")) f.setLength(0);
            if (mode.equals("a")) f.seek(f.length());
            files.add(f);
            return files.size() - 1;
        } catch (Exception e) {
            return -1;
        }
    }

    static void close(long fd) {
        if (fd < 3 || fd >= files.size() || files.get((int) fd) == null) return;
        try { files.get((int) fd).close(); } catch (Exception e) { }
        files.set((int) fd, null);
    }

    static long out(long fd, String content) {
        byte[] bytes = content.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        if (fd == 1) { System.out.print(content); System.out.flush(); return bytes.length; }
        if (fd == 2) { System.err.print(content); System.err.flush(); return bytes.length; }
        if (fd < 0 || fd >= files.size() || files.get((int) fd) == null) return -1;
        try { files.get((int) fd).write(bytes); return bytes.length; }
        catch (Exception e) { return -1; }
    }

    static String in(long fd) {
        try {
            if (fd == 0) {
                if (stdin == null) stdin = new java.io.BufferedReader(
                    new java.io.InputStreamReader(System.in));
                String line = stdin.readLine();
                return line == null ? "" : line;
            }
            if (fd < 0 || fd >= files.size() || files.get((int) fd) == null) return "";
            String line = files.get((int) fd).readLine();
            return line == null ? "" : line;
        } catch (Exception e) {
            return "";
        }
    }
}
"#;

pub const PY_TABLE: &str = r#"import sys as _sys

_bu_files = [_sys.stdin, _sys.stdout, _sys.stderr]

def bu_open(path, mode):
    modes = {"r": "r", "w": "w", "a": "a", "rw": "r+"}
    if mode not in modes:
        print("open: unknown mode '%s' — use r, w, a or rw" % mode, file=_sys.stderr)
        return -1
    try:
        _bu_files.append(open(path, modes[mode]))
    except OSError:
        return -1
    return len(_bu_files) - 1

def bu_close(fd):
    if fd < 3 or fd >= len(_bu_files) or _bu_files[fd] is None:
        return
    _bu_files[fd].close()
    _bu_files[fd] = None

def bu_out(fd, content):
    if fd < 0 or fd >= len(_bu_files) or _bu_files[fd] is None:
        return -1
    _bu_files[fd].write(content)
    _bu_files[fd].flush()
    return len(content.encode("utf-8"))

def bu_in(fd):
    if fd < 0 or fd >= len(_bu_files) or _bu_files[fd] is None:
        return ""
    return _bu_files[fd].readline().rstrip("\r\n")
"#;
