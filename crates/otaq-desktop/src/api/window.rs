//! `window.*`: what the page needs to draw the window's own title bar (UI-01). The window operations themselves
//! (caption buttons, dragging) are Tauri calls and go through the door. The window's keyboard focus goes to the
//! service as `app.focus` (d0090: no OS notification while the user looks at Otaq). The window hiding goes to the service
//! as `app.first_hide`, for the first-hide note (TRY-02, d0139).

use std::sync::atomic::{AtomicBool, Ordering};

use otaq_core::attention::os_permission::FirstHide;
use otaq_core::protocol::{AppFocusParams, Reply, Request};
use serde_json::Value;

use super::{RequestError, reply, unknown};
use crate::App;
use otaq_view::view_models::connection::LinkState;
use crate::window::Chrome;

pub async fn request(_app: &App, method: &str, _params: Value) -> Result<Value, RequestError> {
    match method {
        "window.chrome" => reply(&Chrome::current()),
        _ => Err(unknown(method)),
    }
}

/// Whether the window has keyboard focus, as last reported, and the one send at a time that tells the service.
#[derive(Debug, Default)]
pub struct WindowFocus {
    focused: AtomicBool,
    sending: tokio::sync::Mutex<()>,
    /// The service heard about a hide during this run: the first-hide note is its business from then on.
    hide_told: AtomicBool,
}

/// The window gained or lost keyboard focus (Tauri's `Focused` event; minimized, hidden or behind another app counts
/// as lost): told to the service as `app.focus`, and kept for the next connection ([`follow_window_focus`]).
pub async fn window_focused(app: &App, focused: bool) {
    app.window_focus.focused.store(focused, Ordering::SeqCst);
    // Not connected: the connection, once made, is told ([`follow_window_focus`]).
    let _ = send_focus(app).await;
}

/// The window hid (closing hides it, d0139): the first time in this run, the service is told (`app.first_hide`), so
/// the first-hide note goes out once per data folder. A note the OS can't show is kept for the window. Not connected:
/// the next hide tells it.
pub async fn window_hidden(app: &App) {
    if app.window_focus.hide_told.swap(true, Ordering::SeqCst) {
        return;
    }
    match app.service.send(Request::AppFirstHide).await {
        Ok(Reply::FirstHide(FirstHide::InWindow { title, body })) => {
            app.notices.keep_first_hide_note(title, body);
        }
        Ok(_) => {}
        Err(_) => app.window_focus.hide_told.store(false, Ordering::SeqCst),
    }
}

/// Sends the focus as it is when its turn comes, so reports that cross each other end on the latest.
async fn send_focus(app: &App) -> Result<(), RequestError> {
    let _turn = app.window_focus.sending.lock().await;
    let focused = app.window_focus.focused.load(Ordering::SeqCst);
    app.service
        .send(Request::AppFocus(AppFocusParams { focused }))
        .await
        .map(drop)
}

/// Tells every new connection to the service whether the window has focus: the service forgets it with the
/// connection that told it (a restart, a reconnect). Runs as long as the link.
pub async fn follow_window_focus(app: &App) {
    let mut states = app.service.states();
    let mut connected = false;
    loop {
        let now = matches!(*states.borrow_and_update(), LinkState::Connected(_));
        if now && !connected {
            let _ = send_focus(app).await;
        }
        connected = now;
        if states.changed().await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::service::testing::{Harness, connected};

    async fn until_looking(h: &Harness, want: bool) {
        tokio::time::timeout(Duration::from_secs(10), async {
            while h.service.needs_you().looking() != want {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("the service never saw the window focused = {want}"));
    }

    /// T-0371 (d0090): the window's focus and blur reach the service as `app.focus`; a restarted service hears it
    /// again from the next connection.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_window_focus_reaches_the_service() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        let steps = async {
            window_focused(&h.app, true).await;
            until_looking(&h, true).await;
            window_focused(&h.app, false).await;
            until_looking(&h, false).await;

            // Focused, then the service restarts: the new connection tells the new service.
            window_focused(&h.app, true).await;
            until_looking(&h, true).await;
            h.service.stop_all().await;
            h.service.bind().await.unwrap();
            until_looking(&h, true).await;
        };
        tokio::select! {
            () = follow_window_focus(&h.app) => panic!("the link ended"),
            () = steps => {}
        }
        h.service.stop_all().await;
    }

    /// T-0566 (TRY-02, d0139): the first hide of a run tells the service, which posts the note once per data folder;
    /// later hides tell nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_first_hide_tells_the_service_once() {
        let h = Harness::new(None);
        h.wait_for("connected", connected).await;
        window_hidden(&h.app).await;
        window_hidden(&h.app).await;
        // The service's notifier posts nothing in tests; its flag says the note went out (or was kept).
        let answer = h.app.service.send(Request::AppFirstHide).await.unwrap();
        assert!(
            matches!(answer, Reply::FirstHide(FirstHide::Shown)),
            "{answer:?}"
        );
        h.service.stop_all().await;
    }

}
