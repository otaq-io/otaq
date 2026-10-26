//! M38's example end to end (T-0475; PAU-01…PAU-08, FED-02, FED-08, DPV-03; d0112, d0115, d0056, d0079): shop-web
//! (SHOP) on Product spec · Dev with a Security check, billing-api (BILL) on Otaq's default, and a stand-in
//! `claude` that runs the real `otaq`. Every click is the request the app sends.
//! - Pause shop-web: its agents stop as paused, billing-api keeps working and SHOP-33's Run is left alone; answering
//!   SHOP-12's question starts only its Spec writer and the pause stays; a ticket queued meanwhile keeps its start;
//!   after a restart nothing resumes; Resume brings each one back once.
//! - The blocked drag: SHOP-20 waits for SHOP-18 (in Review) and SHOP-29 (in Dev); dragged into Dev it is refused
//!   and stays put, dragged onto Ready for Dev it queues and nothing starts it; once both merge it starts in Product
//!   spec.
//! - The flow editor: a Project agent's `otaq flow propose` lands in the draft with its marks and no preview opens;
//!   Plan added after Product spec applies at once, so SHOP-12 (in Product spec) goes through it and SHOP-33 (in Dev)
//!   never meets it; Security moved before Validate waits only for Dev…Review, SHOP-12 leaving Plan waits at the span's
//!   edge, and Apply now applies it.
//!
//! No agent tool runs; every folder is temporary (the stand-in's `HOME` too), and the stand-in's PATH has no other
//! `otaq` (`path_without_build_folders`): it only ever runs this build's, by its full path.

use std::path::Path;
use std::time::Duration;

use otaq_core::agent_tools::LaunchContext;
use otaq_core::agent_tools::claude::ClaudeLaunch;
use otaq_core::agents::pause::{PauseCounts, PauseScope};
use otaq_core::board::moves::{DropOn, DropTarget};
use otaq_core::board::questions::Choice;
use otaq_core::board::tickets::Ticket;
use otaq_core::client::{ClientError, request_once};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::{Place, Status};
use otaq_core::flow::apply::{ApplyParams, FlowEvent, Wait};
use otaq_core::flow::apply_now::{ApplyNowAnswer, ApplyNowParams};
use otaq_core::flow::draft::{Draft, DraftApplies, DraftEdit, DraftUpdateParams, Mark};
use otaq_core::flow::proposal::ApplyChoices;
use otaq_core::flow::{FlowFile, yaml};
use otaq_core::naming;
use otaq_core::protocol::{
    DepParams, DragParams, ErrorBody, Event, MoveParams, NeedsYouAnswerParams,
    ProjectAgentOpenParams, ProjectParams, Reply, Request,
};
use otaq_core::service::ServiceConfig;
use otaq_core::service::testing::{TestService, path_without_build_folders};
use otaq_core::session::{END_PAUSED, SessionInfo, SessionKind, SessionTicket};

const CB: &str = env!("CARGO_BIN_EXE_otaq");

const QUESTION: &str = "Should dark mode follow the system setting?";
const ASK_PLAN: &str = "Add a stage after Product spec that writes a plan for big tickets";
const PLAN_WHY: &str = "Big tickets in shop-web get a plan first";

/// The stand-in `claude`. Its state is in `state` (Otaq's data folder in `state/data`, so its `otaq` reaches the
/// test's service). Every start is logged to `state/starts` (`<ticket> <arguments>`). Then:
/// - a ticket's agent with `state/ask-<KEY>` asks that question once (`otaq ask human`);
/// - a ticket's agent then waits for `state/go-<KEY>` (taken when seen), commits if it is a Dev
///   (`/otaq-work-ticket`), reads `otaq prime` and ends its stage (`otaq stage next`); it waits again, for the next `go`, whatever comes
///   after;
/// - a Project agent asked to add a stage after Product spec proposes `state/plan.yaml` (`otaq flow propose --stdin`).
///
/// Nothing typed into it is ever run.
fn stand_in(state: &Path) -> String {
    let state = state.display();
    let data_var = DataDir::env_var();
    let ticket_var = naming::ticket_env_var();
    format!(
        r#"
export HOME='{state}/home'
STATE='{state}'
export {data_var}="$(cat "$STATE/data")"
KEY="${{{ticket_var}-}}"
printf '%s %s\n' "${{KEY:-project}}" "$*" >> "$STATE/starts"
printf 'ARGS %s\n' "$*"
otaq() {{
    '{CB}' "$@" 2>> "$STATE/errors"
}}
case "$*" in
*'{ASK_PLAN}'*)
    otaq flow propose --stdin --silent < "$STATE/plan.yaml" > "$STATE/plan.id"
    exec sleep 300
    ;;
esac
[ -n "$KEY" ] || exec sleep 300
if [ -e "$STATE/ask-$KEY" ]; then
    mv "$STATE/ask-$KEY" "$STATE/asked-$KEY"
    otaq ask human --stdin --option Yes --option No < "$STATE/asked-$KEY" > /dev/null
fi
while :; do
    while [ ! -e "$STATE/go-$KEY" ]; do sleep 0.05; done
    rm "$STATE/go-$KEY"
    case "$*" in
    *otaq-work-ticket*)
        echo "$KEY" >> work.txt
        git add -A
        git -c user.name=t -c user.email=t@t commit -qm "$KEY: work"
        ;;
    esac
    otaq prime > /dev/null
    printf 'Done.' | otaq stage next --stdin > /dev/null
done
"#
    )
}

/// shop-web's flow: Product spec, Dev, Validate, Security, Review (a check by you), Done (a PR).
fn shop_flow() -> FlowFile {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../otaq-core/tests/flows/every-kind.yaml");
    let mut file = yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    file.flow
        .stages
        .iter_mut()
        .find(|s| s.id == "qa")
        .unwrap()
        .name = "Review".into();
    file
}

/// Plan, an Agent stage run by Spec writer, after Product spec.
fn plan_after_product_spec(file: &mut FlowFile) {
    let mut plan = file.flow.stage("product-spec").unwrap().clone();
    plan.id = "plan".into();
    plan.name = "Plan".into();
    plan.options.clear();
    plan.why = Some(PLAN_WHY.into());
    file.flow.stages.insert(1, plan);
}

/// Security moved before Validate.
fn security_first(file: &mut FlowFile) {
    let at = file.flow.position("security").unwrap();
    let security = file.flow.stages.remove(at);
    let validate = file.flow.position("validate").unwrap();
    file.flow.stages.insert(validate, security);
}

struct World {
    w: TestService,
    state: tempfile::TempDir,
}

fn launch(script: String) -> impl FnOnce(ServiceConfig) -> ServiceConfig {
    move |config| ServiceConfig {
        launch: Some(LaunchContext {
            bin_dir: "/nonexistent".into(),
            plugin_dir: "/nonexistent/plugin".into(),
            path: path_without_build_folders().into(),
            claude: ClaudeLaunch {
                program: vec!["sh".into(), "-c".into(), script, "sh".into()],
                config_dir: None,
            },
        }),
        ..config
    }
}

/// shop-web on its flow with SHOP-1…SHOP-`shop` ("filler" but for `titles`), billing-api with BILL-1…BILL-`bill`.
async fn world(shop: u32, bill: u32, titles: &[(u32, &str)]) -> World {
    let state = tempfile::TempDir::new().unwrap();
    std::fs::create_dir(state.path().join("home")).unwrap();
    let w = TestService::start_with(launch(stand_in(state.path()))).await;
    std::fs::write(
        state.path().join("data"),
        w.data.root().display().to_string(),
    )
    .unwrap();
    w.add_project("shop-web").await;
    w.add_project("billing-api").await;
    w.board.use_flow_for_tests("SHOP", &shop_flow()).await;
    for n in 1..=shop {
        let title = titles
            .iter()
            .find(|(k, _)| *k == n)
            .map_or("filler", |(_, t)| t);
        w.create("SHOP", title).await;
    }
    for _ in 1..=bill {
        w.create("BILL", "filler").await;
    }
    World { w, state }
}

impl World {
    fn path(&self, file: &str) -> std::path::PathBuf {
        self.state.path().join(file)
    }

    /// Lets `key`'s agent end its stage.
    fn go(&self, key: &str) {
        std::fs::write(self.path(&format!("go-{key}")), "").unwrap();
    }

    fn starts(&self) -> Vec<String> {
        std::fs::read_to_string(self.path("starts"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn starts_of(&self, key: &str) -> Vec<String> {
        self.starts()
            .into_iter()
            .filter(|l| l.starts_with(&format!("{key} ")))
            .collect()
    }

    /// Waits (a minute at most) until `done` holds; else fails with what the stand-in's `otaq` printed to stderr.
    async fn until(&self, what: &str, mut done: impl AsyncFnMut() -> bool) {
        let reached = tokio::time::timeout(Duration::from_secs(60), async {
            while !done().await {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await;
        assert!(
            reached.is_ok(),
            "never: {what}\n---- starts\n{}\n---- stderr\n{}",
            self.starts().join("\n"),
            std::fs::read_to_string(self.path("errors")).unwrap_or_default()
        );
    }

    async fn until_status(&self, key: &str, want: Status) {
        self.until(&format!("{key} is {want}"), async || {
            self.w.status(key).await == want
        })
        .await;
    }

    async fn until_in(&self, key: &str, stage: &str, want: Status) {
        self.until(&format!("{key} is {want} in {stage}"), async || {
            let t = self.ticket(key).await;
            t.status == want && t.place.is_in(stage)
        })
        .await;
    }

    async fn ticket(&self, key: &str) -> Ticket {
        self.w.board.ticket(key).await.unwrap()
    }

    /// `request` as the app sends it.
    async fn ask(&self, request: Request) -> Result<Reply, ErrorBody> {
        match request_once(&self.w.data.socket_path(), request).await {
            Ok(reply) => Ok(reply),
            Err(ClientError::Service(body)) => Err(body),
            Err(e) => panic!("{e}"),
        }
    }

    /// The user drags `key` onto Ready for Dev.
    async fn to_ready(&self, key: &str) {
        let reply = self
            .ask(Request::TicketMove(MoveParams {
                key: key.into(),
                target: DropTarget::ReadyForDev,
                confirmed: true,
            }))
            .await
            .unwrap();
        assert!(matches!(reply, Reply::Ticket(_)), "{reply:?}");
    }

    /// Every agent session (its own or its check's) `key` ever had, newest first.
    async fn sessions(&self, key: &str) -> Vec<SessionInfo> {
        let t = self.ticket(key).await;
        let mut all = self
            .w
            .sessions
            .history(&SessionTicket {
                id: t.id,
                key: t.key,
            })
            .await
            .unwrap();
        all.retain(|s| s.kind.runs_agent_tool());
        all
    }

    async fn running(&self, key: &str) -> usize {
        self.sessions(key)
            .await
            .iter()
            .filter(|s| s.is_running())
            .count()
    }

    fn running_in(&self, project: &str) -> Vec<SessionInfo> {
        self.w
            .sessions
            .in_memory()
            .into_iter()
            .filter(|s| s.is_running() && s.project == project && s.kind.runs_agent_tool())
            .collect()
    }

    async fn pause(&self, request: Request) -> PauseCounts {
        match self.ask(request).await.unwrap() {
            Reply::PauseCounts(counts) => *counts,
            other => panic!("{other:?}"),
        }
    }

    async fn draft(&self, request: Request) -> Draft {
        match self.ask(request).await.unwrap() {
            Reply::FlowDraft(draft) => *draft,
            other => panic!("{other:?}"),
        }
    }

    async fn stop(self) {
        for s in self
            .w
            .sessions
            .in_memory()
            .into_iter()
            .filter(|s| s.is_running())
        {
            let _ = self.w.sessions.stop(&s.id).await;
        }
        self.w.stop().await;
    }
}

/// Nothing happens for a while: long enough for a scheduler pass and a stand-in's start.
async fn settle() {
    tokio::time::sleep(Duration::from_millis(600)).await;
}

fn shop() -> PauseScope {
    PauseScope::Project {
        project: "SHOP".into(),
    }
}

/// PAU-02…PAU-07: Pause shop-web, answer SHOP-12 while paused, restart the service, Resume.
#[tokio::test(flavor = "multi_thread")]
async fn pause_answer_restart_and_resume() {
    let world = world(
        35,
        7,
        &[
            (12, "Add dark mode toggle"),
            (33, "Shipping estimate"),
            (35, "Slow checkout query"),
        ],
    )
    .await;
    std::fs::write(world.path("ask-SHOP-12"), QUESTION).unwrap();

    // SHOP-12 asks in Product spec; SHOP-33 works in Dev with its Run open; BILL-7 works.
    for key in ["SHOP-12", "SHOP-33", "BILL-7"] {
        world.to_ready(key).await;
    }
    world.until_status("SHOP-12", Status::AsksYou).await;
    world
        .until_in("SHOP-33", "product-spec", Status::Working)
        .await;
    world.go("SHOP-33");
    world.until_in("SHOP-33", "dev", Status::Working).await;
    // Entering Dev sets Working before its agent starts: wait for the stand-in's start.
    world
        .until("SHOP-33's Dev agent starts", async || {
            world.starts_of("SHOP-33").len() == 2
        })
        .await;
    world.until_status("BILL-7", Status::Working).await;
    let shop33 = world.ticket("SHOP-33").await;
    let run = world.w.shell(&shop33, SessionKind::Run).await;
    let conversation = world.sessions("SHOP-33").await[0]
        .tool_session_id
        .clone()
        .unwrap();

    // Pause shop-web.
    let paused = world.pause(Request::AgentsPause(shop())).await;
    assert!(paused.projects[0].paused_since.is_some());
    world
        .until("shop-web's agents stop", async || {
            world.running_in("SHOP").is_empty()
        })
        .await;
    for key in ["SHOP-12", "SHOP-33"] {
        let t = world.ticket(key).await;
        assert_eq!(t.status, Status::Interrupted, "{key}");
        let ended = &world.sessions(key).await[0];
        assert_eq!(ended.end_reason.as_deref(), Some(END_PAUSED), "{key}");
    }
    assert_eq!(world.w.status("BILL-7").await, Status::Working);
    assert_eq!(
        world.running_in("BILL").len(),
        1,
        "billing-api keeps working"
    );
    assert!(
        world
            .w
            .sessions
            .stored(&run.session)
            .await
            .unwrap()
            .is_running(),
        "the Run is left alone"
    );

    // SHOP-35 dragged onto Ready meanwhile keeps its start.
    world.to_ready("SHOP-35").await;
    settle().await;
    assert_eq!(world.w.status("SHOP-35").await, Status::Queued);
    assert!(world.starts_of("SHOP-35").is_empty());

    // Answering SHOP-12 starts only its Spec writer, with the answer; the pause stays.
    let reply = world
        .ask(Request::NeedsYouAnswer(NeedsYouAnswerParams {
            key: "SHOP-12".into(),
            choice: Choice::Option(1),
            clear: false,
        }))
        .await
        .unwrap();
    assert!(matches!(reply, Reply::Delivery(_)), "{reply:?}");
    world.until_status("SHOP-12", Status::Working).await;
    world
        .until("SHOP-12's Spec writer starts with the answer", async || {
            world.starts_of("SHOP-12").len() == 2
        })
        .await;
    let spec_writer = &world.sessions("SHOP-12").await[0];
    assert!(spec_writer.is_running());
    assert_eq!(
        spec_writer.agent.as_ref().map(|a| a.name.as_str()),
        Some("Spec writer")
    );
    settle().await;
    assert_eq!(world.w.status("SHOP-33").await, Status::Interrupted);
    assert_eq!(world.w.status("SHOP-35").await, Status::Queued);
    assert_eq!(
        world.running_in("SHOP").len(),
        1,
        "only SHOP-12's Spec writer"
    );
    assert!(
        world.w.pause.is_paused("SHOP").await.unwrap(),
        "the pause stays"
    );

    // A restart: the pause survives and nothing resumes.
    let World { w, state } = world;
    let w = w.restart_with(launch(stand_in(state.path()))).await;
    let world = World { w, state };
    assert!(world.w.pause.is_paused("SHOP").await.unwrap());
    assert_eq!(world.w.recovery.startup_report().paused, ["SHOP"]);
    settle().await;
    assert!(world.running_in("SHOP").is_empty(), "nothing resumes");
    assert_eq!(world.w.status("SHOP-35").await, Status::Queued);
    let before: Vec<usize> = {
        let mut n = Vec::new();
        for key in ["SHOP-12", "SHOP-33", "SHOP-35"] {
            n.push(world.starts_of(key).len());
        }
        n
    };
    assert_eq!(
        before,
        [2, 2, 0],
        "Product spec and the answer; Product spec and Dev"
    );

    // Resume: each comes back once, SHOP-33 on its own conversation, and SHOP-35's kept start runs.
    let resumed = world.pause(Request::AgentsResume(shop())).await;
    assert_eq!(resumed.projects[0].paused_since, None);
    world.until_in("SHOP-33", "dev", Status::Working).await;
    world
        .until_in("SHOP-12", "product-spec", Status::Working)
        .await;
    world
        .until_in("SHOP-35", "product-spec", Status::Working)
        .await;
    let again = world.pause(Request::AgentsResume(shop())).await;
    assert_eq!(again.projects[0].paused_since, None);
    let starts = |world: &World| -> Vec<usize> {
        ["SHOP-12", "SHOP-33", "SHOP-35"]
            .iter()
            .map(|key| world.starts_of(key).len())
            .collect()
    };
    world
        .until("each stand-in starts again", async || {
            starts(&world) == [3, 3, 1]
        })
        .await;
    settle().await;
    for key in ["SHOP-12", "SHOP-33", "SHOP-35"] {
        assert_eq!(world.running(key).await, 1, "{key}");
    }
    assert_eq!(starts(&world), [3, 3, 1], "one start each");
    let back = world.starts_of("SHOP-33").pop().unwrap();
    assert!(back.contains(&format!("--resume {conversation}")), "{back}");
    world.stop().await;
}

/// FED-02, FED-08: the agent's proposal lands in the draft; Plan added after Product spec; Security moved before
/// Validate waits for its span; Apply now.
#[tokio::test(flavor = "multi_thread")]
async fn the_flow_editor_adds_plan_and_moves_security() {
    let world = world(
        33,
        0,
        &[(12, "Add dark mode toggle"), (33, "Shipping estimate")],
    )
    .await;
    let mut with_plan = shop_flow();
    plan_after_product_spec(&mut with_plan);
    std::fs::write(world.path("plan.yaml"), yaml::write(&with_plan)).unwrap();

    // SHOP-12 in Product spec, SHOP-33 in Dev.
    world.to_ready("SHOP-12").await;
    world.to_ready("SHOP-33").await;
    world
        .until_in("SHOP-12", "product-spec", Status::Working)
        .await;
    world
        .until_in("SHOP-33", "product-spec", Status::Working)
        .await;
    world.go("SHOP-33");
    world.until_in("SHOP-33", "dev", Status::Working).await;

    // The user asks the Project agent from the Ask bar; its `otaq flow propose` lands in the draft, marked, and no
    // preview opens.
    let mut listener = world.w.events.listen();
    let reply = world
        .ask(Request::ProjectAgentOpen(ProjectAgentOpenParams {
            project: "SHOP".into(),
            message: ASK_PLAN.into(),
            agent: None,
        }))
        .await
        .unwrap();
    assert!(matches!(reply, Reply::ProjectAgent(_)), "{reply:?}");
    world
        .until("the agent proposes", async || {
            world.path("plan.id").exists()
        })
        .await;
    let mut changes = Vec::new();
    let reached = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if let Event::FlowChanged { project, change } = listener.receiver.recv().await.unwrap()
                && project == "SHOP"
            {
                changes.push(change.clone());
                if matches!(change, FlowEvent::DraftChanged { .. }) {
                    break;
                }
            }
        }
    })
    .await;
    assert!(reached.is_ok(), "{changes:?}");
    assert!(
        !changes
            .iter()
            .any(|c| matches!(c, FlowEvent::Proposed { .. })),
        "no preview: {changes:?}"
    );
    let project = ProjectParams {
        project: "SHOP".into(),
    };
    let draft = world.draft(Request::FlowDraft(project.clone())).await;
    let marks: Vec<(&str, Mark, Option<&str>)> = draft
        .marks
        .iter()
        .map(|m| (m.stage.as_str(), m.mark, m.why.as_deref()))
        .collect();
    assert_eq!(marks, [("Plan", Mark::Added, Some(PLAN_WHY))]);
    assert!(!draft.marks[0].by.is_empty(), "{:?}", draft.marks[0]);
    assert_eq!(draft.applies, DraftApplies::AtOnce);
    let stages: Vec<String> = world
        .w
        .board
        .effective_flow("SHOP")
        .await
        .unwrap()
        .stages
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(!stages.contains(&"plan".to_owned()), "nothing applies yet");

    // Apply: at once. SHOP-12 leaves Product spec for Plan; SHOP-33 leaves Dev for Validate.
    let reply = world
        .ask(Request::FlowApply(ApplyParams {
            project: "SHOP".into(),
            proposal: draft.proposal.id,
            choices: ApplyChoices::default(),
        }))
        .await
        .unwrap();
    let Reply::FlowApplied(applied) = reply else {
        panic!("{reply:?}")
    };
    assert!(applied.waiting.is_none());
    world.go("SHOP-12");
    world.until_in("SHOP-12", "plan", Status::Working).await;
    world.go("SHOP-33");
    world
        .until_in("SHOP-33", "validate", Status::Checking)
        .await;
    let stages = |runs: Vec<otaq_core::board::stage_runs::StageRun>| {
        runs.into_iter().map(|r| r.stage).collect::<Vec<_>>()
    };
    assert_eq!(
        stages(world.w.board.stage_runs("SHOP-12").await.unwrap()),
        ["product-spec", "plan"]
    );
    assert_eq!(
        stages(world.w.board.stage_runs("SHOP-33").await.unwrap()),
        ["product-spec", "dev", "validate"],
        "SHOP-33 never meets Plan"
    );

    // Security moved before Validate in the editor: it waits for Dev…Review, where SHOP-33 checks.
    let mut moved = shop_flow();
    plan_after_product_spec(&mut moved);
    security_first(&mut moved);
    let draft = world
        .draft(Request::FlowDraftUpdate(DraftUpdateParams {
            project: "SHOP".into(),
            edit: DraftEdit::Document {
                yaml: yaml::write(&moved),
            },
        }))
        .await;
    let DraftApplies::Span { span, inside } = &draft.applies else {
        panic!("{:?}", draft.applies)
    };
    assert_eq!((span.first.as_str(), span.last.as_str()), ("Dev", "Review"));
    let inside: Vec<(String, &str)> = inside
        .iter()
        .map(|i| (i.ticket.to_string(), i.stage.as_str()))
        .collect();
    assert_eq!(inside, [("SHOP-33".to_owned(), "validate")]);
    let reply = world
        .ask(Request::FlowApply(ApplyParams {
            project: "SHOP".into(),
            proposal: draft.proposal.id,
            choices: ApplyChoices::default(),
        }))
        .await
        .unwrap();
    let Reply::FlowApplied(applied) = reply else {
        panic!("{reply:?}")
    };
    let waiting = applied.waiting.expect("it waits");
    assert!(matches!(&waiting.wait, Wait::Span(s) if s.first == "Dev" && s.last == "Review"));
    assert_eq!(
        waiting
            .waiting_for
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        ["SHOP-33"]
    );

    // SHOP-12 leaves Plan: it waits at the span's edge, before Dev, and doesn't start.
    world.go("SHOP-12");
    world
        .until("SHOP-12 waits before Dev", async || {
            world.ticket("SHOP-12").await.place == Place::Waiting(Some("dev".into()))
        })
        .await;
    settle().await;
    let t = world.ticket("SHOP-12").await;
    assert_eq!(
        (t.status, &t.place),
        (Status::Queued, &Place::Waiting(Some("dev".into()))),
        "held at the edge"
    );
    let dev = |world: &World| {
        world
            .starts_of("SHOP-12")
            .iter()
            .filter(|l| l.contains("otaq-work-ticket"))
            .count()
    };
    assert_eq!(dev(&world), 0, "no Dev agent");

    // Apply now: SHOP-33 keeps its place, the change applies, SHOP-12 starts in Dev.
    let reply = world
        .ask(Request::FlowApplyNowPlan(project.clone()))
        .await
        .unwrap();
    let Reply::ApplyNowPlan(Some(plan)) = reply else {
        panic!("{reply:?}")
    };
    assert!(plan.moves.is_empty());
    let kept: Vec<String> = plan.keep.iter().map(|k| k.ticket.to_string()).collect();
    assert_eq!(kept, ["SHOP-33"]);
    let reply = world
        .ask(Request::FlowApplyNow(ApplyNowParams {
            project: "SHOP".into(),
            stamp: plan.stamp.clone(),
            picks: Vec::new(),
        }))
        .await
        .unwrap();
    let Reply::AppliedNow(answer) = reply else {
        panic!("{reply:?}")
    };
    assert!(
        matches!(*answer, ApplyNowAnswer::Applied { .. }),
        "{answer:?}"
    );
    let order: Vec<String> = world
        .w
        .board
        .effective_flow("SHOP")
        .await
        .unwrap()
        .stages
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(
        order,
        [
            "product-spec",
            "plan",
            "dev",
            "security",
            "validate",
            "qa",
            "ship"
        ]
    );
    world.until_in("SHOP-12", "dev", Status::Working).await;
    // The stand-in logs its start a moment after the session counts as working.
    world
        .until("SHOP-12's Dev agent logs its start", async || {
            dev(&world) == 1
        })
        .await;
    assert_eq!(dev(&world), 1);
    let t = world.ticket("SHOP-33").await;
    assert!(t.place.is_in("validate"), "{:?}", t.place);
    world.stop().await;
}

/// DPV-03: SHOP-20 blocked by SHOP-18 (in your check) and SHOP-29 (in Dev) can't be dragged into Dev; onto Ready for
/// Dev it queues and waits; once both merge the scheduler starts it.
#[tokio::test(flavor = "multi_thread")]
async fn a_blocked_ticket_waits_in_ready_for_dev() {
    let world = world(
        29,
        0,
        &[
            (18, "Checkout API"),
            (20, "Checkout page"),
            (29, "Cart totals"),
        ],
    )
    .await;
    let shop18 = world.ticket("SHOP-18").await;
    let shop29 = world.ticket("SHOP-29").await;
    world
        .w
        .force_stage(&shop18, "qa", Status::WaitingForYou)
        .await;
    world.w.force_stage(&shop29, "dev", Status::Working).await;
    for parent in ["SHOP-18", "SHOP-29"] {
        world
            .ask(Request::DepAdd(DepParams {
                child: "SHOP-20".into(),
                parent: parent.into(),
            }))
            .await
            .unwrap();
    }
    let drag = |key: &str, stage: &str| {
        Request::TicketDrag(DragParams {
            key: key.into(),
            target: DropOn::stage(stage),
            confirmed: true,
            note: String::new(),
            clear_first: false,
            report: None,
        })
    };

    // Into Dev: refused, and it stays in Backlog.
    let refused = world.ask(drag("SHOP-20", "dev")).await.unwrap_err();
    assert!(
        refused.message.contains("SHOP-20 is blocked"),
        "{refused:?}"
    );
    assert_eq!(world.w.status("SHOP-20").await, Status::Backlog);

    // Onto Ready for Dev: it queues and nothing starts it.
    world.to_ready("SHOP-20").await;
    settle().await;
    assert_eq!(world.w.status("SHOP-20").await, Status::Queued);
    assert!(world.starts_of("SHOP-20").is_empty());
    let again = world.ask(drag("SHOP-20", "dev")).await.unwrap_err();
    assert!(again.message.contains("SHOP-20 is blocked"), "{again:?}");

    // Both merge: it starts in Product spec.
    world.w.force_status(&shop18, Status::Merged).await;
    world.w.force_status(&shop29, Status::Merged).await;
    world.w.events.publish(Event::ProjectsChanged);
    world
        .until_in("SHOP-20", "product-spec", Status::Working)
        .await;
    world
        .until("SHOP-20's Spec writer starts", async || {
            world.starts_of("SHOP-20").len() == 1
        })
        .await;
    settle().await;
    assert_eq!(world.starts_of("SHOP-20").len(), 1, "once");
    world.stop().await;
}
