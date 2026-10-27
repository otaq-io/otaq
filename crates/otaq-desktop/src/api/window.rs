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
/// the first-hide note goes out once per data folder. A note the OS can't show is kept for the window
/// ([`keep_first_hide_note`]). Not connected: the next hide tells it.
pub async fn window_hidden(app: &App) {
    app.notices.window_hid();
    if app.window_focus.hide_told.swap(true, Ordering::SeqCst) {
        return;
    }
    match app.service.send(Request::AppFirstHide).await {
        Ok(Reply::FirstHide(FirstHide::InWindow { title, body })) => {
            keep_first_hide_note(app, title, body).await;
        }
        Ok(_) => {}
        Err(_) => app.window_focus.hide_told.store(false, Ordering::SeqCst),
    }
}

/// The window was shown: a kept first-hide note shows now, and the service hears that it was seen.
pub async fn window_shown(app: &App) {
    if app.notices.window_shown() {
        first_hide_note_seen(app).await;
    }
}

/// The note goes to the window: shown now if the window shows, else when it shows next.
async fn keep_first_hide_note(app: &App, title: String, body: String) {
    if app.notices.keep_first_hide_note(title, body) {
        first_hide_note_seen(app).await;
    }
}

/// Tells the service the window showed the note, so it never comes back. Not reached: it comes once more.
async fn first_hide_note_seen(app: &App) {
    let _ = app.service.send(Request::AppFirstHideShown).await;
}

/// A note kept for the window by an earlier run (its app quit before the window showed it), asked on each connection.
async fn first_hide_pending(app: &App) {
    if let Ok(Reply::FirstHide(FirstHide::InWindow { title, body })) =
        app.service.send(Request::AppFirstHidePending).await
    {
        keep_first_hide_note(app, title, body).await;
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
/// connection that told it (a restart, a reconnect), and asks it for a first-hide note waiting for the window. Runs as
/// long as the link.
pub async fn follow_window_focus(app: &App) {
    let mut states = app.service.states();
    let mut connected = false;
    loop {
        let now = matches!(*states.borrow_and_update(), LinkState::Connected(_));
        if now && !connected {
            let _ = send_focus(app).await;
            first_hide_pending(app).await;
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

    /// A notifier that allows Otaq's notifications and counts what it posts.
    #[derive(Default)]
    struct Counting(std::sync::atomic::AtomicUsize);

    impl otaq_core::attention::notify::Notifier for Counting {
        fn post(
            &self,
            _n: &otaq_core::attention::notify::OsNotification,
            _on_click: otaq_core::attention::notify::OnClick,
        ) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }

        fn permission(&self) -> otaq_core::attention::notify::PermissionState {
            otaq_core::attention::notify::PermissionState::Allowed
        }
    }

    /// T-0566 (TRY-02, d0139): the first hide of a run tells the service, which posts the note once per data folder;
    /// later hides tell nothing.
    #[tokio::test(flavor = "multi_thread")]
    async fn the_first_hide_tells_the_service_once() {
        let counting = std::sync::Arc::new(Counting::default());
        let notifier = counting.clone();
        let h = Harness::configured(move |config| config.notifier = notifier.clone());
        h.wait_for("connected", connected).await;
        window_hidden(&h.app).await;
        window_hidden(&h.app).await;
        assert_eq!(counting.0.load(Ordering::SeqCst), 1);
        assert_eq!(pending(&h).await, FirstHide::Shown);
        h.service.stop_all().await;
    }


    /// A notifier like macOS's before it was ever asked: it posts nothing.
    struct NeverAsked;

    impl otaq_core::attention::notify::Notifier for NeverAsked {
        fn post(
            &self,
            _n: &otaq_core::attention::notify::OsNotification,
            _on_click: otaq_core::attention::notify::OnClick,
        ) {
        }

        fn permission(&self) -> otaq_core::attention::notify::PermissionState {
            otaq_core::attention::notify::PermissionState::NotAsked
        }
    }

    async fn toasts_soon(h: &Harness, n: usize) -> usize {
        for _ in 0..100 {
            if h.app.notices.view().toasts.len() >= n {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        h.app.notices.view().toasts.len()
    }

    async fn pending(h: &Harness) -> FirstHide {
        match h.app.service.send(Request::AppFirstHidePending).await.unwrap() {
            Reply::FirstHide(answer) => answer,
            other => panic!("{other:?}"),
        }
    }

    /// T-0571's lost note (T-0566, audit AS q4): with notifications never asked for, the first hide's note waits for
    /// the window: shown when the window comes back, even when it came back before the service answered, and only
    /// then counted as seen.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_note_the_os_cant_show_reaches_the_window() {
        let h = Harness::configured(|config| config.notifier = std::sync::Arc::new(NeverAsked));
        h.wait_for("connected", connected).await;
        // The window hides, and comes back before the service's answer is in.
        tokio::join!(window_hidden(&h.app), async {
            tokio::task::yield_now().await;
            window_shown(&h.app).await;
        });
        assert_eq!(toasts_soon(&h, 1).await, 1, "shown at once");
        // Later hides and shows bring nothing more.
        window_hidden(&h.app).await;
        window_shown(&h.app).await;
        assert_eq!(h.app.notices.view().toasts.len(), 1);
        assert_eq!(pending(&h).await, FirstHide::Shown, "seen: never again");
        h.service.stop_all().await;
    }

    /// The app quit while the note waited: the next app's window shows it when it connects.
    #[tokio::test(flavor = "multi_thread")]
    async fn a_note_waiting_from_an_earlier_run_shows_on_connect() {
        let h = Harness::configured(|config| config.notifier = std::sync::Arc::new(NeverAsked));
        h.wait_for("connected", connected).await;
        // An earlier run hid its window and quit.
        let answer = h.app.service.send(Request::AppFirstHide).await.unwrap();
        assert!(matches!(answer, Reply::FirstHide(FirstHide::InWindow { .. })));
        tokio::select! {
            () = follow_window_focus(&h.app) => panic!("the link ended"),
            n = toasts_soon(&h, 1) => assert_eq!(n, 1),
        }
        assert_eq!(pending(&h).await, FirstHide::Shown);
        h.service.stop_all().await;
    }

}
