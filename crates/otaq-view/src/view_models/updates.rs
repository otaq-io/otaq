//! Self-updates in Settings → About (UPD-04, UPD-05, d0140; page 32 items 1–2): the **Check for updates
//! automatically** switch, the **Updates** row with **Check for updates** and the last check's result under it, the
//! notice at the top of About when a newer version is ready (or, from a `.deb`, available), and the dot on Settings
//! in the rail. The app's updater (`otaq-desktop/src/updates.rs`) keeps an [`UpdateState`]; this words it, and holds
//! the rules it follows: when a check is due, what "compatible" means, which installs update by hand.

use serde::Serialize;
use ts_rs::TS;

use otaq_core::naming::PRODUCT_NAME;

use crate::icons::IconName;
use crate::view_models::clock;

/// A day: the automatic check runs once per day (UPD-04).
pub const CHECK_EVERY_MS: i64 = 24 * 60 * 60 * 1000;

/// The release page of a version, where What's new and a `.deb`'s Download go when `latest.json` names no page.
pub fn release_page(version: &str) -> String {
    otaq_core::naming::release_page(version)
}

/// What the app's updater knows now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateState {
    /// No check yet in this data folder.
    NotChecked,
    Checking,
    Downloading {
        version: String,
        done: u64,
        total: Option<u64>,
    },
    UpToDate {
        checked_at: i64,
    },
    /// GitHub didn't answer (offline, a timeout, an unreadable `latest.json`).
    Unreachable {
        checked_at: i64,
    },
    /// The download stopped partway; what came is deleted and the next check tries again.
    DownloadStopped {
        version: String,
        done: u64,
        total: Option<u64>,
    },
    /// The file's signature didn't match the built-in public key; it wasn't kept.
    SignatureRefused {
        version: String,
    },
    /// The downloaded version couldn't replace the installed app (T-0581): `why` says so, naming the folder.
    NotReplaced {
        version: String,
        why: String,
    },
    /// Downloaded, checked and kept in `<data folder>/updates/<version>/`, waiting for Restart now or the next open.
    Ready(Kept),
    /// A newer version, for an install that doesn't update itself (the `.deb`): never downloaded.
    Available {
        version: String,
        notes_url: String,
        checked_at: i64,
    },
    /// A build from source never checks.
    BuiltFromSource,
}

/// A downloaded update, as kept on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Kept {
    pub version: String,
    /// What's new: the release notes page.
    pub notes_url: String,
    /// `latest.json`'s `protocol_version` (T-0583); `None` when it had none (counts as incompatible).
    pub protocol_version: Option<u32>,
    /// When it was kept, in Unix milliseconds.
    pub downloaded_at: i64,
}

/// What the rest of the app says about this moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// This app's version: `0.1.0`.
    pub current_version: String,
    /// The short commit it was built from, for the built-from-source line.
    pub commit: String,
    pub now: i64,
    pub auto_check: bool,
    /// The version Skip this version hides.
    pub skipped: Option<String>,
    /// The running service: its protocol number and its agents working now. `None` while not connected.
    pub service: Option<ServiceSide>,
    /// This app's own protocol number, used while the service's isn't known.
    pub app_protocol: u32,
    /// Restart now is putting the update in place: its button is off.
    pub installing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServiceSide {
    pub protocol_version: u32,
    pub agents_working: u32,
}

// ---- The rules ----

/// Installed from the `.deb`: Linux, not running as an AppImage (`APPIMAGE` unset). It never downloads; About offers
/// the release page instead.
pub fn updates_by_hand(os: &str, appimage: Option<&str>) -> bool {
    os == "linux" && appimage.is_none_or(str::is_empty)
}

/// Whether the automatic check is due: on, and a day since the last check (stored by the service, so a restart of
/// the app doesn't check again early). `last_check` is its time in Unix milliseconds.
pub fn check_due(auto_check: bool, last_check: Option<i64>, now: i64) -> bool {
    auto_check && last_check.is_none_or(|at| now - at >= CHECK_EVERY_MS || at > now)
}

/// How long until the automatic check is due (zero when it is), for the updater's sleep.
pub fn due_in_ms(last_check: Option<i64>, now: i64) -> i64 {
    match last_check {
        Some(at) if at <= now => (at + CHECK_EVERY_MS - now).max(0),
        _ => 0,
    }
}

/// Compatible: the update speaks the running service's protocol, so only the app restarts and agents keep working.
pub fn compatible(update_protocol: Option<u32>, service_protocol: u32) -> bool {
    update_protocol == Some(service_protocol)
}

/// `a` is a newer version than `b` (`0.10.0` > `0.9.1`); numeric parts, a pre-release suffix ignored.
pub fn newer(a: &str, b: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim_start_matches('v')
            .split(['-', '+'])
            .next()
            .unwrap_or_default()
            .split('.')
            .map(|p| p.parse().unwrap_or(0))
            .collect()
    }
    let (a, b) = (parts(a), parts(b));
    let len = a.len().max(b.len());
    let at = |v: &[u64], i: usize| v.get(i).copied().unwrap_or(0);
    (0..len)
        .map(|i| at(&a, i).cmp(&at(&b, i)))
        .find(|o| o.is_ne())
        .is_some_and(|o| o.is_gt())
}

// ---- The view ----

/// Settings → About's update parts and the rail's dot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdatesView")]
pub struct View {
    /// Check for updates automatically; `None` for a build from source.
    pub auto: Option<AutoRow>,
    pub row: UpdatesRow,
    /// The notice at the top of About.
    pub notice: Option<Notice>,
    /// The dot on Settings in the rail, with its tooltip line ("Otaq 0.2.0 is ready").
    pub dot: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdatesAutoRow")]
pub struct AutoRow {
    pub label: String,
    pub description: String,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdatesRow")]
pub struct UpdatesRow {
    pub label: String,
    /// The last check's result under the label.
    pub result: Option<UpdateResult>,
    /// A build from source's plain line instead of a result.
    pub plain: Option<String>,
    /// Check for updates; `None` for a build from source.
    pub check: Option<CheckButton>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdatesCheckButton")]
pub struct CheckButton {
    pub label: String,
    /// Off while a check or a download runs.
    pub enabled: bool,
}

/// How a result line is coloured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "UpdateTone")]
pub enum Tone {
    Plain,
    /// Couldn't reach GitHub, a download that stopped (`--st-paused`).
    Warning,
    /// A refused signature, an app that couldn't be replaced (`--st-error`).
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdateResult")]
pub struct UpdateResult {
    pub icon: IconName,
    pub tone: Tone,
    pub text: String,
    pub detail: Option<String>,
    /// While downloading: the bar and "32 of 52 MB".
    pub progress: Option<Progress>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdateProgress")]
pub struct Progress {
    /// Done, in thousandths of the whole; `None` while the size isn't known.
    pub permille: Option<u32>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "UpdateNoticeKind")]
pub enum NoticeKind {
    /// Restarts only the app.
    Ready,
    /// The running service can't work with it: Restart now restarts both.
    NeedsAgentsRestarted,
    /// From the `.deb`: Download the new package.
    Available,
}

/// What a notice's main button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename = "UpdateNoticeAction")]
pub enum NoticeAction {
    Restart,
    Download,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(rename = "UpdateNotice")]
pub struct Notice {
    pub kind: NoticeKind,
    pub version: String,
    pub title: String,
    pub body: String,
    pub whats_new_label: String,
    pub notes_url: String,
    pub skip_label: String,
    pub action: NoticeAction,
    pub action_label: String,
    /// The accent button; the needs-agents-restarted one isn't (the safe choice is to wait).
    pub primary: bool,
    /// Off while Restart now is putting the update in place.
    pub enabled: bool,
}

const CHECK_LABEL: &str = "Check for updates";

fn mb(bytes: u64) -> u64 {
    (bytes + 512 * 1024) / (1024 * 1024)
}

fn amount(done: u64, total: Option<u64>) -> String {
    match total {
        Some(total) => format!("{} of {} MB", mb(done), mb(total)),
        None => format!("{} MB", mb(done)),
    }
}

/// "2 min ago", "3 h ago", "just now".
fn ago(at: i64, now: i64) -> String {
    let minutes = (now - at).max(0) / 60_000;
    match minutes {
        0 => "just now".into(),
        1..=59 => format!("{minutes} min ago"),
        _ if minutes < 48 * 60 => format!("{} h ago", minutes / 60),
        _ => format!("{} days ago", minutes / (24 * 60)),
    }
}

fn result(
    icon: IconName,
    tone: Tone,
    text: String,
    detail: Option<String>,
) -> Option<UpdateResult> {
    Some(UpdateResult {
        icon,
        tone,
        text,
        detail,
        progress: None,
    })
}

/// The view of `state` at this moment.
pub fn view(state: &UpdateState, ctx: &Context) -> View {
    let name = PRODUCT_NAME;
    let current = &ctx.current_version;
    if *state == UpdateState::BuiltFromSource {
        return View {
            auto: None,
            row: UpdatesRow {
                label: "Updates".into(),
                result: None,
                plain: Some(format!(
                    "Built from source · {}. This build never checks for updates; a release never replaces it. Build it again to update.",
                    ctx.commit
                )),
                check: None,
            },
            notice: None,
            dot: None,
        };
    }
    let busy = matches!(
        state,
        UpdateState::Checking | UpdateState::Downloading { .. }
    );
    let result = match state {
        UpdateState::NotChecked | UpdateState::BuiltFromSource => None,
        UpdateState::Checking => result(
            IconName::LoaderCircle,
            Tone::Plain,
            "Checking GitHub…".into(),
            None,
        ),
        UpdateState::Downloading {
            version,
            done,
            total,
        } => Some(UpdateResult {
            icon: IconName::Download,
            tone: Tone::Plain,
            text: format!("Downloading {name} {version}…"),
            detail: None,
            progress: Some(Progress {
                permille: total
                    .filter(|t| *t > 0)
                    .map(|t| u32::try_from((done.min(&t) * 1000) / t).unwrap_or(1000)),
                text: amount(*done, *total),
            }),
        }),
        UpdateState::UpToDate { checked_at } => result(
            IconName::CircleCheck,
            Tone::Plain,
            format!(
                "You’re up to date · {current} · checked {}",
                ago(*checked_at, ctx.now)
            ),
            None,
        ),
        UpdateState::Unreachable { checked_at } => result(
            IconName::CircleAlert,
            Tone::Warning,
            format!(
                "Couldn’t reach GitHub · checked at {}",
                clock::local_hhmm(*checked_at)
            ),
            Some(format!(
                "{name} tries again at the next daily check. Check your connection, then Check for updates."
            )),
        ),
        UpdateState::DownloadStopped {
            version,
            done,
            total,
        } => result(
            IconName::CircleAlert,
            Tone::Warning,
            format!("{name} {version} didn’t finish downloading"),
            Some(format!(
                "The connection dropped at {}. It’s tried again at the next check, or now with Check for updates.",
                amount(*done, *total)
            )),
        ),
        UpdateState::SignatureRefused { version } => result(
            IconName::ShieldX,
            Tone::Error,
            format!("{name} {version} was refused: its signature didn’t match {name}’s"),
            Some(format!(
                "The file wasn’t used and you’re still on {current}. Check for updates downloads it again."
            )),
        ),
        UpdateState::NotReplaced { version, why } => result(
            IconName::CircleX,
            Tone::Error,
            format!("{name} {version} couldn’t be put in place"),
            Some(why.clone()),
        ),
        UpdateState::Ready(kept) => result(
            IconName::CircleCheck,
            Tone::Plain,
            format!(
                "{name} {} downloaded and checked · at {}",
                kept.version,
                clock::local_hhmm(kept.downloaded_at)
            ),
            None,
        ),
        UpdateState::Available {
            version,
            checked_at,
            ..
        } => result(
            IconName::Download,
            Tone::Plain,
            format!(
                "{name} {version} is available · checked at {}",
                clock::local_hhmm(*checked_at)
            ),
            None,
        ),
    };
    let notice = notice(state, ctx);
    View {
        auto: Some(AutoRow {
            label: "Check for updates automatically".into(),
            description: format!(
                "Once a day {name} asks GitHub for the newest version and downloads it. Nothing about you or your projects is sent. Nothing changes until you restart."
            ),
            on: ctx.auto_check,
        }),
        row: UpdatesRow {
            label: "Updates".into(),
            result,
            plain: None,
            check: Some(CheckButton {
                label: CHECK_LABEL.into(),
                enabled: !busy,
            }),
        },
        dot: notice.as_ref().map(|n| match n.kind {
            NoticeKind::Available => format!("{name} {} is available", n.version),
            _ => format!("{name} {} is ready", n.version),
        }),
        notice,
    }
}

/// "the 3 working agents stop", "the working agent stops".
fn agents_stop(working: u32) -> String {
    match working {
        1 => "the working agent stops and shows Interrupted".into(),
        n => format!("the {n} working agents stop and show Interrupted"),
    }
}

fn notice(state: &UpdateState, ctx: &Context) -> Option<Notice> {
    let name = PRODUCT_NAME;
    let (version, notes_url) = match state {
        UpdateState::Ready(kept) => (&kept.version, &kept.notes_url),
        UpdateState::Available {
            version, notes_url, ..
        } => (version, notes_url),
        _ => return None,
    };
    if ctx.skipped.as_deref() == Some(version.as_str()) {
        return None;
    }
    let base = Notice {
        kind: NoticeKind::Ready,
        version: version.clone(),
        title: format!("{name} {version} is ready"),
        body: format!(
            "Downloaded and checked. Restart now reopens the window on {version} in a few seconds; agents keep working. Or it’s put in place the next time you open {name}."
        ),
        whats_new_label: "What’s new".into(),
        notes_url: notes_url.clone(),
        skip_label: "Skip this version".into(),
        action: NoticeAction::Restart,
        action_label: "Restart now".into(),
        primary: true,
        enabled: !ctx.installing,
    };
    Some(match state {
        UpdateState::Available { .. } => Notice {
            kind: NoticeKind::Available,
            title: format!("{name} {version} is available"),
            body: format!(
                "{name} was installed from the .deb package, which doesn’t update itself. Download the new package and install it; agents keep working until the service restarts."
            ),
            action: NoticeAction::Download,
            action_label: "Download".into(),
            enabled: true,
            ..base
        },
        UpdateState::Ready(kept) => {
            let service = ctx.service.unwrap_or(ServiceSide {
                protocol_version: ctx.app_protocol,
                agents_working: 0,
            });
            if compatible(kept.protocol_version, service.protocol_version) {
                base
            } else {
                let working = service.agents_working;
                let (title, body, label) = if working == 0 {
                    (
                        format!("{name} {version} is ready · needs agents restarted"),
                        "This version can’t work with the background service that runs your agents. Restart now restarts both; no agent is working now.".to_owned(),
                        "Restart now".to_owned(),
                    )
                } else {
                    (
                        format!(
                            "{name} {version} is ready · needs agents restarted ({working} working)"
                        ),
                        format!(
                            "This version can’t work with the background service that runs your agents. Restart now restarts both: {}, and Resume all continues them where they were.",
                            agents_stop(working)
                        ),
                        format!(
                            "Restart now · stops {working} agent{}",
                            if working == 1 { "" } else { "s" }
                        ),
                    )
                };
                Notice {
                    kind: NoticeKind::NeedsAgentsRestarted,
                    title,
                    body,
                    action_label: label,
                    primary: false,
                    ..base
                }
            }
        }
        _ => unreachable!("only Ready and Available have a notice"),
    })
}

// ---- Restart now, Restart to update, the next open (T-0581) ----

/// Where a restart onto the kept update comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The notice's Restart now in About.
    About,
    /// The status icon's Restart to update.
    Menu,
    /// The app opens with an update kept from before.
    NextOpen,
}

/// What a restart does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Nothing to put in place (none kept), or an incompatible update at the next open: it waits for Restart now.
    Wait,
    /// Put the new app in place and reopen only it; the service and its agents keep running.
    AppOnly,
    /// The running service can't work with the new app: stop it as 0022's restart does (Interrupted, Resume all),
    /// then put the app in place; the new app starts its own service.
    WithService,
}

/// The decision table (d0140, gap review R1/G6): a compatible update restarts only the app, from About, the menu or
/// the next open; an incompatible one restarts the service too, from About or the menu only. With no service running
/// (`service_protocol` `None`) nothing is stopped.
pub fn restart_step(
    trigger: Trigger,
    kept_protocol: Option<Option<u32>>,
    service_protocol: Option<u32>,
) -> Step {
    let Some(kept_protocol) = kept_protocol else {
        return Step::Wait;
    };
    let Some(service) = service_protocol else {
        return Step::AppOnly;
    };
    match (compatible(kept_protocol, service), trigger) {
        (true, _) => Step::AppOnly,
        (false, Trigger::NextOpen) => Step::Wait,
        (false, Trigger::About | Trigger::Menu) => Step::WithService,
    }
}

/// The status icon's Restart to update (page 32 item 3): "Restart to update · Otaq 0.2.0", or "Restart to update ·
/// stops 3 agents" for one the service can't work with; only while an update is ready and not skipped.
pub fn restart_item(state: &UpdateState, ctx: &Context) -> Option<String> {
    let notice = notice(state, ctx)?;
    match notice.kind {
        NoticeKind::Ready => Some(format!(
            "Restart to update · {PRODUCT_NAME} {}",
            notice.version
        )),
        NoticeKind::NeedsAgentsRestarted => {
            let working = ctx.service.map_or(0, |s| s.agents_working);
            Some(if working == 0 {
                format!("Restart to update · {PRODUCT_NAME} {}", notice.version)
            } else {
                format!(
                    "Restart to update · stops {working} agent{}",
                    if working == 1 { "" } else { "s" }
                )
            })
        }
        NoticeKind::Available => None,
    }
}

/// The one-time toast after an update (page 32 item 4): "Updated to Otaq 0.2.0", when this version is newer than the
/// one the app last started as. Nothing on the very first start (`last` `None`).
pub fn updated_toast(last: Option<&str>, current: &str) -> Option<(String, String)> {
    last.filter(|last| newer(current, last)).map(|_| {
        (
            format!("Updated to {PRODUCT_NAME} {current}"),
            "The window reopened on the new version; your agents kept working.".to_owned(),
        )
    })
}

/// What's new on the toast.
pub const WHATS_NEW: &str = "What’s new";

/// Why the update couldn't be put in place, under "Otaq 0.2.0 couldn't be put in place" (page 32 item 1). `folder` is
/// the installed app; `permission` when the system refused for rights; `error` otherwise. macOS's rights line is page
/// 32's; elsewhere it's a stand-in.
pub fn not_replaced_why(
    folder: &str,
    permission: bool,
    error: &str,
    current: &str,
    os: &str,
) -> String {
    let name = PRODUCT_NAME;
    if permission && os == "macos" {
        format!(
            "{folder} belongs to another account on this Mac, so {name} can’t replace it. You’re still on {current}. Sign in as that account and update there, or move {name} to ~/Applications."
        )
    } else if permission {
        format!(
            "{folder} belongs to another account on this computer, so {name} can’t replace it. You’re still on {current}. Sign in as that account and update there, or move {name} to a folder of yours."
        )
    } else {
        format!("{name} couldn’t replace {folder}: {error}. You’re still on {current}.")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_793_000_000_000;

    fn ctx() -> Context {
        Context {
            current_version: "0.1.0".into(),
            commit: "bed9c10".into(),
            now: NOW,
            auto_check: true,
            skipped: None,
            service: Some(ServiceSide {
                protocol_version: 1,
                agents_working: 3,
            }),
            app_protocol: 1,
            installing: false,
        }
    }

    fn kept(version: &str, protocol: Option<u32>) -> UpdateState {
        UpdateState::Ready(Kept {
            version: version.into(),
            notes_url: release_page(version),
            protocol_version: protocol,
            downloaded_at: NOW - 60_000,
        })
    }

    fn text(state: &UpdateState) -> String {
        view(state, &ctx()).row.result.unwrap().text
    }

    #[test]
    fn every_result_as_page_32_words_it() {
        assert_eq!(
            text(&UpdateState::UpToDate {
                checked_at: NOW - 2 * 60_000
            }),
            "You’re up to date · 0.1.0 · checked 2 min ago"
        );
        assert_eq!(text(&UpdateState::Checking), "Checking GitHub…");
        let downloading = view(
            &UpdateState::Downloading {
                version: "0.2.0".into(),
                done: 32 * 1024 * 1024,
                total: Some(52 * 1024 * 1024),
            },
            &ctx(),
        );
        let r = downloading.row.result.unwrap();
        assert_eq!(r.text, format!("Downloading {PRODUCT_NAME} 0.2.0…"));
        let progress = r.progress.unwrap();
        assert_eq!(progress.text, "32 of 52 MB");
        assert_eq!(progress.permille, Some(615));
        assert!(!downloading.row.check.unwrap().enabled);

        let unreachable = view(&UpdateState::Unreachable { checked_at: NOW }, &ctx());
        let r = unreachable.row.result.unwrap();
        assert!(r.text.starts_with("Couldn’t reach GitHub · checked at "));
        assert_eq!(r.tone, Tone::Warning);
        assert_eq!(
            r.detail.unwrap(),
            format!(
                "{PRODUCT_NAME} tries again at the next daily check. Check your connection, then Check for updates."
            )
        );

        let stopped = view(
            &UpdateState::DownloadStopped {
                version: "0.2.0".into(),
                done: 31 * 1024 * 1024,
                total: Some(52 * 1024 * 1024),
            },
            &ctx(),
        )
        .row
        .result
        .unwrap();
        assert_eq!(
            stopped.text,
            format!("{PRODUCT_NAME} 0.2.0 didn’t finish downloading")
        );
        assert_eq!(
            stopped.detail.unwrap(),
            "The connection dropped at 31 of 52 MB. It’s tried again at the next check, or now with Check for updates."
        );

        let refused = view(
            &UpdateState::SignatureRefused {
                version: "0.2.0".into(),
            },
            &ctx(),
        )
        .row
        .result
        .unwrap();
        assert_eq!(
            refused.text,
            format!(
                "{PRODUCT_NAME} 0.2.0 was refused: its signature didn’t match {PRODUCT_NAME}’s"
            )
        );
        assert_eq!(refused.tone, Tone::Error);
        assert_eq!(
            refused.detail.unwrap(),
            "The file wasn’t used and you’re still on 0.1.0. Check for updates downloads it again."
        );
        assert!(text(&kept("0.2.0", Some(1))).starts_with(&format!(
            "{PRODUCT_NAME} 0.2.0 downloaded and checked · at "
        )));
    }

    #[test]
    fn built_from_source_never_checks_and_says_so() {
        let v = view(&UpdateState::BuiltFromSource, &ctx());
        assert_eq!(v.auto, None);
        assert_eq!(v.row.check, None);
        assert_eq!(
            v.row.plain.unwrap(),
            "Built from source · bed9c10. This build never checks for updates; a release never replaces it. Build it again to update."
        );
        assert_eq!((v.notice, v.dot), (None, None));
    }

    #[test]
    fn a_compatible_update_restarts_only_the_app() {
        let v = view(&kept("0.2.0", Some(1)), &ctx());
        let n = v.notice.unwrap();
        assert_eq!(n.kind, NoticeKind::Ready);
        assert_eq!(n.title, format!("{PRODUCT_NAME} 0.2.0 is ready"));
        assert_eq!(
            n.body,
            format!(
                "Downloaded and checked. Restart now reopens the window on 0.2.0 in a few seconds; agents keep working. Or it’s put in place the next time you open {PRODUCT_NAME}."
            )
        );
        assert_eq!(
            (n.action_label.as_str(), n.primary, n.enabled),
            ("Restart now", true, true)
        );
        assert_eq!(
            (n.whats_new_label.as_str(), n.skip_label.as_str()),
            ("What’s new", "Skip this version")
        );
        assert_eq!(v.dot.unwrap(), format!("{PRODUCT_NAME} 0.2.0 is ready"));
    }

    #[test]
    fn an_incompatible_update_says_it_stops_the_agents() {
        let n = view(&kept("0.3.0", Some(2)), &ctx()).notice.unwrap();
        assert_eq!(n.kind, NoticeKind::NeedsAgentsRestarted);
        assert_eq!(
            n.title,
            format!("{PRODUCT_NAME} 0.3.0 is ready · needs agents restarted (3 working)")
        );
        assert_eq!(
            n.body,
            "This version can’t work with the background service that runs your agents. Restart now restarts both: the 3 working agents stop and show Interrupted, and Resume all continues them where they were."
        );
        assert_eq!(n.action_label, "Restart now · stops 3 agents");
        assert!(!n.primary);
        // No protocol number in latest.json counts as incompatible.
        assert_eq!(
            view(&kept("0.3.0", None), &ctx()).notice.unwrap().kind,
            NoticeKind::NeedsAgentsRestarted
        );
        assert!(compatible(Some(4), 4) && !compatible(Some(4), 5) && !compatible(None, 1));
    }

    #[test]
    fn skip_this_version_hides_it_until_a_newer_one() {
        let skipped = Context {
            skipped: Some("0.2.0".into()),
            ..ctx()
        };
        let v = view(&kept("0.2.0", Some(1)), &skipped);
        assert_eq!((v.notice, v.dot), (None, None));
        let v = view(&kept("0.2.1", Some(1)), &skipped);
        assert!(v.notice.is_some() && v.dot.is_some());
    }

    #[test]
    fn the_deb_offers_the_download_page() {
        assert!(updates_by_hand("linux", None));
        assert!(updates_by_hand("linux", Some("")));
        assert!(!updates_by_hand(
            "linux",
            Some("/home/mia/Apps/board.AppImage")
        ));
        assert!(!updates_by_hand("macos", None) && !updates_by_hand("windows", None));
        let v = view(
            &UpdateState::Available {
                version: "0.2.0".into(),
                notes_url: release_page("0.2.0"),
                checked_at: NOW,
            },
            &ctx(),
        );
        let n = v.notice.unwrap();
        assert_eq!(n.kind, NoticeKind::Available);
        assert_eq!(n.title, format!("{PRODUCT_NAME} 0.2.0 is available"));
        assert_eq!(n.action, NoticeAction::Download);
        assert_eq!(
            (n.action_label.as_str(), n.enabled, n.primary),
            ("Download", true, true)
        );
        assert_eq!(v.dot.unwrap(), format!("{PRODUCT_NAME} 0.2.0 is available"));
    }

    #[test]
    fn once_a_day() {
        assert!(check_due(true, None, NOW));
        assert!(!check_due(true, Some(NOW - 23 * 3600 * 1000), NOW));
        assert!(check_due(true, Some(NOW - CHECK_EVERY_MS), NOW));
        assert!(!check_due(false, None, NOW));
        // A clock set back doesn't stop checks for good.
        assert!(check_due(true, Some(NOW + 3_600_000), NOW));
        assert_eq!(due_in_ms(Some(NOW - 23 * 3600 * 1000), NOW), 3600 * 1000);
        assert_eq!(due_in_ms(None, NOW), 0);
    }

    #[test]
    fn versions_compare_by_number() {
        assert!(newer("0.10.0", "0.9.1"));
        assert!(newer("0.2.0", "0.1.9"));
        assert!(newer("v1.0", "0.99.0"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("0.1.0", "0.2.0"));
        assert!(!newer("0.2.0-beta.1", "0.2.0"));
    }

    #[test]
    fn the_restart_decision_table() {
        use Step::*;
        use Trigger::*;
        let compatible = Some(Some(1));
        let incompatible = Some(Some(2));
        for trigger in [About, Menu, NextOpen] {
            assert_eq!(
                restart_step(trigger, None, Some(1)),
                Wait,
                "{trigger:?}: nothing kept"
            );
            assert_eq!(
                restart_step(trigger, compatible, Some(1)),
                AppOnly,
                "{trigger:?}"
            );
            // No service running: nothing to stop, the new app starts its own.
            assert_eq!(
                restart_step(trigger, incompatible, None),
                AppOnly,
                "{trigger:?}"
            );
        }
        assert_eq!(restart_step(About, incompatible, Some(1)), WithService);
        assert_eq!(restart_step(Menu, incompatible, Some(1)), WithService);
        assert_eq!(restart_step(NextOpen, incompatible, Some(1)), Wait);
        // latest.json without a protocol number counts as incompatible.
        assert_eq!(restart_step(Menu, Some(None), Some(1)), WithService);
        assert_eq!(restart_step(NextOpen, Some(None), Some(1)), Wait);
    }

    #[test]
    fn restart_to_update_in_the_menu() {
        assert_eq!(
            restart_item(&kept("0.2.0", Some(1)), &ctx()).unwrap(),
            format!("Restart to update · {PRODUCT_NAME} 0.2.0")
        );
        assert_eq!(
            restart_item(&kept("0.3.0", Some(2)), &ctx()).unwrap(),
            "Restart to update · stops 3 agents"
        );
        let skipped = Context {
            skipped: Some("0.2.0".into()),
            ..ctx()
        };
        assert_eq!(restart_item(&kept("0.2.0", Some(1)), &skipped), None);
        assert_eq!(restart_item(&UpdateState::Checking, &ctx()), None);
        assert_eq!(
            restart_item(
                &UpdateState::Available {
                    version: "0.2.0".into(),
                    notes_url: release_page("0.2.0"),
                    checked_at: NOW
                },
                &ctx()
            ),
            None
        );
    }

    #[test]
    fn the_toast_after_an_update_shows_once() {
        let (title, text) = updated_toast(Some("0.1.0"), "0.2.0").unwrap();
        assert_eq!(title, format!("Updated to {PRODUCT_NAME} 0.2.0"));
        assert_eq!(
            text,
            "The window reopened on the new version; your agents kept working."
        );
        assert_eq!(updated_toast(Some("0.2.0"), "0.2.0"), None);
        assert_eq!(updated_toast(None, "0.2.0"), None);
        assert_eq!(updated_toast(Some("0.3.0"), "0.2.0"), None);
    }

    #[test]
    fn why_it_couldnt_be_put_in_place() {
        let v = view(
            &UpdateState::NotReplaced {
                version: "0.2.0".into(),
                why: not_replaced_why("/Applications/Board.app", true, "", "0.1.0", "macos"),
            },
            &ctx(),
        );
        let r = v.row.result.unwrap();
        assert_eq!(
            r.text,
            format!("{PRODUCT_NAME} 0.2.0 couldn’t be put in place")
        );
        assert_eq!(r.tone, Tone::Error);
        assert_eq!(
            r.detail.unwrap(),
            format!(
                "/Applications/Board.app belongs to another account on this Mac, so {PRODUCT_NAME} can’t replace it. You’re still on 0.1.0. Sign in as that account and update there, or move {PRODUCT_NAME} to ~/Applications."
            )
        );
        assert_eq!(
            not_replaced_why("/opt/Board", false, "disk full", "0.1.0", "linux"),
            format!(
                "{PRODUCT_NAME} couldn’t replace /opt/Board: disk full. You’re still on 0.1.0."
            )
        );
    }
}
