//! One-time hints: a hint shows until the user dismisses it, then never again for this data folder, across restarts;
//! a new data folder shows it again.

use otaq_core::client::{Client, ClientConfig};
use otaq_core::protocol::{HintParams, Reply, Request};
use otaq_core::service::testing::TestService;
use otaq_core::store::settings::Hint;

async fn seen(w: &TestService) -> Vec<Hint> {
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    match client.request(Request::HintsSeen).await.unwrap() {
        Reply::HintsSeen(hints) => hints,
        other => panic!("unexpected reply {other:?}"),
    }
}

#[tokio::test]
async fn a_dismissed_hint_stays_dismissed_after_a_restart_and_a_new_data_folder_shows_it() {
    let w = TestService::start().await;
    assert_eq!(seen(&w).await, vec![], "nothing dismissed yet");

    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();
    assert_eq!(
        client
            .request(Request::HintDismiss(HintParams {
                hint: Hint::GallerySample
            }))
            .await
            .unwrap(),
        Reply::Ok
    );
    drop(client);
    assert_eq!(seen(&w).await, vec![Hint::GallerySample]);

    let w = w.restart().await;
    assert_eq!(seen(&w).await, vec![Hint::GallerySample], "after a restart");
    w.stop().await;

    let fresh = TestService::start().await;
    assert_eq!(seen(&fresh).await, vec![], "a new data folder");
    fresh.stop().await;
}
