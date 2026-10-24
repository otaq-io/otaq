//! The Run tab, the card's `▶ :4020` chip and the sheet header's running pill (RUN-01, RUN-03; *13 · Run & Shell*):
//! what each Run state shows and which buttons it offers. Open appears only when the project sets an Open URL, with
//! `$PORT` replaced by the Run port, and is enabled once the port answers.

use otaq_core::terminals::run::{RunInfo, RunState, StopCause};

use super::menu::MenuItem;
use crate::icons::IconName;

/// The colour of the state card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    /// Starting: the accent.
    Progress,
    /// Running: green.
    Success,
    /// Failed to start, port in use: red.
    Danger,
}

/// A button on the state card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunButton {
    Run,
    Stop,
    /// Opens the substituted URL in the system browser; disabled until the port answers.
    Open {
        url: String,
        label: String,
        enabled: bool,
    },
    /// Fix the run command (RUN-02's dialog).
    EditCommand,
    Retry,
    /// Copies `lsof -i :4020`.
    CopyLsof,
}

/// The state card above the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCard {
    pub title: String,
    pub tone: Tone,
    /// ":4020".
    pub port: Option<String>,
    /// The command as configured (`pnpm dev --port $PORT`); `None` when there is none yet.
    pub command: Option<String>,
    pub message: Option<String>,
    pub buttons: Vec<RunButton>,
}

/// The state card for `run`. `hhmm` formats a time of day ("11:48").
pub fn card(run: &RunInfo, hhmm: impl Fn(i64) -> String) -> RunCard {
    let port = run.port.map(|p| p.to_string()).unwrap_or_default();
    let open = |enabled: bool| {
        run.url().map(|url| RunButton::Open {
            label: open_label(&url),
            url,
            enabled,
        })
    };
    let ended = run.ended_at.map(&hhmm).unwrap_or_default();
    let (title, tone, message, buttons) = match run.state {
        RunState::NotStarted => (
            "Not running".to_owned(),
            Tone::Neutral,
            Some(if run.command.is_empty() {
                "No run command yet. Run asks for the command that starts this project.".to_owned()
            } else {
                format!("Runs the project’s run command in this ticket’s worktree on port {port}.")
            }),
            vec![RunButton::Run],
        ),
        RunState::Starting => (
            "Starting…".to_owned(),
            Tone::Progress,
            None,
            open(false).into_iter().chain([RunButton::Stop]).collect(),
        ),
        RunState::Running => (
            "Serving".to_owned(),
            Tone::Success,
            None,
            open(true).into_iter().chain([RunButton::Stop]).collect(),
        ),
        RunState::Stopped => {
            let title = match (run.stopped_by, run.exit_code) {
                (Some(StopCause::You), _) => format!("Stopped by you at {ended}"),
                (Some(StopCause::Finished { .. }), _) => {
                    format!("Stopped at {ended} · {} is finished", run.key)
                }
                (Some(StopCause::Parked), _) => {
                    format!("Stopped at {ended} · {} was parked", run.key)
                }
                (_, Some(code)) => format!("Stopped · exited {code} at {ended}"),
                _ => format!("Stopped at {ended}"),
            };
            (title, Tone::Neutral, None, vec![RunButton::Run])
        }
        RunState::FailedToStart => {
            let program = run
                .command
                .split_whitespace()
                .next()
                .unwrap_or("The command");
            let after = match (run.started_at, run.ended_at) {
                (Some(start), Some(end)) => format!(" after {}s", ((end - start) / 1000).max(0)),
                _ => String::new(),
            };
            let exit = run
                .exit_code
                .map_or_else(String::new, |code| format!(" · exit {code}"));
            (
                format!("Failed to start{exit}"),
                Tone::Danger,
                Some(format!(
                    "{program} exited{after}. The command may be wrong for this project, or setup didn’t finish."
                )),
                vec![RunButton::EditCommand, RunButton::Retry],
            )
        }
        RunState::PortInUse => (
            format!("Port {port} is in use"),
            Tone::Danger,
            Some(format!(
                "Another process is listening on {port}. Stop it and retry. To see what it is: {} (macOS/Linux) \
                 or netstat -ano | findstr {port} (Windows).",
                lsof(&port)
            )),
            vec![RunButton::Retry, RunButton::CopyLsof],
        ),
        RunState::StoppedByRestart => (
            match run.started_at {
                Some(at) => format!("Stopped by restart · was running at {}", hhmm(at)),
                None => "Stopped by restart".to_owned(),
            },
            Tone::Neutral,
            None,
            vec![RunButton::Run],
        ),
    };
    RunCard {
        title,
        tone,
        port: run.port.map(|p| format!(":{p}")),
        command: Some(run.command.clone()).filter(|c| !c.is_empty()),
        message,
        buttons,
    }
}

/// `lsof -i :4020`, the command that names the program holding a port.
pub fn lsof(port: &str) -> String {
    format!("lsof -i :{port}")
}

/// "Open localhost:4020" for `http://localhost:4020/`: the URL without its scheme and trailing slash.
pub fn open_label(url: &str) -> String {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("https://"))
        .unwrap_or(url);
    format!("Open {}", rest.trim_end_matches('/'))
}

/// The card chip's menu ids.
pub const MENU_OPEN: &str = "run-open";
pub const MENU_STOP: &str = "run-stop";

/// The menu of the card's `▶ :4020` chip: Open (only with an Open URL) and Stop.
pub fn chip_menu(run: &RunInfo) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if run.state.is_live()
        && let (Some(_), Some(port)) = (run.url(), run.port)
    {
        items.push(MenuItem::new(MENU_OPEN, format!("Open :{port}")).icon(IconName::ExternalLink));
    }
    if run.state.is_live() {
        items.push(
            MenuItem::new(MENU_STOP, "Stop")
                .icon(IconName::Square)
                .danger(),
        );
    }
    items
}

/// The sheet header's running pill: the port and, when the project has one, the Open URL.
pub fn header_pill(run: Option<&RunInfo>) -> Option<(u16, Option<String>)> {
    let run = run.filter(|r| r.state.is_live())?;
    Some((run.port?, run.url()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::domain::Status;

    fn run(state: RunState) -> RunInfo {
        RunInfo {
            project: "SHOP".into(),
            project_name: "shop-web".into(),
            key: "SHOP-19".parse().unwrap(),
            title: "Price table".into(),
            state,
            port: Some(4020),
            command: "pnpm dev --port $PORT".into(),
            open_url: Some("http://localhost:$PORT".into()),
            session: None,
            started_at: Some(1_000),
            ended_at: Some(4_000),
            exit_code: None,
            stopped_by: None,
        }
    }

    fn hhmm(_: i64) -> String {
        "11:48".into()
    }

    fn open(url: &str, enabled: bool) -> RunButton {
        RunButton::Open {
            url: url.into(),
            label: open_label(url),
            enabled,
        }
    }

    #[test]
    fn state_to_controls() {
        let c = card(&run(RunState::NotStarted), hhmm);
        assert_eq!(c.title, "Not running");
        assert_eq!(c.buttons, [RunButton::Run]);
        assert_eq!(c.port.as_deref(), Some(":4020"));
        assert_eq!(c.command.as_deref(), Some("pnpm dev --port $PORT"));
        assert_eq!(
            c.message.as_deref(),
            Some("Runs the project’s run command in this ticket’s worktree on port 4020.")
        );

        let c = card(&run(RunState::Starting), hhmm);
        assert_eq!((c.title.as_str(), c.tone), ("Starting…", Tone::Progress));
        assert_eq!(
            c.buttons,
            [open("http://localhost:4020", false), RunButton::Stop]
        );

        let c = card(&run(RunState::Running), hhmm);
        assert_eq!((c.title.as_str(), c.tone), ("Serving", Tone::Success));
        assert_eq!(
            c.buttons,
            [open("http://localhost:4020", true), RunButton::Stop]
        );

        let mut stopped = run(RunState::Stopped);
        stopped.exit_code = Some(0);
        assert_eq!(card(&stopped, hhmm).title, "Stopped · exited 0 at 11:48");
        assert_eq!(card(&stopped, hhmm).buttons, [RunButton::Run]);
        stopped.stopped_by = Some(StopCause::You);
        assert_eq!(card(&stopped, hhmm).title, "Stopped by you at 11:48");
        stopped.stopped_by = Some(StopCause::Finished {
            status: Status::Merged,
        });
        assert_eq!(
            card(&stopped, hhmm).title,
            "Stopped at 11:48 · SHOP-19 is finished"
        );
        stopped.stopped_by = Some(StopCause::Parked);
        assert_eq!(
            card(&stopped, hhmm).title,
            "Stopped at 11:48 · SHOP-19 was parked"
        );

        let mut failed = run(RunState::FailedToStart);
        failed.exit_code = Some(1);
        let c = card(&failed, hhmm);
        assert_eq!(
            (c.title.as_str(), c.tone),
            ("Failed to start · exit 1", Tone::Danger)
        );
        assert_eq!(
            c.message.as_deref(),
            Some(
                "pnpm exited after 3s. The command may be wrong for this project, or setup didn’t finish."
            )
        );
        assert_eq!(c.buttons, [RunButton::EditCommand, RunButton::Retry]);

        let c = card(&run(RunState::PortInUse), hhmm);
        assert_eq!(c.title, "Port 4020 is in use");
        assert!(c.message.unwrap().contains("lsof -i :4020 (macOS/Linux)"));
        assert_eq!(c.buttons, [RunButton::Retry, RunButton::CopyLsof]);

        let c = card(&run(RunState::StoppedByRestart), hhmm);
        assert_eq!(c.title, "Stopped by restart · was running at 11:48");
        assert_eq!(c.buttons, [RunButton::Run]);
    }

    #[test]
    fn no_open_url_no_open_button() {
        let mut backend = run(RunState::Running);
        backend.open_url = None;
        assert_eq!(card(&backend, hhmm).buttons, [RunButton::Stop]);
        assert_eq!(header_pill(Some(&backend)), Some((4020, None)));
    }

    #[test]
    fn url_substitution() {
        let mut r = run(RunState::Running);
        r.open_url = Some("http://localhost:$PORT/swagger".into());
        assert_eq!(r.url().as_deref(), Some("http://localhost:4020/swagger"));
        assert_eq!(
            open_label("http://localhost:4020/swagger"),
            "Open localhost:4020/swagger"
        );
        assert_eq!(open_label("http://localhost:4020/"), "Open localhost:4020");
        assert_eq!(
            header_pill(Some(&r)),
            Some((4020, Some("http://localhost:4020/swagger".into())))
        );
        r.port = None;
        assert_eq!(r.url(), None);
    }

    #[test]
    fn a_missing_command_says_so() {
        let mut r = run(RunState::NotStarted);
        r.command.clear();
        let c = card(&r, hhmm);
        assert_eq!(c.command, None);
        assert!(c.message.unwrap().starts_with("No run command yet"));
    }

    #[test]
    fn chip_menu_has_open_only_with_an_open_url() {
        let items = chip_menu(&run(RunState::Running));
        let labels: Vec<_> = items.iter().map(|i| i.label.as_str()).collect();
        assert_eq!(labels, ["Open :4020", "Stop"]);
        assert!(items[1].danger);
        let mut backend = run(RunState::Running);
        backend.open_url = None;
        let labels: Vec<_> = chip_menu(&backend).into_iter().map(|i| i.label).collect();
        assert_eq!(labels, ["Stop"]);
        assert!(chip_menu(&run(RunState::Stopped)).is_empty());
    }

    #[test]
    fn no_pill_once_stopped() {
        assert_eq!(header_pill(Some(&run(RunState::Stopped))), None);
    }
}
