//! Running processes (RUN-04, SHL-03; d0035, d0087; *13 · Run & Shell → All running processes*, *02 → Row · Header
//! parts: Activity, rail (0086 · V)*): Activity's "▷ 3" across every project, shown only while something runs ("6
//! running" stays the agents'), and the popover's PROCESSES with one row per Run (project, key, title, port, uptime)
//! to open its ticket's Run tab or stop it.

use otaq_core::domain::TicketKey;
use otaq_core::terminals::run::RunInfo;

/// Activity's "▷ 3": the live Runs, `None` (hidden) at 0.
pub fn count(runs: &[RunInfo]) -> Option<u32> {
    let live = runs.iter().filter(|r| r.state.is_live()).count();
    (live > 0).then(|| u32::try_from(live).unwrap_or(u32::MAX))
}

/// One row of PROCESSES.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRow {
    pub project: String,
    pub project_name: String,
    pub key: TicketKey,
    pub title: String,
    /// ":4020".
    pub port: Option<String>,
    /// "12 min", "2 h 5 min".
    pub uptime: Option<String>,
    /// The substituted Open URL, when the project has one.
    pub url: Option<String>,
}

/// PROCESSES' rows: grouped by project (by name), then by key.
pub fn rows(runs: &[RunInfo], now_ms: i64) -> Vec<ProcessRow> {
    let mut rows: Vec<ProcessRow> = runs
        .iter()
        .filter(|r| r.state.is_live())
        .map(|r| ProcessRow {
            project: r.project.clone(),
            project_name: r.project_name.clone(),
            key: r.key.clone(),
            title: r.title.clone(),
            // A named app says which (d{RUN}): "api :4021"; a project's one app stays ":4020".
            port: r.port.map(|p| {
                if r.app == otaq_core::board::settings::DEFAULT_APP {
                    format!(":{p}")
                } else {
                    format!("{} :{p}", r.app)
                }
            }),
            uptime: r.started_at.map(|at| super::clock::duration(now_ms - at)),
            url: r.url(),
        })
        .collect();
    rows.sort_by(|a, b| a.project_name.cmp(&b.project_name).then(a.key.cmp(&b.key)));
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::terminals::run::RunState;

    fn run(project: &str, name: &str, key: &str, port: u16, url: Option<&str>) -> RunInfo {
        RunInfo {
            project: project.into(),
            project_name: name.into(),
            key: key.parse().unwrap(),
            title: format!("{key} title"),
            app: otaq_core::board::settings::DEFAULT_APP.into(),
            state: RunState::Running,
            port: Some(port),
            command: "x".into(),
            open_url: url.map(str::to_owned),
            session: None,
            started_at: Some(0),
            ended_at: None,
            exit_code: None,
            stopped_by: None,
            started_by: None,
            command_set_by: None,
        }
    }

    #[test]
    fn counts_and_grouping() {
        assert_eq!(count(&[]), None);
        let runs = vec![
            run("BILL", "billing-api", "BILL-4", 4004, None),
            run(
                "SHOP",
                "shop-web",
                "SHOP-22",
                4024,
                Some("http://localhost:$PORT"),
            ),
            run(
                "SHOP",
                "shop-web",
                "SHOP-19",
                4020,
                Some("http://localhost:$PORT"),
            ),
        ];
        assert_eq!(count(&runs[..1]), Some(1));
        assert_eq!(count(&runs), Some(3));
        let rows = rows(&runs, 12 * 60_000);
        let keys: Vec<String> = rows.iter().map(|r| r.key.to_string()).collect();
        assert_eq!(keys, ["BILL-4", "SHOP-19", "SHOP-22"]);
        assert_eq!(rows[0].url, None, "billing-api has no Open URL");
        assert_eq!(rows[1].url.as_deref(), Some("http://localhost:4020"));
        assert_eq!(rows[1].port.as_deref(), Some(":4020"));
        // A named app (d{RUN}).
        let mut api = run("SHOP", "shop-web", "SHOP-2", 4021, None);
        api.app = "api".into();
        assert_eq!(
            super::rows(&[api], 12 * 60_000)[0].port.as_deref(),
            Some("api :4021")
        );
        assert_eq!(rows[1].uptime.as_deref(), Some("12 min"));
    }

    #[test]
    fn stopped_runs_are_left_out() {
        let mut stopped = run("SHOP", "shop-web", "SHOP-19", 4020, None);
        stopped.state = RunState::Stopped;
        assert!(rows(std::slice::from_ref(&stopped), 0).is_empty());
        assert_eq!(count(&[stopped]), None, "nothing live: ▷ hidden");
    }
}
