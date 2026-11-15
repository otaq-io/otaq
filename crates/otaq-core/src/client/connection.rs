//! One open connection to the service: requests are matched to their answers by id, events are passed on.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use tokio::io::{BufReader, ReadHalf, WriteHalf};
use tokio::sync::{broadcast, oneshot, watch};

use super::ClientError;
use crate::ipc::{self, ClientStream};
use crate::protocol::{
    Body, DecodeError, ErrorBody, Event, FrameReader, Message, Reply, Request, decode,
    encode_within_limit, write_frame,
};

type Answer = Result<Reply, ErrorBody>;

pub(super) struct Connection {
    writer: tokio::sync::Mutex<WriteHalf<ClientStream>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Answer>>>>,
    next_id: AtomicU64,
    /// Recorded as each request's requested actor (`--actor`).
    actor: Option<String>,
    closed: watch::Receiver<bool>,
    reader: tokio::task::JoinHandle<()>,
}

impl Connection {
    /// Connects; events that arrive on this connection are sent to `events`.
    pub(super) async fn open(
        socket: &std::path::Path,
        events: broadcast::Sender<Event>,
        actor: Option<String>,
    ) -> Result<Self, ClientError> {
        let stream = ipc::connect(socket)
            .await
            .map_err(ClientError::NotRunning)?;
        let (read, write) = ipc::split(stream);
        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Answer>>>> = Arc::default();
        let (closed_tx, closed) = watch::channel(false);
        let reader = tokio::spawn(read_loop(read, pending.clone(), events, closed_tx));
        Ok(Self {
            writer: tokio::sync::Mutex::new(write),
            pending,
            next_id: AtomicU64::new(1),
            actor,
            closed,
            reader,
        })
    }

    pub(super) fn is_closed(&self) -> bool {
        *self.closed.borrow()
    }

    /// Resolves when the connection has ended.
    pub(super) async fn closed(&self) {
        let mut closed = self.closed.clone();
        let _ = closed.wait_for(|c| *c).await;
    }

    pub(super) async fn request(&self, request: Request) -> Result<Reply, ClientError> {
        if self.is_closed() {
            return Err(ClientError::Disconnected);
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let message = Message {
            caller: self.actor.clone(),
            ..Message::request(id, request)
        };
        // Too long for one frame: refused here in words, never sent (the service would only drop the connection).
        let frame = encode_within_limit(&message).map_err(ClientError::TooLarge)?;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().expect("pending map").insert(id, tx);
        let sent = {
            let mut writer = self.writer.lock().await;
            write_frame(&mut *writer, &frame).await
        };
        if sent.is_err() {
            self.pending.lock().expect("pending map").remove(&id);
            return Err(ClientError::Disconnected);
        }
        match rx.await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(error)) => Err(ClientError::Service(error)),
            // The reader ended before the answer came.
            Err(_) => Err(ClientError::Disconnected),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

async fn read_loop(
    read: ReadHalf<ClientStream>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Answer>>>>,
    events: broadcast::Sender<Event>,
    closed: watch::Sender<bool>,
) {
    let mut reader = FrameReader::new(BufReader::new(read));
    while let Ok(Some(frame)) = reader.next_frame().await {
        let message = match decode(frame) {
            Ok(message) => message,
            // Another protocol version: only the version-independent answers can be read.
            Err(DecodeError::IncompatibleVersion(foreign)) => match foreign.read_as_current() {
                Some(message) => message,
                None => continue,
            },
            Err(e) => {
                tracing::warn!(error = %e, "unreadable message from the service");
                continue;
            }
        };
        let answer = match message.body {
            Body::Event(event) => {
                let _ = events.send(event);
                continue;
            }
            Body::Reply(reply) => Ok(reply),
            Body::Error(error) => Err(error),
            Body::Request(_) => continue,
        };
        let waiting = message
            .id
            .and_then(|id| pending.lock().expect("pending map").remove(&id));
        if let Some(waiting) = waiting {
            let _ = waiting.send(answer);
        }
    }
    // Dropping the waiting senders fails their requests with Disconnected.
    pending.lock().expect("pending map").clear();
    closed.send_replace(true);
}
