//! The usage-limit banner (USG-06, USG-07, USG-08, USG-10, AGT-08; d0020, d0046, d0086; *11 · Banners → Usage limit ·
//! just hit, waiting for reset, after reset, weekly, reset time unknown*, *Row · Limit banner per tool (0086)*, *Row ·
//! Not available, safety net, warnings per tool (0086)*).
//!
//! One banner for the situation, never one per agent (P6), with **one line per tool** ([`ToolLine`], T-0358): its
//! limit ("Claude Code limit reached · Resets 3:45 pm · 4 tickets waiting", Resume Claude Code) or its being
//! unavailable while tickets wait on it ("Codex isn't available · logged out · 5 tickets waiting", Sign in…). Only that
//! tool's agents wait; the rest keep going, and nothing moves to another tool. Each says when the limit resets (a
//! countdown in the last hour), how many agents it stopped and what waits. Resume is the user's click and never
//! happens by itself: disabled until a known reset time, highlighted after the reset, always available when the reset
//! time is unknown (USG-06). With one tool connected the banner reads as before M23 with its one Resume (no tool name,
//! d0086; USG-10: Resume all became per tool).
//!
//! Limit lines are neutral and a tool that isn't available is amber: never turquoise, which means needs you (d0077;
//! the frames' turquoise lines predate the rule).

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

/// The Resume button: "Resume" with one tool, "Resume Claude Code" with several.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeButton {
    /// "Resume", "Resume (8)", "Resume Claude Code (8)".
    pub label: String,
    pub enabled: bool,
    /// Highlighted (primary) after the reset.
    pub primary: bool,
}

/// What one tool's line says, for the banner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitBanner {
    /// "Session limit reached", "Claude Code limit reached", "Usage limit reset", "Codex isn't available".
    pub title: String,
    /// "Resets at 3:45 pm · 8 agents stopped · no new tickets will start".
    pub text: String,
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
    /// A limit neutral, a tool that isn't available amber; never turquoise (d0077).
    pub tone: BannerTone,
    pub icon: IconName,
    pub title: String,
    pub text: String,
    /// Resume, Sign in… or Show in Agent tools.
    pub buttons: Vec<LineButton>,
}

/// A tool's line: the words and the drawn line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitLine {
    pub tool: Tool,
    pub banner: LimitBanner,
    pub view: LineView,
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
            if several {
                limit_line(l, &limit, now)?
            } else {
                one_tool_line(l.stopped, &limit, now)?
            }
        }
        moves::Situation::Unavailable { reason, .. } => unavailable_line(l, reason),
    };
    let view = view(l, &banner, several);
    Some(LimitLine {
        tool: l.tool,
        banner,
        view,
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

/// A banner's text as sentences (d0093: the middle dot joins metadata only): the first part, then the rest joined by
/// semicolons: "Resets at 3:45 pm. 8 agents stopped; no new tickets will start."
fn sentences(parts: &[String]) -> String {
    let Some((first, rest)) = parts.split_first() else {
        return String::new();
    };
    if rest.is_empty() {
        return format!("{first}.");
    }
    let rest = rest.join("; ");
    let mut chars = rest.chars();
    let rest = match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect::<String>(),
        None => rest,
    };
    format!("{first}. {rest}.")
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

/// When the limit reset: " at 3:00 pm", or nothing when unknown.
fn reset_at(limit: &Limit) -> String {
    limit
        .resets_at
        .and_then(local)
        .map(|t| format!(" at {}", clock_12h(t.hour, t.minute)))
        .unwrap_or_default()
}

/// The line with one tool connected: the words of the banner before M23.
fn one_tool_line(stopped: u32, limit: &Limit, now: i64) -> Option<LimitBanner> {
    let agents = |verb: &str| format!("{} {verb}", plural(stopped, "agent", "agents"));
    if limit.reset_passed {
        if stopped == 0 {
            return None;
        }
        let are = if stopped == 1 { "is" } else { "are" };
        return Some(LimitBanner {
            title: "Usage limit reset".to_owned(),
            text: sentences(&[
                format!("Your plan limit reset{}", reset_at(limit)),
                format!("{} {are} waiting", plural(stopped, "agent", "agents")),
                "nothing resumes until you click".to_owned(),
            ]),
            resume: Some(ResumeButton {
                label: format!("Resume ({stopped})"),
                enabled: true,
                primary: true,
            }),
            after_reset: true,
        });
    }
    let Some(at) = limit.resets_at else {
        return Some(LimitBanner {
            title: "Usage limit reached".to_owned(),
            text: sentences(&[
                "Reset time unknown".to_owned(),
                agents("waiting for limit reset"),
                "no new tickets will start".to_owned(),
            ]),
            resume: Some(ResumeButton {
                label: "Resume".to_owned(),
                enabled: true,
                primary: false,
            }),
            after_reset: false,
        });
    };
    let title = match limit.kind {
        LimitKind::Session => "Session limit reached",
        LimitKind::Weekly => "Weekly limit reached",
        LimitKind::Other => "Usage limit reached",
    };
    let (when, last_hour) = resets(at, now, "at ");
    let tail = if last_hour {
        "tickets in Ready for Dev wait"
    } else {
        "no new tickets will start"
    };
    Some(LimitBanner {
        title: title.to_owned(),
        text: sentences(&[when.clone(), agents("stopped"), tail.to_owned()]),
        resume: Some(ResumeButton {
            label: "Resume".to_owned(),
            enabled: false,
            primary: false,
        }),
        after_reset: false,
    })
}

/// What waits on a tool: "4 tickets waiting".
fn waiting_parts(waiting: u32) -> Vec<String> {
    match waiting {
        0 => Vec::new(),
        n => vec![format!("{} waiting", tickets_n(n))],
    }
}

/// A tool's limit line with several tools connected (USG-10): "Claude Code limit reached · Resets 3:45 pm · 4 tickets
/// waiting", Resume Claude Code.
fn limit_line(l: &ToolLine, limit: &Limit, now: i64) -> Option<LimitBanner> {
    let word = tool_word(l.tool);
    let resume = format!("Resume {word}");
    if limit.reset_passed {
        if l.stopped == 0 && l.waiting == 0 {
            return None;
        }
        let mut parts = vec![format!("{word}\u{2019}s limit reset{}", reset_at(limit))];
        if l.stopped > 0 {
            let are = if l.stopped == 1 { "is" } else { "are" };
            parts.push(format!(
                "{} {are} waiting",
                plural(l.stopped, "agent", "agents")
            ));
        }
        parts.extend(waiting_parts(l.waiting));
        parts.push("nothing resumes until you click".to_owned());
        let label = match l.stopped {
            0 => resume.clone(),
            n => format!("{resume} ({n})"),
        };
        return Some(LimitBanner {
            title: format!("{word} limit reset"),
            text: sentences(&parts),
            resume: Some(ResumeButton {
                label,
                enabled: true,
                primary: true,
            }),
            after_reset: true,
        });
    }
    let title = match (limit.kind, limit.monthly) {
        (LimitKind::Weekly, _) => format!("{word} weekly limit reached"),
        (_, true) => format!("{word} monthly limit reached"),
        _ => format!("{word} limit reached"),
    };
    let when = match limit.resets_at {
        Some(at) => resets(at, now, "").0,
        None => format!("{word} didn\u{2019}t say when it resets"),
    };
    let mut parts = vec![when];
    parts.extend(waiting_parts(l.waiting));
    if l.stopped > 0 {
        let stopped = format!(
            "{} stopped mid-ticket",
            plural(l.stopped, "agent", "agents")
        );
        parts.push(stopped);
    }
    Some(LimitBanner {
        title,
        text: sentences(&parts),
        resume: Some(ResumeButton {
            label: resume,
            enabled: l.resume_enabled,
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
        resume: None,
        after_reset: false,
    }
}

fn view(l: &ToolLine, banner: &LimitBanner, several: bool) -> LineView {
    let tool = l.tool.as_str().to_owned();
    let mut buttons = Vec::new();
    let (tone, icon) = match &l.situation {
        moves::Situation::Limit { .. } => (BannerTone::Neutral, IconName::Gauge),
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

    #[test]
    fn one_tool_just_hit_reads_as_before() {
        let at = NOW + 3 * HOUR;
        let l = one_line(&one(stopped(
            8,
            limit(Tool::Claude, LimitKind::Session, Some(3 * HOUR), false),
        )));
        let b = &l.banner;
        assert_eq!(b.title, "Session limit reached");
        assert_eq!(
            b.text,
            format!(
                "Resets at {}. 8 agents stopped; no new tickets will start.",
                clock(at)
            )
        );
        assert_eq!(
            b.resume,
            Some(ResumeButton {
                label: "Resume".into(),
                enabled: false,
                primary: false
            })
        );
        // One tool: no avatar, no tool name (d0086).
        assert_eq!(l.view.avatar, None);
        assert!(!l.view.title.contains("Claude"));
        assert_eq!(l.view.buttons.len(), 1);
        assert_eq!(
            l.view.buttons[0].action,
            Action::ResumeTool {
                tool: "claude".into()
            }
        );
    }

    #[test]
    fn one_tool_the_last_hour_counts_down() {
        let at = NOW + 42 * MINUTE;
        let l = one_line(&one(stopped(
            1,
            limit(Tool::Claude, LimitKind::Session, Some(42 * MINUTE), false),
        )));
        assert_eq!(
            l.banner.text,
            format!(
                "Resets in 42 min ({}). 1 agent stopped; tickets in Ready for Dev wait.",
                clock(at)
            )
        );
    }

    #[test]
    fn one_tool_weekly_shows_the_day() {
        let l = one_line(&one(stopped(
            8,
            limit(Tool::Claude, LimitKind::Weekly, Some(3 * 24 * HOUR), false),
        )));
        assert_eq!(l.banner.title, "Weekly limit reached");
        assert!(l.banner.text.starts_with("Resets "));
        assert!(!l.banner.text.contains("Resets at"));
        assert!(
            l.banner
                .text
                .ends_with(". 8 agents stopped; no new tickets will start.")
        );
    }

    #[test]
    fn one_tool_unknown_reset_keeps_resume_all_available() {
        let l = one_line(&one(stopped(
            2,
            limit(Tool::Claude, LimitKind::Other, None, false),
        )));
        assert_eq!(l.banner.title, "Usage limit reached");
        assert_eq!(
            l.banner.text,
            "Reset time unknown. 2 agents waiting for limit reset; no new tickets will start."
        );
        let resume = l.banner.resume.unwrap();
        assert!(resume.enabled && !resume.primary);
    }

    #[test]
    fn one_tool_after_the_reset_resume_all_is_the_call_to_action() {
        let l = one_line(&one(stopped(
            8,
            limit(Tool::Claude, LimitKind::Session, Some(-MINUTE), true),
        )));
        assert!(l.banner.after_reset);
        assert_eq!(l.banner.title, "Usage limit reset");
        assert_eq!(
            l.banner.text,
            format!(
                "Your plan limit reset at {}. 8 agents are waiting; nothing resumes until you click.",
                clock(NOW - MINUTE)
            )
        );
        assert_eq!(
            l.banner.resume,
            Some(ResumeButton {
                label: "Resume (8)".into(),
                enabled: true,
                primary: true
            })
        );
        // The reset time passed but the service hasn't marked it yet: the same.
        let unmarked = one_line(&one(stopped(
            8,
            limit(Tool::Claude, LimitKind::Session, Some(-MINUTE), false),
        )));
        assert!(unmarked.banner.after_reset);
        // Nothing left to resume: no line.
        assert!(
            lines(
                &one(limit(Tool::Claude, LimitKind::Session, Some(-MINUTE), true)),
                NOW
            )
            .is_empty()
        );
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
        assert_eq!(
            cc.view.text,
            format!("Resets {}. 4 tickets waiting.", clock(NOW + 4 * HOUR))
        );
        assert_eq!(cx.view.title, "Codex limit reached");
        assert_eq!(
            cx.view.text,
            format!(
                "Resets {}. 2 tickets waiting; 1 agent stopped mid-ticket.",
                clock(NOW + 5 * HOUR)
            )
        );
        let resume = &cx.view.buttons[0];
        assert_eq!(resume.label, "Resume Codex");
        assert!(!resume.enabled, "disabled until the reset");
        assert_eq!(
            resume.action,
            Action::ResumeTool {
                tool: "codex".into()
            }
        );
        assert_eq!(cc.view.avatar.as_ref().unwrap().letters, "CC");
        assert_eq!(cc.view.tone, BannerTone::Neutral, "a limit is neutral");
        assert_eq!(cc.view.icon, IconName::Gauge);
        // Only Resume: nothing moves to another tool.
        assert_eq!(cx.view.buttons.len(), 1);
        assert_eq!(cc.view.buttons.len(), 1);
    }

    #[test]
    fn each_reset_variant_per_tool() {
        let text = |l: ToolLine| one_line(&several(vec![l])).view;
        let last = text(limit(
            Tool::Claude,
            LimitKind::Session,
            Some(42 * MINUTE),
            false,
        ));
        assert_eq!(
            last.text,
            format!(
                "Resets in 42 min ({}). 4 tickets waiting.",
                clock(NOW + 42 * MINUTE)
            )
        );
        let week = text(limit(
            Tool::Claude,
            LimitKind::Weekly,
            Some(3 * 24 * HOUR),
            false,
        ));
        assert_eq!(week.title, "Claude Code weekly limit reached");
        let t = local(NOW + 3 * 24 * HOUR).unwrap();
        assert!(
            week.text.starts_with(&format!(
                "Resets {} {}",
                WEEKDAYS[t.weekday as usize],
                clock_12h(t.hour, t.minute)
            )),
            "{}",
            week.text
        );
        let mut month = limit(Tool::Copilot, LimitKind::Other, Some(20 * 24 * HOUR), false);
        if let moves::Situation::Limit { window_minutes, .. } = &mut month.situation {
            *window_minutes = Some(30 * 24 * 60);
        }
        let month = text(month);
        assert_eq!(month.title, "Copilot monthly limit reached");
        assert_eq!(
            month.text,
            format!(
                "Resets {}. 4 tickets waiting.",
                reset_label(NOW + 20 * 24 * HOUR, NOW)
            )
        );
        let unknown = text(limit(Tool::Codex, LimitKind::Other, None, false));
        assert_eq!(unknown.title, "Codex limit reached");
        assert_eq!(
            unknown.text,
            "Codex didn\u{2019}t say when it resets. 4 tickets waiting."
        );
        assert!(unknown.buttons[0].enabled, "always available when unknown");
        let after = one_line(&several(vec![stopped(
            8,
            limit(Tool::Claude, LimitKind::Session, Some(-HOUR), true),
        )]));
        assert_eq!(after.view.title, "Claude Code limit reset");
        assert_eq!(
            after.view.text,
            format!(
                "Claude Code\u{2019}s limit reset at {}. 8 agents are waiting; 4 tickets waiting; nothing resumes until you click.",
                clock(NOW - HOUR)
            )
        );
        assert_eq!(after.view.buttons[0].label, "Resume Claude Code (8)");
        assert!(after.view.buttons[0].enabled && after.view.buttons[0].primary);
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
            let expected = if line.icon == IconName::TriangleAlert {
                BannerTone::Warning
            } else {
                BannerTone::Neutral
            };
            assert_eq!(line.tone, expected, "{line:?}");
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
