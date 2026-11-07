//! Limits per tool (USG-10, USG-06, USG-07, AGM-03; d0086, d0108, d0020, d0046, d0018): the banner's data, one line
//! per tool ([`ToolLine`]), **Resume** per tool (`limits.resume_tool`).
//!
//! A tool gets a line while its limit is active (session / weekly / another limit, reset known or not), and while
//! it isn't available (logged out, not installed, hidden, update needed) with tickets waiting on it (a held ticket
//! with reason `tool_unavailable`: "Codex isn't available · logged out · 5 tickets waiting"); a tool both at its limit
//! and unavailable gets both. Only that tool's agents wait; the rest keep going. Each line says how many agents the
//! limit stopped, how many tickets wait on the tool (their holds), and whether Resume can be pressed (after a known
//! reset, or always when the reset is unknown, USG-06).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::HoldReason;
use super::gate::{LaunchGate, still_waits};
use crate::agent_tools::Tool;
use crate::agent_tools::connected::{AgentToolState, ConnectedTools};
use crate::attention::NeedsYouQueue;
use crate::board::{Board, BoardError};
use crate::domain::TicketKey;
use crate::usage::limit::{LimitKind, PlanLimit, matching_window};
use crate::usage::plan;

/// The banner's lines, in [`Tool::ALL`] order, and whether several tools are connected (with one, the banner reads
/// as before: no tool name).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLines {
    pub lines: Vec<ToolLine>,
    pub several_tools: bool,
}

/// One tool's banner line (T-0411 words it): "Claude Code limit reached · resets 3:45pm · 4 tickets waiting ·
/// Resume Claude Code", "Codex isn't available · logged out · 5 tickets waiting · Sign in…".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLine {
    pub tool: Tool,
    pub situation: Situation,
    /// Agents the limit stopped mid-turn (Waiting for limit reset; Project agents too). 0 for an unavailable tool.
    pub stopped: u32,
    /// Tickets that would start on the tool and wait (their holds for this situation's reason).
    pub waiting: u32,
    /// Resume can be pressed: the limit reset, or its reset is unknown (USG-06). Never for an unavailable tool.
    pub resume_enabled: bool,
}

/// Why a tool has a line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "situation", rename_all = "snake_case")]
pub enum Situation {
    /// Its limit is active.
    Limit {
        kind: LimitKind,
        /// The length of the tool's window for that limit, when it reports one (a month for Copilot's quota).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        window_minutes: Option<u32>,
        /// When it resets; `None` = unknown (an API-key user has no windows).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resets_at: Option<i64>,
        /// The reset was seen (nothing resumes until the user's Resume).
        reset_seen: bool,
        hit_at: i64,
    },
    /// Tickets wait on it while it isn't available.
    Unavailable {
        /// "logged out", "not installed", "hidden in Settings", "update needed".
        reason: String,
        /// **Sign in…** applies (found, signed out, with a sign-in command).
        sign_in: bool,
        /// Hidden by the user: "Show in Agent tools" instead (Q-E).
        hidden: bool,
        since: i64,
    },
}

/// A tool that isn't available while tickets wait on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableSituation {
    pub tool: Tool,
    /// Tickets waiting (one per ticket, whatever its held roles).
    pub tickets: u32,
    /// The oldest hold's.
    pub since: i64,
}

/// The tickets each tool holds for `reason` that still wait: (tool → ticket ids, the oldest hold).
async fn waiting_for(
    board: &Board,
    reason: HoldReason,
) -> Result<BTreeMap<Tool, (Vec<String>, i64)>, BoardError> {
    let mut by_tool: BTreeMap<Tool, (Vec<String>, i64)> = BTreeMap::new();
    for hold in board.store().holds_for(reason).await? {
        let Some(id) = &hold.ticket_id else {
            continue;
        };
        let Ok(ticket) = board.ticket_by_id(id).await else {
            continue;
        };
        if !still_waits(&ticket, hold.role) {
            continue;
        }
        let (ids, since) = by_tool.entry(hold.tool).or_insert((Vec::new(), hold.since));
        if !ids.contains(id) {
            ids.push(id.clone());
        }
        *since = (*since).min(hold.since);
    }
    Ok(by_tool)
}

/// The tools that aren't available while tickets wait on them, for the line and its one notification (USG-10). A
/// tool with held tickets has a situation even when it was just seen available again (its holds are about to lift).
pub async fn unavailable_situations(
    board: &Board,
) -> Result<Vec<UnavailableSituation>, BoardError> {
    let held = waiting_for(board, HoldReason::ToolUnavailable).await?;
    Ok(held
        .into_iter()
        .map(|(tool, (ids, since))| UnavailableSituation {
            tool,
            tickets: count(ids.len()),
            since,
        })
        .collect())
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// What the lines read: the service's parts.
#[derive(Clone, Copy)]
pub struct LineSources<'a> {
    pub board: &'a Board,
    pub gate: &'a LaunchGate,
    pub plan_limit: &'a PlanLimit,
    pub connected: &'a ConnectedTools,
    pub needs_you: &'a NeedsYouQueue,
}

/// The banner's lines now (USG-10).
pub async fn tool_lines(src: LineSources<'_>) -> Result<ToolLines, BoardError> {
    let store = src.board.store();
    let facts = src.gate.facts().await?;
    let several_tools = src.connected.connected().len() > 1;
    let limits = plan::limits(store).await?;
    let held_limit = waiting_for(src.board, HoldReason::ToolLimit).await?;
    let unavailable = unavailable_situations(src.board).await?;
    let mut lines = Vec::new();
    for &tool in Tool::ALL {
        if let Some(limit) = limits.iter().find(|l| l.tool == tool) {
            let windows = plan::windows(store, tool).await?;
            let window_minutes = matching_window(
                src.plan_limit.adapter(tool),
                limit.kind,
                windows.iter().map(|w| &w.window),
            )
            .and_then(|w| w.length_minutes);
            lines.push(ToolLine {
                tool,
                situation: Situation::Limit {
                    kind: limit.kind,
                    window_minutes,
                    resets_at: limit.resets_at,
                    reset_seen: limit.reset_seen_at.is_some(),
                    hit_at: limit.hit_at,
                },
                stopped: src.plan_limit.stopped(tool).await?,
                waiting: held_limit.get(&tool).map_or(0, |(ids, _)| count(ids.len())),
                resume_enabled: limit.resets_at.is_none() || limit.reset_seen_at.is_some(),
            });
        }
        if let Some(situation) = unavailable.iter().find(|u| u.tool == tool) {
            let status = src.connected.status(tool);
            let reason = facts
                .unavailable
                .get(&tool)
                .cloned()
                .or_else(|| status.state.unavailable_reason().map(str::to_owned))
                .unwrap_or_else(|| "not available".to_owned());
            let since = src
                .needs_you
                .unavailable_since(tool)
                .unwrap_or(situation.since);
            lines.push(ToolLine {
                tool,
                situation: Situation::Unavailable {
                    reason,
                    sign_in: status.can_sign_in,
                    hidden: matches!(status.state, AgentToolState::Hidden { .. }),
                    since,
                },
                stopped: 0,
                waiting: situation.tickets,
                resume_enabled: false,
            });
        }
    }
    Ok(ToolLines {
        lines,
        several_tools,
    })
}

/// `limits.resume_tool`: **Resume `tool`** (the user's click): that tool's limit-stopped agents continue, its limit
/// clears, the validations that waited for a reset start (SCH-04, d0063; one whose validator's tool is still at
/// its limit is held again by its check), and the tool's holds lift (the held starts run, once). Returns the
/// tickets told to continue and those whose validator started, in key order.
pub async fn resume_tool(
    plan_limit: &PlanLimit,
    validation: &crate::validation::Validation,
    gate: &LaunchGate,
    tool: Tool,
) -> Result<Vec<TicketKey>, BoardError> {
    let mut resumed = plan_limit.resume_tool(tool).await?;
    let started = Box::pin(validation.start_waiting())
        .await
        .inspect_err(|e| tracing::warn!(error = %e, "can't start the validations that waited"))
        .unwrap_or_default();
    for id in started {
        if let Ok(ticket) = plan_limit.board().ticket_by_id(&id).await {
            resumed.push(ticket.key);
        }
    }
    gate.lift().await?;
    resumed.sort();
    resumed.dedup();
    Ok(resumed)
}
