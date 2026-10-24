//! Anonymous usage data (d0085): which features are used and how often, sent to Aptabase only after the user said yes
//! (Settings `usage_data`, asked once by the "Help improve Otaq" dialog).
//!
//! - **Nothing without a yes.** Until the stored answer is "on", [`Usage::record`] drops everything: nothing is counted,
//!   kept or sent. Turning it off empties what was counted and not sent yet.
//! - **Nothing without an App Key.** Release builds have Otaq's key built in (`build.rs`); debug builds have none and
//!   never open a connection.
//! - **Only feature names.** A feature is a page request on the [`feature`] list (`board.drop`, `review.approve`…);
//!   loads, polls and keystrokes aren't on it. The few that carry a property carry a fixed word picked by the app (the
//!   column, the button, the setting's name), never text the user typed, a ticket, a project or a path.
//! - **No ID that follows anyone.** Each launch draws a new random session; nothing is stored on disk.
//!
//! Counts are kept in memory and sent in one batch every [`FLUSH_EVERY`] and when the app quits, each feature once with
//! how many times it was used. A failed send is dropped, not retried.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use otaq_core::naming::COMMAND_NAME;
use serde_json::{Value, json};

/// How often the counts are sent while the app runs.
pub const FLUSH_EVERY: Duration = Duration::from_secs(5 * 60);
/// How long quitting waits for the last batch.
const EXIT_WAIT: Duration = Duration::from_secs(2);
const HTTP_TIMEOUT: Duration = Duration::from_secs(10);
/// Aptabase takes at most 25 events per request.
const BATCH: usize = 25;

/// The App Key built into release builds (`A-EU-…`, `A-US-…`, or `A-SH-…` with `OTAQ_APTABASE_HOST`), from
/// `build.rs`; none in debug builds.
const APP_KEY: Option<&str> = option_env!("USAGE_APP_KEY");
const SELF_HOSTED: Option<&str> = option_env!("USAGE_HOST");

/// One counted use: the feature, and its one fixed property if it has one.
type Key = (&'static str, Option<(&'static str, String)>);

/// Where the events go: Aptabase's ingestion endpoint for the key's region.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoint {
    pub app_key: String,
    pub url: String,
}

impl Endpoint {
    /// The endpoint for an App Key, or none for a missing or malformed one (nothing is sent then).
    pub fn for_key(app_key: &str, self_hosted: Option<&str>) -> Option<Self> {
        let mut parts = app_key.split('-');
        let (Some("A"), Some(region), Some(id), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            return None;
        };
        if id.is_empty() {
            return None;
        }
        let base = match region {
            "EU" => "https://eu.aptabase.com".to_owned(),
            "US" => "https://us.aptabase.com".to_owned(),
            "SH" => self_hosted?.trim_end_matches('/').to_owned(),
            _ => return None,
        };
        Some(Self {
            app_key: app_key.to_owned(),
            url: format!("{base}/api/v0/events"),
        })
    }
}

/// What every event says about this copy of the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct System {
    pub app_version: String,
    /// `macOS`, `Windows` or `Linux`.
    pub os_name: String,
    /// `26.0`, `10.0.26100.4652`, `Ubuntu 26.04`.
    pub os_version: String,
    pub debug: bool,
}

impl System {
    /// This build on this machine (the OS as Settings → About reads it).
    fn current() -> Self {
        let os = &crate::about::build().os;
        let (os_name, os_version) = split_os(std::env::consts::OS, os);
        Self {
            app_version: env!("CARGO_PKG_VERSION").to_owned(),
            os_name,
            os_version,
            debug: cfg!(debug_assertions),
        }
    }
}

/// About's "macOS 26.0" as Aptabase's OS name and version; on Linux the distribution is the version.
fn split_os(os: &str, about: &str) -> (String, String) {
    let name = match os {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    let version = about
        .strip_prefix(name)
        .map(str::trim)
        .unwrap_or(if os == "linux" { about } else { "" });
    let version = if version == os { "" } else { version };
    (name.to_owned(), version.to_owned())
}

pub struct Usage {
    /// None: no App Key in this build; nothing is ever sent.
    sender: Option<(Endpoint, reqwest::Client)>,
    system: System,
    /// The user's answer, as last read from the settings: only `true` counts anything.
    on: AtomicBool,
    /// `app_started` goes once per launch, when sending is first on.
    started: AtomicBool,
    counts: Mutex<BTreeMap<Key, u32>>,
    session: String,
    runtime: tokio::runtime::Handle,
}

impl Usage {
    /// The app's one instance. Sends nothing until [`Usage::set_on`] says the user agreed.
    pub fn new(runtime: tokio::runtime::Handle) -> Self {
        let endpoint = APP_KEY.and_then(|key| Endpoint::for_key(key, SELF_HOSTED));
        Self::with(endpoint, System::current(), runtime)
    }

    fn with(endpoint: Option<Endpoint>, system: System, runtime: tokio::runtime::Handle) -> Self {
        let sender = endpoint.and_then(|endpoint| {
            let client = reqwest::Client::builder()
                .timeout(HTTP_TIMEOUT)
                .build()
                .ok()?;
            Some((endpoint, client))
        });
        Self {
            sender,
            system,
            on: AtomicBool::new(false),
            started: AtomicBool::new(false),
            counts: Mutex::default(),
            session: session_id(),
            runtime,
        }
    }

    /// Follows the stored answer. Off empties what was counted and not sent yet.
    pub fn set_on(&self, on: bool) {
        self.on.store(on, Ordering::SeqCst);
        if !on {
            self.counts.lock().expect("usage counts").clear();
        } else if !self.started.swap(true, Ordering::SeqCst) {
            self.count(("app_started", None));
        }
    }

    pub fn is_on(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }

    /// A page request: counted when sending is on and the method is a feature.
    pub fn record(&self, method: &str, params: &Value) {
        if !self.is_on() {
            return;
        }
        if let Some(key) = feature(method, params) {
            self.count(key);
        }
    }

    fn count(&self, key: Key) {
        *self
            .counts
            .lock()
            .expect("usage counts")
            .entry(key)
            .or_default() += 1;
    }

    /// Takes the counts as Aptabase events, emptying them.
    fn take_events(&self) -> Vec<Value> {
        let counts = std::mem::take(&mut *self.counts.lock().expect("usage counts"));
        let timestamp = rfc3339(SystemTime::now());
        counts
            .into_iter()
            .map(|((name, prop), count)| self.event(&timestamp, name, prop, count))
            .collect()
    }

    fn event(
        &self,
        timestamp: &str,
        name: &str,
        prop: Option<(&str, String)>,
        count: u32,
    ) -> Value {
        let mut props = serde_json::Map::new();
        if let Some((key, value)) = prop {
            props.insert(key.to_owned(), Value::String(value));
        }
        props.insert("count".to_owned(), json!(count));
        json!({
            "timestamp": timestamp,
            "sessionId": self.session,
            "eventName": name,
            "systemProps": {
                "isDebug": self.system.debug,
                "osName": self.system.os_name,
                "osVersion": self.system.os_version,
                "locale": "",
                "appVersion": self.system.app_version,
                "sdkVersion": format!("{COMMAND_NAME}-usage@{}", env!("CARGO_PKG_VERSION")),
            },
            "props": props,
        })
    }

    /// Sends what was counted, if sending is on and this build has a key. A failure drops the batch.
    pub async fn flush(&self) {
        if !self.is_on() {
            return;
        }
        let Some((endpoint, client)) = &self.sender else {
            self.counts.lock().expect("usage counts").clear();
            return;
        };
        let events = self.take_events();
        for batch in events.chunks(BATCH) {
            let _ = client
                .post(&endpoint.url)
                .header("App-Key", &endpoint.app_key)
                .json(&batch)
                .send()
                .await;
        }
    }

    /// The last batch when the app quits, waiting at most [`EXIT_WAIT`]. Called from the window's event loop, outside
    /// the runtime.
    pub fn flush_before_exit(&self) {
        if !self.is_on() || self.sender.is_none() {
            return;
        }
        let _ = self
            .runtime
            .block_on(async { tokio::time::timeout(EXIT_WAIT, self.flush()).await });
    }
}

/// The page requests that count as using a feature, and the one fixed property a few of them carry. Everything else
/// (loads, polls, typing, the terminal's bytes) is never counted.
pub fn feature(method: &str, params: &Value) -> Option<Key> {
    let name: &'static str = FEATURES.iter().find(|f| **f == method)?;
    let prop = match method {
        "board.drop" => word(params, "column").map(|v| ("column", v)),
        "panel.action" => word(params, "action").map(|v| ("action", v)),
        "settings.edit" => word(params, "field").map(|v| ("field", v)),
        "theme.pick" => word(params, "key").map(|v| ("theme", v)),
        _ => None,
    };
    Some((name, prop))
}

/// A property's value only when it is a short identifier (`in_review`, `auto_merge`, `tokyo-night`): an enum or a
/// setting's name, never free text.
fn word(params: &Value, key: &str) -> Option<String> {
    let value = params.get(key)?.as_str()?;
    let ok = !value.is_empty()
        && value.len() <= 40
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b));
    ok.then(|| value.to_owned())
}

/// The features, by page request.
const FEATURES: &[&str] = &[
    // The board.
    "board.drop",
    "board.park",
    "board.park_all",
    "board.delete",
    "board.search",
    "board.lead_open",
    "board.lead_resume",
    "board.menu_choose",
    // The ticket editor.
    "editor.save",
    "editor.link",
    "editor.source_mode",
    // The ticket sheet.
    "panel.action",
    "panel.answer",
    "panel.answer_agent",
    "panel.send_again",
    "panel.submit",
    "panel.priority",
    "panel.close",
    "panel.mark_closed",
    "panel.remove_dependency",
    // Review and the Files tab.
    "review.approve",
    "review.local_merge",
    "review.send_back",
    "review.change",
    "diff.comment_add",
    "diff.comment_reply",
    "diff.mode",
    "diff.viewed",
    // Pull requests.
    "pr.start",
    "pr.confirm",
    "pr.retry",
    // The Needs you strip.
    "strip.next",
    "strip.resume_all",
    "strip.suggestion",
    // Epics.
    "sections.create_from_note",
    "sections.move_to_epic",
    "sections.close_epic",
    "sections.answer_message",
    // Terminals, agents and Run.
    "drawer.new",
    "drawer.resume",
    "terminal_tab.deliver_now",
    "run.start",
    "run.stop",
    "run.stop_all",
    "run.shell",
    "run.command_ask",
    "run.command_save",
    // Recovery and Cleanup.
    "recovery.resume",
    "recovery.resume_agent",
    "recovery.start_from_scratch",
    "recovery.cleanup_delete",
    "recovery.warm_clear",
    "shell.cleanup",
    "notices.resume_all",
    "notices.resume_after_limit",
    "notices.restart",
    // Projects, settings and the look.
    "onboarding.add",
    "settings.edit",
    "settings.remove",
    "theme.pick",
];

/// A new session for this launch, in the form Aptabase's own SDKs use (the start second, then 8 random digits).
fn session_id() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let random = u64::from_le_bytes(
        uuid::Uuid::new_v4().as_bytes()[..8]
            .try_into()
            .unwrap_or_default(),
    ) % 100_000_000;
    (seconds * 100_000_000 + random).to_string()
}

/// `2026-10-17T09:30:00Z`, in UTC.
fn rfc3339(time: SystemTime) -> String {
    let secs = time.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system() -> System {
        System {
            app_version: "0.1.0".into(),
            os_name: "Windows".into(),
            os_version: "10.0.26100".into(),
            debug: false,
        }
    }

    fn usage() -> Usage {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let endpoint = Endpoint::for_key("A-EU-0000000000", None);
        let usage = Usage::with(endpoint, system(), runtime.handle().clone());
        // The handle outlives the test's runtime only in flush_before_exit, which these tests don't call.
        std::mem::forget(runtime);
        usage
    }

    #[test]
    fn nothing_is_counted_before_the_user_says_yes() {
        let u = usage();
        u.record("board.drop", &json!({ "key": "SHOP-1", "column": "dev" }));
        assert!(u.take_events().is_empty());
    }

    #[test]
    fn turning_it_off_drops_what_was_not_sent() {
        let u = usage();
        u.set_on(true);
        u.record("review.approve", &json!({}));
        u.set_on(false);
        assert!(u.take_events().is_empty());
        u.record("review.approve", &json!({}));
        assert!(u.take_events().is_empty());
    }

    #[test]
    fn features_are_counted_once_per_batch_with_how_often() {
        let u = usage();
        u.set_on(true);
        for _ in 0..3 {
            u.record("board.drop", &json!({ "key": "SHOP-1", "column": "dev" }));
        }
        u.record("board.view", &json!({ "project": "SHOP" }));
        u.record("terminal.input", &json!({ "data": "secret" }));
        let events = u.take_events();
        let names: Vec<_> = events.iter().map(|e| e["eventName"].clone()).collect();
        assert_eq!(names, [json!("app_started"), json!("board.drop")]);
        let drop = &events[1];
        assert_eq!(drop["props"], json!({ "column": "dev", "count": 3 }));
        assert_eq!(drop["systemProps"]["osName"], "Windows");
        assert_eq!(drop["systemProps"]["appVersion"], "0.1.0");
        assert_eq!(drop["sessionId"], events[0]["sessionId"]);
        // The ticket's key never leaves.
        assert!(!drop.to_string().contains("SHOP"));
        assert!(u.take_events().is_empty(), "taken once");
        // app_started only once per launch.
        u.set_on(false);
        u.set_on(true);
        assert!(u.take_events().is_empty());
    }

    #[test]
    fn properties_are_fixed_words_never_free_text() {
        let key = |params| feature("settings.edit", &params).unwrap().1;
        assert_eq!(
            key(json!({ "field": "auto_merge", "value": "on" })),
            Some(("field", "auto_merge".to_owned()))
        );
        assert_eq!(key(json!({ "field": "My secret project" })), None);
        assert_eq!(key(json!({ "field": "/home/me/repo" })), None);
        assert_eq!(key(json!({ "field": 3 })), None);
        assert_eq!(
            feature("editor.save", &json!({ "title": "x" })),
            Some(("editor.save", None))
        );
        assert_eq!(feature("editor.open", &json!({})), None);
    }

    #[test]
    fn app_keys_pick_their_region_and_bad_ones_send_nothing() {
        let eu = Endpoint::for_key("A-EU-1234567890", None).unwrap();
        assert_eq!(eu.url, "https://eu.aptabase.com/api/v0/events");
        let us = Endpoint::for_key("A-US-1234567890", None).unwrap();
        assert_eq!(us.url, "https://us.aptabase.com/api/v0/events");
        let sh = Endpoint::for_key("A-SH-1234567890", Some("https://stats.example.com/")).unwrap();
        assert_eq!(sh.url, "https://stats.example.com/api/v0/events");
        for bad in [
            "",
            "A-SH-1234567890",
            "A-XX-1",
            "A-EU-",
            "not-a-key",
            "A-EU-1-2",
        ] {
            assert_eq!(Endpoint::for_key(bad, None), None, "{bad:?}");
        }
    }

    #[test]
    fn the_os_splits_into_name_and_version() {
        let s = |os, about| split_os(os, about);
        assert_eq!(s("macos", "macOS 26.0"), ("macOS".into(), "26.0".into()));
        assert_eq!(
            s("windows", "Windows 10.0.26100.4652"),
            ("Windows".into(), "10.0.26100.4652".into())
        );
        assert_eq!(
            s("linux", "Ubuntu 26.04"),
            ("Linux".into(), "Ubuntu 26.04".into())
        );
        assert_eq!(s("windows", "windows"), ("Windows".into(), "".into()));
    }

    #[test]
    fn timestamps_are_utc_rfc3339() {
        assert_eq!(rfc3339(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        let t = UNIX_EPOCH + Duration::from_secs(1_792_229_400);
        assert_eq!(rfc3339(t), "2026-10-17T09:30:00Z");
        let leap = UNIX_EPOCH + Duration::from_secs(951_782_400);
        assert_eq!(rfc3339(leap), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn sessions_are_new_each_launch() {
        assert_ne!(session_id(), session_id());
        assert!(session_id().len() >= 17);
    }
}
