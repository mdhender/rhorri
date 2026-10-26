// Copyright (c) 2026 Michael D Henderson.
// SPDX-License-Identifier: AGPL-3.0-or-later

//! Operating-system error text as the C engine prints it.

use std::io;

/// The `strerror()` text for an I/O error, as `perror("")` prints it.
///
/// `io::Error` displays an OS error as `<strerror> (os error <n>)`; the C
/// engine prints only the `strerror()` part.
pub(crate) fn strerror(err: &io::Error) -> String {
    let text = err.to_string();
    match err.raw_os_error() {
        Some(code) => text
            .strip_suffix(&format!(" (os error {code})"))
            .map_or_else(|| text.clone(), str::to_owned),
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_the_os_error_suffix() {
        let err = io::Error::from_raw_os_error(2);
        assert_eq!(strerror(&err), "No such file or directory");
    }

    #[test]
    fn keeps_non_os_errors_whole() {
        let err = io::Error::other("short write");
        assert_eq!(strerror(&err), "short write");
    }
}
