use std::error::Error;
use std::fmt;
use std::io::Write;
use std::path::{Path, PathBuf};

use terminal_size::terminal_size;
use tokio::net::UnixDatagram;

use crate::util::env::env_var;

const DEFAULT_TRACE_SOCKET_PATH: &str = "/tmp/gareth.trace.sock";

/// Ways the trace server or its client can fail.
#[derive(Debug)]
pub enum TraceError {
    /// Creating, removing or binding the socket failed.
    Socket {
        action: &'static str,
        source: std::io::Error,
    },
    /// Printing an entry failed.
    Print { source: std::io::Error },
}

impl fmt::Display for TraceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Socket { action, source } => {
                write!(f, "could not {action} the trace socket ({source})")
            }
            Self::Print { source } => write!(f, "could not print a trace entry ({source})"),
        }
    }
}

impl Error for TraceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Socket { source, .. } | Self::Print { source } => Some(source),
        }
    }
}

/// Resolves the socket path from the environment.
fn resolve_trace_socket_path() -> PathBuf {
    socket_path_from(
        env_var("GARETH_TRACE_SOCKET").as_deref(),
        env_var("XDG_RUNTIME_DIR").as_deref(),
    )
}

/// Picks the socket path: explicit wins, then the runtime dir, then the default.
fn socket_path_from(explicit: Option<&str>, runtime_dir: Option<&str>) -> PathBuf {
    if let Some(value) = explicit {
        return PathBuf::from(value);
    }

    if let Some(value) = runtime_dir {
        return Path::new(value).join("gareth.trace.sock");
    }

    PathBuf::from(DEFAULT_TRACE_SOCKET_PATH)
}

fn separator_line() -> String {
    let width = terminal_size().map(|(w, _)| w.0 as usize).unwrap_or(80);
    let count = width.saturating_sub(1);
    ".".repeat(count) + "\n"
}

pub async fn run_trace_server() -> Result<(), TraceError> {
    let socket_path = resolve_trace_socket_path();

    // Replace old socket if it exists.
    if socket_path.exists() {
        std::fs::remove_file(&socket_path).map_err(|source| TraceError::Socket {
            action: "remove",
            source,
        })?;
    }

    let socket = UnixDatagram::bind(&socket_path).map_err(|source| TraceError::Socket {
        action: "bind",
        source,
    })?;
    let mut buf = vec![0u8; 64 * 1024];

    let mut counter: u64 = 0;
    let mut stdout = std::io::stdout();

    loop {
        let (len, _) = socket
            .recv_from(&mut buf)
            .await
            .map_err(|source| TraceError::Socket {
                action: "read from",
                source,
            })?;
        let message = String::from_utf8_lossy(&buf[..len]);
        let mut parts = message.splitn(2, '\n');
        let kind = parts.next().unwrap_or("");
        let payload = parts.next().unwrap_or("");

        counter = counter.wrapping_add(1);
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
        let header = format!("[id={counter} ts={ts}]\n{kind}\n");

        let mut entry = Vec::new();
        entry.extend_from_slice(b"\n\n");
        entry.extend_from_slice(separator_line().as_bytes());
        entry.extend_from_slice(b"\n\n");
        entry.extend_from_slice(header.as_bytes());
        entry.extend_from_slice(payload.as_bytes());
        if !payload.ends_with('\n') {
            entry.push(b'\n');
        }
        stdout
            .write_all(&entry)
            .and_then(|()| stdout.flush())
            .map_err(|source| TraceError::Print { source })?;
    }
}

pub async fn send_trace(kind: &str, payload: &str) {
    let socket_path = resolve_trace_socket_path();
    // Nothing is listening when the socket is absent, so skip the syscalls.
    if !socket_path.exists() {
        return;
    }

    let Ok(socket) = UnixDatagram::unbound() else {
        return;
    };

    let message = format!("{}\n{}", kind, payload);
    let _ = socket.send_to(message.as_bytes(), socket_path).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_socket_wins_over_the_runtime_dir() {
        assert_eq!(
            socket_path_from(Some("/run/gareth.sock"), Some("/run/user/1000")),
            PathBuf::from("/run/gareth.sock")
        );
    }

    #[test]
    fn falls_back_to_the_runtime_dir() {
        assert_eq!(
            socket_path_from(None, Some("/run/user/1000")),
            PathBuf::from("/run/user/1000/gareth.trace.sock")
        );
    }

    #[test]
    fn falls_back_to_the_default_socket() {
        assert_eq!(
            socket_path_from(None, None),
            PathBuf::from(DEFAULT_TRACE_SOCKET_PATH)
        );
    }

    #[test]
    fn socket_errors_say_what_failed() {
        let err = TraceError::Socket {
            action: "bind",
            source: std::io::Error::from(std::io::ErrorKind::AddrInUse),
        };
        assert!(err.to_string().contains("bind"));
        assert!(err.to_string().contains("could not bind the trace socket"));
    }
}
