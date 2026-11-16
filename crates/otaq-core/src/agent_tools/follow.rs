//! Follow choices (d0086 "always the newest", d0088): the stored forms of a named agent's model that move to a newer
//! model by themselves, in one place for every adapter and view. Claude Code's aliases (`opus`, `sonnet`…) stay M22's
//! plain text, from its own list; the two forms here are Otaq's:
//! - **newest in a family** (Codex, OpenCode): `newest:<family>` (`newest:gpt-sol`), resolved at session start to the
//!   newest model of that family that is also in the tool's own list (T-0356);
//! - **Copilot's Auto** with a tier: `auto@balance`, `auto@intelligence` (its adapter turns it into its flags,
//!   T-0352); `auto@efficiency` still reads, but Copilot 1.0.112 removed it and it is no longer offered (decision 117).
//!
//! The model picker offers them as entries of the tool's list, so M22's model kinds ([`crate::agents::ToolModel`]) reads them
//! as `Follow`.

use crate::domain::stored_enum;

stored_enum! {
    /// Copilot's Auto setting (d0086): which way Auto leans when it picks a model per task.
    pub enum CopilotTier ("Copilot Auto tier") {
        Efficiency = "efficiency",
        /// Copilot's default tier (plain `auto`).
        Balance = "balance",
        Intelligence = "intelligence",
    }
}

impl CopilotTier {
    /// The tier as the picker shows it: "Auto · efficiency".
    pub fn label(self) -> String {
        format!("Auto \u{b7} {self}")
    }
}

/// A follow choice in one of Otaq's stored forms.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FollowChoice {
    /// The newest model of a models.dev family (`gpt-sol`) that the tool lists.
    Newest { family: String },
    /// Copilot's Auto with a tier.
    Auto { tier: CopilotTier },
}

impl FollowChoice {
    /// The stored prefix of a family choice.
    pub const NEWEST_PREFIX: &'static str = "newest:";
    /// The stored prefix of an Auto choice.
    pub const AUTO_PREFIX: &'static str = "auto@";

    /// Reads a stored model text; `None` for anything else (an exact id, a Claude Code alias, empty).
    pub fn parse(stored: &str) -> Option<Self> {
        let stored = stored.trim();
        if let Some(family) = stored.strip_prefix(Self::NEWEST_PREFIX) {
            let family = family.trim();
            return (!family.is_empty()).then(|| Self::Newest {
                family: family.to_owned(),
            });
        }
        let tier = stored.strip_prefix(Self::AUTO_PREFIX)?.parse().ok()?;
        Some(Self::Auto { tier })
    }

    /// The stored text: `newest:gpt-sol`, `auto@balance`.
    pub fn stored(&self) -> String {
        match self {
            Self::Newest { family } => format!("{}{family}", Self::NEWEST_PREFIX),
            Self::Auto { tier } => format!("{}{tier}", Self::AUTO_PREFIX),
        }
    }
}

impl std::fmt::Display for FollowChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.stored())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stored_forms_round_trip() {
        for stored in [
            "newest:gpt-sol",
            "auto@efficiency",
            "auto@balance",
            "auto@intelligence",
        ] {
            let choice = FollowChoice::parse(stored).unwrap();
            assert_eq!(choice.stored(), stored);
        }
        assert_eq!(
            FollowChoice::parse(" newest:claude-opus "),
            Some(FollowChoice::Newest {
                family: "claude-opus".into()
            })
        );
        assert_eq!(
            FollowChoice::parse("auto@intelligence"),
            Some(FollowChoice::Auto {
                tier: CopilotTier::Intelligence
            })
        );
        assert_eq!(CopilotTier::Balance.label(), "Auto \u{b7} balance");
    }

    #[test]
    fn other_texts_are_not_follow_choices() {
        for text in [
            "",
            "opus",
            "auto",
            "auto@",
            "auto@fast",
            "newest:",
            "gpt-5.5",
            "claude-opus-5-5",
        ] {
            assert_eq!(FollowChoice::parse(text), None, "{text}");
        }
    }
}
