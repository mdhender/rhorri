// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! C library string conversions with the C engine's semantics.
//!
//! The data files are bytes, not UTF-8, so these work on `&[u8]`.

/// `iswhite()` from `z.h`: space or tab only.
#[must_use]
pub fn is_white(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// `isspace()` in the C locale, as `atoi()` and `sscanf()` skip it.
fn is_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\x0b' | b'\x0c' | b'\r')
}

/// Parses an optionally signed decimal integer after leading white space.
///
/// Returns the value as `strtol()` computes it (saturating at the `long`
/// range) and the number of bytes consumed, or `None` if there are no digits.
fn strtol(s: &[u8]) -> Option<(i64, usize)> {
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    let negative = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let start = i;
    let mut value: i64 = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        let digit = i64::from(s[i] - b'0');
        value = if negative {
            value.saturating_mul(10).saturating_sub(digit)
        } else {
            value.saturating_mul(10).saturating_add(digit)
        };
        i += 1;
    }
    (i > start).then_some((value, i))
}

/// `atoi()`: `(int) strtol(s, NULL, 10)`, 0 if there is no number.
///
/// The `long` result is truncated to `int`, as the C cast does on the LP64
/// hosts the fixtures run on.
#[must_use]
pub fn atoi(s: &[u8]) -> i32 {
    strtol(s).map_or(0, |(v, _)| v as i32)
}

/// `sscanf(s, "%d %d ...")` for up to `n` integers: the values converted
/// before the first failure.
///
/// A field `sscanf()` doesn't reach keeps the value it had, so callers
/// assign only the fields this returns.
#[must_use]
pub fn scan_ints(s: &[u8], n: usize) -> Vec<i32> {
    let mut out = Vec::with_capacity(n);
    let mut rest = s;
    while out.len() < n {
        let Some((v, used)) = strtol(rest) else {
            break;
        };
        out.push(v as i32);
        rest = &rest[used..];
    }
    out
}

/// `strtoull(s, NULL, 0)`: decimal, `0x` hex or `0` octal, 0 if there is no
/// number. Values past `u64::MAX` saturate, as `strtoull()` does.
#[must_use]
pub fn strtoull(s: &[u8]) -> u64 {
    let mut i = 0;
    while i < s.len() && is_space(s[i]) {
        i += 1;
    }
    let negative = match s.get(i) {
        Some(b'-') => {
            i += 1;
            true
        }
        Some(b'+') => {
            i += 1;
            false
        }
        _ => false,
    };
    let rest = &s[i..];
    let (radix, digits) = if (rest.starts_with(b"0x") || rest.starts_with(b"0X"))
        && rest.get(2).is_some_and(u8::is_ascii_hexdigit)
    {
        (16, &rest[2..])
    } else if rest.starts_with(b"0") {
        (8, rest)
    } else {
        (10, rest)
    };
    let mut value: u64 = 0;
    for &c in digits {
        let Some(d) = char::from(c).to_digit(radix) else {
            break;
        };
        value = value
            .saturating_mul(u64::from(radix))
            .saturating_add(u64::from(d));
    }
    if negative {
        value.wrapping_neg()
    } else {
        value
    }
}

/// The line as `getlin_ew()`/`readlin_ew()` leave it: leading white space
/// removed, every byte below 32 (and, because `char` is signed, every byte
/// of 128 and up) turned into a space, then trailing white space removed.
#[must_use]
pub fn strip_ew(line: &[u8]) -> Vec<u8> {
    let start = line
        .iter()
        .position(|&c| !is_white(c))
        .unwrap_or(line.len());
    let mut out: Vec<u8> = line[start..]
        .iter()
        .map(|&c| if (32..128).contains(&c) { c } else { b' ' })
        .collect();
    while out.last().is_some_and(|&c| is_white(c)) {
        out.pop();
    }
    out
}

/// `lower_array[]` from `z.c`: ASCII letters folded, every other byte kept.
#[must_use]
pub fn to_lower(c: u8) -> u8 {
    c.to_ascii_lowercase()
}

/// `i_strcmp()`: case-insensitive equality.
#[must_use]
pub fn i_str_eq(s: &[u8], t: &[u8]) -> bool {
    s.len() == t.len() && s.iter().zip(t).all(|(&a, &b)| to_lower(a) == to_lower(b))
}

/// `fuzzy_strcmp()`: whether `two` is `one` with two letters swapped, one
/// letter dropped, one extra or one wrong. Short words must match exactly.
#[must_use]
pub fn fuzzy_eq(one: &[u8], two: &[u8]) -> bool {
    let (l1, l2) = (one.len(), two.len());
    (l2 >= 4 && fuzzy_transpose(one, two))
        || (l2 >= 5 && l1 == l2 + 1 && fuzzy_one_less(one, two))
        || (l2 >= 5 && l1 + 1 == l2 && fuzzy_one_less(two, one))
        || (l2 >= 5 && l1 == l2 && fuzzy_one_bad(one, two))
}

/// `two` with one adjacent pair swapped equals `one`.
fn fuzzy_transpose(one: &[u8], two: &[u8]) -> bool {
    if one.len() != two.len() {
        return false;
    }
    let mut buf = two.to_vec();
    for i in 0..two.len().saturating_sub(1) {
        buf.swap(i, i + 1);
        if i_str_eq(one, &buf) {
            return true;
        }
        buf.swap(i, i + 1);
    }
    false
}

/// `long` is `short` with exactly one byte inserted (`fuzzy_one_less()`, and
/// `fuzzy_one_extra()` with the arguments swapped).
fn fuzzy_one_less(long: &[u8], short: &[u8]) -> bool {
    let mut skipped = false;
    let mut i = 0;
    for &c in short {
        if to_lower(long[i]) != to_lower(c) {
            if skipped {
                return false;
            }
            skipped = true;
            i += 1;
            if to_lower(long[i]) != to_lower(c) {
                return false;
            }
        }
        i += 1;
    }
    true
}

/// At most one byte differs.
fn fuzzy_one_bad(one: &[u8], two: &[u8]) -> bool {
    one.iter()
        .zip(two)
        .filter(|&(&a, &b)| to_lower(a) != to_lower(b))
        .count()
        <= 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atoi_like_c() {
        assert_eq!(atoi(b"  42abc"), 42);
        assert_eq!(atoi(b"-7"), -7);
        assert_eq!(atoi(b"+3"), 3);
        assert_eq!(atoi(b"x1"), 0);
        assert_eq!(atoi(b""), 0);
        assert_eq!(atoi(b"4294967297"), 1, "long truncated to int");
        assert_eq!(atoi(b"99999999999999999999"), -1, "saturates to LONG_MAX");
    }

    #[test]
    fn scan_ints_stops_at_the_first_failure() {
        assert_eq!(scan_ints(b"1 2 3 \\", 5), [1, 2, 3]);
        assert_eq!(scan_ints(b"1 2 3 4 5 6", 5), [1, 2, 3, 4, 5]);
        assert!(scan_ints(b"", 2).is_empty());
    }

    #[test]
    fn strtoull_bases() {
        assert_eq!(strtoull(b"12"), 12);
        assert_eq!(strtoull(b"0x1f"), 31);
        assert_eq!(strtoull(b"017"), 15);
        assert_eq!(strtoull(b"0x"), 0);
        assert_eq!(strtoull(b"-1"), u64::MAX);
    }

    #[test]
    fn strip_ew_turns_controls_and_high_bytes_into_spaces() {
        assert_eq!(strip_ew(b" \ta\tb\x01c\xe9 \t"), b"a b c");
    }

    #[test]
    fn fuzzy_matches_like_c() {
        assert!(fuzzy_eq(b"attack", b"atatck"), "transpose");
        assert!(fuzzy_eq(b"attack", b"atack"), "one less");
        assert!(fuzzy_eq(b"attack", b"attacks"), "one extra");
        assert!(fuzzy_eq(b"attack", b"attaxk"), "one bad");
        assert!(!fuzzy_eq(b"attack", b"atxaxk"));
        assert!(!fuzzy_eq(b"move", b"mov"), "too short for one less");
        assert!(fuzzy_eq(b"move", b"mvoe"), "four letters may transpose");
    }
}
