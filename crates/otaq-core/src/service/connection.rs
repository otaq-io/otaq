//! One client connection: a reader that answers requests in order, and a writer task that sends everything queued
//! for this client. Replies go through the same queue, so a slow client only ever slows itself down.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::json;
use std::collections::HashMap;
use tokio::io::BufReader;

use tokio::sync::{broadcast, mpsc, watch};
use tokio::task::JoinHandle;

use super::{
    Shared, agent_requests, agents_requests, board_requests, events, permissions, session_requests,
    terminal_requests, until_stopped,
};
use crate::agent_tools::setup;
use crate::attention::ClientId;
use crate::caller::{self, Caller};
use crate::protocol::{
    Body, DecodeError, ErrorBody, ErrorKind, Event, FrameReader, Message, OnboardingState,
    PROTOCOL_VERSION, Reply, Request, decode, write_message,
};
use crate::session::{Answering, OutputChunk};
use crate::store::StoreError;
use crate::store::settings::PERMISSION_WARNING_DISMISSED;
use crate::tools;

/// Numbers the connections, so focus can be kept per client (ATT-03).
static NEXT_CLIENT: AtomicU64 = AtomicU64::new(1);

pub(super) async fn serve(stream: crate::ipc::ServerStream, shared: Arc<Shared>) {
    let client: ClientId = NEXT_CLIENT.fetch_add(1, Ordering::Relaxed);
    // The kernel's word on who connected; caller identity is resolved from it (CLI-01).
    let peer_pid = crate::ipc::peer_pid(&stream);
    let (read, mut write) = crate::ipc::split(stream);
    let (out, mut queued) = mpsc::channel::<Message>(shared.config.client_queue);
    // Set when this client must be dropped at once (it fell behind on events).
    let (kick, kicked) = watch::channel(false);
    let kick = Arc::new(kick);

    let mut writer_kicked = kicked.clone();
    let writer = tokio::spawn(async move {
        loop {
            let message = tokio::select! {
                biased;
                _ = until_stopped(&mut writer_kicked) => break,
                message = queued.recv() => match message {
                    Some(message) => message,
                    None => break,
                },
            };
            tokio::select! {
                biased;
                _ = until_stopped(&mut writer_kicked) => break,
                written = write_message(&mut write, &message) => if written.is_err() { break },
            }
        }
    });

    let mut stopped = shared.stop.subscribe();
    let mut reader_kicked = kicked;
    let mut forwarder = None;
    // While this client receives events, a notification click goes to it rather than starting the app.
    let mut subscribed = None;
    // Live output of the sessions this client attached to.
    let mut attached: HashMap<String, JoinHandle<()>> = HashMap::new();
    // The sessions this client's terminal answers the program's questions in (d0117 point 4): as its latest
    // attach of the session said.
    let mut answering: HashMap<String, Answering> = HashMap::new();
    let mut reader = FrameReader::new(BufReader::new(read));
    loop {
        let frame = tokio::select! {
            _ = until_stopped(&mut stopped) => break,
            _ = until_stopped(&mut reader_kicked) => break,
            frame = reader.next_frame() => frame,
        };
        let frame = match frame {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(e) => {
                tracing::debug!(error = %e, "connection read failed");
                break;
            }
        };
        // In a task of its own: a handler that panics closes this connection (the client reconnects) instead of
        // ending the reader while the socket stays open, which would leave every later request unanswered.
        let request = tokio::spawn({
            let frame = frame.to_vec();
            let shared = shared.clone();
            async move { handle(&frame, peer_pid, client, &shared).await }
        });
        let (reply, then) = match request.await {
            Ok(answer) => answer,
            Err(e) => {
                tracing::error!(error = %e, "a request handler failed; closing the connection");
                break;
            }
        };
        // Subscribe before the reply goes out, so no event published after the reply is missed.
        let events = (matches!(then, Then::Subscribe) && forwarder.is_none())
            .then(|| shared.events.subscribe());
        if out.send(reply).await.is_err() {
            break;
        }
        if let Some(events) = events {
            subscribed = Some(shared.opener.subscribed());
            forwarder = Some(tokio::spawn(events::forward(
                events,
                out.clone(),
                kick.clone(),
            )));
        }
        match then {
            Then::Stop => {
                tracing::info!("stop requested");
                shared.stop.send_replace(true);
            }
            // The output receiver was subscribed with the scrollback in the reply, so nothing is lost or
            // repeated between the two. Attached already (two terminals of one app show the session): the running
            // feed carries on, since replacing it would drop or repeat chunks for the terminal reading it; the new
            // terminal skips what its scrollback holds by offset.
            Then::Attach {
                session,
                output,
                answers,
            } => {
                match answers {
                    Some(held) => answering.insert(session.clone(), held),
                    None => answering.remove(&session),
                };
                if attached.get(&session).is_none_or(|task| task.is_finished()) {
                    let task = tokio::spawn(forward_output(
                        session.clone(),
                        output,
                        out.clone(),
                        kick.clone(),
                    ));
                    attached.insert(session, task);
                }
            }
            Then::Detach(session) => {
                answering.remove(&session);
                if let Some(task) = attached.remove(&session) {
                    task.abort();
                }
            }
            Then::Continue | Then::Subscribe => {}
        }
    }
    drop(subscribed);
    drop(answering);
    // A client that went away isn't looking at any terminal (ATT-03) or window (d0090) any more.
    shared.sessions.client_gone(client);
    shared.inbox.client_gone(client);
    for task in attached.into_values().chain(forwarder) {
        task.abort();
        let _ = task.await;
    }
    // Closing the queue lets the writer send what is left (unless kicked), then end.
    drop(out);
    let _ = writer.await;
}

pub(super) enum Then {
    Continue,
    Subscribe,
    Stop,
    /// Forward this session's live output to the client.
    Attach {
        session: String,
        output: broadcast::Receiver<OutputChunk>,
        /// Held while the client's terminal answers the program's questions; `None` for one that doesn't.
        answers: Option<Answering>,
    },
    Detach(String),
}

/// Moves a session's live output into this connection's queue as `session_output` events until the session is
/// gone or the client detaches. A client that can't keep up is disconnected; it re-attaches and redraws from
/// the scrollback.
async fn forward_output(
    session: String,
    mut output: broadcast::Receiver<OutputChunk>,
    out: mpsc::Sender<Message>,
    kick: Arc<watch::Sender<bool>>,
) {
    use crate::protocol::{Event, encode_bytes};
    use tokio::sync::broadcast::error::RecvError;
    use tokio::sync::mpsc::error::TrySendError;
    loop {
        match output.recv().await {
            Ok(chunk) => {
                let event = Event::SessionOutput {
                    session: session.clone(),
                    offset: chunk.offset,
                    data: encode_bytes(&chunk.data),
                };
                match out.try_send(Message::event(event)) {
                    Ok(()) => {}
                    Err(TrySendError::Full(_)) => {
                        tracing::warn!(
                            session,
                            "attached client fell behind (queue full); disconnecting it"
                        );
                        kick.send_replace(true);
                        return;
                    }
                    Err(TrySendError::Closed(_)) => return,
                }
            }
            Err(RecvError::Lagged(missed)) => {
                tracing::warn!(
                    session,
                    missed,
                    "attached client fell behind; disconnecting it"
                );
                kick.send_replace(true);
                return;
            }
            Err(RecvError::Closed) => return,
        }
    }
}

async fn handle(
    frame: &[u8],
    peer_pid: Option<u32>,
    client: ClientId,
    shared: &Shared,
) -> (Message, Then) {
    match decode(frame) {
        Ok(Message {
            id,
            caller,
            pid,
            body: Body::Request(request),
            ..
        }) => {
            if let (Some(claimed), Some(peer)) = (pid, peer_pid)
                && claimed != peer
            {
                tracing::warn!(claimed, peer, "a request claimed another process's PID");
                let error = ErrorBody::new(
                    ErrorKind::BadRequest,
                    "the request's PID isn't the PID of the process on this connection",
                );
                return (Message::error(id, error), Then::Continue);
            }
            let who = caller::resolve(peer_pid, &shared.sessions);
            // The role comes from the stage the session runs (STG-10).
            let who = caller::by_stage(who, shared.board.store().pool()).await;
            // Tickets created from a Project terminal are listed in its summary (T-0167).
            let origin = (!who.is_agent()
                && matches!(
                    request,
                    Request::TicketCreate(_) | Request::TicketCreateBatch(_)
                ))
            .then(|| caller::project_terminal_of(peer_pid, &shared.sessions))
            .flatten();
            respond(
                id,
                request,
                &who,
                caller.as_deref(),
                origin.as_deref(),
                client,
                shared,
            )
            .await
        }
        Ok(Message { id, .. }) => (
            Message::error(
                id,
                ErrorBody::new(ErrorKind::BadRequest, "expected a request"),
            ),
            Then::Continue,
        ),
        Err(DecodeError::IncompatibleVersion(foreign)) => {
            let current = foreign
                .is_version_independent_request()
                .then(|| foreign.read_as_current())
                .flatten();
            match current {
                Some(Message {
                    body: Body::Request(request),
                    ..
                }) => {
                    respond(
                        foreign.id,
                        request,
                        &Caller::User,
                        None,
                        None,
                        client,
                        shared,
                    )
                    .await
                }
                _ => {
                    let mut error = ErrorBody::new(
                        ErrorKind::IncompatibleProtocol,
                        format!(
                            "the background service speaks protocol {PROTOCOL_VERSION}, not {}; restart it",
                            foreign.protocol_version
                        ),
                    );
                    error.data = Some(json!({
                        "protocol_version": PROTOCOL_VERSION,
                        "build_id": shared.config.build_id,
                    }));
                    (Message::error(foreign.id, error), Then::Continue)
                }
            }
        }
        Err(e @ DecodeError::Malformed(_)) => (
            Message::error(None, ErrorBody::new(e.kind(), e.to_string())),
            Then::Continue,
        ),
    }
}

async fn respond(
    id: Option<u64>,
    request: Request,
    who: &Caller,
    requested_actor: Option<&str>,
    origin: Option<&str>,
    client: ClientId,
    shared: &Shared,
) -> (Message, Then) {
    // The agent permission scope (CLI-02): checked once here, for every request.
    let acts = match permissions::check(who, &request, &shared.board).await {
        Ok(action) => action.is_some_and(|action| {
            permissions::agent_activity(&request, &action) == permissions::AgentActivity::Acts
        }),
        Err(e) => {
            return (
                Message::error(id, board_requests::error_body(&e)),
                Then::Continue,
            );
        }
    };
    // A Project agent's command that acts means it is at it again: a permission prompt it waited on was answered
    // (ATT-02). Reads and the status line's bookkeeping aren't it acting (d0119).
    if acts
        && let Some((agent, _)) = who.project_agent()
        && let Err(e) = shared.project_agents.active(agent).await
    {
        tracing::warn!(agent, error = %e, "can't record a Project agent command");
    }
    // An `otaq` command of an agent that acts is a signal: it clears Needs you and Idle and moves the last signal
    // (ATT-02, ATT-05); an open question stays (d0119).
    if acts
        && who.is_agent()
        && let Err(e) = shared.signals.command(who).await
    {
        tracing::warn!(error = %e, "can't record an agent command");
    }
    let method = request.method();
    let (reply, then) = match request {
        Request::Ping => (Reply::Pong, Then::Continue),
        Request::ServiceStatus => (Reply::ServiceStatus(shared.status().await), Then::Continue),
        Request::StartupReport => (
            Reply::StartupReport(Box::new(shared.recovery.startup_report())),
            Then::Continue,
        ),
        Request::ServiceStop => (Reply::Ok, Then::Stop),
        Request::Subscribe => (Reply::Ok, Then::Subscribe),
        Request::ToolsCheck => {
            // The onboarding Tools step checks the agent tools again (d0086): one must be connected (SET-02).
            if let Err(e) = shared.connected.recheck(None).await {
                tracing::warn!(error = %e, "can't check the agent tools");
            }
            let mut report = tools::check(&shared.config.tools).await;
            report.agent_tools = shared.connected.list().tools;
            (Reply::ToolsCheck(report), Then::Continue)
        }
        Request::OnboardingState => match onboarding_state(shared).await {
            Ok(state) => (Reply::OnboardingState(state), Then::Continue),
            Err(e) => return (store_error(id, &e), Then::Continue),
        },
        Request::DismissPermissionWarning => match shared
            .store
            .set_setting(PERMISSION_WARNING_DISMISSED, "true")
            .await
        {
            Ok(()) => (Reply::Ok, Then::Continue),
            Err(e) => return (store_error(id, &e), Then::Continue),
        },
        Request::DismissTrustNote => match setup::dismiss_note(&shared.store).await {
            Ok(()) => {
                shared.events.publish(Event::TrustChanged);
                (Reply::Ok, Then::Continue)
            }
            Err(e) => return (store_error(id, &e), Then::Continue),
        },
        // The app window's focus (d0090), per connection like a terminal's.
        Request::AppFocus(p) => {
            shared.inbox.set_looking(client, p.focused);
            // The user may have changed System Settings meanwhile: read macOS's permission again (T-0372).
            if p.focused {
                shared.notifications.refresh();
            }
            (Reply::Ok, Then::Continue)
        }
        Request::Hook(p) => match shared.signals.hook(who, p.tool, &p.event, &p.payload).await {
            Ok(signals) => {
                shared.after_signals(who, p.tool, &signals).await;
                (Reply::Ok, Then::Continue)
            }
            Err(e) => {
                return (
                    Message::error(id, board_requests::error_body(&e)),
                    Then::Continue,
                );
            }
        },
        request @ (Request::SessionList(_)
        | Request::SessionAttach(_)
        | Request::SessionDetach(_)
        | Request::SessionResize(_)
        | Request::SessionInput(_)
        | Request::SessionDeliverNow(_)
        | Request::SessionFocus(_)) => {
            match session_requests::handle(request, client, shared).await {
                Ok(answer) => answer,
                Err(error) => return (Message::error(id, error), Then::Continue),
            }
        }
        request @ (Request::RunStart(_)
        | Request::RunStop(_)
        | Request::RunStatus(_)
        | Request::RunList
        | Request::RunSuggest(_)
        | Request::RunSuggestCancel(_)
        | Request::ShellOpen(_)
        | Request::ToolTerminalOpen(_)
        | Request::ProjectTerminalOpen(_)
        | Request::ProjectTerminalList(_)
        | Request::ProjectTerminalClose(_)
        | Request::ProjectTerminalCreated(_)
        | Request::ProjectTerminalDismiss(_)) => {
            match terminal_requests::handle(request, shared).await {
                Ok(reply) => (reply, Then::Continue),
                Err(error) => return (Message::error(id, error), Then::Continue),
            }
        }
        request @ (Request::ProjectAgentOpen(_)
        | Request::OnboardingAsk(_)
        | Request::OnboardingFlowSetup(_)
        | Request::OnboardingUseDefault(_)
        | Request::ProjectAgentList(_)
        | Request::ProjectAgentClose(_)
        | Request::ProjectAgentCloseEnded(_)
        | Request::ProjectAgentResume(_)
        | Request::ProjectAgentRename(_)
        | Request::AgentName(_)
        | Request::AgentAsk(_)
        | Request::AgentAskHuman(_)
        | Request::ProjectAgentAnswer(_)
        | Request::ProjectAgentSend(_)
        | Request::ProjectAgentPrime(_)
        | Request::ProjectAgentCreated(_)
        | Request::LeadOpen(_)
        | Request::LeadResume(_)
        | Request::LeadList(_)) => match agent_requests::handle(request, who, shared).await {
            Ok(reply) => (reply, Then::Continue),
            Err(error) => return (Message::error(id, error), Then::Continue),
        },
        request @ (Request::AgentsList
        | Request::AgentsAdd(_)
        | Request::AgentsEdit(_)
        | Request::AgentsDuplicate(_)
        | Request::AgentsDelete(_)
        | Request::AgentsInUse(_)
        | Request::AgentsMoveRole(_)
        | Request::AgentsDefaultModel(_)
        | Request::AgentsFallback(_)
        | Request::AgentsImportSkill(_)
        | Request::AgentsReimportSkill(_)
        | Request::AgentsReads(_)) => match agents_requests::handle(request, shared).await {
            Ok(reply) => (reply, Then::Continue),
            Err(error) => return (Message::error(id, error), Then::Continue),
        },
        // Boxed: the board handler's future is large, and a debug build's worker stack is small.
        board => match Box::pin(board_requests::handle(
            board,
            who,
            requested_actor,
            origin,
            shared,
        ))
        .await
        {
            Some(Ok(reply)) => (reply, Then::Continue),
            Some(Err(error)) => return (Message::error(id, error), Then::Continue),
            // A request routed to neither list: answer it rather than end this connection's reader, which
            // would leave the client waiting forever on every later request.
            None => {
                tracing::error!(method, "request not routed to any handler");
                let error = ErrorBody::new(
                    ErrorKind::Internal,
                    format!("{method} isn't handled by the background service"),
                );
                return (Message::error(id, error), Then::Continue);
            }
        },
    };
    let mut message = Message::reply(0, reply);
    message.id = id;
    (message, then)
}

async fn onboarding_state(shared: &Shared) -> Result<OnboardingState, StoreError> {
    let projects = shared.store.projects().await?;
    let mut trust_failed = shared.board.trust().failed();
    // A project removed since stays out of the warning.
    trust_failed.retain(|f| projects.iter().any(|p| p.key == f.project_key));
    Ok(OnboardingState {
        has_projects: !projects.is_empty(),
        permission_warning_dismissed: shared
            .store
            .setting(PERMISSION_WARNING_DISMISSED)
            .await?
            .is_some_and(|v| v == "true"),
        trust_note: setup::note_pending(&shared.store).await?,
        trust_note_tools: setup::note_tools(&shared.store).await?,
        trust_failed,
    })
}

fn store_error(id: Option<u64>, e: &StoreError) -> Message {
    tracing::error!(error = %e, "store request failed");
    Message::error(id, ErrorBody::new(ErrorKind::Internal, e.to_string()))
}
