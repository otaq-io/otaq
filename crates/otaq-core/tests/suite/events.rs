//! The event stream: subscribers receive events in order; a subscriber that falls behind is disconnected while
//! the others continue and publishing never blocks. Uses a temporary data folder.

use std::time::Duration;

use otaq_core::data_dir::DataDir;
use otaq_core::ipc::{self, ClientStream};
use otaq_core::protocol::{
    Body, Event, FrameReader, Message, Reply, Request, decode, write_message,
};
use otaq_core::service::{EventBus, Service, ServiceConfig, Stopper};
use tokio::io::{BufReader, ReadHalf, WriteHalf};

struct Running {
    data: DataDir,
    bus: EventBus,
    stopper: Stopper,
    task: tokio::task::JoinHandle<()>,
    _tmp: tempfile::TempDir,
}

async fn start(event_capacity: usize, client_queue: usize) -> Running {
    let tmp = tempfile::tempdir().unwrap();
    let data = DataDir::new(tmp.path().join("home"));
    let mut config = ServiceConfig::new(data.clone());
    config.event_capacity = event_capacity;
    config.client_queue = client_queue;
    let service = Service::bind(config).await.unwrap();
    let bus = service.events();
    let stopper = service.stopper();
    let task = tokio::spawn(service.serve());
    Running {
        data,
        bus,
        stopper,
        task,
        _tmp: tmp,
    }
}

struct Subscriber {
    reader: FrameReader<BufReader<ReadHalf<ClientStream>>>,
    _write: WriteHalf<ClientStream>,
}

impl Subscriber {
    async fn connect(data: &DataDir) -> Self {
        let stream = ipc::connect(&data.socket_path()).await.unwrap();
        let (read, mut write) = ipc::split(stream);
        write_message(
            &mut write,
            &Message::request(1, Request::Subscribe(Default::default())),
        )
        .await
        .unwrap();
        let mut sub = Self {
            reader: FrameReader::new(BufReader::new(read)),
            _write: write,
        };
        let reply = sub.next().await.unwrap();
        assert_eq!(reply.body, Body::Reply(Reply::Ok));
        sub
    }

    /// The next message, or `None` once the service closed the connection.
    async fn next(&mut self) -> Option<Message> {
        match self.reader.next_frame().await {
            Ok(Some(frame)) => Some(decode(frame).unwrap()),
            Ok(None) | Err(_) => None,
        }
    }

    async fn next_seq(&mut self) -> Option<u64> {
        match self.next().await?.body {
            Body::Event(Event::Test { seq }) => Some(seq),
            other => panic!("{other:?}"),
        }
    }
}

async fn wait_for_subscribers(bus: &EventBus, n: usize) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while bus.subscriber_count() != n {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("expected {n} subscribers, have {}", bus.subscriber_count()));
}

#[tokio::test]
async fn two_subscribers_receive_the_same_events_in_order() {
    let s = start(1024, 256).await;
    let mut a = Subscriber::connect(&s.data).await;
    let mut b = Subscriber::connect(&s.data).await;
    wait_for_subscribers(&s.bus, 2).await;

    for seq in 0..100 {
        s.bus.publish(Event::Test { seq });
    }
    for seq in 0..100 {
        assert_eq!(a.next_seq().await, Some(seq));
        assert_eq!(b.next_seq().await, Some(seq));
    }
    s.stopper.stop();
    s.task.await.unwrap();
}

#[tokio::test]
async fn a_lagging_subscriber_is_dropped_while_others_continue() {
    let s = start(64, 8).await;
    let mut fast = Subscriber::connect(&s.data).await;
    let mut slow = Subscriber::connect(&s.data).await;
    wait_for_subscribers(&s.bus, 2).await;

    // The slow client never reads; the fast one reads every event right away.
    const N: u64 = 20_000;
    for seq in 0..N {
        s.bus.publish(Event::Test { seq });
        assert_eq!(fast.next_seq().await, Some(seq));
    }
    wait_for_subscribers(&s.bus, 1).await;

    // The slow client gets what was already sent, then the end of the stream.
    let mut received = 0;
    while let Some(seq) = slow.next_seq().await {
        assert_eq!(seq, received, "events stay in order up to the cut");
        received += 1;
    }
    assert!(
        received < N,
        "the slow subscriber was cut off ({received} events)"
    );

    // The fast one keeps going.
    s.bus.publish(Event::Test { seq: N });
    assert_eq!(fast.next_seq().await, Some(N));
    s.stopper.stop();
    s.task.await.unwrap();
}

#[tokio::test]
async fn publishing_without_subscribers_is_fine() {
    let s = start(4, 4).await;
    for seq in 0..1000 {
        s.bus.publish(Event::Test { seq });
    }
    assert_eq!(s.bus.subscriber_count(), 0);
    s.stopper.stop();
    s.task.await.unwrap();
}
