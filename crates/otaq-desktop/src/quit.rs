//! **Quitting the app** (T-0569; TRY-03, TRY-07; d0139): the steps the door runs for Stop agents and quit and for
//! ⌘Q's question, without Tauri. The texts are [`otaq_view::view_models::quit`]'s.
//!
//! **Stop agents and quit** sends `service.stop_agents`: the service stores the All projects pause and answers, then
//! ends every agent, Run, Shell and Project terminal and stops itself. The app waits until the service has exited (its
//! lock is free), at most [`STOP_WAIT_SECS`], and only then quits. If the service can't be reached, refuses, or hasn't
//! exited in time, the app stays and says why ([`StopFailure`]).

use std::time::Duration;

use otaq_core::protocol::{Reply, Request, SessionListParams};
use otaq_core::service::lock::service_alive;
use otaq_view::view_models::quit::{STOP_WAIT_SECS, StopFailure};

use crate::App;

/// How long the app waits for `service.stop_agents`' answer (the pause stored).
const ANSWER_WAIT: Duration = Duration::from_secs(10);
/// How often the app looks whether the service has exited while it waits.
const EXIT_CHECK: Duration = Duration::from_millis(100);

/// The agents working now, as the service counts them; `None` while it can't be asked (⌘Q then quits at once).
pub async fn agents_working(app: &App) -> Option<u32> {
    match tokio::time::timeout(ANSWER_WAIT, app.service.send(Request::ServiceStatus)).await {
        Ok(Ok(Reply::ServiceStatus(status))) => Some(status.agents_working),
        _ => None,
    }
}

/// Stop agents and quit's service part: `Ok` once the service has stopped every agent and exited; the caller then
/// quits the app.
pub async fn stop_agents(app: &App) -> Result<(), StopFailure> {
    let Some(data) = app.service.data() else {
        return Err(StopFailure::Unreachable);
    };
    match tokio::time::timeout(ANSWER_WAIT, app.service.send(Request::ServiceStopAgents)).await {
        Ok(Ok(Reply::Ok)) => {}
        _ => return Err(StopFailure::Unreachable),
    }
    let lock = data.service_lock_path();
    let exited = tokio::time::timeout(Duration::from_secs(STOP_WAIT_SECS), async {
        while service_alive(&lock) {
            tokio::time::sleep(EXIT_CHECK).await;
        }
    })
    .await;
    if exited.is_ok() {
        return Ok(());
    }
    Err(StopFailure::StillRunning {
        still: still_running(app).await,
    })
}

/// The ticket whose agent still runs, with its tool ("SHOP-18", "Codex"), if the service can still say.
async fn still_running(app: &App) -> Option<(String, String)> {
    let request = Request::SessionList(SessionListParams { key: None });
    let Ok(Ok(Reply::Sessions(sessions))) =
        tokio::time::timeout(ANSWER_WAIT, app.service.send(request)).await
    else {
        return None;
    };
    sessions.into_iter().find_map(|s| {
        let running = s.is_running() && s.kind.runs_agent_tool();
        match (running, s.ticket, s.tool) {
            (true, Some(key), Some(tool)) => {
                Some((key.to_string(), tool.display_name().to_owned()))
            }
            _ => None,
        }
    })
}

#[cfg(test)]
mod tests {
    use otaq_core::agents::pause::{PauseKind, PauseScope};
    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;

    use super::*;
    use crate::service::testing::{Harness, connected};

    async fn add(h: &Harness, repos: &TempRepos, name: &str) {
        let path = repos.repo(name, 1);
        let reply = h
            .app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: path.display().to_string(),
                key: None,
                base_branch: None,
                flow_setup: false,
            }))
            .await
            .unwrap();
        assert!(matches!(reply, Reply::ProjectAdded { .. }), "{reply:?}");
    }

    /// TRY-07 against a real (in-process) service: Stop agents and quit stores the pause, the service exits, and only
    /// then does it answer Ok; the next service start finds every project paused.
    #[tokio::test(flavor = "multi_thread")]
    async fn stop_agents_waits_for_the_service_to_exit() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let repos = TempRepos::new();
        add(&h, &repos, "shop-web").await;
        assert_eq!(agents_working(&h.app).await, Some(0));

        stop_agents(&h.app).await.unwrap();
        let data = h.app.service.data().unwrap();
        assert!(!service_alive(&data.service_lock_path()), "exited");
        assert_eq!(agents_working(&h.app).await, None, "nothing to ask");

        h.service.bind().await.unwrap();
        h.wait_for("connected again", connected).await;
        let Reply::PauseCounts(counts) = h
            .app
            .service
            .send(Request::AgentsPauseCounts(PauseScope::All))
            .await
            .unwrap()
        else {
            panic!("counts")
        };
        assert_eq!(
            counts
                .projects
                .iter()
                .map(|p| (p.project.as_str(), p.paused_by))
                .collect::<Vec<_>>(),
            [("SHOP", Some(PauseKind::All))]
        );
        h.service.stop_all().await;
    }

    /// With the service gone first, Stop agents and quit fails and says so; the app keeps running.
    #[tokio::test(flavor = "multi_thread")]
    async fn without_the_service_it_fails() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        h.service.stop_all().await;
        h.wait_for("gone", |c| !connected(c)).await;
        assert_eq!(stop_agents(&h.app).await, Err(StopFailure::Unreachable));
    }
}
