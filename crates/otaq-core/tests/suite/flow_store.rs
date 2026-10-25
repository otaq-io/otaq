//! A project's flow, stored (FLW-01, T-0412; d0108): a new project gets the shipped default flow as version 1, every
//! applied flow is kept in order, the board runs the current one, `use_flow_for_tests` stores a version, a rename
//! follows into every stored flow, and no code reads the Validation, Validation rounds or Auto-merge settings or
//! M32's move-over any more.

use std::path::{Path, PathBuf};

use crate::common::Fixture;
use otaq_core::client::{Client, ClientConfig};
use otaq_core::flow::default::testing::{with_rounds, without_validate};
use otaq_core::flow::{self, AppliedBy, Column, NewFlowVersion, yaml};
use otaq_core::naming;
use otaq_core::protocol::{ProjectParams, Reply, Request};
use otaq_core::service::testing::TestService;

fn ids(flow: &flow::Flow) -> Vec<&str> {
    flow.stages.iter().map(|s| s.id.as_str()).collect()
}

fn every_kind() -> flow::FlowFile {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// A new project's version 1 is `otaq-default.yaml` as shipped, applied by Otaq from that preset; the board runs it.
/// Validate has its own column, Done shares Review's.
#[tokio::test]
async fn a_new_project_starts_on_the_default_flow() {
    let f = Fixture::shop().await;
    let current = f.board.current_flow("SHOP").await.unwrap();
    assert_eq!(
        (
            current.project.as_str(),
            current.version,
            current.yaml.as_str(),
            current.applied_by,
            current.agent_name.as_deref(),
            current.preset.as_deref(),
            current.current
        ),
        (
            "SHOP",
            1,
            naming::DEFAULT_FLOW_YAML,
            AppliedBy::Shipped,
            None,
            Some("otaq-default.yaml"),
            true
        )
    );
    assert!(current.applied_at > 0);
    let running = f.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(running, flow::default_flow());
    assert_eq!(ids(&running), ["dev", "validate", "review", "deliver"]);
    let column = |id: &str| running.stage(id).unwrap().column;
    assert_eq!(column("validate"), Column::Own);
    assert_eq!(column("deliver"), Column::WithPrevious);
    assert!(f.board.effective_flow("NOPE").await.is_err());
}

/// Versions are kept in the order applied, each with who applied it; only the last is current, and the board reads
/// it at once. Back to an earlier flow is one more version.
#[tokio::test]
async fn versions_are_kept_in_order_and_the_current_one_runs() {
    let f = Fixture::shop().await;
    f.board.edit_flow_for_tests("SHOP", without_validate).await;
    assert_eq!(
        ids(&f.board.effective_flow("SHOP").await.unwrap()),
        ["dev", "review", "deliver"]
    );
    let by_agent = f
        .board
        .store_flow(
            "SHOP",
            &NewFlowVersion {
                yaml: naming::DEFAULT_FLOW_YAML.to_owned(),
                applied_by: AppliedBy::ProjectAgent,
                agent_name: Some("Checkout slowness".into()),
                preset: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        (by_agent.version, by_agent.agent_name.as_deref()),
        (3, Some("Checkout slowness"))
    );
    f.board.edit_flow_for_tests("SHOP", with_rounds(5)).await;
    let running = f.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(running.stage("validate").unwrap().rounds, Some(5));

    let versions = f.board.flow_versions("SHOP").await.unwrap();
    let shown: Vec<(u32, AppliedBy, bool)> = versions
        .iter()
        .map(|v| (v.version, v.applied_by, v.current))
        .collect();
    assert_eq!(
        shown,
        [
            (1, AppliedBy::Shipped, false),
            (2, AppliedBy::User, false),
            (3, AppliedBy::ProjectAgent, false),
            (4, AppliedBy::User, true),
        ]
    );
    assert_eq!(f.board.current_flow("SHOP").await.unwrap(), versions[3]);

    // A document that isn't a flow is refused, and nothing changes.
    let refused = f
        .board
        .store_flow(
            "SHOP",
            &NewFlowVersion {
                yaml: "flow: [".into(),
                applied_by: AppliedBy::User,
                agent_name: None,
                preset: None,
            },
        )
        .await;
    assert!(refused.is_err());
    assert_eq!(f.board.flow_versions("SHOP").await.unwrap().len(), 4);
}

/// `use_flow_for_tests` stores the file as a version (no flow lives only in memory any more), and the flow is the
/// project's own: another project stays on the default.
#[tokio::test]
async fn use_flow_for_tests_stores_a_version() {
    let f = Fixture::shop().await;
    f.add_project("pocket-notes").await;
    let file = every_kind();
    f.board.use_flow_for_tests("SHOP", &file).await;
    let current = f.board.current_flow("SHOP").await.unwrap();
    assert_eq!((current.version, current.applied_by), (2, AppliedBy::User));
    assert_eq!(current.file().unwrap(), file);
    assert_eq!(f.board.effective_flow("SHOP").await.unwrap(), file.flow);
    assert_eq!(
        f.board.effective_flow("POCK").await.unwrap(),
        flow::default_flow()
    );
}

/// AGM-01: flows name agents by name, so renaming an agent renames it in every stored version (an earlier one too,
/// for Back), and the stage still runs it.
#[tokio::test]
async fn a_rename_follows_into_every_stored_flow() {
    let f = Fixture::shop().await;
    f.board.edit_flow_for_tests("SHOP", with_rounds(2)).await;
    let store = f.board.store();
    let validator = store.agent_named("Validator").await.unwrap().unwrap();
    let mut fields = otaq_core::agents::store::NewAgent {
        name: "Checker".into(),
        instructions: validator.instructions.clone(),
        skills: validator.skills.clone(),
        imported: validator.imported.clone(),
        model: validator.model.clone(),
        effort: validator.effort.clone(),
        role: validator.role,
    };
    store
        .update_agent(&validator.id, &fields, false)
        .await
        .unwrap();
    for version in f.board.flow_versions("SHOP").await.unwrap() {
        let file = version.file().unwrap();
        assert!(file.agents.iter().any(|a| a.name == "Checker"));
        assert!(!version.yaml.contains("Validator"), "{}", version.yaml);
    }
    let running = f.board.effective_flow("SHOP").await.unwrap();
    assert_eq!(
        running.stage("validate").unwrap().agent.as_deref(),
        Some("Checker")
    );
    assert_eq!(running.stage("validate").unwrap().rounds, Some(2));
    // Saving it with its name unchanged rewrites nothing.
    let before = f.board.flow_versions("SHOP").await.unwrap();
    fields.instructions = "Be strict.".into();
    store
        .update_agent(&validator.id, &fields, false)
        .await
        .unwrap();
    assert_eq!(f.board.flow_versions("SHOP").await.unwrap(), before);
}

/// `flow.current` and `flow.versions` over the socket.
#[tokio::test(flavor = "multi_thread")]
async fn the_requests_answer_with_the_stored_versions() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let params = || ProjectParams {
        project: "SHOP".into(),
    };
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let Reply::FlowVersion(current) = client
        .request(Request::FlowCurrent(params()))
        .await
        .unwrap()
    else {
        panic!("flow.current")
    };
    assert_eq!((current.version, current.current), (1, true));
    let Reply::FlowVersions(versions) = client
        .request(Request::FlowVersions(params()))
        .await
        .unwrap()
    else {
        panic!("flow.versions")
    };
    assert_eq!(versions, [*current]);
    w.stop().await;
}

/// The source files of the workspace that ship (no tests, no migrations, no generated types).
fn sources() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if !["node_modules", "target", "generated", "tests", "dist"]
                    .contains(&name.as_str())
                {
                    walk(&path, out);
                }
            } else if (name.ends_with(".rs") || name.ends_with(".ts") || name.ends_with(".tsx"))
                && !name.contains(".test.")
            {
                out.push(path);
            }
        }
    }
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut out = Vec::new();
    for dir in [
        "otaq-core/src",
        "otaq-view/src",
        "otaq-desktop/src",
        "otaq-desktop/ui/src",
        "otaq-cli/src",
    ] {
        walk(&crates.join(dir), &mut out);
    }
    out
}

/// FLW-01 (d0108, no migration): nothing reads the Validation, Validation rounds or Auto-merge settings, derives a
/// flow from them, or keeps M32's move-over, its notice or the `replaces` link. The patterns name the removed code,
/// not the words: `auto_merge_stop` and the stage's `rounds` stay.
#[test]
fn no_code_reads_the_removed_settings_or_the_move_over() {
    let gone = [
        "settings.validation",
        "effective.auto_merge",
        "settings.auto_merge",
        "auto_merge_inactive",
        "AutoMerge::Off",
        "AutoMerge::EpicChildren",
        "AutoMerge::All",
        "effective_rounds",
        "rounds_problem",
        "validation_rounds_of",
        "effective_flow(&EffectiveSettings",
        "fn ticket_flow",
        "move_over",
        "moved_over",
        "MOVED_OVER",
        "AGENT_FLOW_NAMES",
        "flow_names()",
        "dismiss_dropped",
        "AgentsNotice",
        "stands_for",
        ".replaces",
        "replaces:",
        "\"validation_rounds\"",
        "\"auto_merge\"",
        "auto_merge_off",
        "EpicChildrenAutoMerge",
    ];
    let mut found = Vec::new();
    for path in sources() {
        let text = std::fs::read_to_string(&path).unwrap();
        if text.contains("#![cfg(test)]") {
            continue;
        }
        // A file's own tests (from `#[cfg(test)]` on) may name what is gone, to prove it is.
        for (n, line) in text
            .lines()
            .take_while(|l| !l.contains("#[cfg(test)]"))
            .enumerate()
        {
            for pattern in gone {
                if line.contains(pattern) {
                    found.push(format!("{}:{}: {pattern}", path.display(), n + 1));
                }
            }
            // A setting's field, not the ticket's `validation_rounds_extra` or the stage's `rounds`.
            for (at, _) in line.match_indices(".validation_rounds") {
                let next = line[at + ".validation_rounds".len()..].chars().next();
                if !next.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    found.push(format!("{}:{}: .validation_rounds", path.display(), n + 1));
                }
            }
        }
    }
    assert!(found.is_empty(), "{found:#?}");
}
