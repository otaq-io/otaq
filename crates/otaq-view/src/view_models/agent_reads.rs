//! "What this agent reads" on Settings → Agents (AGR-01…AGR-03; d0108 rule 12 and its changelog of 2026-10-21,
//! `s15-58` / `s15-59` "What this agent reads" and "Another project: no notes, two stages"): one agent's full text for
//! one project, read-only. The **Project** picker (the project on screen, else the first; "Notes differ per project;
//! the agent is shared."), then the parts: **Its instructions** (Edit in Agents →), **Notes for <project>** (Edit in
//! <project>'s settings →; "No notes for otaq"), one **Skill** part per stage it runs and one for its role, each with
//! its SKILL.md in full (Open skill folder →), the **Also has:** line of its other skills (the commit skill and the
//! Project agent's `flow` marked, each opening its folder), and the last line on what isn't shown.
//!
//! The text is the service's (`agents.reads`, [`otaq_core::agent_reads`]), built from what `otaq prime` and the
//! launches use; this only words it.

use otaq_core::agent_reads::{AgentReads, RunsAs, SkillMark};
use otaq_core::agents::Role;
use serde::Serialize;
use ts_rs::TS;

use super::menu::MenuItem;

pub const TITLE: &str = "What this agent reads";
pub const PROJECT_HINT: &str = "Notes differ per project; the agent is shared.";
pub const NOT_SHOWN: &str =
    "Not shown: the ticket and what earlier stages wrote. They change per ticket.";

/// A project the picker offers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub key: String,
    pub name: String,
}

/// Where a part's link goes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "AgentReadsTarget")]
pub enum Target {
    /// The agent's own page.
    Agent,
    /// The project's settings, at its notes.
    ProjectNotes { project: String },
    /// Reveals the skill's folder (`settings.agent_reads_reveal`).
    Folder { skill: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentReadsLink")]
pub struct Link {
    pub label: String,
    pub target: Target,
}

/// One part: its heading (drawn in capitals), its link, and its text or the line for none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentReadsPart")]
pub struct Part {
    pub heading: String,
    pub link: Option<Link>,
    pub text: Option<String>,
    /// "No notes for otaq" when there is no text.
    pub empty: Option<String>,
    /// A skill's SKILL.md, in the mono face.
    pub mono: bool,
}

/// One name of the Also has: line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentReadsOther")]
pub struct Other {
    /// "commit-compact".
    pub name: String,
    /// "(commits with it)", "(for flow changes)".
    pub note: Option<String>,
    /// Opens its folder; `None` when there is none to show.
    pub link: Option<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "AgentReadsAlso")]
pub struct Also {
    pub label: String,
    pub skills: Vec<Other>,
}

/// The whole view.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(rename = "AgentReadsView")]
pub struct View {
    pub agent_id: String,
    pub title: String,
    /// "Rust developer · read-only".
    pub subtitle: String,
    pub back_label: String,
    pub project_label: String,
    /// The picker's rows (id: the project's key), the picked one checked.
    pub projects: Vec<MenuItem>,
    /// The picked project's key and name; `None` when there is no project.
    pub project: Option<String>,
    pub project_name: Option<String>,
    pub project_hint: String,
    pub parts: Vec<Part>,
    /// "Rust developer runs in no stage of billing-api."
    pub no_stage: Option<String>,
    pub also: Option<Also>,
    /// With no project: what to do first.
    pub empty: Option<String>,
    pub not_shown: String,
}

/// The project the view opens on: `wanted` (the picked one, else the one on screen) when it is one of `projects`,
/// else the first.
pub fn opening_project(projects: &[Project], wanted: Option<&str>) -> Option<String> {
    wanted
        .and_then(|w| projects.iter().find(|p| p.key == w))
        .or_else(|| projects.first())
        .map(|p| p.key.clone())
}

/// "Skill: work-ticket · Dev", "Skill: noyan · as Noyan".
pub fn skill_heading(skill: &str, runs: &RunsAs) -> String {
    let place = match runs {
        RunsAs::Stage(stage) => stage.clone(),
        RunsAs::Role(Role::ProjectAgent) => "as Noyan".to_owned(),
    };
    format!("Skill: {skill} \u{b7} {place}")
}

/// The view of `reads` (`None`: no project to pick) for the agent `agent_id` called `agent`.
pub fn view(
    agent_id: &str,
    agent: &str,
    projects: Vec<Project>,
    reads: Option<&AgentReads>,
) -> View {
    let mut parts = Vec::new();
    let mut no_stage = None;
    let mut also = None;
    if let Some(r) = reads {
        parts.push(Part {
            heading: "Its instructions".to_owned(),
            link: Some(Link {
                label: "Edit in Agents \u{2192}".to_owned(),
                target: Target::Agent,
            }),
            text: Some(r.instructions.clone()).filter(|t| !t.trim().is_empty()),
            empty: r
                .instructions
                .trim()
                .is_empty()
                .then(|| format!("No instructions for {}", r.agent)),
            mono: false,
        });
        parts.push(Part {
            heading: format!("Notes for {}", r.project_name),
            link: Some(Link {
                label: format!("Edit in {}\u{2019}s settings \u{2192}", r.project_name),
                target: Target::ProjectNotes {
                    project: r.project.clone(),
                },
            }),
            text: Some(r.notes.clone()).filter(|t| !t.trim().is_empty()),
            empty: r
                .notes
                .trim()
                .is_empty()
                .then(|| format!("No notes for {}", r.project_name)),
            mono: false,
        });
        for skill in &r.skills {
            parts.push(Part {
                heading: skill_heading(&skill.skill, &skill.runs),
                link: skill.folder.as_ref().map(|_| Link {
                    label: "Open skill folder \u{2192}".to_owned(),
                    target: Target::Folder {
                        skill: skill.skill.clone(),
                    },
                }),
                text: skill.text.clone(),
                empty: skill
                    .text
                    .is_none()
                    .then(|| format!("{}\u{2019}s SKILL.md couldn\u{2019}t be read", skill.skill)),
                mono: true,
            });
        }
        if r.skills.is_empty() {
            no_stage = Some(format!(
                "{} runs in no stage of {}.",
                r.agent, r.project_name
            ));
        } else if !r.others.is_empty() {
            also = Some(Also {
                label: "Also has:".to_owned(),
                skills: r
                    .others
                    .iter()
                    .map(|o| Other {
                        name: o.skill.clone(),
                        note: o.mark.map(|m| {
                            match m {
                                SkillMark::Commits => "(commits with it)",
                                SkillMark::FlowChanges => "(for flow changes)",
                            }
                            .to_owned()
                        }),
                        link: o.folder.as_ref().map(|_| Target::Folder {
                            skill: o.skill.clone(),
                        }),
                    })
                    .collect(),
            });
        }
    }
    View {
        agent_id: agent_id.to_owned(),
        title: TITLE.to_owned(),
        subtitle: format!("{agent} \u{b7} read-only"),
        back_label: format!("Back to {agent}"),
        project_label: "Project".to_owned(),
        project: reads.map(|r| r.project.clone()),
        project_name: reads.map(|r| r.project_name.clone()),
        empty: (reads.is_none() && projects.is_empty())
            .then(|| "Add a project to see what this agent reads.".to_owned()),
        projects: projects
            .into_iter()
            .map(|p| {
                let picked = reads.is_some_and(|r| r.project == p.key);
                MenuItem::new(p.key, p.name).checked(picked)
            })
            .collect(),
        project_hint: PROJECT_HINT.to_owned(),
        parts,
        no_stage,
        also,
        not_shown: NOT_SHOWN.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use otaq_core::agent_reads::{OtherSkill, SkillPart};
    // The drawn frames' third project is called after the app.
    use otaq_core::naming::COMMAND_NAME as APP;

    use super::*;

    const RUST: &str = "Work one ticket at a time on its own branch. Write the test first, then the change. Keep \
                        commits small; explain the why in the message, not the what.";
    const WORK_TICKET: &str = "# work-ticket\n1. Read the ticket and its acceptance notes.\n2. Branch from the \
                               ticket\u{2019}s base.\n3. Implement, then run cargo test and cargo clippy.\n4. Commit \
                               with /commit-compact; leave the PR to the delivery stage.";

    fn projects() -> Vec<Project> {
        [("SHOP", "shop-web"), ("BILL", "billing-api"), ("OQ", APP)]
            .map(|(key, name)| Project {
                key: key.into(),
                name: name.into(),
            })
            .to_vec()
    }

    fn part(skill: &str, runs: RunsAs, text: &str) -> SkillPart {
        SkillPart {
            skill: skill.into(),
            runs,
            text: Some(text.into()),
            folder: Some(PathBuf::from(format!("/plugin/skills/{skill}"))),
        }
    }

    fn other(skill: &str, mark: Option<SkillMark>) -> OtherSkill {
        OtherSkill {
            skill: skill.into(),
            folder: Some(PathBuf::from(format!("/data/skills/{skill}"))),
            mark,
        }
    }

    fn reads(
        project: (&str, &str),
        notes: &str,
        skills: Vec<SkillPart>,
        others: Vec<OtherSkill>,
    ) -> AgentReads {
        AgentReads {
            agent_id: "rust".into(),
            agent: "Rust developer".into(),
            project: project.0.into(),
            project_name: project.1.into(),
            instructions: RUST.into(),
            notes: notes.into(),
            skills,
            others,
        }
    }

    fn headings(v: &View) -> Vec<&str> {
        v.parts.iter().map(|p| p.heading.as_str()).collect()
    }

    /// `s15-58` "What this agent reads": Rust developer on shop-web.
    #[test]
    fn the_drawn_shop_web_frame() {
        let r = reads(
            ("SHOP", "shop-web"),
            "Use pnpm, not npm. Run cargo fmt before you commit.",
            vec![part(
                "work-ticket",
                RunsAs::Stage("Dev".into()),
                WORK_TICKET,
            )],
            Vec::new(),
        );
        let v = view("rust", "Rust developer", projects(), Some(&r));
        assert_eq!(v.title, "What this agent reads");
        assert_eq!(v.subtitle, "Rust developer \u{b7} read-only");
        assert_eq!(v.project_label, "Project");
        assert_eq!(v.project.as_deref(), Some("SHOP"));
        assert_eq!(v.project_name.as_deref(), Some("shop-web"));
        let picker: Vec<(&str, &str, bool)> = v
            .projects
            .iter()
            .map(|p| (p.id.as_str(), p.label.as_str(), p.checked))
            .collect();
        assert_eq!(
            picker,
            [
                ("SHOP", "shop-web", true),
                ("BILL", "billing-api", false),
                ("OQ", APP, false)
            ]
        );
        assert_eq!(
            v.project_hint,
            "Notes differ per project; the agent is shared."
        );
        assert_eq!(
            headings(&v),
            [
                "Its instructions",
                "Notes for shop-web",
                "Skill: work-ticket \u{b7} Dev"
            ]
        );
        let links: Vec<&str> = v
            .parts
            .iter()
            .map(|p| p.link.as_ref().unwrap().label.as_str())
            .collect();
        assert_eq!(
            links,
            [
                "Edit in Agents \u{2192}",
                "Edit in shop-web\u{2019}s settings \u{2192}",
                "Open skill folder \u{2192}"
            ]
        );
        assert_eq!(v.parts[0].text.as_deref(), Some(RUST));
        assert_eq!(
            v.parts[1].text.as_deref(),
            Some("Use pnpm, not npm. Run cargo fmt before you commit.")
        );
        assert_eq!(
            v.parts[1].link.as_ref().unwrap().target,
            Target::ProjectNotes {
                project: "SHOP".into()
            }
        );
        assert_eq!(v.parts[2].text.as_deref(), Some(WORK_TICKET));
        assert!(v.parts[2].mono && !v.parts[0].mono);
        assert_eq!(v.no_stage, None);
        assert_eq!(v.also, None);
        assert_eq!(
            v.not_shown,
            "Not shown: the ticket and what earlier stages wrote. They change per ticket."
        );
    }

    /// `s15-58` "Another project: no notes, two stages": otaq, each skill part naming its stage.
    #[test]
    fn the_drawn_otaq_frame_no_notes_and_two_stages() {
        let r = reads(
            ("OQ", APP),
            "",
            vec![
                part("work-ticket", RunsAs::Stage("Dev".into()), WORK_TICKET),
                part(
                    "write-docs",
                    RunsAs::Stage("Docs".into()),
                    "# write-docs\nUpdate the docs the change touches; keep examples runnable.",
                ),
            ],
            Vec::new(),
        );
        let v = view("rust", "Rust developer", projects(), Some(&r));
        assert_eq!(
            headings(&v),
            [
                "Its instructions",
                &format!("Notes for {APP}"),
                "Skill: work-ticket \u{b7} Dev",
                "Skill: write-docs \u{b7} Docs"
            ]
        );
        assert_eq!(v.parts[1].text, None);
        assert_eq!(v.parts[1].empty, Some(format!("No notes for {APP}")));
        assert_eq!(
            v.parts[1].link.as_ref().unwrap().label,
            format!("Edit in {APP}\u{2019}s settings \u{2192}")
        );
    }

    /// AGR-02: "Also has: commit-compact (commits with it) · write-docs", each opening its folder.
    #[test]
    fn also_has_with_the_commit_mark() {
        let r = reads(
            ("SHOP", "shop-web"),
            "",
            vec![part(
                "work-ticket",
                RunsAs::Stage("Dev".into()),
                WORK_TICKET,
            )],
            vec![
                other("commit-compact", Some(SkillMark::Commits)),
                other("write-docs", None),
            ],
        );
        let also = view("rust", "Rust developer", projects(), Some(&r))
            .also
            .unwrap();
        assert_eq!(also.label, "Also has:");
        let names: Vec<(&str, Option<&str>)> = also
            .skills
            .iter()
            .map(|s| (s.name.as_str(), s.note.as_deref()))
            .collect();
        assert_eq!(
            names,
            [
                ("commit-compact", Some("(commits with it)")),
                ("write-docs", None)
            ]
        );
        assert_eq!(
            also.skills[1].link,
            Some(Target::Folder {
                skill: "write-docs".into()
            })
        );
    }

    /// AGR-03: the Project agent's part is named by its role, `flow` marked for flow changes.
    #[test]
    fn a_role_agent() {
        let mut r = reads(
            ("SHOP", "shop-web"),
            "",
            vec![part("noyan", RunsAs::Role(Role::ProjectAgent), "# noyan")],
            vec![other("flow", Some(SkillMark::FlowChanges))],
        );
        r.agent = "Noyan".into();
        r.instructions = String::new();
        let v = view("noyan", "Noyan", projects(), Some(&r));
        assert_eq!(
            headings(&v),
            [
                "Its instructions",
                "Notes for shop-web",
                "Skill: noyan \u{b7} as Noyan"
            ]
        );
        assert_eq!(
            v.parts[0].empty.as_deref(),
            Some("No instructions for Noyan")
        );
        let also = v.also.unwrap();
        assert_eq!(also.skills[0].name, "flow");
        assert_eq!(also.skills[0].note.as_deref(), Some("(for flow changes)"));
        // An agent with a role and a stage: a part for each.
        r.skills.insert(
            0,
            part(
                "product-spec",
                RunsAs::Stage("Product spec".into()),
                "# product-spec",
            ),
        );
        let v = view("noyan", "Noyan", projects(), Some(&r));
        assert_eq!(
            headings(&v)[2..],
            [
                "Skill: product-spec \u{b7} Product spec",
                "Skill: noyan \u{b7} as Noyan"
            ]
        );
    }

    /// AGR-02: an agent in no stage of the project and with no role: the line, and no skill part.
    #[test]
    fn an_agent_in_no_stage() {
        let r = reads(
            ("BILL", "billing-api"),
            "",
            Vec::new(),
            vec![other("work-ticket", None)],
        );
        let v = view("rust", "Rust developer", projects(), Some(&r));
        assert_eq!(headings(&v), ["Its instructions", "Notes for billing-api"]);
        assert_eq!(
            v.no_stage.as_deref(),
            Some("Rust developer runs in no stage of billing-api.")
        );
        assert_eq!(v.also, None);
    }

    /// AGR-02, AGR-04: a ticket's choice of Quick developer for Dev doesn't move Dev's part: the reads come from the
    /// flow (the core test checks the service's side), so Quick developer, Dev's option, runs in no stage while
    /// Rust developer, Dev's own agent, keeps its part.
    #[test]
    fn a_stage_choice_naming_another_agent() {
        let rust = reads(
            ("SHOP", "shop-web"),
            "",
            vec![part(
                "work-ticket",
                RunsAs::Stage("Dev".into()),
                WORK_TICKET,
            )],
            Vec::new(),
        );
        let mut quick = reads(("SHOP", "shop-web"), "", Vec::new(), Vec::new());
        quick.agent = "Quick developer".into();
        assert!(
            view("rust", "Rust developer", projects(), Some(&rust))
                .no_stage
                .is_none()
        );
        assert_eq!(
            view("quick", "Quick developer", projects(), Some(&quick))
                .no_stage
                .as_deref(),
            Some("Quick developer runs in no stage of shop-web.")
        );
    }

    /// AGR-01: the picker opens on the project on screen, else the first; with none, a line instead of parts.
    #[test]
    fn the_project_picker() {
        assert_eq!(
            opening_project(&projects(), Some("OQ")).as_deref(),
            Some("OQ")
        );
        assert_eq!(
            opening_project(&projects(), Some("GONE")).as_deref(),
            Some("SHOP")
        );
        assert_eq!(opening_project(&projects(), None).as_deref(), Some("SHOP"));
        assert_eq!(opening_project(&[], None), None);
        let v = view("rust", "Rust developer", Vec::new(), None);
        assert!(v.parts.is_empty());
        assert_eq!(
            v.empty.as_deref(),
            Some("Add a project to see what this agent reads.")
        );
        assert_eq!(v.back_label, "Back to Rust developer");
    }

    /// A skill whose SKILL.md can't be read (no plugin folder) says so, with no folder to open.
    #[test]
    fn an_unreadable_skill() {
        let mut r = reads(
            ("SHOP", "shop-web"),
            "",
            vec![part("work-ticket", RunsAs::Stage("Dev".into()), "")],
            Vec::new(),
        );
        r.skills[0].text = None;
        r.skills[0].folder = None;
        let v = view("rust", "Rust developer", projects(), Some(&r));
        assert_eq!(v.parts[2].link, None);
        assert_eq!(
            v.parts[2].empty.as_deref(),
            Some("work-ticket\u{2019}s SKILL.md couldn\u{2019}t be read")
        );
    }
}
