//! `otaq attach <KEY>`: a ticket's live session in any terminal (SES-05, d0022).
//!
//! The local terminal goes into raw mode; the stored scrollback is printed, then live output follows while
//! keystrokes and window resizes are relayed through the service (the same relay as the app, so the unsent-input
//! flag stays right). **Detach with Ctrl+]**: the terminal is restored and the session keeps running. Ctrl+] was
//! chosen because Claude Code doesn't use it, unlike Esc and Ctrl+C (implementation choice).
//!
//! The key may be `SHOP-18`, a bare number (`18`, `40.1`) in the current project (`-P`, the project of
//! `$OTAQ_TICKET`, or the project whose repository or worktree holds the current folder), or omitted to use
//! `$OTAQ_TICKET`.

use std::io::{Read, Write};

use otaq_core::client::{Client, ClientError};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::TicketKey;
use otaq_core::protocol::{
    ErrorBody, ErrorKind, Event, FocusParams, InputParams, Reply, Request, ResizeParams,
    SessionAttach, SessionIdParams, SessionTarget, decode_bytes, encode_bytes,
};
use otaq_core::session::{SessionState, TerminalSize};
use tokio::sync::{broadcast, mpsc};

use crate::cli::GlobalArgs;

#[cfg(unix)]
#[path = "attach/unix.rs"]
mod terminal;
#[cfg(windows)]
#[path = "attach/windows.rs"]
mod terminal;

use terminal::{RawTerminal, WindowChanges};

/// Reports the attached terminal's focus; a failure only costs the focused-terminal rule, never the attach.
async fn focus(client: &Client, session: &str, focused: bool) {
    let request = Request::SessionFocus(FocusParams {
        session: session.to_owned(),
        focused,
    });
    if let Err(e) = client.request(request).await {
        tracing::debug!(error = %e, "can't report focus");
    }
}

/// Ctrl+]: leaves the session running and returns to the shell.
pub const DETACH_KEY: u8 = 0x1d;

pub fn run(
    key: Option<String>,
    stage: Option<String>,
    agent: Option<String>,
    global: &GlobalArgs,
) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .map_err(|e| internal(e.to_string()))?;
    let target = Target {
        stage: stage.as_deref(),
        agent: agent.as_deref(),
    };
    runtime.block_on(attach(&data, key, target, global))
}

/// Which of the ticket's sessions: its agent's, or with `stage` the session of the agent running that stage
/// (`--stage`, FLW-20), with `agent` one check agent's of several (`--agent`, d0130 row 17).
#[derive(Clone, Copy)]
struct Target<'a> {
    stage: Option<&'a str>,
    agent: Option<&'a str>,
}

async fn attach(
    data: &DataDir,
    key: Option<String>,
    stage: Target<'_>,
    global: &GlobalArgs,
) -> Result<(), ErrorBody> {
    let client = crate::service::connect(data, global).await?;
    let key = crate::target::resolve_key(&client, data, key, global).await?;
    let mut events = client.subscribe().await.map_err(client_error)?;
    let attached = request_attach(&client, &key, stage).await?;
    let session = attached.session.id.clone();
    // Attached means looking at it: no turquoise, Needs you item or notification for this ticket meanwhile (ATT-03).
    // Disconnecting releases the focus too, so a killed `attach` never leaves it held.
    focus(&client, &session, true).await;

    let terminal = RawTerminal::enter();
    let mut stdout = std::io::stdout();
    let mut end = write_scrollback(&mut stdout, &attached)?;
    if let Some(size) = terminal.as_ref().and_then(|t| t.size()) {
        resize(&client, &session, size).await;
    }

    let (keys, mut typed) = mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        #[cfg(windows)]
        if terminal::read_console_keys(&keys) {
            return;
        }
        read_stdin(&keys)
    });
    let mut winch = WindowChanges::new().map_err(|e| internal(e.to_string()))?;

    let ended = loop {
        tokio::select! {
            chunk = typed.recv() => {
                let Some(chunk) = chunk else { break None };
                let (before, detach) = split_detach(&chunk);
                if !before.is_empty() {
                    let input = Request::SessionInput(InputParams {
                        session: session.clone(),
                        data: encode_bytes(before),
                        reply: false,
                        if_empty: false,
                    });
                    if let Err(e) = client.request(input).await {
                        break Some(Err(client_error(e)));
                    }
                }
                if detach {
                    focus(&client, &session, false).await;
                    let _ = client
                        .request(Request::SessionDetach(SessionIdParams { session: session.clone() }))
                        .await;
                    break None;
                }
            }
            event = events.recv() => match event {
                Ok(Event::SessionOutput { session: s, offset, data }) if s == session => {
                    let bytes = decode_bytes(&data).unwrap_or_default();
                    // Skip anything the scrollback already showed.
                    let skip = usize::try_from(end.saturating_sub(offset)).unwrap_or(usize::MAX);
                    if skip < bytes.len() {
                        write_out(&mut stdout, &bytes[skip..])?;
                    }
                    end = end.max(offset + bytes.len() as u64);
                }
                Ok(Event::SessionState { session: info }) if info.id == session && info.state == SessionState::Ended => {
                    break Some(Ok(info.end_reason.unwrap_or_default()));
                }
                Ok(_) => {}
                // Output was dropped: redraw from the service's scrollback.
                Err(broadcast::error::RecvError::Lagged(_)) => {
                    let again = request_attach(&client, &key, stage).await?;
                    write_out(&mut stdout, b"\x1b[2J\x1b[H")?;
                    end = write_scrollback(&mut stdout, &again)?;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break Some(Err(ErrorBody::new(ErrorKind::Disconnected, "lost the connection to the background service")));
                }
            },
            _ = winch.next() => {
                if let Some(size) = terminal.as_ref().and_then(|t| t.size()) {
                    resize(&client, &session, size).await;
                }
            }
        }
    };
    drop(terminal);
    match ended {
        None => {
            eprintln!("\r\n[detached from {key}; its session keeps running]");
            Ok(())
        }
        Some(Ok(reason)) => {
            eprintln!("\r\n[{key}'s session ended: {reason}]");
            Ok(())
        }
        Some(Err(e)) => Err(e),
    }
}

/// The ticket's session `stage` names ([`Target`]).
async fn request_attach(
    client: &Client,
    key: &TicketKey,
    stage: Target<'_>,
) -> Result<SessionAttach, ErrorBody> {
    let no_session = || ErrorBody::new(ErrorKind::NotFound, format!("{key} has no live session"));
    let reply = client
        .request(Request::SessionAttach(
            SessionTarget {
                session: None,
                key: Some(key.to_string()),
                stage: stage.stage.map(str::to_owned),
                agent: stage.agent.map(str::to_owned),
                ..Default::default()
            }
            .into(),
        ))
        .await;
    match reply {
        Ok(Reply::SessionAttached(attached)) if attached.session.is_running() => Ok(*attached),
        Ok(Reply::SessionAttached(_)) => Err(no_session()),
        Ok(other) => Err(internal(format!("unexpected answer {other:?}"))),
        Err(ClientError::Service(e))
            if e.kind == ErrorKind::NotFound && e.message.contains("no live") =>
        {
            Err(no_session())
        }
        Err(e) => Err(client_error(e)),
    }
}

fn write_scrollback(out: &mut impl Write, attached: &SessionAttach) -> Result<u64, ErrorBody> {
    let bytes = decode_bytes(&attached.scrollback).unwrap_or_default();
    write_out(out, &bytes)?;
    Ok(attached.end)
}

fn write_out(out: &mut impl Write, bytes: &[u8]) -> Result<(), ErrorBody> {
    out.write_all(bytes)
        .and_then(|()| out.flush())
        .map_err(|e| internal(format!("can't write to the terminal: {e}")))
}

async fn resize(client: &Client, session: &str, size: TerminalSize) {
    let _ = client
        .request(Request::SessionResize(ResizeParams {
            session: session.to_owned(),
            size,
        }))
        .await;
}

/// The bytes before the detach key, and whether it was pressed.
pub fn split_detach(chunk: &[u8]) -> (&[u8], bool) {
    match chunk.iter().position(|b| *b == DETACH_KEY) {
        Some(at) => (&chunk[..at], true),
        None => (chunk, false),
    }
}

fn read_stdin(keys: &mpsc::Sender<Vec<u8>>) {
    let mut stdin = std::io::stdin().lock();
    let mut buf = [0u8; 4096];
    loop {
        match stdin.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                if keys.blocking_send(buf[..n].to_vec()).is_err() {
                    return;
                }
            }
        }
    }
}

fn client_error(e: ClientError) -> ErrorBody {
    match e {
        ClientError::Service(body) => body,
        other => ErrorBody::new(ErrorKind::Disconnected, other.to_string()),
    }
}

fn internal(message: String) -> ErrorBody {
    ErrorBody::new(ErrorKind::Internal, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detach_key_splits_the_input() {
        assert_eq!(split_detach(b"abc"), (&b"abc"[..], false));
        assert_eq!(split_detach(b"ab\x1dcd"), (&b"ab"[..], true));
        assert_eq!(split_detach(b"\x1d"), (&b""[..], true));
        // Esc and Ctrl+C go to the agent.
        assert_eq!(split_detach(b"\x1b\x03"), (&b"\x1b\x03"[..], false));
    }
}
