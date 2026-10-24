//! Every request gets an answer over the socket (a reply or an error), and the connection keeps answering after
//! it. A request the service doesn't route used to end the connection's reader while the socket stayed open, so the
//! app waited forever on everything it asked next (T-0167's Project terminal summary; the drawer stayed black).
//!
//! The samples carry made-up keys, so most requests are refused with an error: that is an answer too. Nothing here
//! runs `claude`, `gh` or the user's shell (no tools on PATH, no launch context, `/bin/sh` for terminals).

use std::time::Duration;

use otaq_core::client::{Client, ClientConfig, ClientError};
use otaq_core::protocol::{ErrorKind, Request};
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

/// The parameters every sample starts from: made-up keys and the fields the requests require.
fn base_params() -> Value {
    json!({
        "path": "/nonexistent", "project": "NOPE", "key": "NOPE-1", "child": "NOPE-1", "parent": "NOPE-2",
        "epic": "NOPE-2", "id": "nope", "session": "nope", "items": [], "tickets": [], "event": "Stop",
        "title": "t", "target": "backlog", "choice": {"other": "x"}, "file": "a", "line": 1, "side": "new",
        "body": "b", "message": "m", "question": "q", "answer": "a", "proposal": "p", "name": "n",
        "url": "https://example.invalid/pr/1", "settings": {}, "cols": 80, "rows": 24, "data": "",
        "focused": false, "pass": false, "action": "stop",
        "note": "Not covered by any ticket: x", "query": "q", "ticket": "NOPE-1"
    })
}

/// One request of the kind sent on the wire as `method`.
fn sample(method: &str, unit: bool) -> Request {
    let mut message = json!({ "method": method });
    if !unit {
        let mut params = base_params();
        if method == "ticket.pr_confirm" {
            params["proposal"] = json!({"branch": "b", "commits": []});
        }
        if method == "lead.suggest" {
            params["kind"] = json!("close");
            params["reason"] = json!("r");
        }
        if method == "lead.pass_on" {
            params["reason"] = json!("r");
        }
        if method == "lead.note" {
            params["text"] = json!("t");
        }
        if method == "flow.apply" {
            params["proposal"] = json!(3);
        }
        if method == "flow.propose_from" {
            params["source"] = json!({"from": "previous"});
        }
        if method == "flow.propose" {
            params["source"] = json!({"from": "project", "project": "NOPE"});
        }
        if method == "project_view.set" {
            params["view"] = json!({});
        }
        if method == "flow.write_skill" {
            params["stage"] = json!("plan");
        }
        if method == "flow.draft_update" {
            params["edit"] = json!({"edit": "undo_all"});
        }
        if method == "flow.apply_now" {
            params["stamp"] = json!("s");
        }
        if method == "flow.ask_for_changes" {
            params["proposal"] = json!(3);
            params["text"] = json!("t");
        }
        if method == "hook" {
            params["tool"] = json!("claude");
        }
        if method == "usage.record_limits" {
            params["tool"] = json!("claude");
            params["windows"] = json!([]);
        }
        if method.starts_with("agents.models") {
            params["tool"] = json!("claude");
        }
        if method == "agents.add" || method == "agents.edit" {
            params["model"] = json!({"kind": "default"});
        }
        if method == "agents.move_role" {
            params["role"] = json!("epic-lead");
            params["agent"] = json!("nope");
        }
        if method == "agents.default_model" {
            params["model"] = json!({"tool": "claude"});
        }
        if method == "agents.import_skill" {
            params["folder"] = json!("/nonexistent");
        }
        if method == "agents.reimport_skill" {
            params["skill"] = json!("nope");
        }
        if method == "agents.reads" {
            params["agent"] = json!("nope");
        }
        // The made-up project: refused, so the sample never pauses the test's own project.
        if method.starts_with("agents.pause") || method == "agents.resume" {
            params["scope"] = json!("project");
        }
        if method.starts_with("agent_tools.")
            || method.starts_with("limits.")
            || method == "agents.fallback"
            || method == "onboarding.ask"
        {
            params["tool"] = json!("codex");
        }
        // git has no login: refused, so the sample never runs an install or login command.
        if method == "tools.terminal" {
            params["tool"] = json!("git");
            params["action"] = json!("login");
        }
        if method == "agent_tools.set_hidden" {
            params["hidden"] = json!(true);
        }
        if method == "ticket.set_choice" {
            params["stage"] = json!("Dev");
            params["choice"] = json!("skip");
        }
        if method.starts_with("ticket.comment_") || method == "ticket.report" {
            params["n"] = json!(1);
        }
        if method == "decision.mark" {
            params["source"] = json!("PAY-1");
            params["text"] = json!("t");
        }
        message["params"] = params;
    }
    serde_json::from_value(message).unwrap_or_else(|e| panic!("a sample of {method}: {e}"))
}

/// Every method of [`Request::method`], read from its source, with whether the request has no parameters.
fn wire_methods() -> Vec<(String, bool)> {
    let source = include_str!("../../src/protocol.rs");
    let start = source
        .find("pub fn method(&self) -> &'static str {")
        .expect("Request::method");
    let body = &source[start..];
    let body = &body[..body.find("\n    }\n").expect("the end of Request::method")];
    body.lines()
        .filter_map(|line| {
            let (pattern, name) = line.trim().split_once(" => \"")?;
            Some((
                name.trim_end_matches("\",").to_owned(),
                !pattern.ends_with("(_)"),
            ))
        })
        .collect()
}

/// Whether the request ends the service, so it goes last. No wildcard: a new kind of request doesn't compile until
/// it is listed here, and `wire_methods` then makes this test send it.
fn ends_the_service(request: &Request) -> bool {
    match request {
        Request::ServiceStop => true,
        Request::Ping
        | Request::ServiceStatus
        | Request::StartupReport
        | Request::Subscribe
        | Request::ToolsCheck
        | Request::OnboardingState
        | Request::OnboardingSources(_)
        | Request::OnboardingAsk(_)
        | Request::OnboardingFlowSetup(_)
        | Request::OnboardingUseDefault(_)
        | Request::DismissPermissionWarning
        | Request::DismissTrustNote
        | Request::NotificationsState
        | Request::NotificationsAskAnswered
        | Request::NotificationsDismissBlocked
        | Request::ProjectList
        | Request::ProjectProbe(_)
        | Request::ProjectAdd(_)
        | Request::ProjectUpdate(_)
        | Request::ProjectRemove(_)
        | Request::ProjectBranches(_)
        | Request::ProjectPrProvider(_)
        | Request::BoardSnapshot(_)
        | Request::TicketGet(_)
        | Request::TicketList(_)
        | Request::TicketCreate(_)
        | Request::TicketCreateBatch(_)
        | Request::TicketUpdate(_)
        | Request::TicketMove(_)
        | Request::TicketDrag(_)
        | Request::TicketPin(_)
        | Request::TicketUnpin(_)
        | Request::TicketAgentEdits(_)
        | Request::TicketAgentEditsSeen(_)
        | Request::TicketDeletePreview(_)
        | Request::TicketDelete(_)
        | Request::TicketRetryStart(_)
        | Request::TicketHistory(_)
        | Request::TicketClose(_)
        | Request::TicketReopen(_)
        | Request::TicketSubmitByUser(_)
        | Request::TicketPark(_)
        | Request::TicketResume(_)
        | Request::TicketRetryResume(_)
        | Request::TicketStartFromScratch(_)
        | Request::TicketsResumeAll(_)
        | Request::InboxAnswer(_)
        | Request::InboxList
        | Request::InboxCounts
        | Request::CleanupScan
        | Request::CleanupDelete(_)
        | Request::WarmList
        | Request::WarmClear(_)
        | Request::DiffFiles(_)
        | Request::DiffFile(_)
        | Request::FilesTree(_)
        | Request::FilesSearch(_)
        | Request::FilesRead(_)
        | Request::FilesDiff(_)
        | Request::FilesChanges(_)
        | Request::FilesProjectTree(_)
        | Request::FilesProjectSearch(_)
        | Request::FilesProjectRead(_)
        | Request::FilesProjectDiff(_)
        | Request::FilesProjectChanges(_)
        | Request::FilesProjectSources(_)
        | Request::ReviewCommentAdd(_)
        | Request::ReviewCommentReply(_)
        | Request::ReviewCommentEdit(_)
        | Request::ReviewCommentDelete(_)
        | Request::ReviewComments(_)
        | Request::TicketCommentPost(_)
        | Request::TicketCommentEdit(_)
        | Request::TicketCommentDelete(_)
        | Request::TicketComments(_)
        | Request::TicketReport(_)
        | Request::TicketFail(_)
        | Request::TicketPass(_)
        | Request::ReviewFeedback(_)
        | Request::TicketMergeProposal(_)
        | Request::TicketLocalMerge(_)
        | Request::TicketMergeCheckStop(_)
        | Request::TicketApproveCheck(_)
        | Request::TicketConflict(_)
        | Request::TicketSendBackPr(_)
        | Request::TicketAddChildFromPr(_)
        | Request::TicketPrime(_)
        | Request::TicketAskHuman(_)
        | Request::DecisionRecord(_)
        | Request::DecisionList(_)
        | Request::DecisionChange(_)
        | Request::DecisionMark(_)
        | Request::LeadSuggest(_)
        | Request::LeadPassOn(_)
        | Request::LeadQuestions
        | Request::LeadPrime(_)
        | Request::SuggestionConfirm(_)
        | Request::SuggestionDismiss(_)
        | Request::LeadNote(_)
        | Request::TicketNotes(_)
        | Request::TicketStageNext(_)
        | Request::TicketStageDone(_)
        | Request::ValidationVerdict(_)
        | Request::TicketPrimeValidate(_)
        | Request::TicketValidationAction(_)
        | Request::TicketValidations(_)
        | Request::TicketCreateFromNote(_)
        | Request::TicketPrPropose(_)
        | Request::TicketPrOpened(_)
        | Request::TicketBranchRename(_)
        | Request::TicketAsk(_)
        | Request::MessageAnswer(_)
        | Request::MessageResume(_)
        | Request::TicketQuestions(_)
        | Request::TicketMessages(_)
        | Request::TicketLeadLog(_)
        | Request::TicketTerminal(_)
        | Request::TicketHumanAnswers(_)
        | Request::TicketOpenPr(_)
        | Request::TicketPrConfirm(_)
        | Request::TicketPrCancel(_)
        | Request::TicketPrRetry(_)
        | Request::TicketPrFlow(_)
        | Request::TicketPrBranchCheck(_)
        | Request::TicketMarkMerged(_)
        | Request::TicketMarkClosed(_)
        | Request::EpicMoveTo(_)
        | Request::EpicClosePlan(_)
        | Request::EpicClose(_)
        | Request::EpicQueue(_)
        | Request::EpicTakeBack(_)
        | Request::EpicUndoQueue(_)
        | Request::DepAdd(_)
        | Request::DepRemove(_)
        | Request::DepTree(_)
        | Request::DepCycles(_)
        | Request::DepReadiness(_)
        | Request::SchedulerStatus
        | Request::UsageSummary(_)
        | Request::UsageSessions(_)
        | Request::UsageActivity
        | Request::UsageRecordLimits(_)
        | Request::AgentsModels(_)
        | Request::AgentsModelsRefresh(_)
        | Request::AgentsModelsRetry(_)
        | Request::UsageMarkLimit(_)
        | Request::SettingsGet
        | Request::SettingsSet(_)
        | Request::ProjectSettingsGet(_)
        | Request::ProjectSettingsSet(_)
        | Request::ProjectViewSet(_)
        | Request::FlowCurrent(_)
        | Request::FlowVersions(_)
        | Request::FlowProposal(_)
        | Request::FlowApply(_)
        | Request::FlowCancelProposal(_)
        | Request::FlowWaiting(_)
        | Request::FlowCancelWaiting(_)
        | Request::FlowApplyNowPlan(_)
        | Request::FlowApplyNow(_)
        | Request::FlowProposeFrom(_)
        | Request::FlowShow(_)
        | Request::FlowPropose(_)
        | Request::FlowAskForChanges(_)
        | Request::FlowDraft(_)
        | Request::FlowDraftUpdate(_)
        | Request::FlowWriteSkill(_)
        | Request::SessionList(_)
        | Request::SessionAttach(_)
        | Request::SessionDetach(_)
        | Request::SessionResize(_)
        | Request::SessionInput(_)
        | Request::SessionDeliverNow(_)
        | Request::SessionFocus(_)
        | Request::AppFocus(_)
        | Request::SessionBeforeRestart(_)
        | Request::Hook(_)
        | Request::RunStart(_)
        | Request::RunStop(_)
        | Request::RunStatus(_)
        | Request::RunList
        | Request::RunSuggest(_)
        | Request::RunSuggestCancel(_)
        | Request::ShellOpen(_)
        | Request::ToolTerminalOpen(_)
        | Request::ProjectTerminalOpen(_)
        | Request::ProjectTerminalList(_)
        | Request::ProjectTerminalClose(_)
        | Request::ProjectTerminalCreated(_)
        | Request::ProjectTerminalDismiss(_)
        | Request::ProjectAgentOpen(_)
        | Request::ProjectAgentList(_)
        | Request::ProjectAgentClose(_)
        | Request::ProjectAgentCloseEnded(_)
        | Request::ProjectAgentResume(_)
        | Request::ProjectAgentRename(_)
        | Request::AgentName(_)
        | Request::AgentAsk(_)
        | Request::AgentAskHuman(_)
        | Request::ProjectAgentAnswer(_)
        | Request::ProjectAgentSend(_)
        | Request::ProjectAgentPrime(_)
        | Request::ProjectAgentCreated(_)
        | Request::LeadOpen(_)
        | Request::LeadResume(_)
        | Request::LeadList(_)
        | Request::AgentsList
        | Request::AgentsAdd(_)
        | Request::AgentsEdit(_)
        | Request::AgentsDuplicate(_)
        | Request::AgentsDelete(_)
        | Request::AgentsInUse(_)
        | Request::AgentsMoveRole(_)
        | Request::AgentsDefaultModel(_)
        | Request::AgentsFallback(_)
        | Request::AgentsImportSkill(_)
        | Request::AgentsReimportSkill(_)
        | Request::AgentsReads(_)
        | Request::AgentsPause(_)
        | Request::AgentsResume(_)
        | Request::AgentsPauseCounts(_)
        | Request::TicketAgents(_)
        | Request::BoardAgents(_)
        | Request::TicketChoices(_)
        | Request::TicketSetChoice(_)
        | Request::AgentToolsList
        | Request::AgentToolsRecheck(_)
        | Request::AgentToolsSetHidden(_)
        | Request::AgentToolsSignIn(_)
        | Request::AgentToolsInstall(_)
        | Request::AgentToolsDisconnect(_)
        | Request::LimitsResumeTool(_)
        | Request::LimitsUseFallbackNow(_)
        | Request::TicketUseFallback(_)
        | Request::RecentItems(_) => false,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn every_request_is_answered_over_the_socket() {
    let empty = tempfile::tempdir().unwrap();
    let w = TestService::start_with(|mut c| {
        c.tools.path = empty.path().as_os_str().to_owned();
        c.tools.claude_config_dir = Some(empty.path().to_path_buf());
        c.tools.managed_settings_dir = empty.path().to_path_buf();
        c.terminals.shell = "/bin/sh".into();
        c.terminals.shell_args = Vec::new();
        c
    })
    .await;
    w.add_project("shop-web").await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    // Like the app: subscribed to events, which keeps the connection open whatever happens to its reader.
    let _events = client.subscribe().await.unwrap();

    let methods = wire_methods();
    assert!(methods.len() > 100, "read {} methods", methods.len());
    let mut requests: Vec<Request> = methods
        .iter()
        .map(|(method, unit)| sample(method, *unit))
        .collect();
    for (request, (method, _)) in requests.iter().zip(&methods) {
        assert_eq!(request.method(), method);
    }
    requests.sort_by_key(ends_the_service);

    for request in requests {
        let method = request.method();
        let answer = tokio::time::timeout(Duration::from_secs(20), client.request(request))
            .await
            .unwrap_or_else(|_| panic!("{method} got no answer"));
        match answer {
            // Refusals and "not found" are answers; an internal error means the request wasn't handled.
            Ok(_) => {}
            Err(ClientError::Service(e)) if e.kind != ErrorKind::Internal => {}
            Err(e) => panic!("{method}: {e}"),
        }
    }
}
