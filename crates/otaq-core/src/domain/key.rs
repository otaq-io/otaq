//! Ticket keys (d0029): `<KEY>-<number>` such as `SHOP-18`. Numbers start at 1. There are no child keys (d0133):
//! `SHOP-40.1` is not a key.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A project key: uppercase ASCII letters and digits, starting with a letter (`SHOP`, `BILL`, `CREW`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProjectKey(String);

/// The longest project key accepted. Keys appear on every card, so they stay short.
pub const MAX_PROJECT_KEY_LEN: usize = 10;

impl ProjectKey {
    pub fn new(key: &str) -> Result<Self, KeyError> {
        let mut chars = key.chars();
        let valid = chars.next().is_some_and(|c| c.is_ascii_uppercase())
            && chars.all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
            && key.len() <= MAX_PROJECT_KEY_LEN;
        if valid {
            Ok(Self(key.to_owned()))
        } else {
            Err(KeyError::BadProjectKey(key.to_owned()))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A ticket key: `SHOP-18` is `{SHOP, 18}`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TicketKey {
    pub project: ProjectKey,
    pub number: u32,
}

impl TicketKey {
    pub fn new(project: ProjectKey, number: u32) -> Self {
        Self { project, number }
    }

    /// Resolves what a person or agent typed: a full key (`SHOP-18`), or a bare number (`18`) in the current
    /// project.
    pub fn resolve(input: &str, current: Option<&ProjectKey>) -> Result<Self, KeyError> {
        let input = input.trim();
        if input.starts_with(|c: char| c.is_ascii_digit()) {
            let project = current.ok_or_else(|| KeyError::NoCurrentProject(input.to_owned()))?;
            return Ok(Self::new(project.clone(), parse_number(input)?));
        }
        input.parse()
    }
}

/// Parses `18`: a positive number with no leading zero.
fn parse_number(s: &str) -> Result<u32, KeyError> {
    let bad = || KeyError::BadTicketKey(s.to_owned());
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || s.starts_with('0') {
        return Err(bad());
    }
    s.parse().map_err(|_| bad())
}

impl FromStr for TicketKey {
    type Err = KeyError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || KeyError::BadTicketKey(s.to_owned());
        let (project, numbers) = s.split_once('-').ok_or_else(bad)?;
        let project = ProjectKey::new(project).map_err(|_| bad())?;
        let number = parse_number(numbers).map_err(|_| bad())?;
        Ok(Self::new(project, number))
    }
}

impl fmt::Display for TicketKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}-{}", self.project, self.number)
    }
}

impl Serialize for TicketKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TicketKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl Serialize for ProjectKey {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for ProjectKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = <std::borrow::Cow<'de, str>>::deserialize(d)?;
        ProjectKey::new(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    BadProjectKey(String),
    BadTicketKey(String),
    /// A bare number was given with no current project to put it in.
    NoCurrentProject(String),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadProjectKey(k) => write!(
                f,
                "{k:?} isn't a valid project key: use up to {MAX_PROJECT_KEY_LEN} uppercase letters and digits, starting with a letter"
            ),
            Self::BadTicketKey(k) => write!(
                f,
                "{k:?} isn't a ticket key (expected something like SHOP-18)"
            ),
            Self::NoCurrentProject(k) => {
                write!(
                    f,
                    "{k:?} needs a project: write the full key, like SHOP-{k}"
                )
            }
        }
    }
}

impl std::error::Error for KeyError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn shop() -> ProjectKey {
        ProjectKey::new("SHOP").unwrap()
    }

    #[test]
    fn parses_and_formats() {
        let key: TicketKey = "SHOP-18".parse().unwrap();
        assert_eq!(key, TicketKey::new(shop(), 18));
        assert_eq!(key.to_string(), "SHOP-18");

        assert_eq!(
            "CREW2-7".parse::<TicketKey>().unwrap().to_string(),
            "CREW2-7"
        );
    }

    #[test]
    fn rejects_bad_keys() {
        for bad in [
            "SHOP-40.1",
            "SHOP-40.1.1",
            "shop-18",
            "SHOP-",
            "SHOP-0",
            "SHOP-40.0",
            "SHOP-018",
            "SHOP-18.",
            "SHOP-.1",
            "-18",
            "SHOP18",
            "2SHOP-1",
            "SHOP-1a",
            "SHOP--1",
            "SH OP-1",
            "SHOP-99999999999",
        ] {
            assert!(
                bad.parse::<TicketKey>().is_err(),
                "{bad} should be rejected"
            );
        }
    }

    #[test]
    fn project_keys() {
        for good in ["SHOP", "BILL", "CREW", "A", "X1"] {
            assert!(ProjectKey::new(good).is_ok(), "{good}");
        }
        for bad in ["", "shop", "1SHOP", "SH-OP", "SHÖP", "ABCDEFGHIJK"] {
            assert!(ProjectKey::new(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn bare_numbers_resolve_in_the_current_project() {
        let current = shop();
        assert_eq!(
            TicketKey::resolve("18", Some(&current)).unwrap(),
            TicketKey::new(shop(), 18)
        );
        assert_eq!(
            TicketKey::resolve("BILL-4", Some(&current))
                .unwrap()
                .to_string(),
            "BILL-4"
        );
        assert!(matches!(
            TicketKey::resolve("18", None),
            Err(KeyError::NoCurrentProject(_))
        ));
        for bad in ["0", "40.1", "18x"] {
            assert!(TicketKey::resolve(bad, Some(&current)).is_err(), "{bad}");
        }
    }

    #[test]
    fn serde_uses_the_display_form() {
        let key = TicketKey::new(shop(), 40);
        let json = serde_json::to_string(&key).unwrap();
        assert_eq!(json, "\"SHOP-40\"");
        assert_eq!(serde_json::from_str::<TicketKey>(&json).unwrap(), key);
        assert!(serde_json::from_str::<TicketKey>("\"shop-1\"").is_err());
    }
}
