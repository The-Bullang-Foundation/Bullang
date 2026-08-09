//! `builtin::run(cmd: String) -> i64` — run a shell command, return its exit
//! code.
//!
//! The shell is chosen at run time, not transpile time, so one emitted
//! program works on both Windows and Unix: `cmd /C` there, `sh -c` here.

use bullang::ast::{Backend, Param};
use super::Requirements;

pub const META: &str = "run";

pub fn requirements(backend: &Backend) -> Requirements {
    match backend {
        Backend::Rust   => Requirements::new(&[], &[RUST_HELPER]),
        Backend::Python => Requirements::new(&[], &[PY_HELPER]),
        Backend::C      => Requirements::imports(&["#include <stdlib.h>"]),
        Backend::Cpp    => Requirements::imports(&["#include <cstdlib>", "#include <string>"]),
        Backend::Go     => Requirements::helper(&[GO_HELPER], &["os", "os/exec", "runtime"]),
        Backend::Java   => Requirements::new(&[], &[JAVA_HELPER]),
        Backend::Unknown(_) => Requirements::NONE,
    }
}

pub fn emit(params: &[Param], backend: &Backend) -> Result<String, String> {
    let p = super::need(META, params, 1)?;
    let cmd = p[0];
    Ok(match backend {
        Backend::Rust   => format!("bu_run(&{cmd})"),
        Backend::Python => format!("bu_run({cmd})"),
        // system(3) hands the string to the platform's own shell already.
        Backend::C      => format!("((long long)system({cmd}))"),
        Backend::Cpp    => format!("((long long)std::system({cmd}.c_str()))"),
        Backend::Go     => format!("buRun({cmd})"),
        Backend::Java   => format!("buRun({cmd})"),
        Backend::Unknown(_) => return Err(super::unsupported(META, backend)),
    })
}

const RUST_HELPER: &str = r#"fn bu_run(cmd: &str) -> i64 {
    let status = if cfg!(target_os = "windows") {
        std::process::Command::new("cmd").args(["/C", cmd]).status()
    } else {
        std::process::Command::new("sh").args(["-c", cmd]).status()
    };
    match status {
        Ok(s) => s.code().unwrap_or(-1) as i64,
        Err(_) => -1,
    }
}
"#;

const PY_HELPER: &str = r#"def bu_run(cmd):
    return int(__import__("subprocess").call(cmd, shell=True))
"#;

const GO_HELPER: &str = r#"func buRun(cmd string) int64 {
	var c *exec.Cmd
	if runtime.GOOS == "windows" {
		c = exec.Command("cmd", "/C", cmd)
	} else {
		c = exec.Command("sh", "-c", cmd)
	}
	c.Stdin = os.Stdin
	c.Stdout = os.Stdout
	c.Stderr = os.Stderr
	if err := c.Run(); err != nil {
		if ee, ok := err.(*exec.ExitError); ok {
			return int64(ee.ExitCode())
		}
		return -1
	}
	return 0
}
"#;

const JAVA_HELPER: &str = r#"static long buRun(String cmd) {
    try {
        String[] sh = System.getProperty("os.name", "").toLowerCase().contains("win")
            ? new String[] { "cmd", "/C", cmd }
            : new String[] { "sh", "-c", cmd };
        return new ProcessBuilder(sh).inheritIO().start().waitFor();
    } catch (Exception e) {
        return -1;
    }
}
"#;
