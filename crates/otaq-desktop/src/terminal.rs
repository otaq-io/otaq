//! Terminal output for the page (UIT-05, SVC-05): the door opens one channel per terminal and hands its sending end
//! here as a [`Sink`]; [`attach`] answers with the attach's number and keeps the channel fed in the background.
//!
//! **Attach with replay**, as the M14 test build did it: the task subscribes to the service's events *before* it asks
//! for the attach, sends the stored scrollback ([`Feed::Replay`]), then each live chunk cut to start exactly where the
//! shown output ends ([`Follow`]), so no byte shows twice or goes missing. While the connection is lost the page hears
//! [`Phase::Reconnecting`] and keeps its scrollback (SVC-04); once the service answers again the task attaches anew
//! and the page redraws from the fresh replay (also when the reconnect was so quick that the page never heard
//! about it). A gap in the live output (the event queue overflowed) reattaches too.
//!
//! Input, resize and focus are plain requests (`api/terminal.rs`); [`Terminals::detach`] ends an attach.
//!
//! **Answers to the program** (d0117 point 4). A read-only terminal doesn't answer the program's questions. The app
//! has one connection, so each attach tells the service whether *any* terminal of the page showing the session
//! answers (`quiet` when none does): while none does, the service answers the colour questions itself.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::client::{Client, ClientError, ConnectionState};
use otaq_core::protocol::{
    AttachParams, ErrorKind, Event, Reply, Request, SessionIdParams, SessionTarget, decode_bytes,
    encode_bytes,
};
use otaq_core::session::{SessionKind, SessionState};
use otaq_view::terminal::Phase;
use serde::Serialize;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use ts_rs::TS;

use crate::App;
use crate::events::Sink;

/// One message of a terminal's channel. `TerminalFeed` on the page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(rename = "TerminalFeed")]
pub enum Feed {
    /// Clear the terminal and write the session's stored output (base64; the bytes need not be UTF-8): the first
    /// attach, and again after a reconnect. `size` is the PTY's size while the session runs: the output's cursor
    /// moves were drawn for it, so the page writes the replay at that size and only then fits it to its box.
    Replay {
        data: String,
        size: Option<ReplaySize>,
    },
    /// Live output (base64), continuing exactly where the previous piece ended.
    Live { data: String },
    /// Where the terminal stands; it takes input only while `live`.
    Phase { phase: Phase },
}

/// The cells a replay was drawn in. `ReplaySize` on the page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub struct ReplaySize {
    pub cols: u16,
    pub rows: u16,
}

/// Where the output shown so far ends, in the session's whole output (the service numbers every byte it ever
/// printed): live chunks are cut to start exactly there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Follow {
    next: u64,
}

/// What became of one live chunk.
#[derive(Debug, PartialEq, Eq)]
pub enum Piece<'a> {
    /// Already shown (it arrived before the attach answer and the replay holds it).
    Old,
    /// The part not shown yet.
    New(&'a [u8]),
    /// Bytes before it are missing: attach again.
    Gap,
}

impl Follow {
    /// After a replay that ended at `end`.
    pub fn after_replay(end: u64) -> Self {
        Self { next: end }
    }

    pub fn live<'a>(&mut self, offset: u64, bytes: &'a [u8]) -> Piece<'a> {
        let end = offset + bytes.len() as u64;
        if end <= self.next {
            return Piece::Old;
        }
        if offset > self.next {
            return Piece::Gap;
        }
        let new = &bytes[(self.next - offset) as usize..];
        self.next = end;
        Piece::New(new)
    }
}

/// The page's attaches, by the number [`attach`] answered.
#[derive(Default)]
pub struct Terminals {
    next: AtomicU64,
    attached: Arc<Mutex<HashMap<u64, Attached>>>,
}

struct Attached {
    session: String,
    /// The terminal answers the program's questions (it isn't read-only).
    answers: bool,
    /// The session's kind once the service answered the attach.
    kind: Arc<Mutex<Option<SessionKind>>>,
    task: tokio::task::JoinHandle<()>,
}

impl Drop for Attached {
    fn drop(&mut self) {
        self.task.abort();
    }
}

impl Terminals {
    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<u64, Attached>> {
        self.attached.lock().expect("terminals")
    }

    /// Whether some attach of `session` shows an agent: only the agent's terminal counts for the focused-terminal
    /// rule (ATT-03); a Shell or a Project terminal is the user's own.
    pub fn is_agent(&self, session: &str) -> bool {
        self.lock().values().any(|a| {
            a.session == session && *a.kind.lock().expect("kind") == Some(SessionKind::Agent)
        })
    }

    /// Ends an attach: its channel gets nothing more. The service stops sending the session's output to this app
    /// once no other terminal of the page shows it. The session keeps running. When the last terminal of the
    /// session that answered the program goes and read-only ones stay, the service hears that this app's
    /// terminals no longer answer.
    pub async fn detach(&self, app: &App, attach: u64) {
        let request = {
            let mut attached = self.lock();
            let Some(gone) = attached.remove(&attach) else {
                return;
            };
            let session = gone.session.clone();
            let others: Vec<bool> = attached
                .values()
                .filter(|a| a.session == session)
                .map(|a| a.answers)
                .collect();
            if others.is_empty() {
                Some(Request::SessionDetach(SessionIdParams { session }))
            } else if gone.answers && !others.contains(&true) {
                Some(Request::SessionAttach(AttachParams {
                    target: SessionTarget {
                        session: Some(session),
                        ..SessionTarget::default()
                    },
                    quiet: true,
                }))
            } else {
                None
            }
        };
        if let Some(request) = request {
            let _ = app.service.send(request).await;
        }
    }
}

/// Attaches the page's terminal to `session`: `sink` gets a [`Feed`] per message, as JSON, from now until
/// [`Terminals::detach`]. Answers the attach's number at once; the scrollback follows on the channel.
/// `answers`: the terminal answers the program's questions (it isn't read-only).
pub async fn attach(app: &App, session: &str, answers: bool, sink: Sink) -> Result<u64, String> {
    if session.is_empty() {
        return Err("no terminal session given".into());
    }
    let number = app.terminals.next.fetch_add(1, Ordering::Relaxed) + 1;
    let kind = Arc::new(Mutex::new(None));
    // Asked at each (re)attach: quiet unless this terminal or another of the session answers.
    let quiet: Quiet = {
        let attached = app.terminals.attached.clone();
        let session = session.to_owned();
        Box::new(move || {
            !answers
                && !attached
                    .lock()
                    .expect("terminals")
                    .values()
                    .any(|a| a.session == session && a.answers)
        })
    };
    let task = tokio::spawn(follow(
        app.service.clients(),
        session.to_owned(),
        Feeder::new(sink),
        kind.clone(),
        quiet,
    ));
    app.terminals.lock().insert(
        number,
        Attached {
            session: session.to_owned(),
            answers,
            kind,
            task,
        },
    );
    Ok(number)
}

/// Whether the app's terminals of a session all stay silent to the program's questions, asked when attaching.
type Quiet = Box<dyn Fn() -> bool + Send + Sync>;

/// Sends feeds, leaving out a phase the page already has.
struct Feeder {
    sink: Sink,
    phase: Option<Phase>,
}

impl Feeder {
    fn new(sink: Sink) -> Self {
        Self { sink, phase: None }
    }

    /// False once the page behind the channel is gone.
    fn send(&mut self, feed: Feed) -> bool {
        if let Feed::Phase { phase } = &feed {
            if self.phase.as_ref() == Some(phase) {
                return true;
            }
            self.phase = Some(phase.clone());
        }
        (self.sink)(serde_json::to_value(&feed).expect("feeds serialize"))
    }

    fn phase(&mut self, phase: Phase) -> bool {
        self.send(Feed::Phase { phase })
    }
}

/// How long to wait before asking again when the service didn't answer although the link looked connected.
const RETRY: Duration = Duration::from_millis(200);

/// The attach task: attaches whenever the service is connected, streams until the connection drops or output goes
/// missing, and attaches again.
async fn follow(
    mut clients: watch::Receiver<Option<Client>>,
    session: String,
    mut feed: Feeder,
    kind: Arc<Mutex<Option<SessionKind>>>,
    quiet: Quiet,
) {
    feed.phase(Phase::Attaching);
    let client = loop {
        if let Some(client) = clients.borrow_and_update().clone() {
            break client;
        }
        if clients.changed().await.is_err() {
            return;
        }
    };
    let mut states = client.state();
    loop {
        while !matches!(*states.borrow_and_update(), ConnectionState::Connected(_)) {
            if !feed.phase(Phase::Reconnecting) || states.changed().await.is_err() {
                return;
            }
        }
        // Subscribe first: live output sent while the attach is answered waits in the receiver.
        let Ok(mut events) = client.subscribe().await else {
            tokio::time::sleep(RETRY).await;
            continue;
        };
        let attached = match client
            .request(Request::SessionAttach(AttachParams {
                target: SessionTarget {
                    session: Some(session.clone()),
                    ..SessionTarget::default()
                },
                quiet: quiet(),
            }))
            .await
        {
            Ok(Reply::SessionAttached(attached)) => attached,
            // Gone (ended and forgotten, or a service restart ended it): nothing more will come.
            Err(ClientError::Service(body)) if body.kind == ErrorKind::NotFound => {
                feed.phase(Phase::NoSession);
                return;
            }
            Ok(other) => {
                eprintln!("terminal {session}: unexpected answer to session.attach: {other:?}");
                feed.phase(Phase::NoSession);
                return;
            }
            Err(_) => {
                tokio::time::sleep(RETRY).await;
                continue;
            }
        };
        *kind.lock().expect("kind") = Some(attached.session.kind);
        let mut shown = Follow::after_replay(attached.end);
        let phase = if attached.session.state == SessionState::Ended {
            Phase::Ended
        } else {
            Phase::Live
        };
        if !feed.send(Feed::Replay {
            data: attached.scrollback,
            size: attached.session.size.map(|s| ReplaySize {
                cols: s.cols,
                rows: s.rows,
            }),
        }) || !feed.phase(phase)
        {
            return;
        }
        loop {
            tokio::select! {
                event = events.recv() => match event {
                    Ok(Event::SessionOutput { session: s, offset, data }) if s == session => {
                        let Ok(bytes) = decode_bytes(&data) else {
                            continue;
                        };
                        let data = match shown.live(offset, &bytes) {
                            Piece::Old => continue,
                            Piece::New(new) if new.len() == bytes.len() => data,
                            Piece::New(new) => encode_bytes(new),
                            Piece::Gap => break,
                        };
                        if !feed.send(Feed::Live { data }) {
                            return;
                        }
                    }
                    Ok(Event::SessionState { session: info })
                        if info.id == session && info.state == SessionState::Ended =>
                    {
                        if !feed.phase(Phase::Ended) {
                            return;
                        }
                    }
                    Ok(_) => {}
                    // Output went missing: attach again for a fresh replay.
                    Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return,
                },
                // Any change means the connection that carried the attach dropped, even when a quick reconnect
                // has already replaced it by now: the new connection sends no output until the terminal attaches
                // again.
                changed = states.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    break;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_chunks_are_cut_where_the_replay_ended() {
        let mut shown = Follow::after_replay(10);
        // Entirely inside the replay: dropped.
        assert_eq!(shown.live(4, b"456789"), Piece::Old);
        assert_eq!(shown.live(10, b""), Piece::Old);
        // Overlapping its end: only the new part.
        assert_eq!(shown.live(8, b"89ab"), Piece::New(b"ab"));
        // Straight after: whole.
        assert_eq!(shown.live(12, b"cd"), Piece::New(b"cd"));
        // A repeat of what was just shown: dropped.
        assert_eq!(shown.live(12, b"cd"), Piece::Old);
        // Something missing in between: attach again.
        assert_eq!(shown.live(20, b"kl"), Piece::Gap);
        assert_eq!(
            shown.live(14, b"ef"),
            Piece::New(b"ef"),
            "a gap changes nothing"
        );
    }

    #[test]
    fn the_page_reads_each_feed_by_its_kind() {
        let json = |f: Feed| serde_json::to_value(f).unwrap();
        assert_eq!(
            json(Feed::Replay {
                data: "aGk=".into(),
                size: Some(ReplaySize {
                    cols: 120,
                    rows: 40
                }),
            }),
            serde_json::json!({ "kind": "replay", "data": "aGk=", "size": { "cols": 120, "rows": 40 } })
        );
        assert_eq!(
            json(Feed::Phase {
                phase: Phase::Reconnecting
            }),
            serde_json::json!({ "kind": "phase", "phase": "reconnecting" })
        );
    }

    #[test]
    fn a_phase_the_page_has_is_not_sent_again() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let record = sent.clone();
        let mut feed = Feeder::new(Box::new(move |v| {
            record.lock().unwrap().push(v);
            true
        }));
        feed.phase(Phase::Reconnecting);
        feed.phase(Phase::Reconnecting);
        feed.send(Feed::Live {
            data: "eA==".into(),
        });
        feed.phase(Phase::Live);
        assert_eq!(sent.lock().unwrap().len(), 3);
    }

    /// A stand-in service on a real socket, so a test decides exactly when the connection drops: it answers
    /// status, subscribe and attach (with everything the session `s1` printed so far) and sends later output to
    /// the connection that attached.
    struct FakeService {
        state: Arc<Mutex<FakeState>>,
        _dir: tempfile::TempDir,
        socket: std::path::PathBuf,
        accept: tokio::task::JoinHandle<()>,
    }

    #[derive(Default)]
    struct FakeState {
        printed: Vec<u8>,
        connections: u32,
        attaches: u32,
        /// The open connection: its outgoing queue, whether it attached, and its tasks.
        open: Option<(
            tokio::sync::mpsc::UnboundedSender<Message>,
            bool,
            [tokio::task::JoinHandle<()>; 2],
        )>,
    }

    use otaq_core::protocol::{Body, FrameReader, Message, ServiceStatus, decode};
    use otaq_core::session::SessionInfo;

    impl FakeService {
        fn start() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let socket = otaq_core::data_dir::DataDir::new(dir.path()).socket_path();
            let mut listener = otaq_core::ipc::Listener::bind(&socket).unwrap();
            let state = Arc::new(Mutex::new(FakeState::default()));
            let shared = state.clone();
            let accept = tokio::spawn(async move {
                while let Ok(stream) = listener.accept().await {
                    let (read, mut write) = otaq_core::ipc::split(stream);
                    let (out, mut queued) = tokio::sync::mpsc::unbounded_channel::<Message>();
                    let writer = tokio::spawn(async move {
                        while let Some(message) = queued.recv().await {
                            if otaq_core::protocol::write_message(&mut write, &message)
                                .await
                                .is_err()
                            {
                                return;
                            }
                        }
                    });
                    let reader = tokio::spawn(Self::answer(shared.clone(), read, out.clone()));
                    let mut state = shared.lock().unwrap();
                    state.connections += 1;
                    state.open = Some((out, false, [reader, writer]));
                }
            });
            Self {
                state,
                _dir: dir,
                socket,
                accept,
            }
        }

        async fn answer(
            state: Arc<Mutex<FakeState>>,
            read: impl tokio::io::AsyncRead + Unpin,
            out: tokio::sync::mpsc::UnboundedSender<Message>,
        ) {
            let mut frames = FrameReader::new(tokio::io::BufReader::new(read));
            while let Ok(Some(frame)) = frames.next_frame().await {
                let Ok(Message {
                    id: Some(id),
                    body: Body::Request(request),
                    ..
                }) = decode(frame)
                else {
                    continue;
                };
                let reply = match request {
                    Request::ServiceStatus => Reply::ServiceStatus(ServiceStatus {
                        pid: std::process::id(),
                        build_id: otaq_core::protocol::build_id().to_owned(),
                        protocol_version: otaq_core::protocol::PROTOCOL_VERSION,
                        started_at: 0,
                        uptime_secs: 0,
                        agents_working: 0,
                        agents_waiting: 0,
                        unclean_start: false,
                        interrupted_at_start: 0,
                        paused: Vec::new(),
                        stopped_to_quit: None,
                    }),
                    Request::SessionAttach(_) => {
                        let mut state = state.lock().unwrap();
                        state.attaches += 1;
                        if let Some(open) = &mut state.open {
                            open.1 = true;
                        }
                        Reply::SessionAttached(Box::new(otaq_core::protocol::SessionAttach {
                            session: SessionInfo {
                                id: "s1".into(),
                                kind: SessionKind::Agent,
                                project: "SHOP".into(),
                                ticket: None,
                                state: SessionState::Running,
                                started_at: 0,
                                ended_at: None,
                                end_reason: None,
                                tool: None,
                                tool_session_id: None,
                                model: None,
                                agent: None,
                                effort: None,
                                fallback: None,
                                model_ran: None,
                                size: None,
                            },
                            scrollback: encode_bytes(&state.printed),
                            start: 0,
                            end: state.printed.len() as u64,
                        }))
                    }
                    _ => Reply::Ok,
                };
                let _ = out.send(Message::reply(id, reply));
            }
        }

        /// The session prints `bytes`; the connection sees them only if it attached.
        fn print(&self, bytes: &[u8]) {
            let mut state = self.state.lock().unwrap();
            let offset = state.printed.len() as u64;
            state.printed.extend_from_slice(bytes);
            if let Some((out, true, _)) = &state.open {
                let _ = out.send(Message::event(Event::SessionOutput {
                    session: "s1".into(),
                    offset,
                    data: encode_bytes(bytes),
                }));
            }
        }

        fn drop_connection(&self) {
            if let Some((_, _, tasks)) = self.state.lock().unwrap().open.take() {
                tasks.iter().for_each(|t| t.abort());
            }
        }

        fn counts(&self) -> (u32, u32) {
            let state = self.state.lock().unwrap();
            (state.connections, state.attaches)
        }
    }

    impl Drop for FakeService {
        fn drop(&mut self) {
            self.drop_connection();
            self.accept.abort();
        }
    }

    async fn until(what: &str, done: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(std::time::Instant::now() < deadline, "waited for {what}");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_reconnect_noticed_late_still_attaches_again() {
        let service = FakeService::start();
        service.print(b"one\r\n");
        let mut config =
            otaq_core::client::ClientConfig::new(&otaq_core::data_dir::DataDir::new("unused"));
        config.socket = service.socket.clone();
        config.backoff_min = Duration::from_millis(1);
        let client = Client::connect(config).await.unwrap();
        let (_clients, clients) = watch::channel(Some(client.clone()));

        // The page's terminal; while `busy` is set it takes its time drawing a live chunk.
        let shown = Arc::new(Mutex::new(Vec::<u8>::new()));
        let busy = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
        let drawing = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sink: Sink = Box::new({
            let (shown, busy, drawing) = (shown.clone(), busy.clone(), drawing.clone());
            move |v| {
                let bytes =
                    |v: &serde_json::Value| decode_bytes(v["data"].as_str().unwrap()).unwrap();
                match v["kind"].as_str() {
                    Some("replay") => *shown.lock().unwrap() = bytes(&v),
                    Some("live") => {
                        drawing.store(true, Ordering::SeqCst);
                        tokio::task::block_in_place(|| {
                            let (lock, wake) = &*busy;
                            let _held = wake.wait_while(lock.lock().unwrap(), |b| *b).unwrap();
                        });
                        shown.lock().unwrap().extend(bytes(&v));
                    }
                    _ => {}
                }
                true
            }
        });
        let task = tokio::spawn(follow(
            clients,
            "s1".into(),
            Feeder::new(sink),
            Arc::default(),
            Box::new(|| false),
        ));
        until("the first attach", || service.counts().1 == 1).await;

        // The terminal is busy with "two" while the connection drops and is back before it looks again.
        *busy.0.lock().unwrap() = true;
        service.print(b"two\r\n");
        until("drawing", || drawing.load(Ordering::SeqCst)).await;
        service.drop_connection();
        until("the new connection", || {
            service.counts().0 == 2 && client.is_connected()
        })
        .await;
        // Printed while the new connection isn't attached: only a fresh replay brings it.
        service.print(b"three\r\n");
        *busy.0.lock().unwrap() = false;
        busy.1.notify_all();

        until("the replay with everything", || {
            shown.lock().unwrap().as_slice() == b"one\r\ntwo\r\nthree\r\n"
        })
        .await;
        assert_eq!(service.counts(), (2, 2));
        task.abort();
    }
}
