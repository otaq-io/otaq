//! The one Rust module that names Tauri (UIT-02), so a Tauri upgrade touches only this file and `ui/src/tauri.ts`.
//!
//! The surface is small and generic, so screens never edit it: one request call (a method name and JSON params in, a
//! JSON reply or a [`RequestError`] out, dispatched by [`crate::api`]), one event subscription ([`crate::events`]),
//! one output channel per terminal ([`crate::terminal`], answered with the attach's number), and the window: opening
//! it with its own title bar, the caption buttons, keeping its place ([`crate::window`]), the system folder picker and
//! folders dropped on the window (Add project). The WebView2 key switch ([`webview_keys_off`]) and macOS's text
//! substitutions ([`text_substitutions_off`]) are set up here too, and bringing the window back
//! ([`show_main_window`]) for a second start of the app ([`crate::one_copy`]), a reopen on macOS and a notification
//! click. The status icon in the menu bar / tray and its native menu ([`create_status_icon`], d0139) live here, as
//! do closing the window, which hides it ([`hide_main_window`]), and Otaq's own macOS app menu ([`app_menu`]).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tauri::State;
use tauri::ipc::Channel;

use crate::App;
use crate::api::RequestError;

type AppState<'a> = State<'a, Arc<App>>;

#[tauri::command]
async fn request(app: AppState<'_>, method: String, params: Value) -> Result<Value, RequestError> {
    crate::api::request(&app, &method, params).await
}

#[tauri::command]
fn subscribe_events(app: AppState<'_>, channel: Channel<Value>) {
    app.events
        .subscribe(Box::new(move |event| channel.send(event).is_ok()));
}

#[tauri::command]
async fn attach_terminal(
    app: AppState<'_>,
    session: String,
    channel: Channel<Value>,
    read_only: bool,
) -> Result<u64, String> {
    let sink = Box::new(move |feed| channel.send(feed).is_ok());
    crate::terminal::attach(&app, &session, !read_only, sink).await
}

/// The system folder picker (Add project, T-0233): the folder chosen, or `None` when cancelled.
#[tauri::command]
async fn pick_folder(app: tauri::AppHandle, title: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .pick_folder(move |folder| {
            let _ = tx.send(folder.and_then(|f| f.into_path().ok()));
        });
    rx.await
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

/// The system file picker for a flow file (Import flow…, T-0422): the file chosen, or `None` when cancelled.
#[tauri::command]
async fn pick_flow_file(app: tauri::AppHandle, title: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .add_filter("YAML", &["yaml", "yml"])
        .pick_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    rx.await
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

/// The system file picker for a report (Overview's Add, RPT-02; T-0465): the file chosen, or `None` when cancelled.
/// Any file: the service refuses what can't be a report (missing, over 20 MB) when the comment is posted.
#[tauri::command]
async fn pick_report_file(app: tauri::AppHandle, title: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().set_title(title).pick_file(move |file| {
        let _ = tx.send(file.and_then(|f| f.into_path().ok()));
    });
    rx.await
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

/// The system save dialog for a flow file (Export flow…, T-0422), offering `file_name`: where to save, or `None`
/// when cancelled.
#[tauri::command]
async fn save_flow_file(app: tauri::AppHandle, title: String, file_name: String) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .set_file_name(file_name)
        .add_filter("YAML", &["yaml", "yml"])
        .save_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    rx.await
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

/// The system save dialog for a report (the report view's Download, CMT-12), offering `file_name`: where to save, or
/// `None` when cancelled. The report is written there by `report.download`, from its file.
#[tauri::command]
async fn save_report_file(
    app: tauri::AppHandle,
    title: String,
    file_name: String,
) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title(title)
        .set_file_name(file_name)
        .save_file(move |file| {
            let _ = tx.send(file.and_then(|f| f.into_path().ok()));
        });
    rx.await
        .ok()
        .flatten()
        .map(|path| path.display().to_string())
}

// --- The window (T-0214) ---------------------------------------------------------------------------------------

/// The window's label, which `capabilities/default.json` names.
const MAIN_WINDOW: &str = "main";

#[tauri::command]
fn window(window: tauri::WebviewWindow, action: crate::window::Action) -> Result<(), String> {
    use crate::window::Action;
    match action {
        Action::Minimize => window.minimize(),
        Action::ToggleMaximize if window.is_maximized().unwrap_or(false) => window.unmaximize(),
        Action::ToggleMaximize => window.maximize(),
        Action::Close => window.close(),
        Action::Focus => bring_forward(&window),
    }
    .map_err(|e| e.to_string())
}

/// The window's place as last seen, saved when the app quits.
struct Placement {
    file: Option<PathBuf>,
    last: Mutex<Option<crate::window::Placement>>,
}

/// Opens the main window with its own title bar, where it was last time (d0074, UI-01).
fn open_window(app: &tauri::AppHandle, placement: &Placement) -> tauri::Result<()> {
    use crate::window::{MIN_SIZE, Rect};
    let saved = placement.file.as_deref().and_then(crate::window::load);
    let screens: Vec<Rect> = app
        .available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let (position, size) = (
                m.position().to_logical(m.scale_factor()),
                m.size().to_logical(m.scale_factor()),
            );
            Rect {
                x: position.x,
                y: position.y,
                width: size.width,
                height: size.height,
            }
        })
        .collect();
    let opening = crate::window::opening(saved, &screens);
    *placement.last.lock().unwrap_or_else(|e| e.into_inner()) = saved;

    let builder = tauri::WebviewWindowBuilder::new(app, MAIN_WINDOW, tauri::WebviewUrl::default())
        .title(otaq_core::naming::PRODUCT_NAME)
        .inner_size(opening.width, opening.height)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        // The webview's own zoom keys stay off (UIT-06).
        .zoom_hotkeys_enabled(false)
        // The terminal's paste reads the clipboard (`navigator.clipboard.readText()`): without this, WebView2 asks
        // "tauri.localhost wants to see text and images copied to the clipboard" on the first paste and that paste
        // is lost; WebKitGTK refuses the read. WKWebView needs nothing.
        .enable_clipboard_access();
    let builder = match opening.position {
        Some((x, y)) => builder.position(x, y),
        None => builder.center(),
    };
    // macOS: the page reaches the window's top edge and the traffic lights sit inset in its header row. Elsewhere:
    // no system title bar; the page draws the caption buttons.
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true)
        .traffic_light_position(tauri::LogicalPosition::new(
            crate::window::TRAFFIC_LIGHTS.0,
            crate::window::TRAFFIC_LIGHTS.1,
        ));
    #[cfg(not(target_os = "macos"))]
    let builder = builder.decorations(false);
    let window = builder.build()?;
    // After it is placed: maximized in the builder, it would keep the saved corner with the screen's size.
    if opening.maximized {
        window.maximize()?;
    }
    // Where it opened counts as its place until it moves.
    note_placement(&window.as_ref().window(), placement);
    Ok(())
}

/// Notes the window's place after each move or resize: its normal bounds stay as they were while it is maximized,
/// minimized or full screen.
fn note_placement(window: &tauri::Window, placement: &Placement) {
    let maximized = window.is_maximized().unwrap_or(false);
    let mut last = placement.last.lock().unwrap_or_else(|e| e.into_inner());
    if maximized
        || window.is_minimized().unwrap_or(false)
        || window.is_fullscreen().unwrap_or(false)
    {
        if let Some(last) = last.as_mut() {
            last.maximized = maximized;
        }
        return;
    }
    let (Ok(position), Ok(size), Ok(scale)) = (
        window.outer_position(),
        window.inner_size(),
        window.scale_factor(),
    ) else {
        return;
    };
    let (position, size) = (
        position.to_logical::<f64>(scale),
        size.to_logical::<f64>(scale),
    );
    *last = Some(crate::window::Placement {
        bounds: crate::window::Rect {
            x: position.x,
            y: position.y,
            width: size.width,
            height: size.height,
        },
        maximized: false,
    });
}

fn save_placement(placement: &Placement) {
    let last = *placement.last.lock().unwrap_or_else(|e| e.into_inner());
    if let (Some(file), Some(last)) = (&placement.file, last)
        && let Err(e) = crate::window::save(file, &last)
    {
        eprintln!(
            "couldn't keep the window's place in {}: {e}",
            file.display()
        );
    }
}

/// Shows the main window and puts it in front with the keyboard: a second start of the app (d0139 "Only one copy of
/// Otaq runs"), a reopen from the Dock, Finder or Spotlight, a notification click. Hidden or minimized, it comes back
/// where it was; closed (macOS keeps the app running without a window), it opens again, and its new page takes what to
/// open from `notices.open_at_start`. Call on the main thread.
fn show_main_window(handle: &tauri::AppHandle) -> tauri::Result<()> {
    use tauri::Manager;
    if let Some(window) = handle.get_webview_window(MAIN_WINDOW) {
        return bring_forward(&window);
    }
    handle.state::<Arc<App>>().notices.page_reloading();
    open_window(handle, &handle.state::<Arc<Placement>>())?;
    webview_keys_off(handle)?;
    match handle.get_webview_window(MAIN_WINDOW) {
        Some(window) => bring_forward(&window),
        None => Ok(()),
    }
}

/// Unminimizes, shows and focuses `window`; on macOS the app is unhidden too (⌘H), so the window isn't left behind
/// a hidden app.
/// macOS: back in the Dock and ⌘-Tab first (d0139). A first-hide note kept for the window shows now.
fn bring_forward(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    use tauri::Manager;
    #[cfg(target_os = "macos")]
    {
        let handle = window.app_handle();
        handle.set_activation_policy(tauri::ActivationPolicy::Regular)?;
        handle.show()?;
    }
    window.unminimize()?;
    window.show()?;
    window.set_focus()?;
    window.state::<Arc<App>>().notices.window_shown();
    Ok(())
}

/// Closing the main window hides it while the status icon is up (d0139): the process, its service link and the
/// icon stay. macOS: out of the Dock and ⌘-Tab until it shows again. The service hears of the first hide, for the
/// first-hide note (TRY-02).
fn hide_main_window(window: &tauri::Window) {
    use tauri::Manager;
    if let Err(e) = window.hide() {
        eprintln!("couldn't hide the window: {e}");
        return;
    }
    #[cfg(target_os = "macos")]
    if let Err(e) = window
        .app_handle()
        .set_activation_policy(tauri::ActivationPolicy::Accessory)
    {
        eprintln!(
            "couldn't take {} out of the Dock: {e}",
            otaq_core::naming::PRODUCT_NAME
        );
    }
    let app = window.state::<Arc<App>>().inner().clone();
    tauri::async_runtime::spawn(async move { crate::api::window_hidden(&app).await });
}

// --- The status icon (T-0566) -----------------------------------------------------------------------------------

/// The status icon's id.
const STATUS_ICON: &str = "status";

/// Whether the status icon was made. On Linux, closing hides the window only with it and a tray host ([`close_main_window`]).
#[derive(Default)]
struct StatusIconUp(AtomicBool);

/// Puts the icon in the menu bar / tray (plain, with the plain menu, until the service's summary arrives). Picking a
/// line opens it the way a notification click does (TRY-06); the actions are the summary's.
///
/// On Linux, tray-icon loads `libayatana-appindicator3` (or `libappindicator3`) when the first icon is made and
/// **panics** when neither loads (`libappindicator-sys`); the panic is caught here, so the app starts without the icon
/// and the no-tray rules apply (TRY-08). The panic comes from Rust code with no C frame above it.
fn create_status_icon(handle: &tauri::AppHandle) -> Result<(), String> {
    use otaq_view::view_models::status_icon::{IconLook, summary};
    let first = summary(IconLook::Plain, None, None);
    let builder = tauri::tray::TrayIconBuilder::with_id(STATUS_ICON)
        .icon(status_image(handle, &first).map_err(|e| e.to_string())?)
        .icon_as_template(cfg!(target_os = "macos"))
        .tooltip(&first.name)
        .menu(&status_menu(handle, &first).map_err(|e| e.to_string())?)
        .show_menu_on_left_click(true)
        .on_menu_event(|handle, event| status_menu_picked(handle, event.id().as_ref()));
    let _icon =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder.build(handle))) {
            Ok(built) => built.map_err(|e| e.to_string())?,
            Err(_) => return Err("the AppIndicator library couldn't be loaded".into()),
        };
    #[cfg(target_os = "macos")]
    accessible_name(&_icon, &first.name).map_err(|e| e.to_string())?;
    Ok(())
}

/// Closing the main window (the close button, ⌘W, Alt+F4, the page's own close button): with a tray it hides; on
/// Linux with no tray (checked now, TRY-08) it acts as Stop agents and quit, asking first while agents work
/// ([`otaq_view::view_models::no_tray::on_close`]).
fn close_main_window(window: &tauri::Window) {
    use crate::tray_host::{tray_shows, watcher_present};
    use otaq_view::view_models::no_tray::{OnClose, on_close};
    use tauri::Manager;
    let os = std::env::consts::OS;
    let icon_up = window.state::<StatusIconUp>().0.load(Ordering::SeqCst);
    if os != "linux" {
        hide_main_window(window);
        return;
    }
    let handle = window.app_handle().clone();
    let app = window.state::<Arc<App>>().inner().clone();
    tauri::async_runtime::spawn(async move {
        let tray = tokio::task::spawn_blocking(move || tray_shows(os, icon_up, watcher_present))
            .await
            .unwrap_or(false);
        let agents = if tray {
            0
        } else {
            crate::quit::agents_working(&app).await.unwrap_or(0)
        };
        match on_close(tray, agents) {
            OnClose::Hide => {
                let on_main = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if let Some(window) = on_main.get_webview_window(MAIN_WINDOW) {
                        hide_main_window(&window.as_ref().window());
                    }
                });
            }
            OnClose::StopAndQuit => stop_agents_and_quit(&handle),
            OnClose::Ask(question) => ask_before_stopping(&handle, question),
        }
    });
}

/// The question on close without a tray (page 31 item 9), as the OS's own alert on the window (GTK on Linux): Stop
/// and quit, or Cancel, which leaves the window open.
fn ask_before_stopping(
    handle: &tauri::AppHandle,
    question: otaq_view::view_models::no_tray::NoTrayQuestion,
) {
    let on_main = handle.clone();
    let _ = handle.run_on_main_thread(move || {
        let then = on_main.clone();
        ask_in_window(
            &on_main,
            &question.title,
            &question.text,
            vec![question.stop, question.cancel],
            move |picked| {
                if picked == Some(0) {
                    stop_agents_and_quit(&then);
                }
            },
        );
    });
}

/// A pick in the icon's menu: a line opens what it names, an action does what it says.
fn status_menu_picked(handle: &tauri::AppHandle, id: &str) {
    use otaq_core::agents::pause::PauseScope;
    use otaq_core::protocol::Request;
    use otaq_view::view_models::status_icon::{IconAction, MORE_ID, OPEN_PREFIX};
    if let Some(open) = id.strip_prefix(OPEN_PREFIX) {
        let open = open.to_owned();
        bring_back(handle, move |app, _| app.notices.hand_over(&open));
        return;
    }
    let pause = |request: Request| {
        use tauri::Manager;
        let app = handle.state::<Arc<App>>().inner().clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = app.service.send(request).await {
                eprintln!("the status icon's pause didn't go through: {e:?}");
            }
        });
    };
    match IconAction::from_id(id) {
        Some(IconAction::OpenOtaq) => bring_back(handle, |_, _| {}),
        // Pause all and Resume all work at once, with no question (PAU-06, All projects).
        Some(IconAction::PauseAll) => pause(Request::AgentsPause(PauseScope::All)),
        Some(IconAction::ResumeAll) => pause(Request::AgentsResume(PauseScope::All)),
        // Stop agents and quit stores the pause, stops every agent and the service, then quits (T-0569).
        Some(IconAction::StopAgentsAndQuit) => stop_agents_and_quit(handle),
        // Not connected: Quit Otaq quits the app at once (audit AS q2).
        Some(IconAction::Quit) => handle.exit(0),
        None if id == MORE_ID => bring_back(handle, |_, _| {}),
        None => {}
    }
}

/// The icon's native menu for `summary` (page 31): disabled headings and notes, a dot per line, separators before the
/// actions and before Stop agents and quit.
fn status_menu(
    handle: &tauri::AppHandle,
    summary: &otaq_view::view_models::status_icon::Summary,
) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use otaq_view::view_models::status_icon::{MenuRow, menu};
    use tauri::menu::{IconMenuItem, Menu, MenuItem, PredefinedMenuItem};
    let dark = dark_appearance(handle);
    let built = Menu::new(handle)?;
    for row in menu(summary) {
        match row {
            MenuRow::Text(text) => {
                built.append(&MenuItem::new(handle, text, false, None::<&str>)?)?
            }
            MenuRow::Open {
                id,
                label,
                dot: Some(dot),
            } => built.append(&IconMenuItem::with_id(
                handle,
                id,
                label,
                true,
                Some(dot_image(dot.rgba(dark))),
                None::<&str>,
            )?)?,
            MenuRow::Open {
                id,
                label,
                dot: None,
            } => built.append(&MenuItem::with_id(handle, id, label, true, None::<&str>)?)?,
            MenuRow::Action {
                action,
                label,
                enabled,
            } => built.append(&MenuItem::with_id(
                handle,
                action.id(),
                label,
                enabled,
                None::<&str>,
            )?)?,
            MenuRow::Separator => built.append(&PredefinedMenuItem::separator(handle)?)?,
        }
    }
    Ok(built)
}

/// Whether the OS draws dark menus now: the window's theme, which follows the OS's.
fn dark_appearance(handle: &tauri::AppHandle) -> bool {
    use tauri::Manager;
    handle
        .get_webview_window(MAIN_WINDOW)
        .and_then(|w| w.theme().ok())
        .is_some_and(|theme| theme == tauri::Theme::Dark)
}

/// A line's dot: a filled circle in `rgba` (`0xRRGGBBAA`), 32 px with an 11 px one in its middle, which the menu shows
/// at its icon size.
fn dot_image(rgba: u32) -> tauri::image::Image<'static> {
    const SIZE: u32 = 32;
    const RADIUS: f32 = 5.5;
    let [r, g, b, a] = rgba.to_be_bytes();
    let centre = SIZE as f32 / 2.0;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let (dx, dy) = (x as f32 + 0.5 - centre, y as f32 + 0.5 - centre);
            // Anti-aliased edge: full inside, fading over the last pixel.
            let cover = (RADIUS + 0.5 - (dx * dx + dy * dy).sqrt()).clamp(0.0, 1.0);
            pixels.extend_from_slice(&[r, g, b, (f32::from(a) * cover).round() as u8]);
        }
    }
    tauri::image::Image::new_owned(pixels, SIZE, SIZE)
}

/// The tray images (`icons/tray/`, drawn by `scripts/tray-icons.sh`): `(path under icons/tray, bytes)`, this
/// platform's only (build.rs).
const TRAY_IMAGES: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/tray_images.rs"));

/// The icon's image for `summary`: on macOS the look's template (the count is the item's text); on Windows and Linux
/// the look with its count drawn in, for the taskbar's light or dark, at 32 px on a 200 % display (and on Linux).
fn status_image(
    handle: &tauri::AppHandle,
    summary: &otaq_view::view_models::status_icon::Summary,
) -> tauri::Result<tauri::image::Image<'static>> {
    use otaq_view::view_models::status_icon::{badge_image, template_image};
    let name = if cfg!(target_os = "macos") {
        template_image(summary.look)
    } else {
        badge_image(summary.look, summary.count, taskbar(), tray_px(handle))
    };
    let bytes = TRAY_IMAGES
        .iter()
        .find(|(file, _)| *file == name)
        .map(|(_, bytes)| *bytes)
        .ok_or_else(|| std::io::Error::other(format!("no tray image {name}")))?;
    tauri::image::Image::from_bytes(bytes).map(|image| image.to_owned())
}

/// The tray's size: 32 px on a display at 150 % or more, and on Linux (its trays draw 22–24 px); 16 px otherwise.
fn tray_px(handle: &tauri::AppHandle) -> u32 {
    use tauri::Manager;
    if cfg!(target_os = "linux") {
        return 32;
    }
    let scale = handle
        .get_webview_window(MAIN_WINDOW)
        .and_then(|w| w.scale_factor().ok())
        .or_else(|| {
            handle
                .primary_monitor()
                .ok()
                .flatten()
                .map(|m| m.scale_factor())
        })
        .unwrap_or(1.0);
    if scale >= 1.5 { 32 } else { 16 }
}

/// The taskbar's light or dark: Windows' "Choose your default Windows mode" (`SystemUsesLightTheme`), dark when it
/// can't be read; Linux panels are taken as dark (GNOME's, KDE's and most others).
#[cfg(not(target_os = "macos"))]
fn taskbar() -> otaq_view::view_models::status_icon::Taskbar {
    use otaq_view::view_models::status_icon::Taskbar;
    #[cfg(windows)]
    {
        let light = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .open_subkey(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize")
            .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme"))
            .is_ok_and(|v| v == 1);
        if light {
            return Taskbar::Light;
        }
    }
    Taskbar::Dark
}

#[cfg(target_os = "macos")]
fn taskbar() -> otaq_view::view_models::status_icon::Taskbar {
    otaq_view::view_models::status_icon::Taskbar::Dark
}

/// Shows `summary` on the icon: its image, the count as its text on macOS, its name and its menu. Call on the main
/// thread. On macOS a menu that is open while it changes stays as it was (the status item shows a copy it tracks
/// on its own); the next click shows the new one.
fn show_status_icon(
    handle: &tauri::AppHandle,
    summary: &otaq_view::view_models::status_icon::Summary,
) -> tauri::Result<()> {
    let Some(icon) = handle.tray_by_id(STATUS_ICON) else {
        return Ok(());
    };
    icon.set_icon(Some(status_image(handle, summary)?))?;
    if cfg!(target_os = "macos") {
        icon.set_icon_as_template(true)?;
        icon.set_title(otaq_view::view_models::status_icon::title(summary))?;
    }
    icon.set_menu(Some(status_menu(handle, summary)?))?;
    #[cfg(target_os = "macos")]
    accessible_name(&icon, &summary.name)?;
    icon.set_tooltip(Some(&summary.name))
}

/// The status item's accessible name on macOS ("Otaq · 2 need you"), which VoiceOver reads; the tooltip is only its
/// hover text.
#[cfg(target_os = "macos")]
fn accessible_name(icon: &tauri::tray::TrayIcon, name: &str) -> tauri::Result<()> {
    let name = name.to_owned();
    icon.with_inner_tray_icon(move |inner| {
        use objc2_app_kit::NSAccessibility;
        let (Some(mtm), Some(item)) = (objc2::MainThreadMarker::new(), inner.ns_status_item())
        else {
            return;
        };
        if let Some(button) = item.button(mtm) {
            button.setAccessibilityLabel(Some(&objc2_foundation::NSString::from_str(&name)));
        }
    })
}

/// Keeps the icon on the service's summary ([`crate::status_icon::StatusIcon`]) for as long as the app runs.
fn follow_status_icon(handle: &tauri::AppHandle, app: Arc<App>, runtime: &tokio::runtime::Handle) {
    let handle = handle.clone();
    runtime.spawn(async move {
        let icon = crate::status_icon::StatusIcon::start(&app.service, app.events.clone());
        let mut changes = icon.changes();
        loop {
            let summary = changes.borrow_and_update().clone();
            let on_main = handle.clone();
            let _ = handle.run_on_main_thread(move || {
                if let Err(e) = show_status_icon(&on_main, &summary) {
                    eprintln!("couldn't update the status icon: {e}");
                }
            });
            if changes.changed().await.is_err() {
                return;
            }
        }
    });
}

// --- Otaq's own macOS app menu (T-0566) --------------------------------------------------------------------------

/// The app menu ([`crate::app_menu::macos`]) as native items: macOS's own items, and Otaq's Quit (⌘Q).
#[cfg(target_os = "macos")]
fn app_menu(handle: &tauri::AppHandle) -> tauri::Result<tauri::menu::Menu<tauri::Wry>> {
    use crate::app_menu::{Item, QUIT_ACCELERATOR, QUIT_ID};
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem as P, Submenu};
    let menu = Menu::new(handle)?;
    for (title, items) in crate::app_menu::macos() {
        let submenu = Submenu::new(handle, title, true)?;
        for item in items {
            let h = handle;
            match item {
                Item::Quit => submenu.append(&MenuItem::with_id(
                    h,
                    QUIT_ID,
                    format!("Quit {}", otaq_core::naming::PRODUCT_NAME),
                    true,
                    Some(QUIT_ACCELERATOR),
                )?)?,
                Item::About => submenu.append(&P::about(h, None, None)?)?,
                Item::Services => submenu.append(&P::services(h, None)?)?,
                Item::Hide => submenu.append(&P::hide(h, None)?)?,
                Item::HideOthers => submenu.append(&P::hide_others(h, None)?)?,
                Item::ShowAll => submenu.append(&P::show_all(h, None)?)?,
                Item::Undo => submenu.append(&P::undo(h, None)?)?,
                Item::Redo => submenu.append(&P::redo(h, None)?)?,
                Item::Cut => submenu.append(&P::cut(h, None)?)?,
                Item::Copy => submenu.append(&P::copy(h, None)?)?,
                Item::Paste => submenu.append(&P::paste(h, None)?)?,
                Item::SelectAll => submenu.append(&P::select_all(h, None)?)?,
                Item::Minimize => submenu.append(&P::minimize(h, None)?)?,
                Item::Zoom => submenu.append(&P::maximize(h, None)?)?,
                Item::Fullscreen => submenu.append(&P::fullscreen(h, None)?)?,
                Item::CloseWindow => submenu.append(&P::close_window(h, None)?)?,
                Item::Separator => submenu.append(&P::separator(h)?)?,
            }
        }
        menu.append(&submenu)?;
    }
    Ok(menu)
}

/// **Stop agents and quit** (TRY-07; [`crate::quit::stop_agents`]): no question; the app quits once the service has
/// stopped every agent and exited. If it didn't, the window comes back with "Otaq didn't quit": Try again · Open
/// SHOP-18 · Not now. Callable from any thread.
pub(crate) fn stop_agents_and_quit(handle: &tauri::AppHandle) {
    use otaq_view::view_models::quit::failed_stop;
    use tauri::Manager;
    let app = handle.state::<Arc<App>>().inner().clone();
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let failure = match crate::quit::stop_agents(&app).await {
            Ok(()) => return handle.exit(0),
            Err(failure) => failure,
        };
        eprintln!("Stop agents and quit didn't quit: {failure:?}");
        let view = failed_stop(&failure);
        let mut labels = vec![view.try_again.to_owned()];
        labels.extend(view.open.as_ref().map(|(label, _)| label.clone()));
        labels.push(view.not_now.to_owned());
        let open = view.open.map(|(_, key)| key);
        let on_main = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Err(e) = show_main_window(&on_main) {
                eprintln!("couldn't bring the window back: {e}");
            }
            let again = on_main.clone();
            ask_in_window(
                &on_main,
                &view.title,
                &view.text,
                labels,
                move |picked| match (picked, open) {
                    (Some(0), _) => stop_agents_and_quit(&again),
                    (Some(1), Some(key)) => second_start(&again, Some(key)),
                    _ => {}
                },
            );
        });
    });
}

/// ⌘Q (TRY-03): with agents working, the window asks "2 agents are working": Keep them working and quit (the app
/// only) · Stop agents and quit · Cancel; with none working, or no service to ask, the app quits at once. The Dock's
/// Quit and a logout end the app through macOS without asking (Tauri can't hold them), so a logout never waits.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn quit_asked(handle: &tauri::AppHandle) {
    use otaq_view::view_models::quit::quit_question;
    use tauri::Manager;
    let app = handle.state::<Arc<App>>().inner().clone();
    let handle = handle.clone();
    tauri::async_runtime::spawn(async move {
        let Some(question) = quit_question(crate::quit::agents_working(&app).await) else {
            return handle.exit(0);
        };
        let on_main = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            let labels = vec![
                question.keep.to_owned(),
                question.stop.to_owned(),
                question.cancel.to_owned(),
            ];
            let then = on_main.clone();
            ask_in_window(
                &on_main,
                &question.title,
                &question.text,
                labels,
                move |picked| match picked {
                    Some(0) => then.exit(0),
                    Some(1) => stop_agents_and_quit(&then),
                    _ => {}
                },
            );
        });
    });
}

/// A native alert on the main window with two or three buttons (`labels`, the last one cancels); `then` gets the
/// picked button's place, `None` when it was dismissed. Call on the main thread.
fn ask_in_window(
    handle: &tauri::AppHandle,
    title: &str,
    text: &str,
    labels: Vec<String>,
    then: impl FnOnce(Option<usize>) + Send + 'static,
) {
    use tauri::Manager;
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogResult};
    let buttons = match labels.as_slice() {
        [a, b] => MessageDialogButtons::OkCancelCustom(a.clone(), b.clone()),
        [a, b, c] => MessageDialogButtons::YesNoCancelCustom(a.clone(), b.clone(), c.clone()),
        _ => return then(None),
    };
    let mut dialog = handle.dialog().message(text).title(title).buttons(buttons);
    if let Some(window) = handle.get_webview_window(MAIN_WINDOW) {
        dialog = dialog.parent(&window);
    }
    let last = labels.len() - 1;
    dialog.show_with_result(move |result| {
        then(match result {
            MessageDialogResult::Custom(label) => labels.iter().position(|l| *l == label),
            MessageDialogResult::Yes | MessageDialogResult::Ok => Some(0),
            MessageDialogResult::No if last == 2 => Some(1),
            MessageDialogResult::No | MessageDialogResult::Cancel => Some(last),
        })
    });
}

/// [`show_main_window`] from any thread, then `then` with the app, on the main thread.
fn bring_back(handle: &tauri::AppHandle, then: impl FnOnce(&App, bool) + Send + 'static) {
    let on_main = handle.clone();
    let sent = handle.run_on_main_thread(move || {
        use tauri::Manager;
        let reopened = on_main.get_webview_window(MAIN_WINDOW).is_none();
        if let Err(e) = show_main_window(&on_main) {
            eprintln!("couldn't bring the window back: {e}");
        }
        then(&on_main.state::<Arc<App>>(), reopened);
    });
    if let Err(e) = sent {
        eprintln!("couldn't bring the window back: {e}");
    }
}

/// A second start's hand-over ([`crate::one_copy`]): the window comes back and opens the start's `OTAQ_OPEN` target
/// the way a notification click does.
fn second_start(handle: &tauri::AppHandle, open: Option<String>) {
    bring_back(handle, move |app, _| {
        if let Some(open) = open {
            app.notices.hand_over(&open);
        }
    });
}

/// A notification click (`open_from_notification`, from the service) brings the window back from Rust, so it works
/// while the window is hidden or closed and its page can't. A page that runs opens the target itself; a window opened
/// anew takes it as its start target.
fn follow_notification_clicks(handle: &tauri::AppHandle, app: &App) {
    let handle = handle.clone();
    app.events.subscribe(Box::new(move |event| {
        if event["kind"] == "service" && event["name"] == "open_from_notification" {
            let open = serde_json::from_value::<otaq_core::attention::notify::Target>(
                event["event"]["target"].clone(),
            )
            .ok()
            .map(|target| target.to_open_value());
            bring_back(&handle, move |app, reopened| {
                if let (true, Some(open)) = (reopened, open) {
                    app.notices.hand_over(&open);
                }
            });
        }
        true
    }));
}

// --- Running ----------------------------------------------------------------------------------------------------

/// Opens the window and runs until the app quits. Tauri's async calls run on the app's own `runtime`; the window's
/// place is kept in `placement` (none: not kept). `first_copy`, when this is the one copy of the app for its data
/// folder, takes later starts' hand-overs and brings the window back for each.
pub fn run(
    app: Arc<App>,
    runtime: tokio::runtime::Handle,
    placement: Option<PathBuf>,
    first_copy: Option<crate::one_copy::FirstCopy>,
) -> Result<(), String> {
    tauri::async_runtime::set(runtime.clone());
    // Before the webview exists, so WebKit reads them when it first checks text.
    #[cfg(target_os = "macos")]
    text_substitutions_off(&objc2_foundation::NSUserDefaults::standardUserDefaults());
    let placement = Arc::new(Placement {
        file: placement,
        last: Mutex::new(None),
    });
    let (on_window, on_exit) = (placement.clone(), placement.clone());
    let dropped_on = app.clone();
    let focus_of = app.clone();
    let exiting = app.clone();
    let clicks_of = app.clone();
    let icon_of = app.clone();
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu).on_menu_event(|handle, event| {
        if event.id() == crate::app_menu::QUIT_ID {
            quit_asked(handle);
        }
    });
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(app)
        .manage(placement)
        .manage(StatusIconUp::default())
        .invoke_handler(tauri::generate_handler![
            request,
            subscribe_events,
            attach_terminal,
            window,
            pick_folder,
            pick_flow_file,
            pick_report_file,
            save_flow_file,
            save_report_file
        ])
        .setup(move |tauri_app| {
            use tauri::Manager;
            let handle = tauri_app.handle();
            open_window(handle, &handle.state::<Arc<Placement>>())?;
            webview_keys_off(handle)?;
            follow_notification_clicks(handle, &clicks_of);
            // Linux without a tray library or host: logged, and closing the window quits as before (T-0570).
            match create_status_icon(handle) {
                Ok(()) => {
                    handle
                        .state::<StatusIconUp>()
                        .0
                        .store(true, Ordering::SeqCst);
                    follow_status_icon(handle, icon_of, &runtime);
                }
                Err(e) => eprintln!("couldn't show the status icon: {e}"),
            }
            #[cfg(target_os = "linux")]
            if !crate::tray_host::watcher_present() {
                eprintln!(
                    "no tray on this desktop ({} has no owner): closing the window stops agents and quits",
                    crate::tray_host::WATCHER
                );
            }
            if let Some(first_copy) = first_copy {
                let handle = handle.clone();
                first_copy.serve(&runtime, move |open| second_start(&handle, open));
            }
            Ok(())
        })
        .on_window_event(move |window, event| match event {
            tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) => {
                note_placement(window, &on_window)
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                save_placement(&on_window);
                // The close button, ⌘W, Alt+F4 and the page's own close button all come here.
                if window.label() == MAIN_WINDOW {
                    api.prevent_close();
                    close_main_window(window);
                }
            }
            // The window in front with the keyboard, or not (d0090: no OS notification while the user looks).
            tauri::WindowEvent::Focused(focused) => {
                let (app, focused) = (focus_of.clone(), *focused);
                tauri::async_runtime::spawn(async move {
                    crate::api::window_focused(&app, focused).await;
                });
            }
            // Files dropped on the window: pasted into the terminal under them, or Add project (d0099).
            tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, position }) => {
                let scale = window.scale_factor().unwrap_or(1.0);
                crate::drops::dropped(&dropped_on, paths, (position.x, position.y), scale);
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .map_err(|e| format!("the window couldn't open: {e}"))?
        .run(move |handle, event| {
            #[cfg(not(target_os = "macos"))]
            let _ = handle;
            match event {
                tauri::RunEvent::Exit => {
                    save_placement(&on_exit);
                    exiting.usage.flush_before_exit();
                }
                // Finder, Spotlight, the Dock or `open -a` on the running app: no second process starts, macOS asks
                // this one to reopen.
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { .. } => {
                    if let Err(e) = show_main_window(handle) {
                        eprintln!("couldn't bring the window back: {e}");
                    }
                }
                _ => {}
            }
        });
    Ok(())
}

/// The webview's own keys off (UIT-06), so the §21.3 map is the only one. On Windows, WebView2's browser accelerator
/// keys (Ctrl+F find, Ctrl+P print, F5 / Ctrl+R reload, zoom, Ctrl+Shift+I devtools, F7 caret browsing, Alt+← back…)
/// through `AreBrowserAcceleratorKeysEnabled = false`: Tauri doesn't expose this setting (wry has it, Tauri doesn't
/// pass it), so it goes through `webview2-com`; a Tauri upgrade means checking it again. WKWebView and WebKitGTK act on
/// none of these keys themselves; the zoom keys are off in the window builder ([`open_window`]) and the page swallows
/// the rest (`ui/src/app/keys.ts`).
fn webview_keys_off(tauri_app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    #[cfg(windows)]
    {
        use tauri::Manager;
        if let Some(window) = tauri_app.get_webview_window(MAIN_WINDOW) {
            window.with_webview(|webview| unsafe {
                use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3;
                use windows_core::Interface;
                if let Ok(settings) = webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|w| w.Settings())
                    .and_then(|s| s.cast::<ICoreWebView2Settings3>())
                {
                    let _ = settings.SetAreBrowserAcceleratorKeysEnabled(false);
                }
            })?;
        }
    }
    #[cfg(not(windows))]
    let _ = tauri_app;
    Ok(())
}

/// The defaults that switch macOS's text substitutions off: smart quotes, smart dashes, text replacement and
/// autocorrect, for the webview (`Web…`) and for native text fields (`NS…`). Titles, branch names and terminals take
/// code (`'--port'`), which they would otherwise turn into `‘—port’`, and typing "hi" in a terminal would offer "Hi"
/// (#41, #52).
#[cfg(target_os = "macos")]
const TEXT_SUBSTITUTIONS: [&str; 8] = [
    "WebAutomaticQuoteSubstitutionEnabled",
    "WebAutomaticDashSubstitutionEnabled",
    "WebAutomaticTextReplacementEnabled",
    "WebAutomaticSpellingCorrectionEnabled",
    "NSAutomaticQuoteSubstitutionEnabled",
    "NSAutomaticDashSubstitutionEnabled",
    "NSAutomaticTextReplacementEnabled",
    "NSAutomaticSpellingCorrectionEnabled",
];

/// Sets [`TEXT_SUBSTITUTIONS`] to NO in `defaults`. The app passes `standardUserDefaults`, whose writes go to the
/// app's own domain (its bundle id; the binary's name when run unbundled), never the global one: the user's
/// system-wide choice stays theirs for other apps, and the app's domain wins over it here. WebKit reads these keys
/// from the app's defaults when it first checks text, so they are set before the window opens. The page sets
/// `autocorrect`, `autocapitalize` and `spellcheck` on its fields too (`ui/src/components/plainText.ts`).
#[cfg(target_os = "macos")]
fn text_substitutions_off(defaults: &objc2_foundation::NSUserDefaults) {
    for key in TEXT_SUBSTITUTIONS {
        defaults.setBool_forKey(false, &objc2_foundation::NSString::from_str(key));
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use objc2::AllocAnyThread;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol};
    use objc2_foundation::{NSString, NSUserDefaults};

    use super::{TEXT_SUBSTITUTIONS, text_substitutions_off};

    /// A throwaway defaults domain (a suite), removed when the test ends however it ends.
    struct Suite {
        name: String,
        defaults: Retained<NSUserDefaults>,
    }

    impl Suite {
        fn new() -> Suite {
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let name = format!(
                "{}.test.door.{}.{nanos}",
                otaq_core::naming::BUNDLE_ID,
                std::process::id()
            );
            let defaults = NSUserDefaults::initWithSuiteName(
                NSUserDefaults::alloc(),
                Some(&NSString::from_str(&name)),
            )
            .expect("a suite's defaults");
            Suite { name, defaults }
        }
    }

    impl Drop for Suite {
        fn drop(&mut self) {
            // Through cfprefsd, which then writes nothing more for it.
            let _ = std::process::Command::new("/usr/bin/defaults")
                .args(["delete", &self.name])
                .output();
            // It leaves an empty file behind.
            if let Some(home) = std::env::var_os("HOME") {
                let file = std::path::Path::new(&home)
                    .join("Library/Preferences")
                    .join(format!("{}.plist", self.name));
                let _ = std::fs::remove_file(file);
            }
        }
    }

    /// The global domain's value of each key, read only.
    fn global_values() -> Vec<Option<Retained<AnyObject>>> {
        let global = NSUserDefaults::standardUserDefaults()
            .persistentDomainForName(&NSString::from_str("NSGlobalDomain"));
        TEXT_SUBSTITUTIONS
            .iter()
            .map(|key| global.as_ref()?.objectForKey(&NSString::from_str(key)))
            .collect()
    }

    fn same(a: &Option<Retained<AnyObject>>, b: &Option<Retained<AnyObject>>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => a
                .downcast_ref::<NSObject>()
                .is_some_and(|a| a.isEqual(Some(b))),
            _ => false,
        }
    }

    #[test]
    fn text_substitutions_are_written_off_to_the_given_domain_only() {
        let suite = Suite::new();
        let global_before = global_values();

        text_substitutions_off(&suite.defaults);
        assert!(suite.defaults.synchronize());

        // Every key is NO in the suite's own domain, and nothing else is there.
        let domain = suite
            .defaults
            .persistentDomainForName(&NSString::from_str(&suite.name))
            .expect("the suite's domain was written");
        assert_eq!(domain.count(), TEXT_SUBSTITUTIONS.len());
        for key in TEXT_SUBSTITUTIONS {
            let key = NSString::from_str(key);
            assert!(domain.objectForKey(&key).is_some(), "{key} is in the suite");
            assert!(!suite.defaults.boolForKey(&key), "{key} is NO");
        }
        // As the defaults tool sees the suite's domain (read only, this domain only).
        let read = std::process::Command::new("/usr/bin/defaults")
            .args(["read", &suite.name])
            .output()
            .expect("defaults read");
        let read = String::from_utf8_lossy(&read.stdout);
        for key in TEXT_SUBSTITUTIONS {
            assert!(read.contains(&format!("{key} = 0;")), "{key} in: {read}");
        }

        // The global domain is as it was.
        let global_after = global_values();
        for ((key, before), after) in TEXT_SUBSTITUTIONS
            .iter()
            .zip(&global_before)
            .zip(&global_after)
        {
            assert!(same(before, after), "{key} changed in the global domain");
        }
    }
}
