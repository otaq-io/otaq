//! Board requests (projects, tickets, epics, dependencies, pins, settings) mapped to [`Board`], and the events
//! their changes publish (SDD §5.4). The service enforces every rule; clients stay thin (d0055).
//!
//! Change events come from comparing the project's tickets before and after a mutation: every ticket that
//! differs gets `ticket_changed`, a changed status also `card_status_changed`, and a ticket that is gone
//! `ticket_deleted`. Projects being added, changed or removed publish `projects_changed`.

use std::collections::HashMap;
use std::pin::Pin;

use serde_json::json;

use super::Shared;
use crate::agent_tools::SetupOutcome;
use crate::board::change_note::change_note;
use crate::board::questions::validate_options;
use crate::board::readiness::project_readiness;
use crate::board::tickets::Ticket;
use crate::board::{Board, BoardError, Refusal};
use crate::caller::Caller;
use crate::domain::{Actor, Kind};
use crate::epic_lead::changes::LeadChange;
use crate::flow::apply::FlowEvent;
use crate::flow::apply_now::ApplyNowAnswer;
use crate::flow::proposal::ProposedBy;
use crate::flow::propose;
use crate::protocol::{
    ErrorBody, ErrorKind, Event, GlobalSettingsReply, ProjectSettingsReply, Reply, Request,
};
use crate::session::SessionKind;
use crate::session::deliveries::DeliveryKind;
use crate::signals::LIVE;

/// Answers a board request. `None` when the request isn't a board request.
/// `origin` is the Project terminal the caller runs in, if any: tickets created from it are recorded for its summary
/// (T-0167).
pub(super) async fn handle(
    request: Request,
    caller: &Caller,
    requested_actor: Option<&str>,
    origin: Option<&str>,
    shared: &Shared,
) -> Option<Result<Reply, ErrorBody>> {
    // The actor recorded on changes: an agent caller always acts as itself (CLI-04).
    let actor = caller.actor(requested_actor);
    let board = &shared.board;
    // Every arm's future is built in its own call and boxed (see [`boxed`]), so this frame holds only the box.
    let answer = match request {
        Request::ProjectList => {
            boxed(move || async move { board.project_infos().await.map(Reply::Projects) })
        }
        Request::ProjectProbe(p) => {
            boxed(move || async move { board.probe_folder(&p.path).await.map(Reply::FolderProbe) })
        }
        Request::ProjectAdd(add) => boxed(move || async move {
            let (project, trust) = board.add_project_trusted(&add).await?;
            shared.events.publish(Event::ProjectsChanged);
            // Onboarding ends with a project: the default model, if a tool got connected since the start (AGM-02).
            super::spawn_default_model(&shared.connected, &shared.store);
            // A failed trust lists the project in the notices (PRJ-08).
            if matches!(trust, Some(SetupOutcome::Failed { .. })) {
                shared.events.publish(Event::TrustChanged);
            }
            board
                .project_info_of(&project.key)
                .await
                .map(|project| Reply::ProjectAdded { project, trust })
        }),
        Request::ProjectUpdate(p) => boxed(move || async move {
            let result = board.update_project(&p.project, &p.update).await;
            if result.is_ok() {
                shared.events.publish(Event::ProjectsChanged);
            }
            match result {
                Ok(project) => board
                    .project_info_of(&project.key)
                    .await
                    .map(Reply::Project),
                Err(e) => Err(e),
            }
        }),
        Request::ProjectRemove(p) => boxed(move || async move {
            let before = board.tickets(&p.key).await.unwrap_or_default();
            let result = board.remove_project(&p.key).await;
            if result.is_ok() {
                publish_changes(shared, &p.key, &before, &[]);
                shared.events.publish(Event::ProjectsChanged);
            }
            result.map(|()| Reply::Ok)
        }),
        Request::ProjectBranches(p) => {
            boxed(move || async move { board.branches(&p.project).await.map(Reply::Branches) })
        }
        Request::ProjectPrProvider(p) => boxed(move || async move {
            board
                .pr_provider(&p.project, &shared.config.tools)
                .await
                .map(Reply::PrProvider)
        }),
        Request::OnboardingSources(p) => boxed(move || async move {
            crate::onboarding::sources(board, &shared.connected, &p.project)
                .await
                .map(Reply::OnboardingSources)
        }),
        Request::BoardSnapshot(p) => {
            boxed(move || async move { board.board_snapshot(&p.project).await.map(Reply::Board) })
        }

        Request::TicketGet(p) => boxed(move || async move {
            board
                .ticket(&p.key)
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketList(p) => {
            boxed(move || async move { board.tickets(&p.project).await.map(Reply::Tickets) })
        }
        Request::DepReadiness(p) => boxed(move || async move {
            board
                .tickets(&p.project)
                .await
                .map(|tickets| Reply::Readiness(project_readiness(&tickets)))
        }),
        Request::SchedulerStatus => {
            boxed(
                move || async move { shared.scheduler.status().await.map(Reply::SchedulerStatus) },
            )
        }
        Request::UsageSummary(p) => boxed(move || async move {
            let mut summary = shared.usage.summary(p.project.as_deref()).await?;
            summary.tool_lines =
                crate::agents::limits::tool_lines(crate::agents::limits::LineSources {
                    board,
                    gate: &shared.gate,
                    plan_limit: &shared.plan_limit,
                    connected: &shared.connected,
                    inbox: &shared.inbox,
                })
                .await?;
            Ok(Reply::Usage(Box::new(summary)))
        }),
        Request::UsageActivity => boxed(move || async move {
            match shared.usage.activity().await {
                Ok(activity) => Ok(Reply::Activity(Box::new(activity))),
                Err(e) => Err(e.into()),
            }
        }),
        Request::UsageRecordLimits(p) => boxed(move || async move {
            match shared
                .plan_limit
                .record_windows(p.tool, &p.windows, crate::store::now_ms())
                .await
            {
                Ok(_) => Ok(Reply::Ok),
                Err(e) => Err(e.into()),
            }
        }),
        Request::AgentsModels(p) => {
            boxed(
                move || async move { Ok(Reply::Models(shared.models.copy(p.tool).map(Box::new))) },
            )
        }
        Request::TicketAgents(p) => boxed(move || async move {
            use crate::agents::facts;
            let answer = match (&p.key, &p.project) {
                (Some(key), _) => match facts::ticket_agents(board, key).await {
                    Ok(mut t) => {
                        let id = shared.board.ticket_id(key).await?;
                        t.tokens = shared.usage.agents_of(&id).await?;
                        Ok(t)
                    }
                    Err(e) => Err(e),
                },
                (None, Some(project)) => {
                    let kind = if p.new_epic { Kind::Epic } else { Kind::Ticket };
                    facts::new_ticket_agents(board, project, kind).await
                }
                (None, None) => Err(BoardError::NotFound("a key or a project".into())),
            };
            answer.map(|t| Reply::TicketAgents(Box::new(t)))
        }),
        Request::BoardAgents(p) => boxed(move || async move {
            let mut agents = crate::agents::facts::board_agents(board, &p.project).await?;
            agents.connected = shared.connected.connected();
            Ok(Reply::BoardAgents(Box::new(agents)))
        }),
        // Stage choices (AGM-08, AGM-09).
        Request::TicketChoices(p) => boxed(move || async move {
            let choices = match (&p.key, &p.project) {
                (Some(key), _) => board.ticket_choices(key).await?,
                (None, Some(project)) => board.new_ticket_choices(project).await?,
                (None, None) => return Err(BoardError::NotFound("a key or a project".into())),
            };
            Ok(Reply::TicketChoices(Box::new(choices)))
        }),
        Request::TicketSetChoice(p) => boxed(move || async move {
            let ticket = board.ticket(&p.key).await?;
            let chooser = super::permissions::chooser(caller, board).await;
            let pick = crate::board::stage_choices::ChoicePick {
                stage: p.stage.clone(),
                choice: p.choice.clone(),
                reason: None,
            };
            let project = ticket.key.project.to_string();
            let writes = board
                .check_choices(
                    &project,
                    Some(&ticket),
                    (ticket.kind, ""),
                    std::slice::from_ref(&pick),
                    &chooser,
                    false,
                )
                .await?;
            if board
                .write_choices(&ticket, &writes, &actor, lead_epic(caller))
                .await?
            {
                publish_ticket_changed(shared, &ticket.key).await;
            }
            Ok(Reply::TicketChoices(Box::new(
                board.ticket_choices(&p.key).await?,
            )))
        }),
        Request::AgentToolsList => {
            boxed(move || async move { Ok(Reply::AgentTools(Box::new(shared.connected.list()))) })
        }
        Request::AgentToolsSetHidden(p) => boxed(move || async move {
            shared
                .connected
                .set_hidden(&shared.store, p.tool, p.hidden)
                .await?;
            Ok(Reply::AgentTools(Box::new(shared.connected.list())))
        }),
        Request::AgentToolsRecheck(p) => boxed(move || async move {
            shared.connected.recheck(p.tool).await?;
            Ok(Reply::AgentTools(Box::new(shared.connected.list())))
        }),
        Request::AgentToolsSignIn(p) => boxed(move || async move {
            let terminal = shared.connected.sign_in(p.tool).await?;
            Ok(Reply::ProjectTerminal(Box::new(terminal)))
        }),
        Request::AgentToolsInstall(p) => boxed(move || async move {
            let terminal = shared.connected.install(p.tool).await?;
            Ok(Reply::ProjectTerminal(Box::new(terminal)))
        }),
        Request::AgentToolsDisconnect(p) => boxed(move || async move {
            shared.connected.disconnect(p.tool).await?;
            Ok(Reply::AgentTools(Box::new(shared.connected.list())))
        }),
        Request::LimitsResumeTool(p) => boxed(move || async move {
            let resumed = crate::agents::limits::resume_tool(
                &shared.plan_limit,
                &shared.validation,
                &shared.gate,
                p.tool,
            )
            .await?;
            for key in &resumed {
                publish_ticket_changed(shared, key).await;
            }
            Ok(Reply::Resumed(resumed))
        }),
        Request::LimitsUseFallbackNow(p) => boxed(move || async move {
            let started = shared.gate.use_fallback_now(p.tool).await?;
            for key in &started {
                publish_ticket_changed(shared, key).await;
            }
            Ok(Reply::StartedOnFallback(started))
        }),
        Request::TicketUseFallback(p) => boxed(move || async move {
            shared.gate.use_fallback(&p.key).await?;
            Ok(Reply::Ok)
        }),
        // What macOS allows Otaq's notifications, the ask and the blocked banner (ATT-13, ATT-14; d0090).
        Request::NotificationsState => boxed(move || async move {
            shared
                .notifications
                .state()
                .await
                .map(Reply::NotificationsState)
        }),
        Request::NotificationsAskAnswered => boxed(move || async move {
            shared.notifications.answered().await?;
            Ok(Reply::Ok)
        }),
        Request::NotificationsDismissBlocked => boxed(move || async move {
            shared.notifications.dismiss_blocked().await?;
            Ok(Reply::Ok)
        }),
        // Pause and Resume (PAU-01, PAU-02, PAU-06, PAU-08; d0112).
        Request::AgentsPause(scope) => boxed(move || async move {
            let counts = shared.pause.pause(&scope).await?;
            Ok(Reply::PauseCounts(Box::new(counts)))
        }),
        Request::AgentsResume(scope) => boxed(move || async move {
            let counts = shared.pause.resume(&scope, &actor).await?;
            Ok(Reply::PauseCounts(Box::new(counts)))
        }),
        Request::AgentsPauseCounts(scope) => boxed(move || async move {
            let counts = shared.pause.counts(&scope).await?;
            Ok(Reply::PauseCounts(Box::new(counts)))
        }),
        Request::RecentItems(p) => boxed(move || async move {
            let items = match p.record {
                Some(used) => {
                    shared
                        .store
                        .record_recent(used, crate::store::now_ms())
                        .await?
                }
                None => shared.store.recent_items().await?,
            };
            Ok(Reply::RecentItems(items))
        }),
        Request::AgentsModelsRefresh(p) => boxed(move || async move {
            Ok(Reply::Models(
                shared.models.refresh(p.tool).await.map(Box::new),
            ))
        }),
        Request::AgentsModelsRetry(p) => boxed(move || async move {
            Ok(Reply::Models(
                shared.models.retry(p.tool).await.map(Box::new),
            ))
        }),
        Request::UsageMarkLimit(p) => boxed(move || async move {
            shared
                .plan_limit
                .mark_idle(&p.key, &actor)
                .await
                .map(Reply::LimitMarked)
        }),
        Request::UsageSessions(p) => boxed(move || async move {
            match board.ticket(&p.key).await {
                Ok(ticket) => shared
                    .usage
                    .sessions_of(&ticket.id)
                    .await
                    .map(Reply::SessionUsage)
                    .map_err(Into::into),
                Err(e) => Err(e),
            }
        }),
        Request::TicketCreate(input) => boxed(move || async move {
            let project = input.project.clone();
            let created = mutate(shared, &project, board.create_ticket(&input, &actor)).await;
            match created {
                Ok(t) => {
                    lead_changed(shared, caller, &t, LeadChange::Created, &actor).await;
                    created_in(shared, origin, std::slice::from_ref(&t))
                        .await
                        .map(|()| Reply::Ticket(Box::new(t)))
                }
                Err(e) => Err(e),
            }
        }),
        Request::TicketCreateBatch(p) => boxed(move || async move {
            let project = p.project.clone();
            // Every ticket's choices are checked before anything is created (AGM-11).
            let chooser = super::permissions::chooser(caller, board).await;
            let mut writes = Vec::with_capacity(p.tickets.len());
            for ticket in &p.tickets {
                let label = format!("the new ticket \"{}\"", ticket.title);
                writes.push(
                    board
                        .check_choices(
                            &project,
                            None,
                            (ticket.kind, &label),
                            &ticket.choices,
                            &chooser,
                            p.by_user,
                        )
                        .await?,
                );
            }
            let created = mutate(shared, &project, async {
                let tickets = board.create_batch(&p.project, &p.tickets, &actor).await?;
                for (ticket, writes) in tickets.iter().zip(&writes) {
                    board
                        .write_choices(ticket, writes, &actor, lead_epic(caller))
                        .await?;
                }
                Ok(tickets)
            })
            .await;
            match created {
                Ok(tickets) => {
                    for t in &tickets {
                        lead_changed(shared, caller, t, LeadChange::Created, &actor).await;
                    }
                    created_in(shared, origin, &tickets)
                        .await
                        .map(|()| Reply::Tickets(tickets))
                }
                Err(e) => Err(e),
            }
        }),
        Request::TicketUpdate(p) => boxed(move || async move {
            let project = project_of(&p.key);
            let before = board.ticket(&p.key).await.ok();
            // The choices are checked before any field changes (AGM-11).
            let writes = match &before {
                Some(ticket) if !p.update.choices.is_empty() => {
                    let chooser = super::permissions::chooser(caller, board).await;
                    board
                        .check_choices(
                            ticket.key.project.as_str(),
                            Some(ticket),
                            (ticket.kind, ""),
                            &p.update.choices,
                            &chooser,
                            p.update.by_user,
                        )
                        .await?
                }
                _ => Vec::new(),
            };
            // An agent's or a lead's edit names it in History (CMT-14); the user's is "You".
            let author = match caller {
                Caller::User | Caller::ProjectAgent { .. } => None,
                _ => board.caller_author(caller, &actor).await.ok(),
            };
            let mut chosen = false;
            let after = mutate(shared, &project, async {
                let after = board
                    .update_ticket_by(&p.key, &p.update, &actor, author.as_ref())
                    .await?;
                chosen = board
                    .write_choices(&after, &writes, &actor, lead_epic(caller))
                    .await?;
                Ok(after)
            })
            .await;
            if chosen && let Ok(after) = &after {
                publish_ticket_changed(shared, &after.key).await;
            }
            // The user's description edit cancels a validation first (VAL-08); then the agent gets the note.
            let after = match (&before, after) {
                (Some(before), Ok(after)) if !actor.is_agent() => {
                    if let Err(e) = Box::pin(shared.validation.ticket_edited(before, &after)).await
                    {
                        tracing::warn!(ticket = %after.key, error = %e, "can't cancel the validation");
                    }
                    board.ticket_by_id(&after.id).await
                }
                (_, after) => after,
            };
            if let (Some(before), Ok(after)) = (&before, &after) {
                queue_change_note(shared, before, after, &actor).await;
            }
            let fields = updated_fields(&p.update);
            // Choices alone are logged as choice changes, not as an edit.
            if let Ok(after) = &after
                && (!fields.is_empty() || p.update.choices.is_empty())
            {
                lead_changed(
                    shared,
                    caller,
                    after,
                    LeadChange::Updated { fields },
                    &actor,
                )
                .await;
            }
            after.map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketMove(p) => boxed(move || async move {
            let project = project_of(&p.key);
            let moved = mutate(
                shared,
                &project,
                board.move_ticket(&p.key, p.target, p.confirmed, &actor),
            )
            .await;
            // Entering Queued only queues: the scheduler starts it once it's ready (its change event wakes it).
            moved.map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketDrag(p) => boxed(move || async move {
            let project = project_of(&p.key);
            let answers = crate::review::drag::DragAnswers {
                note: &p.note,
                report: p.report.as_ref(),
                confirmed: p.confirmed,
                clear_first: p.clear_first,
            };
            mutate(
                shared,
                &project,
                Box::pin(shared.review.drag(
                    &p.key,
                    &p.target,
                    answers,
                    &shared.validation,
                    &actor,
                )),
            )
            .await
            .map(Reply::Returned)
        }),
        Request::TicketHistory(p) => {
            boxed(move || async move { board.history(&p.key).await.map(Reply::History) })
        }
        Request::SessionBeforeRestart(p) => boxed(move || async move {
            crate::recovery::before_restart(
                board,
                shared.usage.adapters(),
                &p.key,
                p.limit
                    .map_or(crate::recovery::DEFAULT_ENTRIES, |n| n as usize),
            )
            .await
            .map(|b| Reply::BeforeRestart(Box::new(b)))
        }),
        Request::DiffFiles(p) => boxed(move || async move {
            board
                .diff_summary(&p.key)
                .await
                .map(|d| Reply::DiffSummary(Box::new(d)))
        }),
        Request::DiffFile(p) => boxed(move || async move {
            match &p.since {
                Some(since) => board.diff_file_since(&p.key, &p.path, since).await,
                None => board.diff_file(&p.key, &p.path, p.child.as_deref()).await,
            }
            .map(|d| Reply::FileDiff(Box::new(d)))
        }),
        Request::FilesTree(p) => boxed(move || async move {
            board
                .files_tree(&p.key, &p.path)
                .await
                .map(|t| Reply::FilesTree(Box::new(t)))
        }),
        Request::FilesSearch(p) => boxed(move || async move {
            board
                .files_search(&p.key, &p.query)
                .await
                .map(|s| Reply::FilesSearch(Box::new(s)))
        }),
        Request::FilesRead(p) => boxed(move || async move {
            board
                .files_read(&p.key, &p.path, p.child.as_deref())
                .await
                .map(|c| Reply::FileContent(Box::new(c)))
        }),
        Request::FilesDiff(p) => boxed(move || async move {
            board
                .files_diff(&p.key, &p.path, p.child.as_deref())
                .await
                .map(|d| Reply::FileDiff(Box::new(d)))
        }),
        Request::FilesChanges(p) => boxed(move || async move {
            board
                .files_changes(&p.key)
                .await
                .map(|c| Reply::FilesChanges(Box::new(c)))
        }),
        Request::FilesProjectTree(p) => boxed(move || async move {
            board
                .files_project_tree(&p.project, &p.path, p.base_only)
                .await
                .map(|t| Reply::ProjectTree(Box::new(t)))
        }),
        Request::FilesProjectSearch(p) => boxed(move || async move {
            board
                .files_project_search(&p.project, &p.query)
                .await
                .map(|s| Reply::ProjectSearch(Box::new(s)))
        }),
        Request::FilesProjectRead(p) => boxed(move || async move {
            board
                .files_project_read(&p.project, &p.path)
                .await
                .map(|f| Reply::ProjectFile(Box::new(f)))
        }),
        Request::FilesProjectDiff(p) => boxed(move || async move {
            board
                .files_project_diff(&p.project, &p.path, &p.ticket)
                .await
                .map(|d| Reply::FileDiff(Box::new(d)))
        }),
        Request::FilesProjectChanges(p) => boxed(move || async move {
            board
                .files_project_changes(&p.project)
                .await
                .map(|c| Reply::ProjectChanges(Box::new(c)))
        }),
        Request::FilesProjectSources(p) => boxed(move || async move {
            board
                .files_project_sources(&p.project)
                .await
                .map(|s| Reply::ProjectSources(Box::new(s)))
        }),
        Request::ReviewCommentAdd(p) => boxed(move || async move {
            let added = board.add_comment(&p).await;
            comment_changed(shared, added).await
        }),
        Request::ReviewCommentReply(p) => boxed(move || async move {
            let added = board.reply_to_comment(&p.id, &p.body).await;
            comment_changed(shared, added).await
        }),
        Request::ReviewCommentEdit(p) => boxed(move || async move {
            let edited = board.edit_comment(&p.id, &p.body).await;
            comment_changed(shared, edited).await
        }),
        Request::ReviewCommentDelete(p) => boxed(move || async move {
            match board.delete_comment(&p.id).await {
                Ok(key) => {
                    publish_ticket_changed(shared, &key).await;
                    Ok(Reply::Ok)
                }
                Err(e) => Err(e),
            }
        }),
        request @ (Request::TicketCommentPost(_)
        | Request::TicketCommentEdit(_)
        | Request::TicketCommentDelete(_)
        | Request::TicketComments(_)
        | Request::TicketReport(_)) => {
            boxed(
                move || async move { ticket_comment_request(shared, caller, &actor, request).await },
            )
        }
        Request::TicketFail(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared
                    .review
                    .fail(&p.key, &p.note, p.report.clone(), p.clear_first, &actor),
            )
            .await
            .map(Reply::Returned)
        }),
        Request::TicketPass(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.pass(&p.key, &shared.validation, &actor),
            )
            .await
            .map(Reply::Passed)
        }),
        Request::TicketConflict(p) => boxed(move || async move {
            match board.ticket(&p.key).await {
                Ok(t) => board.conflict(&t.id).await.map(Reply::Conflict),
                Err(e) => Err(e),
            }
        }),
        Request::TicketApproveCheck(p) => {
            boxed(move || async move { board.approve_check(&p.key).await.map(Reply::ApproveCheck) })
        }
        Request::TicketMergeProposal(p) => {
            boxed(
                move || async move { board.merge_proposal(&p.key).await.map(Reply::MergeProposal) },
            )
        }
        Request::TicketLocalMerge(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                crate::decisions::outcome::local_merge_keeping(
                    &shared.review,
                    &p.key,
                    &p.message,
                    p.approve_anyway,
                    &actor,
                ),
            )
            .await
            .map(Reply::LocalMerge)
        }),
        Request::TicketMergeCheckStop(p) => boxed(move || async move {
            shared
                .review
                .stop_merge_check(&p.key)
                .await
                .map(|()| Reply::Ok)
        }),
        Request::TicketSendBackPr(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.send_back_pr(
                    &p.key,
                    &p.note,
                    p.report.clone(),
                    p.clear_first,
                    &actor,
                ),
            )
            .await
            .map(Reply::Returned)
        }),
        // The PR's comments are fetched only on this click (REV-05, d0017).
        Request::TicketAddChildFromPr(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                crate::review::delivery::add_child_from_pr(
                    board,
                    &p.key,
                    &shared.config.tools.path,
                    &actor,
                ),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        // PR comments are fetched only here, while the agent asks for its feedback (REV-05, d0017).
        Request::ReviewFeedback(p) => boxed(move || async move {
            match board.review_feedback(&p.key).await {
                Ok(mut feedback) => board
                    .pr_feedback(&p.key, &shared.config.tools.path)
                    .await
                    .map(|pr| {
                        feedback.pr = pr;
                        Reply::ReviewFeedback(feedback)
                    }),
                Err(e) => Err(e),
            }
        }),
        Request::ReviewComments(p) => boxed(move || async move {
            board
                .comments(&p.key, p.round)
                .await
                .map(Reply::ReviewComments)
        }),
        Request::TicketAskHuman(p) => {
            boxed(move || async move { ask_human(shared, caller, &p, &actor).await })
        }
        Request::DecisionRecord(p) => {
            boxed(move || async move { record_decision(shared, &p, &actor).await })
        }
        Request::DecisionList(p) => boxed(move || async move {
            crate::decisions::list(board, &p.key)
                .await
                .map(Reply::Decisions)
        }),
        Request::DecisionChange(p) => {
            boxed(move || async move { change_decision(shared, &p, &actor).await })
        }
        Request::DecisionMark(p) => {
            boxed(move || async move { mark_decision(shared, caller, &p, &actor).await })
        }
        Request::LeadSuggest(p) => {
            boxed(move || async move { lead_suggest(shared, caller, &p, &actor).await })
        }
        Request::SuggestionConfirm(p) => {
            boxed(move || async move { confirm_suggestion(shared, &p.id, &actor).await })
        }
        Request::SuggestionDismiss(p) => {
            boxed(move || async move { dismiss_suggestion(shared, &p.id, &actor).await })
        }
        Request::LeadPassOn(p) => {
            boxed(move || async move { pass_on(shared, caller, &p, &actor).await })
        }
        Request::LeadQuestions => {
            boxed(move || async move { lead_questions_waiting(shared, caller).await })
        }
        Request::LeadPrime(p) => boxed(move || async move { epic_prime(shared, &p.key).await }),
        Request::LeadNote(p) => {
            boxed(move || async move { lead_note(shared, caller, &p, &actor).await })
        }
        Request::TicketNotes(p) => boxed(move || async move { read_notes(shared, &p.key).await }),
        Request::TicketStageNext(p) => boxed(move || async move {
            // Not while the lead's note is unread (CLI-08); the user's Move to next stage is another request.
            board.refuse_unread_notes(&p.key).await?;
            refuse_unprimed(board, caller, &p.key).await?;
            let project = project_of(&p.key);
            let before = board.ticket(&p.key).await?;
            // The summary is the agent's comment (CMT-05): checked and its report copied before anything moves.
            let hand_over = stage_end_hand_over(shared, caller, &actor, &before, &p).await?;
            // From M12 the submit starts a validator when validation is on for the project (VAL-01).
            let submitted = mutate(
                shared,
                &project,
                Box::pin(shared.validation.submit(
                    &p.key,
                    hand_over.as_ref(),
                    &actor,
                    crate::domain::ValidationTrigger::Agent,
                )),
            )
            .await;
            let submitted =
                crate::board::hand_over::settled(board, hand_over.as_ref(), submitted).await;
            if submitted.is_ok()
                && let Caller::Agent { session, .. } = caller
            {
                shared.signals.mark_submitted_or_asked(session);
            }
            let ticket = submitted?;
            let flow = board.effective_flow(&project).await?;
            let text = crate::board::review::stage_next_text(&flow, &before, &ticket);
            Ok(Reply::StageEnded(Box::new(
                crate::board::review::StageEnded { ticket, text },
            )))
        }),
        Request::TicketStageDone(p) => boxed(move || async move {
            board.refuse_unread_notes(&p.key).await?;
            refuse_unprimed(board, caller, &p.key).await?;
            let project = project_of(&p.key);
            // From inside the ticket's own agent session, the session ends after its turn (the Done cleanup would
            // otherwise end the command's own session before it answers).
            let own_session = match caller {
                Caller::Agent { session, .. } => Some(session.as_str()),
                _ => None,
            };
            let before = board.ticket(&p.key).await?;
            let hand_over = stage_end_hand_over(shared, caller, &actor, &before, &p).await?;
            let done = mutate(
                shared,
                &project,
                Box::pin(
                    shared
                        .review
                        .stage_done(&p.key, hand_over.as_ref(), own_session, &actor),
                ),
            )
            .await;
            let done = crate::board::hand_over::settled(board, hand_over.as_ref(), done).await;
            if done.is_ok()
                && let Caller::Agent { session, .. } = caller
            {
                shared.signals.mark_submitted_or_asked(session);
            }
            done.map(|ticket| {
                let text = crate::board::review::stage_done_text(&ticket);
                Reply::StageEnded(Box::new(crate::board::review::StageEnded { ticket, text }))
            })
        }),
        Request::ValidationVerdict(p) => boxed(move || async move {
            let Caller::Validator { session, .. } = caller else {
                return Err(BoardError::refused(
                    Refusal::NotAllowed,
                    crate::policy::ONLY_A_VALIDATOR,
                ));
            };
            board.refuse_unprimed(&p.key, session).await?;
            let project = project_of(&p.key);
            // The verdict is the checker's comment (CMT-05), refused whole when too long or its report is missing.
            let ticket = board.ticket(&p.key).await?;
            let author = board.session_author(session).await?;
            let hand_over = board
                .hand_over(&ticket, author, &p.report, p.report_file.clone())
                .await?;
            let verdict = mutate(
                shared,
                &project,
                Box::pin(
                    shared
                        .validation
                        .verdict(&p.key, p.pass, hand_over.as_ref(), session),
                ),
            )
            .await;
            crate::board::hand_over::settled(board, hand_over.as_ref(), verdict)
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketValidationAction(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                // Boxed: the validation futures are large, and this match is every request's frame.
                Box::pin(
                    shared
                        .validation
                        .user_action(&p.key, p.action, &p.note, &actor),
                ),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketValidations(p) => boxed(move || async move {
            match board.ticket(&p.key).await {
                Ok(ticket) => validation_records(shared, &ticket)
                    .await
                    .map(Reply::Validations),
                Err(e) => Err(e),
            }
        }),
        Request::TicketCreateFromNote(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                board.create_from_note(&p.key, &p.note, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketPrimeValidate(p) => boxed(move || async move {
            let context = Box::pin(board.validate_prime(&p.key)).await?;
            record_prime(board, caller, &context.ticket.id).await?;
            Ok(Reply::PrimeValidate(Box::new(context)))
        }),
        Request::TicketPrPropose(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.prs.propose(&p.key, &p.proposal, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketPrOpened(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, shared.prs.opened(&p.key, &p.url, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketOpenPr(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.prs.open(&p.key, p.approve_anyway, p.clear, &actor),
            )
            .await
            .map(Reply::PrOutcome)
        }),
        Request::TicketPrConfirm(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.prs.confirm(&p.key, &p.proposal, p.clear, &actor),
            )
            .await
            .map(Reply::PrOutcome)
        }),
        Request::TicketPrRetry(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, shared.prs.retry(&p.key, p.clear))
                .await
                .map(Reply::PrOutcome)
        }),
        Request::TicketPrCancel(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, shared.prs.cancel(&p.key, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketMarkMerged(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, shared.review.mark_merged(&p.key, &actor))
                .await
                .map(Reply::MarkedMerged)
        }),
        Request::TicketMarkClosed(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.mark_closed(&p.key, p.confirm, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketPrFlow(p) => boxed(move || async move {
            board
                .pr_flow_view(&p.key)
                .await
                .map(|v| Reply::PrFlow(Box::new(v)))
        }),
        Request::TicketPrBranchCheck(p) => boxed(move || async move {
            board
                .check_branch_name(&p.key, &p.name)
                .await
                .map(Reply::BranchCheck)
        }),
        Request::TicketAsk(p) => boxed(move || async move {
            shared
                .messages
                .ask(&p.key, &p.target, &p.question, &actor)
                .await
                .map(|m| Reply::Message(Box::new(m)))
        }),
        Request::MessageAnswer(p) => boxed(move || async move {
            // A child's question to its epic's lead (LEAD-06) is answered by the lead; any other is a message's.
            match crate::epic_lead::questions::find(board, &p.id).await {
                Ok(Some(_)) => lead_questions(shared)
                    .answer(caller, &p.id, &p.answer, &actor)
                    .await
                    .map(|q| Reply::LeadQuestion(Box::new(q))),
                Ok(None) => shared
                    .messages
                    .answer(&p.id, &p.answer, &actor)
                    .await
                    .map(|m| Reply::Message(Box::new(m))),
                Err(e) => Err(e),
            }
        }),
        Request::MessageResume(p) => boxed(move || async move {
            shared
                .messages
                .resume_target(&p.id)
                .await
                .map(|m| Reply::Message(Box::new(m)))
        }),
        Request::TicketQuestions(p) => boxed(move || async move {
            match board.ticket(&p.key).await {
                Ok(t) => board.pending_questions(&t.id).await.map(Reply::Messages),
                Err(e) => Err(e),
            }
        }),
        Request::TicketHumanAnswers(p) => boxed(move || async move {
            match board.ticket(&p.key).await {
                Ok(t) => board
                    .answered_human_questions(&t.id)
                    .await
                    .map(Reply::HumanAnswers),
                Err(e) => Err(e),
            }
        }),
        Request::TicketLeadLog(p) => boxed(move || async move {
            crate::epic_lead::log::of(board, &p.key)
                .await
                .map(|log| Reply::LeadLog(Box::new(log)))
        }),
        Request::TicketMessages(p) => boxed(move || async move {
            match board.ticket(&p.key).await {
                Ok(t) => board.messages_of(&t.id).await.map(Reply::Messages),
                Err(e) => Err(e),
            }
        }),
        Request::TicketBranchRename(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                board.rename_branch(&p.key, &p.name, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketPrime(p) => {
            boxed(move || async move { prime(shared, caller, &p.key).await })
        }
        Request::TicketTerminal(p) => boxed(move || async move { terminal(shared, &p).await }),
        Request::TicketRetryStart(p) => boxed(move || async move {
            let project = project_of(&p.key);
            // Back to Queued; the scheduler's next pass runs the start sequence again.
            let retried = mutate(shared, &project, shared.starter.retry(&p.key, &actor)).await;
            retried.map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketPin(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, board.pin(&p.key, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketUnpin(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, board.unpin(&p.key, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketDeletePreview(p) => {
            boxed(
                move || async move { board.delete_preview(&p.key).await.map(Reply::DeletePreview) },
            )
        }
        Request::TicketDelete(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, board.delete_ticket(&p.key))
                .await
                .map(Reply::DeletePreview)
        }),
        Request::TicketAgentEdits(p) => {
            boxed(move || async move { board.agent_edits(&p.key).await.map(Reply::AgentEdits) })
        }
        Request::TicketAgentEditsSeen(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                board.mark_agent_edits_seen(&p.key, &actor),
            )
            .await
            .map(|()| Reply::Ok)
        }),

        Request::EpicMoveTo(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                board.move_to_epic(&p.key, &p.epic, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::EpicQueue(p) => boxed(move || async move {
            let project = project_of(&p.key);
            match &p.child {
                Some(child) => {
                    let queued = async {
                        let ticket = board.ticket(child).await?;
                        if ticket.epic.as_ref().map(ToString::to_string).as_deref()
                            != Some(p.key.as_str())
                        {
                            return Err(BoardError::refused(
                                Refusal::MoveRefused,
                                format!("{child} isn't a child of {}", p.key),
                            ));
                        }
                        board.queue_epic_child(child, &actor).await
                    };
                    mutate(shared, &project, queued).await
                }
                None => mutate(shared, &project, board.queue_epic(&p.key, &actor)).await,
            }
            .map(Reply::EpicQueued)
        }),
        Request::EpicTakeBack(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.take_back_epic(&p.key, p.confirm, &actor),
            )
            .await
            .map(Reply::EpicTakenBack)
        }),
        Request::EpicUndoQueue(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.board.undo_epic_queue(&p.key, &p.keys, &actor),
            )
            .await
            .map(Reply::EpicQueueUndone)
        }),
        Request::EpicClosePlan(p) => {
            boxed(move || async move { board.epic_close_plan(&p.key).await.map(Reply::ClosePlan) })
        }
        Request::EpicClose(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.close_epic(&p.key, p.confirm, &actor),
            )
            .await
            .map(Reply::ClosePlan)
        }),
        Request::TicketSubmitByUser(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                Box::pin(shared.validation.submit_by_user(&p.key, &p.note, &actor)),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketResume(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, shared.review.resume(&p.key, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketRetryResume(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, shared.review.retry_resume(&p.key, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketStartFromScratch(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.start_from_scratch(&p.key, p.confirm, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketsResumeAll(p) => {
            boxed(move || async move { resume_all(shared, p.project.as_deref(), &actor).await })
        }
        Request::InboxList => {
            boxed(
                move || async move { shared.inbox.list().await.map(|l| Reply::Inbox(Box::new(l))) },
            )
        }
        Request::InboxCounts => {
            boxed(move || async move { shared.inbox.counts().await.map(Reply::InboxCounts) })
        }
        Request::CleanupScan => {
            boxed(move || async move { shared.cleanup.scan().await.map(Reply::CleanupScan) })
        }
        Request::CleanupDelete(p) => boxed(move || async move {
            shared
                .cleanup
                .delete(&p.items, p.confirm)
                .await
                .map(Reply::CleanupOutcome)
        }),
        Request::WarmList => boxed(move || async move {
            shared
                .warm
                .list()
                .await
                .map(|list| Reply::WarmList(Box::new(list)))
        }),
        Request::WarmClear(p) => boxed(move || async move {
            let project = match &p.project {
                Some(key) => Some(board.project(key).await?),
                None => None,
            };
            let repo = match (&project, p.repo) {
                (Some(project), _) => crate::warm::store::repo_name(project),
                (None, Some(repo)) => repo,
                (None, None) => {
                    return Err(BoardError::refused(
                        Refusal::EmptyField,
                        "name the project or the warm build to clear",
                    ));
                }
            };
            Ok(Reply::WarmClear(
                shared.warm.start_clear(project.as_ref(), &repo).await,
            ))
        }),
        Request::InboxAnswer(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.inbox.answer(&p.key, &p.choice, p.clear, &actor),
            )
            .await
            .map(Reply::Delivery)
        }),
        Request::TicketPark(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.park(&p.key, p.confirm, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketReopen(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(shared, &project, board.reopen(&p.key, &actor))
                .await
                .map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::TicketClose(p) => boxed(move || async move {
            let project = project_of(&p.key);
            mutate(
                shared,
                &project,
                shared.review.close(&p.key, p.confirm, &actor),
            )
            .await
            .map(|t| Reply::Ticket(Box::new(t)))
        }),

        Request::DepAdd(p) => boxed(move || async move {
            let project = project_of(&p.child);
            let before = board.ticket(&p.child).await.ok();
            let after = mutate(
                shared,
                &project,
                board.add_dependency(&p.child, &p.parent, &actor),
            )
            .await;
            if let (Some(before), Ok(after)) = (&before, &after) {
                queue_change_note(shared, before, after, &actor).await;
                if let Ok(parent) = board.ticket(&p.parent).await {
                    let change = LeadChange::DepAdded { parent: parent.key };
                    lead_changed(shared, caller, after, change, &actor).await;
                }
            }
            after.map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::DepRemove(p) => boxed(move || async move {
            let project = project_of(&p.child);
            let before = board.ticket(&p.child).await.ok();
            let after = mutate(
                shared,
                &project,
                board.remove_dependency(&p.child, &p.parent, &actor),
            )
            .await;
            if let (Some(before), Ok(after)) = (&before, &after) {
                queue_change_note(shared, before, after, &actor).await;
                if let Ok(parent) = board.ticket(&p.parent).await {
                    let change = LeadChange::DepRemoved { parent: parent.key };
                    lead_changed(shared, caller, after, change, &actor).await;
                }
            }
            after.map(|t| Reply::Ticket(Box::new(t)))
        }),
        Request::DepTree(p) => {
            boxed(move || async move { board.dependency_tree(&p.key).await.map(Reply::DepTree) })
        }
        Request::DepCycles(p) => {
            boxed(
                move || async move { board.dependency_cycles(&p.project).await.map(Reply::Cycles) },
            )
        }

        Request::SettingsGet => boxed(move || async move { global_settings(board).await }),
        Request::SettingsSet(settings) => boxed(move || async move {
            match board.save_global_settings(&settings).await {
                Ok(()) => {
                    // The agent limits apply at once (SCH-03).
                    shared.scheduler.wake();
                    shared.events.publish(Event::SettingsChanged);
                    global_settings(board).await
                }
                Err(e) => Err(e),
            }
        }),
        Request::ProjectSettingsGet(p) => {
            boxed(move || async move { project_settings(board, &p.project).await })
        }
        Request::FlowCurrent(p) => boxed(move || async move {
            Ok(Reply::FlowVersion(Box::new(
                board.current_flow(&p.project).await?,
            )))
        }),
        Request::FlowVersions(p) => {
            boxed(
                move || async move { Ok(Reply::FlowVersions(board.flow_versions(&p.project).await?)) },
            )
        }
        Request::FlowProposal(p) => boxed(move || async move {
            let proposal = board.proposal(&p.project, shared.agents.models()).await?;
            Ok(Reply::FlowProposal(proposal.map(Box::new)))
        }),
        Request::FlowProposeFrom(p) => boxed(move || async move {
            let proposal = board
                .propose_from(&p.project, &p.source, shared.agents.models())
                .await?;
            flow_event(
                shared,
                &proposal.project,
                FlowEvent::Proposed {
                    proposal: proposal.id,
                },
            );
            Ok(Reply::FlowProposal(Some(Box::new(proposal))))
        }),
        Request::FlowApply(p) => boxed(move || async move {
            let by = board.proposed_by(&p.project, p.proposal).await?;
            let applied = board
                .apply_proposal(&p.project, p.proposal, &p.choices, shared.agents.models())
                .await?;
            if let Some(by) = &by {
                let waiting = applied.waiting.as_ref().map(|w| w.summary());
                propose::deliver_answer(
                    &shared.deliveries,
                    by,
                    applied.proposal,
                    &propose::Answer::Applied { waiting },
                )
                .await;
            }
            let key = board.project(&p.project).await?.key;
            flow_event(
                shared,
                &key,
                FlowEvent::ProposalClosed {
                    proposal: applied.proposal,
                },
            );
            if !applied.agents.is_empty() {
                shared.events.publish(Event::AgentsChanged);
            }
            if applied.waiting.is_some() {
                flow_event(shared, &key, FlowEvent::Waiting);
            }
            if let Some(version) = &applied.version {
                flow_event(
                    shared,
                    &key,
                    FlowEvent::Applied {
                        version: version.version,
                    },
                );
            }
            // Use this flow, or a preset applied, settles a Flow setup: the other proposals close and their agents
            // are told and end; the chosen agent stays (ASK-03, ASK-04).
            let keep = match &by {
                Some(ProposedBy::ProjectAgent { agent, .. }) => Some(agent.as_str()),
                _ => None,
            };
            shared.project_agents.settle_flow_setup(&key, keep).await?;
            shared.scheduler.wake();
            Ok(Reply::FlowApplied(Box::new(applied)))
        }),
        Request::FlowCancelProposal(p) => boxed(move || async move {
            let by = board.proposal_row(&p.project).await?.map(|r| r.proposed_by);
            if let Some(id) = board.cancel_proposal(&p.project).await? {
                let key = board.project(&p.project).await?.key;
                flow_event(shared, &key, FlowEvent::ProposalClosed { proposal: id });
                if let Some(by) = &by {
                    let answer = propose::Answer::Cancelled;
                    propose::deliver_answer(&shared.deliveries, by, id, &answer).await;
                }
            }
            Ok(Reply::Ok)
        }),
        Request::FlowAskForChanges(p) => boxed(move || async move {
            let by = board
                .ask_for_changes(&p.project, p.proposal, &p.text)
                .await?;
            let key = board.project(&p.project).await?.key;
            flow_event(
                shared,
                &key,
                FlowEvent::ProposalClosed {
                    proposal: p.proposal,
                },
            );
            let answer = propose::Answer::MoreChanges(p.text.clone());
            propose::deliver_answer(&shared.deliveries, &by, p.proposal, &answer).await;
            Ok(Reply::Ok)
        }),
        Request::FlowShow(p) => boxed(move || async move {
            Ok(Reply::FlowDocument(Box::new(
                board.flow_document(&p.project).await?,
            )))
        }),
        Request::FlowPropose(p) => boxed(move || async move {
            let by = match caller {
                Caller::ProjectAgent { agent, session, .. } => {
                    let proposer = shared.project_agents.get(agent).await?;
                    ProposedBy::ProjectAgent {
                        agent: agent.clone(),
                        session: session.clone(),
                        name: proposer.name,
                        tool: proposer.tool,
                    }
                }
                _ => ProposedBy::User,
            };
            // Outside Flow setup it lands in the draft, and no preview opens (FED-02).
            let setup = board.project(&p.project).await?.flow_setup_since.is_some();
            let proposal = board
                .propose(&p.project, &p.source, &by, shared.agents.models())
                .await?;
            let change = match setup {
                true => FlowEvent::Proposed {
                    proposal: proposal.id,
                },
                false => FlowEvent::DraftChanged {
                    proposal: proposal.id,
                },
            };
            flow_event(shared, &proposal.project, change);
            Ok(Reply::FlowProposal(Some(Box::new(proposal))))
        }),
        Request::FlowDraft(p) => boxed(move || async move {
            Ok(Reply::FlowDraft(Box::new(
                board.draft(&p.project, shared.agents.models()).await?,
            )))
        }),
        Request::FlowDraftUpdate(p) => boxed(move || async move {
            let draft = board
                .update_draft(&p.project, &p.edit, shared.agents.models())
                .await?;
            flow_event(
                shared,
                &draft.proposal.project,
                FlowEvent::DraftChanged {
                    proposal: draft.proposal.id,
                },
            );
            Ok(Reply::FlowDraft(Box::new(draft)))
        }),
        Request::FlowWriteSkill(p) => boxed(move || async move {
            let agent = match caller {
                Caller::ProjectAgent { agent, .. } => Some(shared.project_agents.get(agent).await?),
                _ => None,
            };
            let written = match p.done {
                false => {
                    board
                        .skill_folder(&p.project, &p.stage, agent.as_ref().map(|a| a.id.as_str()))
                        .await?
                }
                true => {
                    let by = agent.map_or_else(|| "you".to_owned(), |a| a.name);
                    board.skill_written(&p.project, &p.stage, &by).await?
                }
            };
            let project = board.project(&p.project).await?.key;
            let draft = board.draft(&project, shared.agents.models()).await?;
            flow_event(
                shared,
                &project,
                FlowEvent::DraftChanged {
                    proposal: draft.proposal.id,
                },
            );
            Ok(Reply::SkillWrite(Box::new(written)))
        }),
        Request::FlowWaiting(p) => boxed(move || async move {
            Ok(Reply::FlowWaiting(
                board.waiting_change(&p.project).await?.map(Box::new),
            ))
        }),
        Request::FlowCancelWaiting(p) => boxed(move || async move {
            if board.cancel_waiting(&p.project).await? {
                let key = board.project(&p.project).await?.key;
                flow_event(shared, &key, FlowEvent::Cancelled);
                shared.scheduler.wake();
            }
            Ok(Reply::Ok)
        }),
        Request::FlowApplyNowPlan(p) => boxed(move || async move {
            Ok(Reply::ApplyNowPlan(
                shared
                    .review
                    .apply_now_plan(&p.project)
                    .await?
                    .map(Box::new),
            ))
        }),
        Request::FlowApplyNow(p) => boxed(move || async move {
            let key = board.project(&p.project).await?.key;
            let answer = mutate(
                shared,
                &key,
                shared.review.apply_now(&p, &shared.validation, &actor),
            )
            .await?;
            if let ApplyNowAnswer::Applied { version, .. } = &answer {
                flow_event(
                    shared,
                    &key,
                    FlowEvent::Applied {
                        version: version.version,
                    },
                );
                shared.scheduler.wake();
            }
            Ok(Reply::AppliedNow(Box::new(answer)))
        }),
        Request::ProjectViewSet(p) => boxed(move || async move {
            board.set_project_view(&p.project, &p.view).await?;
            Ok(Reply::Ok)
        }),
        Request::ProjectSettingsSet(p) => boxed(move || async move {
            match board.save_project_settings(&p.project, &p.settings).await {
                Ok(()) => {
                    // Turning the switch off clears the warm build, after saving (WRM-05); the confirmation is
                    // the app's. A Clear that waits for a copy goes on in the background.
                    if !p.settings.warm_builds {
                        let project = board.project(&p.project).await?;
                        let repo = crate::warm::store::repo_name(&project);
                        shared.warm.start_clear(Some(&project), &repo).await;
                    }
                    project_settings(board, &p.project).await
                }
                Err(e) => Err(e),
            }
        }),

        Request::Ping
        | Request::ServiceStatus
        | Request::StartupReport
        | Request::ServiceStop
        | Request::Subscribe
        | Request::ToolsCheck
        | Request::OnboardingState
        | Request::DismissPermissionWarning
        | Request::DismissTrustNote
        | Request::SessionList(_)
        | Request::SessionAttach(_)
        | Request::SessionDetach(_)
        | Request::SessionResize(_)
        | Request::SessionInput(_)
        | Request::SessionDeliverNow(_)
        | Request::SessionFocus(_)
        | Request::AppFocus(_)
        | Request::Hook(_)
        | Request::RunStart(_)
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
        | Request::ProjectTerminalDismiss(_)
        | Request::ProjectAgentOpen(_)
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
        | Request::LeadList(_)
        | Request::AgentsList
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
        | Request::AgentsReads(_) => return None,
    };
    Some(answer.await.map_err(|e| error_body(&e)))
}

/// A board request's future, boxed.
type Answer<'a> = Pin<Box<dyn Future<Output = Result<Reply, BoardError>> + Send + 'a>>;

/// Builds one arm's future in this call's own frame and boxes it. A debug build gives every future that
/// [`handle`] holds inline its own slot in `handle`'s frame; with about a hundred arms that frame reached 1.5 MB, and
/// on Windows a request with a deep future of its own (a review submit starting a validator) overflowed the 2 MB
/// tokio worker stack (T-0275). Built here, each arm's future lives on the heap and `handle` holds only the box.
/// Tells the app about a change in `project`'s flow (FLW-03…FLW-06).
fn flow_event(shared: &Shared, project: &str, change: FlowEvent) {
    shared.events.publish(Event::FlowChanged {
        project: project.to_owned(),
        change,
    });
}

fn boxed<'a, F>(make: impl FnOnce() -> F) -> Answer<'a>
where
    F: Future<Output = Result<Reply, BoardError>> + Send + 'a,
{
    Box::pin(make())
}

async fn global_settings(board: &Board) -> Result<Reply, BoardError> {
    Ok(Reply::GlobalSettings(Box::new(GlobalSettingsReply {
        settings: board.global_settings().await?,
        problems: board.global_setting_problems().await?,
    })))
}

async fn project_settings(board: &Board, project: &str) -> Result<Reply, BoardError> {
    Ok(Reply::ProjectSettings(Box::new(ProjectSettingsReply {
        settings: board.project_settings(project).await?,
        problems: board.project_setting_problems(project).await?,
        effective: board.effective_settings(project).await?,
    })))
}

/// Resume all, then tells clients about every ticket it resumed.
async fn resume_all(
    shared: &Shared,
    project: Option<&str>,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    let resumed = shared.review.resume_all(project, actor).await?;
    for key in &resumed {
        if let Ok(ticket) = shared.board.ticket(&key.to_string()).await {
            shared.events.publish(Event::TicketChanged {
                project: key.project.to_string(),
                id: ticket.id.clone(),
                key: key.clone(),
            });
            shared.events.publish(Event::CardStatusChanged {
                project: key.project.to_string(),
                id: ticket.id,
                key: key.clone(),
                status: ticket.status,
            });
        }
    }
    Ok(Reply::Resumed(resumed))
}

/// `otaq ask human`: the question (with its options) is stored and the ticket shows Asks you (ATT-01,
/// ATT-12).
async fn ask_human(
    shared: &Shared,
    caller: &Caller,
    p: &crate::protocol::AskHumanParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    if p.question.trim().is_empty() {
        return Err(BoardError::refused(
            Refusal::BadQuestion,
            "Write the question on stdin",
        ));
    }
    validate_options(&p.options, p.recommended)?;
    crate::board::questions::validate_notes(&p.options, &p.notes)?;
    let board = &shared.board;
    let ticket = board.ticket(&p.key).await?;
    // During Open PR / Push to PR the skill reports what went wrong (a rejected push, a missing login) to the
    // dialog instead of the board (REV-10).
    if board.pr_flow(&ticket.id).await?.is_some() {
        board
            .report_pr_problem(&ticket.id, p.question.trim())
            .await?;
        return board
            .ticket_by_id(&ticket.id)
            .await
            .map(|t| Reply::Ticket(Box::new(t)));
    }
    let question = board
        .store_question(&ticket.id, &p.question, &p.options, &p.notes, p.recommended)
        .await?;
    // An epic child's question goes to its live lead first (LEAD-06); otherwise the ticket asks the user.
    if !lead_questions(shared)
        .route(caller, &ticket, &question, actor)
        .await?
    {
        shared
            .signals
            .ask_human(caller, &ticket.id, p.question.trim())
            .await?;
    }
    board
        .ticket_by_id(&ticket.id)
        .await
        .map(|t| Reply::Ticket(Box::new(t)))
}

/// `otaq decision record` (DEC-02, d0084): stores the decision and its History line. The ticket's state doesn't
/// change and nobody is notified; only clients showing its History or Decisions refresh.
async fn record_decision(
    shared: &Shared,
    p: &crate::protocol::DecisionRecordParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    let decision = crate::decisions::record(&shared.board, &p.key, &p.decision, actor).await?;
    publish_ticket_changed(shared, &decision.id.ticket).await;
    Ok(Reply::Decision(Box::new(decision)))
}

/// Change… on a recorded decision (DEC-06, d0084): Send back with the text, or a fix child in the epic's review.
async fn change_decision(
    shared: &Shared,
    p: &crate::protocol::DecisionChangeParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    use crate::decisions::outcome;
    let project =
        p.id.parse::<crate::decisions::DecisionId>()?
            .ticket
            .project
            .to_string();
    let changed = match (&p.text, &p.title) {
        (Some(text), _) => {
            mutate(
                shared,
                &project,
                outcome::change_and_send_back(&shared.review, &p.id, text, p.clear_first, actor),
            )
            .await?
        }
        (None, Some(title)) => {
            mutate(
                shared,
                &project,
                outcome::change_with_fix_child(
                    &shared.board,
                    &p.id,
                    title,
                    p.description.as_deref().unwrap_or_default(),
                    actor,
                ),
            )
            .await?
        }
        (None, None) => {
            return Err(BoardError::refused(
                crate::board::Refusal::EmptyField,
                "Change\u{2026} needs the text to send back, or the fix child's title",
            ));
        }
    };
    Ok(Reply::DecisionChanged(changed))
}

/// `otaq show <KEY> --terminal` (SHL-10, d0087): the ticket's agent session (running, or the last ended one still
/// in memory), or with `--stage` the session of the agent running that stage (FLW-20), replayed into plain text.
/// Read-only; nothing is kept between requests. While an agent check runs, the agent's text starts with a line
/// pointing at its `--stage`.
async fn terminal(
    shared: &Shared,
    p: &crate::protocol::TerminalParams,
) -> Result<Reply, BoardError> {
    use crate::session::screen;
    let ticket = shared.board.ticket(&p.key).await?;
    let key = &ticket.key;
    let (kind, stage) = match &p.stage {
        Some(name) => {
            let (kind, stage) = stage_session(&shared.board, &ticket, name).await?;
            (kind, Some(stage.name))
        }
        None => (SessionKind::Agent, None),
    };
    let check = shared
        .sessions
        .running_of(&ticket.id, SessionKind::Validator);
    let (session, bytes, size) = if kind == SessionKind::Validator {
        match check
            .as_ref()
            .and_then(|_| shared.sessions.output_of(&ticket.id, kind))
        {
            Some(output) => output,
            None => {
                return Err(BoardError::refused(
                    Refusal::SessionEnded,
                    format!(
                        "{key}'s {} check isn't running",
                        stage.as_deref().unwrap_or_default()
                    ),
                ));
            }
        }
    } else {
        match shared.sessions.output_of(&ticket.id, kind) {
            Some(output) => output,
            None => {
                // Output never goes to disk (d0045): a stored session means a restart cleared it.
                let stored: Option<(i64,)> = sqlx::query_as(
                    "SELECT 1 FROM sessions WHERE ticket_id = ? AND kind = ? LIMIT 1",
                )
                .bind(&ticket.id)
                .bind(kind.as_str())
                .fetch_optional(shared.board.store().pool())
                .await?;
                let message = match stored {
                    Some(_) => format!("{key}'s terminal output isn't kept after a restart."),
                    None => format!("{key} has no terminal output: its agent hasn't started."),
                };
                return Err(BoardError::refused(Refusal::SessionEnded, message));
            }
        }
    };
    let mut text = screen::render(&bytes, size);
    if p.stage.is_none()
        && check.is_some()
        && let Some(id) = ticket.place.stage()
    {
        let flow = shared.board.effective_flow(key.project.as_str()).await?;
        let name = flow.stage(id).map_or(id, |s| s.name.as_str());
        text = format!("{key} is in {name} \u{b7} its terminal: --stage {id}\n{text}");
    }
    Ok(Reply::TicketTerminal(Box::new(
        crate::protocol::TicketTerminal {
            key: key.clone(),
            session: session.id,
            text,
        },
    )))
}

/// `--stage <name>` on `otaq show --terminal` and `otaq attach` (FLW-20, d0108): the stage `name` names in the
/// ticket's flow (its id, or its name ignoring case) and the kind of the session its agent runs in: an Agent stage's
/// or a Delivery's is the ticket's agent session (an epic's delivery session), an agent check's is the check's.
/// Refused unless the ticket is in that stage; a check by you has no agent.
pub(super) async fn stage_session(
    board: &Board,
    ticket: &Ticket,
    name: &str,
) -> Result<(SessionKind, crate::flow::Stage), BoardError> {
    let project = ticket.key.project.as_str();
    let flow = board.effective_flow(project).await?;
    let wanted = name.trim();
    let folded = wanted.to_lowercase();
    let Some(stage) = flow
        .stages
        .iter()
        .find(|s| s.id == wanted || s.name.to_lowercase() == folded)
    else {
        let names: Vec<&str> = flow.stages.iter().map(|s| s.name.as_str()).collect();
        return Err(BoardError::NotFound(format!(
            "{project}'s flow has no stage {wanted} (its stages: {})",
            names.join(", ")
        )));
    };
    let key = &ticket.key;
    if ticket.place.stage() != Some(stage.id.as_str()) {
        return Err(BoardError::refused(
            Refusal::SessionEnded,
            format!("{key} isn't in {}", stage.name),
        ));
    }
    let kind = match stage.kind {
        crate::flow::StageKind::Check {
            by: crate::flow::CheckBy::You,
        } => {
            return Err(BoardError::refused(
                Refusal::SessionEnded,
                format!("{key}'s {} is your check: no agent runs it", stage.name),
            ));
        }
        crate::flow::StageKind::Check { .. } => SessionKind::Validator,
        crate::flow::StageKind::Delivery if ticket.is_epic() => SessionKind::Delivery,
        crate::flow::StageKind::Agent | crate::flow::StageKind::Delivery => SessionKind::Agent,
    };
    Ok((kind, stage.clone()))
}

/// `otaq prime`: the ticket's context. Its own agent reads the unread lead notes in it, so they are marked read
/// (LEAD-10); anyone else's prime leaves them unread. Skills are named for the caller's session's tool (CLI-01,
/// ADP-05); a caller with no agent session (by hand, with `OTAQ_TICKET`) gets a new session's tool.
async fn prime(shared: &Shared, caller: &Caller, key: &str) -> Result<Reply, BoardError> {
    let session = match caller {
        Caller::Agent { session, .. }
        | Caller::Validator { session, .. }
        | Caller::ProjectAgent { session, .. }
        | Caller::EpicLead { session, .. } => Some(session),
        Caller::User => None,
    };
    let tool = session
        .and_then(|s| shared.sessions.info(s).ok())
        .and_then(|info| info.tool);
    let mut context = shared
        .board
        .prime_context(key, shared.adapters.of_session(tool))
        .await?;
    context.choices = crate::prime::reader_choices(
        &shared.board,
        crate::prime::ChoicesFor::Ticket(&context.ticket),
        &shared.models,
    )
    .await?;
    if let Caller::Agent { ticket_id, .. } = caller
        && *ticket_id == context.ticket.id
        && !context.unread_notes.is_empty()
    {
        let ids: Vec<String> = context.unread_notes.iter().map(|n| n.id.clone()).collect();
        crate::epic_lead::notes::mark_read(
            &shared.board,
            &ids,
            crate::epic_lead::notes::ReadHow::Prime,
        )
        .await?;
        publish_ticket_changed(shared, &context.ticket.key).await;
    }
    record_prime(&shared.board, caller, &context.ticket.id).await?;
    Ok(Reply::Prime(Box::new(context)))
}

/// The ticket's own agent or checker read its prime: its stage run is primed (CMT-08). Anyone else's prime (the user,
/// another ticket's agent, a lead) records nothing.
async fn record_prime(board: &Board, caller: &Caller, ticket_id: &str) -> Result<(), BoardError> {
    match caller {
        Caller::Agent {
            ticket_id: own,
            session,
            ..
        }
        | Caller::Validator {
            ticket_id: own,
            session,
            ..
        } if own == ticket_id => board.record_prime(ticket_id, session).await,
        _ => Ok(()),
    }
}

/// The read gate on an agent's `stage next` / `stage done` (CMT-08): the user's own (a terminal of theirs) isn't
/// gated.
async fn refuse_unprimed(board: &Board, caller: &Caller, key: &str) -> Result<(), BoardError> {
    match caller {
        Caller::Agent { session, .. } => board.refuse_unprimed(key, session).await,
        _ => Ok(()),
    }
}

/// `otaq note <KEY> [--view "…"] --stdin` (LEAD-10): the lead's unread note for a running child of its epic, with
/// History on both. A child already idle between turns gets it typed in at once (d0048). A child whose first stop
/// waits for the lead gets it as Answer the agent (LEAD-07).
async fn lead_note(
    shared: &Shared,
    caller: &Caller,
    p: &crate::protocol::LeadNoteParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    use crate::epic_lead::notes;
    let (epic_id, _) = the_lead(shared, caller, notes::ONLY_A_LEAD_NOTES).await?;
    let lead_id = caller
        .project_agent()
        .map(|(agent, _)| agent)
        .unwrap_or_default();
    let note = notes::leave(
        &shared.board,
        epic_id,
        lead_id,
        &p.key,
        &p.text,
        p.view.as_deref(),
        actor,
    )
    .await?;
    // A child whose first stop waits for the lead: the note answers it (LEAD-07).
    let answered = stuck(shared).answer(&note, actor).await?;
    let note = if answered {
        notes::get(&shared.board, &note.id).await?
    } else {
        note
    };
    let child = shared.board.ticket_by_id(&note.child_id).await?;
    if !answered
        && child.status == crate::domain::Status::Idle
        && let Err(e) = notes::type_in(
            &shared.board,
            &shared.sessions,
            &shared.deliveries,
            &child.id,
        )
        .await
    {
        tracing::warn!(child = %child.key, error = %e, "can't type the lead's note in");
    }
    publish_ticket_changed(shared, &note.child).await;
    publish_ticket_changed(shared, &note.epic).await;
    Ok(Reply::LeadNote(Box::new(note)))
}

/// `otaq notes` (LEAD-10): the ticket's unread lead notes, now read; the card's marker clears.
async fn read_notes(shared: &Shared, key: &str) -> Result<Reply, BoardError> {
    let notes = crate::epic_lead::notes::read_unread(&shared.board, key).await?;
    if let Some(note) = notes.first() {
        publish_ticket_changed(shared, &note.child).await;
    }
    Ok(Reply::LeadNotes(notes))
}

/// The epic lead caller's epic id and name ("Wishlist lead"); refused for anyone else with `only`.
async fn the_lead<'a>(
    shared: &Shared,
    caller: &'a Caller,
    only: &str,
) -> Result<(&'a str, String), BoardError> {
    let Caller::EpicLead { epic_id, .. } = caller else {
        return Err(BoardError::refused(Refusal::NotAllowed, only));
    };
    let epic = shared.board.ticket_by_id(epic_id).await?;
    Ok((epic_id, crate::project_agents::lead::lead_name(&epic.title)))
}

/// Stuck children and their epic's lead (LEAD-07), over the service's parts.
fn stuck(shared: &Shared) -> crate::epic_lead::stuck::Stuck {
    crate::epic_lead::stuck::Stuck::new(crate::epic_lead::stuck::StuckParts {
        board: shared.board.clone(),
        leads: shared.project_agents.leads(),
        signals: shared.signals.clone(),
        validation: shared.validation.clone(),
        deliveries: shared.deliveries.clone(),
        sessions: shared.sessions.clone(),
        events: shared.events.clone(),
    })
}

/// Children's questions to their epic's lead (LEAD-06), over the service's parts.
fn lead_questions(shared: &Shared) -> crate::epic_lead::questions::LeadQuestions {
    crate::epic_lead::questions::LeadQuestions::new(
        shared.board.clone(),
        shared.signals.clone(),
        shared.project_agents.leads(),
        shared.deliveries.clone(),
        shared.sessions.clone(),
        shared.events.clone(),
    )
}

/// `otaq ask human --pass-on` (LEAD-06): the lead passes a child's question on; the child asks the user.
async fn pass_on(
    shared: &Shared,
    caller: &Caller,
    p: &crate::protocol::PassOnParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    lead_questions(shared)
        .pass_on(
            caller,
            &p.id,
            &p.options,
            &p.notes,
            p.recommended,
            &p.reason,
            actor,
        )
        .await
        .map(|q| Reply::LeadQuestion(Box::new(q)))
}

/// `otaq questions` from a lead: its children's questions waiting for it.
async fn lead_questions_waiting(shared: &Shared, caller: &Caller) -> Result<Reply, BoardError> {
    let Caller::EpicLead { epic_id, .. } = caller else {
        return Err(BoardError::refused(
            Refusal::NotAllowed,
            "Only an epic's lead has questions waiting for it here; a ticket's agent runs `questions` on its ticket",
        ));
    };
    crate::epic_lead::questions::waiting(&shared.board, epic_id)
        .await
        .map(Reply::LeadQuestions)
}

/// `otaq prime --epic` (LEAD-03): the epic lead's context.
async fn epic_prime(shared: &Shared, key: &str) -> Result<Reply, BoardError> {
    let mut context = crate::epic_lead::prime::context(&shared.board, key).await?;
    context.choices = crate::prime::reader_choices(
        &shared.board,
        crate::prime::ChoicesFor::NewTickets(context.epic.key.project.as_str()),
        &shared.models,
    )
    .await?;
    Ok(Reply::EpicPrime(Box::new(context)))
}

/// `otaq decision mark` (DEC-04, d0083): the lead's mark on a child's decision; the policy checked the child is its
/// epic's. Only clients showing the child's History or Decisions refresh.
async fn mark_decision(
    shared: &Shared,
    caller: &Caller,
    p: &crate::protocol::DecisionMarkParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    let (_, lead) = the_lead(shared, caller, crate::policy::ONLY_A_LEAD_MARKS).await?;
    let decision =
        crate::decisions::mark::mark(&shared.board, &p.id, &p.source, &p.text, &lead, actor)
            .await?;
    publish_ticket_changed(shared, &decision.id.ticket).await;
    Ok(Reply::Decision(Box::new(decision)))
}

/// `otaq suggest close|delete` (LEAD-08): stores the suggestion; the inbox picks it up (no notification, no status
/// change). The child's card doesn't change.
async fn lead_suggest(
    shared: &Shared,
    caller: &Caller,
    p: &crate::protocol::SuggestParams,
    actor: &Actor,
) -> Result<Reply, BoardError> {
    let (epic_id, lead) = the_lead(shared, caller, crate::policy::ONLY_A_LEAD_SUGGESTS).await?;
    let suggestion = crate::epic_lead::suggest::suggest(
        &shared.board,
        epic_id,
        &p.key,
        p.kind,
        &p.reason,
        &lead,
        actor,
    )
    .await?;
    // The inbox derives its items again on the child's change event.
    publish_ticket_changed(shared, &suggestion.child).await;
    Ok(Reply::Suggestion(Box::new(suggestion)))
}

/// Close… / Delete… on a suggestion (LEAD-08): the usual close (d0044) or delete (d0060) of the child as the user,
/// then the suggestion is confirmed with its History lines.
async fn confirm_suggestion(shared: &Shared, id: &str, actor: &Actor) -> Result<Reply, BoardError> {
    use crate::epic_lead::suggest::{self, SuggestionKind, SuggestionState};
    let suggestion = suggest::open_one(&shared.board, id).await?;
    let key = suggestion.child.to_string();
    let project = project_of(&key);
    match suggestion.kind {
        SuggestionKind::Close => {
            Box::pin(mutate(
                shared,
                &project,
                shared.review.close(&key, true, actor),
            ))
            .await?;
        }
        SuggestionKind::Delete => {
            Box::pin(mutate(shared, &project, shared.board.delete_ticket(&key))).await?;
        }
    }
    suggest::resolve(
        &shared.board,
        &suggestion,
        SuggestionState::Confirmed,
        actor,
    )
    .await?;
    publish_ticket_changed(shared, &suggestion.epic).await;
    // A delete took the row with the child: the reply is the suggestion as it ended.
    let confirmed = match suggest::get(&shared.board, id).await {
        Ok(confirmed) => confirmed,
        Err(_) => suggest::Suggestion {
            state: SuggestionState::Confirmed,
            resolved_by: Some(actor.clone()),
            resolved_at: Some(crate::store::now_ms()),
            ..suggestion
        },
    };
    Ok(Reply::Suggestion(Box::new(confirmed)))
}

/// Dismiss on a suggestion (LEAD-08): it doesn't wake the lead; `prime --epic` lists it as dismissed.
async fn dismiss_suggestion(shared: &Shared, id: &str, actor: &Actor) -> Result<Reply, BoardError> {
    use crate::epic_lead::suggest::{self, SuggestionState};
    let suggestion = suggest::open_one(&shared.board, id).await?;
    suggest::resolve(
        &shared.board,
        &suggestion,
        SuggestionState::Dismissed,
        actor,
    )
    .await?;
    publish_ticket_changed(shared, &suggestion.child).await;
    publish_ticket_changed(shared, &suggestion.epic).await;
    Ok(Reply::Suggestion(Box::new(
        suggest::get(&shared.board, id).await?,
    )))
}

/// The fields an `update` set, as History names them.
fn updated_fields(update: &crate::board::tickets::TicketUpdate) -> Vec<&'static str> {
    [
        (update.title.is_some(), "title"),
        (update.body.is_some(), "description"),
        (update.priority.is_some(), "priority"),
        (update.base_branch.is_some(), "base branch"),
        (update.external_id.is_some(), "External ID"),
    ]
    .into_iter()
    .filter_map(|(set, name)| set.then_some(name))
    .collect()
}

/// An epic lead's change of a child goes into the child's and the epic's History (CLI-07, d0083); other callers'
/// changes are left as they are.
async fn lead_changed(
    shared: &Shared,
    caller: &Caller,
    child: &Ticket,
    change: LeadChange,
    actor: &Actor,
) {
    let Caller::EpicLead { epic_id, .. } = caller else {
        return;
    };
    let Ok(epic) = shared.board.ticket_by_id(epic_id).await else {
        return;
    };
    let lead = crate::project_agents::lead::lead_name(&epic.title);
    if let Err(e) =
        crate::epic_lead::changes::record(&shared.board, &lead, epic_id, child, &change, actor)
            .await
    {
        tracing::warn!(child = %child.key, error = %e, "can't record the lead's change");
    }
    publish_ticket_changed(shared, &epic.key).await;
}

/// SES-08: a user's change to a ticket whose agent is at work queues a note with what changed. Agent edits and
/// tickets that aren't working (Queued, Parked, Backlog, …) get nothing.
async fn queue_change_note(shared: &Shared, before: &Ticket, after: &Ticket, actor: &Actor) {
    let validating = matches!(
        after.status,
        crate::domain::Status::Checking | crate::domain::Status::CheckerNeedsYou
    );
    if actor.is_agent() || !(LIVE.contains(&after.status) || validating) {
        return;
    }
    let Some(session) = shared.sessions.running_of(&after.id, SessionKind::Agent) else {
        return;
    };
    let Some(note) = change_note(before, after) else {
        return;
    };
    if let Err(e) = shared
        .deliveries
        .queue_delivery(&session.id, DeliveryKind::TicketChange, &note)
        .await
    {
        tracing::warn!(ticket = %after.key, error = %e, "can't queue the ticket-change note");
    }
}

/// A comment was added or changed: its ticket's pending counter changed.
async fn comment_changed(
    shared: &Shared,
    result: Result<crate::board::comments::ReviewComment, BoardError>,
) -> Result<Reply, BoardError> {
    let comment = result?;
    if let Ok((key, _)) = shared.board.comment(&comment.id).await {
        publish_ticket_changed(shared, &key).await;
    }
    Ok(Reply::ReviewComment(comment))
}

/// The comment an agent's `stage next` / `stage done` hands over (CMT-05, CMT-06): its summary with the
/// `--report-file`, by the calling session's agent (the user's own terminal: the user). Refused before anything moves.
async fn stage_end_hand_over(
    shared: &Shared,
    caller: &Caller,
    actor: &Actor,
    ticket: &crate::board::tickets::Ticket,
    p: &crate::protocol::StageEndParams,
) -> Result<Option<crate::board::hand_over::HandOver>, BoardError> {
    let board = &shared.board;
    let author = board.caller_author(caller, actor).await?;
    board
        .hand_over(ticket, author, &p.summary, p.report_file.clone())
        .await
}

/// The ticket comment requests (CMT-01…CMT-06, d0109); the policy judged the caller first (CMT-04). Each change
/// publishes `ticket_changed`.
async fn ticket_comment_request(
    shared: &Shared,
    caller: &Caller,
    actor: &Actor,
    request: Request,
) -> Result<Reply, BoardError> {
    let board = &shared.board;
    let (key, reply) = match request {
        Request::TicketComments(p) => {
            return board
                .ticket_comments(&p.key)
                .await
                .map(Reply::TicketComments);
        }
        Request::TicketReport(p) => {
            let report = board.ticket_report(&p.key, p.n).await?;
            // A deleted comment's report is the user's (History); agents that start later never read it (Q375).
            if report.deleted && !matches!(caller, Caller::User) {
                return Err(BoardError::refused(
                    Refusal::NotAllowed,
                    format!("Comment {} on {} was deleted.", report.n, report.key),
                ));
            }
            return Ok(Reply::TicketReport(Box::new(report)));
        }
        Request::TicketCommentPost(p) => {
            let author = board.caller_author(caller, actor).await?;
            let posted = board
                .post_ticket_comment(&p.key, &author, &p.body, p.report, actor)
                .await?;
            (p.key, posted)
        }
        Request::TicketCommentEdit(p) => {
            let edited = board
                .edit_ticket_comment(&p.key, p.n, &p.body, actor)
                .await?;
            (p.key, edited)
        }
        Request::TicketCommentDelete(p) => {
            let deleted = board.delete_ticket_comment(&p.key, p.n, actor).await?;
            (p.key, deleted)
        }
        _ => unreachable!("only ticket comment requests come here"),
    };
    if let Ok(ticket) = board.ticket(&key).await {
        publish_ticket_changed(shared, &ticket.key).await;
    }
    Ok(Reply::TicketComment(Box::new(reply)))
}

/// The ticket's validation rounds with each validator's tokens (History, VAL-12).
async fn validation_records(
    shared: &Shared,
    ticket: &Ticket,
) -> Result<Vec<crate::board::validation::ValidationRecord>, BoardError> {
    let mut records = shared.board.validations(&ticket.key.to_string()).await?;
    let usage = shared.usage.sessions_of(&ticket.id).await?;
    for record in &mut records {
        record.tokens = record
            .session
            .as_ref()
            .and_then(|s| usage.iter().find(|u| &u.session == s))
            .and_then(|u| u.tokens);
    }
    Ok(records)
}

/// Tells clients a ticket changed in a way its row doesn't show (its review comments).
async fn publish_ticket_changed(shared: &Shared, key: &crate::domain::TicketKey) {
    if let Ok(ticket) = shared.board.ticket(&key.to_string()).await {
        shared.events.publish(Event::TicketChanged {
            project: ticket.key.project.to_string(),
            id: ticket.id,
            key: ticket.key,
        });
    }
}

/// The epic of an epic lead caller, whose choices for a child are logged on the epic too.
fn lead_epic(caller: &Caller) -> Option<&str> {
    match caller {
        Caller::EpicLead { epic_id, .. } => Some(epic_id),
        _ => None,
    }
}

/// The project part of a ticket key (`SHOP-18` → `SHOP`). Aliases keep their project, so this holds for them too.
fn project_of(key: &str) -> String {
    key.split_once('-').map_or(key, |(p, _)| p).to_owned()
}

/// Runs a mutation of one project's tickets and publishes the changes it made.
async fn mutate<T>(
    shared: &Shared,
    project: &str,
    change: impl Future<Output = Result<T, BoardError>>,
) -> Result<T, BoardError> {
    let before = shared.board.tickets(project).await.unwrap_or_default();
    let result = change.await?;
    match shared.board.tickets(project).await {
        Ok(after) => publish_changes(shared, project, &before, &after),
        Err(e) => tracing::warn!(error = %e, project, "can't read the board after a change"),
    }
    Ok(result)
}

fn publish_changes(shared: &Shared, project: &str, before: &[Ticket], after: &[Ticket]) {
    let old: HashMap<&str, &Ticket> = before.iter().map(|t| (t.id.as_str(), t)).collect();
    for ticket in after {
        let previous = old.get(ticket.id.as_str());
        if previous.is_some_and(|p| *p == ticket) {
            continue;
        }
        shared.events.publish(Event::TicketChanged {
            project: project.to_owned(),
            id: ticket.id.clone(),
            key: ticket.key.clone(),
        });
        if previous.is_none_or(|p| p.status != ticket.status) {
            shared.events.publish(Event::CardStatusChanged {
                project: project.to_owned(),
                id: ticket.id.clone(),
                key: ticket.key.clone(),
                status: ticket.status,
            });
        }
    }
    for gone in before
        .iter()
        .filter(|b| !after.iter().any(|a| a.id == b.id))
    {
        shared.events.publish(Event::TicketDeleted {
            project: project.to_owned(),
            id: gone.id.clone(),
            key: gone.key.clone(),
        });
    }
}

/// A board error as the protocol's error body: refusals keep their code in `data.refusal`.
/// Tickets created from a Project terminal are recorded for its summary, and their cards refreshed (T-0167).
async fn created_in(
    shared: &Shared,
    origin: Option<&str>,
    tickets: &[crate::board::tickets::Ticket],
) -> Result<(), BoardError> {
    let Some(session) = origin else {
        return Ok(());
    };
    let ids: Vec<String> = tickets.iter().map(|t| t.id.clone()).collect();
    shared.board.mark_created_in(&ids, session).await?;
    for t in tickets {
        shared.events.publish(Event::TicketChanged {
            project: t.key.project.to_string(),
            id: t.id.clone(),
            key: t.key.clone(),
        });
    }
    Ok(())
}

pub(super) fn error_body(e: &BoardError) -> ErrorBody {
    match e {
        BoardError::Refused { code, message } => {
            let mut body = ErrorBody::new(ErrorKind::Refused, message.clone());
            body.data = Some(json!({ "refusal": code }));
            body
        }
        BoardError::NotFound(message) => ErrorBody::new(ErrorKind::NotFound, message.clone()),
        BoardError::Git(_) | BoardError::Store(_) => {
            tracing::error!(error = %e, "board request failed");
            ErrorBody::new(ErrorKind::Internal, e.to_string())
        }
    }
}
