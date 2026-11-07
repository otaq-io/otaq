//! Raw access to the global `settings` table (key → JSON value). Typed settings with defaults and validation
//! come in T-0028; until then only the onboarding's flags live here.

use serde::{Deserialize, Serialize};

use super::{Store, StoreError, now_ms};

/// Key of the remembered dismissal of the first-run permission-mode warning (d0011, d0016).
pub const PERMISSION_WARNING_DISMISSED: &str = "onboarding.permission_warning_dismissed";

/// Set by a startup pass that recorded Claude Code's trust for a project added earlier: the one-time note shows
/// (PRJ-08, d0085).
pub const TRUST_NOTE_PENDING: &str = "trust.note_pending";

/// The user dismissed the folder trust note: it never comes back (PRJ-08).
pub const TRUST_NOTE_DISMISSED: &str = "trust.note_dismissed";

/// The default model for new agents (AGM-03, d0108): `{"tool": "claude", "model": "sonnet"}`; `model: default`
/// resolves to it at session start. Absent until a tool connects.
pub const DEFAULT_MODEL: &str = "agents.default_model";

/// The agent Noyan uses (an app setting, "Noyan uses <agent>"): the agent's id as a JSON string. Set at the first
/// start to the shipped Noyan agent; a new conversation may pick another for itself.
pub const NOYAN_AGENT: &str = "agents.noyan";

/// The agent tools the user set Hidden on Settings → Agent tools (d0086): a list of stored tool names
/// (`["copilot"]`). A hidden tool doesn't count as connected.
pub const AGENT_TOOLS_HIDDEN: &str = "agent_tools.hidden";

/// The agent tools that counted as connected at the last check (d0086): a list of stored tool names. A tool that
/// counts and isn't listed has just become Connected: it is set up, gets its entry and its folder trust, once.
pub const AGENT_TOOLS_CONNECTED: &str = "agent_tools.connected";

/// The tools the pending folder trust note names (`["codex"]`), cleared with the note (PRJ-08, d0085, d0086).
pub const TRUST_NOTE_TOOLS: &str = "trust.note_tools";

/// The tools whose connecting raised the folder trust note once already: it is raised once per tool (d0086).
pub const TRUST_NOTE_RAISED: &str = "trust.note_raised";

/// Otaq's own "Allow notifications?" line is up (ATT-13, d0090): `true`, set when the first agent session starts while
/// macOS hasn't been asked; removed once answered.
pub const NOTIFICATIONS_ASK_PENDING: &str = "notifications.ask_pending";

/// The user answered the ask (Allow, Not now, or Settings' Allow notifications): `true`. It is never set pending again.
pub const NOTIFICATIONS_ASK_ANSWERED: &str = "notifications.ask_answered";

/// The user closed the "macOS is blocking Otaq's notifications" banner (ATT-14): the permission it was closed for
/// (`"turned_off"`). Removed when the permission becomes allowed, so a later turn-off can show it once more.
pub const NOTIFICATIONS_BLOCKED_DISMISSED: &str = "notifications.blocked_dismissed";

/// The first-hide note (TRY-02, d0139) was seen: posted as an OS notification, or shown in the window: `true`. Once
/// per data folder, never again.
pub const FIRST_HIDE_NOTE_SHOWN: &str = "app.first_hide_note_shown";

/// The first-hide note the OS couldn't show, waiting for the window (d0139 audit AS q4): `{"title": …, "body": …}`.
/// Kept until a window has shown it (`app.first_hide_shown`), also across restarts of the app.
pub const FIRST_HIDE_NOTE_PENDING: &str = "app.first_hide_note_pending";

/// Check for updates automatically (UPD-04, d0140): `false` once switched off; absent means on.
pub const UPDATES_AUTO_CHECK: &str = "updates.auto_check";

/// The last update check (UPD-04): `{"at": <ms>, "reached": true}`, so a restart of the app doesn't check again early.
pub const UPDATES_LAST_CHECK: &str = "updates.last_check";

/// The version Skip this version hides (UPD-04): `"0.2.0"`, until a newer one.
pub const UPDATES_SKIPPED: &str = "updates.skipped";

/// The version the app last started as (UPD-05): `"0.2.0"`, so "Updated to Otaq 0.2.0" shows once after an update.
pub const UPDATES_LAST_VERSION: &str = "updates.last_version";

/// The one-time hints the user dismissed, one key per hint (`hints.seen.gallery_sample` → `true`): a hint shows at its
/// moment until dismissed, then never again for this data folder.
pub const HINT_SEEN_PREFIX: &str = "hints.seen.";

/// A one-time hint: one sentence at the moment a concept first matters, with one button that dismisses it for good.
/// Never behind a setting. The set is closed and named: a part of the app that wants a hint adds its own here (and to
/// [`Hint::ALL`]), with its wording in the view model that shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hint {
    /// The design gallery's sample hint: the one hint with no moment in the app, so the popover can be seen and
    /// tried.
    GallerySample,
    /// The first time Edit flow turns the board into the editor: the flow is its columns, and nothing changes until
    /// the user saves.
    FlowColumns,
}

impl Hint {
    /// Every hint.
    pub const ALL: &[Hint] = &[Hint::GallerySample, Hint::FlowColumns];

    /// The hint's stored and wire name (`"gallery_sample"`).
    pub fn id(self) -> &'static str {
        match self {
            Hint::GallerySample => "gallery_sample",
            Hint::FlowColumns => "flow_columns",
        }
    }

    fn seen_key(self) -> String {
        format!("{HINT_SEEN_PREFIX}{}", self.id())
    }
}

impl Store {
    /// The hints the user dismissed, in [`Hint::ALL`]'s order. A stored name no hint has any more is left out.
    pub async fn hints_seen(&self) -> Result<Vec<Hint>, StoreError> {
        let keys: Vec<String> =
            sqlx::query_scalar("SELECT key FROM settings WHERE substr(key, 1, ?) = ?")
                .bind(i64::try_from(HINT_SEEN_PREFIX.len()).unwrap_or(i64::MAX))
                .bind(HINT_SEEN_PREFIX)
                .fetch_all(self.pool())
                .await?;
        Ok(Hint::ALL
            .iter()
            .copied()
            .filter(|hint| keys.contains(&hint.seen_key()))
            .collect())
    }

    /// Remembers that the user dismissed `hint`: it never shows again. Dismissing it again changes nothing.
    pub async fn see_hint(&self, hint: Hint) -> Result<(), StoreError> {
        self.set_setting(&hint.seen_key(), "true").await
    }

    /// The stored JSON value of a global setting.
    pub async fn setting(&self, key: &str) -> Result<Option<String>, StoreError> {
        Ok(
            sqlx::query_scalar("SELECT value_json FROM settings WHERE key = ?")
                .bind(key)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    /// Stores a global setting's JSON value, replacing an earlier one.
    pub async fn set_setting(&self, key: &str, value_json: &str) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO settings (key, value_json, updated_at) VALUES (?, ?, ?)
             ON CONFLICT (key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value_json)
        .bind(now_ms())
        .execute(self.pool())
        .await?;
        Ok(())
    }

    /// Removes a global setting; nothing happens when it isn't stored.
    pub async fn remove_setting(&self, key: &str) -> Result<(), StoreError> {
        sqlx::query("DELETE FROM settings WHERE key = ?")
            .bind(key)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub async fn project_count(&self) -> Result<i64, StoreError> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM projects")
            .fetch_one(self.pool())
            .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hints_id_is_its_wire_name() {
        for hint in Hint::ALL {
            assert_eq!(
                serde_json::to_value(hint).unwrap(),
                serde_json::Value::from(hint.id())
            );
        }
    }

    #[tokio::test]
    async fn a_dismissed_hint_stays_seen_and_others_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hints.db");
        let store = Store::open(&path).await.unwrap();
        assert_eq!(store.hints_seen().await.unwrap(), vec![]);

        // A name no hint has any more (a hint that was removed) is left out.
        store
            .set_setting("hints.seen.long_gone", "true")
            .await
            .unwrap();
        assert_eq!(store.hints_seen().await.unwrap(), vec![]);

        store.see_hint(Hint::GallerySample).await.unwrap();
        store.see_hint(Hint::GallerySample).await.unwrap();
        assert_eq!(store.hints_seen().await.unwrap(), vec![Hint::GallerySample]);
        drop(store);

        // The same database opened again (the app started again).
        let store = Store::open(&path).await.unwrap();
        assert_eq!(store.hints_seen().await.unwrap(), vec![Hint::GallerySample]);
    }
}
