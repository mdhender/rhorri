// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Entity codes and display names (`olympia/code.c`).
//!
//! Names are byte strings, as in the data files, so messages that print
//! them must write bytes ([`eprint_bytes`]) rather than go through `str`.

use std::io::Write;

use crate::entity::{Db, Kind, Text};

/// `garrison_magic`: the box number that stands for a garrison in
/// messages. It should never be allocated.
pub const GARRISON_MAGIC: i32 = 999;

/// `letters`: the alphabet of player codes.
const LETTERS: &[u8; 26] = b"abcdefghijklmnopqrstuvwxyz";
/// `letters2`: the restricted alphabet of location codes, with no vowels
/// but `a` and no `l`.
const LETTERS2: &[u8; 20] = b"abcdfghjkmnpqrstvwxz";

/// `int_to_code()`: the code an entity number is shown as, such as `aa01`
/// for province 10001 or `ab3` for player 50013.
#[must_use]
pub fn int_to_code(l: i32) -> String {
    let letter = |set: &[u8], i: i32| char::from(set[i as usize]);

    if l < 10000 {
        return l.to_string();
    }
    if l < 50000 {
        // CCNN
        let l = l - 10000;
        let n = l % 100;
        let l = l / 100;
        let (a, b) = (l % 20, l / 20);
        return format!("{}{}{n:02}", letter(LETTERS2, b), letter(LETTERS2, a));
    }
    if l < 56760 {
        // CCN
        let l = l - 50000;
        let n = l % 10;
        let l = l / 10;
        let (a, b) = (l % 26, l / 26);
        return format!("{}{}{n}", letter(LETTERS, b), letter(LETTERS, a));
    }
    if l < 58760 {
        // CNN
        let l = l - 56760;
        let n = l % 10;
        let l = l / 10;
        let (a, b) = (l % 10, l / 10);
        return format!("{}{a}{n}", letter(LETTERS2, b));
    }
    if l < 59000 {
        return l.to_string();
    }
    if l < 79000 {
        // CNNN
        let l = l - 59000;
        let (a, n) = (l / 1000, l % 1000);
        return format!("{}{n:03}", letter(LETTERS2, a));
    }
    l.to_string()
}

/// `letter_val()`: the index of `c` in `set`, or 0 if it isn't there.
fn letter_val(c: u8, set: &[u8]) -> i32 {
    set.iter().position(|&x| x == c).map_or(0, |i| i as i32)
}

/// `code_to_int()`: the entity number an entity code stands for, the
/// inverse of [`int_to_code`]. A string of digits is read with `atoi`;
/// anything else that isn't a code is 0.
#[must_use]
pub fn code_to_int(s: &[u8]) -> i32 {
    let Some(&first) = s.first() else {
        return 0;
    };
    if first.is_ascii_digit() {
        return crate::cstr::atoi(s);
    }
    if !first.is_ascii_alphabetic() {
        return 0;
    }
    let alpha = |i: usize| s[i].is_ascii_alphabetic();
    let digit = |i: usize| s[i].is_ascii_digit();
    let num = |i: usize| i32::from(s[i]) - i32::from(b'0');
    let low = |i: usize| s[i].to_ascii_lowercase();
    match s.len() {
        3 if alpha(1) && digit(2) => {
            // CCN
            let a = i32::from(low(0)) - i32::from(b'a');
            let b = i32::from(low(1)) - i32::from(b'a');
            a * 260 + b * 10 + num(2) + 50000
        }
        3 if digit(1) && digit(2) => {
            // CNN
            letter_val(low(0), LETTERS2) * 100 + num(1) * 10 + num(2) + 56760
        }
        4 if alpha(1) && digit(2) && digit(3) => {
            // CCNN
            letter_val(low(0), LETTERS2) * 2000
                + letter_val(low(1), LETTERS2) * 100
                + num(2) * 10
                + num(3)
                + 10000
        }
        4 if digit(1) && digit(2) && digit(3) => {
            // CNNN
            letter_val(low(0), LETTERS2) * 1000 + num(1) * 100 + num(2) * 10 + num(3) + 59000
        }
        _ => 0,
    }
}

/// `scode()`: [`code_to_int`], after one leading `[` or `(`.
#[must_use]
pub fn scode(s: &[u8]) -> i32 {
    let s = match s.first() {
        Some(b'[' | b'(') => &s[1..],
        _ => s,
    };
    code_to_int(s)
}

/// `box_code()`: the entity's code in brackets, such as `[aa01]`.
#[must_use]
pub fn box_code(n: i32) -> Text {
    if n == GARRISON_MAGIC {
        return b"Garrison".to_vec();
    }
    format!("[{}]", int_to_code(n)).into_bytes()
}

/// `cap()`: a copy with the first byte upper-cased.
fn cap(mut s: Text) -> Text {
    if let Some(c) = s.first_mut() {
        *c = c.to_ascii_uppercase();
    }
    s
}

/// `display_name()`: the entity's name, or a stand-in for an unnamed one:
/// its kind, the name of the item a noble stands for, or its subkind.
#[must_use]
pub fn display_name(db: &Db, n: i32) -> Text {
    if !db.valid_box(n) {
        return Text::new();
    }
    let s = db.name(n);
    if !s.is_empty() {
        return s.to_vec();
    }
    match db.kind(n) {
        Kind::PLAYER => return b"Player".to_vec(),
        Kind::GATE => return b"Gate".to_vec(),
        Kind::POST => return b"Sign".to_vec(),
        _ => {}
    }
    // noble_item()
    let item = db.entity(n).char_.as_ref().map_or(0, |c| c.unit_item);
    if item != 0 {
        return cap(plural_item_name(db, item, 1));
    }
    cap(db.subkind(n).name().as_bytes().to_vec())
}

/// `plural_item_name()`: an item's name for a quantity of `qty`.
#[must_use]
pub fn plural_item_name(db: &Db, item: i32, qty: i32) -> Text {
    if qty == 1 {
        return display_name(db, item);
    }
    let plural = db
        .entity(item)
        .item
        .as_ref()
        .and_then(|i| i.plural_name.as_deref())
        .unwrap_or(b"");
    if plural.is_empty() {
        let mut msg = b"warning: plural name not set for item ".to_vec();
        msg.extend(box_code(item));
        msg.push(b'\n');
        eprint_bytes(&msg);
        return display_name(db, item);
    }
    plural.to_vec()
}

/// `box_name()`: the display name and code, such as `Forest~[aa01]`, or
/// just the code for an entity with no name.
#[must_use]
pub fn box_name(db: &Db, n: i32) -> Text {
    if n == GARRISON_MAGIC {
        return b"Garrison".to_vec();
    }
    if db.valid_box(n) {
        let mut s = display_name(db, n);
        if !s.is_empty() {
            s.push(b'~');
            s.extend(box_code(n));
            return s;
        }
    }
    box_code(n)
}

/// `just_name()`: the display name, or the code for an entity with no name.
#[must_use]
pub fn just_name(db: &Db, n: i32) -> Text {
    if n == GARRISON_MAGIC {
        return b"Garrison".to_vec();
    }
    if db.valid_box(n) {
        let s = display_name(db, n);
        if !s.is_empty() {
            return s;
        }
    }
    box_code(n)
}

/// `printf("%.3d", n)`: at least three digits, after any sign.
fn prec3(n: i32) -> String {
    if n < 0 {
        format!("-{:03}", n.unsigned_abs())
    } else {
        format!("{n:03}")
    }
}

/// `olympia/u.c:comma_num`: `n` with commas between the thousands.
///
/// Each group of a negative number carries its own sign, as in C.
#[must_use]
pub fn comma_num(n: i32) -> String {
    let further = n / 1_000_000_000;
    let n = n % 1_000_000_000;
    let millions = n / 1_000_000;
    let n = n % 1_000_000;
    let thousands = n / 1000;
    let ones = n % 1000;

    if further == 0 && millions == 0 && thousands == 0 {
        format!("{ones}")
    } else if further == 0 && millions == 0 {
        format!("{thousands},{}", prec3(ones))
    } else if further == 0 {
        format!("{millions},{},{}", prec3(thousands), prec3(ones))
    } else {
        format!(
            "{further},{},{},{}",
            prec3(millions),
            prec3(thousands),
            prec3(ones)
        )
    }
}

/// `num_s`: the numbers spelled out.
const NUM_S: [&str; 11] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

/// `olympia/u.c:nice_num`: `n` spelled out up to ten, else with commas.
#[must_use]
pub fn nice_num(n: i32) -> String {
    if (0..=10).contains(&n) {
        NUM_S[n as usize].to_owned()
    } else {
        comma_num(n)
    }
}

/// Writes `msg` to standard error as bytes, as C's `fprintf(stderr, ...)`
/// does with names that aren't UTF-8.
pub fn eprint_bytes(msg: &[u8]) {
    let _ = std::io::stderr().write_all(msg);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Subkind;

    #[test]
    fn code_to_int_inverts_int_to_code() {
        for n in [
            10101, 13451, 49999, 50000, 50013, 56759, 56760, 57081, 58759, 59000, 78999,
        ] {
            assert_eq!(code_to_int(int_to_code(n).as_bytes()), n, "{n}");
        }
        assert_eq!(code_to_int(b"AA01"), 10001);
        assert_eq!(code_to_int(b"1234x"), 1234);
        assert_eq!(code_to_int(b"aa1x"), 0);
        assert_eq!(code_to_int(b"-5"), 0);
        assert_eq!(code_to_int(b""), 0);
        assert_eq!(scode(b"[bs51"), 13451);
        // Only the opening bracket is skipped.
        assert_eq!(scode(b"[bs51]"), 0);
        assert_eq!(scode(b"(57081)"), 57081);
    }

    #[test]
    fn comma_num_and_nice_num_match_printf() {
        assert_eq!(comma_num(0), "0");
        assert_eq!(comma_num(1005), "1,005");
        assert_eq!(comma_num(12_345_678), "12,345,678");
        assert_eq!(comma_num(2_000_000_001), "2,000,000,001");
        // Each group keeps its sign, and %.3d pads after it.
        assert_eq!(comma_num(-1005), "-1,-005");
        assert_eq!(nice_num(7), "seven");
        assert_eq!(nice_num(11), "11");
        assert_eq!(nice_num(-1), "-1");
    }

    #[test]
    fn codes_follow_each_number_range() {
        assert_eq!(int_to_code(999), "999");
        assert_eq!(int_to_code(10000), "aa00");
        assert_eq!(int_to_code(10101), "ab01");
        assert_eq!(int_to_code(49999), "zz99");
        assert_eq!(int_to_code(50000), "aa0");
        assert_eq!(int_to_code(50013), "ab3");
        assert_eq!(int_to_code(56759), "zz9");
        assert_eq!(int_to_code(56760), "a00");
        assert_eq!(int_to_code(58759), "z99");
        assert_eq!(int_to_code(58760), "58760");
        assert_eq!(int_to_code(59000), "a000");
        assert_eq!(int_to_code(78999), "z999");
        assert_eq!(int_to_code(79000), "79000");
    }

    #[test]
    fn names_fall_back_to_kind_noble_item_or_subkind() {
        let mut db = Db::new();
        db.alloc_box(100, Kind::PLAYER, Subkind(52));
        db.alloc_box(10001, Kind::LOC, Subkind::FOREST);
        db.alloc_box(12, Kind::ITEM, Subkind::default());
        db.set_name(12, b"peasant");
        db.alloc_box(1001, Kind::CHAR, Subkind::default());
        db.p_char(1001).unit_item = 12;

        assert_eq!(box_name(&db, 100), b"Player~[100]");
        assert_eq!(box_name(&db, 10001), b"Forest~[aa01]");
        assert_eq!(just_name(&db, 1001), b"Peasant");
        assert_eq!(box_name(&db, 5), b"[5]");
        assert_eq!(box_name(&db, GARRISON_MAGIC), b"Garrison");
        db.set_name(100, b"Independent player");
        assert_eq!(box_name(&db, 100), b"Independent player~[100]");
    }
}
