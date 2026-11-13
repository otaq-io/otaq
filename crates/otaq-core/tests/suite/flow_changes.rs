//! Proposals, what applies when, and the quiet-board wait (T-0413; FLW-03, FLW-05, FLW-06, FLW-09, FLW-10; d0108):
//! each refusal names its stage or agent; the diff keeps ids on a rename and says when each change applies; a shape
//! change waits for a quiet board while the project's starts are held; Cancel change resumes them; a second change
//! merges into the waiting one; a choice the flow drops falls back to the
//! stage's agent with a History note. On the example's names: pocket-api, API-1, QA, Security, Quick developer and
//! Rust developer (`tests/flows/every-kind.yaml`). `claude` is stood in for by an ordinary command.

use std::collections::HashSet;
use std::path::Path;

use otaq_core::agents::FixedModels;
use otaq_core::board::moves::DropTarget;
use otaq_core::board::tickets::Ticket;
use otaq_core::board::{BoardError, Refusal};
use otaq_core::domain::{Actor, Status};
use otaq_core::flow::apply::{ProposeFrom, Wait};
use otaq_core::flow::proposal::{
    About, AgentChangeKind, Applies, ApplyChoices, Change, ClashChoice, Fix, NewProposal, Proposal,
    ProposedBy, SAME, Source,
};
use otaq_core::flow::{FlowFile, StageOption, check, yaml};
use otaq_core::scheduler::{Gate, plan_holding};
use otaq_core::service::testing::TestService;

fn me() -> Actor {
    Actor::User("me".into())
}

fn models() -> FixedModels {
    FixedModels::sample()
}

fn planner() -> ProposedBy {
    ProposedBy::ProjectAgent {
        agent: "pa-1".into(),
        session: "session-1".into(),
        name: "Noyan".into(),
        tool: otaq_core::agent_tools::Tool::Claude,
    }
}

fn flows() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows")
}

fn every_kind() -> FlowFile {
    yaml::read(&std::fs::read_to_string(flows().join("every-kind.yaml")).unwrap()).unwrap()
}

/// A document of `file`'s stages only, with no ids: what a Project agent writes.
fn stages_only(file: &FlowFile) -> String {
    let text = yaml::write(&FlowFile {
        agents: Vec::new(),
        flow: file.flow.clone(),
    });
    text.lines()
        .filter(|l| !l.trim_start().starts_with("- id:"))
        .map(|l| {
            if l.starts_with("    name:") {
                l.replacen("    name:", "  - name:", 1)
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

/// A stage whose agent's tool doesn't have its skill only warns: "Codex doesn’t have the skill `$team-validate`" on the
/// stage, and the flow applies (the stage is held when due to start on that tool). The same skill on an agent whose
/// tool has it doesn't warn.
#[tokio::test(flavor = "multi_thread")]
async fn a_stage_whose_agents_tool_lacks_its_skill_warns() {
    let (w, key) = world().await;
    // Committed in the project's `.claude/skills`: Claude Code, Copilot and OpenCode read it; Codex doesn't.
    let repo = std::path::PathBuf::from(w.board.project(&key).await.unwrap().repo_path);
    std::fs::create_dir_all(repo.join(".claude/skills/team-validate")).unwrap();
    otaq_core::git::testing::commit(
        &repo,
        ".claude/skills/team-validate/SKILL.md",
        "---\nname: team-validate\ndescription: The team's check.\n---\n\nCheck it.\n",
    );
    let doc = |name: &str, model: &str| {
        format!(
            "agents:\n  - name: {name}\n    model: {model}\nflow:\n  - name: Dev\n    kind: agent\n    agent: \
             Developer\n    skill: work-ticket\n  - name: Validate\n    kind: check\n    by: agent\n    agent: \
             {name}\n    skill: team-validate\n    rounds: 1\n  - name: Review\n    kind: check\n    by: you\n  \
             - name: Done\n    kind: delivery\n    column: with-previous\n    agent: Developer\n    mode: click\n    \
             delivers: either\n"
        )
    };
    let warned = |p: &Proposal| -> Vec<(Option<String>, String)> {
        p.diff
            .warnings
            .iter()
            .map(|w| (w.stage.clone(), w.text.clone()))
            .collect()
    };
    let on_codex = propose_as(&w, &key, &doc("Codex checker", "codex"), planner())
        .await
        .unwrap();
    assert!(on_codex.problems.is_empty(), "{:?}", on_codex.problems);
    assert_eq!(
        warned(&on_codex),
        [(
            Some("Validate".to_owned()),
            "Codex doesn\u{2019}t have the skill `$team-validate`".to_owned()
        )]
    );
    w.board
        .apply_proposal(&key, on_codex.id, &ApplyChoices::default(), &models())
        .await
        .expect("only a warning: the flow applies");
    let on_claude = propose_as(&w, &key, &doc("Claude checker", "claude"), planner())
        .await
        .unwrap();
    assert_eq!(warned(&on_claude), []);
    w.stop().await;
}

/// pocket-api on the default flow, its key.
async fn world() -> (TestService, String) {
    let w = TestService::start().await;
    let key = w.add_project("pocket-api").await.key;
    (w, key)
}

async fn propose_as(
    w: &TestService,
    key: &str,
    yaml: &str,
    by: ProposedBy,
) -> Result<Proposal, BoardError> {
    w.board
        .open_proposal(
            key,
            &NewProposal {
                yaml: yaml.to_owned(),
                source: Source::Document { file: None },
                proposed_by: by,
            },
            &models(),
        )
        .await
}

fn refusal(result: Result<Proposal, BoardError>) -> String {
    match result {
        Err(e) => {
            assert_eq!(e.refusal(), Some(Refusal::BadFlow), "{e}");
            e.to_string()
        }
        Ok(p) => panic!("not refused: {:?}", p.diff),
    }
}

/// The default flow with `edit` made to its file.
fn default_with(edit: impl FnOnce(&mut FlowFile)) -> FlowFile {
    let mut file = otaq_core::flow::default_file().clone();
    edit(&mut file);
    file
}

/// QA, a check by you, after `after` in the default flow.
fn with_qa_after(after: &str) -> FlowFile {
    default_with(|file| {
        let at = file.flow.position(after).unwrap() + 1;
        let mut qa = file.flow.stage("review").unwrap().clone();
        qa.id = String::new();
        qa.name = "QA".into();
        file.flow.stages.insert(at, qa);
    })
}

async fn create(w: &TestService, key: &str, title: &str) -> Ticket {
    w.create(key, title).await
}

/// FLW-03: every refusal names its stage or agent: each has its broken file in `tests/flows/broken/` (each STG-02
/// rule; an unknown agent, a bad agent name, a model that doesn't read or the tool doesn't list, a written `_` id, a
/// document that doesn't read). A skill the user doesn't have is never refused: the stage carries a warning, and the
/// flow applies (T6). The user's own import opens with the problems marked instead, with what to do about the fixable
/// ones.
#[tokio::test(flavor = "multi_thread")]
async fn each_refusal_names_its_stage_or_agent() {
    let (w, key) = world().await;

    // A valid one opens, and the same flow says so.
    let same = propose_as(&w, &key, otaq_core::naming::DEFAULT_FLOW_YAML, planner())
        .await
        .unwrap();
    assert!(same.problems.is_empty());
    assert!(same.diff.same, "{:?}", same.diff);
    assert_eq!(SAME, "This is the same as your current flow.");

    let mut broken: Vec<_> = std::fs::read_dir(flows().join("broken"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    broken.sort();
    assert_eq!(broken.len(), 15);
    for path in broken {
        let text = std::fs::read_to_string(&path).unwrap();
        let message = refusal(propose_as(&w, &key, &text, planner()).await);
        // A refusal other than STG-02's says on its first line what it is refused with (T-0425).
        if let Some(expected) = text.lines().next().unwrap().strip_prefix("# Refused: ") {
            assert!(
                message.contains(expected),
                "{}: {message:?} lacks {expected:?}",
                path.display()
            );
            continue;
        }
        let file = yaml::read(&text).unwrap();
        let problems = check(&file.flow).expect_err("a broken file");
        for problem in problems {
            assert!(
                message.contains(&problem.message),
                "{}: {message:?} lacks {:?}",
                path.display(),
                problem.message
            );
            if let Some(stage) = problem.stage.and_then(|id| file.flow.stage(&id).cloned()) {
                assert!(
                    problem.message.contains(&stage.name) || problem.message.contains(&stage.id),
                    "{}",
                    problem.message
                );
            }
        }
    }

    // A model the tool doesn't list is refused; a skill the user doesn't have only warns, and an old file's agent
    // `skills:` still reads and is ignored.
    let reviewer = "agents:\n  - name: Security reviewer\n    skills: [team-security-review]\n    model: claude:gpt-9\n";
    let doc = format!(
        "{reviewer}flow:\n  - name: Dev\n    kind: agent\n    agent: Developer\n    skill: work-ticket\n  - name: \
         Security\n    kind: check\n    by: agent\n    agent: Security reviewer\n    skill: team-security-review\n    \
         rounds: 2\n  - name: Review\n    kind: check\n    by: you\n  - name: Done\n    kind: delivery\n    column: \
         with-previous\n    agent: Developer\n    mode: click\n    delivers: either\n"
    );
    let message = refusal(propose_as(&w, &key, &doc, planner()).await);
    assert!(
        message.contains(
            "Security reviewer's model gpt-9 isn't in Claude Code's model list: pick a model from it"
        ),
        "{message}"
    );
    assert!(!message.contains("team-security-review"), "{message}");
    // The user's import opens, marked: Apply waits for a local model only.
    let imported = w
        .board
        .propose_from(
            &key,
            &ProposeFrom::File {
                name: "team.yaml".into(),
                yaml: doc.clone(),
            },
            &models(),
        )
        .await
        .unwrap();
    let fixes: Vec<(&About, Option<&Fix>)> = imported
        .problems
        .iter()
        .map(|p| (&p.about, p.fix.as_ref()))
        .collect();
    assert_eq!(
        fixes,
        [(
            &About::Agent {
                name: "Security reviewer".into(),
            },
            Some(&Fix::PickModel {
                agent: "Security reviewer".into()
            })
        )]
    );
    let warned: Vec<(Option<&str>, &str)> = imported
        .diff
        .warnings
        .iter()
        .map(|w| (w.stage.as_deref(), w.text.as_str()))
        .collect();
    assert!(
        warned.contains(&(
            Some("Security"),
            "You don't have the skill `team-security-review`"
        )),
        "{warned:?}"
    );
    assert!(
        !warned.iter().any(|(_, text)| text.contains("work-ticket")),
        "Otaq's own skills never warn: {warned:?}"
    );
    let refused = w
        .board
        .apply_proposal(&key, imported.id, &ApplyChoices::default(), &models())
        .await
        .unwrap_err();
    assert_eq!(refused.refusal(), Some(Refusal::BadFlow));
    // With the model picked it applies, the unknown skill and all; the stage names it as written.
    let choices = ApplyChoices {
        models: [("Security reviewer".to_owned(), "claude:opus".to_owned())].into(),
        ..Default::default()
    };
    let applied = w
        .board
        .apply_proposal(&key, imported.id, &choices, &models())
        .await
        .unwrap();
    assert_eq!(applied.agents, ["Security reviewer"]);
    let flow = w.board.effective_flow(&key).await.unwrap();
    let security = flow.stages.iter().find(|s| s.name == "Security").unwrap();
    assert_eq!(security.skill.as_deref(), Some("team-security-review"));
    // The stored flow never writes an agent's `skills:`.
    let stored = w.board.store().current_flow(&key).await.unwrap().unwrap();
    assert!(!stored.yaml.contains("skills:"), "{}", stored.yaml);

    w.stop().await;
}

/// A stage's skill can't be a tool's built-in command: `/security-review` on a Claude Code check is refused with what
/// to do instead, shows as a problem on the stage in the user's own proposal, and is fine once the user has a skill of
/// that name (it is theirs).
#[tokio::test(flavor = "multi_thread")]
async fn a_built_in_command_is_not_a_skill() {
    let (w, key) = world().await;
    let doc = "agents:\n  - name: Security reviewer\n    model: claude:opus\nflow:\n  - name: Dev\n    kind: agent\n    \
               agent: Developer\n    skill: work-ticket\n  - name: Security\n    kind: check\n    by: agent\n    agent: \
               Security reviewer\n    skill: security-review\n    rounds: 2\n  - name: Done\n    kind: delivery\n    \
               column: with-previous\n    agent: Developer\n    mode: click\n    delivers: either\n";
    let said = "Security: `/security-review` is Claude Code's built-in command, not a skill. Write a small skill that \
                runs it, and name that skill here";
    let message = refusal(propose_as(&w, &key, doc, planner()).await);
    assert!(message.contains(said), "{message}");

    let mine = propose_as(&w, &key, doc, ProposedBy::User).await.unwrap();
    // Security takes the id of the stage at its place in the current flow (Validate's).
    let problems: Vec<(Option<&str>, &str)> = mine
        .problems
        .iter()
        .map(|p| match &p.about {
            About::Stage { name, .. } => (Some(name.as_str()), p.message.as_str()),
            _ => (None, p.message.as_str()),
        })
        .collect();
    assert_eq!(problems, [(Some("Security"), said)]);
    assert!(
        !mine
            .diff
            .warnings
            .iter()
            .any(|w| w.text.contains("security-review")),
        "the problem says it, no warning too: {:?}",
        mine.diff.warnings
    );

    w.claude_has_skill("security-review").await;
    let accepted = propose_as(&w, &key, doc, planner()).await.unwrap();
    assert!(accepted.problems.is_empty(), "{:?}", accepted.problems);

    w.stop().await;
}

/// FLW-03, FLW-05, FED-08: a rename is a rename (Review's id kept, applied at once, nothing waits); adding QA applies
/// at once, removing Validate waits for its span; a stage's new agent "applies when Security next starts"; a changed shared agent lists every
/// project that uses it; a removed option lists the tickets that chose it.
#[tokio::test(flavor = "multi_thread")]
async fn the_diff_says_what_changes_and_when() {
    let (w, key) = world().await;

    // Adding QA after Validate applies at once (FED-08).
    let qa = with_qa_after("validate");
    let proposal = propose_as(&w, &key, &stages_only(&qa), planner())
        .await
        .unwrap();
    let added: Vec<(&str, &str, bool, String)> = proposal
        .diff
        .stages
        .iter()
        .map(|c| {
            (
                c.stage.as_str(),
                c.text.as_str(),
                c.change.is_shape(),
                c.applies.text(),
            )
        })
        .collect();
    assert!(
        added.contains(&(
            "QA",
            "stage QA (your check) after Validate",
            true,
            "applies at once".to_owned()
        )),
        "{added:?}"
    );
    assert!(!proposal.diff.waits);
    w.board.cancel_proposal(&key).await.unwrap();

    // Removing Validate waits for Dev to Review: Dev → Validate and Validate → Review are gone.
    let no_validate = default_with(|f| f.flow.stages.retain(|s| s.id != "validate"));
    let proposal = propose_as(&w, &key, &stages_only(&no_validate), planner())
        .await
        .unwrap();
    let removed: Vec<String> = proposal
        .diff
        .stages
        .iter()
        .map(|c| format!("{} \u{2014} {}", c.text, c.applies.text()))
        .collect();
    assert_eq!(
        removed,
        ["stage Validate removed \u{2014} waits until no ticket is between Dev and Review"]
    );
    assert!(proposal.diff.waits);
    w.board.cancel_proposal(&key).await.unwrap();

    // A rename keeps the id: applied now, no waiting change.
    let renamed = default_with(|f| f.flow.stages[2].name = "QA".into());
    let proposal = propose_as(&w, &key, &stages_only(&renamed), planner())
        .await
        .unwrap();
    let only: Vec<(&str, &Change, &Applies)> = proposal
        .diff
        .stages
        .iter()
        .map(|c| (c.stage_id.as_str(), &c.change, &c.applies))
        .collect();
    assert_eq!(
        only,
        [(
            "review",
            &Change::Renamed {
                from: "Review".into()
            },
            &Applies::AtOnce
        )]
    );
    assert_eq!(proposal.diff.stages[0].text, "stage Review \u{2192} QA");
    assert!(!proposal.diff.waits);
    let applied = w
        .board
        .apply_proposal(&key, proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(applied.waiting.is_none());
    let flow = w.board.effective_flow(&key).await.unwrap();
    assert_eq!(flow.stage("review").unwrap().name, "QA");

    // On the test flow, in two projects: Security's agent, and a shared agent's model.
    let other = w.add_project("pocket-notes").await.key;
    w.board.use_flow_for_tests(&key, &every_kind()).await;
    w.board.use_flow_for_tests(&other, &every_kind()).await;
    let mut strict = every_kind();
    strict
        .flow
        .stages
        .iter_mut()
        .find(|s| s.id == "security")
        .unwrap()
        .agent = Some("Validator".into());
    let proposal = propose_as(&w, &key, &stages_only(&strict), planner())
        .await
        .unwrap();
    assert!(proposal.problems.is_empty(), "{:?}", proposal.problems);
    let security: Vec<String> = proposal
        .diff
        .stages
        .iter()
        .map(|c| format!("{} \u{2014} {}", c.text, c.applies.text()))
        .collect();
    assert_eq!(
        security,
        [
            "Security \u{b7} agent: Security reviewer \u{2192} Validator \u{2014} applies when Security next starts"
        ]
    );
    assert!(!proposal.diff.waits);

    let shared = format!(
        "agents:\n  - name: Security reviewer\n    skills: [team-security-review]\n    model: claude:opus\n{}",
        stages_only(&every_kind())
    );
    let proposal = propose_as(&w, &key, &shared, planner()).await.unwrap();
    let reviewer = &proposal.diff.agents[0];
    assert_eq!(reviewer.change, AgentChangeKind::Changed);
    assert_eq!(
        reviewer.clash, None,
        "a Project agent's change isn't a clash"
    );
    let fields: Vec<(&str, &str, &str)> = reviewer
        .fields
        .iter()
        .map(|f| (f.field.as_str(), f.from.as_str(), f.to.as_str()))
        .collect();
    assert!(
        fields.contains(&("model", "default", "claude:opus")),
        "{fields:?}"
    );
    let used: Vec<String> = reviewer.used_in.iter().map(|u| u.label()).collect();
    assert_eq!(
        used,
        ["pocket-api \u{b7} Security", "pocket-notes \u{b7} Security"]
    );

    w.stop().await;
}

/// The project's waiting to begin tickets the scheduler would start now (FLW-06's hold), after a settling pass.
async fn would_start(w: &TestService, key: &str) -> Vec<String> {
    let settled = w.board.settle_flow_changes().await.unwrap();
    let tickets = w.board.tickets(key).await.unwrap();
    plan_holding(
        &tickets,
        &HashSet::new(),
        &Gate::default(),
        &settled.held,
        &HashSet::new(),
    )
    .start
    .into_iter()
    .map(|t| t.key.to_string())
    .collect()
}

/// FLW-06, FED-08, sentence by sentence: Validate removed while API-1 is in Dev waits for its span, Dev to Review,
/// which holds the first Agent stage, so no ticket of pocket-api starts while waiting to begin (another project's still
/// does); when API-1 reaches Review (a check by you that stays) nothing inside is at work: the change applies and
/// starts resume. Cancel change resumes starts too. A second apply while one waits was compared with the waiting flow
/// (only its own part shows) and merges into it, even a part that would otherwise apply at once; the banner names
/// both.
#[tokio::test(flavor = "multi_thread")]
async fn a_removal_waits_for_its_span() {
    let (w, key) = world().await;
    let api1 = create(&w, &key, "Rate limits").await;
    let api2 = create(&w, &key, "Pagination").await;
    w.force_stage(&api1, "dev", Status::Working).await;
    w.board
        .move_ticket(&api2.key.to_string(), DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    let notes = w.add_project("pocket-notes").await.key;
    let note = create(&w, &notes, "Search").await;
    w.board
        .move_ticket(&note.key.to_string(), DropTarget::ReadyForDev, true, &me())
        .await
        .unwrap();
    // API-3's PR is open: a card waiting in the Delivery stage doesn't hold a change (Q-new-1).
    let api3 = create(&w, &key, "Webhooks").await;
    w.force_stage(&api3, "deliver", Status::PrOpen).await;
    assert_eq!(would_start(&w, &key).await, [api2.key.to_string()]);

    // Validate removed: it waits for API-1 only.
    let no_validate = default_with(|f| f.flow.stages.retain(|s| s.id != "validate"));
    let proposal = propose_as(&w, &key, &stages_only(&no_validate), planner())
        .await
        .unwrap();
    let applied = w
        .board
        .apply_proposal(&key, proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert_eq!(applied.version, None, "nothing applies now");
    let waiting = applied.waiting.unwrap();
    assert_eq!(waiting.summary(), "Validate removed");
    assert_eq!(waiting.waiting_for, std::slice::from_ref(&api1.key));
    let Wait::Span(span) = &waiting.wait else {
        panic!("{:?}", waiting.wait)
    };
    assert_eq!((span.first.as_str(), span.last.as_str()), ("Dev", "Review"));
    assert!(waiting.hold.ready_for_dev);
    assert!(w.board.proposal(&key, &models()).await.unwrap().is_none());
    assert!(would_start(&w, &key).await.is_empty(), "starts held");
    assert_eq!(
        would_start(&w, &notes).await,
        [note.key.to_string()],
        "other projects keep starting"
    );
    assert!(
        w.board
            .effective_flow(&key)
            .await
            .unwrap()
            .stage("validate")
            .is_some()
    );

    // A second change while it waits: compared with the waiting flow, so only Review's skip shows; it merges.
    let mut skippable = no_validate.clone();
    skippable
        .flow
        .stages
        .iter_mut()
        .find(|s| s.id == "review")
        .unwrap()
        .options = vec![StageOption::Skip];
    let second = propose_as(&w, &key, &stages_only(&skippable), planner())
        .await
        .unwrap();
    assert!(second.diff.against_waiting);
    let shown: Vec<(&str, &Applies)> = second
        .diff
        .stages
        .iter()
        .map(|c| (c.label.as_str(), &c.applies))
        .collect();
    assert_eq!(shown, [("Review can be skipped", &Applies::QuietBoard)]);
    let merged = w
        .board
        .apply_proposal(&key, second.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert_eq!(
        merged.version, None,
        "even the part that would apply at once waits"
    );
    assert_eq!(
        merged.waiting.unwrap().summary(),
        "Validate removed \u{b7} Review can be skipped"
    );
    assert!(!w.board.effective_flow(&key).await.unwrap().stages[2].can_skip());

    // API-1 reaches Review, a check by you the new flow keeps: quiet. The change applies and starts resume.
    w.force_stage(&api1, "review", Status::WaitingForYou).await;
    assert_eq!(would_start(&w, &key).await, [api2.key.to_string()]);
    assert!(w.board.waiting_change(&key).await.unwrap().is_none());
    let flow = w.board.effective_flow(&key).await.unwrap();
    let ids: Vec<&str> = flow.stages.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["dev", "review"]);
    assert!(flow.stage("review").unwrap().can_skip());
    let current = w.board.current_flow(&key).await.unwrap();
    assert_eq!(
        (current.version, current.agent_name.as_deref()),
        (2, Some("Noyan"))
    );

    // Cancel change: API-1 back in Dev, a removal waits, Cancel drops it and starts resume. Nothing else changes.
    w.force_stage(&api1, "dev", Status::Working).await;
    let mut removing = w.board.current_flow_file(&key).await.unwrap();
    removing.flow.stages.retain(|s| s.id != "review");
    let proposal = propose_as(&w, &key, &stages_only(&removing), planner())
        .await
        .unwrap();
    w.board
        .apply_proposal(&key, proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(would_start(&w, &key).await.is_empty());
    assert!(w.board.cancel_waiting(&key).await.unwrap());
    assert_eq!(would_start(&w, &key).await, [api2.key.to_string()]);
    // Review's removal is gone, and nothing else applied.
    let current = w.board.current_flow(&key).await.unwrap();
    assert_eq!(current.version, 2);
    assert!(current.file().unwrap().flow.stage("review").is_some());
    w.stop().await;
}

/// FLW-09, FLW-10: Back to the previous flow brings its stages only, never agents; Copy from another project brings
/// that project's stages, matched by name; a preset that is the current flow is the same; the shipped agents file
/// brings agents only, each changed one a clash (Keep mine by default · Take theirs).
#[tokio::test(flavor = "multi_thread")]
async fn the_other_sources() {
    let (w, key) = world().await;
    assert_eq!(
        w.board
            .propose_from(&key, &ProposeFrom::Previous { version: None }, &models())
            .await
            .unwrap_err()
            .to_string(),
        format!("{key} has no previous flow")
    );
    w.board
        .edit_flow_for_tests(&key, otaq_core::flow::default::testing::without_validate)
        .await;
    let back = w
        .board
        .propose_from(&key, &ProposeFrom::Previous { version: None }, &models())
        .await
        .unwrap();
    assert_eq!(back.source, Source::Previous { version: 1 });
    assert!(back.diff.agents.is_empty(), "stages only");
    assert_eq!(
        back.diff.stages[0].text,
        "stage Validate (agent check) after Dev"
    );

    let other = w.add_project("pocket-notes").await.key;
    let copy = w
        .board
        .propose_from(
            &key,
            &ProposeFrom::Project {
                project: "pocket-notes".into(),
            },
            &models(),
        )
        .await
        .unwrap();
    assert_eq!(
        copy.source,
        Source::Project {
            key: other.clone(),
            name: "pocket-notes".into()
        }
    );
    assert_eq!(copy.diff.stages.len(), 1, "{:?}", copy.diff.stages);

    let preset = w
        .board
        .propose_from(
            &other,
            &ProposeFrom::Preset {
                file: otaq_core::naming::DEFAULT_FLOW_FILE.into(),
            },
            &models(),
        )
        .await
        .unwrap();
    assert!(preset.diff.same, "{:?}", preset.diff);

    // The agents file: Noyan edited here, so it clashes; Keep mine leaves it, Take theirs puts it back.
    w.edit_agent("Noyan", |page| page.instructions = "Mine.".into())
        .await;
    let agents = w
        .board
        .propose_from(&key, &ProposeFrom::AgentsFile, &models())
        .await
        .unwrap();
    assert!(agents.diff.stages.is_empty(), "agents only");
    let planner = agents
        .diff
        .agents
        .iter()
        .find(|a| a.name == "Noyan")
        .unwrap();
    assert_eq!(planner.change, AgentChangeKind::Changed);
    assert_eq!(planner.clash.as_ref().unwrap().add_as, None);
    w.board
        .apply_proposal(&key, agents.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    let kept = w.board.store().agent_named("Noyan").await.unwrap().unwrap();
    assert_eq!(kept.instructions, "Mine.");
    let again = w
        .board
        .propose_from(&key, &ProposeFrom::AgentsFile, &models())
        .await
        .unwrap();
    let choices = ApplyChoices {
        clashes: [("Noyan".to_owned(), ClashChoice::TakeTheirs)].into(),
        ..Default::default()
    };
    w.board
        .apply_proposal(&key, again.id, &choices, &models())
        .await
        .unwrap();
    let theirs = w.board.store().agent_named("Noyan").await.unwrap().unwrap();
    assert_eq!(theirs.instructions, "");
    w.stop().await;
}

/// FLW-06, FLW-09, FED-08: an addition with an edit is stored once, whole (no in-between version, no waiting change).
/// A removal applied while a ticket works inside its span stores its edit now and the whole later; Back to the
/// previous flow then skips that in-between part, to the flow the user had before the change.
#[tokio::test(flavor = "multi_thread")]
async fn one_version_per_apply_and_back_skips_the_part() {
    let (w, key) = world().await;
    // QA after Review, and Validate on 5 rounds: both apply at once.
    let change = |file: &mut FlowFile| {
        file.flow
            .stages
            .iter_mut()
            .find(|s| s.id == "validate")
            .unwrap()
            .rounds = Some(5);
    };
    let mut first = with_qa_after("review");
    change(&mut first);
    let proposal = propose_as(&w, &key, &stages_only(&first), planner())
        .await
        .unwrap();
    assert!(!proposal.diff.waits);
    let applied = w
        .board
        .apply_proposal(&key, proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(applied.waiting.is_none());
    let versions = w.board.flow_versions(&key).await.unwrap();
    assert_eq!(versions.len(), 2, "stored whole, once");
    let now = versions[1].file().unwrap().flow;
    assert!(now.stage("qa").is_some());
    assert_eq!(now.stage("validate").unwrap().rounds, Some(5));

    // While API-1 works: Validate on 2 rounds now, QA's removal waits; then API-1 reaches Review.
    let api1 = create(&w, &key, "Rate limits").await;
    w.force_stage(&api1, "dev", Status::Working).await;
    let mut second = otaq_core::flow::default_file().clone();
    second
        .flow
        .stages
        .iter_mut()
        .find(|s| s.id == "validate")
        .unwrap()
        .rounds = Some(2);
    let proposal = propose_as(&w, &key, &stages_only(&second), planner())
        .await
        .unwrap();
    let applied = w
        .board
        .apply_proposal(&key, proposal.id, &ApplyChoices::default(), &models())
        .await
        .unwrap();
    assert!(applied.waiting.is_some());
    assert_eq!(
        applied.version.unwrap().version,
        3,
        "the part that doesn't wait"
    );
    w.force_stage(&api1, "review", Status::WaitingForYou).await;
    assert!(w.board.settle_flow_change(&key).await.unwrap().is_some());
    assert_eq!(w.board.flow_versions(&key).await.unwrap().len(), 4);

    // Back goes to version 2, the flow before that change, not to the in-between version 3.
    let back = w
        .board
        .propose_from(&key, &ProposeFrom::Previous { version: None }, &models())
        .await
        .unwrap();
    assert_eq!(back.source, Source::Previous { version: 2 });
    w.stop().await;
}
