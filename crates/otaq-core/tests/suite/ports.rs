//! The port pool (T-0046): two ports per ticket, idempotent, skipping held and busy ports, freed for reuse,
//! read back after a restart, and a typed error when the range runs out.

use std::net::TcpListener;
use std::ops::RangeInclusive;

use crate::common::Fixture;
use otaq_core::data_dir::DataDir;
use otaq_core::ports::{PortError, PortPool, TicketPorts, is_free_on_machine};
use otaq_core::store::Store;

/// A range of `len` ports that are all free right now, below the OS's ephemeral ports (macOS hands those out
/// from 49152, Linux from 32768), so connections made anywhere on the machine during the test can't take them.
/// Each call in this process gets its own block, so tests running side by side never share a port.
fn free_range(len: u16) -> RangeInclusive<u16> {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    // Spread processes (several `cargo test` runs) over 20000–31999.
    let base = 20_000 + (std::process::id() % 400) as u16 * 30;
    for _ in 0..200 {
        let block = NEXT.fetch_add(1, Ordering::SeqCst) % 400;
        let start = 20_000 + (base - 20_000 + block * 30) % 12_000;
        let end = start + len - 1;
        if (start..=end).all(is_free_on_machine) {
            return start..=end;
        }
    }
    panic!("no free port range found");
}

fn pool(f: &Fixture, range: RangeInclusive<u16>) -> PortPool {
    PortPool::with_range(f.board.store().clone(), range)
}

#[tokio::test]
async fn two_distinct_ports_per_ticket_and_idempotent() {
    let f = Fixture::shop().await;
    let range = free_range(6);
    let pool = pool(&f, range.clone());
    let a = f.create("SHOP-1").await;
    let b = f.create("SHOP-2").await;

    let pa = pool
        .allocate(&a.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    assert_ne!(pa.agent, pa.apps[0].1);
    assert!(range.contains(&pa.agent) && range.contains(&pa.apps[0].1));
    assert_eq!(
        pool.allocate(&a.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
            .await
            .unwrap(),
        pa,
        "same two again"
    );

    let pb = pool
        .allocate(&b.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    for port in [pb.agent, pb.apps[0].1] {
        assert!(
            port != pa.agent && port != pa.apps[0].1,
            "{port} is held by SHOP-1"
        );
    }
    // Lowest free first.
    assert_eq!(
        pa,
        TicketPorts {
            agent: *range.start(),
            apps: vec![("app".to_owned(), range.start() + 1)],
        }
    );
}

#[tokio::test]
async fn skips_a_port_bound_on_the_machine() {
    let f = Fixture::shop().await;
    let range = free_range(5);
    let busy = TcpListener::bind(("127.0.0.1", *range.start())).unwrap();
    let pool = pool(&f, range.clone());
    let t = f.create("one").await;
    let ports = pool
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    assert_ne!(ports.agent, *range.start());
    assert_ne!(ports.apps[0].1, *range.start());
    drop(busy);
}

#[tokio::test]
async fn freed_ports_are_reused() {
    let f = Fixture::shop().await;
    let pool = pool(&f, free_range(4));
    let a = f.create("a").await;
    let b = f.create("b").await;
    let c = f.create("c").await;
    let pa = pool
        .allocate(&a.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    pool.allocate(&b.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    assert!(matches!(
        pool.allocate(&c.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
            .await,
        Err(PortError::Exhausted { .. })
    ));

    pool.free(&a.id).await.unwrap();
    let row = f.board.store().ticket(&a.id).await.unwrap().unwrap();
    assert_eq!((row.agent_port, row.run_port), (None, None));
    assert_eq!(
        pool.allocate(&c.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
            .await
            .unwrap(),
        pa
    );
}

#[tokio::test]
async fn exhausted_is_a_typed_error() {
    let f = Fixture::shop().await;
    let t = f.create("t").await;
    // One port can never give a ticket two.
    let range = free_range(1);
    let err = pool(&f, range.clone())
        .allocate(&t.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap_err();
    assert!(matches!(&err, PortError::Exhausted { range: r } if *r == range));
    assert!(err.to_string().contains("no free ports left"), "{err}");
    let row = f.board.store().ticket(&t.id).await.unwrap().unwrap();
    assert_eq!(row.agent_port, None, "nothing stored");
}

#[tokio::test]
async fn allocations_survive_a_restart() {
    let f = Fixture::shop().await;
    let range = free_range(4);
    let a = f.create("a").await;
    let pa = pool(&f, range.clone())
        .allocate(&a.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();

    // A new store and pool on the same database, as after a service restart.
    let store = Store::open_in(&DataDir::new(f.home.path().join("home")))
        .await
        .unwrap();
    let restarted = PortPool::with_range(store, range);
    assert_eq!(
        restarted
            .allocate(&a.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
            .await
            .unwrap(),
        pa
    );
    let b = f.create("b").await;
    let pb = restarted
        .allocate(&b.id, &[otaq_core::board::settings::DEFAULT_APP.to_owned()])
        .await
        .unwrap();
    assert!(![pb.agent, pb.apps[0].1].contains(&pa.agent));
    assert!(![pb.agent, pb.apps[0].1].contains(&pa.apps[0].1));
}
