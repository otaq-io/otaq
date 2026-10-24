//! Maps every request to the [`Action`] the permission policy judges (CLI-02, CLI-03, CLI-07). The match is
//! exhaustive, so a new request can't be added without saying what it does.

use crate::board::batch::{BatchTicket, Ref};
use crate::board::epic_flight::is_never_started;
use crate::board::stage_choices::Chooser;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError};
use crate::caller::Caller;
use crate::epic_lead::questions;
use crate::policy::{
    Action, Names, ONLY_A_LEAD_MARKS, ONLY_A_LEAD_SUGGESTS, TicketFacts, authorize_in_stage,
};
use crate::protocol::Request;

/// Checks `request` for `caller` before it runs, and returns the action it was judged as. The user passes
/// without any lookup (`None`); every other caller (an agent, a validator, a Project agent, an epic lead) is judged
/// by the policy.
pub(super) async fn check(
    caller: &Caller,
    request: &Request,
    board: &Board,
) -> Result<Option<Action>, BoardError> {
    if *caller == Caller::User {
        return Ok(None);
    }
    let action = action_for(caller, request, board).await?;
    let caller_name = match caller {
        Caller::EpicLead { epic_id, .. } => board
            .ticket_by_id(epic_id)
            .await
            .ok()
            .map(|epic| crate::project_agents::lead::lead_name(&epic.title)),
        // Only the terminal rule names a Project agent and its project (CLI-09).
        Caller::ProjectAgent { agent, .. } if matches!(action, Action::ReadTerminal { .. }) => {
            Some(project_agent_name(board, agent).await)
        }
        _ => None,
    };
    let project_name = match caller {
        Caller::ProjectAgent { project, .. } if matches!(action, Action::ReadTerminal { .. }) => {
            board.project(project).await.ok().map(|p| p.name)
        }
        _ => None,
    };
    let names = Names {
        caller: caller_name.as_deref(),
        project: project_name.as_deref(),
    };
    let stage = stage_name(caller, board).await;
    authorize_in_stage(caller, names, stage.as_deref(), &action)?;
    Ok(Some(action))
}

/// What an agent's request says about the agent (d0119, d0042, d0027).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentActivity {
    /// A hook, or the status line's bookkeeping (`usage.record_limits`): never the agent running a command.
    Bookkeeping,
    /// A read-only `otaq` command the agent ran (`prime`, `diff files`, `ping`): it changes no status and moves no
    /// last signal.
    Reads,
    /// A command that acts (writes): the agent is at it again. It clears Needs you and Idle and moves the last
    /// signal; it never clears an open question.
    Acts,
}

/// Which requests count as the agent acting (d0119): only those that act. `action` is what the policy judged
/// `request` as ([`check`]).
pub(crate) fn agent_activity(request: &Request, action: &Action) -> AgentActivity {
    match request {
        Request::Hook(_) | Request::UsageRecordLimits(_) => AgentActivity::Bookkeeping,
        // A Project agent naming itself and asking (the handler refuses everyone else): judged as reads, they act.
        Request::AgentName(_) | Request::AgentAsk(_) | Request::AgentAskHuman(_) => {
            AgentActivity::Acts
        }
        _ => match action {
            Action::Read | Action::ReadTerminal { .. } => AgentActivity::Reads,
            _ => AgentActivity::Acts,
        },
    }
}

/// The name of the stage a ticket session's stage run is in, for its refusals (STG-10), on every flow (Q269, AGM-10):
/// "NOTE-1's Validate check can only give its verdict". None for a session with no stage run.
async fn stage_name(caller: &Caller, board: &Board) -> Option<String> {
    let (Caller::Agent {
        ticket, session, ..
    }
    | Caller::Validator {
        ticket, session, ..
    }) = caller
    else {
        return None;
    };
    let run = crate::caller::SessionStage::of(board.store().pool(), session)
        .await
        .ok()??;
    let flow = board.effective_flow(ticket.project.as_str()).await.ok();
    Some(
        flow.as_ref()
            .and_then(|f| f.stage(&run.stage))
            .map_or(run.stage.clone(), |s| s.name.clone()),
    )
}

/// Who sets stage choices in a request (AGM-08, AGM-11): the user, the Project agent by its name, an epic lead
/// ("Wishlist lead"), or a ticket's agent by the agent its session runs ("Griller").
pub(super) async fn chooser(caller: &Caller, board: &Board) -> Chooser {
    match caller {
        Caller::User => Chooser::User,
        Caller::ProjectAgent { agent, .. } => Chooser::ProjectAgent {
            name: project_agent_name(board, agent).await,
        },
        Caller::EpicLead { epic_id, epic, .. } => Chooser::EpicLead {
            name: board.ticket_by_id(epic_id).await.map_or_else(
                |_| format!("{epic}'s lead"),
                |e| crate::project_agents::lead::lead_name(&e.title),
            ),
        },
        Caller::Agent {
            ticket, session, ..
        }
        | Caller::Validator {
            ticket, session, ..
        } => {
            let name: Option<Option<String>> =
                sqlx::query_scalar("SELECT agent_name FROM sessions WHERE id = ?")
                    .bind(session)
                    .fetch_optional(board.store().pool())
                    .await
                    .ok()
                    .flatten();
            Chooser::TicketAgent {
                name: name
                    .flatten()
                    .unwrap_or_else(|| format!("{ticket}'s agent")),
            }
        }
    }
}

/// A Project agent's name ("New agent" until named, PAG-04).
async fn project_agent_name(board: &Board, agent: &str) -> String {
    let name: Option<(Option<String>,)> =
        sqlx::query_as("SELECT name FROM project_agents WHERE id = ?")
            .bind(agent)
            .fetch_optional(board.store().pool())
            .await
            .ok()
            .flatten();
    name.and_then(|(name,)| name)
        .unwrap_or_else(|| crate::project_agents::NEW_AGENT.to_owned())
}

async fn facts(board: &Board, key: &str) -> Result<TicketFacts, BoardError> {
    let ticket = board.ticket(key).await?;
    facts_of(board, ticket).await
}

async fn facts_of(board: &Board, ticket: Ticket) -> Result<TicketFacts, BoardError> {
    let epic_id = match &ticket.epic {
        Some(epic) => Some(board.ticket_id(&epic.to_string()).await?),
        None => None,
    };
    Ok(TicketFacts {
        backlog: ticket.place.is_backlog() && !ticket.has_started(),
        never_started: is_never_started(&ticket),
        epic_id,
        id: ticket.id,
        label: ticket.key.to_string(),
        created_by: ticket.created_by,
    })
}

/// The epic id a new ticket is created in, from the epic's key (`--parent SHOP-40`).
async fn epic_of_new(board: &Board, epic: Option<&str>) -> Result<Option<String>, BoardError> {
    match epic {
        Some(key) => Ok(Some(board.ticket_id(key).await?)),
        None => Ok(None),
    }
}

/// A batch creates tickets (always allowed) and links them: each link is judged with the new tickets created
/// by the caller.
async fn batch_action(
    caller: &Caller,
    tickets: &[BatchTicket],
    board: &Board,
) -> Result<Action, BoardError> {
    // Each new ticket's epic: an existing epic's id; an epic new in the batch has none yet.
    let mut epics = Vec::with_capacity(tickets.len());
    for ticket in tickets {
        epics.push(match &ticket.parent {
            Some(parent) => match board.batch_ref(tickets, parent).await? {
                Ref::Existing(epic) => Some(epic.id),
                Ref::Batch(_) => None,
            },
            None => None,
        });
    }
    let new = |n: usize| TicketFacts {
        id: format!("batch:{n}"),
        label: format!("the new ticket \"{}\"", tickets[n].title),
        created_by: caller.actor(None),
        backlog: true,
        epic_id: epics[n].clone(),
        never_started: true,
    };
    let mut actions: Vec<Action> = epics
        .iter()
        .map(|epic| Action::CreateTicket { epic: epic.clone() })
        .collect();
    // Choices first, so a ticket agent hears why it can't set them on a new ticket.
    for (n, ticket) in tickets.iter().enumerate() {
        if !ticket.choices.is_empty() {
            actions.insert(0, Action::SetChoices(new(n)));
        }
    }
    for (n, ticket) in tickets.iter().enumerate() {
        for dep in &ticket.depends_on {
            let parent = match board.batch_ref(tickets, dep).await? {
                Ref::Batch(m) => new(m),
                Ref::Existing(t) => facts_of(board, *t).await?,
            };
            actions.push(Action::Dependency {
                child: new(n),
                parent,
            });
        }
    }
    Ok(Action::All(actions))
}

async fn action_for(
    caller: &Caller,
    request: &Request,
    board: &Board,
) -> Result<Action, BoardError> {
    use Request as R;
    Ok(match request {
        R::Ping
        | R::ServiceStatus
        | R::StartupReport
        | R::Subscribe
        | R::ToolsCheck
        | R::OnboardingState
        | R::NotificationsState
        | R::AgentsPauseCounts(_)
        | R::ProjectList
        | R::ProjectProbe(_)
        | R::ProjectBranches(_)
        | R::ProjectPrProvider(_)
        | R::TicketPrFlow(_)
        | R::TicketQuestions(_)
        | R::TicketMessages(_)
        | R::TicketLeadLog(_)
        | R::TicketHumanAnswers(_)
        | R::TicketPrBranchCheck(_)
        | R::BoardSnapshot(_)
        | R::TicketGet(_)
        | R::TicketList(_)
        | R::DepReadiness(_)
        | R::SchedulerStatus
        | R::UsageSummary(_)
        | R::UsageSessions(_)
        | R::UsageActivity
        | R::TicketAgentEdits(_)
        | R::TicketDeletePreview(_)
        | R::TicketHistory(_)
        | R::DiffFiles(_)
        | R::DiffFile(_)
        | R::FilesTree(_)
        | R::FilesSearch(_)
        | R::FilesRead(_)
        | R::FilesDiff(_)
        | R::FilesChanges(_)
        | R::FilesProjectTree(_)
        | R::FilesProjectSearch(_)
        | R::FilesProjectRead(_)
        | R::FilesProjectDiff(_)
        | R::FilesProjectChanges(_)
        | R::FilesProjectSources(_)
        | R::ReviewComments(_)
        | R::ReviewFeedback(_)
        | R::TicketMergeProposal(_)
        | R::TicketApproveCheck(_)
        | R::TicketConflict(_)
        | R::TicketPrime(_)
        | R::DecisionList(_)
        | R::TicketPrimeValidate(_)
        | R::TicketValidations(_)
        | R::EpicClosePlan(_)
        | R::DepTree(_)
        | R::DepCycles(_)
        | R::SettingsGet
        | R::ProjectSettingsGet(_)
        | R::FlowCurrent(_)
        | R::FlowVersions(_)
        | R::FlowProposal(_)
        | R::FlowDraft(_)
        | R::FlowWaiting(_)
        | R::SessionList(_)
        | R::SessionBeforeRestart(_)
        | R::InboxList
        | R::InboxCounts
        | R::CleanupScan
        | R::WarmList
        | R::SessionAttach(_)
        | R::SessionDetach(_)
        | R::RunStatus(_)
        | R::RunList
        | R::ProjectTerminalCreated(_)
        | R::ProjectTerminalList(_)
        | R::ProjectAgentList(_)
        | R::LeadList(_)
        | R::ProjectAgentPrime(_)
        | R::ProjectAgentCreated(_) => Action::Read,
        // Only a Project agent names itself and asks with these (the handler refuses everyone else).
        // A lead is named after its epic: `agent name` is refused with the lead's text (CLI-07).
        R::AgentName(_) if matches!(caller, Caller::EpicLead { .. }) => {
            Action::UserOnly("name agents")
        }
        R::AgentName(_) | R::AgentAsk(_) | R::AgentAskHuman(_) => Action::Read,
        R::ProjectAgentAnswer(_) => Action::UserOnly("answer Project agents"),
        R::ProjectAgentSend(_) => Action::UserOnly("message Project agents"),
        // A hook only ever signals the caller's own session.
        R::Hook(_) => Action::Read,
        // Bookkeeping from the agent's own status line: the plan windows Claude Code reported.
        R::UsageRecordLimits(_) => Action::Read,
        R::AgentsModels(_) => Action::Read,
        R::TicketAgents(_) => Action::Read,
        R::BoardAgents(_) => Action::Read,
        // Starts the tool: only the user's Settings → Agents asks for it.
        R::AgentsModelsRefresh(_) => Action::UserOnly("read the model lists again"),
        R::AgentsModelsRetry(_) => Action::UserOnly("read the model lists again"),

        R::TicketUpdate(p) => {
            let target = facts(board, &p.key).await?;
            let fields = crate::board::tickets::TicketUpdate {
                choices: Vec::new(),
                by_user: false,
                ..p.update.clone()
            };
            match (p.update.choices.is_empty(), fields == Default::default()) {
                (true, _) => Action::EditTicket(target),
                // Choices only: the choice rule alone.
                (false, true) => Action::SetChoices(target),
                (false, false) => Action::All(vec![
                    Action::SetChoices(target.clone()),
                    Action::EditTicket(target),
                ]),
            }
        }
        R::TicketSetChoice(p) => Action::SetChoices(facts(board, &p.key).await?),
        R::TicketChoices(_) => Action::Read,
        R::TicketAskHuman(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        R::DecisionRecord(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        R::DecisionChange(_) => Action::UserOnly("change recorded decisions"),
        // Ending a stage changes the agent's own ticket (CLI-02, AGM-10); a check session may only give its verdict.
        R::TicketStageNext(p) | R::TicketStageDone(p) => {
            Action::ChangeTicket(facts(board, &p.key).await?)
        }
        R::ValidationVerdict(p) => Action::Verdict(facts(board, &p.key).await?),
        R::TicketValidationAction(_) => Action::UserOnly("act on a validation"),
        R::TicketCreateFromNote(_) => Action::UserOnly("create tickets from validation notes"),
        R::TicketPrPropose(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        R::TicketPrOpened(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        R::TicketBranchRename(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        // Asking changes the asker's own ticket; answering, the ticket that was asked (CLI-02).
        R::TicketAsk(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        R::MessageAnswer(p) => {
            // A child's question that went to its epic's lead (LEAD-06): only that lead answers it.
            if let Some(q) = questions::find(board, &p.id).await? {
                return Ok(Action::LeadCommand {
                    target: facts(board, &q.child.to_string()).await?,
                    only: questions::ONLY_ITS_LEAD_ANSWERS,
                    started_too: true,
                });
            }
            let message = board.message(&p.id).await?;
            let asker = match &message.from {
                Some(from) => Some(facts(board, &from.to_string()).await?),
                None => None,
            };
            Action::Answer {
                asker,
                asked: facts(board, &message.to.to_string()).await?,
            }
        }
        // Its own narrower rule, not "read anything" (CLI-09, d0087).
        R::TicketTerminal(p) => {
            let target = board.ticket(&p.key).await?;
            Action::ReadTerminal {
                project: target.key.project.to_string(),
                target: facts_of(board, target).await?,
            }
        }
        // A first comment at creation is the user's (RPT-03): the New ticket form's report.
        R::TicketCreate(input) if input.first_comment.is_some() => {
            Action::UserOnly("post a new ticket's first comment")
        }
        R::TicketCreate(input) => Action::CreateTicket {
            epic: epic_of_new(board, input.epic.as_deref()).await?,
        },
        // The lead's own commands (CLI-07): on a child of its epic; the handler refuses the user.
        R::LeadSuggest(p) => Action::LeadCommand {
            target: facts(board, &p.key).await?,
            only: ONLY_A_LEAD_SUGGESTS,
            started_too: false,
        },
        R::DecisionMark(p) => {
            let id: crate::decisions::DecisionId = p.id.parse()?;
            Action::LeadCommand {
                target: facts(board, &id.ticket.to_string()).await?,
                only: ONLY_A_LEAD_MARKS,
                started_too: true,
            }
        }
        R::LeadPassOn(p) => match questions::find(board, &p.id).await? {
            Some(q) => Action::LeadCommand {
                target: facts(board, &q.child.to_string()).await?,
                only: questions::ONLY_A_LEAD_PASSES_ON,
                started_too: true,
            },
            None => return Err(BoardError::NotFound(format!("question {}", p.id))),
        },
        // Its own waiting questions; the handler refuses anyone but a lead.
        R::LeadQuestions => Action::Read,
        R::LeadPrime(_) => Action::Read,
        R::LeadNote(p) => Action::LeadCommand {
            target: facts(board, &p.key).await?,
            only: crate::epic_lead::notes::ONLY_A_LEAD_NOTES,
            started_too: true,
        },
        // Reading marks the notes read: the ticket's own agent (or the user).
        R::TicketNotes(p) => Action::ChangeTicket(facts(board, &p.key).await?),
        R::SuggestionConfirm(_) | R::SuggestionDismiss(_) => {
            Action::UserOnly("confirm or dismiss suggestions")
        }
        R::TicketCreateBatch(p) => batch_action(caller, &p.tickets, board).await?,
        R::DepAdd(p) | R::DepRemove(p) => Action::Dependency {
            child: facts(board, &p.child).await?,
            parent: facts(board, &p.parent).await?,
        },

        // The flow editor's buttons are the user's (FLW-04, FLW-06); proposing is a Project agent's too (FLW-03).
        R::FlowApply(_)
        | R::FlowCancelProposal(_)
        | R::FlowCancelWaiting(_)
        | R::FlowApplyNowPlan(_)
        | R::FlowApplyNow(_)
        | R::FlowProposeFrom(_)
        | R::FlowAskForChanges(_)
        | R::FlowDraftUpdate(_) => Action::UserOnly("change flows"),
        R::FlowShow(_) => Action::Read,
        R::FlowPropose(p) => Action::ProposeFlow {
            project: board.project(&p.project).await?.key,
        },
        // Writing a stage's skill is part of changing the flow (FED-06): the user, or a Project agent of the project,
        // in the folder Otaq gives it.
        R::FlowWriteSkill(p) => Action::ProposeFlow {
            project: board.project(&p.project).await?.key,
        },
        R::ServiceStop => Action::UserOnly("stop the background service"),
        R::DismissPermissionWarning => Action::UserOnly("change onboarding"),
        // Asking spends the user's tokens on their tool (ASK-02) and settling ends agents: the user's clicks only.
        R::OnboardingSources(_) | R::OnboardingFlowSetup(_) => {
            Action::UserOnly("look at a project's Flow setup")
        }
        R::OnboardingAsk(_) => Action::UserOnly("ask an agent tool for a flow"),
        R::OnboardingUseDefault(_) => Action::UserOnly("settle a project's flow"),
        R::DismissTrustNote => Action::UserOnly("dismiss notices"),
        R::NotificationsAskAnswered => Action::UserOnly("answer the notifications ask"),
        R::NotificationsDismissBlocked => Action::UserOnly("dismiss notices"),
        // A pause never lifts on its own, and no agent stops or starts the others (PAU-01, d0112).
        R::AgentsPause(_) | R::AgentsResume(_) => Action::UserOnly("pause or resume agents"),
        R::ProjectAdd(_) | R::ProjectUpdate(_) | R::ProjectRemove(_) => {
            Action::UserOnly("change projects")
        }
        R::TicketMove(_) | R::TicketDrag(_) => Action::UserOnly("move tickets"),
        R::TicketPin(_) | R::TicketUnpin(_) => Action::UserOnly("pin tickets"),
        R::TicketAgentEditsSeen(_) => Action::UserOnly("mark agent edits as seen"),
        R::TicketDelete(_) => Action::UserOnly("delete tickets"),
        R::TicketRetryStart(_) => Action::UserOnly("retry starts"),
        R::EpicMoveTo(_) => Action::UserOnly("move tickets into epics"),
        R::EpicClose(_) => Action::UserOnly("close epics"),
        R::EpicQueue(_) => Action::UserOnly("queue epics"),
        R::EpicTakeBack(_) => Action::UserOnly("take back epics"),
        R::EpicUndoQueue(_) => Action::UserOnly("take back epics"),
        R::TicketClose(_) => Action::UserOnly("close tickets"),
        R::TicketReopen(_) => Action::UserOnly("reopen tickets"),
        R::TicketPark(_) => Action::UserOnly("park tickets"),
        R::TicketResume(_) | R::TicketRetryResume(_) | R::TicketsResumeAll(_) => {
            Action::UserOnly("resume agents")
        }
        R::TicketStartFromScratch(_) => Action::UserOnly("start agents from scratch"),
        R::InboxAnswer(_) => Action::UserOnly("answer agents"),
        R::UsageMarkLimit(_) => Action::UserOnly("mark agents as stopped by the usage limit"),
        R::CleanupDelete(_) => Action::UserOnly("delete leftovers"),
        R::WarmClear(_) => Action::UserOnly("clear warm builds"),
        R::RunStart(_) | R::RunStop(_) => Action::UserOnly("start or stop Run"),
        R::RunSuggest(_) | R::RunSuggestCancel(_) => {
            Action::UserOnly("ask the agent for a run command")
        }
        R::ShellOpen(_) | R::ProjectTerminalOpen(_) | R::ToolTerminalOpen(_) => {
            Action::UserOnly("open shells")
        }
        R::ProjectTerminalClose(_) => Action::UserOnly("close terminals"),
        R::ProjectAgentOpen(_) | R::ProjectAgentResume(_) => {
            Action::UserOnly("open Project agents")
        }
        R::ProjectAgentClose(_) | R::ProjectAgentCloseEnded(_) => {
            Action::UserOnly("close Project agents")
        }
        R::LeadOpen(_) | R::LeadResume(_) => Action::UserOnly("open epic leads"),
        R::ProjectAgentRename(_) => Action::UserOnly("rename Project agents"),
        R::ProjectTerminalDismiss(_) => Action::UserOnly("dismiss the Project terminal's summary"),
        R::TicketSubmitByUser(_) => Action::UserOnly("submit for review in the user's name"),
        R::SettingsSet(_) | R::ProjectSettingsSet(_) => Action::UserOnly("change settings"),
        R::ProjectViewSet(_) => Action::UserOnly("change the board's view"),
        // Agents, the default model and the fallbacks are the user's (AGM-01, AGM-03, AGM-04).
        R::AgentsList
        | R::AgentsAdd(_)
        | R::AgentsEdit(_)
        | R::AgentsDuplicate(_)
        | R::AgentsDelete(_)
        | R::AgentsInUse(_)
        | R::AgentsMoveRole(_)
        | R::AgentsDefaultModel(_)
        | R::AgentsFallback(_)
        | R::AgentsImportSkill(_)
        | R::AgentsReimportSkill(_) => Action::UserOnly("change agents"),
        // What an agent reads is shown on the user's Settings → Agents (AGR-01).
        R::AgentsReads(_) => Action::UserOnly("read what an agent reads"),
        // Agent tools, limits per tool and fallbacks are the user's (d0086).
        R::AgentToolsList
        | R::AgentToolsRecheck(_)
        | R::AgentToolsSetHidden(_)
        | R::AgentToolsSignIn(_)
        | R::AgentToolsInstall(_)
        | R::AgentToolsDisconnect(_) => Action::UserOnly("manage agent tools"),
        R::LimitsResumeTool(_) | R::LimitsUseFallbackNow(_) => {
            Action::UserOnly("resume or use the fallback")
        }
        R::TicketUseFallback(_) => Action::UserOnly("use the fallback"),
        R::RecentItems(_) => Action::UserOnly("use the command palette"),
        R::ReviewCommentAdd(_)
        | R::ReviewCommentReply(_)
        | R::ReviewCommentEdit(_)
        | R::ReviewCommentDelete(_) => Action::UserOnly("write review comments"),
        // Ticket comments (CMT-04): read by anyone; written by the policy's rules, edits judged by their author.
        R::TicketComments(_) | R::TicketReport(_) => Action::Read,
        R::TicketCommentPost(p) => Action::Comment {
            target: facts(board, &p.key).await?,
            own: None,
        },
        R::TicketCommentEdit(crate::protocol::TicketCommentEditParams { key, n, .. })
        | R::TicketCommentDelete(crate::protocol::TicketCommentParams { key, n }) => {
            Action::Comment {
                target: facts(board, key).await?,
                own: Some(board.is_own_comment(caller, key, *n).await?),
            }
        }
        R::TicketFail(_) => Action::UserOnly("send tickets back"),
        R::TicketPass(_) => Action::UserOnly("pass human checks"),
        R::TicketLocalMerge(_) | R::TicketMergeCheckStop(_) => Action::UserOnly("merge tickets"),
        R::TicketMarkMerged(_) | R::TicketMarkClosed(_) => Action::UserOnly("mark pull requests"),
        R::MessageResume(_) => Action::UserOnly("resume finished tickets to answer"),
        R::TicketOpenPr(_) | R::TicketPrConfirm(_) | R::TicketPrCancel(_) | R::TicketPrRetry(_) => {
            Action::UserOnly("open or push pull requests")
        }
        R::TicketSendBackPr(_) => Action::UserOnly("send tickets back"),
        R::TicketAddChildFromPr(_) => Action::UserOnly("add a child from PR comments"),
        R::SessionResize(_) | R::SessionInput(_) | R::SessionDeliverNow(_) | R::SessionFocus(_) => {
            Action::UserOnly("type into sessions")
        }
        R::AppFocus(_) => Action::UserOnly("report the app's window focus"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_tools::Tool;
    use crate::protocol::{AgentAskHumanParams, HookParams, KeyParams, RecordLimitsParams};

    /// T-0489 (d0119): only requests that act count as the agent acting; reads and the status line don't.
    #[test]
    fn which_requests_count_as_the_agent_acting() {
        let key = || KeyParams {
            key: "SHOP-1".into(),
        };
        let limits = Request::UsageRecordLimits(RecordLimitsParams {
            tool: Tool::Claude,
            windows: Vec::new(),
        });
        assert_eq!(
            agent_activity(&limits, &Action::Read),
            AgentActivity::Bookkeeping
        );
        let hook = Request::Hook(HookParams {
            tool: Tool::Claude,
            event: "Stop".into(),
            payload: serde_json::Value::Null,
            role: None,
        });
        assert_eq!(
            agent_activity(&hook, &Action::Read),
            AgentActivity::Bookkeeping
        );
        for read in [
            Request::Ping,
            Request::TicketPrime(key()),
            Request::DiffFiles(key()),
        ] {
            assert_eq!(
                agent_activity(&read, &Action::Read),
                AgentActivity::Reads,
                "{read:?}"
            );
        }
        let ask = Request::AgentAskHuman(AgentAskHumanParams {
            question: "Import closed issues too?".into(),
            options: Vec::new(),
            notes: Vec::new(),
            recommended: None,
        });
        assert_eq!(agent_activity(&ask, &Action::Read), AgentActivity::Acts);
        assert_eq!(
            agent_activity(&Request::TicketNotes(key()), &Action::UserOnly("x")),
            AgentActivity::Acts
        );
    }
}
