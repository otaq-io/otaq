//! A tool's own sign-in screen at a session's start (decision 118, F-E2-4).
//!
//! Codex whose ChatGPT sign-in ran out still says "Logged in using ChatGPT" to `codex login status` (it reads its
//! `auth.json` without refreshing it), so Settings → Agents says signed in and the agent starts; but the session opens
//! Codex's onboarding menu ("Welcome to Codex … 1. Sign in with ChatGPT … Press enter to continue") instead of the
//! conversation, and sends no hook until the user gets past it. Nothing else would tell: the card said Working.
//!
//! So while a session of such a tool ([`crate::agent_tools::AgentTool::has_sign_in_screen`]) has sent no hook yet, its
//! screen is looked at every [`LOOK_EVERY`]. Once it shows the tool's sign-in screen
//! ([`crate::agent_tools::AgentTool::shows_sign_in_screen`]):
//!
//! - a ticket's agent reads **Needs you** with "Codex needs you to sign in" ([`prompt`]), the way a prompt drawn on its
//!   screen does: the session is kept, the user signs in in its terminal, and the agent goes on with the prompt it was
//!   started with (Codex keeps its launch prompt through the menu and runs it after, checked in the Windows VM with
//!   Codex 0.166); its first hook moves the card on. The ticket's other sessions (the message writer, an answer
//!   session) show it on their own screen's prompt, as a permission prompt there does; a checker, a helper or a
//!   Project agent read it as their hook's Needs you would;
//! - the tool reads **not signed in** on Settings → Agents ([`ConnectedTools::at_sign_in_screen`]), and stays so
//!   whatever its status command says, until a session of it sends its first hook after showing the screen (it got past
//!   it) or a Sign in… terminal of it ends; the tool's own answer then stands again.
//!
//! Each session counts once per showing: a screen that goes and comes back counts again. In memory, like the turns.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use tokio::sync::watch;

use super::{Input, Signal, Signals};
use crate::agent_tools::Tool;
use crate::agent_tools::connected::ConnectedTools;
use crate::caller::Caller;
use crate::session::SessionKind;

/// How often the screens of the sessions that haven't sent a hook yet are looked at.
pub const LOOK_EVERY: Duration = Duration::from_secs(2);

/// How the prompt ends: "Codex needs you to sign in".
pub const PROMPT_END: &str = " needs you to sign in";

/// What Needs you says while `tool`'s session shows its sign-in screen: "Codex needs you to sign in".
pub fn prompt(tool: Tool) -> String {
    format!("{}{PROMPT_END}", tool.display_name())
}

/// Whether a Needs you prompt is a sign-in screen's ([`prompt`]) rather than a permission prompt's.
pub fn is_prompt(text: &str) -> bool {
    let text = text.trim();
    Tool::ALL.iter().any(|&tool| text == prompt(tool))
}

/// The sessions looked at, in memory.
#[derive(Debug, Default)]
pub(super) struct SignInScreens {
    /// Sessions that sent a hook: past any screen of the tool's own at start.
    heard: HashSet<String>,
    /// Sessions that showed the sign-in screen and haven't sent a hook since, with their tool.
    seen: HashMap<String, Tool>,
    /// Sessions whose screen showed it at the last look.
    showing: HashSet<String>,
}

/// What one look found.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Looked {
    /// The tools whose sessions started showing their sign-in screen.
    pub shown: Vec<Tool>,
    /// The tools whose sessions sent their first hook after showing it.
    pub past: Vec<Tool>,
}

impl Signals {
    /// `session` sent a hook (or its tool's watched file a line): it is past any screen of the tool's own at start.
    pub(super) fn heard_from(&self, session: &str) {
        self.inner
            .sign_in
            .lock()
            .expect("sign-in screens")
            .heard
            .insert(session.to_owned());
    }

    /// One look at the screens of the running sessions that haven't sent a hook yet, of a tool that may stop at a
    /// sign-in screen of its own: one that starts showing it is applied to its session's kind (Needs you). Which tools
    /// showed it, and which got past it.
    pub async fn look_for_sign_in_screens(&self) -> Looked {
        let running: Vec<_> = self
            .inner
            .sessions
            .in_memory()
            .into_iter()
            .filter(|info| info.is_running())
            .filter_map(|info| info.tool.map(|tool| (info, tool)))
            .filter(|(_, tool)| self.inner.adapters.get(*tool).has_sign_in_screen())
            .collect();
        let mut looked = Looked::default();
        let mut apply = Vec::new();
        {
            let mut screens = self.inner.sign_in.lock().expect("sign-in screens");
            let ids: HashSet<&str> = running.iter().map(|(info, _)| info.id.as_str()).collect();
            screens.seen.retain(|id, _| ids.contains(id.as_str()));
            screens.showing.retain(|id| ids.contains(id.as_str()));
            for (info, tool) in &running {
                if screens.heard.contains(&info.id) {
                    if let Some(tool) = screens.seen.remove(&info.id) {
                        looked.past.push(tool);
                    }
                    screens.showing.remove(&info.id);
                    continue;
                }
                let adapter = self.inner.adapters.get(*tool);
                let shows = self
                    .inner
                    .sessions
                    .visible_screen(&info.id)
                    .is_some_and(|screen| adapter.shows_sign_in_screen(&screen));
                if !shows {
                    screens.showing.remove(&info.id);
                    continue;
                }
                screens.seen.insert(info.id.clone(), *tool);
                if screens.showing.insert(info.id.clone()) {
                    looked.shown.push(*tool);
                    apply.push((info.id.clone(), info.kind, *tool));
                }
            }
            // Only the running sessions' hooks matter (any session's first hook comes while it runs).
            let heard = std::mem::take(&mut screens.heard);
            screens.heard = heard
                .into_iter()
                .filter(|id| {
                    self.inner
                        .sessions
                        .info(id)
                        .is_ok_and(|info| info.is_running())
                })
                .collect();
        }
        for (session, kind, tool) in apply {
            self.at_sign_in_screen(&session, kind, tool).await;
        }
        looked
    }

    /// `session` (of `kind`) shows `tool`'s sign-in screen: Needs you for whoever it works for, as a prompt drawn on
    /// its screen.
    async fn at_sign_in_screen(&self, session: &str, kind: SessionKind, tool: Tool) {
        let prompt = prompt(tool);
        let Some(caller) = super::watch::caller_of(&self.inner.sessions, session) else {
            tracing::info!(session, %tool, "a session of no ticket or agent shows the tool's sign-in screen");
            return;
        };
        tracing::info!(session, %tool, "the session shows the tool's sign-in screen: it needs the user to sign in");
        match &caller {
            // The ticket's agent: Needs you, without moving its last signal or taking its first prompt as delivered
            // (it waits behind the screen).
            Caller::Agent { ticket_id, .. } if kind == SessionKind::Agent => {
                if let Err(e) = self.apply(ticket_id, &Input::Drawn { prompt }).await {
                    tracing::warn!(session, error = %e, "can't show the sign-in screen's Needs you");
                }
            }
            // The ticket's other sessions (the message writer, an answer session): their screen's prompt, which their
            // next hook clears ([`Self::track_prompts`]).
            Caller::Agent {
                ticket, ticket_id, ..
            } => {
                self.inner
                    .board
                    .session_prompts()
                    .drawn(ticket_id, session, kind, &prompt);
                self.prompt_changed(ticket, ticket_id);
            }
            // A checker, a helper or a Project agent: as their hook's Needs you.
            _ => {
                if let Some(on_shown) = self.inner.on_shown.get() {
                    on_shown(caller.clone(), tool, Signal::NeedsYou { prompt }).await;
                }
            }
        }
    }
}

/// Looks at the sessions' screens every [`LOOK_EVERY`] until `stop`: a tool whose session shows its sign-in screen
/// reads not signed in, and one whose session got past it is checked again.
pub async fn run(signals: Signals, connected: ConnectedTools, mut stop: watch::Receiver<bool>) {
    let mut tick = tokio::time::interval(LOOK_EVERY);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = stop.wait_for(|s| *s) => break,
            _ = tick.tick() => {}
        }
        let looked = signals.look_for_sign_in_screens().await;
        for tool in looked.shown {
            connected.at_sign_in_screen(tool);
        }
        for tool in looked.past {
            tracing::info!(%tool, "a session got past the tool's sign-in screen: its sign-in is checked again");
            if let Err(e) = connected.past_sign_in_screen(tool).await {
                tracing::warn!(%tool, error = %e, "can't check the tool after its sign-in screen");
            }
        }
    }
}
