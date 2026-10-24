//! The optional agent limits in the app (SCH-03, SHL-03; d0016, d0087; *15 · Settings → Global · Agent limits OFF
//! (default) / ON*, *02 · App shell → Row · Header states (0086 · V) → Agent limits on*).
//!
//! Off (the default), Activity shows the plain running count ("● 3 running", agents across all projects). On, it
//! reads "● 3 / 5 agents", and its popover starts with the same two values as Settings. Every change saves at once
//! and reaches the scheduler without restarting anything; running agents are never stopped by a limit.

use otaq_core::board::settings::AgentLimits;
use otaq_core::board::snapshot::ProjectInfo;
use otaq_core::scheduler::SchedulerStatus;
use serde::Serialize;
use ts_rs::TS;

/// The values the steppers accept.
pub const MAX_AGENTS: std::ops::RangeInclusive<u32> = 1..=50;
pub const BACK_PRESSURE_ITEMS: std::ops::RangeInclusive<u32> = 1..=50;

/// Activity's agents part (SHL-03): "● 6 running", or "● 3 / 5 agents" with limits on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct AgentsChip {
    /// "3" (off) or "3 / 5" (on).
    pub count: String,
    /// "running" (off) or "agents" (on).
    pub label: &'static str,
    /// On: the limits' stepper heads Activity's popover.
    pub control: bool,
    /// The dot is lit while something runs.
    pub active: bool,
    /// "5 paused" while the open project is paused (PAU-08): the dot is amber.
    pub paused: bool,
}

/// The stepper at the top of Activity's popover (limits on): the same two values as Settings, saved on every step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LimitsPopover")]
pub struct Popover {
    pub title: &'static str,
    pub max_agents: Stepper,
    pub back_pressure: Stepper,
    pub note: &'static str,
}

/// One stepper row: its label, value, and whether − and + can be pressed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "LimitStepper")]
pub struct Stepper {
    pub label: &'static str,
    pub value: u32,
    pub can_lower: bool,
    pub can_raise: bool,
}

impl Stepper {
    fn new(label: &'static str, value: u32, range: &std::ops::RangeInclusive<u32>) -> Self {
        Self {
            label,
            value,
            can_lower: value > *range.start(),
            can_raise: value < *range.end(),
        }
    }
}

/// The back-pressure setting's label, before its number, in Settings and in the popover (SCH-03; never "Pause",
/// which is 0112's, Q450).
pub const BACK_PRESSURE: &str = "Don\u{2019}t start new tickets while Needs you has";

/// The popover for `limits`.
pub fn popover(limits: &AgentLimits) -> Popover {
    Popover {
        title: "Agent limits",
        max_agents: Stepper::new(
            "Max agents running at once",
            limits.max_concurrency,
            &MAX_AGENTS,
        ),
        back_pressure: Stepper::new(BACK_PRESSURE, limits.back_pressure, &BACK_PRESSURE_ITEMS),
        note: "Tickets in Ready for Dev wait for a free slot. Nothing that is already running is stopped.",
    }
}

/// Which of the popover's values a stepper press changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "LimitField")]
pub enum Field {
    MaxAgents,
    BackPressure,
}

/// The limits after one press of a popover stepper.
pub fn stepped(limits: &AgentLimits, field: Field, up: bool) -> AgentLimits {
    match field {
        Field::MaxAgents => AgentLimits {
            max_concurrency: step(limits.max_concurrency, up, &MAX_AGENTS),
            ..*limits
        },
        Field::BackPressure => AgentLimits {
            back_pressure: step(limits.back_pressure, up, &BACK_PRESSURE_ITEMS),
            ..*limits
        },
    }
}

/// Activity's agents part from the limits, the scheduler's count (slots held) and the plain running count (agents in a turn
/// across all projects, the rail's numbers).
pub fn agents_chip(
    limits: &AgentLimits,
    status: Option<&SchedulerStatus>,
    running: u32,
) -> AgentsChip {
    if limits.enabled {
        let holding = status.map_or(running, |s| s.holding);
        AgentsChip {
            count: format!("{holding}/{}", limits.max_concurrency.max(1)),
            label: "agents",
            control: true,
            active: holding > 0,
            paused: false,
        }
    } else {
        AgentsChip {
            count: running.to_string(),
            label: "running",
            control: false,
            active: running > 0,
            paused: false,
        }
    }
}

/// Agents running across all projects (Activity's plain count).
pub fn running_total(projects: &[ProjectInfo]) -> u32 {
    projects.iter().map(|p| p.agents_working).sum()
}

/// A stepper press: one up or down, kept inside `range`.
pub fn step(value: u32, up: bool, range: &std::ops::RangeInclusive<u32>) -> u32 {
    let next = if up {
        value.saturating_add(1)
    } else {
        value.saturating_sub(1)
    };
    next.clamp(*range.start(), *range.end())
}

/// Turning the limits on or off. The values stay as they are (5 and 3 unless changed); a stored value the
/// steppers can't show (0) comes back as the default.
pub fn toggled(limits: &AgentLimits, on: bool) -> AgentLimits {
    let defaults = AgentLimits::default();
    let fix = |v: u32, range: &std::ops::RangeInclusive<u32>, default: u32| {
        if range.contains(&v) { v } else { default }
    };
    AgentLimits {
        enabled: on,
        max_concurrency: fix(
            limits.max_concurrency,
            &MAX_AGENTS,
            defaults.max_concurrency,
        ),
        back_pressure: fix(
            limits.back_pressure,
            &BACK_PRESSURE_ITEMS,
            defaults.back_pressure,
        ),
    }
}

/// One row of the Settings section: label and description, which change with the toggle (both frames).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LimitRow {
    pub label: &'static str,
    pub description: &'static str,
    /// Visible but disabled while the limits are off.
    pub enabled: bool,
}

/// The Settings section's three rows.
pub fn settings_rows(limits: &AgentLimits) -> [LimitRow; 3] {
    let on = limits.enabled;
    [
        LimitRow {
            label: "Limit agents",
            description: if on {
                "Activity shows “3/5 agents”; adjust it in its popover."
            } else {
                "Off by default. When on, Activity shows “3/5 agents”."
            },
            enabled: true,
        },
        LimitRow {
            label: "Max agents running at once",
            description: if on {
                "Tickets in Ready for Dev wait for a free slot."
            } else {
                "Suggested 5. Tickets in Ready for Dev wait for a free slot."
            },
            enabled: on,
        },
        LimitRow {
            label: BACK_PRESSURE,
            description: if on {
                "Running agents are never stopped."
            } else {
                "Suggested 3 items. Running agents are never stopped."
            },
            enabled: on,
        },
    ]
}

/// The "Right now" line under the ON section: "Right now: 3 running · 2 queued tickets waiting for a slot ·
/// needs you 7 (no new starts)".
pub fn right_now(status: &SchedulerStatus) -> String {
    let waiting = match status.waiting_for_slot {
        1 => "1 ticket waiting for a slot".to_owned(),
        n => format!("{n} tickets waiting for a slot"),
    };
    // Back-pressure never says "paused": Pause is 0112's (SCH-03, Q450).
    let paused = if status.starts_paused {
        " (no new starts)"
    } else {
        ""
    };
    format!(
        "Right now: {} running · {waiting} · needs you {}{paused}",
        status.holding, status.needs_you
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on(max: u32, back_pressure: u32) -> AgentLimits {
        AgentLimits {
            enabled: true,
            max_concurrency: max,
            back_pressure,
        }
    }

    fn project(name: &str, working: u32) -> ProjectInfo {
        ProjectInfo {
            key: name.to_uppercase(),
            name: name.into(),
            repo_path: format!("/code/{name}"),
            base_branch: "main".into(),
            agents_working: working,
            needs_you: 0,
            flow_setup_since: None,
            paused_since: None,
        }
    }

    #[test]
    fn off_shows_the_plain_running_count() {
        let chip = agents_chip(&AgentLimits::default(), None, 3);
        assert_eq!(
            chip,
            AgentsChip {
                count: "3".into(),
                label: "running",
                control: false,
                active: true,
                paused: false,
            }
        );
        assert!(!agents_chip(&AgentLimits::default(), None, 0).active);
    }

    #[test]
    fn on_shows_slots_held_out_of_the_max() {
        let status = SchedulerStatus {
            limits: on(5, 3),
            holding: 3,
            ..Default::default()
        };
        let chip = agents_chip(&on(5, 3), Some(&status), 2);
        assert_eq!(chip.count, "3/5");
        assert_eq!(chip.label, "agents");
        assert!(chip.control);
        // Before the scheduler answered, the running count stands in.
        assert_eq!(agents_chip(&on(5, 3), None, 2).count, "2/5");
    }

    #[test]
    fn the_plain_count_adds_every_project() {
        let projects = [
            project("shop-web", 2),
            project("billing-api", 1),
            project("docs-site", 0),
        ];
        assert_eq!(running_total(&projects), 3);
    }

    #[test]
    fn toggling_on_reveals_the_defaults_and_keeps_changed_values() {
        let first = toggled(&AgentLimits::default(), true);
        assert_eq!(first, on(5, 3));
        let changed = on(7, 2);
        assert_eq!(
            toggled(&changed, false),
            AgentLimits {
                enabled: false,
                ..changed
            }
        );
        assert_eq!(
            toggled(
                &AgentLimits {
                    enabled: false,
                    ..changed
                },
                true
            ),
            changed
        );
        assert_eq!(toggled(&on(0, 0), true), on(5, 3), "a flagged 0 resets");
    }

    #[test]
    fn steppers_stay_in_range() {
        assert_eq!(step(5, true, &MAX_AGENTS), 6);
        assert_eq!(step(1, false, &MAX_AGENTS), 1);
        assert_eq!(step(50, true, &MAX_AGENTS), 50);
        assert_eq!(step(3, false, &BACK_PRESSURE_ITEMS), 2);
    }

    #[test]
    fn rows_follow_the_toggle() {
        let off = settings_rows(&AgentLimits::default());
        assert!(off[0].enabled && !off[1].enabled && !off[2].enabled);
        assert_eq!(
            off[1].description,
            "Suggested 5. Tickets in Ready for Dev wait for a free slot."
        );
        assert_eq!(
            off[2].label, "Don\u{2019}t start new tickets while Needs you has",
            "Q450: not \"Pause\", which is 0112's"
        );
        let on_rows = settings_rows(&on(5, 3));
        assert!(on_rows.iter().all(|r| r.enabled));
        assert_eq!(on_rows[2].description, "Running agents are never stopped.");
    }

    #[test]
    fn right_now_line() {
        let status = SchedulerStatus {
            limits: on(5, 3),
            holding: 3,
            waiting_for_slot: 2,
            needs_you: 7,
            starts_paused: true,
        };
        assert_eq!(
            right_now(&status),
            "Right now: 3 running · 2 tickets waiting for a slot · needs you 7 (no new starts)"
        );
        let calm = SchedulerStatus {
            waiting_for_slot: 1,
            needs_you: 0,
            starts_paused: false,
            ..status
        };
        assert_eq!(
            right_now(&calm),
            "Right now: 3 running · 1 ticket waiting for a slot · needs you 0"
        );
    }

    #[test]
    fn the_popover_steps_inside_the_ranges() {
        let limits = AgentLimits {
            enabled: true,
            max_concurrency: 1,
            back_pressure: 50,
        };
        let view = popover(&limits);
        assert_eq!(view.title, "Agent limits");
        assert_eq!(view.max_agents.label, "Max agents running at once");
        assert_eq!(
            view.back_pressure.label,
            "Don\u{2019}t start new tickets while Needs you has"
        );
        assert!(!view.max_agents.can_lower && view.max_agents.can_raise);
        assert!(view.back_pressure.can_lower && !view.back_pressure.can_raise);
        let up = stepped(&limits, Field::MaxAgents, true);
        assert_eq!(
            (up.max_concurrency, up.back_pressure, up.enabled),
            (2, 50, true)
        );
        assert_eq!(stepped(&limits, Field::MaxAgents, false), limits);
        assert_eq!(stepped(&limits, Field::BackPressure, true), limits);
        assert_eq!(
            stepped(&limits, Field::BackPressure, false).back_pressure,
            49
        );
    }
}
