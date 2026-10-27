//! Raw access to the global `settings` table (key → JSON value). Typed settings with defaults and validation
//! come in T-0028; until then only the onboarding's flags live here.

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

/// Each tool's fallback (AGM-03): `{"claude": {"tool": "codex", "model": "newest:gpt-sol"}}`. Used only on the
/// user's click (Use fallback, Use fallback now).
pub const AGENT_FALLBACK: &str = "agents.fallback";

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

impl Store {
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
