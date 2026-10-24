//! Limits per tool (USG-10, USG-06, USG-07, AGM-03; d0086, d0108, d0020, d0046, d0018): the banner's data, one line
//! per tool ([`ToolLine`]), **Resume** per tool (`limits.resume_tool`); **Use fallback now** is the gate's
//! ([`super::gate::LaunchGate::use_fallback_now`]).
//!
//! A tool gets a line while its limit is active (session / weekly / another limit, reset known or not), and while
//! it isn't available (logged out, not installed, hidden, update needed) with tickets waiting on it (a held ticket
//! with reason `tool_unavailable`) or started on its fallback since it was last available ("Codex isn't available ·
//! logged out · 5 tickets on Claude Code"); a tool both at its limit and unavailable gets both. Each line says how
//! many agents the limit stopped, how many tickets wait on the tool (their holds), how many started on its fallback
//! since the line started, whether Resume can be pressed (after a known reset, or always when the reset is unknown,
//! USG-06), and the tool's fallback with whether Use fallback now can start it (AGM-03).

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use super::gate::{self, LaunchGate, still_waits};
use super::{HoldReason, ModelRef};
use crate::agent_tools::Tool;
use crate::agent_tools::connected::{AgentToolState, ConnectedTools};
use crate::attention::NeedsYouQueue;
use crate::board::{Board, BoardError};
use crate::domain::TicketKey;
use crate::usage::limit::{LimitKind, PlanLimit, matching_window};
use crate::usage::plan;

/// The banner's lines, in [`Tool::ALL`] order, and whether several tools are connected (with one, the banner reads
/// as before: no tool name, no fallback).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLines {
    pub lines: Vec<ToolLine>,
    pub several_tools: bool,
}

/// One tool's banner line (T-0411 words it): "Claude Code limit reached · resets 3:45pm · 4 tickets waiting ·
/// Resume Claude Code · Use fallback now", "Codex isn't available · logged out · 5 tickets waiting · Sign in…".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolLine {
    pub tool: Tool,
    pub situation: Situation,
    /// Agents the limit stopped mid-turn (Waiting for limit reset; Project agents too). 0 for an unavailable tool.
    pub stopped: u32,
    /// Tickets that would start on the tool and wait (their holds for this situation's reason).
    pub waiting: u32,
    /// Tickets started on the tool's fallback since the line started (Use fallback, Use fallback now).
    #[serde(default)]
    pub on_fallback: u32,
    /// Resume can be pressed: the limit reset, or its reset is unknown (USG-06). Never for an unavailable tool.
    pub resume_enabled: bool,
    /// The tool's fallback (AGM-03), with several tools connected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<ModelRef>,
    /// Use fallback now can start the waiting tickets there: the fallback passes the check before a launch.
    #[serde(default)]
    pub fallback_can_start: bool,
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

/// A tool that isn't available while tickets wait on it, or started on its fallback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableSituation {
    pub tool: Tool,
    /// Tickets waiting (one per ticket, whatever its held roles).
    pub tickets: u32,
    /// Tickets started on its fallback since it stopped being available.
    pub on_fallback: u32,
    /// The oldest hold's, or the first fallback start's.
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

/// The tools that aren't available while tickets wait on them, or while tickets started on their fallback (Use
/// fallback, Use fallback now), for the line and its one notification (USG-10). `unavailable` are the tools not
/// available now; fallback starts count from `floor(tool)` on (when it was last seen available). A tool with held
/// tickets has a situation even when it was just seen available again (its holds are about to lift).
pub async fn unavailable_situations(
    board: &Board,
    unavailable: &HashMap<Tool, String>,
    floor: impl Fn(Tool) -> i64,
) -> Result<Vec<UnavailableSituation>, BoardError> {
    let held = waiting_for(board, HoldReason::ToolUnavailable).await?;
    let mut out = Vec::new();
    for &tool in Tool::ALL {
        let held = held.get(&tool);
        let (on_fallback, first_move) = match unavailable.contains_key(&tool) {
            true => {
                on_fallback_since(board, tool, &[HoldReason::ToolUnavailable], floor(tool)).await?
            }
            false => (0, None),
        };
        if held.is_none() && on_fallback == 0 {
            continue;
        }
        let since = [held.map(|(_, since)| *since), first_move]
            .into_iter()
            .flatten()
            .min()
            .unwrap_or_default();
        out.push(UnavailableSituation {
            tool,
            tickets: held.map_or(0, |(ids, _)| count(ids.len())),
            on_fallback,
            since,
        });
    }
    Ok(out)
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
    let fallbacks = store.fallbacks().await?;
    let several_tools = src.connected.connected().len() > 1;
    let limits = plan::limits(store).await?;
    let held_limit = waiting_for(src.board, HoldReason::ToolLimit).await?;
    let unavailable = unavailable_situations(src.board, &facts.unavailable, |tool| {
        src.needs_you.available_floor(tool)
    })
    .await?;
    // The tool's fallback, with several tools connected, and whether it can start now.
    let fallback = |tool: Tool| {
        let fallback = fallbacks.get(&tool).filter(|_| several_tools).cloned();
        let can_start = fallback
            .as_ref()
            .is_some_and(|f| gate::check(f, &facts).is_ok());
        (fallback, can_start)
    };
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
                on_fallback: on_fallback_since(src.board, tool, &BOTH, limit.hit_at)
                    .await?
                    .0,
                resume_enabled: limit.resets_at.is_none() || limit.reset_seen_at.is_some(),
                fallback: fallback(tool).0,
                fallback_can_start: fallback(tool).1,
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
                on_fallback: situation.on_fallback,
                resume_enabled: false,
                fallback: fallback(tool).0,
                fallback_can_start: fallback(tool).1,
            });
        }
    }
    Ok(ToolLines {
        lines,
        several_tools,
    })
}

/// A limit line counts the fallback starts of both kinds.
const BOTH: [HoldReason; 2] = [HoldReason::ToolLimit, HoldReason::ToolUnavailable];

/// The tickets started on `tool`'s fallback for one of `reasons` since `since`, and when the first of them started.
async fn on_fallback_since(
    board: &Board,
    tool: Tool,
    reasons: &[HoldReason],
    since: i64,
) -> Result<(u32, Option<i64>), BoardError> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT ticket_id, fallback_reason, MIN(started_at) FROM sessions
         WHERE fallback_from = ? AND started_at >= ? AND ticket_id IS NOT NULL AND fallback_reason IS NOT NULL
         GROUP BY ticket_id, fallback_reason",
    )
    .bind(tool.as_str())
    .bind(since)
    .fetch_all(board.store().pool())
    .await?;
    let mut tickets: Vec<&str> = Vec::new();
    let mut first: Option<i64> = None;
    for (ticket, reason, at) in &rows {
        if !reasons.iter().any(|r| r.as_str() == reason) {
            continue;
        }
        first = Some(first.map_or(*at, |f| f.min(*at)));
        if !tickets.contains(&ticket.as_str()) {
            tickets.push(ticket);
        }
    }
    Ok((count(tickets.len()), first))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_limit_line_counts_both_kinds_of_fallback_start() {
        assert_eq!(BOTH, [HoldReason::ToolLimit, HoldReason::ToolUnavailable]);
    }
}
