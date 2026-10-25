//! Ticket priorities (TKT-09, d0065): **1 Urgent · 2 High · 3 Normal · 4 Low**, default 3.
//!
//! The number is the stored and the wire form (`"priority": 2`, `otaq create -p 2`), so a priority reads
//! the same in SQL, JSON and on the command line. Every ticket has one (d0133).

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A ticket's priority. Ordered by urgency: `Urgent < High < Normal < Low`, as the start order sorts.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Priority {
    Urgent = 1,
    High = 2,
    #[default]
    Normal = 3,
    Low = 4,
}

/// The refusal for a number outside 1–4 (the CLI's `-p 0` / `-p 5`).
pub const PRIORITY_RANGE: &str = "Priority is 1 (Urgent) to 4 (Low)";

impl Priority {
    /// Every value, most urgent first.
    pub const ALL: [Self; 4] = [Self::Urgent, Self::High, Self::Normal, Self::Low];

    /// 1 to 4.
    pub const fn number(self) -> u8 {
        self as u8
    }

    pub fn from_number(n: i64) -> Option<Self> {
        Self::ALL.into_iter().find(|p| i64::from(p.number()) == n)
    }

    /// "Urgent", "High", "Normal", "Low".
    pub const fn label(self) -> &'static str {
        match self {
            Self::Urgent => "Urgent",
            Self::High => "High",
            Self::Normal => "Normal",
            Self::Low => "Low",
        }
    }
}

/// "P2 · High".
impl fmt::Display for Priority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "P{} · {}", self.number(), self.label())
    }
}

impl std::str::FromStr for Priority {
    type Err = String;

    /// "2" or "P2".
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        let digits = s.strip_prefix(['P', 'p']).unwrap_or(s);
        digits
            .parse::<i64>()
            .ok()
            .and_then(Self::from_number)
            .ok_or_else(|| PRIORITY_RANGE.to_owned())
    }
}

impl Serialize for Priority {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_u8(self.number())
    }
}

impl<'de> Deserialize<'de> for Priority {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let n = i64::deserialize(d)?;
        Self::from_number(n).ok_or_else(|| serde::de::Error::custom(PRIORITY_RANGE))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_priorities_default_normal() {
        assert_eq!(Priority::default(), Priority::Normal);
        let labels: Vec<_> = Priority::ALL.iter().map(|p| p.to_string()).collect();
        assert_eq!(
            labels,
            ["P1 · Urgent", "P2 · High", "P3 · Normal", "P4 · Low"]
        );
        assert!(Priority::Urgent < Priority::Low, "most urgent sorts first");
    }

    #[test]
    fn numbers_on_the_wire_and_the_command_line() {
        assert_eq!(serde_json::to_string(&Priority::High).unwrap(), "2");
        assert_eq!(
            serde_json::from_str::<Priority>("4").unwrap(),
            Priority::Low
        );
        assert!(serde_json::from_str::<Priority>("5").is_err());
        assert_eq!("1".parse::<Priority>(), Ok(Priority::Urgent));
        assert_eq!("P3".parse::<Priority>(), Ok(Priority::Normal));
        for bad in ["0", "5", "high", ""] {
            assert_eq!(bad.parse::<Priority>(), Err(PRIORITY_RANGE.to_owned()));
        }
    }
}
