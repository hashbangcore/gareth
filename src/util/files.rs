//! Reading files and deciding whether they hold text.

use std::env;
use std::fmt;
use std::fs;
use std::io;

use super::text::looks_like_path;

/// Number of bytes inspected to decide whether a file holds text.
const SNIFF_LEN: usize = 8192;

/// Percentage of control characters above which the content is binary.
const MAX_CONTROL_PERCENT: usize = 30;

/// A file read from disk, keeping the path as the user wrote it.
#[derive(Debug)]
pub struct Attachment {
    /// Path as written by the user (not expanded).
    pub path: String,
    pub content: String,
}

/// Why a file could not be read.
#[derive(Debug)]
pub enum ReadError {
    /// The path does not exist.
    NotFound(String),
    /// The path exists but is not a regular file.
    NotAFile(String),
    /// The path exists but does not hold text.
    NotText(String),
    /// The file exists but could not be read.
    Io { path: String, source: io::Error },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "{path}: no such file"),
            Self::NotAFile(path) => write!(f, "{path}: not a regular file"),
            Self::NotText(path) => write!(f, "{path}: not a text file"),
            Self::Io { path, source } => write!(f, "{path}: {source}"),
        }
    }
}

impl std::error::Error for ReadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Expands a leading `~/` into the given home directory.
pub fn expand_path(path: &str, home: Option<&str>) -> String {
    match (path.strip_prefix("~/"), home) {
        (Some(rest), Some(home)) => format!("{}/{}", home, rest),
        _ => path.to_string(),
    }
}

/// Decides whether a byte slice holds text.
///
/// The verdict comes from the content and never from the extension: a NUL
/// byte, invalid UTF-8 or a high ratio of control characters all mean the
/// content is binary.
pub fn is_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) {
        return false;
    }

    match std::str::from_utf8(bytes) {
        Ok(text) => {
            let total = text.chars().count();
            if total == 0 {
                return true;
            }
            let control = text
                .chars()
                .filter(|c| c.is_control() && !matches!(c, '\t' | '\n' | '\r'))
                .count();
            control * 100 <= total * MAX_CONTROL_PERCENT
        }
        // A multibyte character cut by the end of the sample is not binary data.
        Err(err) if err.error_len().is_none() => true,
        Err(_) => false,
    }
}

/// Reads a file that must hold text, failing when it does not.
///
/// # Errors
///
/// Returns [`ReadError::NotFound`] when there is no file, [`ReadError::NotAFile`]
/// when it is not a regular file and [`ReadError::NotText`] when it does not
/// hold text.
pub fn read_text_file(path: &str) -> Result<String, ReadError> {
    let home = env::var("HOME").ok();
    let bytes = read_bytes(path, home.as_deref())?;

    let sample = &bytes[..bytes.len().min(SNIFF_LEN)];
    if !is_text(sample) {
        return Err(ReadError::NotText(path.to_string()));
    }

    String::from_utf8(bytes).map_err(|_| ReadError::NotText(path.to_string()))
}

/// Resolves a value that may be either literal text or a path to a text file.
///
/// A value naming an existing regular file is read, and anything else is
/// taken as literal text.
///
/// # Errors
///
/// Returns [`ReadError::NotFound`] when the value looks like a path but no
/// file is there, [`ReadError::NotAFile`] when it is not a regular file and
/// [`ReadError::NotText`] when the file does not hold text.
pub fn resolve_value(value: &str) -> Result<String, ReadError> {
    let home = env::var("HOME").ok();
    let expanded = expand_path(value, home.as_deref());

    match fs::metadata(&expanded) {
        Ok(meta) if meta.is_file() => read_text_file(value),
        Ok(_) => Err(ReadError::NotAFile(value.to_string())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            if looks_like_path(value) {
                Err(ReadError::NotFound(value.to_string()))
            } else {
                Ok(value.to_string())
            }
        }
        Err(source) => Err(ReadError::Io {
            path: value.to_string(),
            source,
        }),
    }
}

/// Reads the bytes of a regular file, reporting why it failed.
fn read_bytes(path: &str, home: Option<&str>) -> Result<Vec<u8>, ReadError> {
    let expanded = expand_path(path, home);

    let meta = fs::metadata(&expanded).map_err(|source| match source.kind() {
        io::ErrorKind::NotFound => ReadError::NotFound(path.to_string()),
        _ => ReadError::Io {
            path: path.to_string(),
            source,
        },
    })?;
    if !meta.is_file() {
        return Err(ReadError::NotAFile(path.to_string()));
    }

    fs::read(&expanded).map_err(|source| ReadError::Io {
        path: path.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("gareth-files-test-{}-{name}", std::process::id()))
    }

    #[test]
    fn expands_home_against_the_real_environment() {
        // A home directory is always set in a test environment.
        let home = env::var("HOME").unwrap();
        assert_eq!(expand_path("~/x", Some(&home)), format!("{home}/x"));
        assert_eq!(expand_path("~/x", None), "~/x");
    }

    #[test]
    fn read_errors_expose_their_source() {
        use std::error::Error;
        let err = resolve_value(std::env::temp_dir().to_str().unwrap()).unwrap_err();
        // NotAFile carries no underlying error.
        assert!(err.source().is_none());

        let err = ReadError::Io {
            path: "x".to_string(),
            source: io::Error::new(io::ErrorKind::PermissionDenied, "nope"),
        };
        assert!(err.source().is_some());
    }

    #[test]
    fn accepts_empty_plain_and_unicode_text() {
        assert!(is_text(b""));
        assert!(is_text(b"hello world\n"));
        assert!(is_text("naïve café €\n".as_bytes()));
    }

    #[test]
    fn rejects_a_nul_byte() {
        assert!(!is_text(b"\x00\x01\x02"));
        assert!(!is_text(b"plain text\n\x00at the end\n"));
    }

    #[test]
    fn rejects_invalid_utf8() {
        assert!(!is_text(&[0xff, 0xfe, 0xfd]));
        assert!(!is_text(b"plain text\n\xff\n"));
    }

    #[test]
    fn accepts_a_multibyte_character_cut_at_the_end() {
        let mut sample = "€".as_bytes().to_vec();
        sample.pop();
        assert!(is_text(&sample));
    }

    #[test]
    fn rejects_content_made_of_control_characters() {
        let bytes: Vec<u8> = (0..100u8).map(|_| 0x01).collect();
        assert!(!is_text(&bytes));
    }

    #[test]
    fn allows_tabs_and_newlines() {
        let text = "linea\tcon\ttabs\ny\r\nsaltos\n".repeat(20);
        assert!(is_text(text.as_bytes()));
    }

    #[test]
    fn takes_a_plain_value_as_literal_text() {
        assert_eq!(resolve_value("a refactor").unwrap(), "a refactor");
    }

    #[test]
    fn reads_an_existing_text_file() {
        let path = temp_path("resolve-text.txt");
        fs::write(&path, "file content\n").unwrap();

        let text = resolve_value(path.to_str().unwrap()).unwrap();

        assert_eq!(text, "file content\n");
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn refuses_a_binary_file() {
        let path = temp_path("resolve-binary.bin");
        fs::write(&path, [0x00u8, 0x01, 0x02]).unwrap();

        let err = resolve_value(path.to_str().unwrap()).unwrap_err();

        assert!(matches!(err, ReadError::NotText(_)));
        assert!(err.to_string().ends_with(": not a text file"));
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reports_a_missing_path_that_looks_like_one() {
        let err = resolve_value("./gareth-no-existe-jamas.txt").unwrap_err();
        assert!(matches!(err, ReadError::NotFound(_)));
        assert_eq!(
            err.to_string(),
            "./gareth-no-existe-jamas.txt: no such file"
        );
    }

    #[test]
    fn takes_a_missing_bare_name_as_literal_text() {
        assert_eq!(
            resolve_value("gareth-no-existe-jamas.txt").unwrap(),
            "gareth-no-existe-jamas.txt"
        );
    }

    #[test]
    fn refuses_a_directory() {
        let dir = std::env::temp_dir();
        let err = resolve_value(dir.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, ReadError::NotAFile(_)));
    }

    #[test]
    fn reads_a_text_file_without_touching_the_extension() {
        let path = temp_path("resolve-no-extension");
        fs::write(&path, "sin extension\n").unwrap();

        let content = read_text_file(path.to_str().unwrap()).unwrap();

        assert_eq!(content, "sin extension\n");
        fs::remove_file(&path).unwrap();
    }
}
