//! The app's update preferences (UPD-04, d0140), kept by the service so a restart of the app doesn't check again
//! early: **Check for updates automatically** (on by default), when the last check ran and whether it reached the
//! release, and the version **Skip this version** hides. The check itself runs in the app.

use serde::{Deserialize, Serialize};

use crate::store::settings::{UPDATES_AUTO_CHECK, UPDATES_LAST_CHECK, UPDATES_SKIPPED};
use crate::store::{Store, StoreError};

/// What `updates.prefs` answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePrefs {
    /// Check for updates automatically (on by default).
    pub auto_check: bool,
    /// The last check, automatic or by Check for updates; `None` before the first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<LastCheck>,
    /// The version Skip this version hides (`"0.2.0"`), until a newer one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

impl Default for UpdatePrefs {
    fn default() -> Self {
        Self {
            auto_check: true,
            last_check: None,
            skipped: None,
        }
    }
}

/// One check: when it ran (Unix milliseconds) and whether it got an answer from the release.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastCheck {
    pub at: i64,
    pub reached: bool,
}

/// `updates.set_prefs`: each field given replaces the stored one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdatePrefsChange {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_check: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_check: Option<LastCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

/// The stored preferences; an unreadable value counts as not set.
pub async fn prefs(store: &Store) -> Result<UpdatePrefs, StoreError> {
    let read = async |key| -> Result<Option<serde_json::Value>, StoreError> {
        Ok(store
            .setting(key)
            .await?
            .and_then(|json| serde_json::from_str(&json).ok()))
    };
    let defaults = UpdatePrefs::default();
    Ok(UpdatePrefs {
        auto_check: read(UPDATES_AUTO_CHECK)
            .await?
            .and_then(|v| v.as_bool())
            .unwrap_or(defaults.auto_check),
        last_check: read(UPDATES_LAST_CHECK)
            .await?
            .and_then(|v| serde_json::from_value(v).ok()),
        skipped: read(UPDATES_SKIPPED)
            .await?
            .and_then(|v| v.as_str().map(str::to_owned)),
    })
}

/// Stores the fields given and answers the preferences now.
pub async fn change(store: &Store, change: &UpdatePrefsChange) -> Result<UpdatePrefs, StoreError> {
    let json = |value: serde_json::Value| value.to_string();
    if let Some(on) = change.auto_check {
        store
            .set_setting(UPDATES_AUTO_CHECK, &json(on.into()))
            .await?;
    }
    if let Some(check) = &change.last_check {
        let value = serde_json::to_value(check).expect("a check serializes");
        store.set_setting(UPDATES_LAST_CHECK, &json(value)).await?;
    }
    if let Some(version) = &change.skipped {
        store
            .set_setting(UPDATES_SKIPPED, &json(version.as_str().into()))
            .await?;
    }
    prefs(store).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn on_by_default_then_each_field_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("prefs.db")).await.unwrap();
        assert_eq!(prefs(&store).await.unwrap(), UpdatePrefs::default());

        let check = LastCheck {
            at: 1_700_000_000_000,
            reached: true,
        };
        let after = change(
            &store,
            &UpdatePrefsChange {
                last_check: Some(check),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(after.last_check, Some(check));
        assert!(after.auto_check);

        let after = change(
            &store,
            &UpdatePrefsChange {
                auto_check: Some(false),
                skipped: Some("0.2.0".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            after,
            UpdatePrefs {
                auto_check: false,
                last_check: Some(check),
                skipped: Some("0.2.0".into()),
            }
        );
    }
}
