//! `otaq helper start <n>` (d0133): a helper's branch, worktree, port, skills and session, in that order. Every step
//! reuses what exists (a start that failed half-way runs again with the same command, SES-02); a helper held for its
//! tool's limit has nothing made yet and starts once the hold lifts ([`Helpers::start_held`]).

use std::path::{Path, PathBuf};

use super::store::{self, NewHelper};
use super::{
    Helper, HelperReply, HelperStatus, Helpers, branch_name, first_prompt, folder_name,
    piece_env_var, piece_label, ticket_branch, ticket_worktree,
};
use crate::agent_tools::{FirstPrompt, LaunchRequest, LaunchRole};
use crate::agents::{RunsOn, SessionRole, resolve};
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::caller::Caller;
use crate::naming;
use crate::session::{SessionInfo, SessionKind, SessionTicket, SpawnSpec, Start, TerminalSize};
use crate::start::worktree::{add_worktree, create_branch_unless_exists};
use crate::start::{begin_setup_log, copy_env_files, run_setup};
use crate::tools::Os;

/// A helper's terminal size until a client attaches and resizes it.
const INITIAL_SIZE: TerminalSize = TerminalSize {
    cols: 120,
    rows: 36,
};

/// The longest piece title (one line, like a ticket title).
const TITLE_MAX: usize = 120;

impl Helpers {
    /// `otaq helper start <n> --title "<piece title>" [--stdin]` from the agent in charge (see [`super`]). `note`
    /// (stdin) is posted as a comment "To 40.3: …" once the helper runs.
    pub async fn start(
        &self,
        caller: &Caller,
        key: &str,
        piece: u32,
        title: &str,
        note: Option<&str>,
    ) -> Result<HelperReply, BoardError> {
        let (ticket, in_charge) = self.in_charge(caller, key).await?;
        let title = title.trim();
        if piece == 0 {
            return Err(BoardError::refused(
                Refusal::BadQuestion,
                "pieces are numbered from 1, as the Plan's Pieces list numbers them",
            ));
        }
        if title.is_empty() {
            return Err(BoardError::refused(
                Refusal::EmptyField,
                format!(
                    "name the piece: `{} helper start {piece} --title \"Wishlist page\"`",
                    naming::COMMAND_NAME
                ),
            ));
        }
        if title.contains('\n') || title.chars().count() > TITLE_MAX {
            return Err(BoardError::refused(
                Refusal::TooLong,
                format!("a piece's title is one line of at most {TITLE_MAX} characters"),
            ));
        }
        let note = note.map(str::trim).filter(|n| !n.is_empty());
        let lock = self.lock_of(&ticket.id);
        let _one = lock.lock().await;
        let pool = self.inner.board.store().pool();
        let label = piece_label(&ticket.key, piece);
        if let Some(existing) = store::of_piece(pool, &ticket.id, piece).await? {
            if existing.status == HelperStatus::Merged {
                return Err(BoardError::refused(
                    Refusal::AlreadyRunning,
                    format!("{label} is merged already"),
                ));
            }
            if self.live_session(&existing).is_some() {
                return Err(BoardError::refused(
                    Refusal::AlreadyRunning,
                    format!(
                        "{label}'s helper is already working: send it a note with `{} helper back {piece} --stdin`",
                        naming::COMMAND_NAME
                    ),
                ));
            }
        }
        self.start_piece(&ticket, &in_charge, piece, title, note)
            .await
    }

    /// The held helpers whose tool can start now (a lifted limit, a tool back): each starts as its agent in charge
    /// asked, while that agent is still in charge (its live Agent-stage session). Returns how many started.
    pub async fn start_held(&self) -> Result<usize, BoardError> {
        let pool = self.inner.board.store().pool();
        let held = store::with_status(pool, HelperStatus::Held).await?;
        let mut started = 0;
        for helper in held {
            let Ok(ticket) = self.inner.board.ticket(&helper.ticket.to_string()).await else {
                continue;
            };
            let Some(in_charge) = self
                .inner
                .sessions
                .running_of(&ticket.id, SessionKind::Agent)
            else {
                continue;
            };
            let lock = self.lock_of(&ticket.id);
            let _one = lock.lock().await;
            // Read again under the lock: a start running meanwhile (its row is `held` until its session runs) has
            // started it already, and a second session would run beside the first.
            match store::of_piece(pool, &ticket.id, helper.piece).await? {
                Some(now)
                    if now.status == HelperStatus::Held && self.live_session(&now).is_none() => {}
                _ => continue,
            }
            match self
                .start_piece(&ticket, &in_charge, helper.piece, &helper.title, None)
                .await
            {
                Ok(reply) if reply.helper.status == HelperStatus::Working => {
                    // The agent in charge hears it the way it hears a helper's note (d0048).
                    let text = format!("{} Its hold lifted.", reply.text);
                    if let Err(e) = self
                        .inner
                        .deliveries
                        .queue_delivery(
                            &in_charge.id,
                            crate::session::deliveries::DeliveryKind::AgentMessage,
                            &text,
                        )
                        .await
                    {
                        tracing::warn!(helper = helper.id, error = %e, "can't tell the agent in charge");
                    }
                    started += 1;
                }
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!(helper = helper.id, error = %e, "a held helper didn't start")
                }
            }
        }
        Ok(started)
    }

    /// The helper's session when it runs.
    pub(super) fn live_session(&self, helper: &Helper) -> Option<SessionInfo> {
        helper
            .session
            .as_deref()
            .and_then(|s| self.inner.sessions.info(s).ok())
            .filter(SessionInfo::is_running)
    }

    /// Starts piece `piece` of `ticket` for the agent in charge running in `in_charge` (its lock held).
    async fn start_piece(
        &self,
        ticket: &Ticket,
        in_charge: &SessionInfo,
        piece: u32,
        title: &str,
        note: Option<&str>,
    ) -> Result<HelperReply, BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let pool = board.store().pool();
        let label = piece_label(&ticket.key, piece);
        let ticket_branch = ticket_branch(ticket)?;
        let ticket_worktree = ticket_worktree(ticket)?;
        let project = board.project(ticket.key.project.as_str()).await?;
        let repo = PathBuf::from(&project.repo_path);
        let repo_name = crate::warm::store::repo_name(&project);
        let name = folder_name(&ticket.key, piece);
        let path = inner
            .data
            .worktree_path(&repo_name, &name)
            .map_err(|e| BoardError::refused(Refusal::BadPath, e.to_string()))?;
        let branch = branch_name(&ticket.key, piece);
        let before = store::of_piece(pool, &ticket.id, piece).await?;
        let fresh = before.is_none();
        let id = store::upsert(
            pool,
            &NewHelper {
                ticket_id: &ticket.id,
                piece,
                title,
                branch: &branch,
                worktree: &path.display().to_string(),
                port: None,
            },
        )
        .await?;

        // The stage's agent, on the tool and model the agent in charge runs on (Q513 may add a pick per piece).
        let agent = match &in_charge.agent {
            Some(agent) => board.store().agent(&agent.id).await?,
            None => None,
        };
        let agent = match agent {
            Some(agent) => agent,
            None => resolve::session_agent(board, ticket, SessionRole::Developer).await?,
        };
        let runs_on = RunsOn {
            tool: in_charge.tool,
            model: in_charge.model.clone(),
            effort: in_charge.effort.clone(),
            agent: in_charge
                .agent
                .clone()
                .or_else(|| Some(agent.session_agent())),
            fallback: in_charge.fallback.clone(),
        };
        let tool = runs_on
            .tool
            .unwrap_or_else(|| inner.adapters.for_new_session().tool());
        // The plan-limit hold applies to starting a helper (d0133 audit 4): nothing is made while it holds.
        if let Some(gate) = inner.gate.get()
            && let Some(reason) = gate.helper_blocked(tool).await?
        {
            // Held already (a lift that found it still blocked): nothing new to record. Never a nudge here: the lift
            // that runs held helpers would come straight back.
            let held_already = before.as_ref().is_some_and(|h| {
                h.status == HelperStatus::Held && h.held_reason.as_deref() == Some(reason.as_str())
            });
            if !held_already {
                store::set_status(pool, &id, HelperStatus::Held, None, Some(&reason)).await?;
            }
            let helper = self.piece(ticket, piece).await?;
            if !held_already {
                self.record(ticket, &helper, "held", Some(&reason)).await?;
            }
            return Ok(HelperReply {
                text: format!(
                    "{reason}: {label}'s helper ({title}) is waiting for limit reset. {} starts it then and tells you; carry on with the rest meanwhile.",
                    naming::PRODUCT_NAME
                ),
                helper,
            });
        }

        let made = self
            .make_worktree(
                ticket,
                &project,
                &repo,
                &ticket_branch,
                &ticket_worktree,
                &path,
                &branch,
                &name,
            )
            .await;
        // The row names the session before its program starts: a helper that runs `otaq helper done` at once is
        // found by its session ([`super::Helpers::helper_of`]).
        let session_id = crate::store::new_id();
        let launched = match made {
            Ok(()) => match store::set_session(pool, &id, Some(&session_id)).await {
                Ok(()) => {
                    self.launch(
                        ticket,
                        &project,
                        &path,
                        piece,
                        title,
                        &runs_on,
                        &agent,
                        session_id,
                        None,
                        FirstPrompt::Text(first_prompt(&ticket.key, piece, title)),
                    )
                    .await
                }
                Err(e) => Err(e.into()),
            },
            Err(e) => Err(e),
        };
        let session = match launched {
            Ok(session) => session,
            Err(e) => {
                // Never started: the row goes, so the same command runs it again (its worktree is reused); a row
                // kept names its previous session again.
                if fresh {
                    sqlx::query("DELETE FROM helpers WHERE id = ?")
                        .bind(&id)
                        .execute(pool)
                        .await?;
                } else {
                    let previous = before.as_ref().and_then(|h| h.session.as_deref());
                    store::set_session(pool, &id, previous).await?;
                }
                return Err(e);
            }
        };
        store::started(pool, &id, &session.id, note).await?;
        if let Some(note) = note {
            self.note_comment(ticket, &in_charge.id, &label, note)
                .await?;
        }
        let helper = self.piece(ticket, piece).await?;
        self.record(ticket, &helper, "started", note).await?;
        Ok(HelperReply {
            text: format!(
                "Started {label}'s helper ({title}) on {branch}, in {}. It hands its piece back with `{cmd} helper done`; read its work with `{cmd} helper diff {piece}`, send it back with `{cmd} helper back {piece} --stdin`, merge it with `{cmd} helper merge {piece}`.",
                path.display(),
                cmd = naming::COMMAND_NAME
            ),
            helper,
        })
    }

    /// The helper's branch from the ticket's branch's head and its worktree; a new worktree is seeded (d0082: the
    /// ticket's own worktree build first, else the warm build; never a rename), gets the env files, and the setup
    /// command runs in it, as a start does.
    #[allow(clippy::too_many_arguments)]
    async fn make_worktree(
        &self,
        ticket: &Ticket,
        project: &crate::store::projects::Project,
        repo: &Path,
        ticket_branch: &str,
        ticket_worktree: &Path,
        path: &Path,
        branch: &str,
        name: &str,
    ) -> Result<(), BoardError> {
        let inner = &self.inner;
        let board = &inner.board;
        let git = board.project_git(&project.key).await?;
        let head = git
            .resolve(repo, &format!("refs/heads/{ticket_branch}"))
            .await?
            .ok_or_else(|| {
                BoardError::refused(
                    Refusal::NoBranch,
                    format!("{}'s branch {ticket_branch} is gone", ticket.key),
                )
            })?;
        create_branch_unless_exists(&git, repo, branch, &head).await?;
        let new = !path.join(".git").exists();
        add_worktree(&git, repo, path, branch)
            .await
            .map_err(start_error)?;
        if !new {
            return Ok(());
        }
        let seeded = match inner.seeder.get() {
            Some(seeder) => {
                seeder
                    .seed_scratch(board, &git, project, name, Some(ticket_worktree), path)
                    .await
            }
            None => None,
        };
        let settings = board.project_settings(&project.key).await?;
        let log = inner
            .data
            .setup_log_path(name)
            .map_err(|e| BoardError::refused(Refusal::BadPath, e.to_string()))?;
        let first = seeded.map(|from| match from {
            crate::warm::seed::ScratchSeed::Worktree => {
                format!("Started from {}'s worktree build.", ticket.key)
            }
            crate::warm::seed::ScratchSeed::WarmBuild => {
                format!("Started from {}'s warm build.", project.name)
            }
        });
        if let Err(e) = begin_setup_log(&log, first.as_deref()) {
            tracing::warn!(helper = name, error = %e, "can't begin the setup log");
        }
        copy_env_files(repo, path, &settings.env_files).map_err(start_error)?;
        let recorded = ticket.work.as_ref().and_then(|w| w.resource_name.clone());
        let own = crate::ticket_env::resources::command_vars(
            &inner.data,
            project,
            &ticket.key,
            recorded.as_deref(),
            &settings,
        );
        let project_env = crate::session::env::applied(&settings.env_vars, Os::current());
        run_setup(&settings.setup_command, path, &log, &project_env, &own)
            .await
            .map_err(start_error)?;
        Ok(())
    }

    /// The helper's port, skills and session `session_id` (its row names it already) in its worktree: a fresh
    /// conversation told its piece, or (`resume`, the tool's conversation id) its own conversation continued (d0133
    /// audit 5), `first` its first prompt.
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn launch(
        &self,
        ticket: &Ticket,
        project: &crate::store::projects::Project,
        worktree: &Path,
        piece: u32,
        title: &str,
        runs_on: &RunsOn,
        agent: &crate::agents::Agent,
        session_id: String,
        resume: Option<String>,
        first: FirstPrompt,
    ) -> Result<SessionInfo, BoardError> {
        let inner = &self.inner;
        let port = inner
            .ports
            .allocate_helper(&ticket.id, piece)
            .await
            .map_err(|e| BoardError::refused(Refusal::NotAvailableYet, e.to_string()))?;
        // Its own worktree gets its own copies (d0118), so a tool starting beside it never lists a copy being
        // replaced: every skill is in place before the tool starts.
        let (first_prompt, written) = inner
            .board
            .copy_for_ticket_session(
                worktree,
                inner.adapters.shipped_skills(),
                Some(agent),
                Some(first),
                &project.key,
            )
            .await;
        let mut launched = inner
            .adapters
            .launch(
                runs_on.tool,
                &LaunchRequest {
                    role: LaunchRole::TicketAgent,
                    cwd: worktree.to_owned(),
                    project: project.key.clone(),
                    ticket: Some(ticket.key.clone()),
                    port,
                    first_prompt,
                    resume,
                    model: runs_on.model.clone(),
                    effort: runs_on.effort.clone(),
                    title: Some(format!(
                        "{} \u{b7} {} {title}",
                        ticket.title,
                        piece_label(&ticket.key, piece)
                    )),
                    theme: inner.board.theme_in_use().await,
                },
            )
            .map_err(|e| BoardError::refused(Refusal::NotAvailableYet, e.to_string()))?;
        launched.plan.env.push((piece_env_var(), piece.to_string()));
        let session = inner
            .sessions
            .prepared(launched.prepare)
            .with_id(session_id)
            .spawn(SpawnSpec {
                kind: SessionKind::Helper,
                start: Start::Auto,
                project_id: project.id.clone(),
                project_key: project.key.clone(),
                ticket: Some(SessionTicket {
                    id: ticket.id.clone(),
                    key: ticket.key.clone(),
                }),
                program: launched.plan.program,
                args: launched.plan.args,
                cwd: launched.plan.cwd,
                env: launched.plan.env,
                size: INITIAL_SIZE,
                tool: Some(launched.tool),
                tool_session_id: launched.tool_session_id,
                model: launched.model,
                agent: runs_on.agent.clone(),
                effort: runs_on.effort.clone(),
                fallback: runs_on.fallback.clone(),
                theme: Some(launched.theme),
            })
            .await
            .map_err(|e| BoardError::refused(Refusal::NotAvailableYet, e.to_string()))?;
        if let Err(e) = inner
            .sessions
            .set_started_skills(&session.id, written.names)
        {
            tracing::warn!(helper = %session.id, error = %e, "the helper's skills weren't recorded");
        }
        Ok(session)
    }
}

fn start_error(e: crate::start::StartError) -> BoardError {
    match e {
        crate::start::StartError::Board(e) => e,
        crate::start::StartError::Git(e) => BoardError::Git(e),
        crate::start::StartError::Store(e) => BoardError::Store(e),
        other => BoardError::refused(Refusal::NotAvailableYet, other.to_string()),
    }
}
