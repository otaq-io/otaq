//! The tools' model lists (AGT-02; d0086, d0088): what each tool itself says it can run, read through its adapter
//! ([`super::AgentTool::list_models`]), and Otaq's copy of it.
//!
//! Only what a tool's list contains can be picked for a named agent (d0086). The copy is kept per tool in the
//! `settings` table (one key per tool, JSON: [`settings_key`]) with when it was read, and refreshed at service
//! start, on `agents.models_refresh` (Settings → Agents opening) and once a day ([`ModelLists::run`]). A failed
//! read keeps the old copy and is logged. Readers ask the in-memory copy, synchronously
//! ([`ModelLists::copy`]): "is `opus` in Claude Code's list, and does it follow the newest?" is
//! `lists.copy(Tool::Claude).and_then(|c| c.list.entry("opus").map(|e| e.kind == ModelKind::Follow))`.
//!
//! Codex and OpenCode have no aliases of their own: their "always the newest" choices are families (d0086). A copy
//! answers them first, one [`ModelKind::Follow`] entry per family that has at least one model in the tool's list,
//! stored `newest:<family>` and resolving to that family's newest model in the list by release date
//! ([`follow_entries`]). They are made each time a copy is asked for, from the tool's list and the current
//! models.dev copy ([`super::catalog`]), so a new list or a new catalog changes them at once.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use super::catalog::{Catalog, Catalogs, Family, FamilySource};
use super::follow::FollowChoice;
use super::{Adapters, Tool};
use crate::store::{Store, now_ms};

/// How often the copies are read again while the service runs (d0086: "once a day").
pub const REFRESH_EVERY: Duration = Duration::from_secs(24 * 60 * 60);

/// What a list entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelKind {
    /// The tool's own default (Claude Code's `default`): shown as "Tool default" and passed as nothing (d0088).
    ToolDefault,
    /// A name the tool resolves to its newest model of a family (Claude Code's `opus`, `sonnet`, `haiku`,
    /// `fable`): "always the newest", passed as it is (d0086).
    Follow,
    /// One exact model.
    Exact,
}

/// One model a tool can run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelEntry {
    /// What is passed to the tool (`opus`); the tool default's id is the tool's own name for it (`default`).
    pub id: String,
    /// The tool's name for it: "Opus".
    pub display_name: String,
    /// The tool's description, when it gives one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The exact model it runs today: `claude-opus-5-5`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolves_to: Option<String>,
    pub kind: ModelKind,
    /// The model's family when the tool's own list says it (OpenCode's `family`, [`FamilySource::ListCarriesFamily`]);
    /// on a follow entry, the family it follows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// When the model was released, in Unix milliseconds, when the tool's own list says it (OpenCode's
    /// `time.released`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub released_at: Option<i64>,
}

impl ModelEntry {
    /// The model option the tool gets: nothing for the tool default, else the id unchanged (d0088).
    pub fn passed(&self) -> Option<&str> {
        match self.kind {
            ModelKind::ToolDefault => None,
            ModelKind::Follow | ModelKind::Exact => Some(&self.id),
        }
    }
}

/// A tool's model list, in the tool's own order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelList {
    pub entries: Vec<ModelEntry>,
}

impl ModelList {
    /// The entry passed as `id` (`opus`, `claude-opus-5-5`).
    pub fn entry(&self, id: &str) -> Option<&ModelEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Whether `id` can be picked.
    pub fn contains(&self, id: &str) -> bool {
        self.entry(id).is_some()
    }

    /// The tool default's entry: what an empty model runs on today.
    pub fn tool_default(&self) -> Option<&ModelEntry> {
        self.entries
            .iter()
            .find(|e| e.kind == ModelKind::ToolDefault)
    }
}

/// Why a tool's list couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelListError {
    /// The sentence a person reads: "Claude Code didn't list its models".
    pub message: String,
    /// What went wrong, for the log.
    pub reason: String,
}

impl std::fmt::Display for ModelListError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.message, self.reason)
    }
}

impl std::error::Error for ModelListError {}

/// Otaq's copy of one tool's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelCopy {
    pub tool: Tool,
    pub list: ModelList,
    /// When the tool was read, in Unix milliseconds.
    pub read_at: i64,
}

/// Whether `id` follows a family (`newest:gpt-sol`, [`FollowChoice::Newest`]).
fn follows_family(id: &str) -> bool {
    matches!(FollowChoice::parse(id), Some(FollowChoice::Newest { .. }))
}

impl ModelCopy {
    /// The exact model a follow choice `id` runs on now (AGT-05, d0086): `newest:gpt-sol` → the newest model of
    /// that family in this copy (`gpt-6-sol`), `None` when the list has none of it any more (the ticket then
    /// holds as Model unavailable). Any other id resolves to itself: Claude Code's aliases and Copilot's
    /// `auto@…` are passed as they are.
    pub fn resolve_follow(&self, id: &str) -> Option<String> {
        if !follows_family(id) {
            return Some(id.to_owned());
        }
        self.list.entry(id)?.resolves_to.clone()
    }
}

/// The follow entries of `list`, one per family with at least one of its models in `list` (d0086), in the order
/// their first model appears: id `newest:<family>`, named after the family ("GPT Sol"), resolving to the newest
/// model of the family in `list` by release date (ties: the list's own order). Families and dates come from
/// `source`: models.dev's `provider` (Codex), or the list's own fields with models.dev for what they miss
/// (OpenCode). A model with no family stays an exact model only; without a catalog a models.dev source gives none.
pub fn follow_entries(
    list: &ModelList,
    source: Option<FamilySource>,
    catalog: Option<&Catalog>,
) -> Vec<ModelEntry> {
    let Some(source) = source else {
        return Vec::new();
    };
    // Each exact model's family and date, in the list's order.
    let known = list
        .entries
        .iter()
        .filter(|e| e.kind == ModelKind::Exact)
        .filter_map(|e| {
            let (family, released) = match source {
                FamilySource::ModelsDev { provider } => {
                    let m = catalog?.model(provider, &e.id)?;
                    (m.family.clone()?, m.released_at())
                }
                FamilySource::ListCarriesFamily {
                    models_dev_fallback,
                } => {
                    let m = catalog
                        .filter(|_| models_dev_fallback)
                        .and_then(|c| c.model_by_path(&e.id));
                    let family = e
                        .family
                        .clone()
                        .or_else(|| m.and_then(|m| m.family.clone()))?;
                    (
                        family,
                        e.released_at.or_else(|| m.and_then(|m| m.released_at())),
                    )
                }
            };
            Some((family, released, e.id.as_str()))
        });
    // family → (its newest model's date and id), in the order families first appear.
    let mut newest: Vec<(String, Option<i64>, &str)> = Vec::new();
    for (family, released, id) in known {
        match newest.iter_mut().find(|(f, ..)| *f == family) {
            // Only a later date replaces it, so a tie keeps the one listed first.
            Some(found) if released > found.1 => (found.1, found.2) = (released, id),
            Some(_) => {}
            None => newest.push((family, released, id)),
        }
    }
    newest
        .into_iter()
        .map(|(family, _, id)| ModelEntry {
            id: FollowChoice::Newest {
                family: family.clone(),
            }
            .stored(),
            display_name: Family::new(family.as_str()).display_name(),
            description: None,
            resolves_to: Some(id.to_owned()),
            kind: ModelKind::Follow,
            family: Some(family),
            released_at: None,
        })
        .collect()
}

/// The `settings` key of `tool`'s copy: `agent_tools.models.claude`.
pub fn settings_key(tool: Tool) -> String {
    format!("agent_tools.models.{}", tool.as_str())
}

/// The copies of every tool's list, held by the service. Cheap to clone.
#[derive(Clone)]
pub struct ModelLists {
    store: Store,
    adapters: Adapters,
    /// Whether the tools are read at all: only the daemon's service reads them, so no test starts a tool.
    read_tools: bool,
    /// The tools' lists as they read them, without follow entries.
    copies: Arc<RwLock<HashMap<Tool, ModelCopy>>>,
    /// The models.dev copy the follow entries come from.
    catalogs: Catalogs,
    /// Sent each time a copy is replaced, for the check before every launch (a model listed again, T-0357).
    changes: Arc<watch::Sender<()>>,
    /// The hand-check seam `OTAQ_DEV_DROP_MODEL` ([`dev_drop_model`]): a model every read of its tool's list leaves
    /// out. Debug builds only; `None` otherwise.
    dropped: Option<(Tool, String)>,
}

/// The environment variable of the hand-check seam (debug builds only, `OTAQ_DEV_DROP_MODEL`): `<tool>:<id>`, e.g.
/// `codex:gpt-5.4`.
pub fn dev_drop_model_var() -> String {
    crate::naming::env_var("DEV_DROP_MODEL")
}

/// What `OTAQ_DEV_DROP_MODEL`'s `value` asks for (T-0357): every read of that tool's list leaves the id out, so "a
/// model no longer listed" can be produced by hand without a tool retiring one. Only in debug builds: a release build
/// ignores it (`None`).
pub fn dev_drop_model(value: Option<&str>) -> Option<(Tool, String)> {
    if !cfg!(debug_assertions) {
        return None;
    }
    let (tool, id) = value?.trim().split_once(':')?;
    let tool: Tool = tool.trim().parse().ok()?;
    let id = id.trim();
    (!id.is_empty()).then(|| (tool, id.to_owned()))
}

impl ModelLists {
    /// The copies stored earlier, and the models.dev copy of `catalogs`. With `read_tools` off, [`Self::refresh`]
    /// and [`Self::run`] read nothing (neither the tools nor models.dev) and keep them.
    pub async fn load(
        store: Store,
        adapters: Adapters,
        read_tools: bool,
        catalogs: Catalogs,
    ) -> Self {
        let mut copies = HashMap::new();
        for &tool in Tool::ALL {
            match stored(&store, tool).await {
                Ok(Some(mut copy)) => {
                    // A copy stored by an earlier Otaq may hold an Auto tier Copilot no longer runs (decision 117):
                    // it goes now, so the check before a launch holds it before the next read (offline too).
                    if tool == Tool::Copilot {
                        super::copilot::models::drop_retired(&mut copy.list);
                    }
                    copies.insert(tool, copy);
                }
                Ok(None) => {}
                Err(e) => tracing::warn!(%tool, error = %e, "can't read the stored model list"),
            }
        }
        Self {
            store,
            adapters,
            read_tools,
            copies: Arc::new(RwLock::new(copies)),
            catalogs,
            changes: Arc::new(watch::channel(()).0),
            dropped: None,
        }
    }

    /// Leaves `dropped`'s model out of every later read of its tool's list (the debug-build seam,
    /// [`dev_drop_model`]; set once at service start).
    pub fn drop_model(mut self, dropped: Option<(Tool, String)>) -> Self {
        if let Some((tool, id)) = &dropped {
            tracing::warn!(%tool, model = id, "{}: every read of this list leaves the model out", dev_drop_model_var());
        }
        self.dropped = dropped;
        self
    }

    /// Fires each time a tool's copy is replaced.
    pub fn subscribe(&self) -> watch::Receiver<()> {
        self.changes.subscribe()
    }

    /// `tool`'s copy, if it was ever read: its follow entries on families first, then the tool's list.
    pub fn copy(&self, tool: Tool) -> Option<ModelCopy> {
        let mut copy = self
            .copies
            .read()
            .expect("model lists")
            .get(&tool)
            .cloned()?;
        let catalog = self.catalogs.copy();
        let mut entries = follow_entries(
            &copy.list,
            self.adapters.get(tool).family_source(),
            catalog.as_ref().map(|c| &c.catalog),
        );
        entries.append(&mut copy.list.entries);
        copy.list.entries = entries;
        Some(copy)
    }

    /// The exact model `tool`'s follow choice `id` runs on now ([`ModelCopy::resolve_follow`]); an id that
    /// isn't a family resolves to itself even before the list was read.
    pub fn resolve_follow(&self, tool: Tool, id: &str) -> Option<String> {
        if !follows_family(id) {
            return Some(id.to_owned());
        }
        self.copy(tool)?.resolve_follow(id)
    }

    /// The picker's Retry (`agents.models_retry`): reads `tool`'s list again at once, and models.dev too when
    /// there is no copy of it yet or it is old.
    pub async fn retry(&self, tool: Tool) -> Option<ModelCopy> {
        self.refresh_catalog().await;
        self.refresh(tool).await
    }

    /// Reads models.dev when the copy is missing or a day old: only in a service that reads the tools, and only
    /// while some tool takes its families from it.
    async fn refresh_catalog(&self) {
        let used = Tool::ALL
            .iter()
            .any(|&tool| self.adapters.get(tool).family_source().is_some());
        if self.read_tools && used {
            let _ = self
                .catalogs
                .refresh_if_older(super::catalog::MAX_AGE, now_ms())
                .await;
        }
    }

    /// Reads `tool`'s list again through its adapter and answers the copy as it is then.
    pub async fn refresh(&self, tool: Tool) -> Option<ModelCopy> {
        if !self.read_tools {
            tracing::debug!(%tool, "model lists aren't read by this service");
            return self.copy(tool);
        }
        let read = self.adapters.get(tool).list_models().await;
        self.record(tool, read, now_ms()).await
    }

    /// Reads every tool's list again.
    pub async fn refresh_all(&self) {
        for &tool in Tool::ALL {
            self.refresh(tool).await;
        }
    }

    /// Keeps what a read of `tool`'s list gave at `now`: a list replaces the copy (stored, then kept in memory);
    /// a failure keeps the old copy and is logged. Answers the copy as it is then.
    pub async fn record(
        &self,
        tool: Tool,
        read: Result<ModelList, ModelListError>,
        now: i64,
    ) -> Option<ModelCopy> {
        match read {
            Ok(mut list) => {
                if let Some((dropped, id)) = &self.dropped
                    && *dropped == tool
                {
                    list.entries.retain(|e| &e.id != id);
                }
                let copy = ModelCopy {
                    tool,
                    list,
                    read_at: now,
                };
                tracing::info!(%tool, models = copy.list.entries.len(), "model list read");
                let json = serde_json::to_string(&copy).expect("a model list serialises");
                if let Err(e) = self.store.set_setting(&settings_key(tool), &json).await {
                    tracing::warn!(%tool, error = %e, "can't store the model list");
                }
                self.copies.write().expect("model lists").insert(tool, copy);
                self.changes.send_replace(());
                self.copy(tool)
            }
            Err(e) => {
                tracing::warn!(%tool, error = %e, "model list not read; the last copy stays");
                self.copy(tool)
            }
        }
    }

    /// Reads models.dev when its copy is a day old, then every tool's list, at once (the service's start), then
    /// once a day, until stopped.
    pub async fn run(self, mut stop: watch::Receiver<bool>) {
        let mut tick = tokio::time::interval(REFRESH_EVERY);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = stop.wait_for(|s| *s) => break,
                _ = tick.tick() => {}
            }
            tokio::select! {
                _ = stop.wait_for(|s| *s) => break,
                () = async {
                    self.refresh_catalog().await;
                    self.refresh_all().await;
                } => {}
            }
        }
    }
}

/// `tool`'s stored copy. A value that no longer reads as a copy counts as none (logged).
async fn stored(store: &Store, tool: Tool) -> Result<Option<ModelCopy>, crate::store::StoreError> {
    let Some(json) = store.setting(&settings_key(tool)).await? else {
        return Ok(None);
    };
    Ok(serde_json::from_str(&json)
        .inspect_err(|e| tracing::warn!(%tool, error = %e, "the stored model list doesn't read"))
        .ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_dir::DataDir;

    fn entry(id: &str, kind: ModelKind) -> ModelEntry {
        ModelEntry {
            id: id.into(),
            display_name: id.into(),
            description: None,
            resolves_to: Some(format!("claude-{id}-5-5")),
            kind,
            family: None,
            released_at: None,
        }
    }

    fn exact(id: &str) -> ModelEntry {
        ModelEntry {
            resolves_to: None,
            ..entry(id, ModelKind::Exact)
        }
    }

    /// Codex's own list as `model/list` gives it signed in, hidden models left out (written from
    /// `reports/Models across the four tools (2026-10-17).md`, Codex row 2).
    fn codex_like() -> ModelList {
        ModelList {
            entries: [
                "gpt-6-astra",
                "gpt-6-sol",
                "gpt-6-luna",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
            ]
            .map(exact)
            .to_vec(),
        }
    }

    const CODEX_LIKE: Option<FamilySource> = Some(FamilySource::ModelsDev { provider: "openai" });
    const OPENCODE_LIKE: Option<FamilySource> = Some(FamilySource::ListCarriesFamily {
        models_dev_fallback: true,
    });

    /// OpenCode's own list (`/api/model`): `provider/id`, with `family` and `time.released` on some models only
    /// (written from the ticket file's CLI probe facts and the models report; T-0369 replaces it with a capture).
    fn opencode_like() -> ModelList {
        let with = |id: &str, family: Option<&str>, released: Option<&str>| ModelEntry {
            family: family.map(str::to_owned),
            released_at: released.and_then(super::super::catalog::tests::date),
            ..exact(id)
        };
        ModelList {
            entries: vec![
                // Its own family and date.
                with(
                    "opencode/big-pickle",
                    Some("big-pickle"),
                    Some("2025-10-17"),
                ),
                // No family in the list: models.dev names one for the same provider/id.
                with("anthropic/claude-sonnet-4-5", None, Some("2025-09-29")),
                with("anthropic/claude-sonnet-5-5", None, None),
                // No family anywhere: an exact model only.
                with("opencode/fledge-alpha-free", None, Some("2026-10-01")),
            ],
        }
    }

    fn follow_ids(entries: &[ModelEntry]) -> Vec<(String, String, Option<String>)> {
        entries
            .iter()
            .map(|e| (e.id.clone(), e.display_name.clone(), e.resolves_to.clone()))
            .collect()
    }

    fn row(id: &str, name: &str, now: &str) -> (String, String, Option<String>) {
        (id.to_owned(), name.to_owned(), Some(now.to_owned()))
    }

    /// Codex: one entry per family with a model in its list, the newest by date ("GPT Sol · always the newest"
    /// now `gpt-6-sol`, though models.dev also lists the newer `gpt-6.1-sol` Codex doesn't offer).
    #[test]
    fn codex_families_follow_the_newest_model_in_its_own_list() {
        let catalog = super::super::catalog::tests::fixture();
        let entries = follow_entries(&codex_like(), CODEX_LIKE, Some(&catalog));
        assert_eq!(
            follow_ids(&entries),
            [
                row("newest:gpt-astra", "GPT Astra", "gpt-6-astra"),
                row("newest:gpt-sol", "GPT Sol", "gpt-6-sol"),
                row("newest:gpt-luna", "GPT Luna", "gpt-6-luna"),
                row("newest:gpt-terra", "GPT Terra", "gpt-5.6-terra"),
            ]
        );
        assert!(entries.iter().all(|e| e.kind == ModelKind::Follow));
        // `gpt-5.6` is in models.dev's gpt-sol family, but not in Codex's list: it changes nothing. A family with
        // no model in the list gets no entry.
        let only_sol = ModelList {
            entries: vec![exact("gpt-5.6-sol")],
        };
        assert_eq!(
            follow_ids(&follow_entries(&only_sol, CODEX_LIKE, Some(&catalog))),
            [row("newest:gpt-sol", "GPT Sol", "gpt-5.6-sol")]
        );
        // No catalog, or a tool with no family source: no follow entries.
        assert!(follow_entries(&codex_like(), CODEX_LIKE, None).is_empty());
        assert!(follow_entries(&codex_like(), None, Some(&catalog)).is_empty());
    }

    /// OpenCode: the list's own family and date first, models.dev's for what it misses; a model with no family
    /// anywhere stays exact only; ties keep the list's order.
    #[test]
    fn opencode_families_come_from_its_list_then_models_dev() {
        let catalog = super::super::catalog::tests::fixture();
        assert_eq!(
            follow_ids(&follow_entries(
                &opencode_like(),
                OPENCODE_LIKE,
                Some(&catalog)
            )),
            [
                row("newest:big-pickle", "Big Pickle", "opencode/big-pickle"),
                row(
                    "newest:claude-sonnet",
                    "Claude Sonnet",
                    "anthropic/claude-sonnet-5-5"
                ),
            ]
        );
        // Without models.dev only the list's own families count.
        assert_eq!(
            follow_ids(&follow_entries(&opencode_like(), OPENCODE_LIKE, None)),
            [row(
                "newest:big-pickle",
                "Big Pickle",
                "opencode/big-pickle"
            )]
        );
        // Same date: the one listed first.
        let tie = ModelList {
            entries: vec![
                ModelEntry {
                    family: Some("big-pickle".into()),
                    ..exact("opencode/big-pickle")
                },
                ModelEntry {
                    family: Some("big-pickle".into()),
                    ..exact("zen/big-pickle")
                },
            ],
        };
        assert_eq!(
            follow_entries(&tie, OPENCODE_LIKE, None)[0]
                .resolves_to
                .as_deref(),
            Some("opencode/big-pickle")
        );
    }

    /// A follow choice resolves to its family's newest model in the copy, `None` once the list has none of it;
    /// other ids resolve to themselves.
    #[test]
    fn a_follow_choice_resolves_from_the_copy() {
        let catalog = super::super::catalog::tests::fixture();
        let with_follow = |list: ModelList| {
            let mut entries = follow_entries(&list, CODEX_LIKE, Some(&catalog));
            entries.extend(list.entries);
            ModelCopy {
                tool: Tool::Claude,
                list: ModelList { entries },
                read_at: 1,
            }
        };
        let copy = with_follow(codex_like());
        assert_eq!(
            copy.resolve_follow("newest:gpt-sol").as_deref(),
            Some("gpt-6-sol")
        );
        assert_eq!(copy.resolve_follow("newest:gpt-nova"), None);
        assert_eq!(
            copy.resolve_follow("gpt-6-luna").as_deref(),
            Some("gpt-6-luna")
        );
        assert_eq!(
            copy.resolve_follow("auto@balance").as_deref(),
            Some("auto@balance")
        );
        // The list no longer has any GPT Sol model.
        let gone = with_follow(ModelList {
            entries: vec![exact("gpt-6-luna")],
        });
        assert_eq!(gone.resolve_follow("newest:gpt-sol"), None);
    }

    fn list(ids: &[(&str, ModelKind)]) -> ModelList {
        ModelList {
            entries: ids.iter().map(|&(id, kind)| entry(id, kind)).collect(),
        }
    }

    async fn lists(home: &std::path::Path) -> ModelLists {
        let data = DataDir::new(home);
        let store = Store::open_in(&data).await.unwrap();
        let catalogs = Catalogs::load(&data);
        ModelLists::load(store, Adapters::launching_only(None, data), false, catalogs).await
    }

    #[test]
    fn entries_say_what_is_passed() {
        let l = list(&[
            ("default", ModelKind::ToolDefault),
            ("opus", ModelKind::Follow),
        ]);
        assert!(l.contains("opus") && !l.contains("foo-bar-9"));
        assert_eq!(l.entry("opus").unwrap().passed(), Some("opus"));
        assert_eq!(l.tool_default().unwrap().passed(), None);
        assert_eq!(settings_key(Tool::Claude), "agent_tools.models.claude");
    }

    /// The copy is stored, read back by the next service, and replaced by a later read; a failed read keeps it.
    #[tokio::test]
    async fn the_copy_is_stored_read_back_replaced_and_kept_on_failure() {
        let home = tempfile::tempdir().unwrap();
        let first = lists(home.path()).await;
        assert_eq!(first.copy(Tool::Claude), None);
        let one = list(&[("default", ModelKind::ToolDefault)]);
        let copy = first.record(Tool::Claude, Ok(one.clone()), 1_000).await;
        assert_eq!(copy.as_ref().map(|c| c.read_at), Some(1_000));

        // The next service reads it back from the settings table.
        let again = lists(home.path()).await;
        assert_eq!(again.copy(Tool::Claude), copy);

        // A later read replaces it.
        let two = list(&[
            ("default", ModelKind::ToolDefault),
            ("opus", ModelKind::Follow),
        ]);
        let replaced = again.record(Tool::Claude, Ok(two.clone()), 2_000).await;
        assert_eq!(
            replaced,
            Some(ModelCopy {
                tool: Tool::Claude,
                list: two.clone(),
                read_at: 2_000
            })
        );

        // A failed read keeps the old copy, in memory and stored.
        let failed = ModelListError {
            message: "Claude Code didn't list its models".into(),
            reason: "timed out".into(),
        };
        assert_eq!(
            again.record(Tool::Claude, Err(failed), 3_000).await,
            replaced
        );
        assert_eq!(lists(home.path()).await.copy(Tool::Claude), replaced);
    }

    /// Decision 117: Copilot's copy stored by an earlier Otaq still has `auto@efficiency`. Loaded, it has it no
    /// more, before any read of Copilot's list (offline), so the check before a new start holds an agent saved on it
    /// "Model unavailable"; the tiers Copilot runs still pass, and another tool's copy is left as stored.
    #[tokio::test]
    async fn copilots_stored_copy_loses_the_retired_tier_on_load() {
        let home = tempfile::tempdir().unwrap();
        let stale = list(&[
            ("auto@efficiency", ModelKind::Follow),
            ("auto@balance", ModelKind::Follow),
            ("auto@intelligence", ModelKind::Follow),
            ("gpt-5.5", ModelKind::Exact),
        ]);
        let first = lists(home.path()).await;
        first.record(Tool::Copilot, Ok(stale.clone()), 1_000).await;
        first.record(Tool::Codex, Ok(stale.clone()), 1_000).await;

        let again = lists(home.path()).await;
        let copy = again.copy(Tool::Copilot).unwrap();
        let ids: Vec<&str> = copy.list.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["auto@balance", "auto@intelligence", "gpt-5.5"]);
        assert_eq!(copy.read_at, 1_000);
        use crate::agents::ModelRef;
        use crate::agents::gate::{Blocked, Facts, check};
        let mut facts = Facts::default();
        facts.lists.insert(Tool::Copilot, copy);
        let on = |model: &str| ModelRef {
            tool: Tool::Copilot,
            model: Some(model.to_owned()),
        };
        assert_eq!(
            check(&on("auto@efficiency"), &facts),
            Err(Blocked::ModelUnlisted)
        );
        assert_eq!(
            check(&on("auto@balance"), &facts),
            Ok(Some("auto@balance".to_owned()))
        );
        assert!(
            again
                .copy(Tool::Codex)
                .unwrap()
                .list
                .contains("auto@efficiency")
        );
    }

    /// A service that doesn't read tools (every test's) refreshes nothing and answers the copy it has.
    #[tokio::test]
    async fn the_hand_check_seam_leaves_one_model_out_of_every_read_and_tells() {
        let home = tempfile::tempdir().unwrap();
        let l = lists(home.path())
            .await
            .drop_model(Some((Tool::Claude, "fable".into())));
        let changes = l.subscribe();
        let read = list(&[("opus", ModelKind::Follow), ("fable", ModelKind::Follow)]);
        let copy = l
            .record(Tool::Claude, Ok(read.clone()), 1_000)
            .await
            .unwrap();
        assert!(copy.list.contains("opus") && !copy.list.contains("fable"));
        assert!(changes.has_changed().unwrap(), "a new copy is announced");
        // Another tool's list keeps it.
        let other = l.record(Tool::Codex, Ok(read), 1_000).await.unwrap();
        assert!(other.list.contains("fable"));
    }

    #[tokio::test]
    async fn a_service_that_reads_no_tools_keeps_its_copy() {
        let home = tempfile::tempdir().unwrap();
        let l = lists(home.path()).await;
        assert_eq!(l.refresh(Tool::Claude).await, None);
        let copy = l
            .record(
                Tool::Claude,
                Ok(list(&[("opus", ModelKind::Follow)])),
                1_000,
            )
            .await;
        assert_eq!(l.refresh(Tool::Claude).await, copy);
        // Retry reads nothing either (no tool, no models.dev) and answers the copy.
        assert_eq!(l.retry(Tool::Claude).await, copy);
    }

    /// A tool with no family source (Claude Code) keeps its list as it read it, catalog or not; its aliases
    /// resolve to themselves, read or not.
    #[tokio::test]
    async fn a_tool_without_families_keeps_its_list() {
        let home = tempfile::tempdir().unwrap();
        let l = lists(home.path()).await;
        assert_eq!(
            l.resolve_follow(Tool::Claude, "opus").as_deref(),
            Some("opus")
        );
        assert_eq!(l.resolve_follow(Tool::Claude, "newest:gpt-sol"), None);
        let read = list(&[
            ("default", ModelKind::ToolDefault),
            ("opus", ModelKind::Follow),
        ]);
        let copy = l.record(Tool::Claude, Ok(read.clone()), 1_000).await;
        assert_eq!(copy.map(|c| c.list), Some(read));
    }
}
