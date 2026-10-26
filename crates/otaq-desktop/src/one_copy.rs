//! Only one copy of the app runs per data folder (d0139 "Only one copy of Otaq runs", TRY-01). The first copy takes
//! `app.lock` in the data folder and listens on `app.sock` (a pipe named after it on Windows), both owner-only like
//! the service's. A second start (the `otaq` command, the app's icon, a notification launching the app) finds the
//! lock held, hands its `OTAQ_OPEN` value to the running copy over the socket and exits; the running copy shows its
//! window and opens the target.
//!
//! Keyed by the data folder, not the bundle identifier: a copy with a throwaway `OTAQ_HOME` never reaches the one on
//! `~/.otaq`, and the two run side by side.
//!
//! The hand-over is one line each way: the second start sends its `OTAQ_OPEN` value (empty for none), the running
//! copy answers [`SHOWN`] once it has passed the value on.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

use otaq_core::data_dir::DataDir;
use otaq_core::ipc;
use otaq_core::service::lock::{LockError, ServiceLock};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

/// The running copy's answer to a hand-over.
const SHOWN: &str = "shown";
/// The longest hand-over line read; an open value is a few dozen bytes.
const MAX_LINE: u64 = 4096;
/// How long a second start keeps trying to reach a copy that holds the lock: it may have just taken it and not be
/// listening yet.
const REACH_FOR: Duration = Duration::from_secs(5);
const RETRY_EVERY: Duration = Duration::from_millis(50);
/// How long either side waits for the other's line.
const LINE_TIMEOUT: Duration = Duration::from_secs(5);

/// What a start found.
pub enum Claim {
    /// No other copy runs for this data folder: this one is it.
    First(FirstCopy),
    /// Another copy runs and took this start's open target: this process exits.
    HandedOver,
}

#[derive(Debug)]
pub enum ClaimError {
    /// Another copy holds the lock but didn't take the hand-over.
    NoAnswer {
        pid: Option<u32>,
        error: io::Error,
    },
    Io(io::Error),
}

impl std::fmt::Display for ClaimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let product = otaq_core::naming::PRODUCT_NAME;
        match self {
            Self::NoAnswer {
                pid: Some(pid),
                error,
            } => write!(
                f,
                "{product} is already running (process {pid}) but didn't answer: {error}"
            ),
            Self::NoAnswer { pid: None, error } => {
                write!(f, "{product} is already running but didn't answer: {error}")
            }
            Self::Io(e) => write!(f, "can't check whether {product} is already running: {e}"),
        }
    }
}

/// The copy that runs: it holds the lock until it is dropped (the OS drops it at exit, even after a crash).
pub struct FirstCopy {
    _lock: ServiceLock,
    /// `None` when the socket couldn't be made (a data folder whose path is too long for a Unix socket): the copy runs
    /// and still keeps others out, but a second start can't hand over and says so.
    listener: Option<ipc::Listener>,
}

/// Where the lock and the socket are for `data`.
pub struct Paths {
    lock: PathBuf,
    socket: PathBuf,
}

impl Paths {
    pub fn of(data: &DataDir) -> Self {
        Self {
            lock: data.app_lock_path(),
            socket: data.app_socket_path(),
        }
    }
}

/// Becomes the one copy for the data folder, or hands `open` (this start's `OTAQ_OPEN` value) to the copy that is.
/// Call inside a tokio runtime; the folder is created when missing.
pub async fn claim(paths: &Paths, open: Option<&str>) -> Result<Claim, ClaimError> {
    claim_within(paths, open, REACH_FOR).await
}

async fn claim_within(
    paths: &Paths,
    open: Option<&str>,
    reach_for: Duration,
) -> Result<Claim, ClaimError> {
    if let Some(folder) = paths.lock.parent() {
        DataDir::ensure_dir(folder).map_err(ClaimError::Io)?;
    }
    match ServiceLock::acquire(&paths.lock) {
        Ok(lock) => {
            let listener = ipc::Listener::bind(&paths.socket)
                .inspect_err(|e| {
                    eprintln!(
                        "a second start can't reach this app ({}): {e}",
                        paths.socket.display()
                    );
                })
                .ok();
            Ok(Claim::First(FirstCopy {
                _lock: lock,
                listener,
            }))
        }
        Err(LockError::Held { pid }) => {
            match hand_over(&paths.socket, open.unwrap_or(""), reach_for).await {
                Ok(()) => Ok(Claim::HandedOver),
                Err(error) => Err(ClaimError::NoAnswer { pid, error }),
            }
        }
        Err(LockError::Io(e)) => Err(ClaimError::Io(e)),
    }
}

/// Sends `open` to the running copy and waits for its answer.
async fn hand_over(socket: &Path, open: &str, reach_for: Duration) -> io::Result<()> {
    let started = tokio::time::Instant::now();
    let mut stream = loop {
        match ipc::connect(socket).await {
            Ok(stream) => break stream,
            Err(e) if started.elapsed() >= reach_for => return Err(e),
            Err(_) => tokio::time::sleep(RETRY_EVERY).await,
        }
    };
    // One line: a value with a line break in it would end early, so it opens nothing (as `OTAQ_OPEN` would).
    let open = open.lines().next().unwrap_or("");
    stream.write_all(format!("{open}\n").as_bytes()).await?;
    stream.flush().await?;
    let mut answer = String::new();
    timeout(BufReader::new(stream).read_line(&mut answer)).await?;
    if answer.trim_end() == SHOWN {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unexpected answer {answer:?}"),
        ))
    }
}

impl FirstCopy {
    /// Takes every later start's hand-over until the app ends: `on_start` gets its `OTAQ_OPEN` value (`None` for a
    /// start without one) and shows the window. Only connections from this OS user are read.
    pub fn serve(
        self,
        runtime: &tokio::runtime::Handle,
        on_start: impl Fn(Option<String>) + Send + Sync + 'static,
    ) {
        let on_start = std::sync::Arc::new(on_start);
        let FirstCopy { _lock, listener } = self;
        let Some(mut listener) = listener else {
            // The lock lives as long as the app.
            std::mem::forget(_lock);
            return;
        };
        runtime.spawn(async move {
            let _lock = _lock;
            loop {
                let stream = match listener.accept().await {
                    Ok(stream) => stream,
                    Err(e) => {
                        eprintln!("a second start couldn't reach this app: {e}");
                        continue;
                    }
                };
                if !ipc::peer_user(&stream).is_ok_and(|user| user == ipc::current_user()) {
                    continue;
                }
                let on_start = on_start.clone();
                tokio::spawn(async move {
                    if let Err(e) = take_hand_over(stream, &*on_start).await {
                        eprintln!("a second start's hand-over failed: {e}");
                    }
                });
            }
        });
    }
}

async fn take_hand_over(
    stream: ipc::ServerStream,
    on_start: &(dyn Fn(Option<String>) + Send + Sync),
) -> io::Result<()> {
    let (read, mut write) = ipc::split(stream);
    let mut line = String::new();
    timeout(BufReader::new(read.take(MAX_LINE)).read_line(&mut line)).await?;
    if !line.ends_with('\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "no complete line",
        ));
    }
    on_start(open_value(&line));
    write.write_all(format!("{SHOWN}\n").as_bytes()).await?;
    write.flush().await
}

/// The hand-over line's `OTAQ_OPEN` value: `None` when it carries none.
fn open_value(line: &str) -> Option<String> {
    let value = line.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

async fn timeout<T>(read: impl Future<Output = io::Result<T>>) -> io::Result<T> {
    tokio::time::timeout(LINE_TIMEOUT, read)
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "no answer in time"))?
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn paths(root: &Path) -> Paths {
        Paths::of(&DataDir::new(root))
    }

    /// Serves `first`, collecting what each later start handed over.
    fn serve(first: FirstCopy) -> Arc<Mutex<Vec<Option<String>>>> {
        let got = Arc::new(Mutex::new(Vec::new()));
        let into = got.clone();
        first.serve(&tokio::runtime::Handle::current(), move |open| {
            into.lock().unwrap().push(open)
        });
        got
    }

    #[tokio::test]
    async fn a_second_start_hands_its_target_over_and_goes() {
        let home = tempfile::tempdir().unwrap();
        let Claim::First(first) = claim(&paths(home.path()), None).await.unwrap() else {
            panic!("the first start runs");
        };
        let got = serve(first);

        let second = claim(&paths(home.path()), Some("SHOP-22")).await.unwrap();
        assert!(matches!(second, Claim::HandedOver));
        let third = claim(&paths(home.path()), None).await.unwrap();
        assert!(matches!(third, Claim::HandedOver));
        assert_eq!(*got.lock().unwrap(), [Some("SHOP-22".to_owned()), None]);
    }

    #[tokio::test]
    async fn each_data_folder_has_its_own_copy() {
        let (real, throwaway) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let Claim::First(first) = claim(&paths(real.path()), None).await.unwrap() else {
            panic!("the first folder's copy runs");
        };
        let got = serve(first);
        let Claim::First(_other) = claim(&paths(throwaway.path()), Some("WEB-7"))
            .await
            .unwrap()
        else {
            panic!("another folder starts its own copy");
        };
        assert!(
            got.lock().unwrap().is_empty(),
            "nothing reached the first copy"
        );
    }

    #[tokio::test]
    async fn once_the_copy_ends_the_next_start_runs() {
        let home = tempfile::tempdir().unwrap();
        let Claim::First(first) = claim(&paths(home.path()), None).await.unwrap() else {
            panic!("the first start runs");
        };
        drop(first);
        // The socket file stays behind, as after a crash; the next copy replaces it.
        let Claim::First(again) = claim(&paths(home.path()), None).await.unwrap() else {
            panic!("the next start runs");
        };
        let got = serve(again);
        assert!(matches!(
            claim(&paths(home.path()), Some("needs_you")).await.unwrap(),
            Claim::HandedOver
        ));
        assert_eq!(*got.lock().unwrap(), [Some("needs_you".to_owned())]);
    }

    #[tokio::test]
    async fn a_copy_that_holds_the_lock_but_never_answers_is_an_error() {
        let home = tempfile::tempdir().unwrap();
        let p = paths(home.path());
        // Holds the lock, never listens.
        let Claim::First(first) = claim(&p, None).await.unwrap() else {
            panic!("the first start runs");
        };
        let FirstCopy { _lock, listener } = first;
        drop(listener);
        let _ = std::fs::remove_file(&p.socket);
        let _first = FirstCopy {
            _lock,
            listener: None,
        };
        match claim_within(&p, Some("SHOP-22"), Duration::from_millis(200)).await {
            Err(ClaimError::NoAnswer { pid, .. }) => assert_eq!(pid, Some(std::process::id())),
            Err(e) => panic!("{e}"),
            Ok(_) => panic!("a start without an answer doesn't count as handed over"),
        }
    }

    /// A Unix socket path has a short limit (104 bytes on macOS); a data folder deeper than that still gets one copy.
    #[cfg(unix)]
    #[tokio::test]
    async fn a_folder_too_deep_for_a_socket_still_runs_one_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths(&tmp.path().join("d".repeat(120)));
        let Claim::First(first) = claim(&p, None).await.unwrap() else {
            panic!("the first start runs");
        };
        assert!(first.listener.is_none());
        first.serve(&tokio::runtime::Handle::current(), |_| {});
        assert!(matches!(
            claim_within(&p, None, Duration::from_millis(100)).await,
            Err(ClaimError::NoAnswer { .. })
        ));
    }

    #[test]
    fn the_line_carries_the_value_or_nothing() {
        assert_eq!(open_value("SHOP-22\n"), Some("SHOP-22".to_owned()));
        assert_eq!(open_value("\n"), None);
        assert_eq!(open_value("  \r\n"), None);
    }
}
