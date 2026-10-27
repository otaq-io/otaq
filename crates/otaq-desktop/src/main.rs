//! The Otaq desktop app on Tauri 2 (d0074, d0075): a window whose page is React with TypeScript, built with Vite
//! from `ui/`. Only [`door`] names Tauri (UIT-02); everything else here is plain Rust the door calls into.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod api;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod app_menu;
mod door;
mod drops;
mod events;
mod fonts;
#[cfg(test)]
mod generated;
mod install_place;
mod notices;
mod notify_check;
mod one_copy;
mod quit;
mod service;
mod shell_path;
mod status_icon;
#[cfg(test)]
mod stylesheet;
mod terminal;
mod tray_host;
mod updates;
mod usage;
mod window;

use std::sync::Arc;

use events::EventHub;
use otaq_core::client::CommandLauncher;
use otaq_core::data_dir::DataDir;
use service::{ServiceLink, Settings};

/// What the door hands to every call from the page.
pub struct App {
    events: Arc<EventHub>,
    service: ServiceLink,
    /// The rail's switcher, kept between requests so a project that newly needs you rings (T-0219).
    switcher: std::sync::Mutex<otaq_view::view_models::switcher::SwitcherModel>,
    /// Each project's last-used tab of the Terminals drawer, for Terminals (⌃`; T-0242, T-0331).
    drawer_tabs: std::sync::Mutex<std::collections::BTreeMap<String, String>>,
    /// The Files tabs' viewed ticks, folded summaries and Unified / Side-by-side, while the app runs (T-0225).
    review: std::sync::Mutex<otaq_view::view_models::diff::ReviewMemory>,
    /// The sheet's Files counts, taken on open and after a turn ends, not on every refresh (Q421).
    files_counts: std::sync::Arc<std::sync::Mutex<otaq_view::view_models::sheet::FilesCounts>>,
    terminals: terminal::Terminals,
    notices: Arc<notices::Notices>,
    /// The open Open PR / Push to PR dialogs by ticket, kept between requests (T-0227).
    pr_dialogs: std::sync::Mutex<
        std::collections::HashMap<String, otaq_view::view_models::pr_dialog::PrDialog>,
    >,
    /// The Cleanup overlay's last scan and what is ticked, kept between requests (T-0235).
    cleanup: std::sync::Mutex<otaq_view::view_models::cleanup::CleanupModel>,
    /// The open run-command dialogs by project, kept between requests (T-0232).
    run_commands: std::sync::Mutex<
        std::collections::HashMap<String, otaq_view::view_models::run_command::RunCommandModel>,
    >,
    /// Anonymous usage data, sent only after the user said yes (d0085).
    usage: usage::Usage,
    /// Whether the window has keyboard focus, told to every connection to the service (`app.focus`, d0090).
    window_focus: api::WindowFocus,
    /// The reports open in the report view (CMT-12), each indexed once.
    reports: api::OpenReports,
    /// When the user last looked at each Project agent conversation, for the Ask bar's "New reply" (SHB-03).
    looked: std::sync::Mutex<otaq_view::view_models::ask_bar::Looked>,
    /// The installed fonts, read once (d0117 point 5).
    fonts: fonts::Fonts,
    /// Where the app runs from: outside an Applications folder a macOS release shows only the Move window (T-0579).
    install: install_place::InstallState,
    /// Self-updates (UPD-04, d0140): the check, the download, the kept update.
    updates: Arc<updates::Updates>,
    /// Where the updater checks: Tauri's updater plugin, set by the door once the app is up.
    update_source: std::sync::OnceLock<Arc<dyn updates::Source>>,
    /// Puts a kept update in place and reopens the app: the plugin and the door, set with `update_source`.
    update_installer:
        std::sync::OnceLock<(Arc<dyn updates::Installer>, Arc<dyn updates::Relaunch>)>,
}

fn main() {
    // The bundle check (T-0370): no window, no service.
    if notify_check::requested() {
        std::process::exit(notify_check::run());
    }
    // A copy opened by Move to Applications waits for the one that moved it to quit, so it takes the one-copy lock
    // below instead of handing over to the quitting copy (T-0579). The variable isn't passed on to the service.
    let moved_from = install_place::moved_from_var();
    install_place::wait_for_previous(std::env::var(&moved_from).ok().as_deref());
    // SAFETY: the app has no other thread yet, so nothing reads the environment while it changes.
    unsafe { std::env::remove_var(&moved_from) };
    // The last start couldn't put a kept update in place: say so in About instead of trying again at once (UPD-05).
    let install_failed = std::env::var(updates::failed_var())
        .ok()
        .and_then(|v| updates::read_failed(&v));
    // SAFETY: as above, no other thread yet.
    unsafe { std::env::remove_var(updates::failed_var()) };
    // On macOS a release runs only from an Applications folder (UPD-06): elsewhere no service, no update check and no
    // `bin` link start, and the window shows only Move to Applications.
    let gate = install_place::current();
    // Before any other thread or the webview exists (Q39).
    let os = std::env::consts::OS;
    if let Some((name, value)) =
        window::webkit_env(os, std::env::var_os(window::WEBKIT_DMABUF).as_deref())
    {
        // SAFETY: the app has no other thread yet, so nothing reads the environment while it changes.
        unsafe { std::env::set_var(name, value) };
    }
    // One runtime for the whole app; the door hands it to Tauri, so its async calls run here too.
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(e) => {
            eprintln!("can't start the app's runtime: {e}");
            std::process::exit(1);
        }
    };
    let data = DataDir::from_env().map_err(|e| e.to_string());
    // Only one copy runs per data folder (d0139): a second start hands its open target to it and goes, before any
    // window, Dock icon or service link exists.
    let first_copy = match &data {
        Ok(data) => {
            let open = std::env::var(otaq_core::attention::notify::open_env_var()).ok();
            match runtime.block_on(one_copy::claim(&one_copy::Paths::of(data), open.as_deref())) {
                Ok(one_copy::Claim::First(first)) => Some(first),
                Ok(one_copy::Claim::HandedOver) => std::process::exit(0),
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        }
        Err(_) => None,
    };
    // `<data folder>/bin/otaq` points at this copy's `otaq` (d0140: the copy opened last wins); never from a copy
    // outside Applications (UPD-06), whose folder may disappear.
    if let Ok(data) = &data
        && gate.runs()
    {
        let data = data.clone();
        runtime.spawn_blocking(move || shell_path::refresh_bin(&data));
    }
    let events = Arc::new(EventHub::default());
    // The `otaq` command next to this program starts the service when none runs (d0055).
    let placement = data.as_ref().ok().map(window::placement_file);
    let install = install_place::InstallState {
        gate: gate.clone(),
        data: data.as_ref().ok().cloned(),
    };
    let update_env =
        updates::Env::of_this_app(data.as_ref().ok().map(|d| d.updates_dir()), gate.runs());
    // A compatible update kept from before goes in place before anything else starts (d0140, gap review G6).
    let install_first = match &data {
        Ok(data) if install_failed.is_none() && first_copy.is_some() => {
            runtime.block_on(updates::kept_for_open(&update_env, data))
        }
        _ => None,
    };
    let settings = data.and_then(|data| {
        if !gate.runs() {
            return Err(format!(
                "{} runs only from an Applications folder",
                otaq_core::naming::PRODUCT_NAME
            ));
        }
        if install_first.is_some() {
            return Err("putting an update in place".into());
        }
        let launcher = CommandLauncher::next_to_current_exe().map_err(|e| e.to_string())?;
        Ok(Settings::new(data, Arc::new(launcher)))
    });
    let (service, notices) = {
        let _inside = runtime.enter();
        let service = ServiceLink::start(settings, events.clone());
        let notices = notices::Notices::start(&service, events.clone());
        (service, notices)
    };
    let updates = updates::Updates::new(
        update_env,
        Arc::new(updates::ServicePrefs::new(service.clients())),
    );
    if let Some(kept) = install_first.clone() {
        updates.adopt_kept(kept);
    }
    if let Some((version, why)) = install_failed.filter(|(version, _)| !version.is_empty()) {
        updates.install_failed(version, why);
    }
    let app = Arc::new(App {
        events,
        service,
        switcher: Default::default(),
        drawer_tabs: Default::default(),
        review: Default::default(),
        files_counts: Default::default(),
        terminals: terminal::Terminals::default(),
        notices,
        pr_dialogs: Default::default(),
        cleanup: Default::default(),
        run_commands: Default::default(),
        window_focus: Default::default(),
        reports: Default::default(),
        looked: std::sync::Mutex::new(otaq_view::view_models::ask_bar::Looked::new(
            otaq_core::store::now_ms(),
        )),
        usage: usage::Usage::new(runtime.handle().clone()),
        fonts: fonts::Fonts::system(),
        install,
        updates,
        update_source: std::sync::OnceLock::new(),
        update_installer: std::sync::OnceLock::new(),
    });
    if gate.runs() && install_first.is_none() {
        runtime.spawn(api::usage_loop(app.clone()));
    }
    {
        let app = app.clone();
        runtime.spawn(async move { api::follow_window_focus(&app).await });
    }
    if let Err(e) = door::run(
        app,
        runtime.handle().clone(),
        placement,
        first_copy,
        !gate.runs(),
        install_first.is_some(),
    ) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
