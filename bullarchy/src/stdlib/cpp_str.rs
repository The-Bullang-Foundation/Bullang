//! Shared C++ helpers for the string builtins.
//!
//! C++ has `std::string`, so unlike C these return by value and need no
//! caller-supplied destination. They are free functions rather than the
//! immediately-invoked lambdas the old code emitted: generated code is read
//! and edited by people, and `[&]() -> std::string { ... }()` in the middle
//! of an assignment is not something a reader can follow at a glance.

/// Emission order — same reason as C's: helpers are deduplicated into a set,
/// which has no idea `bu_out` reads the `bu_files` table declared in the shim
/// core. Anything absent has no dependants and goes last.
pub const ORDER: &[&str] = &[super::open::CPP_CORE];

pub const IMPORTS: &[&str] = &["#include <string>"];

pub const BU_TO_UPPER: &str = r#"static std::string bu_to_upper(const std::string &s) {
	std::string r = s;
	for (char &c : r)
		if (c >= 'a' && c <= 'z') c = static_cast<char>(c - 32);
	return r;
}
"#;

pub const BU_TO_LOWER: &str = r#"static std::string bu_to_lower(const std::string &s) {
	std::string r = s;
	for (char &c : r)
		if (c >= 'A' && c <= 'Z') c = static_cast<char>(c + 32);
	return r;
}
"#;

pub const BU_TRIM: &str = r#"static std::string bu_trim(const std::string &s) {
	size_t a = 0;
	size_t b = s.size();
	auto space = [](char c) {
		return c == ' ' || c == '\t' || c == '\n'
			|| c == '\v' || c == '\f' || c == '\r';
	};
	while (a < b && space(s[a])) a++;
	while (b > a && space(s[b - 1])) b--;
	return s.substr(a, b - a);
}
"#;

pub const BU_REPLACE_STR: &str = r#"static std::string bu_replace_str(const std::string &s,
		const std::string &from, const std::string &to) {
	if (from.empty()) return s;
	std::string r;
	size_t i = 0;
	while (i < s.size()) {
		if (s.compare(i, from.size(), from) == 0) {
			r += to;
			i += from.size();
		} else {
			r += s[i++];
		}
	}
	return r;
}
"#;

/// Characters, not bytes: every byte that is not a UTF-8 continuation byte
/// starts one character.
pub const BU_UTF8_LEN: &str = r#"static long long bu_utf8_len(const std::string &s) {
	long long n = 0;
	for (unsigned char c : s)
		if ((c & 0xC0) != 0x80) n++;
	return n;
}
"#;

/// `std::stoll` throws on a string that does not parse. Every other backend
/// returns 0, so this one does too.
pub const BU_STR_TO_I64: &str = r#"static long long bu_str_to_i64(const std::string &s) {
	try {
		return std::stoll(s);
	} catch (...) {
		return 0;
	}
}
"#;
