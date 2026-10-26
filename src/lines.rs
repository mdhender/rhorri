// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Line readers for the data files (`olympia/z.c`).

use std::fs;
use std::path::Path;

use crate::cstr::strip_ew;
use crate::os::strerror;

/// `LEN` from `z.h`: `readlin()` keeps at most `LEN - 1` bytes of a line.
const LEN: usize = 2048;

/// Reads a file a line at a time, as `readfile()`/`readlin()` do.
#[derive(Debug)]
pub struct LineReader {
    data: Vec<u8>,
    pos: usize,
}

impl LineReader {
    /// Opens `path` (`readfile()`), or prints `can't open <path>: <strerror>`
    /// to standard error and returns `None`.
    #[must_use]
    pub fn open(path: &Path) -> Option<Self> {
        match fs::read(path) {
            Ok(data) => Some(Self { data, pos: 0 }),
            Err(err) => {
                eprintln!("can't open {}: {}", path.display(), strerror(&err));
                None
            }
        }
    }

    /// The next line without its newline, cut to `LEN - 1` bytes
    /// (`readlin()`), or `None` at the end of the file.
    pub fn readlin(&mut self) -> Option<Vec<u8>> {
        if self.pos >= self.data.len() {
            return None;
        }
        let rest = &self.data[self.pos..];
        let (line, used) = match rest.iter().position(|&c| c == b'\n') {
            Some(end) => (&rest[..end], end + 1),
            None => (rest, rest.len()),
        };
        self.pos += used;
        Some(line[..line.len().min(LEN - 1)].to_vec())
    }

    /// The next line as `readlin_ew()` returns it.
    pub fn readlin_ew(&mut self) -> Option<Vec<u8>> {
        self.readlin().map(|line| strip_ew(&line))
    }
}

/// The lines of a file as `getlin_ew()` returns them, or the open error.
///
/// `getlin()` keeps whole lines, but cuts a last line with no newline to
/// 1023 bytes.
///
/// # Errors
///
/// Returns the error if the file cannot be read.
pub fn getlin_ew_lines(path: &Path) -> std::io::Result<Vec<Vec<u8>>> {
    let data = fs::read(path)?;
    let mut lines = Vec::new();
    let mut rest = &data[..];
    while !rest.is_empty() {
        match rest.iter().position(|&c| c == b'\n') {
            Some(end) => {
                lines.push(strip_ew(&rest[..end]));
                rest = &rest[end + 1..];
            }
            None => {
                lines.push(strip_ew(&rest[..rest.len().min(1023)]));
                rest = &[];
            }
        }
    }
    Ok(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reader(data: &[u8]) -> LineReader {
        LineReader {
            data: data.to_vec(),
            pos: 0,
        }
    }

    #[test]
    fn readlin_splits_lines_and_keeps_a_final_partial_line() {
        let mut r = reader(b"a\n\nb");
        assert_eq!(r.readlin().as_deref(), Some(&b"a"[..]));
        assert_eq!(r.readlin().as_deref(), Some(&b""[..]));
        assert_eq!(r.readlin().as_deref(), Some(&b"b"[..]));
        assert_eq!(r.readlin(), None);
    }

    #[test]
    fn readlin_cuts_long_lines() {
        let long = vec![b'x'; 3000];
        let mut r = reader(&long);
        assert_eq!(r.readlin().map(|l| l.len()), Some(LEN - 1));
    }
}
