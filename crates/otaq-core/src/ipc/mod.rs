//! The local connection between the app, the CLI and the service (PLT-01, PLT-02): a Unix domain socket on
//! macOS and Linux, a named pipe on Windows. Both are locked to the current OS user (SVC-02, d0027).
//!
//! Callers name the endpoint by [`crate::data_dir::DataDir::socket_path`] on every platform. On Windows that path
//! is never created; it only names the pipe ([`pipe_name`]), so every data folder (and every test's temporary
//! one) gets its own pipe.

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as os;
#[cfg(windows)]
use windows as os;

pub use os::{
    ClientStream, Listener, ServerStream, UserId, connect, current_user, peer_pid, peer_user,
};
#[cfg(windows)]
pub use os::{dacl_sddl_as_written, pipe_name, pipe_sddl};

/// The pipe's short name for clients that take one (`otaq-1a2b…` of `\\.\pipe\otaq-1a2b…`) on
/// Windows; the socket path elsewhere.
pub fn endpoint_name(path: &std::path::Path) -> String {
    #[cfg(windows)]
    {
        pipe_name(path).trim_start_matches(r"\\.\pipe\").to_owned()
    }
    #[cfg(not(windows))]
    {
        path.display().to_string()
    }
}

/// Splits a stream into a read half and a write half that can be used from two tasks.
pub fn split<S>(stream: S) -> (tokio::io::ReadHalf<S>, tokio::io::WriteHalf<S>)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite,
{
    tokio::io::split(stream)
}
