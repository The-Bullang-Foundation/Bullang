//! Shared C helpers for the string builtins.
//!
//! C has no owning string type, so every string builtin here writes into a
//! destination the caller declared — `ft_strcpy` semantics. Nothing in this
//! file allocates. See `stdlib::CDest` for why.
//!
//! These are `&'static str` constants rather than functions because the
//! backend deduplicates helpers by their text: a file using `to_upper` ten
//! times emits `ft_to_upper` once, and a file using both `to_upper` and
//! `trim` emits `ft_strlen` once even though both name it.

/// Emission order.
///
/// C requires a function to be declared before it is used, and helpers are
/// deduplicated into a set — which has no idea that `ft_trim` calls
/// `ft_strlen`. Sorting by this list puts shared primitives first. A helper
/// absent from it has no dependants and goes last, in whatever order the
/// collector produced.
pub const ORDER: &[&str] = &[
    super::open::C_CORE,
    FT_STRLEN,
    FT_UTF8_LEN,
    FT_TRIM,
    FT_REPLACE,
];

/// `<string.h>` is needed by every helper below; `<stdlib.h>` by the ones
/// that convert numbers.
pub const IMPORTS: &[&str] = &["#include <stddef.h>", "#include <string.h>"];

pub const FT_STRLEN: &str = r#"static size_t	ft_strlen(const char *s)
{
	size_t	i;

	i = 0;
	while (s[i])
		i++;
	return (i);
}
"#;

/// Length in characters, not bytes: every byte that is not a UTF-8
/// continuation byte starts one character. `len` is documented as counting
/// characters in all six backends, and C was the one counting bytes.
pub const FT_UTF8_LEN: &str = r#"static long long	ft_utf8_len(const char *s)
{
	long long	n;
	size_t		i;

	n = 0;
	i = 0;
	while (s[i])
	{
		if ((s[i] & 0xC0) != 0x80)
			n++;
		i++;
	}
	return (n);
}
"#;

pub const FT_TO_UPPER: &str = r#"static void	ft_to_upper(char *dest, const char *src)
{
	size_t	i;

	i = 0;
	while (src[i])
	{
		if (src[i] >= 'a' && src[i] <= 'z')
			dest[i] = (char)(src[i] - 32);
		else
			dest[i] = src[i];
		i++;
	}
	dest[i] = '\0';
}
"#;

pub const FT_TO_LOWER: &str = r#"static void	ft_to_lower(char *dest, const char *src)
{
	size_t	i;

	i = 0;
	while (src[i])
	{
		if (src[i] >= 'A' && src[i] <= 'Z')
			dest[i] = (char)(src[i] + 32);
		else
			dest[i] = src[i];
		i++;
	}
	dest[i] = '\0';
}
"#;

pub const FT_TRIM: &str = r#"static int	ft_is_space(char c)
{
	return (c == ' ' || c == '\t' || c == '\n'
		|| c == '\v' || c == '\f' || c == '\r');
}

static void	ft_trim(char *dest, const char *src)
{
	size_t	start;
	size_t	end;
	size_t	i;

	start = 0;
	while (src[start] && ft_is_space(src[start]))
		start++;
	end = ft_strlen(src);
	while (end > start && ft_is_space(src[end - 1]))
		end--;
	i = 0;
	while (start + i < end)
	{
		dest[i] = src[start + i];
		i++;
	}
	dest[i] = '\0';
}
"#;

/// Exact size for `replace_str`'s destination: the source length, adjusted by
/// the difference in length for every occurrence found, plus the terminator.
/// Counting first is what lets the destination be a plain array — the old
/// code guessed with `malloc` and could not have got this right in general.
pub const FT_REPLACE: &str = r#"static size_t	ft_replace_size(const char *s, const char *from, const char *to)
{
	size_t	flen;
	size_t	tlen;
	size_t	n;
	size_t	i;

	flen = ft_strlen(from);
	tlen = ft_strlen(to);
	if (flen == 0)
		return (ft_strlen(s) + 1);
	n = 0;
	i = 0;
	while (s[i])
	{
		if (strncmp(s + i, from, flen) == 0)
		{
			n++;
			i += flen;
		}
		else
			i++;
	}
	return (ft_strlen(s) + n * tlen - n * flen + 1);
}

static void	ft_replace_str(char *dest, const char *s, const char *from,
		const char *to)
{
	size_t	flen;
	size_t	tlen;
	size_t	i;
	size_t	j;

	flen = ft_strlen(from);
	tlen = ft_strlen(to);
	i = 0;
	j = 0;
	while (s[i])
	{
		if (flen > 0 && strncmp(s + i, from, flen) == 0)
		{
			memcpy(dest + j, to, tlen);
			j += tlen;
			i += flen;
		}
		else
			dest[j++] = s[i++];
	}
	dest[j] = '\0';
}
"#;

/// A 64-bit integer is at most 20 characters wide (`-9223372036854775808`),
/// so `BU_I64_STR_MAX` of 24 is a fixed, provably sufficient size — no
/// counting pass and no allocation. `LLONG_MIN` is negated in `unsigned long
/// long` because negating it as a signed value is undefined.
pub const FT_I64_TO_STR: &str = r#"#define BU_I64_STR_MAX 24

static void	ft_i64_to_str(char *dest, long long v)
{
	unsigned long long	u;
	char				tmp[BU_I64_STR_MAX];
	size_t				n;
	size_t				i;

	if (v < 0)
		u = (unsigned long long)(-(v + 1)) + 1;
	else
		u = (unsigned long long)v;
	n = 0;
	if (u == 0)
		tmp[n++] = '0';
	while (u > 0)
	{
		tmp[n++] = (char)('0' + (u % 10));
		u /= 10;
	}
	i = 0;
	if (v < 0)
		dest[i++] = '-';
	while (n > 0)
		dest[i++] = tmp[--n];
	dest[i] = '\0';
}
"#;
