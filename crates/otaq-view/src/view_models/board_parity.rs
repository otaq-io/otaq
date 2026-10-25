//! The board's parity check (STG-08, d0108: "a project that never changes its flow sees today's board"): today's
//! `BoardView`, every card's drag answer and every drop's step for the M31 example (pocket-notes with Validate,
//! pocket-api without) and for a project with a card in every status, against a snapshot written before the columns
//! came from the flow (`fixtures/board-parity.json`), rewritten by M33 for Validate's own column (FLW-01) and for
//! drags to any stage (FLW-18, T-0421: Validate is a drop target, a drag back opens its dialog, a forward drag from
//! Dev between turns skips the stages between, a parked card returns wherever it is dropped). `OTAQ_UPDATE_PARITY=1`
//! rewrites it.

#![cfg(test)]

use serde_json::{Value, json};

use otaq_core::board::tickets::Ticket;
use otaq_core::domain::Status;
use otaq_core::flow::{Flow, default_flow};

use crate::key_labels::KeyLabels;
use crate::keymap::Platform;
use crate::view_models::board_page::{self, DropTargetId, Params, ProjectFacts};
use crate::view_models::card_fixtures::{NOW, ticket};
use crate::view_models::search_page::FilterState;

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/view_models/fixtures/board-parity.json"
);

fn facts<'a>(name: &'static str, flow: &'a Flow) -> ProjectFacts<'a> {
    ProjectFacts {
        flow,
        name,
        repo_path: "/tmp/m31/project",
        base_branch: "main",
        agent_names: &[],
        was_validating: &[],
        agent_filter: None,
        agents: None,
        agent_list: None,
        starts_held: None,
        view: Box::leak(Box::default()),
        paused: false,
    }
}

fn params(project: &str, width: f32, filter: &[&str]) -> Params {
    Params {
        project: project.into(),
        width,
        height: 856.,
        filter: FilterState {
            tokens: filter.iter().map(|t| (*t).to_owned()).collect(),
            text: String::new(),
        },
        agent: None,
    }
}

/// The ticket file's example: NOTE-1 in Review, NOTE-2 waiting for it, NOTE-3 in Backlog, NOTE-4 in Dev, the
/// NOTE-10 in Backlog, NOTE-11 merged and NOTE-12 in Dev.
fn pocket_notes() -> Vec<Ticket> {
    let mut note2 = ticket("NOTE-2", "Export as CSV", Status::Queued);
    note2.depends_on = vec!["NOTE-1".parse().unwrap()];
    let mut note4 = ticket("NOTE-4", "Search notes", Status::Working);
    note4.started_at = Some(NOW - 60_000);
    vec![
        ticket("NOTE-1", "Sort notes by date", Status::WaitingForYou),
        note2,
        ticket("NOTE-3", "Tag notes", Status::Backlog),
        note4,
        ticket("NOTE-10", "Sharing", Status::Backlog),
        ticket("NOTE-11", "Share link", Status::Merged),
        ticket("NOTE-12", "Share settings", Status::Working),
    ]
}

fn pocket_api() -> Vec<Ticket> {
    vec![ticket("API-1", "Rate limit the export", Status::Working)]
}

/// One card in every status.
fn every_status() -> Vec<Ticket> {
    Status::ALL
        .iter()
        .enumerate()
        .map(|(i, status)| {
            let mut t = ticket(&format!("ALL-{}", i + 1), status.label(), *status);
            if *status == Status::PrOpen {
                t.flags.pr_number = Some(481);
            }
            t
        })
        .collect()
}

/// The default flow with Validate's `rounds`, or without Validate (as M31's Validation settings derived it).
fn derived(validation: Option<u32>) -> Flow {
    let mut flow = default_flow();
    match validation {
        Some(rounds) => {
            for stage in flow.stages.iter_mut().filter(|s| s.id == "validate") {
                stage.rounds = Some(rounds);
            }
        }
        None => flow.stages.retain(|s| s.id != "validate"),
    }
    flow
}

fn project(name: &'static str, key: &str, flow: &Flow, tickets: &[Ticket]) -> Value {
    let keys = KeyLabels::base(Platform::Mac);
    let pages: Vec<Value> = [
        params(key, 1440., &[]),
        params(key, 1200., &[]),
        params(key, 1440., &["status:working"]),
    ]
    .iter()
    .map(|p| {
        serde_json::to_value(board_page::page(tickets, &facts(name, flow), p, NOW, &keys)).unwrap()
    })
    .collect();
    let drags: Vec<Value> = tickets
        .iter()
        .map(|t| serde_json::to_value(board_page::drag_answer_in(flow, t, tickets, None)).unwrap())
        .collect();
    let drops: Vec<Value> = tickets
        .iter()
        .map(|t| {
            let steps: serde_json::Map<String, Value> = DropTargetId::all(flow)
                .iter()
                .map(|target| {
                    let id = serde_json::to_value(target).unwrap();
                    let step = board_page::drop_step_in(flow, t, tickets, target);
                    (
                        id.as_str().unwrap().to_owned(),
                        Value::String(format!("{step:?}")),
                    )
                })
                .collect();
            json!({ "key": t.key.to_string(), "steps": steps })
        })
        .collect();
    json!({ "pages": pages, "drags": drags, "drops": drops })
}

fn snapshot() -> Value {
    json!({
        "pocket-notes": project("pocket-notes", "NOTE", &derived(Some(2)), &pocket_notes()),
        "pocket-api": project("pocket-api", "API", &derived(None), &pocket_api()),
        "every-status": project("every-status", "ALL", &derived(Some(3)), &every_status()),
    })
}

/// The fields T-0404 added for the page to draw any flow (a column's `kind`, a section's own drop `target`); the
/// snapshot was taken before them.
fn without_new_fields(mut snapshot: Value) -> Value {
    for project in snapshot
        .as_object_mut()
        .into_iter()
        .flat_map(|p| p.values_mut())
    {
        for page in project["pages"].as_array_mut().into_iter().flatten() {
            for column in page["columns"].as_array_mut().into_iter().flatten() {
                column.as_object_mut().map(|c| c.remove("kind"));
                for section in column["sections"].as_array_mut().into_iter().flatten() {
                    section.as_object_mut().map(|s| s.remove("target"));
                }
            }
        }
    }
    drop_footer_seconds(&mut snapshot);
    snapshot
}

/// The card footer's `second` button (T-0411: Open agent… beside Use fallback), added after the snapshot.
fn drop_footer_seconds(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if map.contains_key("action") && map.contains_key("tokens") {
                map.remove("second");
                // T-0424: the two buttons stacked (`s03-46`).
                map.remove("stacked");
            }
            // T-0531: a check of several agents' rows and the action's note (d0130), none on the default board.
            if map.contains_key("action_note") {
                map.remove("action_note");
                map.remove("check");
                // T-0540: the Plan's line (d0133), none on the default board.
                map.remove("plan");
            }
            map.values_mut().for_each(drop_footer_seconds);
        }
        Value::Array(items) => items.iter_mut().for_each(drop_footer_seconds),
        _ => {}
    }
}

#[test]
fn the_default_flow_draws_todays_board() {
    let now = snapshot();
    let update = otaq_core::naming::env_var("UPDATE_PARITY");
    if std::env::var_os(&update).is_some() {
        let now = without_new_fields(now);
        std::fs::write(FIXTURE, serde_json::to_string_pretty(&now).unwrap() + "\n").unwrap();
        return;
    }
    let then: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    assert!(
        without_new_fields(now) == then,
        "the board changed: rerun with {update}=1 and diff the fixture"
    );
}

/// The new fields on the default flow: the columns' kinds, and Approved the one section with a target of its own.
#[test]
fn the_default_flow_names_its_kinds_and_the_approved_target() {
    let page = &snapshot()["pocket-notes"]["pages"][0];
    let kinds: Vec<&str> = page["columns"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "backlog",
            "ready_for_dev",
            "agent",
            "agent",
            "human_check",
            "done"
        ]
    );
    let targets: Vec<&Value> = page["columns"][4]["sections"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| &s["target"])
        .collect();
    assert_eq!(targets, [&Value::Null, &json!("approved")]);
}
