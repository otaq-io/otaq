//! Wake-ups and the hand-back (LEAD-04, LEAD-12; d0083).
//!
//! - **Wake-ups:** [`EpicLeads::wake`] types an automatic message into a **live** lead (a `LeadWake` delivery, typed
//!   by the §9.3 rules: only when it is idle and its input line is empty) and sets the reason of the turn it starts,
//!   for the Lead Status. With no live lead nothing is queued. While the plan's usage limit holds the message waits,
//!   and the limit's Resume types it. The board calls it when a child **merges into the epic branch** ("SHOP-40.1 merged:
//!   Wishlist API") or **is closed** ("SHOP-40.6 closed without merging: Wishlist export (CSV)"); questions (T-0284)
//!   and first stops (T-0285) call it too. Nothing else wakes the lead.
//! - **The board's side** ([`LeadEvents`], set on the [`crate::board::Board`] through its [`LeadLink`]): a whole
//!   epic queued starts its lead when it has none; a child merged or closed wakes it; the epic reaching Ready for
//!   Review ends it; a child added or reopened after that resumes it with no message.
//! - **Handovers** (the frame T-0284 and T-0285 build on): a question or a first stop sent to the lead is registered
//!   with [`EpicLeads::wake_with_handover`]. It belongs to the lead's turn its wake-up was typed in. When that turn
//!   ends (Stop) and its owner hasn't settled it ([`EpicLeads::settle_handover`]), or when the lead ends while it is
//!   open, it is **handed back** through the callback its owner registered with [`EpicLeads::on_hand_back`], which
//!   sends it to the user as if no lead were live. Handovers live in memory: a restart ends the lead, and the
//!   owners' own rows say what still waits.
//! - **Children wait for the lead's turn** (d0124): a merge's or a close's wake-up is a handover of its own
//!   ([`FREED`], [`EpicLeads::wake_for_finished`]). Until the turn it was typed in ends, or the lead ends, the epic's
//!   children that the finished child held back don't start ([`LeadWait`]): "Waiting for the lead's update". Only that
//!   turn counts: one the user starts by typing while the wake-up still waits isn't it. Nothing waits on a delay, and
//!   it is all in memory: after a restart the lead is "Ended by restart", so nothing waits for it.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use crate::agent_tools::Tool;
use crate::board::tickets::Ticket;
use crate::board::{BoardError, Refusal};
use crate::domain::{Actor, TicketKey};
use crate::epic_lead::stuck::FirstStop;
use crate::project_agents::lead::{EpicLeads, TurnReason, Wake};
use crate::session::deliveries::DeliveryKind;
use crate::store::new_id;

/// "SHOP-40.1 merged: Wishlist API": the wake-up after a child merged into the epic branch.
pub fn merged_text(child: &TicketKey, title: &str) -> String {
    format!("{child} merged: {}", title.trim())
}

/// "SHOP-40.6 closed without merging: Wishlist export (CSV)": the wake-up after a child was closed.
pub fn closed_text(child: &TicketKey, title: &str) -> String {
    format!("{child} closed without merging: {}", title.trim())
}

/// How a child finished, for its wake-up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Finished {
    /// Merged into the epic branch: Local merge, Auto-merge, or Mark merged on its PR.
    Merged,
    /// Closed without merging.
    Closed,
}

type Fut<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What the board tells the epic leads (d0083). Each call logs its own failures: a lead never fails a board change.
pub trait LeadEvents: Send + Sync {
    /// A whole epic was queued (EPC-12): an epic with no lead at all gets one that checks the plan. Whether one
    /// started.
    fn epic_queued<'a>(&'a self, epic: &'a TicketKey) -> Fut<'a, bool>;
    /// An epic child merged into the epic branch or was closed: its lead wakes up.
    fn child_finished<'a>(&'a self, child: &'a Ticket, how: Finished) -> Fut<'a, ()>;
    /// The epic moved to Ready for Review: its lead ends.
    fn epic_in_review<'a>(&'a self, epic: &'a TicketKey, actor: &'a Actor) -> Fut<'a, ()>;
    /// A child was added to, or reopened in, an epic that had reached Review: its lead (ended there) resumes with
    /// no message.
    fn epic_back<'a>(&'a self, epic: &'a TicketKey) -> Fut<'a, ()>;
    /// What each live lead's epic children wait for (d0124), for the scheduler.
    fn lead_waits(&self) -> Fut<'_, Vec<LeadWait>>;
    /// Whether the child's next stop would go to its lead (T-0285): a live lead, and the child's attempt unused.
    fn takes_stop<'a>(&'a self, child: &'a Ticket) -> Fut<'a, bool>;
    /// The child reached a stop (T-0285): whether its live lead took it as the child's first stop, which is then
    /// held ([`crate::epic_lead::stuck::hold`]).
    fn child_stuck<'a>(&'a self, child: &'a Ticket, stop: &'a FirstStop) -> Fut<'a, bool>;
}

/// The board's link to the epic leads, set once the service has them. Without it (a board on its own) the calls
/// do nothing. It also keeps which tickets wait for their lead's update now (the scheduler's last pass), for the
/// cards. Cheap to clone.
#[derive(Clone, Default)]
pub struct LeadLink {
    events: Arc<Mutex<Option<Arc<dyn LeadEvents>>>>,
    waiting: Arc<Mutex<HashSet<String>>>,
}

impl std::fmt::Debug for LeadLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LeadLink")
    }
}

impl LeadLink {
    pub fn set(&self, events: Arc<dyn LeadEvents>) {
        *self.events.lock().expect("lead link") = Some(events);
    }

    fn get(&self) -> Option<Arc<dyn LeadEvents>> {
        self.events.lock().expect("lead link").clone()
    }

    /// See [`LeadEvents::lead_waits`]; none without leads.
    pub async fn lead_waits(&self) -> Vec<LeadWait> {
        match self.get() {
            Some(events) => events.lead_waits().await,
            None => Vec::new(),
        }
    }

    /// Whether the ticket waits for its lead's update ("Waiting for the lead's update", d0124).
    pub fn waits_for_lead(&self, ticket_id: &str) -> bool {
        self.waiting.lock().expect("lead waits").contains(ticket_id)
    }

    /// Whether any ticket waits for its lead's update now.
    pub fn any_waiting(&self) -> bool {
        !self.waiting.lock().expect("lead waits").is_empty()
    }

    /// The tickets that wait for their lead's update now (the scheduler's pass): those whose card changed.
    pub fn set_waiting(&self, now: HashSet<String>) -> Vec<String> {
        let mut waiting = self.waiting.lock().expect("lead waits");
        let changed = waiting.symmetric_difference(&now).cloned().collect();
        *waiting = now;
        changed
    }

    /// See [`LeadEvents::epic_queued`]; false without leads.
    pub async fn epic_queued(&self, epic: &TicketKey) -> bool {
        match self.get() {
            Some(events) => events.epic_queued(epic).await,
            None => false,
        }
    }

    /// See [`LeadEvents::child_finished`]; only for an epic child.
    pub async fn child_finished(&self, child: &Ticket, how: Finished) {
        if child.epic.is_none() {
            return;
        }
        if let Some(events) = self.get() {
            events.child_finished(child, how).await;
        }
    }

    /// See [`LeadEvents::epic_in_review`].
    pub async fn epic_in_review(&self, epic: &TicketKey, actor: &Actor) {
        if let Some(events) = self.get() {
            events.epic_in_review(epic, actor).await;
        }
    }

    /// See [`LeadEvents::epic_back`].
    pub async fn epic_back(&self, epic: &TicketKey) {
        if let Some(events) = self.get() {
            events.epic_back(epic).await;
        }
    }

    /// See [`LeadEvents::takes_stop`]; false without leads and for a ticket outside an epic.
    pub async fn takes_stop(&self, child: &Ticket) -> bool {
        if child.epic.is_none() {
            return false;
        }
        match self.get() {
            Some(events) => events.takes_stop(child).await,
            None => false,
        }
    }

    /// See [`LeadEvents::child_stuck`]; false without leads and for a ticket outside an epic.
    pub async fn child_stuck(&self, child: &Ticket, stop: &FirstStop) -> bool {
        if child.epic.is_none() {
            return false;
        }
        match self.get() {
            Some(events) => events.child_stuck(child, stop).await,
            None => false,
        }
    }
}

/// The handover owner of a merge's or a close's wake-up (d0124): the children it freed wait for the turn it starts.
pub const FREED: &str = "freed";

/// An epic with a live lead, and what its children wait for (d0124).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeadWait {
    pub epic: TicketKey,
    /// When the lead's session started: a child that finished before it was never this session's to pass on.
    pub since: i64,
    /// Finished children whose turn ended (or whose wake-up couldn't be queued), by id, with when: they hold nothing
    /// back unless they finished again after it.
    pub released: HashMap<String, i64>,
    /// The children each open turn's finished child held back when it finished, by id: they wait even when the lead
    /// removes the link in that turn.
    pub freed: HashSet<String>,
}

/// A question or a first stop sent to a lead, waiting for it to act (T-0284, T-0285).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Handover {
    pub id: String,
    /// Whose it is: the owner's callback hands it back ("question", "stuck").
    pub owner: &'static str,
    /// The child it is about.
    pub child: TicketKey,
    /// The owner's own reference (the question's id, …): what [`EpicLeads::settle_handover`] names.
    pub reference: String,
    /// The epic, and its lead's Project agent id.
    pub epic: TicketKey,
    pub lead: String,
    /// The wake-up text it was typed with.
    text: String,
    /// The lead's turn its wake-up was typed in; `None` until that turn starts.
    turn: Option<u64>,
}

/// Called with each handover handed back: its owner sends it to the user as if no lead were live.
pub type HandBack = Arc<dyn Fn(Handover) -> Fut<'static, ()> + Send + Sync>;

/// The open handovers of every lead, and the owners' hand-back callbacks. Cheap to clone.
#[derive(Clone, Default)]
pub struct Handovers {
    inner: Arc<Mutex<HandoversInner>>,
}

#[derive(Default)]
struct HandoversInner {
    /// By lead (Project agent id), oldest first.
    open: HashMap<String, Vec<Handover>>,
    /// Each lead's turn count, from its turns' Working hooks.
    turns: HashMap<String, u64>,
    callbacks: HashMap<&'static str, HandBack>,
    /// By lead: the finished children whose turn hasn't ended, with the children each held back (d0124).
    freed: HashMap<String, HashMap<String, Vec<String>>>,
    /// By lead: the finished children whose turn ended, with when (d0124).
    released: HashMap<String, HashMap<String, i64>>,
}

impl Handovers {
    fn lock(&self) -> std::sync::MutexGuard<'_, HandoversInner> {
        self.inner.lock().expect("handovers")
    }

    fn register(&self, handover: Handover) {
        self.lock()
            .open
            .entry(handover.lead.clone())
            .or_default()
            .push(handover);
    }

    /// The lead `lead`'s turn started: its handovers whose wake-up was typed (no longer waiting in `pending`) belong
    /// to this turn.
    pub(crate) fn turn_started(&self, lead: &str, pending: &[String]) {
        let mut inner = self.lock();
        let turn = {
            let n = inner.turns.entry(lead.to_owned()).or_default();
            *n += 1;
            *n
        };
        if let Some(open) = inner.open.get_mut(lead) {
            for h in open
                .iter_mut()
                .filter(|h| h.turn.is_none() && !pending.contains(&h.text))
            {
                h.turn = Some(turn);
            }
        }
    }

    /// The lead `lead`'s turn ended: the handovers of that turn it didn't settle, taken out.
    pub(crate) fn turn_ended(&self, lead: &str) -> Vec<Handover> {
        let mut inner = self.lock();
        let turn = inner.turns.get(lead).copied();
        let Some(open) = inner.open.get_mut(lead) else {
            return Vec::new();
        };
        let (back, keep) = std::mem::take(open)
            .into_iter()
            .partition(|h| h.turn.is_some() && h.turn == turn);
        *open = keep;
        back
    }

    /// The lead `lead` ended: every open handover, taken out. Its children wait for it no more (d0124).
    pub(crate) fn lead_ended(&self, lead: &str) -> Vec<Handover> {
        let mut inner = self.lock();
        inner.freed.remove(lead);
        inner.released.remove(lead);
        let mut open = inner.open.remove(lead).unwrap_or_default();
        open.retain(|h| h.owner != FREED);
        open
    }

    /// `child` (by id) finished while the lead `lead` was live: `held` (ids) wait for the turn about it (d0124).
    pub(crate) fn freed(&self, lead: &str, child: &str, held: Vec<String>) {
        self.lock()
            .freed
            .entry(lead.to_owned())
            .or_default()
            .insert(child.to_owned(), held);
    }

    /// The lead `lead`'s turn about `child` ended (or it couldn't be woken): what it held back may start.
    pub(crate) fn release(&self, lead: &str, child: &str) {
        let mut inner = self.lock();
        if let Some(freed) = inner.freed.get_mut(lead) {
            freed.remove(child);
        }
        inner
            .released
            .entry(lead.to_owned())
            .or_default()
            .insert(child.to_owned(), crate::store::now_ms());
    }

    /// What the lead `lead`'s epic children wait for, with its session's start.
    fn wait_of(&self, lead: &str, epic: TicketKey, since: i64) -> LeadWait {
        let inner = self.lock();
        LeadWait {
            epic,
            since,
            released: inner.released.get(lead).cloned().unwrap_or_default(),
            freed: inner
                .freed
                .get(lead)
                .into_iter()
                .flat_map(|f| f.values().flatten().cloned())
                .collect(),
        }
    }

    fn settle(&self, owner: &str, reference: &str) -> bool {
        let mut inner = self.lock();
        for open in inner.open.values_mut() {
            if let Some(i) = open
                .iter()
                .position(|h| h.owner == owner && h.reference == reference)
            {
                open.remove(i);
                return true;
            }
        }
        false
    }

    fn of(&self, lead: &str) -> Vec<Handover> {
        self.lock().open.get(lead).cloned().unwrap_or_default()
    }

    /// Hands each handover back through its owner's callback. A merge's or a close's ([`FREED`]) releases the
    /// children it held back.
    pub(crate) async fn hand_back(&self, handovers: Vec<Handover>) {
        for handover in handovers {
            if handover.owner == FREED {
                tracing::info!(child = %handover.child, epic = %handover.epic, "the lead's turn about it ended");
                self.release(&handover.lead, &handover.reference);
                continue;
            }
            let callback = self.lock().callbacks.get(handover.owner).cloned();
            tracing::info!(
                owner = handover.owner, child = %handover.child, epic = %handover.epic,
                "handed back by the lead"
            );
            match callback {
                Some(callback) => callback(handover).await,
                None => tracing::warn!(
                    owner = handover.owner,
                    "no hand-back registered for this handover"
                ),
            }
        }
    }
}

impl EpicLeads {
    /// Wakes the epic's lead (LEAD-04): `text` is queued as a `LeadWake` delivery on its live session, typed once
    /// it is idle with an empty input line, and the turn it starts gets `wake` as its reason. With no live lead
    /// nothing is queued and the reason is dropped. While the lead's tool is at its limit, the message waits for that tool's Resume.
    /// Whether it was queued.
    pub async fn wake(&self, epic: &str, wake: Wake, text: &str) -> Result<bool, BoardError> {
        Ok(self.queue_wake(epic, wake, text).await?.is_some())
    }

    /// Wakes the lead like [`wake`](Self::wake) and registers a handover for `child` (T-0284, T-0285), owned by
    /// `owner` with the owner's `reference`. Its id, or `None` when no lead is live: the caller then sends it to
    /// the user as today.
    pub async fn wake_with_handover(
        &self,
        epic: &str,
        wake: Wake,
        text: &str,
        owner: &'static str,
        child: &TicketKey,
        reference: &str,
    ) -> Result<Option<String>, BoardError> {
        // Registered before it is typed, so the turn it starts finds it.
        let Some(lead) = self.live_lead(epic).await? else {
            return Ok(None);
        };
        let handover = Handover {
            id: new_id(),
            owner,
            child: child.clone(),
            reference: reference.to_owned(),
            epic: lead.epic.clone(),
            lead: lead.agent.id.clone(),
            text: text.to_owned(),
            turn: None,
        };
        let id = handover.id.clone();
        self.handovers().register(handover);
        if self.queue_wake(epic, wake, text).await?.is_none() {
            self.handovers().settle(owner, reference);
            return Ok(None);
        }
        Ok(Some(id))
    }

    /// Wakes the lead after `child` merged or was closed (LEAD-04), as a handover of its own ([`FREED`], d0124):
    /// until the turn the wake-up is typed in ends, or the lead ends, `held` (ids of the epic's children that waited
    /// for `child`) don't start. With no live lead, or when the wake-up can't be queued, nothing waits. Whether it
    /// was queued.
    pub async fn wake_for_finished(
        &self,
        epic: &str,
        wake: Wake,
        text: &str,
        child: &Ticket,
        held: Vec<String>,
    ) -> Result<bool, BoardError> {
        let Some(lead) = self.live_lead(epic).await? else {
            tracing::debug!(epic, "no live lead to wake");
            return Ok(false);
        };
        let id = &lead.agent.id;
        // Before it is typed, so the turn's end always finds it.
        self.handovers().freed(id, &child.id, held);
        let woken = self
            .wake_with_handover(epic, wake, text, FREED, &child.key, &child.id)
            .await;
        if !matches!(woken, Ok(Some(_))) {
            self.handovers().release(id, &child.id);
            self.lead_changed(&lead);
        }
        Ok(woken?.is_some())
    }

    /// What each live lead's epic children wait for (d0124): its session's start and its open and ended turns about
    /// finished children.
    pub async fn lead_waits(&self) -> Result<Vec<LeadWait>, BoardError> {
        let mut waits = Vec::new();
        for (lead, epic, since) in self.live_leads().await? {
            waits.push(self.handovers().wait_of(&lead, epic, since));
        }
        Ok(waits)
    }

    /// The lead acted on a handover (answered, passed on, sent a note): it is no longer handed back. Whether it
    /// was open.
    pub fn settle_handover(&self, owner: &str, reference: &str) -> bool {
        self.handovers().settle(owner, reference)
    }

    /// Registers what hands back `owner`'s handovers (once per owner, at service start).
    pub fn on_hand_back(&self, owner: &'static str, callback: HandBack) {
        self.handovers().lock().callbacks.insert(owner, callback);
    }

    /// The open handovers of the epic's lead, oldest first.
    pub async fn handovers_of(&self, epic: &str) -> Result<Vec<Handover>, BoardError> {
        Ok(match self.get(epic).await? {
            Some(lead) => self.handovers().of(&lead.agent.id),
            None => Vec::new(),
        })
    }

    /// Types the wake-ups held during `tool`'s usage limit into every live lead on that tool that is idle (its
    /// Resume). Leads on other tools are left as they are.
    pub async fn deliver_held(&self, tool: Tool) -> Result<(), BoardError> {
        let default = self.default_tool();
        for (session, on) in self.live_sessions().await? {
            if on.unwrap_or(default) != tool {
                continue;
            }
            if let Err(e) = self.deliveries().deliver_if_ready(&session).await {
                tracing::warn!(session, error = %e, "can't type the lead's wake-up");
            }
        }
        Ok(())
    }

    /// Queues the wake-up on the live lead's session: its session id, or `None` with no live lead.
    async fn queue_wake(
        &self,
        epic: &str,
        wake: Wake,
        text: &str,
    ) -> Result<Option<String>, BoardError> {
        let Some(lead) = self.live_lead(epic).await? else {
            tracing::debug!(epic, "no live lead to wake");
            return Ok(None);
        };
        let session = lead
            .agent
            .session
            .as_ref()
            .map(|s| s.id.clone())
            .expect("a live lead has a session");
        self.set_turn_reason(epic, TurnReason::Wake { wake })
            .await?;
        self.set_next_woke(&lead.agent.id, text);
        let deliveries = self.deliveries();
        // Held only while the lead's own tool is at its limit (USG-10).
        let tool = lead.agent.session.as_ref().and_then(|s| s.tool);
        let queued = if self.limit_held_now(tool).await? {
            deliveries
                .hold_delivery(&session, DeliveryKind::LeadWake, text)
                .await
        } else {
            deliveries
                .queue_delivery(&session, DeliveryKind::LeadWake, text)
                .await
        };
        queued.map_err(|e| BoardError::refused(Refusal::SessionEnded, e.to_string()))?;
        tracing::info!(epic = %lead.epic, "lead woken");
        Ok(Some(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handover(id: &str, text: &str) -> Handover {
        Handover {
            id: id.into(),
            owner: "dummy",
            child: "SHOP-40.2".parse().unwrap(),
            reference: id.into(),
            epic: "SHOP-40".parse().unwrap(),
            lead: "lead".into(),
            text: text.into(),
            turn: None,
        }
    }

    #[test]
    fn a_handover_belongs_to_the_turn_its_wake_up_was_typed_in() {
        let h = Handovers::default();
        // Registered while the lead is busy: its wake-up waits.
        h.turn_started("lead", &[]);
        h.register(handover("7", "asks 7"));
        assert!(h.turn_ended("lead").is_empty(), "not typed in that turn");
        // The user's own turn starts before the wake-up is typed: still not this turn.
        h.turn_started("lead", &["asks 7".to_owned()]);
        assert!(h.turn_ended("lead").is_empty());
        // Typed: the next turn is its own, and its end hands it back.
        h.turn_started("lead", &[]);
        let back = h.turn_ended("lead");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].reference, "7");
        assert!(h.of("lead").is_empty());
    }

    #[test]
    fn a_settled_handover_isnt_handed_back_and_the_lead_ending_takes_the_rest() {
        let h = Handovers::default();
        h.register(handover("7", "asks 7"));
        h.register(handover("8", "asks 8"));
        h.turn_started("lead", &[]);
        assert!(h.settle("dummy", "7"));
        assert!(!h.settle("dummy", "7"), "once");
        assert!(!h.settle("other", "8"), "the owner's own");
        let back = h.turn_ended("lead");
        assert_eq!(
            back.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
            ["8"]
        );
        h.register(handover("9", "asks 9"));
        assert_eq!(h.lead_ended("lead").len(), 1);
        assert!(h.lead_ended("lead").is_empty());
    }

    /// d0124: a merge's handover releases its children at the end of its own turn, never by a callback; the lead
    /// ending forgets it and hands nothing of it back.
    #[tokio::test]
    async fn a_merge_turn_releases_what_it_held_back() {
        let h = Handovers::default();
        let epic: TicketKey = "SHOP-40".parse().unwrap();
        let mut merge = handover("m", "SHOP-40.1 merged: Wishlist API");
        merge.owner = FREED;
        merge.reference = "id-40.1".into();
        h.freed("lead", "id-40.1", vec!["id-40.3".into()]);
        h.register(merge.clone());
        let wait = h.wait_of("lead", epic.clone(), 7);
        assert_eq!(wait.freed, HashSet::from(["id-40.3".to_owned()]));
        assert!(wait.released.is_empty());

        // The user's own turn, while the wake-up still waits, isn't it.
        h.turn_started("lead", &[merge.text.clone()]);
        h.hand_back(h.turn_ended("lead")).await;
        assert!(!h.wait_of("lead", epic.clone(), 7).freed.is_empty());
        h.turn_started("lead", &[]);
        h.hand_back(h.turn_ended("lead")).await;
        let wait = h.wait_of("lead", epic.clone(), 7);
        assert!(wait.freed.is_empty());
        assert!(wait.released.contains_key("id-40.1"));

        h.freed("lead", "id-40.2", vec!["id-40.4".into()]);
        let mut close = handover("c", "SHOP-40.2 closed without merging: Export");
        close.owner = FREED;
        h.register(close);
        assert!(h.lead_ended("lead").is_empty(), "nothing to hand back");
        assert_eq!(
            h.wait_of("lead", epic, 7),
            LeadWait {
                epic: "SHOP-40".parse().unwrap(),
                since: 7,
                released: HashMap::new(),
                freed: HashSet::new(),
            }
        );
    }

    #[test]
    fn wake_up_texts() {
        let key = |k: &str| k.parse::<TicketKey>().unwrap();
        assert_eq!(
            merged_text(&key("SHOP-40.1"), "Wishlist API "),
            "SHOP-40.1 merged: Wishlist API"
        );
        assert_eq!(
            closed_text(&key("SHOP-40.6"), "Wishlist export (CSV)"),
            "SHOP-40.6 closed without merging: Wishlist export (CSV)"
        );
    }
}
