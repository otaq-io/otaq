//! Run requests (RUN-01, RUN-02, RUN-04): start, stop, status, the list of every Run that is up, and Ask the agent;
//! the Shell (RUN-05) and the Project terminals (RUN-06): open, list, close.

use super::Shared;
use super::board_requests::error_body;
use super::permissions::stage_name;
use crate::caller::Caller;
use crate::protocol::{ErrorBody, ErrorKind, Reply, Request};
use crate::session::SessionKind;
use crate::terminals::run::{LOG_TAIL, RunAgent};

/// The ticket agent or check asking for a Run (agents run the app), named by its stage ("Dev"); `None` for the user. The
/// permission check already let only the ticket's own agent and check through.
async fn run_agent(caller: &Caller, shared: &Shared) -> Option<RunAgent> {
    let (session, kind, fallback) = match caller {
        Caller::Agent { session, .. } => (session, SessionKind::Agent, "agent"),
        Caller::Validator { session, .. } => (session, SessionKind::Validator, "check"),
        _ => return None,
    };
    let stage = stage_name(caller, &shared.board)
        .await
        .unwrap_or_else(|| fallback.to_owned());
    Some(RunAgent {
        session: session.clone(),
        stage,
        kind,
    })
}

pub(super) async fn handle(
    request: Request,
    caller: &Caller,
    shared: &Shared,
) -> Result<Reply, ErrorBody> {
    let runs = &shared.runs;
    let run = |r: Result<crate::terminals::run::RunInfo, crate::board::BoardError>| {
        r.map(|info| Reply::Run(Box::new(info)))
            .map_err(|e| error_body(&e))
    };
    match request {
        Request::RunStart(p) => run(runs.start_as(&p.key, run_agent(caller, shared).await).await),
        Request::RunStop(p) => {
            let agent = run_agent(caller, shared).await;
            run(runs.stop_as(&p.key, agent.as_ref()).await)
        }
        Request::RunStatus(p) => run(runs.info(&p.key).await),
        Request::RunSet(p) => match run_agent(caller, shared).await {
            Some(agent) => run(runs
                .set_command(&p.key, agent, &p.command, p.open_url.as_deref())
                .await),
            None => Err(ErrorBody::new(
                ErrorKind::Refused,
                "`run set` is for a ticket's agent; set the run command in Settings → Run command",
            )),
        },
        Request::RunCommandUndo(p) => shared
            .board
            .undo_agent_run_command(&p.project)
            .await
            .map(|()| {
                shared
                    .events
                    .publish(crate::protocol::Event::ProjectsChanged);
                Reply::Ok
            })
            .map_err(|e| error_body(&e)),
        Request::RunLog(p) => runs
            .log(&p.key, p.tail.map_or(LOG_TAIL, |t| t as usize))
            .await
            .map(|log| Reply::RunLog(Box::new(log)))
            .map_err(|e| error_body(&e)),
        Request::RunList => Ok(Reply::Runs(runs.running().await)),
        Request::RunSuggest(p) => shared
            .suggester
            .start(&p.project)
            .await
            .map(|()| Reply::Ok)
            .map_err(|e| error_body(&e)),
        Request::ToolTerminalOpen(p) => shared
            .tool_terminals
            .open(p.tool, p.action)
            .await
            .map(|s| Reply::Session(Box::new(s)))
            .map_err(|e| error_body(&e)),
        Request::ShellOpen(p) => shared
            .shells
            .open_shell(&p.key, p.reopen)
            .await
            .map(|s| Reply::Session(Box::new(s)))
            .map_err(|e| error_body(&e)),
        Request::ProjectTerminalOpen(p) => shared
            .shells
            .open_project(&p.project, p.new)
            .await
            .map(|t| Reply::ProjectTerminal(Box::new(t)))
            .map_err(|e| error_body(&e)),
        Request::ProjectTerminalList(p) => {
            let project = shared
                .board
                .project(&p.project)
                .await
                .map_err(|e| error_body(&e))?;
            Ok(Reply::ProjectTerminals(
                shared.shells.project_terminals(&project.key),
            ))
        }
        Request::ProjectTerminalClose(p) => shared
            .shells
            .close_project_terminal(&p.session)
            .await
            .map(|()| Reply::Ok)
            .map_err(|e| error_body(&e)),
        Request::ProjectTerminalCreated(p) => shared
            .board
            .created_in_terminal(&p.project)
            .await
            .map(|c| Reply::CreatedHere(Box::new(c)))
            .map_err(|e| error_body(&e)),
        Request::ProjectTerminalDismiss(p) => shared
            .board
            .dismiss_created(&p.project)
            .await
            .map(|()| Reply::Ok)
            .map_err(|e| error_body(&e)),
        Request::RunSuggestCancel(p) => {
            shared.suggester.cancel(&p.project);
            Ok(Reply::Ok)
        }
        other => Err(ErrorBody::new(
            ErrorKind::BadRequest,
            format!("{} isn't a terminal request", other.method()),
        )),
    }
}
