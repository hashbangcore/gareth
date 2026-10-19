use std::env;
use std::fmt;
use std::fs;
use std::io;

use super::text::{looks_like_path, split_args};

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

/// Identifies the file paths mentioned in a string, in order of appearance.
///
/// Paths are returned exactly as written, duplicates are dropped, and the
/// filesystem is never touched: a returned path is a candidate, not a
/// guarantee. Use [`read_files`] to load them.
pub fn find_paths(text: &str) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for token in split_args(text) {
        if looks_like_path(&token) && !found.contains(&token) {
            found.push(token);
        }
    }
    found
}

/// Reads one or more files, failing on the first one that cannot be read.
///
/// Every returned attachment keeps the path as it was written, while the
/// filesystem is accessed through the expanded path.
pub fn read_files(paths: &[&str]) -> Result<Vec<Attachment>, ReadError> {
    let home = env::var("HOME").ok();
    paths
        .iter()
        .map(|path| read_one(path, home.as_deref()))
        .collect()
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
pub fn read_text_file(path: &str) -> Result<String, ReadError> {
    let home = env::var("HOME").ok();
    let bytes = read_bytes(path, home.as_deref())?;

    let sample = &bytes[..bytes.len().min(SNIFF_LEN)];
    if !is_text(sample) {
        return Err(ReadError::NotText(path.to_string()));
    }

    String::from_utf8(bytes).map_err(|_| ReadError::NotText(path.to_string()))
}

/// A value supplied by the user, resolved to the text to inject in a prompt.
#[derive(Debug, PartialEq, Eq)]
pub struct Value {
    /// Text to inject in the prompt.
    pub text: String,
    /// Path of the file the text came from, when it came from a file.
    pub path: Option<String>,
}

/// Resolves a value that may be either literal text or a path to a text file.
///
/// A value naming an existing regular file is read and must hold text. A
/// value that clearly looks like a path but does not exist is an error.
/// Anything else is taken as literal text.
pub fn resolve_value(value: &str) -> Result<Value, ReadError> {
    let home = env::var("HOME").ok();
    let expanded = expand_path(value, home.as_deref());

    match fs::metadata(&expanded) {
        Ok(meta) if meta.is_file() => Ok(Value {
            text: read_text_file(value)?,
            path: Some(value.to_string()),
        }),
        Ok(_) => Err(ReadError::NotAFile(value.to_string())),
        Err(source) if source.kind() == io::ErrorKind::NotFound => {
            if looks_like_path(value) {
                Err(ReadError::NotFound(value.to_string()))
            } else {
                Ok(Value {
                    text: value.to_string(),
                    path: None,
                })
            }
        }
        Err(source) => Err(ReadError::Io {
            path: value.to_string(),
            source,
        }),
    }
}

/// Reads a single path, reporting why it failed.
fn read_one(path: &str, home: Option<&str>) -> Result<Attachment, ReadError> {
    let bytes = read_bytes(path, home)?;

    let content = String::from_utf8(bytes).map_err(|source| ReadError::Io {
        path: path.to_string(),
        source: io::Error::new(io::ErrorKind::InvalidData, source),
    })?;

    Ok(Attachment {
        path: path.to_string(),
        content,
    })
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
    fn finds_absolute_relative_and_home_paths() {
        let found = find_paths("lee /etc/hosts y ./notas.txt y ~/pendientes.txt y nada");
        assert_eq!(found, vec!["/etc/hosts", "./notas.txt", "~/pendientes.txt"]);
    }

    #[test]
    fn finds_nothing_in_text_without_paths() {
        assert!(find_paths("hola mundo").is_empty());
        assert!(find_paths("").is_empty());
        // A bare word is not a path even with a slash-looking tail.
        assert!(find_paths("docs/").is_empty());
    }

    #[test]
    fn keeps_the_path_as_written() {
        assert_eq!(find_paths("mira ~/notas.txt"), vec!["~/notas.txt"]);
    }

    #[test]
    fn drops_duplicates_keeping_order() {
        let found = find_paths("./b.txt ./a.txt ./b.txt");
        assert_eq!(found, vec!["./b.txt", "./a.txt"]);
    }

    #[test]
    fn does_not_touch_the_filesystem() {
        // Identifies candidates only: a path that does not exist is still returned.
        assert_eq!(
            find_paths("/definitely/not/here.txt"),
            vec!["/definitely/not/here.txt"]
        );
    }

    #[test]
    fn reads_a_single_file() {
        let path = temp_path("single.txt");
        fs::write(&path, "contenido\n").unwrap();

        let read = read_files(&[path.to_str().unwrap()]).unwrap();

        assert_eq!(read.len(), 1);
        assert_eq!(read[0].content, "contenido\n");
        fs::remove_file(&path).unwrap();
    }

    #[test]
    fn reads_several_files_in_order() {
        let first = temp_path("first.txt");
        let second = temp_path("second.txt");
        fs::write(&first, "uno").unwrap();
        fs::write(&second, "dos").unwrap();

        let read = read_files(&[first.to_str().unwrap(), second.to_str().unwrap()]).unwrap();

        assert_eq!(read.len(), 2);
        assert_eq!(read[0].content, "uno");
        assert_eq!(read[1].content, "dos");
        fs::remove_file(&first).unwrap();
        fs::remove_file(&second).unwrap();
    }

    #[test]
    fn reports_a_missing_file() {
        let err = read_files(&["/definitely/not/here.txt"]).unwrap_err();
        assert!(matches!(err, ReadError::NotFound(_)));
        assert_eq!(err.to_string(), "/definitely/not/here.txt: no such file");
    }

    #[test]
    fn reports_a_directory() {
        let dir = std::env::temp_dir();
        let err = read_files(&[dir.to_str().unwrap()]).unwrap_err();
        assert!(matches!(err, ReadError::NotAFile(_)));
        assert!(err.to_string().ends_with(": not a regular file"));
    }

    #[test]
    fn fails_on_the_first_unreadable_file() {
        let good = temp_path("good.txt");
        fs::write(&good, "ok").unwrap();

        let err = read_files(&["/definitely/not/here.txt", good.to_str().unwrap()]).unwrap_err();
        assert!(matches!(err, ReadError::NotFound(_)));
        fs::remove_file(&good).unwrap();
    }

    #[test]
    fn expands_home_against_the_real_environment() {
        // Only meaningful when a home directory is set, which it always is here.
        let home = env::var("HOME").unwrap();
        assert_eq!(expand_path("~/x", Some(&home)), format!("{home}/x"));
        assert_eq!(expand_path("~/x", None), "~/x");
    }

    #[test]
    fn read_errors_expose_their_source() {
        use std::error::Error;
        let err = read_files(&[std::env::temp_dir().to_str().unwrap()]).unwrap_err();
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
        assert!(is_text(b"hola mundo\n"));
        assert!(is_text("ñandú 日本\n".as_bytes()));
    }

    #[test]
    fn rejects_a_nul_byte() {
        assert!(!is_text(b"\x00\x01\x02"));
        assert!(!is_text(b"texto normal\n\x00al final\n"));
    }

    #[test]
    fn rejects_invalid_utf8() {
        assert!(!is_text(&[0xff, 0xfe, 0xfd]));
        assert!(!is_text(b"texto normal\n\xff\n"));
    }

    #[test]
    fn accepts_a_multibyte_character_cut_at_the_end() {
        let mut sample = "ñ".as_bytes().to_vec();
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
        let value = resolve_value("es un refactor").unwrap();
        assert_eq!(value.text, "es un refactor");
        assert_eq!(value.path, None);
    }

    #[test]
    fn reads_an_existing_text_file() {
        let path = temp_path("resolve-text.txt");
        fs::write(&path, "contenido del fichero\n").unwrap();

        let value = resolve_value(path.to_str().unwrap()).unwrap();
        assert_eq!(value.text, "contenido del fichero\n");
        assert_eq!(value.path.as_deref(), Some(path.to_str().unwrap()));
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
        let value = resolve_value("gareth-no-existe-jamas.txt").unwrap();
        assert_eq!(value.text, "gareth-no-existe-jamas.txt");
        assert_eq!(value.path, None);
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
