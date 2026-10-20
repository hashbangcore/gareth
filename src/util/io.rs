//! Reading the standard input.

use std::io::{self, IsTerminal, Read};

/// Reads all content from a reader, failing on invalid UTF-8.
fn read_stdin(mut reader: impl Read) -> io::Result<String> {
    let mut input = String::new();
    reader.read_to_string(&mut input)?;
    Ok(input)
}

/// Reads all stdin content when input is piped, otherwise returns empty string.
///
/// # Errors
///
/// Returns an error when the standard input cannot be read or is not valid
/// UTF-8.
pub fn get_stdin() -> io::Result<String> {
    let stdin = io::stdin();
    if stdin.is_terminal() {
        return Ok(String::new());
    }
    read_stdin(stdin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn reads_piped_content() {
        assert_eq!(read_stdin(Cursor::new("hello\n")).unwrap(), "hello\n");
        assert_eq!(read_stdin(Cursor::new("")).unwrap(), "");
    }

    #[test]
    fn rejects_invalid_utf8_instead_of_panicking() {
        let err = read_stdin(Cursor::new([0xff, 0xfe])).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }
}
