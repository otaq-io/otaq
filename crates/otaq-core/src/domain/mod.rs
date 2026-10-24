//! The nouns every other module talks about, as plain types with no I/O: ticket keys, kinds, places, statuses
//! and flags (SDD §6.2, §7.1, §7.2).
//!
//! Each enum has one stored string form (`as_str` / `FromStr`), used in the database and on the wire, so a
//! value always reads the same in SQL, JSON and logs.

pub mod actor;
pub mod flags;
pub mod hand_off;
pub mod key;
pub mod priority;
pub mod status;
pub mod ticket_env;
pub mod validation;

pub use actor::Actor;
pub use flags::{
    AskedLead, ByLead, Flag, Flags, LeadNoteSent, MergeCheckState, SameFile, SentBack, StuckStop,
    UnreadNotes,
};
pub use hand_off::HandOffKind;
pub use key::{KeyError, ProjectKey, TicketKey};
pub use priority::{PRIORITY_RANGE, Priority};
pub use status::{Kind, Place, StartStep, Status, StatusSet};
pub use ticket_env::{BuildState, RemoteWarning, ResourceState, SchemaOverlap, WentOn};
pub use validation::{
    AutoMergeStop, MovedPast, ReviewEntry, ReviewEntryFlag, ValidationAction, ValidationOutcome,
    ValidationRound, ValidationTrigger,
};

use std::fmt;

/// A stored string that names no known value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownValue {
    pub what: &'static str,
    pub value: String,
}

impl fmt::Display for UnknownValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown {}: {:?}", self.what, self.value)
    }
}

impl std::error::Error for UnknownValue {}

/// Declares a fieldless enum with a stored string per variant, `ALL`, `as_str`, `Display`, `FromStr` and serde
/// in the same string form. A stored string is a literal or a constant (such as a `naming` one).
macro_rules! stored_enum {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident ($what:literal) {
            $( $(#[$vmeta:meta])* $variant:ident = $stored:expr, )+
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        $vis enum $name {
            $( $(#[$vmeta])* $variant, )+
        }

        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];

            /// The stored string form.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $stored,)+
                }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::domain::UnknownValue;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $(_ if s == $stored => Ok(Self::$variant),)+
                    _ => Err($crate::domain::UnknownValue { what: $what, value: s.to_owned() }),
                }
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
                s.parse().map_err(serde::de::Error::custom)
            }
        }
    };
}

pub(crate) use stored_enum;
