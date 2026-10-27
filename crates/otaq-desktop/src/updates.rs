//! The app's self-updates (UPD-04, d0140): once a day (and on Check for updates) it asks the release's `latest.json`
//! whether there is a newer version, downloads it by itself with its signature checked, and keeps it in
//! `<data folder>/updates/<version>/` with what `latest.json` said about it, so "ready" survives a quit. Restart now,
//! Restart to update and the next open put it in place ([`Updates::put_in_place`], `otaq_view`'s `restart_step`):
//! only the app for a compatible update, the service too (0022's stop) for one it can't work with.
//!
//! Plain Rust: the check and the download come through [`Source`], which the door implements with Tauri's updater
//! plugin; the preferences (the switch, the last check, the skipped version) through [`Prefs`], kept by the service
//! so a restart of the app doesn't check again early. Never in a build from source; never before the app is
//! installed where it may update (T-0579's gate); never for the `.deb`, which only says a newer version
//! is available.
//!
//! A release is [`crate::install_place::release`] (built with the release workflow's flag, T-0578/T-0579); the
//! test overrides come from `build.rs`: `UPDATES_TEST_ENDPOINT` / `UPDATES_TEST_PUBKEY` exist only in a non-release build
//! whose environment set the test overrides (T-0585), and a release build with them set doesn't build. A build with a
//! test endpoint checks (against it) although it isn't a release; every other non-release build is built from source.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use otaq_core::client::Client;
use otaq_core::protocol::{Reply, Request};
use otaq_core::updates::{LastCheck, UpdatePrefs, UpdatePrefsChange};
use otaq_view::view_models::updates::{self as vm, Kept, UpdateState};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Notify, watch};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The public key the updater checks signatures against (minisign, as `tauri signer generate` prints it).
/// `tauri.conf.json`'s `plugins.updater.pubkey` carries the same placeholder.
pub const PUBKEY: &str =
    "PLACEHOLDER — replaced by the maintainer before the first release (T-0583)";

/// Whether this build is a release (the release workflow's flag at build time); every other build is built from
/// source and never checks.
pub fn is_release() -> bool {
    crate::install_place::release()
}

/// The key updates are checked against: the built-in one, or a test build's.
pub fn pubkey() -> &'static str {
    test_pubkey().unwrap_or(PUBKEY)
}

/// Whether `bytes` carry `signature` (`latest.json`'s: the base64 of minisign's `.sig`) by `pubkey` (base64, as
/// `tauri signer generate` prints it), checked as the updater plugin checks a download.
pub fn signature_matches(bytes: &[u8], signature: &str, pubkey: &str) -> bool {
    use base64::Engine;
    let text = |b64: &str| {
        base64::engine::general_purpose::STANDARD
            .decode(b64.trim())
            .ok()
            .and_then(|v| String::from_utf8(v).ok())
    };
    let (Some(key), Some(sig)) = (text(pubkey), text(signature)) else {
        return false;
    };
    match (
        minisign_verify::PublicKey::decode(&key),
        minisign_verify::Signature::decode(&sig),
    ) {
        (Ok(key), Ok(sig)) => key.verify(bytes, &sig, true).is_ok(),
        _ => false,
    }
}

/// A test build's own `latest.json` address (T-0585); never set in a release build.
pub fn test_endpoint() -> Option<&'static str> {
    option_env!("UPDATES_TEST_ENDPOINT").filter(|_| !is_release())
}

/// A test build's own public key (T-0585); never set in a release build.
pub fn test_pubkey() -> Option<&'static str> {
    option_env!("UPDATES_TEST_PUBKEY").filter(|_| !is_release())
}

/// What the release said, and how to fetch it.
pub struct Found {
    pub version: String,
    /// `latest.json` as it came: `protocol_version` (T-0583) and `notes_url` are read from it.
    pub raw_json: Value,
    pub fetch: Box<dyn Fetch>,
}

/// Downloads the found version and checks its signature; `progress` gets the bytes so far and the whole size.
pub trait Fetch: Send + Sync {
    fn fetch(
        &self,
        progress: Box<dyn FnMut(u64, Option<u64>) + Send>,
    ) -> BoxFuture<'_, Result<Vec<u8>, FetchFailed>>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchFailed {
    /// The connection dropped, a server error, anything but the signature.
    Stopped,
    /// The file's signature doesn't match the built-in public key.
    SignatureRefused,
}

/// The release's `latest.json`: a newer version (`Ok(Some)`), none (`Ok(None)`) or no answer (`Err`).
pub trait Source: Send + Sync {
    fn check(&self) -> BoxFuture<'_, Result<Option<Found>, String>>;
}

/// Where the preferences are kept (the service): `None` when it can't answer.
pub trait Prefs: Send + Sync {
    fn load(&self) -> BoxFuture<'_, Option<UpdatePrefs>>;
    fn change(&self, change: UpdatePrefsChange) -> BoxFuture<'_, Option<UpdatePrefs>>;
}

/// The service's settings, through the link's client once it has one.
pub struct ServicePrefs {
    clients: watch::Receiver<Option<Client>>,
}

impl ServicePrefs {
    pub fn new(clients: watch::Receiver<Option<Client>>) -> Self {
        Self { clients }
    }

    async fn ask(&self, request: Request) -> Option<UpdatePrefs> {
        let mut clients = self.clients.clone();
        let client = clients.wait_for(Option::is_some).await.ok()?.clone()?;
        match client.request(request).await {
            Ok(Reply::UpdatePrefs(prefs)) => Some(prefs),
            _ => None,
        }
    }
}

impl Prefs for ServicePrefs {
    fn load(&self) -> BoxFuture<'_, Option<UpdatePrefs>> {
        Box::pin(self.ask(Request::UpdatePrefs))
    }

    fn change(&self, change: UpdatePrefsChange) -> BoxFuture<'_, Option<UpdatePrefs>> {
        Box::pin(self.ask(Request::UpdateSetPrefs(change)))
    }
}

/// Puts the kept update in place: Tauri's updater plugin in the door, a fake in tests. In two steps, so nothing is
/// stopped before the update is known to be installable.
pub trait Installer: Send + Sync {
    /// Gets `<dir>/<version>` ready: the plugin's own `Update` for it, asked of the release again (the plugin can't
    /// make one from the kept files), and the kept bytes checked against the signature that answer carries.
    fn prepare<'a>(
        &'a self,
        dir: &'a Path,
        version: &'a str,
    ) -> BoxFuture<'a, Result<Box<dyn Prepared>, InstallFailed>>;
}

/// An update ready to be put in place.
pub trait Prepared: Send {
    /// Installs it; it doesn't reopen the app (on Windows the setup does, and this process ends).
    fn install(self: Box<Self>) -> Result<(), InstallFailed>;
}

/// Whether this user may change `folder` and the folder holding it (what replacing an installed app needs): a file
/// can be made in both. A folder another account owns, or a read-only one, isn't; then "couldn't be put in place"
/// says so (page 32's line), and any other failure says what went wrong.
pub fn folder_writable(folder: &Path) -> bool {
    let probe = |dir: &Path| {
        let file = dir.join(format!(".update-write-check-{}", std::process::id()));
        let made = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&file)
            .is_ok();
        if made {
            let _ = std::fs::remove_file(&file);
        }
        made
    };
    probe(folder) && folder.parent().is_none_or(probe)
}

/// Reopens the app once the new version is in place, as the only copy (T-0579's hand-over: the new process waits for
/// this one to exit before it takes the one-copy lock); with `failed`, the same version reopens and shows why.
pub trait Relaunch: Send + Sync {
    fn relaunch(&self, failed: Option<(String, String)>);
}

/// Why an update couldn't be put in place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallFailed {
    /// The installed app it was to replace (`/Applications/Otaq.app`).
    pub folder: String,
    /// The system refused for rights.
    pub permission: bool,
    /// The release couldn't be reached to confirm it: nothing changed, it stays ready.
    pub offline: bool,
    pub error: String,
}

/// Stops the running service for an update it can't work with (the link's 0022 stop).
pub trait ServiceStop: Send + Sync {
    fn stop(&self) -> BoxFuture<'_, Result<(), String>>;
}

impl ServiceStop for crate::service::ServiceLink {
    fn stop(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(self.stop_for_update())
    }
}

/// What [`Updates::put_in_place`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placed {
    /// Nothing ready, an incompatible update at the next open, or a restart already running.
    Nothing,
    /// The new version is in place: reopen the app.
    Relaunch,
    /// The release couldn't be reached to confirm the update: nothing changed, it stays ready.
    Offline,
    Failed {
        version: String,
        why: String,
    },
}

/// The variable a reopened app reads when the last start couldn't put an update in place (`{"version", "why"}`), so it
/// shows "couldn't be put in place" instead of trying again at once.
pub fn failed_var() -> String {
    otaq_core::naming::env_var("UPDATE_FAILED")
}

/// `failed_var`'s value.
pub fn failed_value(version: &str, why: &str) -> String {
    serde_json::json!({ "version": version, "why": why }).to_string()
}

/// Reads `failed_var`'s value back.
pub fn read_failed(value: &str) -> Option<(String, String)> {
    let v: Value = serde_json::from_str(value).ok()?;
    Some((
        v["version"].as_str()?.to_owned(),
        v["why"].as_str()?.to_owned(),
    ))
}

/// At the next open, before the window or the service link: whether the kept update goes in place first. Only a
/// compatible one (or any, with no service running); `service_protocol` is the running service's.
pub fn install_at_open(env: &Env, service_protocol: Option<u32>) -> Option<Kept> {
    let dir = env
        .dir
        .as_ref()
        .filter(|_| env.checks && env.installed && !env.by_hand)?;
    let kept = load_kept(dir, &env.current_version)?;
    (vm::restart_step(
        vm::Trigger::NextOpen,
        Some(kept.protocol_version),
        service_protocol,
    ) == vm::Step::AppOnly)
        .then_some(kept)
}

impl Updates {
    /// The kept update chosen at the next open, before the updater runs: ready, for [`Self::put_in_place`].
    pub fn adopt_kept(&self, kept: Kept) {
        *self.kept.lock().expect("kept update") = Some(kept.clone());
        self.set_state(UpdateState::Ready(kept));
    }
}

/// At the next open: the kept update to put in place first, asking the running service (if any) for its protocol
/// number. A service that doesn't answer in time counts as unknown: nothing is installed then.
pub async fn kept_for_open(env: &Env, data: &otaq_core::data_dir::DataDir) -> Option<Kept> {
    // Nothing kept, nothing to ask.
    install_at_open(env, None)?;
    let status = tokio::time::timeout(
        Duration::from_secs(3),
        otaq_core::client::request_once(&data.socket_path(), Request::ServiceStatus),
    )
    .await;
    let protocol = match status {
        Ok(Ok(Reply::ServiceStatus(status))) => Some(status.protocol_version),
        Ok(Err(otaq_core::client::ClientError::NotRunning(_))) => None,
        _ => return None,
    };
    install_at_open(env, protocol)
}

/// What this copy of the app is.
#[derive(Debug, Clone)]
pub struct Env {
    /// Checks for updates: a release, or a test build pointed at its own `latest.json` (T-0585). Otherwise built
    /// from source.
    pub checks: bool,
    pub installed: bool,
    /// Installed from the `.deb`: never downloads.
    pub by_hand: bool,
    pub current_version: String,
    /// `<data folder>/updates`; `None` without a data folder (nothing is kept, nothing checks).
    pub dir: Option<PathBuf>,
}

impl Env {
    /// This build on this machine; `installed` is T-0579's gate (`app.install.gate.runs()`).
    pub fn of_this_app(dir: Option<PathBuf>, installed: bool) -> Self {
        Self {
            checks: is_release() || test_endpoint().is_some(),
            installed,
            by_hand: vm::updates_by_hand(
                std::env::consts::OS,
                std::env::var("APPIMAGE").ok().as_deref(),
            ),
            current_version: env!("CARGO_PKG_VERSION").to_owned(),
            dir,
        }
    }
}

/// The state and the preferences, as the page's view needs them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub state: UpdateState,
    pub prefs: UpdatePrefs,
    /// Restart now is putting the update in place.
    pub installing: bool,
}

/// The updater.
pub struct Updates {
    env: Env,
    prefs: Arc<dyn Prefs>,
    now: Box<dyn Fn() -> i64 + Send + Sync>,
    snapshot: watch::Sender<Snapshot>,
    /// A check runs (automatic or Check for updates); a second one waits.
    checking: tokio::sync::Mutex<()>,
    /// The kept update, if any (also while a check runs).
    kept: Mutex<Option<Kept>>,
    wake: Notify,
    /// The last start couldn't put this update in place: `(version, why)`, shown instead of "ready".
    failed: Mutex<Option<(String, String)>>,
    /// "Updated to Otaq 0.2.0" waiting to be shown once: `(title, text)`.
    updated: Mutex<Option<(String, String)>>,
}

impl Updates {
    pub fn new(env: Env, prefs: Arc<dyn Prefs>) -> Arc<Self> {
        Self::with_clock(env, prefs, Box::new(otaq_core::store::now_ms))
    }

    pub fn with_clock(
        env: Env,
        prefs: Arc<dyn Prefs>,
        now: Box<dyn Fn() -> i64 + Send + Sync>,
    ) -> Arc<Self> {
        let state = if env.checks {
            UpdateState::NotChecked
        } else {
            UpdateState::BuiltFromSource
        };
        Arc::new(Self {
            env,
            prefs,
            now,
            snapshot: watch::Sender::new(Snapshot {
                state,
                prefs: UpdatePrefs::default(),
                installing: false,
            }),
            failed: Mutex::new(None),
            updated: Mutex::new(None),
            checking: tokio::sync::Mutex::new(()),
            kept: Mutex::new(None),
            wake: Notify::new(),
        })
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.borrow().clone()
    }

    /// Each change of the state or the preferences.
    pub fn changes(&self) -> watch::Receiver<Snapshot> {
        self.snapshot.subscribe()
    }

    fn set_state(&self, state: UpdateState) {
        self.snapshot.send_if_modified(|s| {
            let changed = s.state != state;
            s.state = state;
            changed
        });
    }

    fn set_prefs(&self, prefs: UpdatePrefs) {
        self.snapshot.send_if_modified(|s| {
            let changed = s.prefs != prefs;
            s.prefs = prefs;
            changed
        });
    }

    /// Runs for as long as the app does: the kept update, the preferences, then the daily check.
    pub async fn run(self: Arc<Self>, source: Arc<dyn Source>) {
        if !self.env.checks {
            return;
        }
        if let Some(dir) = &self.env.dir
            && let Some(kept) = load_kept(dir, &self.env.current_version)
        {
            *self.kept.lock().expect("kept update") = Some(kept.clone());
            self.set_state(UpdateState::Ready(kept));
        }
        let failed = self.failed.lock().expect("failed install").clone();
        if let Some((version, why)) = failed {
            self.set_state(UpdateState::NotReplaced { version, why });
        }
        let mut prefs = self.prefs.load().await.unwrap_or_default();
        // Once per version: "Updated to Otaq 0.2.0" after starting on a newer version than last time.
        let current = &self.env.current_version;
        if prefs.last_version.as_deref() != Some(current.as_str()) {
            *self.updated.lock().expect("updated toast") =
                vm::updated_toast(prefs.last_version.as_deref(), current);
            let change = UpdatePrefsChange {
                last_version: Some(current.clone()),
                ..Default::default()
            };
            if let Some(stored) = self.prefs.change(change).await {
                prefs = stored;
            }
        }
        if self.snapshot.borrow().state == UpdateState::NotChecked
            && let Some(last) = prefs.last_check
        {
            self.set_state(if last.reached {
                UpdateState::UpToDate {
                    checked_at: last.at,
                }
            } else {
                UpdateState::Unreachable {
                    checked_at: last.at,
                }
            });
        }
        self.set_prefs(prefs);
        if !self.env.installed || self.env.dir.is_none() {
            return;
        }
        loop {
            let prefs = self.snapshot.borrow().prefs.clone();
            let last = prefs.last_check.map(|c| c.at);
            let now = (self.now)();
            if vm::check_due(prefs.auto_check, last, now) {
                self.check(source.as_ref()).await;
                continue;
            }
            let wait = if prefs.auto_check {
                // At most an hour at a time, so a sleeping computer catches up soon after it wakes.
                Duration::from_millis(u64::try_from(vm::due_in_ms(last, now)).unwrap_or(0))
                    .min(Duration::from_secs(3600))
            } else {
                Duration::from_secs(3600)
            };
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = self.wake.notified() => {}
            }
        }
    }

    /// The update kept on disk, if one is ready.
    pub fn kept(&self) -> Option<Kept> {
        self.kept.lock().expect("kept update").clone()
    }

    /// The last start couldn't put `version` in place (`why`): About says so instead of "ready".
    pub fn install_failed(&self, version: String, why: String) {
        *self.failed.lock().expect("failed install") = Some((version, why));
    }

    /// "Updated to Otaq 0.2.0" once, after starting on a newer version.
    pub fn take_updated_toast(&self) -> Option<(String, String)> {
        self.updated.lock().expect("updated toast").take()
    }

    /// Restart now, Restart to update or the next open: the step `restart_step` decides for the kept update and the
    /// running service's protocol number, carried out. [`Placed::Relaunch`] when the new app is in place (the caller
    /// reopens it); on a failure About says why and the running app goes on.
    pub async fn put_in_place(
        &self,
        trigger: vm::Trigger,
        service_protocol: Option<u32>,
        service: &dyn ServiceStop,
        installer: &dyn Installer,
    ) -> Placed {
        let Some(dir) = self.env.dir.clone().filter(|_| self.env.checks) else {
            return Placed::Nothing;
        };
        let kept = self.kept();
        let step = vm::restart_step(
            trigger,
            kept.as_ref().map(|k| k.protocol_version),
            service_protocol,
        );
        let Some(kept) = kept.filter(|_| step != vm::Step::Wait) else {
            return Placed::Nothing;
        };
        let started = self
            .snapshot
            .send_if_modified(|s| !std::mem::replace(&mut s.installing, true));
        if !started {
            return Placed::Nothing;
        }
        let done = async {
            let prepared = installer.prepare(&dir, &kept.version).await?;
            if step == vm::Step::WithService {
                service.stop().await.map_err(|error| InstallFailed {
                    folder: String::new(),
                    permission: false,
                    offline: false,
                    error,
                })?;
            }
            prepared.install()
        }
        .await;
        self.snapshot.send_modify(|s| s.installing = false);
        match done {
            Ok(()) => Placed::Relaunch,
            Err(failed) if failed.offline => Placed::Offline,
            Err(failed) => {
                let why = vm::not_replaced_why(
                    &failed.folder,
                    failed.permission,
                    &failed.error,
                    &self.env.current_version,
                    std::env::consts::OS,
                );
                self.set_state(UpdateState::NotReplaced {
                    version: kept.version.clone(),
                    why: why.clone(),
                });
                Placed::Failed {
                    version: kept.version,
                    why,
                }
            }
        }
    }

    /// Check for updates: now, whatever the switch says. Nothing in a build from source or before the app is
    /// installed.
    pub async fn check_now(&self, source: &dyn Source) {
        if self.env.checks && self.env.installed && self.env.dir.is_some() {
            self.check(source).await;
        }
    }

    /// The switch.
    pub async fn set_auto(&self, on: bool) {
        let change = UpdatePrefsChange {
            auto_check: Some(on),
            ..Default::default()
        };
        if let Some(prefs) = self.prefs.change(change).await {
            self.set_prefs(prefs);
        }
        self.wake.notify_one();
    }

    /// Skip this version: the version the notice shows.
    pub async fn skip(&self) {
        let version = match &self.snapshot.borrow().state {
            UpdateState::Ready(kept) => kept.version.clone(),
            UpdateState::Available { version, .. } => version.clone(),
            _ => return,
        };
        let change = UpdatePrefsChange {
            skipped: Some(version),
            ..Default::default()
        };
        if let Some(prefs) = self.prefs.change(change).await {
            self.set_prefs(prefs);
        }
    }

    async fn record_check(&self, at: i64, reached: bool) {
        let check = LastCheck { at, reached };
        let change = UpdatePrefsChange {
            last_check: Some(check),
            ..Default::default()
        };
        let prefs = match self.prefs.change(change).await {
            Some(prefs) => prefs,
            // The service didn't answer: the check still counts for this run.
            None => UpdatePrefs {
                last_check: Some(check),
                ..self.snapshot.borrow().prefs.clone()
            },
        };
        self.set_prefs(prefs);
    }

    async fn check(&self, source: &dyn Source) {
        let Ok(_one) = self.checking.try_lock() else {
            return;
        };
        let kept = self.kept.lock().expect("kept update").clone();
        self.set_state(UpdateState::Checking);
        let found = source.check().await;
        let now = (self.now)();
        self.record_check(now, found.is_ok()).await;
        let found = match found {
            Err(_) => {
                return self.set_state(match kept {
                    Some(kept) => UpdateState::Ready(kept),
                    None => UpdateState::Unreachable { checked_at: now },
                });
            }
            Ok(found) => found.filter(|f| vm::newer(&f.version, &self.env.current_version)),
        };
        let Some(found) = found else {
            return self.set_state(match kept {
                Some(kept) => UpdateState::Ready(kept),
                None => UpdateState::UpToDate { checked_at: now },
            });
        };
        let notes_url = notes_url(&found.version, &found.raw_json);
        if self.env.by_hand {
            return self.set_state(UpdateState::Available {
                version: found.version,
                notes_url,
                checked_at: now,
            });
        }
        if let Some(kept) = kept.filter(|k| !vm::newer(&found.version, &k.version)) {
            return self.set_state(UpdateState::Ready(kept));
        }
        self.download(found, notes_url).await;
    }

    async fn download(&self, found: Found, notes_url: String) {
        let version = found.version.clone();
        self.set_state(UpdateState::Downloading {
            version: version.clone(),
            done: 0,
            total: None,
        });
        let seen: Arc<Mutex<(u64, Option<u64>)>> = Arc::default();
        let progress = {
            let (seen, snapshot) = (seen.clone(), self.snapshot.clone());
            let version = version.clone();
            Box::new(move |done: u64, total: Option<u64>| {
                let before =
                    std::mem::replace(&mut *seen.lock().expect("download progress"), (done, total));
                // The page hears of each new megabyte, not of each chunk.
                if before.0 >> 20 != done >> 20 || before.1 != total {
                    snapshot.send_modify(|s| {
                        s.state = UpdateState::Downloading {
                            version: version.clone(),
                            done,
                            total,
                        }
                    });
                }
            })
        };
        let fetched = found.fetch.fetch(progress).await;
        let (done, total) = *seen.lock().expect("download progress");
        let bytes = match fetched {
            Ok(bytes) => bytes,
            Err(FetchFailed::SignatureRefused) => {
                return self.set_state(UpdateState::SignatureRefused { version });
            }
            Err(FetchFailed::Stopped) => {
                return self.set_state(UpdateState::DownloadStopped {
                    version,
                    done,
                    total,
                });
            }
        };
        let kept = Kept {
            version: version.clone(),
            notes_url,
            protocol_version: protocol_version(&found.raw_json),
            downloaded_at: (self.now)(),
        };
        let dir = self
            .env
            .dir
            .clone()
            .expect("checked only with a data folder");
        match keep(&dir, &kept, &found.raw_json, &bytes) {
            Ok(()) => {
                *self.kept.lock().expect("kept update") = Some(kept.clone());
                self.set_state(UpdateState::Ready(kept));
            }
            Err(e) => {
                eprintln!("couldn't keep the downloaded update: {e}");
                self.set_state(UpdateState::DownloadStopped {
                    version,
                    done,
                    total,
                });
            }
        }
    }
}

/// `latest.json`'s `protocol_version` (T-0583).
fn protocol_version(raw: &Value) -> Option<u32> {
    raw.get("protocol_version")?.as_u64()?.try_into().ok()
}

/// `latest.json`'s `notes_url`, or the version's release page.
fn notes_url(version: &str, raw: &Value) -> String {
    raw.get("notes_url")
        .and_then(Value::as_str)
        .filter(|u| u.starts_with("https://"))
        .map_or_else(|| vm::release_page(version), str::to_owned)
}

// ---- Kept on disk: `<dir>/<version>/update.json` and `update.bin` ----

const KEPT_JSON: &str = "update.json";
const KEPT_BYTES: &str = "update.bin";

/// `update.json`.
#[derive(Debug, Serialize, Deserialize)]
struct KeptFile {
    version: String,
    notes_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    protocol_version: Option<u32>,
    downloaded_at: i64,
    /// `latest.json` as it came.
    raw_json: Value,
}

/// The kept update's bytes, for the install.
pub fn kept_bytes(dir: &Path, version: &str) -> PathBuf {
    dir.join(version).join(KEPT_BYTES)
}

/// Writes the update into a temporary folder, then renames it to `<dir>/<version>`: a folder named by its version is
/// always complete. Older kept versions go.
fn keep(dir: &Path, kept: &Kept, raw_json: &Value, bytes: &[u8]) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let partial = dir.join(format!(".{}.partial-{}", kept.version, std::process::id()));
    let _ = std::fs::remove_dir_all(&partial);
    let write = || -> std::io::Result<()> {
        std::fs::create_dir_all(&partial)?;
        std::fs::write(partial.join(KEPT_BYTES), bytes)?;
        let file = KeptFile {
            version: kept.version.clone(),
            notes_url: kept.notes_url.clone(),
            protocol_version: kept.protocol_version,
            downloaded_at: kept.downloaded_at,
            raw_json: raw_json.clone(),
        };
        let json = serde_json::to_vec_pretty(&file).map_err(std::io::Error::other)?;
        std::fs::write(partial.join(KEPT_JSON), json)?;
        let target = dir.join(&kept.version);
        let _ = std::fs::remove_dir_all(&target);
        std::fs::rename(&partial, &target)
    };
    let written = write();
    if written.is_err() {
        let _ = std::fs::remove_dir_all(&partial);
    }
    written?;
    for entry in std::fs::read_dir(dir)?.flatten() {
        if entry.file_name().to_string_lossy() != kept.version.as_str() {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
    Ok(())
}

/// The newest complete kept update newer than this version. Everything else in the folder (older or same versions,
/// half-written folders) is deleted.
fn load_kept(dir: &Path, current: &str) -> Option<Kept> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut found: Vec<(PathBuf, Kept)> = Vec::new();
    let mut gone: Vec<PathBuf> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let read = || -> Option<Kept> {
            if !path.join(KEPT_BYTES).is_file() {
                return None;
            }
            let file: KeptFile =
                serde_json::from_slice(&std::fs::read(path.join(KEPT_JSON)).ok()?).ok()?;
            let named = entry.file_name().to_string_lossy() == file.version.as_str();
            (named && vm::newer(&file.version, current)).then_some(Kept {
                version: file.version,
                notes_url: file.notes_url,
                protocol_version: file.protocol_version,
                downloaded_at: file.downloaded_at,
            })
        };
        match read() {
            Some(kept) => found.push((path, kept)),
            None => gone.push(path),
        }
    }
    found.sort_by(|a, b| {
        if vm::newer(&a.1.version, &b.1.version) {
            std::cmp::Ordering::Greater
        } else {
            std::cmp::Ordering::Less
        }
    });
    let newest = found.pop();
    gone.extend(found.into_iter().map(|(path, _)| path));
    for path in gone {
        let _ = std::fs::remove_dir_all(path);
    }
    newest.map(|(_, kept)| kept)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

    use super::*;

    const DAY: i64 = vm::CHECK_EVERY_MS;
    const T0: i64 = 1_793_000_000_000;

    #[derive(Default)]
    struct FakePrefs {
        stored: Mutex<UpdatePrefs>,
    }

    impl Prefs for FakePrefs {
        fn load(&self) -> BoxFuture<'_, Option<UpdatePrefs>> {
            let prefs = self.stored.lock().unwrap().clone();
            Box::pin(async move { Some(prefs) })
        }

        fn change(&self, change: UpdatePrefsChange) -> BoxFuture<'_, Option<UpdatePrefs>> {
            let mut stored = self.stored.lock().unwrap();
            if let Some(on) = change.auto_check {
                stored.auto_check = on;
            }
            if change.last_check.is_some() {
                stored.last_check = change.last_check;
            }
            if change.skipped.is_some() {
                stored.skipped = change.skipped;
            }
            if change.last_version.is_some() {
                stored.last_version = change.last_version;
            }
            let prefs = stored.clone();
            Box::pin(async move { Some(prefs) })
        }
    }

    /// What the fake release answers.
    #[derive(Clone)]
    enum Answer {
        Offline,
        Nothing,
        Version(&'static str, Result<&'static [u8], FetchFailed>),
    }

    struct FakeSource {
        answer: Mutex<Answer>,
        checks: AtomicUsize,
    }

    impl FakeSource {
        fn new(answer: Answer) -> Arc<Self> {
            Arc::new(Self {
                answer: Mutex::new(answer),
                checks: AtomicUsize::new(0),
            })
        }
    }

    struct FakeFetch(Result<&'static [u8], FetchFailed>);

    impl Fetch for FakeFetch {
        fn fetch(
            &self,
            mut progress: Box<dyn FnMut(u64, Option<u64>) + Send>,
        ) -> BoxFuture<'_, Result<Vec<u8>, FetchFailed>> {
            let result = self.0;
            Box::pin(async move {
                progress(3 << 20, Some(5 << 20));
                result.map(<[u8]>::to_vec)
            })
        }
    }

    impl Source for FakeSource {
        fn check(&self) -> BoxFuture<'_, Result<Option<Found>, String>> {
            self.checks.fetch_add(1, Ordering::SeqCst);
            let answer = self.answer.lock().unwrap().clone();
            Box::pin(async move {
                match answer {
                    Answer::Offline => Err("offline".into()),
                    Answer::Nothing => Ok(None),
                    Answer::Version(version, fetched) => Ok(Some(Found {
                        version: version.into(),
                        raw_json: serde_json::json!({"version": version, "protocol_version": 1}),
                        fetch: Box::new(FakeFetch(fetched)),
                    })),
                }
            })
        }
    }

    struct Rig {
        _tmp: tempfile::TempDir,
        dir: PathBuf,
        prefs: Arc<FakePrefs>,
        clock: Arc<AtomicI64>,
    }

    impl Rig {
        fn new() -> Self {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("updates");
            Self {
                _tmp: tmp,
                dir,
                prefs: Arc::default(),
                clock: Arc::new(AtomicI64::new(T0)),
            }
        }

        fn env(&self) -> Env {
            Env {
                checks: true,
                installed: true,
                by_hand: false,
                current_version: "0.1.0".into(),
                dir: Some(self.dir.clone()),
            }
        }

        fn updates(&self, env: Env) -> Arc<Updates> {
            let clock = self.clock.clone();
            Updates::with_clock(
                env,
                self.prefs.clone(),
                Box::new(move || clock.load(Ordering::SeqCst)),
            )
        }
    }

    async fn settle(updates: &Arc<Updates>, want: impl Fn(&UpdateState) -> bool) -> UpdateState {
        let mut changes = updates.changes();
        tokio::time::timeout(Duration::from_secs(5), changes.wait_for(|s| want(&s.state)))
            .await
            .expect("the state came")
            .unwrap()
            .state
            .clone()
    }

    #[tokio::test]
    async fn a_newer_version_downloads_and_is_kept_on_disk() {
        let rig = Rig::new();
        let updates = rig.updates(rig.env());
        let source = FakeSource::new(Answer::Version("0.1.1", Ok(b"the app")));
        tokio::spawn(updates.clone().run(source.clone()));
        let state = settle(&updates, |s| matches!(s, UpdateState::Ready(_))).await;
        let UpdateState::Ready(kept) = state else {
            unreachable!()
        };
        assert_eq!(kept.version, "0.1.1");
        assert_eq!(kept.protocol_version, Some(1));
        assert_eq!(kept.notes_url, vm::release_page("0.1.1"));
        assert_eq!(
            std::fs::read(kept_bytes(&rig.dir, "0.1.1")).unwrap(),
            b"the app"
        );
        // Only the finished folder is left.
        let names: Vec<_> = std::fs::read_dir(&rig.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["0.1.1"]);
        // The check is recorded with the service.
        assert_eq!(
            rig.prefs.stored.lock().unwrap().last_check,
            Some(LastCheck {
                at: T0,
                reached: true
            })
        );

        // "Ready" survives a quit: a new start reads it back without checking (checked less than a day ago).
        let again = rig.updates(rig.env());
        let source = FakeSource::new(Answer::Nothing);
        tokio::spawn(again.clone().run(source.clone()));
        let state = settle(&again, |s| matches!(s, UpdateState::Ready(_))).await;
        assert!(matches!(state, UpdateState::Ready(k) if k.version == "0.1.1"));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(source.checks.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn once_a_day_across_restarts() {
        let rig = Rig::new();
        rig.prefs.stored.lock().unwrap().last_check = Some(LastCheck {
            at: T0 - DAY + 3_600_000,
            reached: true,
        });
        let updates = rig.updates(rig.env());
        let source = FakeSource::new(Answer::Nothing);
        tokio::spawn(updates.clone().run(source.clone()));
        let state = settle(&updates, |s| matches!(s, UpdateState::UpToDate { .. })).await;
        assert_eq!(
            state,
            UpdateState::UpToDate {
                checked_at: T0 - DAY + 3_600_000
            }
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            source.checks.load(Ordering::SeqCst),
            0,
            "checked less than a day ago"
        );

        // A day after the last check, a new start checks.
        rig.clock.store(T0 + 3_600_000, Ordering::SeqCst);
        let later = rig.updates(rig.env());
        let source = FakeSource::new(Answer::Nothing);
        tokio::spawn(later.clone().run(source.clone()));
        settle(&later, |s| {
            *s == UpdateState::UpToDate {
                checked_at: T0 + 3_600_000,
            }
        })
        .await;
        assert_eq!(source.checks.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn switched_off_it_checks_only_on_the_button() {
        let rig = Rig::new();
        rig.prefs.stored.lock().unwrap().auto_check = false;
        let updates = rig.updates(rig.env());
        let source = FakeSource::new(Answer::Offline);
        tokio::spawn(updates.clone().run(source.clone()));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(source.checks.load(Ordering::SeqCst), 0);
        assert_eq!(updates.snapshot().state, UpdateState::NotChecked);
        updates.check_now(source.as_ref()).await;
        assert_eq!(
            updates.snapshot().state,
            UpdateState::Unreachable { checked_at: T0 }
        );
        assert_eq!(
            rig.prefs.stored.lock().unwrap().last_check,
            Some(LastCheck {
                at: T0,
                reached: false
            })
        );
    }

    #[tokio::test]
    async fn a_stopped_download_and_a_refused_signature_keep_nothing() {
        let rig = Rig::new();
        let updates = rig.updates(rig.env());
        let stopped = FakeSource::new(Answer::Version("0.2.0", Err(FetchFailed::Stopped)));
        updates.check_now(stopped.as_ref()).await;
        assert_eq!(
            updates.snapshot().state,
            UpdateState::DownloadStopped {
                version: "0.2.0".into(),
                done: 3 << 20,
                total: Some(5 << 20)
            }
        );
        let refused = FakeSource::new(Answer::Version("0.2.0", Err(FetchFailed::SignatureRefused)));
        updates.check_now(refused.as_ref()).await;
        assert_eq!(
            updates.snapshot().state,
            UpdateState::SignatureRefused {
                version: "0.2.0".into()
            }
        );
        assert!(!rig.dir.exists() || std::fs::read_dir(&rig.dir).unwrap().next().is_none());
        // The next check downloads it again.
        let fine = FakeSource::new(Answer::Version("0.2.0", Ok(b"ok")));
        updates.check_now(fine.as_ref()).await;
        assert!(matches!(updates.snapshot().state, UpdateState::Ready(_)));
    }

    #[tokio::test]
    async fn the_deb_never_downloads() {
        let rig = Rig::new();
        let updates = rig.updates(Env {
            by_hand: true,
            ..rig.env()
        });
        let source = FakeSource::new(Answer::Version("0.2.0", Ok(b"never")));
        updates.check_now(source.as_ref()).await;
        assert!(matches!(
            updates.snapshot().state,
            UpdateState::Available { ref version, .. } if version == "0.2.0"
        ));
        assert!(!rig.dir.exists());
    }

    #[tokio::test]
    async fn built_from_source_and_not_installed_never_check() {
        let rig = Rig::new();
        let source = FakeSource::new(Answer::Nothing);
        let built = rig.updates(Env {
            checks: false,
            ..rig.env()
        });
        tokio::spawn(built.clone().run(source.clone()));
        built.check_now(source.as_ref()).await;
        let not_installed = rig.updates(Env {
            installed: false,
            ..rig.env()
        });
        tokio::spawn(not_installed.clone().run(source.clone()));
        not_installed.check_now(source.as_ref()).await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(source.checks.load(Ordering::SeqCst), 0);
        assert_eq!(built.snapshot().state, UpdateState::BuiltFromSource);
    }

    #[tokio::test]
    async fn skip_this_version_is_stored() {
        let rig = Rig::new();
        let updates = rig.updates(rig.env());
        updates
            .check_now(FakeSource::new(Answer::Version("0.1.1", Ok(b"x"))).as_ref())
            .await;
        updates.skip().await;
        assert_eq!(updates.snapshot().prefs.skipped.as_deref(), Some("0.1.1"));
        assert_eq!(
            rig.prefs.stored.lock().unwrap().skipped.as_deref(),
            Some("0.1.1")
        );
        // 0.1.2 replaces the kept 0.1.1 and isn't skipped.
        updates
            .check_now(FakeSource::new(Answer::Version("0.1.2", Ok(b"y"))).as_ref())
            .await;
        assert!(
            matches!(updates.snapshot().state, UpdateState::Ready(ref k) if k.version == "0.1.2")
        );
        let names: Vec<_> = std::fs::read_dir(&rig.dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["0.1.2"]);
    }

    #[test]
    fn half_written_and_older_folders_are_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        let kept = |v: &str| Kept {
            version: v.into(),
            notes_url: vm::release_page(v),
            protocol_version: Some(1),
            downloaded_at: T0,
        };
        keep(dir, &kept("0.1.1"), &Value::Null, b"a").unwrap();
        // A newer version whose writing stopped, and one already installed.
        std::fs::create_dir_all(dir.join(".0.1.3.partial-9")).unwrap();
        std::fs::create_dir_all(dir.join("0.1.2")).unwrap();
        std::fs::write(dir.join("0.1.2").join(KEPT_JSON), "{").unwrap();
        assert_eq!(load_kept(dir, "0.1.0").unwrap().version, "0.1.1");
        let names: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(names, ["0.1.1"]);
        // Once 0.1.1 runs, its folder is old.
        assert_eq!(load_kept(dir, "0.1.1"), None);
        assert!(std::fs::read_dir(dir).unwrap().next().is_none());
    }

    #[test]
    fn notes_url_and_protocol_from_latest_json() {
        let raw = serde_json::json!({"protocol_version": 3, "notes_url": "https://example.com/n"});
        assert_eq!(protocol_version(&raw), Some(3));
        assert_eq!(notes_url("0.2.0", &raw), "https://example.com/n");
        assert_eq!(notes_url("0.2.0", &Value::Null), vm::release_page("0.2.0"));
        assert_eq!(protocol_version(&Value::Null), None);
    }

    /// Records what the restart did, and fails when told to.
    #[derive(Default)]
    struct FakeInstall {
        log: Arc<Mutex<Vec<String>>>,
        fail: Option<InstallFailed>,
    }

    impl FakeInstall {
        fn log_handle(&self) -> Arc<Mutex<Vec<String>>> {
            self.log.clone()
        }
    }

    impl Installer for FakeInstall {
        fn prepare<'a>(
            &'a self,
            _dir: &'a Path,
            version: &'a str,
        ) -> BoxFuture<'a, Result<Box<dyn Prepared>, InstallFailed>> {
            self.log.lock().unwrap().push(format!("prepare {version}"));
            let ready = FakePrepared(self.fail.clone(), self.log_handle());
            Box::pin(async move {
                if let Some(failed) = ready.0.clone().filter(|f| f.offline) {
                    return Err(failed);
                }
                Ok(Box::new(ready) as Box<dyn Prepared>)
            })
        }
    }

    struct FakePrepared(Option<InstallFailed>, Arc<Mutex<Vec<String>>>);

    impl Prepared for FakePrepared {
        fn install(self: Box<Self>) -> Result<(), InstallFailed> {
            self.1.lock().unwrap().push("install".into());
            self.0.map_or(Ok(()), Err)
        }
    }

    impl ServiceStop for FakeInstall {
        fn stop(&self) -> BoxFuture<'_, Result<(), String>> {
            self.log.lock().unwrap().push("stop service".into());
            Box::pin(async { Ok(()) })
        }
    }

    async fn ready(rig: &Rig, protocol: u32) -> Arc<Updates> {
        let updates = rig.updates(rig.env());
        let source = FakeSource::new(Answer::Version("0.2.0", Ok(b"app")));
        updates.check_now(source.as_ref()).await;
        // The fake's latest.json says protocol 1; rewrite the kept file for another number.
        if protocol != 1 {
            let path = rig.dir.join("0.2.0").join(KEPT_JSON);
            let mut file: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            file["protocol_version"] = protocol.into();
            std::fs::write(&path, file.to_string()).unwrap();
            let again = rig.updates(rig.env());
            *again.kept.lock().unwrap() = load_kept(&rig.dir, "0.1.0");
            return again;
        }
        updates
    }

    #[tokio::test]
    async fn restart_now_puts_it_in_place_as_the_table_says() {
        use vm::Trigger::*;
        let rig = Rig::new();
        // Compatible: only the app, from About, the menu or the next open.
        for trigger in [About, Menu, NextOpen] {
            let updates = ready(&rig, 1).await;
            let fake = FakeInstall::default();
            assert_eq!(
                updates.put_in_place(trigger, Some(1), &fake, &fake).await,
                Placed::Relaunch
            );
            assert_eq!(
                *fake.log.lock().unwrap(),
                ["prepare 0.2.0", "install"],
                "{trigger:?}"
            );
        }
        // Incompatible: the service stops first from About and the menu; the next open waits.
        for trigger in [About, Menu] {
            let updates = ready(&rig, 2).await;
            let fake = FakeInstall::default();
            assert_eq!(
                updates.put_in_place(trigger, Some(1), &fake, &fake).await,
                Placed::Relaunch
            );
            // The update is confirmed before anything stops.
            assert_eq!(
                *fake.log.lock().unwrap(),
                ["prepare 0.2.0", "stop service", "install"],
                "{trigger:?}"
            );
        }
        let updates = ready(&rig, 2).await;
        let fake = FakeInstall::default();
        assert_eq!(
            updates.put_in_place(NextOpen, Some(1), &fake, &fake).await,
            Placed::Nothing
        );
        assert!(fake.log.lock().unwrap().is_empty());
        assert!(install_at_open(&rig.env(), Some(1)).is_none());
        assert_eq!(install_at_open(&rig.env(), None).unwrap().version, "0.2.0");
    }

    #[tokio::test]
    async fn a_failed_install_says_why_and_keeps_the_old_version() {
        let rig = Rig::new();
        let updates = ready(&rig, 1).await;
        let fake = FakeInstall {
            fail: Some(InstallFailed {
                folder: "/Applications/Board.app".into(),
                permission: true,
                offline: false,
                error: String::new(),
            }),
            ..Default::default()
        };
        let placed = updates
            .put_in_place(vm::Trigger::About, Some(1), &fake, &fake)
            .await;
        let Placed::Failed { version, why } = placed else {
            panic!("{placed:?}")
        };
        assert_eq!(version, "0.2.0");
        assert!(
            why.starts_with("/Applications/Board.app belongs to another account"),
            "{why}"
        );
        assert!(matches!(
            updates.snapshot().state,
            UpdateState::NotReplaced { .. }
        ));
        assert!(!updates.snapshot().installing);
        // The reopened app reads it back and shows it instead of "ready".
        let value = failed_value(&version, &why);
        assert_eq!(read_failed(&value), Some((version, why)));
        assert!(kept_bytes(&rig.dir, "0.2.0").is_file());
    }

    #[tokio::test]
    async fn the_updated_toast_shows_once_per_version() {
        let rig = Rig::new();
        rig.prefs.stored.lock().unwrap().last_version = Some("0.0.9".into());
        let first = rig.updates(rig.env());
        tokio::spawn(first.clone().run(FakeSource::new(Answer::Nothing)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        let (title, _) = first.take_updated_toast().unwrap();
        assert!(title.ends_with(" 0.1.0"), "{title}");
        assert_eq!(
            rig.prefs.stored.lock().unwrap().last_version.as_deref(),
            Some("0.1.0")
        );
        let second = rig.updates(rig.env());
        tokio::spawn(second.clone().run(FakeSource::new(Answer::Nothing)));
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(second.take_updated_toast(), None);
    }

    #[tokio::test]
    async fn offline_it_stays_ready_and_stops_nothing() {
        let rig = Rig::new();
        let updates = ready(&rig, 2).await;
        let fake = FakeInstall {
            fail: Some(InstallFailed {
                folder: String::new(),
                permission: false,
                offline: true,
                error: "offline".into(),
            }),
            ..Default::default()
        };
        let placed = updates
            .put_in_place(vm::Trigger::About, Some(1), &fake, &fake)
            .await;
        assert_eq!(placed, Placed::Offline);
        assert_eq!(*fake.log.lock().unwrap(), ["prepare 0.2.0"]);
        assert!(matches!(
            updates.snapshot().state,
            UpdateState::Ready(_) | UpdateState::NotChecked
        ));
    }

    #[test]
    fn the_kept_bytes_are_checked_against_the_signature() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/updater-test-key");
        let bytes = std::fs::read(fixture.join("payload.txt")).unwrap();
        let signature = std::fs::read_to_string(fixture.join("payload.txt.sig")).unwrap();
        let key = std::fs::read_to_string(fixture.join("test.key.pub")).unwrap();
        assert!(signature_matches(&bytes, &signature, &key));
        let mut tampered = bytes.clone();
        tampered[0] ^= 1;
        assert!(!signature_matches(&tampered, &signature, &key));
        assert!(!signature_matches(&bytes, &signature, PUBKEY));
        assert!(!signature_matches(&bytes, "not base64", &key));
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_folder_isnt_writable() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let app = tmp.path().join("Board.app");
        std::fs::create_dir(&app).unwrap();
        assert!(folder_writable(&app));
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o555)).unwrap();
        assert!(!folder_writable(&app));
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        assert!(!folder_writable(&app));
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            std::fs::read_dir(&app).unwrap().next().is_none(),
            "the probe leaves nothing behind"
        );
    }
}
