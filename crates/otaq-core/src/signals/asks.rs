//! Every agent session's open permission prompts, and the one on screen (Every prompt an agent shows reads Needs you,
//! one at a time).
//!
//! A tool may ask several permissions at once and draw them one after another: Codex fires the `PermissionRequest`
//! hooks of the commands it runs together in the same millisecond, then shows one approval dialog at a time. So each
//! session keeps **every** prompt its tool asked that isn't answered yet ([`OpenPrompts`]), and the one **on screen**
//! is the one Needs you names. Every session kind keeps them here alike (a ticket's agent and delivery session, a
//! checker, a helper, a Project agent): [`Signals::on_signal`] passes on a prompt's signal only when the one on screen
//! changes, so each kind's own wait (the ticket's status, the checker's Needs you, the helper's row, the Project
//! agent's wait) only ever names the prompt drawn.
//!
//! Which one is drawn is read off the session's screen ([`crate::agent_tools::AgentTool::prompt_screen`], Codex): the
//! open prompt the dialog on screen asks for. A tool whose screen isn't read shows its newest. A prompt that comes while
//! none shows is the one on screen at once, as the only one open (its hook comes as it is drawn); one queued behind
//! another shows once the screen draws it.
//!
//! The user's answer (a key in the session's terminal: Enter, or Codex's "y" alone, after which the tool sends nothing
//! until its turn ends or asks again) closes the one on screen only; the next one reads Needs you as soon as the screen
//! draws it, looked for every [`LOOK_EVERY`] while prompts wait. Any other signal of the tool that moves its turn on
//! closes them all. In memory, like the turns.
//!
//! For a ticket's own sessions (its agent, the delivery's message writer, a finished ticket's answer session) the one on
//! screen shows whatever the ticket's status: the message writer works while the ticket waits in Review, where its
//! prompt must not move the card, so Approve's dialogs ask here whether the writer waits for permission. A change tells
//! clients the ticket changed, so an open dialog asks again. The one on screen is mirrored into the board
//! ([`crate::board::session_prompts`]): a session's header reads "Waiting for you" over it, and one in a delivery skill
//! turn reads Needs you while the ticket keeps its status.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::Signals;
use super::hooks::Signal;
use crate::agent_tools::{PromptScreen, Tool};
use crate::board::BoardError;
use crate::caller::Caller;
use crate::domain::{Status, TicketKey};
use crate::protocol::Event;
use crate::session::SessionKind;

/// How often a session's screen is looked at while a prompt waits to be drawn.
pub const LOOK_EVERY: Duration = Duration::from_millis(150);

/// What applies a prompt the screen now shows to its session's kind, as a hook's signal is (the service: the
/// signal, then what follows it for a checker, a helper or a Project agent).
pub type OnShown =
    Arc<dyn Fn(Caller, Tool, Signal) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// What a session's screen shows of its tool's permission dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    /// The tool's screen isn't read: its newest open prompt is taken as the one drawn.
    Unread,
    /// No permission dialog is drawn.
    Hidden,
    /// A dialog is drawn: its text.
    Drawn(String),
}

/// One session's open permission prompts.
#[derive(Debug, Clone)]
pub struct OpenPrompts {
    caller: Caller,
    tool: Tool,
    /// Every prompt the tool asked that isn't answered yet, oldest first.
    open: Vec<String>,
    /// The one on screen, which Needs you names.
    shown: Option<String>,
    /// The last one answered, and the dialog on screen as the user answered it: while either still shows, the next
    /// isn't drawn yet.
    answered: Option<(String, Option<String>)>,
    /// Whether the tool's screen is read ([`Dialog::Unread`] otherwise).
    readable: bool,
    /// Whether a look at the screen is under way.
    looking: bool,
}

impl OpenPrompts {
    fn new(caller: Caller, tool: Tool, readable: bool) -> Self {
        Self {
            caller,
            tool,
            open: Vec::new(),
            shown: None,
            answered: None,
            readable,
            looking: false,
        }
    }

    /// Every open prompt, oldest first.
    pub fn open(&self) -> &[String] {
        &self.open
    }

    /// The one on screen.
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    /// The open prompt the screen shows (`screen`: how the tool's dialog is read). `asked`: a prompt whose hook just
    /// came.
    fn on_screen(
        &self,
        dialog: &Dialog,
        screen: Option<&dyn PromptScreen>,
        asked: Option<&str>,
    ) -> Option<String> {
        let kept = self.shown.clone().filter(|s| self.open.contains(s));
        match dialog {
            Dialog::Unread => self.open.last().cloned(),
            Dialog::Drawn(text) => {
                let shows = |prompt: &str| screen.is_some_and(|s| s.shows(text, prompt));
                if let Some(drawn) = self.open.iter().find(|p| shows(p)) {
                    Some(drawn.clone())
                } else if self.answered.as_ref().is_some_and(|(prompt, dialog)| {
                    shows(prompt) || dialog.as_deref() == Some(text.as_str())
                }) {
                    // The answered dialog until the tool draws the next.
                    kept
                } else {
                    // A dialog that names none of them: the one shown, else the oldest the screen can't name (an
                    // edit's approval). One it can name waits for its own hook.
                    kept.or_else(|| {
                        self.open
                            .iter()
                            .find(|p| !screen.is_some_and(|s| s.recognises(p)))
                            .cloned()
                    })
                }
            }
            // Its hook comes as the tool draws it: the only one open shows at once.
            Dialog::Hidden => {
                kept.or_else(|| asked.filter(|_| self.open.len() == 1).map(str::to_owned))
            }
        }
    }

    /// The tool asked `prompt`: the signal to pass on, a prompt only when the one on screen changed.
    fn asked(
        &mut self,
        prompt: &str,
        dialog: &Dialog,
        screen: Option<&dyn PromptScreen>,
    ) -> Signal {
        if !self.open.iter().any(|p| p == prompt) {
            self.open.push(prompt.to_owned());
        }
        let now = self.on_screen(dialog, screen, Some(prompt));
        self.show(now).unwrap_or(Signal::Ignored)
    }

    /// A look at the screen: the prompt it now shows, if another.
    fn looked(&mut self, dialog: &Dialog, screen: Option<&dyn PromptScreen>) -> Option<Signal> {
        let now = self.on_screen(dialog, screen, None);
        self.show(now)
    }

    fn show(&mut self, now: Option<String>) -> Option<Signal> {
        let prompt = now.filter(|p| self.shown.as_ref() != Some(p))?;
        self.shown = Some(prompt.clone());
        Some(Signal::NeedsYou { prompt })
    }

    /// The user answered the one on screen (`dialog`: what the screen shows as they did): it closes. The answered
    /// prompt, if one was shown.
    fn answer(&mut self, dialog: &Dialog) -> Option<String> {
        let answered = self.shown.take()?;
        if let Some(at) = self.open.iter().position(|p| *p == answered) {
            self.open.remove(at);
        }
        let on_screen = match dialog {
            Dialog::Drawn(text) => Some(text.clone()),
            Dialog::Unread | Dialog::Hidden => None,
        };
        self.answered = Some((answered.clone(), on_screen));
        Some(answered)
    }

    /// Whether the screen must be looked at: a prompt waits to be drawn, or one queued behind the one shown may be the
    /// one actually drawn (their hooks came together).
    fn wants_look(&self) -> bool {
        !self.open.is_empty() && (self.shown.is_none() || (self.readable && self.open.len() > 1))
    }
}

impl Signals {
    /// The permission prompt on `session`'s screen now, if any: the one Needs you names.
    pub fn open_prompt(&self, session: &str) -> Option<String> {
        self.inner
            .asks
            .lock()
            .expect("asks")
            .get(session)
            .and_then(|p| p.shown.clone())
    }

    /// `session`'s open prompts, if it has any.
    pub fn open_prompts(&self, session: &str) -> Option<OpenPrompts> {
        self.inner.asks.lock().expect("asks").get(session).cloned()
    }

    /// What applies a prompt the screen shows later to its session's kind (set once at service start).
    pub fn set_on_shown(&self, on_shown: OnShown) {
        let _ = self.inner.on_shown.set(on_shown);
    }

    /// A signal of `caller`'s `session` from `tool`, before it is applied: a prompt joins the session's open prompts and
    /// is passed on only once it is the one on screen ([`Signal::Ignored`] otherwise); any other signal that moves the
    /// turn on closes them all. Whether the one on screen changed comes with it.
    pub(super) fn track_prompts(
        &self,
        caller: &Caller,
        tool: Tool,
        session: &str,
        signal: Signal,
    ) -> (Signal, bool) {
        match signal {
            Signal::NeedsYou { prompt } => {
                let screen = self.prompt_screen(tool);
                let dialog = self.dialog(session, screen);
                let (signal, look) = {
                    let mut asks = self.inner.asks.lock().expect("asks");
                    let entry = asks.entry(session.to_owned()).or_insert_with(|| {
                        OpenPrompts::new(caller.clone(), tool, screen.is_some())
                    });
                    let signal = entry.asked(&prompt, &dialog, screen);
                    (signal, entry.wants_look() && !entry.looking)
                };
                self.mirror(session);
                if look {
                    self.look_soon(session);
                }
                let changed = matches!(signal, Signal::NeedsYou { .. });
                (signal, changed)
            }
            Signal::Ignored => (signal, false),
            other => {
                let closed = self.inner.asks.lock().expect("asks").remove(session);
                self.mirror(session);
                (other, closed.is_some_and(|p| p.shown.is_some()))
            }
        }
    }

    /// Mirrors the prompt on `session`'s screen into the board when the session is one of a ticket's own (its agent's,
    /// its delivery's, an answer session), where the ticket reads it as it loads ([`crate::board::session_prompts`]).
    fn mirror(&self, session: &str) {
        let shown = self
            .inner
            .asks
            .lock()
            .expect("asks")
            .get(session)
            .and_then(|p| match (&p.caller, &p.shown) {
                (Caller::Agent { ticket_id, .. }, Some(shown)) => {
                    Some((ticket_id.clone(), shown.clone()))
                }
                _ => None,
            });
        let prompts = self.inner.board.session_prompts();
        match shown {
            Some((ticket_id, prompt)) => {
                let kind = self
                    .inner
                    .sessions
                    .info(session)
                    .map_or(SessionKind::Agent, |info| info.kind);
                prompts.drawn(&ticket_id, session, kind, &prompt);
            }
            None => {
                prompts.gone(session);
            }
        }
    }

    /// The user answered `session`'s prompt on screen (a key in its terminal, or a command of the agent's that acts):
    /// it closes, and the next one open shows once the screen draws it. The answered prompt, if one was shown.
    pub fn answer_shown(&self, session: &str) -> Option<String> {
        let tool = self.inner.asks.lock().expect("asks").get(session)?.tool;
        let dialog = self.dialog(session, self.prompt_screen(tool));
        let (answered, look) = {
            let mut asks = self.inner.asks.lock().expect("asks");
            let entry = asks.get_mut(session)?;
            let answered = entry.answer(&dialog);
            (answered, entry.wants_look() && !entry.looking)
        };
        self.mirror(session);
        if look {
            self.look_soon(session);
        }
        answered
    }

    /// The ticket's own session's prompt on screen was answered (the user's key in its terminal, or a command of the
    /// agent's that acts): it closes, and clients are told.
    pub fn prompt_answered(&self, ticket: &TicketKey, ticket_id: &str, session: &str) {
        if self.answer_shown(session).is_some() {
            self.prompt_changed(ticket, ticket_id);
        }
    }

    /// The user's key in the ticket's agent terminal answered its prompt on screen. On a tool whose prompt takes a key
    /// alone (Codex's "y", [`crate::agent_tools::AgentTool::key_alone_approves_prompt`]) nothing else will say so until
    /// the turn ends or asks again, so a card showing Needs you is Working again now, as after Enter, until the next
    /// prompt open is drawn.
    pub async fn key_answered_prompt(
        &self,
        ticket: &TicketKey,
        ticket_id: &str,
        session: &str,
    ) -> Result<(), BoardError> {
        self.prompt_answered(ticket, ticket_id, session);
        let Ok(info) = self.inner.sessions.info(session) else {
            return Ok(());
        };
        let alone = info
            .tool
            .is_some_and(|tool| self.inner.adapters.get(tool).key_alone_approves_prompt());
        if alone
            && info.kind == SessionKind::Agent
            && self.status_of(ticket_id).await? == Some(Status::NeedsYou)
        {
            self.answered(ticket_id).await?;
        }
        Ok(())
    }

    /// A prompt the screen now shows, applied to `caller` as its hook's would be (without passing through
    /// [`Self::track_prompts`] again). The signal as applied.
    pub async fn prompt_shown(
        &self,
        caller: &Caller,
        signal: Signal,
    ) -> Result<Signal, BoardError> {
        if let Caller::Agent {
            ticket, ticket_id, ..
        } = caller
        {
            self.prompt_changed(ticket, ticket_id);
        }
        self.apply_signal(caller, signal).await
    }

    pub(super) fn prompt_changed(&self, ticket: &TicketKey, ticket_id: &str) {
        self.inner.events.publish(Event::TicketChanged {
            project: ticket.project.to_string(),
            id: ticket_id.to_owned(),
            key: ticket.clone(),
        });
    }

    fn prompt_screen(&self, tool: Tool) -> Option<&dyn PromptScreen> {
        self.inner.adapters.get(tool).prompt_screen()
    }

    /// What `session`'s screen shows of its tool's dialog.
    fn dialog(&self, session: &str, screen: Option<&dyn PromptScreen>) -> Dialog {
        let Some(reader) = screen else {
            return Dialog::Unread;
        };
        let Some(text) = self.inner.sessions.visible_screen(session) else {
            return Dialog::Hidden;
        };
        match reader.dialog(&text) {
            Some(dialog) => Dialog::Drawn(dialog.to_owned()),
            None => Dialog::Hidden,
        }
    }

    /// Looks at `session`'s screen every [`LOOK_EVERY`] while a prompt of it waits to be drawn, and applies the one it
    /// draws. Ends once none waits, or the session ended.
    fn look_soon(&self, session: &str) {
        {
            let mut asks = self.inner.asks.lock().expect("asks");
            match asks.get_mut(session) {
                Some(entry) if !entry.looking => entry.looking = true,
                _ => return,
            }
        }
        let signals = self.clone();
        let session = session.to_owned();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(LOOK_EVERY).await;
                let Some((caller, tool)) = signals.still_waiting(&session) else {
                    return;
                };
                let screen = signals.prompt_screen(tool);
                let dialog = signals.dialog(&session, screen);
                let shown = {
                    let mut asks = signals.inner.asks.lock().expect("asks");
                    match asks.get_mut(&session) {
                        Some(entry) => entry.looked(&dialog, screen),
                        None => return,
                    }
                };
                signals.mirror(&session);
                if let Some(signal) = shown {
                    tracing::debug!(session, ?signal, "a waiting prompt is drawn");
                    match signals.inner.on_shown.get() {
                        Some(on_shown) => on_shown(caller, tool, signal).await,
                        None => {
                            if let Err(e) = signals.prompt_shown(&caller, signal).await {
                                tracing::warn!(session, error = %e, "can't apply the prompt drawn");
                            }
                        }
                    }
                }
            }
        });
    }

    /// The caller and tool of `session`'s prompts while one still wants a look at the screen; otherwise the look ends
    /// (and a session that ended forgets its prompts).
    fn still_waiting(&self, session: &str) -> Option<(Caller, Tool)> {
        let running = self
            .inner
            .sessions
            .info(session)
            .is_ok_and(|info| info.is_running());
        let mut asks = self.inner.asks.lock().expect("asks");
        if !running {
            asks.remove(session);
            drop(asks);
            self.inner.board.session_prompts().gone(session);
            return None;
        }
        let entry = asks.get_mut(session)?;
        if !entry.wants_look() {
            entry.looking = false;
            return None;
        }
        Some((entry.caller.clone(), entry.tool))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Shows a prompt when the dialog's text contains it; can't name "Use …" ones.
    struct Contains;

    impl PromptScreen for Contains {
        fn dialog<'s>(&self, screen: &'s str) -> Option<&'s str> {
            Some(screen)
        }

        fn shows(&self, dialog: &str, prompt: &str) -> bool {
            dialog.contains(prompt)
        }

        fn recognises(&self, prompt: &str) -> bool {
            !prompt.starts_with("Use ")
        }
    }

    const SHOWS: Option<&dyn PromptScreen> = Some(&Contains);

    fn prompts() -> OpenPrompts {
        OpenPrompts::new(Caller::User, Tool::Codex, true)
    }

    fn needs_you(prompt: &str) -> Signal {
        Signal::NeedsYou {
            prompt: prompt.into(),
        }
    }

    fn drawn(text: &str) -> Dialog {
        Dialog::Drawn(text.into())
    }

    /// Two hooks together: the first shows at once, the second waits; "y" closes the first, the second shows once
    /// drawn, not while the answered dialog still shows; "y" again closes it.
    #[test]
    fn two_prompts_at_once_show_one_at_a_time() {
        let mut p = prompts();
        assert_eq!(
            p.asked("git log", &Dialog::Hidden, SHOWS),
            needs_you("git log")
        );
        assert_eq!(p.asked("git diff", &Dialog::Hidden, SHOWS), Signal::Ignored);
        assert_eq!(p.shown(), Some("git log"));
        assert!(p.wants_look(), "the second may be the one drawn");
        assert_eq!(p.looked(&drawn("$ git log"), SHOWS), None);

        assert_eq!(p.answer(&drawn("$ git log")).as_deref(), Some("git log"));
        assert_eq!(p.open(), ["git diff"]);
        assert_eq!(p.looked(&drawn("$ git log"), SHOWS), None, "not drawn yet");
        assert_eq!(
            p.looked(&drawn("$ git log (y)"), SHOWS),
            None,
            "redrawn, still the answered one"
        );
        assert_eq!(p.looked(&Dialog::Hidden, SHOWS), None, "its command runs");
        assert_eq!(
            p.looked(&drawn("$ git diff"), SHOWS),
            Some(needs_you("git diff"))
        );
        assert_eq!(p.looked(&drawn("$ git diff"), SHOWS), None, "once");

        assert_eq!(p.answer(&drawn("$ git diff")).as_deref(), Some("git diff"));
        assert!(p.open().is_empty());
        assert!(!p.wants_look());
    }

    /// Hooks that come in another order than the dialogs: the screen says which one is drawn.
    #[test]
    fn the_screen_says_which_is_drawn() {
        let mut p = prompts();
        assert_eq!(
            p.asked("git show", &Dialog::Hidden, SHOWS),
            needs_you("git show")
        );
        assert_eq!(
            p.asked("Get-Content", &drawn("$ Get-Content"), SHOWS),
            needs_you("Get-Content")
        );
        assert_eq!(p.looked(&drawn("$ Get-Content"), SHOWS), None);
        assert_eq!(
            p.answer(&drawn("$ Get-Content")).as_deref(),
            Some("Get-Content")
        );
        assert_eq!(
            p.looked(&drawn("$ git show"), SHOWS),
            Some(needs_you("git show"))
        );
    }

    /// A hook that comes while another command's dialog is drawn waits for that one's hook, which shows at once.
    #[test]
    fn a_dialog_waits_for_its_own_hook() {
        let mut p = prompts();
        assert_eq!(
            p.asked("git show", &drawn("$ Get-Content"), SHOWS),
            Signal::Ignored
        );
        assert_eq!(p.shown(), None);
        assert_eq!(
            p.asked("Get-Content", &drawn("$ Get-Content"), SHOWS),
            needs_you("Get-Content")
        );
    }

    /// A dialog that names none of them (an edit's approval) is the oldest's the screen can't name; a tool whose screen
    /// isn't read shows its newest.
    #[test]
    fn a_dialog_naming_none_and_an_unread_screen() {
        let mut p = prompts();
        p.asked("Use apply_patch", &Dialog::Hidden, SHOWS);
        p.asked("git diff", &Dialog::Hidden, SHOWS);
        p.answer(&drawn("make the following edits"));
        assert_eq!(p.looked(&drawn("make the following edits"), SHOWS), None);
        assert_eq!(
            p.looked(&drawn("$ git diff"), SHOWS),
            Some(needs_you("git diff"))
        );

        let mut p = OpenPrompts::new(Caller::User, Tool::Claude, false);
        assert_eq!(p.asked("one", &Dialog::Unread, None), needs_you("one"));
        assert_eq!(p.asked("two", &Dialog::Unread, None), needs_you("two"));
        assert!(!p.wants_look());
        assert_eq!(p.answer(&Dialog::Unread).as_deref(), Some("two"));
        assert_eq!(p.looked(&Dialog::Unread, None), Some(needs_you("one")));
    }
}
