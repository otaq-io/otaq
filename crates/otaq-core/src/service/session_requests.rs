//! Session requests: list, attach (stored output + live output on this connection), detach, resize, input
//! (SES-04, SVC-05, SVC-06) and Deliver now (SES-06, SES-07).

use serde_json::json;

use super::Shared;
use super::board_requests::error_body;
use super::connection::Then;
use crate::attention::ClientId;
use crate::board::{BoardError, Refusal};
use crate::protocol::{
    ErrorBody, ErrorKind, Reply, Request, SessionAttach, SessionTarget, decode_bytes, encode_bytes,
};
use crate::session::{SessionError, SessionInfo, SessionKind, SessionTicket};

pub(super) async fn handle(
    request: Request,
    client: ClientId,
    shared: &Shared,
) -> Result<(Reply, Then), ErrorBody> {
    let sessions = &shared.sessions;
    match request {
        Request::SessionList(p) => {
            let list = match p.key {
                Some(key) => {
                    let ticket = ticket(shared, &key).await?;
                    sessions.history(&ticket).await.map_err(session_error)?
                }
                None => sessions.in_memory(),
            };
            Ok((Reply::Sessions(list), Then::Continue))
        }
        Request::SessionAttach(p) => {
            let id = session_id(shared, p.target).await?;
            let (attached, answers) = sessions
                .attach_answering(&id, !p.quiet)
                .map_err(session_error)?;
            let reply = Reply::SessionAttached(Box::new(SessionAttach {
                session: attached.info,
                scrollback: encode_bytes(&attached.scrollback),
                start: attached.start,
                end: attached.end,
            }));
            Ok((
                reply,
                Then::Attach {
                    session: id,
                    output: attached.output,
                    answers,
                },
            ))
        }
        Request::SessionDetach(p) => Ok((Reply::Ok, Then::Detach(p.session))),
        Request::SessionResize(p) => {
            sessions.resize(&p.session, p.size).map_err(session_error)?;
            Ok((Reply::Ok, Then::Continue))
        }
        Request::SessionInput(p) => {
            let bytes = decode_bytes(&p.data).map_err(|e| {
                ErrorBody::new(ErrorKind::BadRequest, format!("input isn't base64: {e}"))
            })?;
            if p.reply {
                sessions.write(&p.session, &bytes).map_err(session_error)?;
                return Ok((Reply::Ok, Then::Continue));
            }
            // Never over the user's unsent text when the caller asks for an empty line (SDD §2, d0048).
            if p.if_empty
                && sessions
                    .has_unsent_input(&p.session)
                    .map_err(session_error)?
            {
                let mut body = ErrorBody::new(
                    ErrorKind::Refused,
                    "Its input line has unsent text: send or clear it in its terminal.",
                );
                body.data = Some(json!({ "refusal": Refusal::NeedsConfirmation }));
                return Err(body);
            }
            let effect = sessions.input(&p.session, &bytes).map_err(session_error)?;
            // Enter in the ticket's terminal is the user's answer: the needs-you label clears (ATT-02). Not in its
            // Shell tab or Run, which are "not the agent", nor in a helper's (its own waits, below). In the agent's
            // own terminal the first line sent answers an open question (d0119).
            let info = sessions.info(&p.session).ok();
            let line = effect
                .sent
                .iter()
                .find(|line| !line.is_empty())
                .map(String::as_str);
            if effect.enters > 0
                && let Some(info) = info.as_ref().filter(|i| {
                    !matches!(
                        i.kind,
                        crate::session::SessionKind::Shell
                            | crate::session::SessionKind::Run
                            | crate::session::SessionKind::Helper
                    )
                })
                && let Some(key) = &info.ticket
                && let Ok(id) = shared.board.ticket_id(&key.to_string()).await
            {
                let line = line.filter(|_| info.kind == SessionKind::Agent);
                if let Err(e) = shared.signals.enter(&id, line).await {
                    tracing::warn!(session = p.session, error = %e, "can't clear the signal");
                }
            }
            // A key in a ticket's agent terminal answers its open permission prompt, whatever the ticket's status
            // (Codex takes "y" alone): the message writer's prompt in Review closes here, and a card left at Needs you
            // by such a key is Working again. Escape sequences (arrows, the mouse) and pastes don't.
            if (effect.enters > 0 || bytes.len() == 1)
                && let Some(info) = info
                    .as_ref()
                    .filter(|i| matches!(i.kind, SessionKind::Agent | SessionKind::Delivery))
                && let Some(key) = &info.ticket
                && shared.signals.open_prompt(&p.session).is_some()
                && let Ok(id) = shared.board.ticket_id(&key.to_string()).await
                && let Err(e) = shared
                    .signals
                    .key_answered_prompt(key, &id, &p.session)
                    .await
            {
                tracing::warn!(session = p.session, error = %e, "can't clear the prompt");
            }
            // The user's answer in any other agent session's terminal (ATT-02): Enter, or a key alone on a tool whose
            // permission prompt takes one (Codex's "y", after which it sends nothing until its turn ends or asks
            // again). Escape sequences and pastes don't count.
            let key_alone = effect.enters == 0
                && bytes.len() == 1
                && info
                    .as_ref()
                    .and_then(|i| i.tool)
                    .is_some_and(|tool| shared.adapters.get(tool).key_alone_approves_prompt());
            if (effect.enters > 0 || key_alone)
                && let Some(info) = &info
                && let Err(e) = answered(shared, info, line.filter(|_| !key_alone)).await
            {
                tracing::warn!(session = p.session, error = %e, "can't clear the agent's wait");
            }
            // Any key may answer a tool's own prompt without a signal (Copilot's Esc, ADP-08).
            if !bytes.is_empty()
                && let Err(e) = shared.signals.typed(&p.session).await
            {
                tracing::warn!(session = p.session, error = %e, "can't check the typed key");
            }
            // An Enter or a backspace may have emptied the line: waiting messages may go now. A key a tool's own
            // menu took leaves the line empty on screen, which counts once the program has drawn it: look again
            // then.
            let deliveries = shared.deliveries.clone();
            let sessions = sessions.clone();
            let session = p.session.clone();
            tokio::spawn(async move {
                if sessions.has_unsent_input(&session).unwrap_or(true) {
                    tokio::time::sleep(crate::session::KEY_SETTLE).await;
                    if sessions.has_unsent_input(&session).unwrap_or(true) {
                        return;
                    }
                }
                if let Err(e) = deliveries.deliver_if_ready(&session).await {
                    tracing::warn!(session, error = %e, "delivery failed");
                }
            });
            Ok((Reply::Ok, Then::Continue))
        }
        Request::SessionSendAgain(p) => {
            let id = session_id(shared, p.target).await?;
            let delivery = shared
                .deliveries
                .send_again(&id, p.clear_first)
                .await
                .map_err(session_error)?;
            Ok((Reply::Delivery(delivery), Then::Continue))
        }
        Request::SessionFocus(p) => {
            sessions
                .set_focused(&p.session, client, p.focused)
                .map_err(session_error)?;
            Ok((Reply::Ok, Then::Continue))
        }
        Request::SessionDeliverNow(p) => {
            let id = session_id(shared, p.target).await?;
            let delivery = if p.clear_first {
                shared.deliveries.clear_and_send(&id, None).await
            } else {
                shared.deliveries.deliver_now(&id).await
            }
            .map_err(session_error)?;
            Ok((Reply::Delivery(delivery), Then::Continue))
        }
        other => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            format!("{} isn't a session request", other.method()),
        )),
    }
}

/// The user answered in `info`'s terminal (Enter, with `line` its first line that isn't empty, or a key alone that
/// answers its tool's prompt): its open permission prompt closes, wherever the session keeps it. A session with none
/// open is left as it is. The ticket's agent and delivery sessions are answered in [`handle`], with the ticket's status.
async fn answered(
    shared: &Shared,
    info: &SessionInfo,
    line: Option<&str>,
) -> Result<(), BoardError> {
    let session = info.id.as_str();
    // The prompt on its screen closes; the next one open reads Needs you once drawn (a helper's closes in its own).
    if matches!(
        info.kind,
        SessionKind::ProjectAgent | SessionKind::Validator
    ) {
        shared.signals.answer_shown(session);
    }
    match info.kind {
        // Or its passed-on question (d0133 audit 3, d0119).
        SessionKind::Helper => shared.helpers.enter(session, line).await,
        // Or its open question (d0068, d0119).
        SessionKind::ProjectAgent => shared.project_agents.enter(session, line).await,
        // A checker's: the ticket is Validating… again.
        SessionKind::Validator => match &info.ticket {
            Some(key) => {
                let id = shared.board.ticket_id(&key.to_string()).await?;
                shared.validation.prompt_answered(&id, session).await
            }
            None => Ok(()),
        },
        // A finished ticket's answer session: only the session shows its prompt.
        SessionKind::Answer => {
            if let Some(key) = &info.ticket
                && shared.signals.open_prompt(session).is_some()
            {
                let id = shared.board.ticket_id(&key.to_string()).await?;
                shared.signals.prompt_answered(key, &id, session);
            }
            Ok(())
        }
        SessionKind::Agent
        | SessionKind::Delivery
        | SessionKind::Shell
        | SessionKind::Project
        | SessionKind::Run => Ok(()),
    }
}

/// A session by id, the newest session of a kind (agent by default) of a ticket by key or the one running its stage
/// (FLW-20), or a project's Project terminal.
async fn session_id(shared: &Shared, target: SessionTarget) -> Result<String, ErrorBody> {
    let mut kind = target.kind.unwrap_or(SessionKind::Agent);
    match (target.session, target.key, target.project) {
        (Some(id), _, _) => Ok(id),
        (None, Some(key), _) => {
            if let Some(stage) = &target.stage {
                let full = shared
                    .board
                    .ticket(&key)
                    .await
                    .map_err(|e| error_body(&e))?;
                let (of_stage, stage) =
                    super::board_requests::stage_session(&shared.board, &full, stage)
                        .await
                        .map_err(|e| error_body(&e))?;
                kind = of_stage;
                // A check has one agent: `--stage` names its session (`--agent` is ignored).
                if kind == SessionKind::Validator {
                    return shared
                        .sessions
                        .running_of(&full.id, kind)
                        .map(|s| s.id)
                        .ok_or_else(|| {
                            ErrorBody::new(
                                ErrorKind::NotFound,
                                format!("{}'s {} check isn't running", full.key, stage.name),
                            )
                        });
                }
            }
            let ticket = ticket(shared, &key).await?;
            shared
                .sessions
                .latest_of(&ticket.id, kind)
                .map(|s| s.id)
                .ok_or_else(|| {
                    ErrorBody::new(
                        ErrorKind::NotFound,
                        match kind {
                            SessionKind::Agent => format!("{} has no live session", ticket.key),
                            other => format!("{} has no live {other} session", ticket.key),
                        },
                    )
                })
        }
        (None, None, Some(project)) => shared
            .shells
            .latest_project_terminal(&project)
            .map(|t| t.session.id)
            .ok_or_else(|| {
                ErrorBody::new(
                    ErrorKind::NotFound,
                    format!("{project} has no Project terminal"),
                )
            }),
        (None, None, None) => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            "name a session, a ticket key or a project",
        )),
    }
}

async fn ticket(shared: &Shared, key: &str) -> Result<SessionTicket, ErrorBody> {
    let ticket = shared.board.ticket(key).await.map_err(|e| error_body(&e))?;
    Ok(SessionTicket {
        id: ticket.id,
        key: ticket.key,
    })
}

pub(super) fn session_error(e: SessionError) -> ErrorBody {
    let refused = |code: Refusal, message: String| {
        let mut body = ErrorBody::new(ErrorKind::Refused, message);
        body.data = Some(json!({ "refusal": code }));
        body
    };
    match e {
        SessionError::NotFound(_) => ErrorBody::new(ErrorKind::NotFound, e.to_string()),
        SessionError::AlreadyRunning { .. } => refused(Refusal::AlreadyRunning, e.to_string()),
        SessionError::Ended(_) => refused(Refusal::SessionEnded, e.to_string()),
        SessionError::Paused { .. } => refused(Refusal::NotAllowed, e.to_string()),
        SessionError::Spawn(_) | SessionError::Io(_) | SessionError::Store(_) => {
            tracing::error!(error = %e, "session request failed");
            ErrorBody::new(ErrorKind::Internal, e.to_string())
        }
    }
}
