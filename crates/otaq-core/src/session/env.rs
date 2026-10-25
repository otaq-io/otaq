//! A project's environment variables in what Otaq starts (ENV-01…03; d0082).
//!
//! A project lists `NAME = value` rows ([`EnvVar`]). The rows with no problem ([`applied`]) are set in everything
//! Otaq starts in the project: every session ([`super::Sessions::spawn`] asks its [`EnvProvider`] at each spawn),
//! the setup command ([`crate::start::run_setup`]) and the git commands Otaq runs in the project's repository and
//! worktrees ([`crate::board::Board::project_git`]), so the user's hooks see them. `gh` and `az` don't get them.
//!
//! **The order rule.** A program gets the service's environment, then the project variables on top, then Otaq's
//! own variables on top of those: **Otaq's own win** over a row of the same name, because Otaq needs them to
//! work (the data folder variable, `OTAQ_TICKET`, the role variables, `PORT`, `TERM`, `COLORTERM`,
//! `COLORFGBG` (the theme's kind, d0117), and for setup on Windows the variable carrying the command). The one exception is `PATH`: a `PATH` row replaces
//! the service's `PATH`, and where Otaq puts its bin folder in front of `PATH` (sessions), it puts it in front
//! of the row's value instead, so `otaq` still resolves. Names compare as the OS does: ignoring case on Windows.
//!
//! **When a change applies.** The rows are read at every spawn, setup and git command, never cached: a change
//! reaches what starts (or resumes) after it. A running session keeps the values it started with; nothing
//! restarts and nothing is offered.
//!
//! **Otaq's own variables inside a value** (d0087). A value can name one of Otaq's own variables as `${NAME}`
//! (for example a database URL ending in the ticket's database name): it is replaced by that variable's value in
//! what is being started, or by nothing where Otaq doesn't set it (git commands set none). Only names with Otaq's
//! prefix are touched; `$NAME`, `${HOME}` and the like reach the program as written ([`substitute`]).

use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use crate::board::settings::{EnvVar, env_var_problems, same_env_name};
use crate::tools::Os;

/// The applied rows of the project with this id, read when asked: set once at service start
/// ([`super::Sessions::set_env_provider`]).
pub type EnvProvider = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Vec<(String, String)>> + Send>> + Send + Sync,
>;

/// Otaq's own variables for the ticket with this id (the ticket resource variables, d0087), read when asked: set
/// once at service start ([`super::Sessions::set_ticket_env_provider`]). Only sessions of a ticket ask.
pub type TicketEnvProvider = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Vec<(String, String)>> + Send>> + Send + Sync,
>;

/// The rows that apply, in row order: every row with no problem (ENV-03). The first of two rows with the same
/// name applies; on Windows names that differ only in case are the same.
pub fn applied(env_vars: &[EnvVar], os: Os) -> Vec<(String, String)> {
    env_vars
        .iter()
        .zip(env_var_problems(env_vars, os))
        .filter(|(_, problem)| problem.is_none())
        .map(|(row, _)| (row.name.clone(), row.value.clone()))
        .collect()
}

/// What to set on top of the inherited environment, by the order rule (module doc): the project's `rows`, then
/// Otaq's `own` variables, which win. When both set `PATH`, Otaq's `PATH` keeps what it put in front of
/// `inherited_path` (the service's `PATH`, which `own`'s extends) and continues with the row's value.
pub fn layered(
    rows: &[(String, String)],
    own: &[(String, String)],
    inherited_path: Option<&OsStr>,
    os: Os,
) -> Vec<(String, String)> {
    let rows = substitute(rows, own, os);
    let is_path = |name: &str| same_env_name(name, "PATH", os);
    let row_path = rows.iter().find(|(name, _)| is_path(name));
    let mut env: Vec<(String, String)> = rows
        .iter()
        .filter(|(name, _)| !own.iter().any(|(o, _)| same_env_name(o, name, os)))
        .cloned()
        .collect();
    for (name, value) in own {
        let value = match row_path {
            Some((_, row_value)) if is_path(name) => {
                path_with_row(value, inherited_path.unwrap_or_default(), row_value)
            }
            _ => value.clone(),
        };
        env.push((name.clone(), value));
    }
    env
}

/// The rows with each `${NAME}` in their values expanded, where NAME starts with Otaq's variable prefix
/// ([`crate::naming::ENV_PREFIX`]; d0087): to the value of `own`'s variable of that name (names compare as the OS
/// does), or to nothing when Otaq doesn't set it in what is being started. Anything else stays as written: `$NAME`
/// without braces, `${OTHER}` with another name, an unclosed `${`. Nothing is expanded twice: a value taken from
/// `own` is put in as it is.
pub fn substitute(
    rows: &[(String, String)],
    own: &[(String, String)],
    os: Os,
) -> Vec<(String, String)> {
    rows.iter()
        .map(|(name, value)| (name.clone(), substitute_value(value, own, os)))
        .collect()
}

fn substitute_value(value: &str, own: &[(String, String)], os: Os) -> String {
    let mut out = String::with_capacity(value.len());
    let mut rest = value;
    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let reference = after
            .find('}')
            .map(|end| &after[..end])
            .filter(|name| is_own_reference(name, os));
        match reference {
            Some(name) => {
                // The last of two with the same name is the one the program sees (`layered` sets them in order).
                if let Some((_, own_value)) =
                    own.iter().rev().find(|(o, _)| same_env_name(o, name, os))
                {
                    out.push_str(own_value);
                }
                rest = &after[name.len() + 1..];
            }
            None => {
                out.push_str("${");
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Whether `name` (between `${` and `}`) names one of Otaq's variables: a variable name (letters, digits and
/// `_`) with Otaq's prefix, ignoring case on Windows.
fn is_own_reference(name: &str, os: Os) -> bool {
    let prefix = crate::naming::ENV_PREFIX;
    name.len() > prefix.len()
        && name.is_char_boundary(prefix.len())
        && same_env_name(&name[..prefix.len()], prefix, os)
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Otaq's `PATH` (`own`, which is what Otaq put in front, then `inherited`) with the row's value in place of
/// `inherited`. When `own` doesn't end with `inherited`, only its first folder counts as Otaq's.
fn path_with_row(own: &str, inherited: &OsStr, row: &str) -> String {
    let own: Vec<PathBuf> = split(OsStr::new(own));
    let inherited: Vec<PathBuf> = split(inherited);
    let front = if own.ends_with(&inherited) {
        &own[..own.len() - inherited.len()]
    } else {
        &own[..own.len().min(1)]
    };
    let parts = front.iter().cloned().chain(split(OsStr::new(row)));
    std::env::join_paths(parts)
        .unwrap_or_else(|_| OsString::from(row))
        .to_string_lossy()
        .into_owned()
}

fn split(path: &OsStr) -> Vec<PathBuf> {
    if path.is_empty() {
        Vec::new()
    } else {
        std::env::split_paths(path).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::settings::{
        ENV_VAR_EMPTY_NAME, env_var_duplicate_message, env_var_equals_message, env_var_null_message,
    };

    fn rows(pairs: &[(&str, &str)]) -> Vec<EnvVar> {
        pairs.iter().map(|(n, v)| EnvVar::new(n, v)).collect()
    }

    fn pairs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(n, v)| ((*n).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn joined(dirs: &[&str]) -> String {
        std::env::join_paths(dirs)
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn each_problem_has_its_wording() {
        let env = rows(&[
            ("", "x"),
            ("MAX=JOBS", "2"),
            ("A\0B", "1"),
            ("CI", "true"),
            ("CI", "false"),
        ]);
        for os in [Os::Mac, Os::Linux, Os::Windows] {
            assert_eq!(
                env_var_problems(&env, os),
                [
                    Some(ENV_VAR_EMPTY_NAME.to_owned()),
                    Some(env_var_equals_message("MAX=JOBS")),
                    Some(env_var_null_message("A\0B")),
                    None,
                    Some(env_var_duplicate_message("CI")),
                ]
            );
        }
        assert_eq!(ENV_VAR_EMPTY_NAME, "Add a name, or remove the row");
        assert_eq!(
            env_var_equals_message("MAX=JOBS"),
            "Remove “=” from the name. This row isn’t set until then."
        );
        assert_eq!(
            env_var_duplicate_message("NODE_OPTIONS"),
            "NODE_OPTIONS is already set above. Only the first row applies; remove one of them."
        );
        assert!(env_var_null_message("A\0B").starts_with("Remove the null character"));
    }

    #[test]
    fn spaces_digits_and_any_value_are_allowed() {
        let env = rows(&[("MY VAR", ""), ("1ST", "a=b"), ("X", " spaced \0? ")]);
        assert_eq!(env_var_problems(&env, Os::current()), [None, None, None]);
        assert_eq!(applied(&env, Os::current()).len(), 3);
    }

    #[test]
    fn names_differing_in_case_clash_on_windows_only() {
        let env = rows(&[("NODE_OPTIONS", "--a"), ("node_options", "--b")]);
        assert_eq!(
            env_var_problems(&env, Os::Windows)[1],
            Some(env_var_duplicate_message("node_options"))
        );
        assert_eq!(
            applied(&env, Os::Windows),
            pairs(&[("NODE_OPTIONS", "--a")])
        );
        for os in [Os::Mac, Os::Linux] {
            assert_eq!(env_var_problems(&env, os), [None, None]);
            assert_eq!(
                applied(&env, os),
                pairs(&[("NODE_OPTIONS", "--a"), ("node_options", "--b")])
            );
        }
    }

    #[test]
    fn applied_keeps_row_order_and_the_first_of_a_name() {
        let env = rows(&[
            ("B", "1"),
            ("", "skipped"),
            ("A", "2"),
            ("B", "3"),
            ("C", "4"),
        ]);
        assert_eq!(
            applied(&env, Os::Linux),
            pairs(&[("B", "1"), ("A", "2"), ("C", "4")])
        );
        assert!(applied(&[], Os::current()).is_empty());
    }

    #[test]
    fn own_variables_win() {
        let rows = pairs(&[("PORT", "1"), ("CI", "true"), ("port", "2")]);
        let own = pairs(&[("PORT", "4019")]);
        assert_eq!(
            layered(&rows, &own, None, Os::Linux),
            pairs(&[("CI", "true"), ("port", "2"), ("PORT", "4019")])
        );
        // On Windows `port` is `PORT`.
        assert_eq!(
            layered(&rows, &own, None, Os::Windows),
            pairs(&[("CI", "true"), ("PORT", "4019")])
        );
    }

    fn own_name(suffix: &str) -> String {
        crate::naming::env_var(suffix)
    }

    #[test]
    fn own_variables_are_substituted_in_rows() {
        let db = own_name("DB_NAME");
        let ticket = own_name("TICKET");
        let rows = vec![
            (
                "DATABASE_URL".to_owned(),
                format!("postgres://localhost/${{{db}}}?app=${{{ticket}}}"),
            ),
            ("PLAIN".to_owned(), "as is".to_owned()),
        ];
        let own = vec![
            (db.clone(), "shop_18".to_owned()),
            (ticket, "SHOP-18".to_owned()),
        ];
        assert_eq!(
            substitute(&rows, &own, Os::Linux),
            pairs(&[
                ("DATABASE_URL", "postgres://localhost/shop_18?app=SHOP-18"),
                ("PLAIN", "as is"),
            ])
        );
        // layered substitutes too, and Otaq's own still come last.
        assert_eq!(
            layered(&rows[..1], &own[..1], None, Os::Linux),
            vec![
                (
                    "DATABASE_URL".to_owned(),
                    "postgres://localhost/shop_18?app=".to_owned()
                ),
                (db.clone(), "shop_18".to_owned()),
            ]
        );
        // Of two own variables with the same name, the later one (the one the program sees) is used.
        let twice = vec![
            (db.clone(), "first".to_owned()),
            (db.clone(), "last".to_owned()),
        ];
        let row = vec![("U".to_owned(), format!("${{{db}}}"))];
        assert_eq!(substitute(&row, &twice, Os::Linux), pairs(&[("U", "last")]));
        // A value taken from own is put in as it is, never expanded again.
        let nested = vec![(own_name("A"), format!("${{{}}}", own_name("B")))];
        let rows = vec![("X".to_owned(), format!("${{{}}}", own_name("A")))];
        assert_eq!(
            substitute(&rows, &nested, Os::Linux),
            vec![("X".to_owned(), format!("${{{}}}", own_name("B")))]
        );
    }

    #[test]
    fn unknown_own_references_become_empty() {
        let rows = vec![(
            "URL".to_owned(),
            format!("db/${{{}}}/end", own_name("DB_NAME")),
        )];
        assert_eq!(
            substitute(&rows, &[], Os::Linux),
            pairs(&[("URL", "db//end")])
        );
        let other = vec![(own_name("TICKET"), "SHOP-1".to_owned())];
        assert_eq!(
            substitute(&rows, &other, Os::Linux),
            pairs(&[("URL", "db//end")])
        );
    }

    #[test]
    fn other_placeholders_stay_literal() {
        let db = own_name("DB_NAME");
        let own = vec![
            (db.clone(), "shop_18".to_owned()),
            ("HOME".to_owned(), "/h".to_owned()),
        ];
        for value in [
            "${HOME}/x".to_owned(),
            format!("${db}"),
            format!("${{{db}"),
            format!("${{ {db} }}"),
            format!("${{{db}-default}}"),
            "${}".to_owned(),
            format!("${{{}}}", crate::naming::ENV_PREFIX),
            "no placeholder, $ and { and }".to_owned(),
        ] {
            let rows = vec![("V".to_owned(), value.clone())];
            assert_eq!(substitute(&rows, &own, Os::Linux), rows, "{value}");
        }
        // A literal one next to a substituted one.
        let rows = vec![("V".to_owned(), format!("${{HOME}}:${{{db}}}:${{"))];
        assert_eq!(
            substitute(&rows, &own, Os::Linux),
            pairs(&[("V", "${HOME}:shop_18:${")])
        );
        // The row's name is never touched.
        let rows = vec![(format!("${{{db}}}"), "v".to_owned())];
        assert_eq!(substitute(&rows, &own, Os::Linux), rows);
    }

    #[test]
    fn substitution_ignores_case_on_windows() {
        let db = own_name("DB_NAME");
        let lower = db.to_lowercase();
        let rows = vec![("URL".to_owned(), format!("db/${{{lower}}}"))];
        let own = vec![(db, "shop_18".to_owned())];
        assert_eq!(
            substitute(&rows, &own, Os::Windows),
            pairs(&[("URL", "db/shop_18")])
        );
        // Elsewhere a lowercase prefix isn't Otaq's: it stays as written.
        for os in [Os::Mac, Os::Linux] {
            assert_eq!(substitute(&rows, &own, os), rows);
        }
    }

    #[test]
    fn a_path_row_keeps_the_bin_folder_first() {
        let service = joined(&["/usr/bin", "/bin"]);
        let own = pairs(&[("PATH", &joined(&["/opt/cb/bin", "/usr/bin", "/bin"]))]);
        let row = joined(&["/custom/bin", "/usr/bin"]);
        let env = layered(
            &pairs(&[("PATH", &row)]),
            &own,
            Some(OsStr::new(&service)),
            Os::Linux,
        );
        assert_eq!(
            env,
            pairs(&[("PATH", &joined(&["/opt/cb/bin", "/custom/bin", "/usr/bin"]))])
        );
        // A `Path` row is `PATH` on Windows.
        let env = layered(
            &pairs(&[("Path", &row)]),
            &own,
            Some(OsStr::new(&service)),
            Os::Windows,
        );
        assert_eq!(
            env,
            pairs(&[("PATH", &joined(&["/opt/cb/bin", "/custom/bin", "/usr/bin"]))])
        );
        // With nothing of Otaq's in PATH (setup, git), the row's PATH is set as written.
        assert_eq!(
            layered(&pairs(&[("PATH", &row)]), &[], None, Os::Linux),
            pairs(&[("PATH", &row)])
        );
        // No PATH row: Otaq's PATH as it is.
        assert_eq!(
            layered(&[], &own, Some(OsStr::new(&service)), Os::Linux),
            own
        );
    }
}
