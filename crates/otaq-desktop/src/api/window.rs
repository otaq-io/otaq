//! `window.*`: what the page needs to draw the window's own title bar (UI-01). The window operations themselves
//! (caption buttons, dragging) are Tauri calls and go through the door. The window's keyboard focus goes to the
//! service as `app.focus` (d0090: no OS notification while the user looks at Otaq).

use std::sync::atomic::{AtomicBool, Ordering};

use otaq_core::protocol::{AppFocusParams, Request};
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
}

/// The window gained or lost keyboard focus (Tauri's `Focused` event; minimized, hidden or behind another app counts
/// as lost): told to the service as `app.focus`, and kept for the next connection ([`follow_window_focus`]).
pub async fn window_focused(app: &App, focused: bool) {
    app.window_focus.focused.store(focused, Ordering::SeqCst);
    // Not connected: the connection, once made, is told ([`follow_window_focus`]).
    let _ = send_focus(app).await;
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
}
