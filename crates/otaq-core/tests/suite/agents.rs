//! Agents (T-0405; AGM-01, AGM-03, AGM-05; d0108, d0088): a fresh install's agents, the name rules, one agent per
//! role (delete and role removal refused, the role moved), delete refused while a stage uses an agent, models from
//! the tool's list with the tool's own effort, skills, the default model and the fallbacks, the flow's names bound to
//! ids (a rename changes nothing), the holds' store, and the requests (the user's only).

use std::sync::Arc;

use otaq_core::agent_tools::Tool;
use otaq_core::agents::store::HoldScope;
use otaq_core::agents::{
    Agent, AgentFields, AgentHold, AgentModel, Agents, CameFrom, FixedModels, HoldReason, Mark,
    ModelRef, Role, SessionRole, SkillRef, resolve,
};
use otaq_core::board::Refusal;
use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::flow::{self, FlowFile};
use otaq_core::protocol::{Event, Reply, Request};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

use crate::common::Fixture;

async fn world() -> (Fixture, Agents) {
    let f = Fixture::shop().await;
    let agents = Agents::new(f.board.clone(), Arc::new(FixedModels::sample()));
    (f, agents)
}

fn claude(model: Option<&str>) -> AgentModel {
    AgentModel::Tool {
        tool: Tool::ALL[0],
        model: model.map(str::to_owned),
    }
}

fn fields(name: &str, model: AgentModel) -> AgentFields {
    AgentFields {
        name: name.into(),
        instructions: String::new(),
        skills: vec![SkillRef::Shipped("work-ticket".into())],
        model,
        effort: None,
    }
}

async fn named(agents: &Agents, name: &str) -> Agent {
    agents
        .list()
        .await
        .unwrap()
        .agents
        .into_iter()
        .find(|a| a.name == name)
        .unwrap_or_else(|| panic!("no agent {name}"))
}

fn refusal<T: std::fmt::Debug>(
    result: Result<T, otaq_core::board::BoardError>,
) -> (Refusal, String) {
    let e = result.expect_err("refused");
    (e.refusal().expect("a refusal"), e.to_string())
}

/// AGM-02 (fresh install): Otaq's agents file and the default flow's agents, every one on `default`, with their
/// roles, skills and where they came from; the default model not set until a tool connects.
#[tokio::test]
async fn a_fresh_install_has_the_shipped_agents_on_default() {
    let (_f, agents) = world().await;
    let list = agents.list().await.unwrap();
    let summary: Vec<(&str, Option<Role>, &AgentModel, &CameFrom)> = list
        .agents
        .iter()
        .map(|a| (a.name.as_str(), a.role, &a.model, &a.came_from))
        .collect();
    assert_eq!(
        summary,
        [
            (
                "Noyan",
                Some(Role::ProjectAgent),
                &AgentModel::Default,
                &CameFrom::AgentsFile
            ),
            (
                "Epic lead",
                Some(Role::EpicLead),
                &AgentModel::Default,
                &CameFrom::AgentsFile
            ),
            (
                "Developer",
                None,
                &AgentModel::Default,
                &CameFrom::DefaultPreset
            ),
            (
                "Validator",
                None,
                &AgentModel::Default,
                &CameFrom::DefaultPreset
            ),
        ]
    );
    let skills = |name: &str| {
        list.agents
            .iter()
            .find(|a| a.name == name)
            .unwrap()
            .skills
            .clone()
    };
    let shipped = |names: &[&str]| -> Vec<SkillRef> {
        names
            .iter()
            .map(|n| SkillRef::Shipped((*n).into()))
            .collect()
    };
    assert_eq!(skills("Developer"), shipped(&["work-ticket", "commit"]));
    assert_eq!(skills("Validator"), shipped(&["validate"]));
    assert_eq!(skills("Noyan"), shipped(&["noyan", "flow"]));
    assert_eq!(skills("Epic lead"), shipped(&["epic-lead"]));
    assert_eq!(list.default_model, None);
    assert!(list.fallbacks.is_empty());
}

/// AGM-01, Q256: names are trimmed and new ignoring case; `skip` and names starting with `_` are refused.
#[tokio::test]
async fn names_are_new_ignoring_case_and_never_skip_or_underscore() {
    let (_f, agents) = world().await;
    let rust = agents
        .add(&fields("  Rust developer ", claude(Some("opus"))))
        .await
        .unwrap();
    assert_eq!(rust.name, "Rust developer");
    assert_eq!(rust.came_from, CameFrom::You);
    assert_eq!(
        refusal(agents.add(&fields("developer", AgentModel::Default)).await),
        (
            Refusal::NameTaken,
            "Another agent is already called developer".into()
        )
    );
    assert_eq!(
        refusal(agents.add(&fields("Skip", AgentModel::Default)).await).0,
        Refusal::BadName
    );
    assert_eq!(
        refusal(agents.add(&fields("_x", AgentModel::Default)).await).0,
        Refusal::BadName
    );
    assert_eq!(
        refusal(agents.add(&fields(" ", AgentModel::Default)).await).0,
        Refusal::EmptyField
    );
    // A rename onto another agent's name, ignoring case, is refused; onto its own name in another case it isn't.
    assert_eq!(
        refusal(
            agents
                .edit(&rust.id, &fields("VALIDATOR", AgentModel::Default))
                .await
        )
        .0,
        Refusal::NameTaken
    );
    let renamed = agents
        .edit(&rust.id, &fields("rust Developer", AgentModel::Default))
        .await
        .unwrap();
    assert_eq!(renamed.name, "rust Developer");
}

/// AGM-01: exactly one agent holds each role: deleting it is refused with AGM-01's text, an edit keeps the role, and
/// the role moves to another agent in one step (after which the old holder can go).
#[tokio::test]
async fn one_agent_per_role_and_the_role_moves() {
    let (_f, agents) = world().await;
    let planner = named(&agents, "Noyan").await;
    assert_eq!(
        refusal(agents.delete(&planner.id).await),
        (
            Refusal::AgentInUse,
            "Noyan has the Noyan role. Give the role to another agent first".into()
        )
    );
    let lead = named(&agents, "Epic lead").await;
    assert_eq!(
        refusal(agents.delete(&lead.id).await).1,
        "Epic lead is your Epic lead. Give the role to another agent first"
    );
    // An edit can't remove the role.
    let edited = agents
        .edit(&planner.id, &fields("Noyan", claude(Some("opus"))))
        .await
        .unwrap();
    assert_eq!(edited.role, Some(Role::ProjectAgent));

    let strong = agents
        .add(&fields("Strong", claude(Some("opus"))))
        .await
        .unwrap();
    let moved = agents
        .move_role(Role::ProjectAgent, &strong.id)
        .await
        .unwrap();
    assert_eq!(moved.role, Some(Role::ProjectAgent));
    assert_eq!(named(&agents, "Noyan").await.role, None);
    let holders: Vec<String> = agents
        .list()
        .await
        .unwrap()
        .agents
        .into_iter()
        .filter(|a| a.role == Some(Role::ProjectAgent))
        .map(|a| a.name)
        .collect();
    assert_eq!(holders, ["Strong"]);
    // The Epic lead's role can't go to the Project agent: the Project agent's role would be left with no agent.
    assert_eq!(
        refusal(agents.move_role(Role::EpicLead, &strong.id).await),
        (
            Refusal::AgentInUse,
            "Strong has the Noyan role. Give the role to another agent first".into()
        )
    );
    // Giving it the role it holds changes nothing.
    agents
        .move_role(Role::ProjectAgent, &strong.id)
        .await
        .unwrap();
    agents.delete(&planner.id).await.unwrap();
    assert!(agents.agent(&planner.id).await.is_err());
}

/// AGM-01: delete is refused while a stage of a project's flow names the agent, naming the stages; with no project
/// the default flow's stages count (every project derives from it); an unused agent goes.
#[tokio::test]
async fn delete_is_refused_while_a_stage_uses_the_agent() {
    let (f, agents) = world().await;
    let developer = named(&agents, "Developer").await;
    assert_eq!(
        refusal(agents.delete(&developer.id).await),
        (
            Refusal::AgentInUse,
            "Developer is used in shop-web \u{b7} Dev and shop-web \u{b7} Done. Change those stages first."
                .into()
        )
    );
    let validator = named(&agents, "Validator").await;
    assert_eq!(
        refusal(agents.delete(&validator.id).await).1,
        "Validator is used in shop-web \u{b7} Validate. Change that stage first."
    );
    // shop-web turns Validation off: Validator is still the default flow's.
    f.board
        .edit_flow_for_tests("SHOP", otaq_core::flow::default::testing::without_validate)
        .await;
    let uses = agents.in_use(&validator.id).await.unwrap();
    assert_eq!(
        uses.refusal().unwrap(),
        format!(
            "Validator is used in {}'s default flow \u{b7} Validate. Change that stage first.",
            otaq_core::naming::PRODUCT_NAME
        )
    );
    let spare = agents
        .add(&fields("Spare", AgentModel::Default))
        .await
        .unwrap();
    assert!(agents.in_use(&spare.id).await.unwrap().is_empty());
    agents.delete(&spare.id).await.unwrap();
}

/// AGM-01 with d0130: every check agent an agent check lists is used there, the second as much as the first.
#[tokio::test]
async fn every_check_agent_is_in_use() {
    let (f, agents) = world().await;
    let second = agents
        .add(&fields("Performance reviewer", AgentModel::Default))
        .await
        .unwrap();
    f.board
        .edit_flow_for_tests("SHOP", |file| {
            for stage in file.flow.stages.iter_mut().filter(|s| s.id == "validate") {
                stage.agents.push("Performance reviewer".into());
            }
        })
        .await;
    assert_eq!(
        refusal(agents.delete(&second.id).await),
        (
            Refusal::AgentInUse,
            "Performance reviewer is used in shop-web \u{b7} Validate. Change that stage first."
                .into()
        )
    );
    let validator = named(&agents, "Validator").await;
    assert_eq!(
        refusal(agents.delete(&validator.id).await).1,
        "Validator is used in shop-web \u{b7} Validate. Change that stage first."
    );
}

/// AGT-02, AGM-01: a model from the tool's list (or the tool's default, or `default`); the tool's own effort level
/// only with a tool that has one, and one of its levels.
#[tokio::test]
async fn models_come_from_the_list_and_effort_only_where_the_tool_has_one() {
    let (_f, agents) = world().await;
    assert_eq!(
        refusal(
            agents
                .add(&fields("Fable", claude(Some("claude-fable-5-1"))))
                .await
        )
        .0,
        Refusal::ModelNotListed
    );
    // `default` in the tool's list is its own default: no model stored.
    let plain = agents
        .add(&fields("Plain", claude(Some("default"))))
        .await
        .unwrap();
    assert_eq!(plain.model, claude(None));

    let level = Tool::ALL[0].effort_levels()[2];
    let mut high = fields("Thinker", claude(Some("opus")));
    high.effort = Some(level.into());
    assert_eq!(
        agents.add(&high).await.unwrap().effort.as_deref(),
        Some(level)
    );
    high.name = "Turbo".into();
    high.effort = Some("turbo".into());
    assert_eq!(refusal(agents.add(&high).await).0, Refusal::BadEffort);
    high.model = AgentModel::Default;
    high.effort = Some(level.into());
    assert_eq!(
        refusal(agents.add(&high).await).1,
        "An effort level goes with a tool's model, not with default"
    );
    // A tool without a documented effort level gets none.
    let none = *Tool::ALL
        .iter()
        .find(|t| t.effort_levels().is_empty())
        .expect("a tool without effort levels");
    high.model = AgentModel::Tool {
        tool: none,
        model: None,
    };
    assert_eq!(
        refusal(agents.add(&high).await).1,
        format!("{} has no effort level", none.display_name())
    );
}

/// AGM-01: shipped skills by the names Otaq ships; an imported skill must be one an agent has.
#[tokio::test]
async fn skills_are_shipped_ones_or_imported_ones() {
    let (_f, agents) = world().await;
    let mut odd = fields("Odd", AgentModel::Default);
    odd.skills = vec![SkillRef::Shipped("not-a-skill".into())];
    assert_eq!(refusal(agents.add(&odd).await).0, Refusal::BadSkill);
    odd.skills = vec![SkillRef::Imported("0199-gone".into())];
    assert_eq!(refusal(agents.add(&odd).await).0, Refusal::BadSkill);
    odd.skills = vec![
        SkillRef::Shipped("work-ticket".into()),
        SkillRef::Shipped("commit".into()),
        SkillRef::Shipped("work-ticket".into()),
    ];
    let added = agents.add(&odd).await.unwrap();
    assert_eq!(added.skills.len(), 2, "a skill once");
}

/// AGT-05: a saved model change clears the mark; a rename doesn't. Duplicate is a draft named "<name> copy".
#[tokio::test]
async fn a_model_change_clears_the_mark_and_duplicate_is_a_draft() {
    let (f, agents) = world().await;
    let developer = named(&agents, "Developer").await;
    otaq_core::agents::mark::set(
        f.board.store(),
        &developer.id,
        &Mark {
            text: "Claude Code couldn't run fable on SHOP-1".into(),
            model: Some("fable".into()),
            ticket: Some("SHOP-1".into()),
            message: None,
            at: 1,
        },
    )
    .await
    .unwrap();
    let mut page = fields("Builder", AgentModel::Default);
    page.skills = developer.skills.clone();
    let renamed = agents.edit(&developer.id, &page).await.unwrap();
    assert!(renamed.mark.is_some(), "a rename keeps it");
    page.model = claude(Some("sonnet"));
    let saved = agents.edit(&developer.id, &page).await.unwrap();
    assert_eq!(saved.mark, None);

    let draft = agents.duplicate(&developer.id).await.unwrap();
    assert_eq!(draft.name, "Builder copy");
    assert_eq!(draft.model, claude(Some("sonnet")));
    assert_eq!(draft.skills, developer.skills);
}

/// AGM-01: a rename changes nothing: it follows into every stored flow, so SHOP's Dev stage still runs the renamed
/// Developer, by its new name.
#[tokio::test]
async fn a_rename_changes_nothing_for_the_flow() {
    let (f, agents) = world().await;
    let developer = named(&agents, "Developer").await;
    let mut page = fields("Rust developer", AgentModel::Default);
    page.skills = developer.skills.clone();
    agents.edit(&developer.id, &page).await.unwrap();
    let t = f.create("Price table").await;
    let running = resolve::session_agent(&f.board, &t, SessionRole::Developer)
        .await
        .unwrap();
    assert_eq!(
        (running.id.as_str(), running.name.as_str()),
        (developer.id.as_str(), "Rust developer")
    );
    let validator = resolve::session_agent(&f.board, &t, SessionRole::Validator)
        .await
        .unwrap();
    assert_eq!(validator.name, "Validator");
    let lead = resolve::session_agent(&f.board, &t, SessionRole::EpicLead)
        .await
        .unwrap();
    assert_eq!(lead.role, Some(Role::EpicLead));
}

/// AGM-01, AGM-13: the test flow's own agents are created by `use_flow_for_tests` ("Added by SHOP's flow"; a role
/// another agent holds stays with it); an unknown name in a flow is a check error naming the stage.
#[tokio::test]
async fn a_flow_names_agents_and_an_unknown_one_is_a_check_error() {
    let (f, agents) = world().await;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/flows/every-kind.yaml");
    let file = flow::yaml::read(&std::fs::read_to_string(path).unwrap()).unwrap();
    f.board.use_flow_for_tests("SHOP", &file).await;
    let griller = named(&agents, "Griller").await;
    assert_eq!(
        griller.came_from,
        CameFrom::Flow {
            project: "SHOP".into()
        }
    );
    assert_eq!(griller.skills, [SkillRef::Shipped("grill".into())]);
    assert_eq!(
        named(&agents, "Noyan").await.came_from,
        CameFrom::AgentsFile
    );
    let names: Vec<String> = agents
        .list()
        .await
        .unwrap()
        .agents
        .into_iter()
        .map(|a| a.name)
        .collect();
    for name in [
        "Griller",
        "Rust developer",
        "Quick developer",
        "Security reviewer",
        "Shipper",
    ] {
        assert!(names.contains(&name.to_owned()), "{name}: {names:?}");
    }
    let t = f.create("Price table").await;
    let running = resolve::session_agent(&f.board, &t, SessionRole::Developer)
        .await
        .unwrap();
    assert_eq!(running.name, "Griller", "the first Agent stage is Grill");

    // Every name is known: the check passes; an unknown one names its stage.
    assert_eq!(
        flow::check_agents(&file.flow, |n| names.iter().any(|a| a == n)),
        Ok(())
    );
    let without_quick: Vec<&String> = names.iter().filter(|n| *n != "Quick developer").collect();
    let problems =
        flow::check_agents(&file.flow, |n| without_quick.iter().any(|a| *a == n)).unwrap_err();
    assert_eq!(problems.len(), 1);
    assert_eq!(problems[0].stage.as_deref(), Some("dev"));
    assert_eq!(
        problems[0].message,
        "Dev names the agent Quick developer, which isn't one of your agents"
    );
    // Deleting an agent a stage option names is refused too.
    let quick = named(&agents, "Quick developer").await;
    assert_eq!(
        refusal(agents.delete(&quick.id).await).1,
        "Quick developer is used in shop-web \u{b7} Dev. Change that stage first."
    );
    // A flow file that names an agent it doesn't define and nobody has: its stage can't resolve.
    let mut lonely = FlowFile {
        agents: Vec::new(),
        flow: flow::default_flow(),
    };
    lonely.flow.stages[0].agent = Some("Nobody".into());
    f.add_project("billing-api").await;
    let bill = f
        .board
        .projects()
        .await
        .unwrap()
        .into_iter()
        .find(|p| p.name == "billing-api")
        .unwrap()
        .key;
    f.board.use_flow_for_tests(&bill, &lonely).await;
    let other = f
        .board
        .create_ticket(
            &otaq_core::board::tickets::NewTicketInput {
                project: bill.clone(),
                title: "Invoices".into(),
                ..Default::default()
            },
            &otaq_core::domain::Actor::User("me".into()),
        )
        .await
        .unwrap();
    let e = resolve::session_agent(&f.board, &other, SessionRole::Developer)
        .await
        .unwrap_err();
    assert_eq!(e.refusal(), Some(Refusal::UnknownAgent));
    assert_eq!(
        e.to_string(),
        "Dev names the agent Nobody, which isn't one of your agents"
    );
}

/// AGM-03: the default model and each tool's fallback come from the tools' lists; a fallback is on another tool.
#[tokio::test]
async fn the_default_model_and_the_fallbacks_are_checked() {
    let (_f, agents) = world().await;
    let [a, b] = [Tool::ALL[0], Tool::ALL[1]];
    let model = |tool, model: &str| ModelRef {
        tool,
        model: Some(model.into()),
    };
    assert_eq!(
        refusal(agents.set_default_model(&model(a, "claude-x")).await).0,
        Refusal::ModelNotListed
    );
    agents.set_default_model(&model(a, "haiku")).await.unwrap();
    assert_eq!(
        agents.list().await.unwrap().default_model,
        Some(model(a, "haiku"))
    );
    assert_eq!(
        refusal(agents.set_fallback(a, Some(&model(a, "sonnet"))).await),
        (
            Refusal::BadFallback,
            "A fallback must be on another tool".into()
        )
    );
    agents
        .set_fallback(a, Some(&model(b, "opus")))
        .await
        .unwrap();
    let list = agents.list().await.unwrap();
    assert_eq!(list.fallbacks.len(), 1);
    assert_eq!(
        (list.fallbacks[0].tool, &list.fallbacks[0].fallback),
        (a, &model(b, "opus"))
    );
    agents.set_fallback(a, None).await.unwrap();
    assert!(agents.list().await.unwrap().fallbacks.is_empty());
}

fn hold(
    ticket: Option<&str>,
    project: Option<&str>,
    role: SessionRole,
    reason: HoldReason,
) -> AgentHold {
    AgentHold {
        id: uuid::Uuid::now_v7().to_string(),
        ticket_id: ticket.map(str::to_owned),
        project_id: project.map(str::to_owned),
        check_agent: None,
        role,
        reason,
        tool: Tool::ALL[0],
        model: Some("opus".into()),
        agent_id: None,
        fallback_tool: None,
        fallback_held: false,
        fallback_asked: false,
        since: 100,
    }
}

/// SCH-04 `[M23]`: one hold per ticket (or project) and session role; setting it again replaces it and keeps when it
/// started while the reason stays, and keeps the user's Use fallback; queries by ticket, tool and reason; clearing
/// lifts it.
#[tokio::test]
async fn holds_are_one_per_scope_and_role() {
    let (f, _agents) = world().await;
    let store = f.board.store();
    let shop31 = f.create("Checkout flow").await;
    let shop24 = f.create("Update footer links").await;
    let project = f.board.project("SHOP").await.unwrap();

    let mut developer = hold(
        Some(&shop31.id),
        None,
        SessionRole::Developer,
        HoldReason::ToolLimit,
    );
    store.set_agent_hold(&developer).await.unwrap();
    store
        .set_agent_hold(&hold(
            Some(&shop31.id),
            None,
            SessionRole::Validator,
            HoldReason::ToolLimit,
        ))
        .await
        .unwrap();
    store
        .set_agent_hold(&hold(
            Some(&shop24.id),
            None,
            SessionRole::Developer,
            HoldReason::ModelUnavailable,
        ))
        .await
        .unwrap();
    store
        .set_agent_hold(&hold(
            None,
            Some(&project.id),
            SessionRole::ProjectAgent,
            HoldReason::ToolUnavailable,
        ))
        .await
        .unwrap();

    // Use fallback on the Developer's hold, then the same role again: replaced, the start and the ask kept while the
    // reason stays.
    assert!(store.ask_fallback(&developer.id).await.unwrap());
    developer.id = "another".into();
    developer.since = 900;
    developer.fallback_tool = Some(Tool::ALL[0]);
    developer.fallback_held = true;
    store.set_agent_hold(&developer).await.unwrap();
    let held = store.ticket_holds(&shop31.id).await.unwrap();
    assert_eq!(held.len(), 2);
    let dev = held
        .iter()
        .find(|h| h.role == SessionRole::Developer)
        .unwrap();
    assert_eq!(
        (
            dev.since,
            dev.fallback_tool,
            dev.fallback_held,
            dev.fallback_asked
        ),
        (100, Some(Tool::ALL[0]), true, true)
    );
    developer.reason = HoldReason::ToolUnavailable;
    store.set_agent_hold(&developer).await.unwrap();
    let held = store.ticket_holds(&shop31.id).await.unwrap();
    let dev = held
        .iter()
        .find(|h| h.role == SessionRole::Developer)
        .unwrap();
    assert_eq!((dev.reason, dev.since), (HoldReason::ToolUnavailable, 900));

    // A second row for the same scope and role can't be written by hand either.
    let twice = sqlx::query(
        "INSERT INTO agent_holds (id, ticket_id, role, reason, tool, since)
         VALUES ('x', ?, 'validator', 'tool_limit', 'claude', 1)",
    )
    .bind(&shop31.id)
    .execute(store.pool())
    .await;
    assert!(twice.is_err(), "one hold per ticket and role");
    let both = sqlx::query(
        "INSERT INTO agent_holds (id, ticket_id, project_id, role, reason, tool, since)
         VALUES ('y', ?, ?, 'lead', 'tool_limit', 'claude', 1)",
    )
    .bind(&shop24.id)
    .bind(&project.id)
    .execute(store.pool())
    .await;
    assert!(both.is_err(), "a ticket's or a project's, not both");

    assert_eq!(store.holds_on_tool(Tool::ALL[0]).await.unwrap().len(), 4);
    assert!(store.holds_on_tool(Tool::ALL[1]).await.unwrap().is_empty());
    let unavailable = store.holds_for(HoldReason::ModelUnavailable).await.unwrap();
    assert_eq!(unavailable.len(), 1);
    assert_eq!(
        unavailable[0].ticket_id.as_deref(),
        Some(shop24.id.as_str())
    );
    assert_eq!(store.project_holds(&project.id).await.unwrap().len(), 1);

    assert!(
        store
            .clear_agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
    );
    assert!(
        !store
            .clear_agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
    );
    assert!(
        store
            .clear_agent_hold(HoldScope::Project(&project.id), SessionRole::ProjectAgent)
            .await
            .unwrap()
    );

    // An agent check's holds are per check agent (d0130 row 17): Use fallback asks one, a check agent's scope clears
    // one, the ticket's clears every one.
    let mut security = hold(
        Some(&shop31.id),
        None,
        SessionRole::Validator,
        HoldReason::ToolLimit,
    );
    security.check_agent = Some("Security reviewer".into());
    let mut validator = hold(
        Some(&shop31.id),
        None,
        SessionRole::Validator,
        HoldReason::ToolLimit,
    );
    validator.check_agent = Some("Validator".into());
    store.set_agent_hold(&security).await.unwrap();
    store.set_agent_hold(&validator).await.unwrap();
    assert!(store.ask_fallback(&security.id).await.unwrap());
    let checks: Vec<(Option<String>, bool)> = store
        .ticket_holds(&shop31.id)
        .await
        .unwrap()
        .into_iter()
        .filter(|h| h.role == SessionRole::Validator)
        .map(|h| (h.check_agent, h.fallback_asked))
        .collect();
    assert_eq!(
        checks,
        [
            (Some("Security reviewer".into()), true),
            (Some("Validator".into()), false)
        ]
    );
    let of_validator = HoldScope::Check {
        ticket: &shop31.id,
        agent: "Validator",
    };
    assert_eq!(
        store
            .agent_hold(of_validator, SessionRole::Validator)
            .await
            .unwrap()
            .map(|h| h.id),
        Some(validator.id.clone())
    );
    assert!(
        store
            .clear_agent_hold(of_validator, SessionRole::Validator)
            .await
            .unwrap()
    );
    store.set_agent_hold(&validator).await.unwrap();
    assert!(
        store
            .clear_agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
    );
    assert!(
        store
            .agent_hold(HoldScope::Ticket(&shop31.id), SessionRole::Validator)
            .await
            .unwrap()
            .is_none()
    );
    // A deleted ticket takes its holds with it.
    sqlx::query("DELETE FROM tickets WHERE id = ?")
        .bind(&shop24.id)
        .execute(store.pool())
        .await
        .unwrap();
    assert!(
        store
            .holds_for(HoldReason::ModelUnavailable)
            .await
            .unwrap()
            .is_empty()
    );
}

/// One request over the socket, as the app sends it.
async fn ask(client: &Client, method: &str, params: Value) -> Result<Reply, ClientError> {
    let request: Request = serde_json::from_value(json!({"method": method, "params": params}))
        .unwrap_or_else(|e| panic!("{method}: {e}"));
    client.request(request).await
}

/// The requests (AGM-04's page): list, add, edit, delete refused with the AGM-01 text, a role moved, the default model
/// and a fallback; each change publishes `agents_changed`; an agent's caller is refused every one of them.
#[tokio::test(flavor = "multi_thread")]
async fn the_requests_are_the_users() {
    let w = TestService::start().await;
    w.add_project("shop-web").await;
    let ticket = w.create("SHOP", "Price table").await;
    let key = ticket.key.to_string();
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    let mut events = client.subscribe().await.unwrap();

    let Reply::Agents(list) = client.request(Request::AgentsList).await.unwrap() else {
        panic!("agents.list")
    };
    assert_eq!(list.agents.len(), 4);
    let tool = Tool::ALL[0].as_str();
    let Reply::Agent(strong) = ask(
        &client,
        "agents.add",
        json!({"name": "Strong", "instructions": "Think hard.", "skills": [{"shipped": "work-ticket"}],
               "model": {"kind": "tool", "tool": tool}}),
    )
    .await
    .unwrap() else {
        panic!("agents.add")
    };
    assert_eq!(strong.instructions, "Think hard.");
    let changed = loop {
        let event = events.recv().await.unwrap();
        if event == Event::AgentsChanged {
            break event;
        }
    };
    assert_eq!(changed, Event::AgentsChanged);
    let developer = list.agents.iter().find(|a| a.name == "Developer").unwrap();
    let Err(ClientError::Service(refused)) =
        ask(&client, "agents.delete", json!({"id": developer.id})).await
    else {
        panic!("refused")
    };
    assert_eq!(refused.data.as_ref().unwrap()["refusal"], "agent_in_use");
    assert_eq!(
        refused.message,
        "Developer is used in shop-web \u{b7} Dev and shop-web \u{b7} Done. Change those stages first."
    );
    let Reply::Agent(holder) = ask(
        &client,
        "agents.move_role",
        json!({"role": "epic-lead", "agent": strong.id}),
    )
    .await
    .unwrap() else {
        panic!("agents.move_role")
    };
    assert_eq!(holder.role, Some(Role::EpicLead));
    let Reply::Agents(list) = ask(
        &client,
        "agents.default_model",
        json!({"model": {"tool": tool}}),
    )
    .await
    .unwrap() else {
        panic!("agents.default_model")
    };
    assert_eq!(
        list.default_model,
        Some(ModelRef {
            tool: Tool::ALL[0],
            model: None
        })
    );
    let other = Tool::ALL[1].as_str();
    let Reply::Agents(list) = ask(
        &client,
        "agents.fallback",
        json!({"tool": tool, "fallback": {"tool": other}}),
    )
    .await
    .unwrap() else {
        panic!("agents.fallback")
    };
    assert_eq!(list.fallbacks.len(), 1);

    // A ticket's agent is refused every one.
    let agent = w.agent_shell(&ticket).await;
    for (method, params) in [
        ("agents.list", Value::Null),
        (
            "agents.add",
            json!({"name": "X", "model": {"kind": "default"}}),
        ),
        ("agents.delete", json!({"id": strong.id})),
        (
            "agents.move_role",
            json!({"role": "epic-lead", "agent": strong.id}),
        ),
        ("agents.default_model", json!({"model": {"tool": tool}})),
        ("agents.fallback", json!({"tool": tool})),
        (
            "agents.import_skill",
            json!({"id": strong.id, "folder": "/tmp/x"}),
        ),
        ("limits.use_fallback_now", json!({"tool": tool})),
        ("ticket.use_fallback", json!({"key": key})),
    ] {
        let mut request = json!({"method": method});
        if !params.is_null() {
            request["params"] = params;
        }
        let line = json!({"protocol_version": 1, "id": 1, "request": request}).to_string();
        let ran = agent.send_raw(&w.data.socket_path(), &line).await;
        let answer: Value = serde_json::from_str(ran.stdout.lines().next().unwrap_or_default())
            .unwrap_or_else(|e| panic!("{e}: {ran:?}"));
        assert_eq!(
            answer["error"]["data"]["refusal"], "not_allowed",
            "{method}: {answer}"
        );
    }
    w.stop().await;
}
