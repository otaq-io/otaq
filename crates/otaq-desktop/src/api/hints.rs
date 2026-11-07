//! `hints.*`: one-time hints. `hints.view` answers a hint's popover, or `null` once the user dismissed it;
//! `hints.dismiss` (its one button) dismisses it for good. The service keeps the dismissed ones per data folder.

use otaq_core::protocol::{HintParams, Reply, Request};
use otaq_view::view_models::hint::{self as vm, HintId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

use super::{RequestError, reply, unknown};
use crate::App;

/// `{"hint": "gallery_sample"}`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[ts(rename = "HintParams")]
pub struct Params {
    pub hint: HintId,
}

pub async fn request(app: &App, method: &str, params: Value) -> Result<Value, RequestError> {
    let Params { hint } =
        serde_json::from_value(params).map_err(|e| RequestError::plain(e.to_string()))?;
    match method {
        "hints.view" => {
            let seen = match app.service.send(Request::HintsSeen).await? {
                Reply::HintsSeen(seen) => seen,
                other => {
                    return Err(RequestError::plain(format!(
                        "unexpected reply to hints.seen: {other:?}"
                    )));
                }
            };
            reply(&vm::shown(hint, &seen, &super::keymap::labels(app).await))
        }
        "hints.dismiss" => {
            app.service
                .send(Request::HintDismiss(HintParams { hint: hint.into() }))
                .await?;
            Ok(Value::Null)
        }
        _ => Err(unknown(method)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::api::request;
    use crate::service::testing::{Harness, connected};

    /// A hint shows until dismissed; after the service restarts it stays dismissed.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_hint_shows_until_dismissed_and_stays_dismissed_after_a_restart() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let hint = json!({"hint": "gallery_sample"});
        let shown = request(&h.app, "hints.view", hint.clone()).await.unwrap();
        assert_eq!(shown["id"], "gallery_sample");
        assert_eq!(shown["button"], "Got it");

        assert_eq!(
            request(&h.app, "hints.dismiss", hint.clone()).await.unwrap(),
            Value::Null
        );
        assert_eq!(
            request(&h.app, "hints.view", hint.clone()).await.unwrap(),
            Value::Null
        );

        h.app.service.restart().await.unwrap();
        h.wait_for("connected", connected).await;
        assert_eq!(
            request(&h.app, "hints.view", hint.clone()).await.unwrap(),
            Value::Null,
            "still dismissed"
        );
        h.service.stop_all().await;

        // A fresh data folder shows it again.
        let fresh = Harness::new(None);
        fresh.wait_for("connected", connected).await;
        assert_eq!(
            request(&fresh.app, "hints.view", hint).await.unwrap()["id"],
            "gallery_sample"
        );
        fresh.service.stop_all().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_unknown_hint_is_refused() {
        let h = Harness::new(None);
        assert!(
            request(&h.app, "hints.view", json!({"hint": "nope"}))
                .await
                .is_err()
        );
        h.wait_for("connected", connected).await;
        h.service.stop_all().await;
    }
}
