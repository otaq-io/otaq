//! The amber mark on a model (AGT-05, AGM-04; d0088, d0077, UX-65): "Claude Code couldn't run `claude-fable-5-1` on
//! SHOP-48". It is on what the model came from ([`super::About`]): a stage of a project's flow, the flow's delivery,
//! or Noyan's setting (`model_marks`).
//!
//! - **Set** on any model refused ([`set`]): what the session ran for, with the model (the session's exact model,
//!   else the model passed, else the one the tool named) and where it happened (the ticket, or the project for a
//!   Noyan conversation), and the tool's own message. A later refusal replaces it.
//! - **Cleared**, never dismissed: when the stage's model or effort is changed and saved ([`clear_changed_in`]), or
//!   when a session that started after the mark ends its first turn normally ([`clear_after_first_turn`]). An
//!   instructions change, or another stage's run, leaves it.
//!
//! M23 (T-0357, d0088) adds two marks of the same kind:
//! - **A model no longer listed** ([`no_longer_lists`]): the check before a launch found the stage's model missing
//!   from its tool's list ("Codex no longer lists gpt-5.4"); it also clears when the tool's list has it again
//!   ([`clear_listed_again`]).
//! - **A swap** ([`swapped`]): a tool that quietly runs another model ([`crate::agent_tools::AgentTool::
//!   swaps_refused_model`]) ran another one than the exact model asked for ("Copilot ran auto instead of gpt-5.5 on
//!   SHOP-29"). A follow choice (Copilot's Auto, a family) choosing a model is never a swap (Q-J).

use super::{About, Mark, ModelCatalog, ToolModel};
use crate::agent_tools::Tool;
use crate::agent_tools::follow::FollowChoice;
use crate::store::{Store, StoreError, now_ms};

/// The model named when nothing says which one ran: a tool default the tool didn't name.
pub const DEFAULT_MODEL: &str = "its default model";

/// The mark's line: "Claude Code couldn't run claude-fable-5-1 on SHOP-48" (views draw the model in mono).
pub fn text(tool: &str, model: &str, on: &str) -> String {
    format!("{tool} couldn't run {model} on {on}")
}

/// The model a mark names: the session's exact model, else the model passed, else the one in the tool's message.
pub fn model_named<'a>(
    model_ran: Option<&'a str>,
    passed: Option<&'a str>,
    in_message: Option<&'a str>,
) -> &'a str {
    [model_ran, passed, in_message]
        .into_iter()
        .flatten()
        .find(|m| !m.trim().is_empty())
        .unwrap_or(DEFAULT_MODEL)
}

/// The mark for a refusal of `model` by `tool` (its name as shown) on `on` (a ticket's key, or a project), with
/// the tool's own `message`.
pub fn mark(tool: &str, model: &str, on: &str, message: &str) -> Mark {
    Mark {
        text: text(tool, model, on),
        model: Some(model.to_owned()).filter(|m| m != DEFAULT_MODEL),
        ticket: Some(on.to_owned()),
        message: Some(message.trim().to_owned()).filter(|m| !m.is_empty()),
        at: now_ms(),
    }
}

/// The line of a model no longer in its tool's list: "Codex no longer lists gpt-5.4".
pub fn no_longer_lists_text(tool: &str, model: &str) -> String {
    format!("{tool} no longer lists {model}")
}

/// The mark of `model` missing from `tool`'s list (its name as shown), found before a launch on `on`.
pub fn no_longer_lists(tool: &str, model: &str, on: &str) -> Mark {
    Mark {
        text: no_longer_lists_text(tool, model),
        model: Some(model.to_owned()),
        ticket: Some(on.to_owned()),
        message: None,
        at: now_ms(),
    }
}

/// The line of a swap: "Copilot ran auto instead of gpt-5.5 on SHOP-29".
pub fn swapped_text(tool: &str, ran: &str, asked: &str, on: &str) -> String {
    format!("{tool} ran {ran} instead of {asked} on {on}")
}

/// The mark of `tool` (its name as shown) running `ran` when `asked` was asked for, on `on`.
pub fn swapped(tool: &str, ran: &str, asked: &str, on: &str) -> Mark {
    Mark {
        text: swapped_text(tool, ran, asked, on),
        model: Some(asked.to_owned()),
        ticket: Some(on.to_owned()),
        message: None,
        at: now_ms(),
    }
}

/// Whether the model a session asked for is one exact model (Q-J): what the tool's list says it is, else by its
/// form (an Otaq follow choice, `newest:…` or `auto@…`, is none). A tool default (`None`) isn't.
pub fn asked_exact(tool: Tool, asked: Option<&str>, models: Option<&dyn ModelCatalog>) -> bool {
    let Some(asked) = asked.map(str::trim).filter(|m| !m.is_empty()) else {
        return false;
    };
    match models.and_then(|m| m.pick(tool, asked)) {
        Some(kind) => matches!(kind, ToolModel::Exact(_)),
        None => FollowChoice::parse(asked).is_none(),
    }
}

/// Whether a session that asked for `asked` and ran `ran` swapped (AGT-05): only on a tool that swaps, only for an
/// exact model asked for, and only when another one ran. A variant of the asked model (`#high`), which the reported
/// model doesn't carry, is the same model.
pub fn is_swap(tool_swaps: bool, asked_exact: bool, asked: Option<&str>, ran: &str) -> bool {
    let model = |m: &str| m.trim().split('#').next().unwrap_or_default().to_owned();
    tool_swaps && asked_exact && asked.is_some_and(|asked| model(asked) != model(ran))
}

/// The mark on `about`'s model, if any.
pub async fn get(store: &Store, about: &About) -> Result<Option<Mark>, StoreError> {
    let row: Option<MarkRow> = sqlx::query_as(
        "SELECT scope, text, model, ticket, message, marked_at FROM model_marks WHERE scope = ?",
    )
    .bind(about.scope())
    .fetch_optional(store.pool())
    .await?;
    Ok(row.map(|r| r.1))
}

/// Every mark, with what it is about (a stage, a delivery, Noyan's setting), oldest first.
pub async fn all(store: &Store) -> Result<Vec<(About, Mark)>, StoreError> {
    let rows: Vec<MarkRow> = sqlx::query_as(
        "SELECT scope, text, model, ticket, message, marked_at FROM model_marks ORDER BY marked_at, scope",
    )
    .fetch_all(store.pool())
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|MarkRow(scope, mark)| Some((About::parse(&scope)?, mark)))
        .collect())
}

/// A row of `model_marks`, read.
struct MarkRow(String, Mark);

impl<'r> sqlx::FromRow<'r, sqlx::sqlite::SqliteRow> for MarkRow {
    fn from_row(row: &'r sqlx::sqlite::SqliteRow) -> Result<Self, sqlx::Error> {
        use sqlx::Row;
        Ok(Self(
            row.try_get("scope")?,
            Mark {
                text: row.try_get("text")?,
                model: row.try_get("model")?,
                ticket: row.try_get("ticket")?,
                message: row.try_get("message")?,
                at: row.try_get("marked_at")?,
            },
        ))
    }
}

/// Sets `mark` on `about` unless it has that very mark already (same line and model). Whether it changed.
pub async fn set_if_new(store: &Store, about: &About, mark: &Mark) -> Result<bool, StoreError> {
    if get(store, about)
        .await?
        .is_some_and(|m| m.text == mark.text && m.model == mark.model)
    {
        return Ok(false);
    }
    set(store, about, mark).await
}

/// The tool's list has the model of a "no longer lists" mark again: the mark clears (AGT-05). `listed` says whether
/// a model is in its tool's list now. The scopes whose mark cleared.
pub async fn clear_listed_again(
    store: &Store,
    listed: impl Fn(Tool, &str) -> bool,
) -> Result<Vec<String>, StoreError> {
    let mut cleared = Vec::new();
    for (about, mark) in all(store).await? {
        let Some(model) = mark.model.as_deref() else {
            continue;
        };
        // The tool the mark names.
        let Some(tool) = Tool::ALL
            .iter()
            .copied()
            .find(|t| mark.text == no_longer_lists_text(t.display_name(), model))
        else {
            continue;
        };
        if !listed(tool, model) {
            continue;
        }
        let scope = about.scope();
        sqlx::query("DELETE FROM model_marks WHERE scope = ? AND text = ?")
            .bind(&scope)
            .bind(&mark.text)
            .execute(store.pool())
            .await?;
        cleared.push(scope);
    }
    Ok(cleared)
}

/// Sets (or replaces) the mark on `about`. Always `true`: a mark needs nothing else to be there.
pub async fn set(store: &Store, about: &About, mark: &Mark) -> Result<bool, StoreError> {
    sqlx::query(
        "INSERT INTO model_marks (scope, text, model, ticket, message, marked_at) VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (scope) DO UPDATE SET text = excluded.text, model = excluded.model, ticket = excluded.ticket,
             message = excluded.message, marked_at = excluded.marked_at",
    )
    .bind(about.scope())
    .bind(&mark.text)
    .bind(&mark.model)
    .bind(&mark.ticket)
    .bind(&mark.message)
    .bind(mark.at)
    .execute(store.pool())
    .await?;
    Ok(true)
}

/// Clears the mark on `about` (its model was changed and saved). Whether there was one.
pub async fn clear(store: &Store, about: &About) -> Result<bool, StoreError> {
    let done = sqlx::query("DELETE FROM model_marks WHERE scope = ?")
        .bind(about.scope())
        .execute(store.pool())
        .await?;
    Ok(done.rows_affected() > 0)
}

/// Clears, in `tx`, the marks on the stages (and the delivery) of the project `project` (its key) whose model or
/// effort `new` changed from `old` (a saved model change, AGT-05).
pub(crate) async fn clear_changed_in(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project: &str,
    old: &crate::flow::Flow,
    new: &crate::flow::Flow,
) -> Result<(), StoreError> {
    let changed = |a: &crate::flow::model::WhoDoesIt, b: &crate::flow::model::WhoDoesIt| {
        a.model != b.model || a.effort != b.effort
    };
    let mut gone = Vec::new();
    for stage in &old.stages {
        if new
            .stage(&stage.id)
            .is_none_or(|s| changed(&s.who, &stage.who))
        {
            gone.push(About::Stage {
                project: project.to_owned(),
                stage: stage.id.clone(),
            });
        }
    }
    if changed(&old.delivery.who, &new.delivery.who) {
        gone.push(About::Delivery {
            project: project.to_owned(),
        });
    }
    for about in gone {
        sqlx::query("DELETE FROM model_marks WHERE scope = ?")
            .bind(about.scope())
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// A session got through its first turn: the mark on what it ran for clears when the session started after it (the
/// model, as it is now, runs). Whether a mark was cleared.
pub async fn clear_after_first_turn(
    store: &Store,
    about: &About,
    session_started_at: i64,
) -> Result<bool, StoreError> {
    let done = sqlx::query("DELETE FROM model_marks WHERE scope = ? AND marked_at <= ?")
        .bind(about.scope())
        .bind(session_started_at)
        .execute(store.pool())
        .await?;
    Ok(done.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_names_the_tool_the_model_and_where() {
        assert_eq!(
            text("Claude Code", "claude-fable-5-1", "SHOP-48"),
            "Claude Code couldn't run claude-fable-5-1 on SHOP-48"
        );
    }

    #[test]
    fn the_model_is_the_one_that_ran_else_the_one_passed_else_the_tools() {
        let ran = Some("claude-fable-5-1");
        assert_eq!(model_named(ran, Some("fable"), None), "claude-fable-5-1");
        assert_eq!(model_named(None, Some("fable"), ran), "fable");
        assert_eq!(model_named(None, None, ran), "claude-fable-5-1");
        assert_eq!(model_named(None, Some(" "), None), DEFAULT_MODEL);
        let m = mark("Claude Code", DEFAULT_MODEL, "shop-web", " ");
        assert_eq!((m.model, m.message), (None, None));
        assert_eq!(m.ticket.as_deref(), Some("shop-web"));
    }

    #[test]
    fn the_m23_lines_name_the_tool_and_the_models() {
        assert_eq!(
            no_longer_lists_text("Codex", "gpt-5.4"),
            "Codex no longer lists gpt-5.4"
        );
        assert_eq!(
            swapped_text("Copilot", "auto", "gpt-5.5", "SHOP-29"),
            "Copilot ran auto instead of gpt-5.5 on SHOP-29"
        );
        let m = swapped("Copilot", "auto", "gpt-5.5", "SHOP-29");
        assert_eq!(m.model.as_deref(), Some("gpt-5.5"));
        assert_eq!(m.message, None);
    }

    #[test]
    fn only_an_exact_model_on_a_swapping_tool_swaps() {
        let tool = Tool::ALL[0];
        let list = super::super::FixedModels::new(vec![
            ToolModel::Exact("gpt-5.5".into()),
            ToolModel::Follow("auto".into()),
        ]);
        let exact = |m: Option<&str>| asked_exact(tool, m, Some(&list));
        assert!(exact(Some("gpt-5.5")));
        // Not listed: by its form.
        assert!(exact(Some("gpt-5.4")));
        assert!(!exact(Some("auto")));
        assert!(!exact(Some("auto@balance")));
        assert!(!exact(Some("newest:gpt-sol")));
        assert!(!exact(None));
        assert!(is_swap(true, true, Some("gpt-5.5"), "auto"));
        assert!(!is_swap(true, true, Some("gpt-5.5"), "gpt-5.5"));
        assert!(!is_swap(
            true,
            true,
            Some("opencode/gpt-5.5#high"),
            "opencode/gpt-5.5"
        ));
        assert!(!is_swap(false, true, Some("gpt-5.5"), "auto"));
        // Auto choosing a model is never a swap (Q-J).
        assert!(!is_swap(
            true,
            exact(Some("auto")),
            Some("auto"),
            "mai-code-1.1-flash"
        ));
    }
}
