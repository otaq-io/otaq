//! `otaq flow show`, `presets`, `propose` and `write-skill` (FLW-02, FLW-03, FED-06; d0108, d0115): a Project agent
//! reads the project's flow, the ready-made ones, and proposes a whole document, or writes a stage's skill in the
//! folder Otaq gives it. Proposing lands in Otaq's flow editor and returns at once; the
//! user's answer (Apply, Cancel, Ask for more changes) comes back as the Project agent's next message. From the
//! user's own terminal nothing comes back. A ticket's agent, a check and an epic lead are refused.
//!
//! # `--json` shapes
//!
//! - `show`: `{project, version, yaml, document}`, `document` the YAML as JSON.
//! - `presets`: `[{file, name, line, stages, yaml}]`, `stages` the preset's stage names in order, `yaml` its file
//!   (every field: an agent check's `agents`, a check's `fail_to`).
//! - `propose`: the proposal as the flow editor shows it (`id`, `problems`, `diff`, …).
//! - `write-skill`: `{stage, folder, skill?, text}`; `--quiet` prints the folder (the skill's name with `--done`).

use std::io::Read;

use otaq_core::data_dir::DataDir;
use otaq_core::flow::presets::presets;
use otaq_core::flow::propose::{ProposeParams, Proposed, waiting_text};
use otaq_core::flow::write_skill::WriteSkillParams;
use otaq_core::naming;
use otaq_core::protocol::{ErrorBody, ErrorKind, ProjectParams, Reply, Request};
use serde_json::json;

use crate::cli::{FlowCommand, GlobalArgs};
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

pub fn run(command: FlowCommand, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    match command {
        FlowCommand::Presets => {
            list_presets(out);
            Ok(())
        }
        FlowCommand::Show => show(global, out),
        FlowCommand::Propose { file, stdin, from } => {
            let source = match (file, stdin, from) {
                (Some(path), _, _) => Proposed::Document {
                    yaml: std::fs::read_to_string(&path).map_err(|e| {
                        ErrorBody::new(
                            ErrorKind::Usage,
                            format!("can't read {}: {e}", path.display()),
                        )
                    })?,
                    file: path.file_name().map(|n| n.to_string_lossy().into_owned()),
                },
                (None, true, _) => {
                    let mut text = String::new();
                    std::io::stdin()
                        .read_to_string(&mut text)
                        .map_err(|e| internal(format!("can't read stdin: {e}")))?;
                    Proposed::Document {
                        yaml: text,
                        file: None,
                    }
                }
                (None, false, Some(project)) => Proposed::Project { project },
                (None, false, None) => {
                    return Err(ErrorBody::new(
                        ErrorKind::Usage,
                        format!(
                            "give the flow: `{} flow propose -f <file>`, `--stdin` or `--from <project>`",
                            naming::COMMAND_NAME
                        ),
                    ));
                }
            };
            propose(source, global, out)
        }
        FlowCommand::WriteSkill { stage, done } => write_skill(stage, done, global, out),
    }
}

fn write_skill(
    stage: String,
    done: bool,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let project = target::require_project(&client, &data, global).await?;
        let written = match client
            .request(Request::FlowWriteSkill(WriteSkillParams {
                project: project.to_string(),
                stage,
                done,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::SkillWrite(w) => *w,
            other => return Err(unexpected(&other)),
        };
        let quiet = written
            .skill
            .clone()
            .unwrap_or_else(|| written.folder.clone());
        out.print(&written.text, &written, Some(&quiet));
        Ok(())
    })
}

/// One line per shipped preset: its file, name and line ("team-pr.yaml · Team PR · Teams that review on GitHub or
/// GitLab: the PR is the review").
fn list_presets(out: &Output) {
    let human: Vec<String> = presets()
        .iter()
        .map(|p| format!("{} \u{b7} {} \u{b7} {}", p.file, p.name, p.line))
        .collect();
    let json: Vec<_> = presets()
        .iter()
        .map(|p| {
            let stages: Vec<String> = p.read().flow.stages.into_iter().map(|s| s.name).collect();
            json!({ "file": p.file, "name": p.name, "line": p.line, "stages": stages, "yaml": p.yaml })
        })
        .collect();
    let files: Vec<&str> = presets().iter().map(|p| p.file).collect();
    out.print(&human.join("\n"), &json, Some(&files.join("\n")));
}

fn show(global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let project = target::require_project(&client, &data, global).await?;
        let document = match client
            .request(Request::FlowShow(ProjectParams {
                project: project.to_string(),
            }))
            .await
            .map_err(client_error)?
        {
            Reply::FlowDocument(d) => *d,
            other => return Err(unexpected(&other)),
        };
        out.print(
            document.yaml.trim_end(),
            &document,
            Some(&document.version.to_string()),
        );
        Ok(())
    })
}

fn propose(source: Proposed, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let project = target::require_project(&client, &data, global).await?;
        let proposal = match client
            .request(Request::FlowPropose(ProposeParams {
                project: project.to_string(),
                source,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::FlowProposal(Some(p)) => *p,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &waiting_text(&proposal),
            &proposal,
            Some(&proposal.id.to_string()),
        );
        Ok(())
    })
}
