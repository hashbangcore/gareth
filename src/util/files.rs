use std::env;
use std::fmt;
use std::fs;
use std::io;

use super::text::{looks_like_path, split_args};

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
    /// The file exists but could not be read.
    Io { path: String, source: io::Error },
}

impl fmt::Display for ReadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "{path}: no such file"),
            Self::NotAFile(path) => write!(f, "{path}: not a regular file"),
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

/// Reads a single path, reporting why it failed.
fn read_one(path: &str, home: Option<&str>) -> Result<Attachment, ReadError> {
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

    let content = fs::read_to_string(&expanded).map_err(|source| ReadError::Io {
        path: path.to_string(),
        source,
    })?;

    Ok(Attachment {
        path: path.to_string(),
        content,
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
}
