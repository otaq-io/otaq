//! Refresh's three-way merge (the otaq-marketplace README, "Ticket sources"): for each field, what was imported (the
//! snapshot), what the ticket says now, and what the tracker says now.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome", content = "value")]
pub enum Merge {
    /// Nothing to change.
    Same,
    /// Take the tracker's text: the ticket wasn't edited.
    TakeRemote(String),
    /// Keep the ticket's text: only it changed.
    KeepLocal,
    /// Both changed, differently: the user picks.
    Conflict { local: String, remote: String },
}

pub fn merge(snapshot: &str, local: &str, remote: &str) -> Merge {
    let norm = |s: &str| s.trim().replace("\r\n", "\n");
    let (snapshot, local, remote) = (norm(snapshot), norm(local), norm(remote));
    if local == remote {
        Merge::Same
    } else if local == snapshot {
        Merge::TakeRemote(remote)
    } else if remote == snapshot {
        Merge::KeepLocal
    } else {
        Merge::Conflict { local, remote }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_merge_table() {
        assert_eq!(merge("a", "a", "a"), Merge::Same);
        assert_eq!(merge("a", "a", "b"), Merge::TakeRemote("b".into()));
        assert_eq!(merge("a", "b", "a"), Merge::KeepLocal);
        assert_eq!(merge("a", "b", "b"), Merge::Same, "both made the same edit");
        assert_eq!(
            merge("a", "b", "c"),
            Merge::Conflict {
                local: "b".into(),
                remote: "c".into()
            }
        );
        assert_eq!(
            merge("a\r\n", "a\n", "a"),
            Merge::Same,
            "line endings and edges don't count"
        );
    }
}
