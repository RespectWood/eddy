//! Output of `--explain` (CLI only).

/// `s` without control characters, for printing to a terminal. Titles and names come
/// from API responses; an escape sequence in one could otherwise drive the terminal
/// (change its title, write to the clipboard, …).
pub(crate) fn printable(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() { '\u{fffd}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_control_characters() {
        assert_eq!(
            printable("Yeah! \u{1b}]52;c;aGk=\u{7}"),
            "Yeah! �]52;c;aGk=�"
        );
        assert_eq!(printable("Sigur Rós\nx"), "Sigur Rós�x");
        assert_eq!(
            printable("Never Gonna Give You Up"),
            "Never Gonna Give You Up"
        );
    }
}
