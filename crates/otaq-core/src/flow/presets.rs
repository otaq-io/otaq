//! The presets (FLW-08, d0108; design `s14-18`, `s15-62`): the ready-made flows Otaq ships as files, each with its
//! agents, embedded at build time: the default flow, Solo autopilot, Spec first and Team PR. `otaq flow presets`
//! and Use a preset… list them with [`presets`]; Otaq has no code path only presets use, so each is read and checked
//! as any proposed file is.

use std::sync::LazyLock;

use super::model::{Column, Delivers, DeliveryMode, FlowFile, Stage};
use super::yaml;
use crate::naming;

/// A shipped preset: its name, one line, file and contents, as Use a preset… shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preset {
    /// "Solo autopilot".
    pub name: String,
    /// "Solo projects that trust the checks".
    pub line: &'static str,
    /// "solo-autopilot.yaml".
    pub file: &'static str,
    /// The file's text, as `otaq flow propose` reads it.
    pub yaml: &'static str,
}

impl Preset {
    /// The file, as read (every shipped preset reads: tested).
    pub fn read(&self) -> FlowFile {
        yaml::read(self.yaml).expect("a shipped preset reads (tested)")
    }

    /// The small diagram's columns: Ready for Dev, each stage with its own column but the Delivery, and Done
    /// (`s14-18`: "Ready for Dev · Dev · Validate · Review · Done").
    pub fn columns(&self) -> Vec<String> {
        let file = self.read();
        let stages = file
            .flow
            .stages
            .iter()
            .filter(|s| s.column == Column::Own && !s.is_delivery())
            .map(|s| s.name.clone());
        std::iter::once("Ready for Dev".to_owned())
            .chain(stages)
            .chain(std::iter::once("Done".to_owned()))
            .collect()
    }

    /// How it delivers, under its name: "Done · click", "Done · Local merge, automatic", "Done · PR, automatic".
    pub fn delivers(&self) -> String {
        let file = self.read();
        let Some((stage, delivery)) = file
            .flow
            .stages
            .iter()
            .find_map(|s| s.delivery.map(|d| (s, d)))
        else {
            return String::new();
        };
        delivery_line(stage, delivery.mode, delivery.delivers)
    }
}

fn delivery_line(stage: &Stage, mode: DeliveryMode, delivers: Delivers) -> String {
    let how = match (mode, delivers) {
        (DeliveryMode::Click, _) => "click",
        (DeliveryMode::Automatic, Delivers::Local) => "Local merge, automatic",
        (DeliveryMode::Automatic, Delivers::Pr) => "PR, automatic",
        // Refused by the checks (`either` is click only).
        (DeliveryMode::Automatic, Delivers::Either) => "automatic",
    };
    format!("{} \u{b7} {how}", stage.name)
}

pub const SOLO_AUTOPILOT_FILE: &str = "solo-autopilot.yaml";
pub const SPEC_FIRST_FILE: &str = "spec-first.yaml";
pub const TEAM_PR_FILE: &str = "team-pr.yaml";

static PRESETS: LazyLock<Vec<Preset>> = LazyLock::new(|| {
    vec![
        Preset {
            name: format!("{} default", naming::PRODUCT_NAME),
            line: "Today\u{2019}s board",
            file: naming::DEFAULT_FLOW_FILE,
            yaml: naming::DEFAULT_FLOW_YAML,
        },
        Preset {
            name: "Solo autopilot".to_owned(),
            line: "Solo projects that trust the checks",
            file: SOLO_AUTOPILOT_FILE,
            yaml: include_str!("../../flows/solo-autopilot.yaml"),
        },
        Preset {
            name: "Spec first".to_owned(),
            line: "Plan on the board, then build",
            file: SPEC_FIRST_FILE,
            yaml: include_str!("../../flows/spec-first.yaml"),
        },
        Preset {
            name: "Team PR".to_owned(),
            line: "Teams that review on GitHub or GitLab: the PR is the review",
            file: TEAM_PR_FILE,
            yaml: include_str!("../../flows/team-pr.yaml"),
        },
    ]
});

/// The shipped presets, in the order Use a preset… and `otaq flow presets` list them; the default flow first.
pub fn presets() -> &'static [Preset] {
    &PRESETS
}

/// A shipped preset by its file name (`team-pr.yaml`) or its name (`Team PR`), ignoring case.
pub fn preset(name: &str) -> Option<&'static Preset> {
    let name = name.trim();
    PRESETS
        .iter()
        .find(|p| p.file.eq_ignore_ascii_case(name) || p.name.eq_ignore_ascii_case(name))
}
