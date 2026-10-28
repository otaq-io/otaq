//! `terminal.*`: what a terminal sends besides its output channel (T-0216): typed input and the terminal's own
//! answers to the program (SVC-06), its size, its keyboard focus (SES-10), the end of an attach, the terminal settings
//! (font size, font, renderer) and opening a link clicked in it. The output comes over the door's channel
//! ([`crate::terminal`]).

use otaq_core::board::settings::{TERMINAL_FONT_SIZES, TerminalRenderer};
use otaq_core::protocol::{FocusParams, InputParams, Reply, Request, ResizeParams};
use otaq_core::session::TerminalSize;
use otaq_view::view_models::terminal_font;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    match method {
        "terminal.input" => input(app, parse(params)?).await,
        "terminal.resize" => resize(app, parse(params)?).await,
        "terminal.focus" => focus(app, parse(params)?).await,
        "terminal.detach" => {
            let params: TerminalDetach = parse(params)?;
            app.terminals.detach(app, params.attach).await;
            Ok(Value::Null)
        }
        "terminal.settings" => settings(app).await,
        "terminal.open_link" => open_link(parse(params)?),
        _ => Err(unknown(method)),
    }
}

fn parse<T: serde::de::DeserializeOwned>(params: Value) -> Result<T, RequestError> {
    serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))
}

/// Bytes for the session's program (`terminal.input`).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct TerminalInput {
    pub session: String,
    /// The bytes, base64.
    pub data: String,
    /// The terminal answering the program (cursor position, colours, mouse reports), not a person typing: these
    /// bytes don't touch the unsent-input flag (d0048).
    pub reply: bool,
}

/// The terminal's size in cells (`terminal.resize`); the PTY follows it.
#[derive(Debug, Clone, Deserialize, TS)]
pub struct TerminalResize {
    pub session: String,
    pub cols: u16,
    pub rows: u16,
}

/// The terminal gained or lost the keyboard (`terminal.focus`).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct TerminalFocus {
    pub session: String,
    pub focused: bool,
}

/// The end of an attach (`terminal.detach`), by the number the attach answered.
#[derive(Debug, Clone, Copy, Deserialize, TS)]
pub struct TerminalDetach {
    pub attach: u64,
}

/// How terminals draw. `TerminalRenderer` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "TerminalRenderer")]
pub enum Renderer {
    /// xterm.js's WebGL renderer: drawn on the GPU.
    Webgl,
    /// xterm.js's DOM renderer: drawn with the page's elements.
    Dom,
}

/// The terminal settings every terminal follows (`terminal.settings`, asked again on `settings_changed`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct TerminalSettings {
    /// Points, 8–32.
    pub font_size: u32,
    /// The CSS font stack (d0117 point 5): the chosen font or Geist Mono, the installed Nerd Fonts for missing
    /// symbols, then `monospace`.
    pub font_family: String,
    pub renderer: Renderer,
}

/// A link clicked in a terminal (`terminal.open_link`).
#[derive(Debug, Clone, Deserialize, TS)]
pub struct TerminalLink {
    pub url: String,
}

async fn input(app: &App, params: TerminalInput) -> Result<Value, RequestError> {
    app.service
        .send(Request::SessionInput(InputParams {
            session: params.session,
            data: params.data,
            reply: params.reply,
            if_empty: false,
        }))
        .await?;
    Ok(Value::Null)
}

async fn resize(app: &App, params: TerminalResize) -> Result<Value, RequestError> {
    app.service
        .send(Request::SessionResize(ResizeParams {
            session: params.session,
            size: TerminalSize {
                cols: params.cols.max(1),
                rows: params.rows.max(1),
            },
        }))
        .await?;
    Ok(Value::Null)
}

/// Told to the service only for an agent's terminal: the focused-terminal rule is about agents (ATT-03).
async fn focus(app: &App, params: TerminalFocus) -> Result<Value, RequestError> {
    if app.terminals.is_agent(&params.session) {
        app.service
            .send(Request::SessionFocus(FocusParams {
                session: params.session,
                focused: params.focused,
            }))
            .await?;
    }
    Ok(Value::Null)
}

async fn settings(app: &App) -> Result<Value, RequestError> {
    let settings = match app.service.send(Request::SettingsGet).await? {
        Reply::GlobalSettings(reply) => reply.settings,
        other => {
            return Err(RequestError::plain(format!(
                "unexpected answer to settings.get: {other:?}"
            )));
        }
    };
    let fonts = app.fonts.get().await;
    reply(&TerminalSettings {
        font_family: terminal_font::stack(&settings, &fonts),
        font_size: settings
            .terminal_font_size
            .clamp(*TERMINAL_FONT_SIZES.start(), *TERMINAL_FONT_SIZES.end()),
        renderer: match settings.terminal_renderer {
            TerminalRenderer::Webgl => Renderer::Webgl,
            TerminalRenderer::Dom => Renderer::Dom,
        },
    })
}

/// Opens a web link in the system browser. Only `http` and `https`: a terminal's output is the program's, so a
/// `file:` or custom-scheme link isn't opened from it.
fn open_link(params: TerminalLink) -> Result<Value, RequestError> {
    let url = params.url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(RequestError::plain(format!(
            "only web links open from a terminal: {url}"
        )));
    }
    open::that_detached(url)
        .map_err(|e| RequestError::plain(format!("couldn't open {url}: {e}")))?;
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use otaq_core::board::projects::AddProject;
    use otaq_core::git::testing::TempRepos;
    use otaq_core::protocol::{ProjectTerminalParams, decode_bytes, encode_bytes};
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// What one terminal's channel received, in order.
    #[derive(Clone, Default)]
    struct Screen(std::sync::Arc<std::sync::Mutex<Vec<Value>>>);

    impl Screen {
        fn sink(&self) -> crate::events::Sink {
            let seen = self.0.clone();
            Box::new(move |v| {
                seen.lock().unwrap().push(v);
                true
            })
        }

        /// The bytes as the page would draw them: each replay starts over.
        fn text(&self) -> String {
            let mut bytes = Vec::new();
            for feed in self.0.lock().unwrap().iter() {
                match feed["kind"].as_str() {
                    Some("replay") => {
                        bytes = decode_bytes(feed["data"].as_str().unwrap()).unwrap();
                    }
                    Some("live") => {
                        bytes.extend(decode_bytes(feed["data"].as_str().unwrap()).unwrap());
                    }
                    _ => {}
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }

        /// The first replay's bytes.
        fn replay(&self) -> String {
            let feeds = self.0.lock().unwrap();
            let first = feeds.iter().find(|f| f["kind"] == "replay").expect("a replay");
            String::from_utf8_lossy(&decode_bytes(first["data"].as_str().unwrap()).unwrap()).into_owned()
        }

        /// Each replay's length (its end, as offsets count from the session's first byte while the scrollback
        /// holds everything) and the start of the live output after it.
        fn seams(&self) -> String {
            let feeds = self.0.lock().unwrap();
            let mut out = Vec::new();
            for (i, feed) in feeds.iter().enumerate() {
                if feed["kind"] != "replay" {
                    continue;
                }
                let replay = decode_bytes(feed["data"].as_str().unwrap()).unwrap();
                let live = feeds[i + 1..]
                    .iter()
                    .take_while(|f| f["kind"] != "replay")
                    .find(|f| f["kind"] == "live")
                    .map(|f| decode_bytes(f["data"].as_str().unwrap()).unwrap());
                let tail = &replay[replay.len().saturating_sub(40)..];
                out.push(format!(
                    "replay ends at {} with {:?}, first live chunk {:?}",
                    replay.len(),
                    String::from_utf8_lossy(tail),
                    live.map(|l| String::from_utf8_lossy(&l[..l.len().min(40)]).into_owned())
                ));
            }
            format!("{} attach(es): {}", out.len(), out.join(" | "))
        }

        fn phases(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .filter(|f| f["kind"] == "phase")
                .map(|f| f["phase"].as_str().unwrap().to_owned())
                .collect()
        }

        /// Whether `want` holds within `limit`.
        async fn wait_up_to(&self, limit: Duration, want: impl Fn(&Self) -> bool) -> bool {
            let deadline = Instant::now() + limit;
            while !want(self) {
                if Instant::now() >= deadline {
                    return false;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            true
        }

        async fn wait_for(&self, what: &str, want: impl Fn(&Self) -> bool) {
            let deadline = Instant::now() + Duration::from_secs(20);
            while !want(self) {
                assert!(
                    Instant::now() < deadline,
                    "waited for {what}; phases {:?}, text ends {:?}",
                    self.phases(),
                    self.text().chars().rev().take(300).collect::<String>()
                );
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        }
    }

    /// A project with a Project terminal running `sh` (the harness's shell), and that terminal's session.
    async fn project_terminal(h: &Harness, repos: &TempRepos) -> String {
        h.wait_for("connected", connected).await;
        h.app
            .service
            .send(Request::ProjectAdd(AddProject {
                path: repos.repo("shop-web", 1).display().to_string(),
                ..Default::default()
            }))
            .await
            .unwrap();
        let reply = h
            .app
            .service
            .send(Request::ProjectTerminalOpen(ProjectTerminalParams {
                project: "SHOP".into(),
                new: true,
            }))
            .await
            .unwrap();
        let Reply::ProjectTerminal(terminal) = reply else {
            panic!("{reply:?}")
        };
        terminal.session.id
    }

    async fn type_text(h: &Harness, session: &str, text: &str, reply: bool) {
        let params = json!({ "session": session, "data": encode_bytes(text.as_bytes()), "reply": reply });
        request(&h.app, "terminal.input", params).await.unwrap();
    }

    /// The numbers of the complete `n <i>` lines, in order. A line's text is what follows its last `\r`: on Windows
    /// the first one comes after the shell's own `ESC [ ? 2004 l` and a `\r`, which Otaq's ConPTY passes through.
    fn numbered(text: &str) -> Vec<u32> {
        text.split("\r\n")
            .filter_map(|line| line.rsplit('\r').next()?.strip_prefix("n ")?.parse().ok())
            .collect()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn attaching_mid_stream_shows_every_line_once_in_order() {
        let h = Harness::new(None);
        let repos = TempRepos::new();
        let session = project_terminal(&h, &repos).await;
        // An ordinary command printing numbered lines. It waits at two gates, so that however slow the machine,
        // the terminal attaches after lines were printed and while more keep coming, and before the end.
        let gates = tempfile::tempdir().unwrap();
        let gate = |n: u32| gates.path().join(format!("gate{n}"));
        let wait = |n: u32| format!("while [ ! -e '{}' ]; do sleep 0.02; done", gate(n).display());
        type_text(
            &h,
            &session,
            &format!(
                "i=0; while [ $i -lt 6000 ]; do echo \"n $i\"; i=$((i+1)); \
                 [ $i -eq 1000 ] && {}; [ $i -eq 5000 ] && {}; \
                 [ $((i % 300)) -eq 0 ] && sleep 0.1; done; echo finished\r",
                wait(1),
                wait(2)
            ),
            false,
        )
        .await;
        let printed = |line: &str| {
            let stored = h.service.sessions().attach(&session).unwrap().scrollback;
            String::from_utf8_lossy(&stored).contains(line)
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while !printed("\nn 999\r\n") {
            assert!(Instant::now() < deadline, "the command printed its first lines");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }

        std::fs::write(gate(1), "").unwrap();
        let screen = Screen::default();
        let attach = crate::terminal::attach(&h.app, &session, true, screen.sink())
            .await
            .unwrap();
        screen
            .wait_for("live", |s| s.phases().last().is_some_and(|p| p == "live"))
            .await;
        std::fs::write(gate(2), "").unwrap();
        screen
            .wait_for("the last line", |s| s.text().contains("\r\nfinished"))
            .await;
        // The replay holds the lines printed before the attach, the live output the rest.
        let replayed = numbered(&screen.replay());
        assert!(replayed.len() >= 1000, "attached after the first gate");
        assert!(replayed.len() < 5000, "attached before the second gate");
        let text = screen.text();
        let lines = numbered(&text);
        let first = *lines.first().expect("lines");
        if let Some(missing) = (first..6000).find(|n| !lines.contains(n)) {
            // Say what is there instead, so a failure tells a lost byte from a line glued to other output, and
            // where the replay ended and the live output began (the seam), and how often it attached.
            let before = format!("\nn {}\r\n", missing.saturating_sub(1));
            let at = text.find(&before).unwrap_or(0);
            panic!(
                "line {missing} is missing; the text there is {:?}; {}",
                &text[at..(at + 120).min(text.len())],
                screen.seams()
            );
        }
        assert_eq!(
            lines,
            (first..6000).collect::<Vec<_>>(),
            "every line from the first stored one, once, in order"
        );
        assert_eq!(screen.phases(), ["attaching", "live"]);

        request(&h.app, "terminal.detach", json!({ "attach": attach }))
            .await
            .unwrap();
        assert!(!h.app.terminals.is_agent(&session));
        h.service.stop_all().await;
    }

    /// A shell line asking the terminal for its background, reading what comes back within a second and printing
    /// it in hex after `seen-<marker>:` (written in two pieces, so the typed command line doesn't contain it).
    #[cfg(unix)]
    fn ask_background(marker: &str) -> String {
        format!(
            "stty -icanon -echo min 0 time 10; printf '\\033]11;?\\007'; sleep 1; \
             a=$(dd bs=256 count=1 2>/dev/null | od -An -tx1 | tr -d ' \\n'); stty sane; \
             echo \"seen\"\"-{marker}:$a.\"\r"
        )
    }

    /// [`ask_background`] on Windows: this test binary, started again as [`colour_asker`] (Git's `sh` drops the
    /// question on its way out, Q601).
    #[cfg(windows)]
    fn ask_background(marker: &str) -> String {
        use otaq_core::service::testing::{ask_marker_var, colour_asker_args};
        let program = std::env::current_exe().expect("the test binary");
        format!(
            "{}={marker} '{}' {}\r",
            ask_marker_var(),
            program.display(),
            colour_asker_args("api::terminal::tests::colour_asker").join(" ")
        )
    }

    /// The program [`ask_background`] starts on Windows; does nothing when run with the other ignored tests.
    #[cfg(windows)]
    #[test]
    #[ignore = "the colour test's stand-in program on Windows"]
    fn colour_asker() {
        otaq_core::service::testing::colour_asker();
    }

    /// d0117 point 4: a program's colour question gets exactly one answer. A read-only terminal
    /// doesn't answer, so the service does, with the theme's background (Otaq Dark, none picked yet); a terminal
    /// that answers keeps the service silent (this page stands in for xterm.js and answers nothing, so the program
    /// reads nothing).
    #[tokio::test(flavor = "multi_thread")]
    async fn a_read_only_terminal_leaves_the_colour_answer_to_the_service() {
        let h = Harness::new(None);
        let repos = TempRepos::new();
        let session = project_terminal(&h, &repos).await;
        let answer = b"\x1b]11;rgb:0a0a/0b0b/0d0d\x07"
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        for (read_only, expected) in [(true, answer.as_str()), (false, "")] {
            let marker = format!("seen-{read_only}");
            let ask = ask_background(&read_only.to_string());
            let screen = Screen::default();
            let attach = crate::terminal::attach(&h.app, &session, !read_only, screen.sink())
                .await
                .unwrap();
            screen
                .wait_for("live", |s| s.phases().last().is_some_and(|p| p == "live"))
                .await;
            type_text(&h, &session, &ask, false).await;
            screen
                .wait_for(&marker, |s| s.text().contains(&format!("{marker}:")))
                .await;
            assert!(
                screen.text().contains(&format!("{marker}:{expected}.")),
                "{}",
                screen.text()
            );
            request(&h.app, "terminal.detach", json!({ "attach": attach }))
                .await
                .unwrap();
        }

        // A read-only terminal and one that answers show the session; the one that answers closes: the service
        // answers again.
        let read_only = Screen::default();
        let watching = crate::terminal::attach(&h.app, &session, false, read_only.sink())
            .await
            .unwrap();
        let answering = Screen::default();
        let closing = crate::terminal::attach(&h.app, &session, true, answering.sink())
            .await
            .unwrap();
        for screen in [&read_only, &answering] {
            screen
                .wait_for("live", |s| s.phases().last().is_some_and(|p| p == "live"))
                .await;
        }
        request(&h.app, "terminal.detach", json!({ "attach": closing }))
            .await
            .unwrap();
        type_text(
            &h,
            &session,
            &ask_background("after"),
            false,
        )
        .await;
        read_only
            .wait_for("seen-after", |s| s.text().contains("seen-after:"))
            .await;
        assert!(
            read_only.text().contains(&format!("seen-after:{answer}.")),
            "{}",
            read_only.text()
        );
        request(&h.app, "terminal.detach", json!({ "attach": watching }))
            .await
            .unwrap();
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn typing_paste_and_resize_reach_the_pty_and_the_unsent_flag() {
        let h = Harness::new(None);
        let repos = TempRepos::new();
        let session = project_terminal(&h, &repos).await;
        let screen = Screen::default();
        crate::terminal::attach(&h.app, &session, true, screen.sink())
            .await
            .unwrap();
        screen
            .wait_for("live", |s| s.phases().last().is_some_and(|p| p == "live"))
            .await;
        let unsent = || h.service.sessions().has_unsent_input(&session).unwrap();

        // The terminal's own answers don't count as typing; a typed character does, Enter sends it.
        // (A cursor position report, then Ctrl+U so the shell drops it from its line.)
        type_text(&h, &session, "\x1b[1;1R\x15", true).await;
        assert!(!unsent(), "a reply is not unsent text");
        type_text(&h, &session, "echo typed-$((40+2))", false).await;
        assert!(unsent());
        type_text(&h, &session, "\r", false).await;
        assert!(!unsent());
        screen
            .wait_for("the echo", |s| s.text().contains("typed-42"))
            .await;

        // A paste arrives as one piece (wrapped in bracketed-paste markers by xterm.js only when the program asks).
        type_text(&h, &session, "echo pasted-1\necho pasted-2\r", false).await;
        // Each command's output is a line of its own, after the shell's prompt where it prints one first (dash on
        // Linux shows "$ pasted-2").
        let printed = |text: &str, word: &str| {
            text.lines()
                .any(|line| line.trim_end().ends_with(word) && !line.contains("echo "))
        };
        screen
            .wait_for("the pasted commands", |s| {
                let text = s.text();
                printed(&text, "pasted-1") && printed(&text, "pasted-2")
            })
            .await;

        let size = json!({ "session": session, "cols": 101, "rows": 33 });
        request(&h.app, "terminal.resize", size).await.unwrap();
        // Windows repaints the console after a resize, and under load a key typed during that repaint can be lost
        // ("stty size" arrives as "tty size"): clear the line and type it again until the new size shows.
        for tries in 1.. {
            type_text(&h, &session, "\x15stty size\r", false).await;
            if screen
                .wait_up_to(Duration::from_secs(5), |s| s.text().contains("33 101"))
                .await
            {
                break;
            }
            assert!(tries < 4, "the new size never showed: {:?}", screen.text());
        }

        // Focus counts only for an agent's terminal; a Project terminal's is accepted and not passed on.
        let focus = json!({ "session": session, "focused": true });
        assert_eq!(
            request(&h.app, "terminal.focus", focus).await,
            Ok(Value::Null)
        );
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn while_reconnecting_the_terminal_says_so_and_a_gone_session_ends_it() {
        let h = Harness::new(None);
        let repos = TempRepos::new();
        let session = project_terminal(&h, &repos).await;
        type_text(&h, &session, "echo before-the-drop\r", false).await;
        let screen = Screen::default();
        crate::terminal::attach(&h.app, &session, true, screen.sink())
            .await
            .unwrap();
        // The printed line, not the typed one: a line's text after its last \r, as in `numbered` (Microsoft's
        // ConPTY passes the shell's own `\x1b[?2004l\r` through before it).
        screen
            .wait_for("the output", |s| {
                s.text()
                    .split("\r\n")
                    .any(|line| line.rsplit('\r').next() == Some("before-the-drop"))
            })
            .await;

        // The service goes away: the page hears "reconnecting" and keeps what it shows.
        h.service.stop_all().await;
        screen
            .wait_for("reconnecting", |s| {
                s.phases().last().is_some_and(|p| p == "reconnecting")
            })
            .await;
        assert!(screen.text().contains("before-the-drop"));
        assert!(
            request(&h.app, "terminal.input", json!({ "session": session, "data": "eA==", "reply": false }))
                .await
                .is_err(),
            "input fails while disconnected"
        );

        // A new service doesn't have the old one's terminals (a restart ends them): the terminal stops there.
        h.service.bind().await.unwrap();
        screen
            .wait_for("no session", |s| {
                s.phases().last().is_some_and(|p| p == "no_session")
            })
            .await;
        assert_eq!(
            screen.phases(),
            ["attaching", "live", "reconnecting", "no_session"]
        );
        h.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn settings_carry_the_font_size_the_font_and_the_renderer() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let got = request(&h.app, "terminal.settings", Value::Null)
            .await
            .unwrap();
        let renderer = if cfg!(target_os = "linux") { "dom" } else { "webgl" };
        let geist = r#""Geist Mono", monospace"#;
        assert_eq!(
            got,
            json!({ "font_size": 13, "font_family": geist, "renderer": renderer })
        );

        let Reply::GlobalSettings(mut stored) = h.app.service.send(Request::SettingsGet).await.unwrap()
        else {
            panic!()
        };
        stored.settings.terminal_renderer = TerminalRenderer::Dom;
        stored.settings.terminal_font_size = 18;
        // A font that isn't installed (here: no fonts at all) is Geist Mono, quietly.
        stored.settings.terminal_font_family = Some("MesloLGS NF".into());
        h.app
            .service
            .send(Request::SettingsSet(stored.settings))
            .await
            .unwrap();
        let got = request(&h.app, "terminal.settings", Value::Null)
            .await
            .unwrap();
        assert_eq!(
            got,
            json!({ "font_size": 18, "font_family": geist, "renderer": "dom" })
        );
        h.service.stop_all().await;
    }

    #[test]
    fn only_web_links_open() {
        for url in ["file:///etc/passwd", "javascript:alert(1)", "vscode://x", ""] {
            let link = TerminalLink { url: url.into() };
            assert!(open_link(link).is_err(), "{url}");
        }
    }
}
