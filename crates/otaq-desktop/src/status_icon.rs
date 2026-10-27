//! The status icon's look and menu as the link to the service changes (d0139, TRY-04; the model is
//! [`otaq_view::view_models::status_icon`]). [`follow`] hands each new view to the door, which owns the native icon.

use std::time::Instant;

use otaq_view::view_models::status_icon::{IconLink, StatusIconView};

use crate::App;

/// Calls `show` with the icon's view at once and whenever it changes, until the link ends: on each link state, and
/// when a link that stays down passes the grace period.
pub async fn follow(app: &App, show: impl Fn(&StatusIconView)) {
    let mut states = app.service.states();
    let mut model = IconLink::default();
    let mut shown: Option<StatusIconView> = None;
    loop {
        let now = Instant::now();
        model.on_link(&states.borrow_and_update(), now);
        let view = model.view(now);
        if shown.as_ref() != Some(&view) {
            show(&view);
            shown = Some(view);
        }
        let changed = match model.changes_at(now) {
            Some(at) => {
                tokio::select! {
                    changed = states.changed() => changed,
                    () = tokio::time::sleep_until(at.into()) => Ok(()),
                }
            }
            None => states.changed().await,
        };
        if changed.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use otaq_view::view_models::status_icon::IconLook;

    use super::*;
    use crate::service::testing::{Harness, connected};

    /// T-0566 (TRY-04): the icon is plain while connected and turns not connected once the stopped service stays
    /// away past the grace period; back to plain when it returns.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_icon_follows_the_link() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let looks = Arc::new(Mutex::new(Vec::new()));
        let seen = looks.clone();
        let until = |want: IconLook| {
            let looks = looks.clone();
            async move {
                tokio::time::timeout(Duration::from_secs(20), async {
                    while looks.lock().unwrap().last() != Some(&want) {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                })
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "the icon never looked {want:?}: {:?}",
                        looks.lock().unwrap()
                    )
                });
            }
        };
        let steps = async {
            until(IconLook::Plain).await;
            h.service.stop_all().await;
            until(IconLook::NotConnected).await;
            h.service.bind().await.unwrap();
            until(IconLook::Plain).await;
        };
        tokio::select! {
            () = follow(&h.app, move |view| seen.lock().unwrap().push(view.look)) => panic!("the link ended"),
            () = steps => {}
        }
        assert_eq!(
            *looks.lock().unwrap(),
            [IconLook::Plain, IconLook::NotConnected, IconLook::Plain],
            "no flicker"
        );
        h.service.stop_all().await;
    }
}
