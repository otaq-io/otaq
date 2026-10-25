//! Token usage (USG-01, USG-09; d0020, d0088): tokens per ticket and today's total, read **read-only** from the
//! conversations the agent tools save, for the tool ids Otaq stored. Nothing here knows a tool: each session's
//! adapter gives its [`TokenSource`] ([`crate::agent_tools::AgentTool::token_source`]).
//!
//! The [`Usage`] reader runs in the service on a slow tick ([`DEFAULT_TICK`]) and soon after a session starts or
//! ends or a card's status changes. Each read takes only what was added since the last one, attributes every step
//! (one model response) to the Otaq session that was running when it was written (a resumed conversation keeps its
//! tool id, so its files are shared by several sessions) and to the local day it was written on, and stores the
//! totals in the `usage` table, one row per session and day. Clients get `usage_changed` and ask again.
//!
//! The tools' files are internal and may change (P8): a missing, unreadable or unrecognised file logs one warning
//! and makes that conversation's usage **unknown**, as does a session with no tool or no tool id yet; nothing
//! else is affected. Errors never reach tickets, sessions or the scheduler. Conversations whose sessions have all
//! ended are read once more and then left alone; after a service restart, those that ended more than a day ago
//! and already have counts aren't read again.

pub mod activity;
pub mod limit;
pub mod plan;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, broadcast, watch};

use crate::agent_tools::{Adapters, TokenReader, TokenSource, TokenStep, TokensUnknown, Tool};
use crate::board::BoardError;
use crate::protocol::Event;
use crate::service::EventBus;
use crate::store::{Store, StoreError, now_ms};

/// The slow tick: a read at least this often.
pub const DEFAULT_TICK: Duration = Duration::from_secs(60);
/// How long a burst of session and status changes may settle before a read.
const SETTLE: Duration = Duration::from_secs(1);
/// After a restart, finished conversations with counts aren't read again once they ended this long ago.
const SETTLED_AFTER_MS: i64 = 24 * 60 * 60 * 1000;

/// Token counts, the same four numbers for every tool (USG-09, d0088). Each adapter maps its own source onto them
/// without counting anything twice:
/// - Claude Code: cache writes count as input and cache reads as cached input; no reasoning (its thinking is part
///   of its output). The mapping is in [`crate::agent_tools::claude::transcript`].
/// - Codex: its `token_count` events' input **includes** the cached input, so its adapter subtracts `cached` from
///   it; `reasoning` is reported apart from `output`.
/// - Copilot: `session.usage_record` in its events file, with its cache counts.
/// - OpenCode: `session.usage.updated` (`input`, `output`, `reasoning`, `cache.read`, `cache.write`): cache writes
///   go into `input`, as for Claude Code.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tokens {
    /// Input not read from a cache (cache writes included).
    pub input: u64,
    /// Input read from a cache.
    pub cached: u64,
    pub output: u64,
    /// Reasoning tokens, only where the tool reports them apart from `output`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<u64>,
}

impl Tokens {
    /// The one number shown on cards, in the sheet and in the header: input (cache writes included), output and
    /// reasoning. Cached input (the conversation re-read on every request) is stored but not shown (Q26, deferred:
    /// built as option 1).
    pub fn shown(&self) -> u64 {
        self.input + self.output + self.reasoning.unwrap_or(0)
    }

    fn add(&mut self, other: &Tokens) {
        self.input += other.input;
        self.cached += other.cached;
        self.output += other.output;
        self.reasoning = match (self.reasoning, other.reasoning) {
            (None, None) => None,
            (a, b) => Some(a.unwrap_or(0) + b.unwrap_or(0)),
        };
    }
}

/// A ticket's tokens over all its sessions (agent, resumed, answer, validator; USG-01, VAL-14).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TicketUsage {
    /// The ticket's id.
    pub id: String,
    /// The sum of every session: the card's number. `None` when no conversation of the ticket could be read
    /// ("unknown", shown as "—").
    pub tokens: Option<Tokens>,
    /// The validators' share (d0063): "agent 61k · validation 23k (2 rounds)". Cancelled or interrupted rounds'
    /// validators count too.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation: Option<Tokens>,
    /// Validation rounds that counted (a pass or a fail).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rounds: u32,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
}

impl TicketUsage {
    /// The agent's share: everything but the validators.
    pub fn agent(&self) -> Option<Tokens> {
        let mut agent = self.tokens?;
        if let Some(v) = self.validation {
            agent.input = agent.input.saturating_sub(v.input);
            agent.cached = agent.cached.saturating_sub(v.cached);
            agent.output = agent.output.saturating_sub(v.output);
            agent.reasoning = match (agent.reasoning, v.reasoning) {
                (Some(a), b) => Some(a.saturating_sub(b.unwrap_or(0))),
                (None, _) => None,
            };
        }
        Some(agent)
    }
}

/// A ticket's tokens on one named agent in one role (d0086: "The ticket panel splits developer and validator tokens
/// as today (0063), with the named agent and tool for each"): the parts of "Strong 61k · Light 23k (1 round)". The
/// view models word it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTokens {
    /// Developer (the agent's sessions, its answers and its deliveries) or Validator.
    pub role: crate::agents::SessionRole,
    /// The named agent's name at its sessions' start ("Strong"); `None` for sessions that ran on none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Tool>,
    /// `None` when none of these sessions' conversations could be read.
    pub tokens: Option<Tokens>,
}

/// One session's tokens (the History tab).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionUsage {
    pub session: String,
    /// `None` when its conversation couldn't be read.
    pub tokens: Option<Tokens>,
}

/// The answer to `usage.summary`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct UsageSummary {
    /// Today, as `YYYY-MM-DD` in local time.
    pub day: String,
    /// Today's tokens across all projects.
    pub today: Tokens,
    /// The requested project's tickets that ran a conversation.
    pub tickets: Vec<TicketUsage>,
    /// Every tool's limit windows and the usage limit, from the tools' reports and limit signals (USG-04, USG-05,
    /// USG-09); `None` when no tool has windows and no limit is active (API-key users).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan: Option<plan::PlanLimits>,
    /// Agents waiting for the limit reset, across all projects (the banner's "8 agents stopped", USG-06).
    #[serde(default)]
    pub limit_waiting: u32,
    /// The banner's line per tool (USG-10): its limit or its being unavailable, what waits, Resume, Move waiting
    /// to… (filled by the service).
    #[serde(default)]
    pub tool_lines: crate::agents::limits::ToolLines,
}

/// `YYYY-MM-DD` of a Unix-milliseconds moment in local time (the `usage.day` column).
pub fn local_day(at_ms: i64) -> String {
    match crate::local_time::local(at_ms) {
        Some(t) => format!("{:04}-{:02}-{:02}", t.year, t.month, t.day),
        None => format!("unix-day-{}", at_ms.div_euclid(86_400_000)),
    }
}

/// An Otaq session that ran an agent tool's conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSpan {
    pub id: String,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

/// Totals per (session, day) of a conversation's steps. A step belongs to the last session that had started when
/// it was written (the first session when it is older, the last when its time is unknown).
pub fn attribute<'a>(
    sessions: &[SessionSpan],
    messages: impl IntoIterator<Item = &'a TokenStep>,
    now: i64,
) -> BTreeMap<(String, String), Tokens> {
    let mut sorted: Vec<&SessionSpan> = sessions.iter().collect();
    sorted.sort_by_key(|s| (s.started_at, s.id.clone()));
    let mut rows: BTreeMap<(String, String), Tokens> = BTreeMap::new();
    let (Some(first), Some(last)) = (sorted.first(), sorted.last()) else {
        return rows;
    };
    for message in messages {
        let session = match message.at {
            Some(at) => sorted
                .iter()
                .rev()
                .find(|s| s.started_at <= at)
                .unwrap_or(first),
            None => last,
        };
        let day = local_day(message.at.unwrap_or(now));
        rows.entry((session.id.clone(), day))
            .or_default()
            .add(&message.tokens);
    }
    rows
}

/// The usage reader of one service. Cheap to clone.
#[derive(Clone)]
pub struct Usage {
    inner: Arc<Inner>,
}

struct Inner {
    store: Store,
    events: EventBus,
    /// Every agent tool's adapter, for each session's token source.
    adapters: Adapters,
    tick: Duration,
    /// One read at a time.
    state: tokio::sync::Mutex<HashMap<String, Tracked>>,
    /// Tool ids whose usage is unknown.
    unknown: Mutex<HashSet<String>>,
    wake: Notify,
}

/// What the reader knows about one tool id.
#[derive(Default)]
struct Tracked {
    /// The tool's reader, once asked for; it remembers how far it has read.
    reader: Option<Box<dyn TokenReader>>,
    unknown: Option<TokensUnknown>,
    /// The one warning for this conversation was logged.
    warned: bool,
    /// Every session ended and was read after: nothing more will come.
    settled: bool,
    /// The rows last written.
    written: BTreeMap<(String, String), Tokens>,
}

/// One row of the session query.
#[derive(Debug, Clone)]
struct StoredSession {
    span: SessionSpan,
    /// The agent tool; `None` (or one this build doesn't know) makes the conversation unknown.
    tool: Option<Tool>,
    tool_id: String,
    /// The worktree, else the repository: where the tool ran.
    cwd: Option<String>,
}

impl Usage {
    pub fn new(store: Store, events: EventBus, adapters: Adapters, tick: Duration) -> Self {
        Self {
            inner: Arc::new(Inner {
                store,
                events,
                adapters,
                tick,
                state: tokio::sync::Mutex::new(HashMap::new()),
                unknown: Mutex::new(HashSet::new()),
                wake: Notify::new(),
            }),
        }
    }

    /// Every agent tool's adapter (the registry built at service start).
    pub fn adapters(&self) -> &Adapters {
        &self.inner.adapters
    }

    /// Asks the loop for a read soon.
    pub fn wake(&self) {
        self.inner.wake.notify_one();
    }

    /// Reads what was added to every conversation that may still change and stores the totals. Returns whether
    /// anything changed (counts or unknown conversations); `usage_changed` is published then. Errors are logged
    /// by the loop and never reach anything else.
    pub async fn read_now(&self) -> Result<bool, StoreError> {
        let inner = &self.inner;
        let mut state = inner.state.lock().await;
        let started = now_ms();
        let sessions = self.sessions().await?;
        let mut by_tool_id: BTreeMap<String, Vec<StoredSession>> = BTreeMap::new();
        for session in sessions {
            by_tool_id
                .entry(session.tool_id.clone())
                .or_default()
                .push(session);
        }
        let counted = self.counted_conversations().await?;

        let mut work = Vec::new();
        for (tool_id, sessions) in &by_tool_id {
            let ended_at = sessions
                .iter()
                .map(|s| s.span.ended_at)
                .collect::<Option<Vec<_>>>()
                .and_then(|ends| ends.into_iter().max());
            let tracked = state.entry(tool_id.clone()).or_insert_with(|| Tracked {
                // Finished long ago and counted by an earlier run: nothing new will come.
                settled: ended_at.is_some_and(|end| started - end > SETTLED_AFTER_MS)
                    && counted.contains(tool_id),
                ..Default::default()
            });
            // A session started again on this conversation (a resume).
            if ended_at.is_none() {
                tracked.settled = false;
            }
            if tracked.settled {
                continue;
            }
            work.push((tool_id.clone(), std::mem::take(tracked), sessions.clone()));
        }

        let adapters = inner.adapters.clone();
        let read = tokio::task::spawn_blocking(move || {
            work.into_iter()
                .map(|(tool_id, mut tracked, sessions)| {
                    let result = read_conversation(&adapters, &tool_id, &sessions, &mut tracked);
                    (tool_id, tracked, sessions, result)
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();

        let mut changed = false;
        for (tool_id, mut tracked, sessions, result) in read {
            match result {
                Ok(steps) => {
                    if tracked.unknown.take().is_some() {
                        changed = true;
                    }
                    let spans: Vec<SessionSpan> = sessions.iter().map(|s| s.span.clone()).collect();
                    let rows = attribute(&spans, &steps, started);
                    for (key, counted) in &rows {
                        if tracked.written.get(key) != Some(counted) {
                            self.write_row(&key.0, &key.1, counted).await?;
                            changed = true;
                        }
                    }
                    tracked.written = rows;
                }
                Err(why) => {
                    if !tracked.warned {
                        tracing::warn!(tool_session = %tool_id, reason = ?why, "token usage unknown");
                        tracked.warned = true;
                    }
                    if tracked.unknown.replace(why).is_none() {
                        changed = true;
                    }
                }
            }
            let all_ended = sessions
                .iter()
                .all(|s| s.span.ended_at.is_some_and(|end| end < started));
            tracked.settled = all_ended;
            state.insert(tool_id, tracked);
        }

        let unknown: HashSet<String> = state
            .iter()
            .filter(|(_, t)| t.unknown.is_some())
            .map(|(id, _)| id.clone())
            .collect();
        *inner.unknown.lock().expect("unknown usage") = unknown;
        if changed {
            inner.events.publish(Event::UsageChanged);
        }
        Ok(changed)
    }

    /// Runs reads on the tick and soon after session and status changes until `stop` is set.
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut listener = self.inner.events.listen();
        let mut tick = tokio::time::interval(self.inner.tick);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            let settle = tokio::select! {
                _ = stop.wait_for(|s| *s) => return,
                _ = tick.tick() => false,
                () = self.inner.wake.notified() => false,
                event = listener.receiver.recv() => match event {
                    Ok(Event::SessionState { .. } | Event::CardStatusChanged { .. })
                    | Err(broadcast::error::RecvError::Lagged(_)) => true,
                    Ok(_) => continue,
                    Err(broadcast::error::RecvError::Closed) => return,
                },
            };
            if settle {
                // Let the burst settle (a session start brings several changes), then read once.
                tokio::time::sleep(SETTLE).await;
                while listener.receiver.try_recv().is_ok() {}
            }
            if let Err(e) = self.read_now().await {
                tracing::warn!(error = %e, "can't record token usage");
            }
        }
    }

    /// Today's tokens across all projects, and the tokens of `project`'s tickets (`usage.summary`).
    pub async fn summary(&self, project: Option<&str>) -> Result<UsageSummary, BoardError> {
        let day = local_day(now_ms());
        let today = self.day_total(&day).await?;
        let tickets = match project {
            Some(project) => self.tickets(project).await?,
            None => Vec::new(),
        };
        let plan = plan::latest(&self.inner.store).await?;
        let (limit_waiting,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tickets WHERE state = ?")
                .bind(crate::domain::Status::WaitingForLimitReset.as_str())
                .fetch_one(self.inner.store.pool())
                .await?;
        Ok(UsageSummary {
            day,
            today,
            tickets,
            plan,
            limit_waiting: u32::try_from(limit_waiting).unwrap_or(0),
            tool_lines: Default::default(),
        })
    }

    /// Activity's data per tool: agents running, today's tokens and the limit windows (`usage.activity`, SHL-03).
    pub async fn activity(&self) -> Result<activity::Activity, StoreError> {
        activity::activity(&self.inner.store, &self.inner.adapters).await
    }

    /// The tokens of one day (`YYYY-MM-DD`) across all projects.
    pub async fn day_total(&self, day: &str) -> Result<Tokens, StoreError> {
        let row: DayRow = sqlx::query_as(
            "SELECT SUM(input), SUM(cached), SUM(output), SUM(reasoning) FROM usage WHERE day = ?",
        )
        .bind(day)
        .fetch_one(self.inner.store.pool())
        .await?;
        Ok(tokens(row))
    }

    /// The tokens of every ticket of `project` (by key) that ran a conversation.
    pub async fn tickets(&self, project: &str) -> Result<Vec<TicketUsage>, StoreError> {
        let rows: Vec<TicketConversationRow> = sqlx::query_as(
            "SELECT s.ticket_id, s.tool_session_id, SUM(u.input), SUM(u.cached), SUM(u.output),
                    SUM(u.reasoning), MAX(s.kind = 'validator')
             FROM sessions s
             JOIN projects p ON p.id = s.project_id
             LEFT JOIN usage u ON u.session_id = s.id
             WHERE p.key = ? AND s.ticket_id IS NOT NULL
                   AND (s.tool IS NOT NULL OR s.tool_session_id IS NOT NULL)
             GROUP BY s.ticket_id, s.tool_session_id
             ORDER BY s.ticket_id",
        )
        .bind(project)
        .fetch_all(self.inner.store.pool())
        .await?;
        let rounds: std::collections::HashMap<String, i64> = sqlx::query_as(
            "SELECT v.ticket_id, COUNT(DISTINCT COALESCE(v.round_id, v.id)) FROM validations v
             JOIN tickets t ON t.id = v.ticket_id JOIN projects p ON p.id = t.project_id
             WHERE p.key = ? AND v.outcome IN ('pass', 'fail') GROUP BY v.ticket_id",
        )
        .bind(project)
        .fetch_all(self.inner.store.pool())
        .await?
        .into_iter()
        .collect();
        let unknown = self.inner.unknown.lock().expect("unknown usage").clone();
        let mut out: Vec<TicketUsage> = Vec::new();
        // A session with no tool id yet has no tokens: unknown, like a conversation that can't be read.
        let known =
            |tool_id: &Option<String>| tool_id.as_ref().is_some_and(|id| !unknown.contains(id));
        for (ticket, tool_id, a, b, c, d, validator) in rows {
            let known = known(&tool_id);
            if out.last().is_none_or(|t| t.id != ticket) {
                let counted = rounds.get(&ticket).copied().unwrap_or(0);
                out.push(TicketUsage {
                    rounds: u32::try_from(counted).unwrap_or(0),
                    id: ticket,
                    ..TicketUsage::default()
                });
            }
            let entry = out.last_mut().expect("just pushed");
            if known {
                let these = tokens((a, b, c, d));
                entry.tokens.get_or_insert_with(Tokens::default).add(&these);
                if validator == Some(true) {
                    entry
                        .validation
                        .get_or_insert_with(Tokens::default)
                        .add(&these);
                }
            }
        }
        Ok(out)
    }

    /// The tokens of each session of a ticket (by id) that ran a conversation, oldest first; `None` when unknown
    /// (no tool id yet, or a conversation that can't be read).
    pub async fn sessions_of(&self, ticket_id: &str) -> Result<Vec<SessionUsage>, StoreError> {
        let rows: Vec<ConversationRow> = sqlx::query_as(
            "SELECT s.id, s.tool_session_id, SUM(u.input), SUM(u.cached), SUM(u.output),
                    SUM(u.reasoning)
             FROM sessions s LEFT JOIN usage u ON u.session_id = s.id
             WHERE s.ticket_id = ? AND (s.tool IS NOT NULL OR s.tool_session_id IS NOT NULL)
             GROUP BY s.id, s.tool_session_id
             ORDER BY MIN(s.started_at), s.id",
        )
        .bind(ticket_id)
        .fetch_all(self.inner.store.pool())
        .await?;
        let unknown = self.inner.unknown.lock().expect("unknown usage").clone();
        Ok(rows
            .into_iter()
            .map(|(session, tool_id, a, b, c, d)| SessionUsage {
                session,
                tokens: tool_id
                    .is_some_and(|id| !unknown.contains(&id))
                    .then(|| tokens((a, b, c, d))),
            })
            .collect())
    }

    /// A ticket's (by id) tokens by role and named agent ([`AgentTokens`]), in role order (Developer, Validator) and
    /// then the order the agents first ran.
    pub async fn agents_of(&self, ticket_id: &str) -> Result<Vec<AgentTokens>, StoreError> {
        use crate::agents::SessionRole;
        type Row = (
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            Option<i64>,
            i64,
        );
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT s.kind, s.agent_name, s.tool, s.tool_session_id, SUM(u.input), SUM(u.cached),
                    SUM(u.output), SUM(u.reasoning), s.started_at
             FROM sessions s LEFT JOIN usage u ON u.session_id = s.id
             WHERE s.ticket_id = ? AND (s.tool IS NOT NULL OR s.tool_session_id IS NOT NULL)
             GROUP BY s.id
             ORDER BY 9",
        )
        .bind(ticket_id)
        .fetch_all(self.inner.store.pool())
        .await?;
        let unknown = self.inner.unknown.lock().expect("unknown usage").clone();
        let mut out: Vec<AgentTokens> = Vec::new();
        for (kind, agent, tool, tool_id, a, b, c, d, _) in rows {
            let role = match kind.as_str() {
                "validator" => SessionRole::Validator,
                _ => SessionRole::Developer,
            };
            let tool = tool.and_then(|t| t.parse().ok());
            let at = match out
                .iter()
                .position(|t| t.role == role && t.agent == agent && t.tool == tool)
            {
                Some(at) => at,
                None => {
                    out.push(AgentTokens {
                        role,
                        agent,
                        tool,
                        tokens: None,
                    });
                    out.len() - 1
                }
            };
            if tool_id.is_some_and(|id| !unknown.contains(&id)) {
                out[at]
                    .tokens
                    .get_or_insert_with(Tokens::default)
                    .add(&tokens((a, b, c, d)));
            }
        }
        out.sort_by_key(|t| SessionRole::ALL.iter().position(|r| *r == t.role));
        Ok(out)
    }

    /// Every session that stored a tool id, with its tool and where it ran.
    async fn sessions(&self) -> Result<Vec<StoredSession>, StoreError> {
        let rows: Vec<SessionRow> = sqlx::query_as(
            "SELECT s.id, s.tool, s.tool_session_id, s.started_at, s.ended_at, t.worktree_path, p.repo_path
             FROM sessions s
             JOIN projects p ON p.id = s.project_id
             LEFT JOIN tickets t ON t.id = s.ticket_id
             WHERE s.tool_session_id IS NOT NULL AND s.tool_session_id <> ''",
        )
        .fetch_all(self.inner.store.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(
                |(id, tool, tool_id, started_at, ended_at, worktree, repo)| StoredSession {
                    span: SessionSpan {
                        id,
                        started_at,
                        ended_at,
                    },
                    tool: tool.and_then(|t| t.parse().ok()),
                    tool_id,
                    cwd: worktree.or(Some(repo)),
                },
            )
            .collect())
    }

    /// The tool ids that already have counts stored.
    async fn counted_conversations(&self) -> Result<HashSet<String>, StoreError> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT s.tool_session_id FROM usage u JOIN sessions s ON s.id = u.session_id
             WHERE s.tool_session_id IS NOT NULL",
        )
        .fetch_all(self.inner.store.pool())
        .await?;
        Ok(rows.into_iter().map(|(id,)| id).collect())
    }

    async fn write_row(&self, session: &str, day: &str, t: &Tokens) -> Result<(), StoreError> {
        let n = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
        sqlx::query(
            "INSERT INTO usage (session_id, day, input, cached, output, reasoning)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (session_id, day) DO UPDATE SET input = excluded.input,
                 cached = excluded.cached, output = excluded.output, reasoning = excluded.reasoning",
        )
        .bind(session)
        .bind(day)
        .bind(n(t.input))
        .bind(n(t.cached))
        .bind(n(t.output))
        .bind(t.reasoning.map(n))
        .execute(self.inner.store.pool())
        .await?;
        Ok(())
    }
}

/// A conversation's running totals as its tool reported them in an event ([`TokenSource::Events`]: OpenCode's
/// plugin): what they add to the counts already stored for the same conversation (every session with the same
/// tool and tool id) goes to `session`'s row of today. Returns whether a row changed.
pub async fn totals_reported(
    pool: &sqlx::SqlitePool,
    session: &str,
    totals: &Tokens,
) -> Result<bool, StoreError> {
    let stored: DayRow = sqlx::query_as(
        "SELECT SUM(u.input), SUM(u.cached), SUM(u.output), SUM(u.reasoning)
         FROM sessions me JOIN sessions s
              ON s.id = me.id
                 OR (s.tool = me.tool AND s.tool_session_id = me.tool_session_id AND me.tool_session_id <> '')
         JOIN usage u ON u.session_id = s.id
         WHERE me.id = ?",
    )
    .bind(session)
    .fetch_one(pool)
    .await?;
    let stored = tokens(stored);
    let added = Tokens {
        input: totals.input.saturating_sub(stored.input),
        cached: totals.cached.saturating_sub(stored.cached),
        output: totals.output.saturating_sub(stored.output),
        reasoning: totals
            .reasoning
            .map(|r| r.saturating_sub(stored.reasoning.unwrap_or(0))),
    };
    if added.input + added.cached + added.output + added.reasoning.unwrap_or(0) == 0 {
        return Ok(false);
    }
    let n = |v: u64| i64::try_from(v).unwrap_or(i64::MAX);
    sqlx::query(
        "INSERT INTO usage (session_id, day, input, cached, output, reasoning)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (session_id, day) DO UPDATE SET input = input + excluded.input,
             cached = cached + excluded.cached, output = output + excluded.output,
             reasoning = CASE WHEN excluded.reasoning IS NULL THEN reasoning
                              ELSE COALESCE(reasoning, 0) + excluded.reasoning END",
    )
    .bind(session)
    .bind(local_day(now_ms()))
    .bind(n(added.input))
    .bind(n(added.cached))
    .bind(n(added.output))
    .bind(added.reasoning.map(n))
    .execute(pool)
    .await?;
    Ok(true)
}

/// `(SUM(input), SUM(cached), SUM(output), SUM(reasoning))` of a day.
type DayRow = (Option<i64>, Option<i64>, Option<i64>, Option<i64>);

/// `(id, tool, tool_session_id, started_at, ended_at, worktree_path, repo_path)`.
type SessionRow = (
    String,
    Option<String>,
    String,
    i64,
    Option<i64>,
    Option<String>,
    String,
);

/// A conversation of a ticket: `(ticket_id, tool_session_id, SUM(input), SUM(cached), SUM(output),
/// SUM(reasoning), whether it was a validator's)`.
type TicketConversationRow = (
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<bool>,
);

/// `(id, tool_session_id, SUM(input), SUM(cached), SUM(output), SUM(reasoning))`.
type ConversationRow = (
    String,
    Option<String>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
    Option<i64>,
);

/// The tokens of a row of sums `(input, cached, output, reasoning)`; a `NULL` reasoning sum (no row reported
/// it) stays `None`.
pub(crate) fn tokens(
    (input, cached, output, reasoning): (Option<i64>, Option<i64>, Option<i64>, Option<i64>),
) -> Tokens {
    let n = |v: i64| u64::try_from(v).unwrap_or(0);
    Tokens {
        input: n(input.unwrap_or(0)),
        cached: n(cached.unwrap_or(0)),
        output: n(output.unwrap_or(0)),
        reasoning: reasoning.map(n),
    }
}

/// Asks the conversation's adapter (once) for its token source and reads what was added; returns every step
/// counted so far. Only ever opens files for reading.
fn read_conversation(
    adapters: &Adapters,
    tool_id: &str,
    sessions: &[StoredSession],
    tracked: &mut Tracked,
) -> Result<Vec<TokenStep>, TokensUnknown> {
    if tracked.reader.is_none() {
        // The latest session's tool and folder (a resume runs in the same folder as the first session).
        let latest = sessions.iter().rev().find(|s| s.cwd.is_some());
        let (Some(tool), Some(cwd)) = (
            latest.and_then(|s| s.tool),
            latest.and_then(|s| s.cwd.as_deref()),
        ) else {
            return Err(TokensUnknown::Missing);
        };
        match adapters.get(tool).token_source(tool_id, Path::new(cwd)) {
            TokenSource::Reader(reader) => tracked.reader = Some(reader),
            // Stored as the events arrive ([`totals_reported`]): nothing to read.
            TokenSource::Events => return Ok(Vec::new()),
            // Tools that don't report them.
            TokenSource::NotReported => return Err(TokensUnknown::Missing),
        }
    }
    let reader = tracked.reader.as_mut().expect("just set");
    reader.read()?;
    Ok(reader.steps())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(id: &str, started_at: i64) -> SessionSpan {
        SessionSpan {
            id: id.into(),
            started_at,
            ended_at: None,
        }
    }

    fn message(id: &str, at: Option<i64>, output: u64) -> TokenStep {
        TokenStep {
            id: id.into(),
            at,
            model: None,
            tokens: Tokens {
                output,
                ..Default::default()
            },
        }
    }

    #[test]
    fn responses_go_to_the_session_running_when_they_were_written() {
        // Noon local time on two days, so the day split doesn't depend on the time zone.
        let day1 = 1_791_547_200_000 - 12 * 3_600_000;
        let noon = |d: i64| day1 + d * 86_400_000 + 12 * 3_600_000;
        let sessions = [span("first", noon(0)), span("resumed", noon(1))];
        let messages = [
            message("m0", Some(noon(0) - 1000), 1),
            message("m1", Some(noon(0) + 1000), 2),
            message("m2", Some(noon(1) + 1000), 4),
            message("m3", None, 8),
        ];
        let rows = attribute(&sessions, &messages, noon(1) + 5000);
        let get = |s: &str, d: i64| {
            rows.get(&(s.to_owned(), local_day(noon(d))))
                .map(|c| c.output)
        };
        assert_eq!(get("first", 0), Some(3));
        assert_eq!(get("resumed", 1), Some(12));
        assert_eq!(rows.len(), 2);
        assert!(attribute(&[], &messages, 0).is_empty());
    }

    #[test]
    fn days_are_local_dates() {
        let day = local_day(1_791_547_200_000);
        assert_eq!(day.len(), 10);
        assert!(day.starts_with("2026-10-"));
    }

    #[test]
    fn the_shown_number_leaves_cached_input_out() {
        let t = Tokens {
            input: 1 + 4,
            cached: 1000,
            output: 2,
            reasoning: None,
        };
        assert_eq!(t.shown(), 7);
        let with_reasoning = Tokens {
            reasoning: Some(8),
            ..t
        };
        assert_eq!(with_reasoning.shown(), 15);
    }

    #[test]
    fn reasoning_stays_none_until_a_tool_reports_it() {
        let mut sum = Tokens::default();
        sum.add(&Tokens {
            input: 1,
            ..Default::default()
        });
        assert_eq!(sum.reasoning, None);
        sum.add(&Tokens {
            reasoning: Some(3),
            ..Default::default()
        });
        assert_eq!(sum.reasoning, Some(3));
        assert_eq!(tokens((Some(5), Some(6), Some(7), None)).reasoning, None);
        assert_eq!(tokens((None, None, None, Some(2))).shown(), 2);
    }
}
