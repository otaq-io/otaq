//! The usage-limit banner (USG-06, USG-07, USG-08, USG-10, AGT-08; d0020, d0046, d0086; *11 · Banners → Usage limit ·
//! just hit, waiting for reset, after reset, weekly, reset time unknown*, *Row · Limit banner per tool (0086)*, *Row ·
//! Not available, safety net, warnings per tool (0086)*).
//!
//! One banner for the situation, never one per agent (P6), amber like the paused banner, with **one line per tool**
//! ([`ToolLine`], T-0358): its limit ("Claude Code limit reached · Its 3 agents wait; agents on other tools keep
//! working. Nothing switches tools." with "0% left · resets 3:45 pm" and **Resume Claude Code**) or its being
//! unavailable while tickets wait on it ("Codex isn't available · logged out · 5 tickets waiting", Sign in…). Only
//! that tool's agents wait; the rest keep going, and nothing ever moves to another tool: there is no fallback. Resume
//! is the user's click and never happens by itself: off until a known reset time, the call to action once the reset
//! time has passed ("The limit has reset. Its 3 agents are waiting where they stopped."), always on when the reset
//! time is unknown (USG-06). The line always names its tool.
//!
//! Never turquoise, which means needs you (d0077).

use otaq_core::agent_tools::Tool;
use otaq_core::agents::limits::{self as moves, ToolLine, ToolLines};
use otaq_core::local_time::local;
use otaq_core::usage::limit::LimitKind;
use serde::Serialize;
use ts_rs::TS;

use crate::icons::IconName;
use crate::view_models::banners::BannerTone;
use crate::view_models::clock::{WEEKDAYS, clock_12h, in_label, reset_label};
use crate::view_models::tool_avatar::ToolAvatarView;
use crate::view_models::tool_problems::{Action, SHOW_IN_AGENT_TOOLS, SIGN_IN, tool_word};

/// The error toast when a limit's Resume failed.
pub const RESUME_FAILED: &str = "Resume failed";

/// The error toast when Resume all of the interrupted sessions failed (REC-01, not a limit's).
pub const RESUME_ALL_FAILED: &str = "Resume all failed";

/// A month's window: 28 to 31 days.
const MONTH_MINUTES: std::ops::RangeInclusive<u32> = 28 * 24 * 60..=31 * 24 * 60;

/// The toast after Resume: "8 agents told to continue" and what happens next.
pub fn resumed(agents: usize) -> (String, String) {
    let noun = if agents == 1 { "agent" } else { "agents" };
    (
        format!("{agents} {noun} told to continue"),
        "Each continues once it is idle and its input line is empty.".to_owned(),
    )
}

/// The Resume button: "Resume Claude Code", always naming the tool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeButton {
    /// "Resume Claude Code"; the paused banner's "Resume".
    pub label: String,
    pub enabled: bool,
    /// Highlighted (primary) after the reset.
    pub primary: bool,
}

/// What one tool's line says, for the banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitBanner {
    /// "Claude Code limit reached", "Claude Code weekly limit reached", "Codex isn't available".
    pub title: String,
    /// "Its 3 agents wait; agents on other tools keep working. Nothing switches tools."
    pub text: String,
    /// On the right before Resume: "0% left · resets 3:45 pm", or "Reset at 3:45 pm" after the reset.
    pub aside: Option<String>,
    /// Resume (a limit); `None` for a tool that isn't available.
    pub resume: Option<ResumeButton>,
    /// The reset has happened.
    pub after_reset: bool,
}

/// What a line's button does. Resume and Sign in… carry the tool as stored (`codex`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LimitLineButton")]
pub struct LineButton {
    pub label: String,
    pub icon: IconName,
    /// `resume_tool` → `limits.resume_tool { tool }`; `sign_in` → `agent_tools.sign_in`; `show_in_agent_tools` →
    /// Settings → Agent tools.
    pub action: Action,
    pub enabled: bool,
    pub primary: bool,
}

/// One tool's line as the page draws it. `LimitLine` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LimitLine")]
pub struct LineView {
    /// The tool, as stored (`claude`).
    pub tool: String,
    /// Hidden while only one tool is connected.
    pub avatar: Option<ToolAvatarView>,
    /// Amber, a limit or a tool that isn't available; never turquoise (d0077).
    pub tone: BannerTone,
    pub icon: IconName,
    pub title: String,
    pub text: String,
    /// "0% left · resets 3:45 pm" on the right; `None` for a tool that isn't available.
    pub aside: Option<String>,
    /// Resume, Sign in… or Show in Agent tools.
    pub buttons: Vec<LineButton>,
}

/// A tool's line: the words and the drawn line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitLine {
    pub tool: Tool,
    pub banner: LimitBanner,
    pub view: LineView,
    /// What the line stands for, apart from the clock: its tool, its limit and reset, whether the reset passed (or
    /// why the tool isn't available). A closed limit banner shows again once any line's state differs.
    pub state: String,
}

/// The banner's lines at `now` (USG-10), in the service's order, from its [`ToolLines`]. A limit that has reset with
/// nothing left to resume has no line.
pub fn lines(tool_lines: &ToolLines, now: i64) -> Vec<LimitLine> {
    tool_lines
        .lines
        .iter()
        .filter_map(|l| line(l, tool_lines.several_tools, now))
        .collect()
}

fn line(l: &ToolLine, several: bool, now: i64) -> Option<LimitLine> {
    let state;
    let banner = match &l.situation {
        moves::Situation::Limit {
            kind,
            window_minutes,
            resets_at,
            reset_seen,
            ..
        } => {
            let limit = Limit {
                kind: *kind,
                monthly: window_minutes.is_some_and(|m| MONTH_MINUTES.contains(&m)),
                resets_at: *resets_at,
                reset_passed: *reset_seen || resets_at.is_some_and(|at| at <= now),
            };
            state = format!(
                "{}:limit:{:?}:{resets_at:?}:{}",
                l.tool.short_name(),
                limit.kind,
                limit.reset_passed
            );
            limit_line(l, &limit, now, several)?
        }
        moves::Situation::Unavailable { reason, .. } => {
            state = format!("{}:unavailable:{reason:?}", l.tool.short_name());
            unavailable_line(l, reason)
        }
    };
    let view = view(l, &banner, several);
    Some(LimitLine {
        tool: l.tool,
        banner,
        view,
        state,
    })
}

/// A limit as the line words it.
struct Limit {
    kind: LimitKind,
    /// The limit is on a month's window (Copilot's plan).
    monthly: bool,
    resets_at: Option<i64>,
    /// Seen reset, or its reset time passed.
    reset_passed: bool,
}

fn plural(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn tickets_n(n: u32) -> String {
    plural(n, "ticket", "tickets")
}

/// The reset at `at`, as the banner says it: "Resets in 42 min (3:45 pm)" in the last hour, "Resets at 3:45 pm"
/// (`at` is "at " with one tool, empty with several, as the frames), "Resets Mon 12:00 am" within a week, else
/// "Resets Nov 1". The second part is true in the last hour.
fn resets(at: i64, now: i64, at_word: &str) -> (String, bool) {
    let left = at - now;
    let clock = local(at)
        .map(|t| clock_12h(t.hour, t.minute))
        .unwrap_or_default();
    if left <= 3_600_000 {
        (format!("Resets in {} ({clock})", in_label(left)), true)
    } else if left < 24 * 3_600_000 {
        (format!("Resets {at_word}{clock}"), false)
    } else if left < 7 * 24 * 3_600_000 {
        let day = local(at)
            .map(|t| WEEKDAYS[t.weekday as usize])
            .unwrap_or_default();
        (format!("Resets {day} {clock}"), false)
    } else {
        (format!("Resets {}", reset_label(at, now)), false)
    }
}

/// What waits on a tool: "4 tickets waiting".
fn waiting_parts(waiting: u32) -> Vec<String> {
    match waiting {
        0 => Vec::new(),
        n => vec![format!("{} waiting", tickets_n(n))],
    }
}

/// "Its 3 agents", "Its agent": what of the tool's work waits.
fn its(n: u32, one: &str, many: &str) -> String {
    if n == 1 {
        format!("Its {one}")
    } else {
        format!("Its {n} {many}")
    }
}

/// The reset as the line's right side says it: "resets 3:45 pm", "resets in 42 min (3:45 pm)", "resets Mon 12:00 am".
fn resets_aside(at: i64, now: i64) -> String {
    let (when, _) = resets(at, now, "");
    let mut chars = when.chars();
    match chars.next() {
        Some(c) => c.to_lowercase().chain(chars).collect(),
        None => when,
    }
}

/// A tool's limit line: "Claude Code limit reached · Its 3 agents wait; agents on other tools keep working. Nothing
/// switches tools." with "0% left · resets 3:45 pm" and Resume Claude Code, off until the reset; after the reset "The
/// limit has reset. Its 3 agents are waiting where they stopped." with "Reset at 3:45 pm" and Resume Claude Code on.
/// Only that tool's agents wait, and nothing moves to another tool. `several`: more than one tool is connected.
fn limit_line(l: &ToolLine, limit: &Limit, now: i64, several: bool) -> Option<LimitBanner> {
    let word = tool_word(l.tool);
    let resume = format!("Resume {word}");
    let title = match (limit.kind, limit.monthly) {
        (LimitKind::Weekly, _) => format!("{word} weekly limit reached"),
        (_, true) => format!("{word} monthly limit reached"),
        _ => format!("{word} limit reached"),
    };
    if limit.reset_passed {
        if l.stopped == 0 && l.waiting == 0 {
            return None;
        }
        let mut text = "The limit has reset. ".to_owned();
        if l.stopped > 0 {
            let (are, where_) = if l.stopped == 1 {
                ("is", "it")
            } else {
                ("are", "they")
            };
            text.push_str(&format!(
                "{} {are} waiting where {where_} stopped",
                its(l.stopped, "agent", "agents")
            ));
            if l.waiting > 0 {
                text.push_str(&format!("; {} wait too", tickets_n(l.waiting)));
            }
        } else {
            let verb = if l.waiting == 1 { "waits" } else { "wait" };
            text.push_str(&format!(
                "{} {verb} to start",
                its(l.waiting, "ticket", "tickets")
            ));
        }
        text.push('.');
        return Some(LimitBanner {
            title,
            text,
            aside: limit
                .resets_at
                .and_then(local)
                .map(|t| format!("Reset at {}", clock_12h(t.hour, t.minute))),
            resume: Some(ResumeButton {
                label: resume,
                enabled: true,
                primary: true,
            }),
            after_reset: true,
        });
    }
    let waits = if l.stopped > 0 {
        let verb = if l.stopped == 1 { "waits" } else { "wait" };
        format!("{} {verb}", its(l.stopped, "agent", "agents"))
    } else if l.waiting > 0 {
        let verb = if l.waiting == 1 { "waits" } else { "wait" };
        format!("{} {verb}", its(l.waiting, "ticket", "tickets"))
    } else {
        "Its tickets wait".to_owned()
    };
    let others = if several {
        "; agents on other tools keep working"
    } else {
        ""
    };
    let reset = match limit.resets_at {
        Some(at) => resets_aside(at, now),
        None => "reset time unknown".to_owned(),
    };
    Some(LimitBanner {
        title,
        text: format!("{waits}{others}. Nothing switches tools."),
        aside: Some(format!("0% left \u{b7} {reset}")),
        resume: Some(ResumeButton {
            label: resume,
            // Off until the reset time; always on when the tool didn't say when it resets.
            enabled: l.resume_enabled || limit.resets_at.is_none(),
            primary: false,
        }),
        after_reset: false,
    })
}

/// A tool that isn't available while tickets wait on it (USG-10): "Codex isn't available" over
/// "logged out · 5 tickets waiting". It names the tool also with one tool connected: nothing else says which.
fn unavailable_line(l: &ToolLine, reason: &str) -> LimitBanner {
    let word = tool_word(l.tool);
    let title = format!("{word} isn\u{2019}t available");
    let mut parts = vec![reason.to_owned()];
    parts.extend(waiting_parts(l.waiting));
    let text = parts.join(" \u{b7} ");
    LimitBanner {
        title,
        text,
        aside: None,
        resume: None,
        after_reset: false,
    }
}

fn view(l: &ToolLine, banner: &LimitBanner, several: bool) -> LineView {
    let tool = l.tool.as_str().to_owned();
    let mut buttons = Vec::new();
    let (tone, icon) = match &l.situation {
        moves::Situation::Limit { .. } => (BannerTone::Warning, IconName::Gauge),
        moves::Situation::Unavailable { .. } => (BannerTone::Warning, IconName::TriangleAlert),
    };
    if let Some(resume) = &banner.resume {
        buttons.push(LineButton {
            label: resume.label.clone(),
            icon: IconName::Play,
            action: Action::ResumeTool { tool: tool.clone() },
            enabled: resume.enabled,
            primary: resume.primary,
        });
    }
    if let moves::Situation::Unavailable {
        sign_in, hidden, ..
    } = &l.situation
    {
        // Sign in… where it applies; a hidden tool offers Show in Agent tools instead (Q111).
        if *sign_in {
            buttons.push(LineButton {
                label: SIGN_IN.to_owned(),
                icon: IconName::Plug,
                action: Action::SignIn { tool: tool.clone() },
                enabled: true,
                primary: false,
            });
        } else if *hidden {
            buttons.push(LineButton {
                label: SHOW_IN_AGENT_TOOLS.to_owned(),
                icon: IconName::Eye,
                action: Action::ShowInAgentTools,
                enabled: true,
                primary: false,
            });
        }
    }
    LineView {
        tool,
        avatar: several.then(|| ToolAvatarView::of(l.tool, None)),
        tone,
        icon,
        title: banner.title.clone(),
        text: banner.text.clone(),
        aside: banner.aside.clone(),
        buttons,
    }
}

/// Sample lines for the gallery and the tests: a fixed clock, Claude Code at its limit, Codex logged out.
pub mod fixtures {
    use otaq_core::agent_tools::Tool;
    use otaq_core::agents::limits::{Situation, ToolLine, ToolLines};
    use otaq_core::usage::limit::LimitKind;

    /// A fixed moment (2026-10-08, around noon UTC).
    pub const NOW: i64 = 1_791_547_200_000;
    pub const MINUTE: i64 = 60_000;
    pub const HOUR: i64 = 60 * MINUTE;

    pub fn limit(
        tool: Tool,
        kind: LimitKind,
        resets_in: Option<i64>,
        reset_seen: bool,
    ) -> ToolLine {
        ToolLine {
            tool,
            situation: Situation::Limit {
                kind,
                window_minutes: None,
                resets_at: resets_in.map(|ms| NOW + ms),
                reset_seen,
                hit_at: NOW - 10 * MINUTE,
            },
            stopped: 0,
            waiting: 4,
            resume_enabled: resets_in.is_none() || reset_seen,
        }
    }

    pub fn unavailable(tool: Tool, reason: &str, sign_in: bool, hidden: bool) -> ToolLine {
        ToolLine {
            tool,
            situation: Situation::Unavailable {
                reason: reason.into(),
                sign_in,
                hidden,
                since: NOW - 5 * MINUTE,
            },
            stopped: 0,
            waiting: 5,
            resume_enabled: false,
        }
    }

    /// Claude Code's limit, resetting in 4 hours.
    pub fn claude_at_limit() -> ToolLine {
        limit(Tool::Claude, LimitKind::Session, Some(4 * HOUR), false)
    }

    pub fn several(lines: Vec<ToolLine>) -> ToolLines {
        ToolLines {
            lines,
            several_tools: true,
        }
    }

    pub fn one(line: ToolLine) -> ToolLines {
        ToolLines {
            lines: vec![line],
            several_tools: false,
        }
    }
}

/// The gallery's banner lines (*11 → Row · Limit banner per tool*, *Not available*).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LimitLineGallery")]
pub struct Gallery {
    pub cells: Vec<GalleryCell>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LimitLineGalleryCell")]
pub struct GalleryCell {
    pub name: String,
    pub lines: Vec<LineView>,
}

pub fn gallery() -> Gallery {
    use fixtures::*;
    let codex_limit = ToolLine {
        stopped: 1,
        waiting: 2,
        ..limit(Tool::Codex, LimitKind::Session, Some(5 * HOUR), false)
    };
    let cells = vec![
        ("One tool at its limit", several(vec![claude_at_limit()])),
        (
            "Two tools at their limits",
            several(vec![claude_at_limit(), codex_limit]),
        ),
        (
            "A tool isn't available",
            several(vec![unavailable(Tool::Codex, "signed out", true, false)]),
        ),
        (
            "Hidden in Settings",
            several(vec![unavailable(
                Tool::Codex,
                "hidden in Settings",
                false,
                true,
            )]),
        ),
        (
            "Last hour",
            several(vec![limit(
                Tool::Claude,
                LimitKind::Session,
                Some(42 * MINUTE),
                false,
            )]),
        ),
        (
            "After the reset",
            several(vec![ToolLine {
                stopped: 8,
                waiting: 0,
                ..limit(Tool::Claude, LimitKind::Session, Some(-HOUR), true)
            }]),
        ),
        (
            "Weekly",
            several(vec![limit(
                Tool::Claude,
                LimitKind::Weekly,
                Some(3 * 24 * HOUR),
                false,
            )]),
        ),
        (
            "Reset time unknown",
            several(vec![limit(Tool::Codex, LimitKind::Other, None, false)]),
        ),
        (
            "One tool connected",
            one(ToolLine {
                stopped: 8,
                ..limit(Tool::Claude, LimitKind::Session, Some(3 * HOUR), false)
            }),
        ),
    ];
    Gallery {
        cells: cells
            .into_iter()
            .map(|(name, l)| GalleryCell {
                name: name.to_owned(),
                lines: lines(&l, NOW).into_iter().map(|l| l.view).collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use otaq_core::attention::notify;

    use super::fixtures::*;
    use super::*;

    fn clock(at: i64) -> String {
        let t = local(at).unwrap();
        clock_12h(t.hour, t.minute)
    }

    fn one_line(l: &ToolLines) -> LimitLine {
        let mut all = lines(l, NOW);
        assert_eq!(all.len(), 1, "{all:?}");
        all.remove(0)
    }

    fn stopped(n: u32, l: ToolLine) -> ToolLine {
        ToolLine { stopped: n, ..l }
    }

    #[test]
    fn no_line_no_banner() {
        assert!(lines(&ToolLines::default(), NOW).is_empty());
    }

    /// A tool at its limit: only its agents wait, nothing switches tools, and Resume names the tool, off until the
    /// reset time.
    #[test]
    fn a_limit_line_says_what_waits_and_when_it_resets() {
        let l = one_line(&one(stopped(
            3,
            limit(Tool::Claude, LimitKind::Session, Some(3 * HOUR), false),
        )));
        let b = &l.banner;
        assert_eq!(b.title, "Claude Code limit reached");
        assert_eq!(b.text, "Its 3 agents wait. Nothing switches tools.");
        assert_eq!(
            b.aside.as_deref(),
            Some(format!("0% left \u{b7} resets {}", clock(NOW + 3 * HOUR)).as_str())
        );
        assert_eq!(
            b.resume,
            Some(ResumeButton {
                label: "Resume Claude Code".into(),
                enabled: false,
                primary: false
            })
        );
        // One tool: no avatar; the line still names it.
        assert_eq!(l.view.avatar, None);
        assert_eq!(l.view.tone, BannerTone::Warning, "amber");
        assert_eq!(l.view.buttons.len(), 1);
        assert_eq!(
            l.view.buttons[0].action,
            Action::ResumeTool {
                tool: "claude".into()
            }
        );
        // Several tools: the others keep working.
        let two = one_line(&several(vec![stopped(
            3,
            limit(Tool::Claude, LimitKind::Session, Some(3 * HOUR), false),
        )]));
        assert_eq!(
            two.view.text,
            "Its 3 agents wait; agents on other tools keep working. Nothing switches tools."
        );
        assert_eq!(two.view.avatar.as_ref().unwrap().letters, "CC");
        // One agent, or only tickets waiting.
        let agent = one_line(&one(stopped(
            1,
            limit(Tool::Claude, LimitKind::Session, Some(3 * HOUR), false),
        )));
        assert_eq!(
            agent.banner.text,
            "Its agent waits. Nothing switches tools."
        );
        let tickets = one_line(&one(claude_at_limit()));
        assert_eq!(
            tickets.banner.text,
            "Its 4 tickets wait. Nothing switches tools."
        );
    }

    /// Resume is the user's click once the reset time has passed: off before, the call to action after, also while
    /// the service hasn't seen the reset yet.
    #[test]
    fn resume_is_active_once_the_reset_time_has_passed() {
        let at = NOW + 3 * HOUR;
        let tool_lines = one(ToolLine {
            stopped: 3,
            waiting: 0,
            ..limit(Tool::Claude, LimitKind::Session, Some(3 * HOUR), false)
        });
        let before = lines(&tool_lines, at - MINUTE).remove(0);
        assert!(!before.view.buttons[0].enabled);
        let after = lines(&tool_lines, at + MINUTE).remove(0);
        assert!(after.banner.after_reset);
        assert_eq!(after.view.title, "Claude Code limit reached");
        assert_eq!(
            after.view.text,
            "The limit has reset. Its 3 agents are waiting where they stopped."
        );
        assert_eq!(
            after.view.aside.as_deref(),
            Some(format!("Reset at {}", clock(at)).as_str())
        );
        let resume = &after.view.buttons[0];
        assert_eq!(resume.label, "Resume Claude Code");
        assert!(resume.enabled && resume.primary);
        // Nothing left to resume: no line.
        assert!(
            lines(
                &one(ToolLine {
                    waiting: 0,
                    ..limit(Tool::Claude, LimitKind::Session, Some(-MINUTE), true)
                }),
                NOW
            )
            .is_empty()
        );
        // Tickets only.
        let tickets = one_line(&one(limit(
            Tool::Claude,
            LimitKind::Session,
            Some(-MINUTE),
            true,
        )));
        assert_eq!(
            tickets.view.text,
            "The limit has reset. Its 4 tickets wait to start."
        );
    }

    #[test]
    fn each_reset_variant_per_tool() {
        let view = |l: ToolLine| one_line(&several(vec![l])).view;
        let last = view(limit(
            Tool::Claude,
            LimitKind::Session,
            Some(42 * MINUTE),
            false,
        ));
        assert_eq!(
            last.aside.as_deref(),
            Some(
                format!(
                    "0% left \u{b7} resets in 42 min ({})",
                    clock(NOW + 42 * MINUTE)
                )
                .as_str()
            )
        );
        let week = view(limit(
            Tool::Claude,
            LimitKind::Weekly,
            Some(3 * 24 * HOUR),
            false,
        ));
        assert_eq!(week.title, "Claude Code weekly limit reached");
        let t = local(NOW + 3 * 24 * HOUR).unwrap();
        assert_eq!(
            week.aside.as_deref(),
            Some(
                format!(
                    "0% left \u{b7} resets {} {}",
                    WEEKDAYS[t.weekday as usize],
                    clock_12h(t.hour, t.minute)
                )
                .as_str()
            )
        );
        let mut month = limit(Tool::Copilot, LimitKind::Other, Some(20 * 24 * HOUR), false);
        if let moves::Situation::Limit { window_minutes, .. } = &mut month.situation {
            *window_minutes = Some(30 * 24 * 60);
        }
        let month = view(month);
        assert_eq!(month.title, "Copilot monthly limit reached");
        let unknown = view(limit(Tool::Codex, LimitKind::Other, None, false));
        assert_eq!(unknown.title, "Codex limit reached");
        assert_eq!(
            unknown.aside.as_deref(),
            Some("0% left \u{b7} reset time unknown")
        );
        assert!(unknown.buttons[0].enabled, "always available when unknown");
        assert_eq!(unknown.buttons[0].label, "Resume Codex");
    }

    #[test]
    fn two_tools_at_their_limits_get_a_line_each() {
        let codex = stopped(
            1,
            ToolLine {
                waiting: 2,
                ..limit(Tool::Codex, LimitKind::Session, Some(5 * HOUR), false)
            },
        );
        let all = lines(&several(vec![claude_at_limit(), codex]), NOW);
        assert_eq!(all.len(), 2);
        let (cc, cx) = (&all[0], &all[1]);
        assert_eq!(cc.view.title, "Claude Code limit reached");
        assert_eq!(cx.view.title, "Codex limit reached");
        assert_eq!(
            cx.view.text,
            "Its agent waits; agents on other tools keep working. Nothing switches tools."
        );
        let resume = &cx.view.buttons[0];
        assert_eq!(resume.label, "Resume Codex");
        assert!(!resume.enabled, "disabled until the reset");
        assert_eq!(cc.view.icon, IconName::Gauge);
        // Only Resume: nothing moves to another tool.
        assert_eq!(cx.view.buttons.len(), 1);
        assert_eq!(cc.view.buttons.len(), 1);
    }

    #[test]
    fn the_unavailable_line_offers_sign_in_or_show_in_agent_tools() {
        let l = one_line(&several(vec![unavailable(
            Tool::Codex,
            "signed out",
            true,
            false,
        )]));
        assert_eq!(l.view.title, "Codex isn\u{2019}t available");
        assert_eq!(l.view.text, "signed out · 5 tickets waiting");
        assert_eq!(
            (l.view.tone, l.view.icon),
            (BannerTone::Warning, IconName::TriangleAlert),
            "amber"
        );
        let labels: Vec<&str> = l.view.buttons.iter().map(|b| b.label.as_str()).collect();
        assert_eq!(labels, ["Sign in\u{2026}"]);
        assert_eq!(
            l.view.buttons[0].action,
            Action::SignIn {
                tool: "codex".into()
            }
        );
        assert_eq!(l.banner.resume, None);
        let hidden = one_line(&several(vec![unavailable(
            Tool::Codex,
            "hidden in Settings",
            false,
            true,
        )]));
        assert_eq!(hidden.view.buttons[0].label, "Show in Agent tools");
        assert_eq!(hidden.view.buttons[0].action, Action::ShowInAgentTools);
        // One tool connected: the line still names the tool.
        let only = one_line(&one(unavailable(Tool::Claude, "signed out", true, false)));
        assert_eq!(only.view.title, "Claude Code isn\u{2019}t available");
        assert_eq!(only.view.avatar, None);
        assert_eq!(only.view.buttons.len(), 1);
    }

    #[test]
    fn the_toasts() {
        assert_eq!(resumed(1).0, "1 agent told to continue");
        assert_eq!(resumed(8).0, "8 agents told to continue");
        assert_eq!(
            resumed(8).1,
            "Each continues once it is idle and its input line is empty."
        );
    }

    #[test]
    fn the_notification_matches_the_line() {
        let n = notify::tool_unavailable(Tool::Codex, 5);
        assert_eq!(n.title, "Codex isn\u{2019}t available");
        assert_eq!(n.body, "5 tickets waiting");
        let l = one_line(&several(vec![unavailable(
            Tool::Codex,
            "signed out",
            true,
            false,
        )]));
        assert_eq!(l.view.title, n.title);
        assert!(l.view.text.ends_with(&n.body));
    }

    /// No button on the banner moves tickets to another tool: it resumes the tool, signs in or opens Settings.
    #[test]
    fn no_button_moves_tickets() {
        for cell in gallery().cells {
            for line in cell.lines {
                for b in &line.buttons {
                    match &b.action {
                        Action::ResumeTool { .. }
                        | Action::SignIn { .. }
                        | Action::ShowInAgentTools => {}
                        other => panic!("{other:?} on the banner"),
                    }
                }
            }
        }
    }

    #[test]
    fn no_line_is_turquoise() {
        let g = gallery();
        for line in g.cells.iter().flat_map(|c| &c.lines) {
            assert_ne!(line.tone, BannerTone::Attention, "{line:?}");
            assert_eq!(line.tone, BannerTone::Warning, "amber: {line:?}");
        }
    }

    #[test]
    fn the_gallery_has_every_cell_and_no_dollar() {
        let g = gallery();
        assert_eq!(g.cells.len(), 9);
        assert!(g.cells.iter().all(|c| !c.lines.is_empty()), "{g:?}");
        assert!(!serde_json::to_string(&g).unwrap().contains('$'));
    }
}
