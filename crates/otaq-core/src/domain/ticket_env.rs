//! The ticket environment's stored states (M19; d0085, d0086, d0087): where a ticket's build output is, and what
//! became of the ticket resource Otaq created for it.

use serde::{Deserialize, Serialize};

use super::{TicketKey, stored_enum};

stored_enum! {
    /// Where the ticket's ignored build output (dependencies, build caches) is (d0085, d0086).
    #[derive(Default)]
    pub enum BuildState ("build state") {
        /// In the ticket's worktree.
        #[default]
        Present = "present",
        /// Moved into the warm stock at Review entry; the worktree holds the tracked files only.
        Freed = "freed",
        /// Being brought back for Send back, Fix, Run or Shell.
        Fetching = "fetching",
    }
}

impl BuildState {
    pub fn is_present(&self) -> bool {
        *self == Self::Present
    }
}

stored_enum! {
    /// The ticket resource the project's Create command made for the ticket (d0087).
    #[derive(Default)]
    pub enum ResourceState ("resource state") {
        /// Nothing created (no Create command, or not started yet).
        #[default]
        None = "none",
        Created = "created",
        /// The Create command failed; `resource_problem` says why.
        Failed = "failed",
        /// The Remove command ran at Done or Close.
        Removed = "removed",
    }
}

impl ResourceState {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }
}

/// Another Review ticket that adds files under the same schema path (M19-e): which ticket, which path glob, and
/// the files the other ticket adds under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaOverlap {
    pub other: TicketKey,
    /// The project's schema-path glob both tickets add files under.
    pub path: String,
    /// The files `other` adds under `path`, sorted.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<String>,
}

/// The fetch before a start or a Local merge failed and the ticket went on from its local base branch (d0121). Kept
/// on the ticket until a later fetch for it works.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteWarning {
    pub went_on: WentOn,
    /// The base branch: `main`.
    pub branch: String,
    /// The remote that couldn't be reached: `origin`.
    pub remote: String,
    /// Git's reason: "git fetch --quiet origin failed (exit 128): Host key verification failed."
    pub reason: String,
}

/// What went on without the remote (d0121).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WentOn {
    /// The ticket started from the local base branch.
    Started,
    /// The ticket merged into the local base branch.
    Merged,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn round_trip<T>(all: &[T])
    where
        T: Copy + PartialEq + std::fmt::Debug + std::str::FromStr + Serialize,
        T: serde::de::DeserializeOwned,
        <T as std::str::FromStr>::Err: std::fmt::Debug,
    {
        let mut seen = std::collections::HashSet::new();
        for value in all {
            let stored = serde_json::to_value(value).unwrap();
            let text = stored.as_str().unwrap().to_owned();
            assert!(seen.insert(text.clone()), "{text} used twice");
            assert_eq!(text.parse::<T>().unwrap(), *value);
            assert_eq!(serde_json::from_value::<T>(stored).unwrap(), *value);
        }
    }

    #[test]
    fn states_round_trip_as_strings() {
        round_trip(BuildState::ALL);
        round_trip(ResourceState::ALL);
        assert_eq!(BuildState::default(), BuildState::Present);
        assert_eq!(ResourceState::default(), ResourceState::None);
        assert_eq!(
            BuildState::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            ["present", "freed", "fetching"]
        );
        assert_eq!(
            ResourceState::ALL
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>(),
            ["none", "created", "failed", "removed"]
        );
        assert!("gone".parse::<BuildState>().is_err());
        assert!("x".parse::<ResourceState>().is_err());
    }

    #[test]
    fn schema_overlap_reads_as_stored_json() {
        let json = r#"[{"other":"SHOP-18","path":"db/migrations/**"}]"#;
        let list: Vec<SchemaOverlap> = serde_json::from_str(json).unwrap();
        assert_eq!(list[0].other.to_string(), "SHOP-18");
        assert_eq!(serde_json::to_string(&list).unwrap(), json);
    }
}
