//! An agent's imported skills (AGM-01, AGM-04; WSK-02…WSK-04, d0092): **Import skill folder…** checks a folder, keeps
//! Otaq's own copy in `$OTAQ_HOME/skills/<id>/` and adds it to the agent's skills; **Re-import** copies the remembered
//! original again. Agents only ever use the copy, written into each session's folder by one name with the agent's
//! other skills ([`crate::skills`], AGM-07); a missing or broken copy means the shipped skill it stands for.

use std::path::{Path, PathBuf};

use super::{Agent, ImportedSkill, SkillRef};
use crate::board::{Board, BoardError, Refusal};
use crate::skills::folder;
use crate::store::{new_id, now_ms};

/// Runs blocking file work off the service's async threads.
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(work)
        .await
        .expect("skill file work panicked")
}

/// Copies `folder` into a new `skills/<id>/` (WSK-02): refused, with nothing changed, for a relative path, a folder
/// with no SKILL.md, or front matter that doesn't read.
async fn copy_in(board: &Board, from: &Path) -> Result<ImportedSkill, BoardError> {
    if !from.is_absolute() {
        return Err(BoardError::refused(
            Refusal::BadSkillFolder,
            format!("{} isn't an absolute path", from.display()),
        ));
    }
    let id = new_id();
    let (data, source, copy_id) = (board.data().clone(), from.to_owned(), id.clone());
    let copied = blocking(move || folder::import(&data, &source, &copy_id))
        .await
        .map_err(|folder::Refused(why)| BoardError::refused(Refusal::BadSkillFolder, why))?;
    Ok(ImportedSkill {
        id,
        name: copied.name,
        original: from.display().to_string(),
        imported_at: now_ms(),
        fingerprint: copied.fingerprint,
        warnings: copied.warnings,
    })
}

/// Imports the skill folder `from` with no agent yet (a proposal's skill, imported on Apply, FLW-04): Otaq's copy
/// and its record, which the caller gives to an agent.
pub async fn import_folder(board: &Board, from: &Path) -> Result<ImportedSkill, BoardError> {
    let _changing = board.lock_skill_changes().await;
    copy_in(board, from).await
}

async fn remove_copy(board: &Board, id: &str) {
    if let Ok(copy) = board.data().skill_path(id) {
        blocking(move || folder::remove(&copy)).await;
    }
}

/// **Import skill folder…** on `agent_id` (`agents.import_skill`): the folder checked and copied, then added at the
/// end of the agent's skills. The import's warnings are the record's.
pub async fn import(board: &Board, agent_id: &str, from: &Path) -> Result<Agent, BoardError> {
    let _changing = board.lock_skill_changes().await;
    let store = board.store();
    let agent = store
        .agent(agent_id)
        .await?
        .ok_or_else(|| BoardError::NotFound(format!("no agent {agent_id}")))?;
    let skill = copy_in(board, from).await?;
    let mut new = super::store::NewAgent {
        name: agent.name.clone(),
        instructions: agent.instructions.clone(),
        skills: agent.skills.clone(),
        imported: agent.imported.clone(),
        model: agent.model.clone(),
        effort: agent.effort.clone(),
        role: agent.role,
    };
    new.skills.push(SkillRef::Imported(skill.id.clone()));
    new.imported.push(skill.clone());
    if let Err(e) = store.update_agent(agent_id, &new, false).await {
        remove_copy(board, &skill.id).await;
        return Err(e.into());
    }
    store
        .agent(agent_id)
        .await?
        .ok_or_else(|| BoardError::NotFound(format!("no agent {agent_id}")))
}

/// **Re-import** (`agents.reimport_skill`, WSK-04): the import again from the remembered original, into a fresh copy
/// that replaces the old one for every agent holding it. "Original not found" when its folder is gone (the copy
/// stays).
pub async fn reimport(board: &Board, agent_id: &str, skill_id: &str) -> Result<Agent, BoardError> {
    let _changing = board.lock_skill_changes().await;
    let store = board.store();
    let agent = store
        .agent(agent_id)
        .await?
        .ok_or_else(|| BoardError::NotFound(format!("no agent {agent_id}")))?;
    let Some(old) = agent.imported_skill(skill_id).cloned() else {
        return Err(BoardError::refused(
            Refusal::NotImported,
            format!("{} has no such imported skill", agent.name),
        ));
    };
    let original = PathBuf::from(&old.original);
    if !original.is_dir() {
        return Err(BoardError::refused(
            Refusal::OriginalNotFound,
            format!("Original not found: {}", old.original),
        ));
    }
    let fresh = copy_in(board, &original).await?;
    let done = sqlx::query(
        "UPDATE agent_skills SET imported_id = ?, name = ?, original = ?, imported_at = ?, fingerprint = ?,
                                 warnings_json = ?
         WHERE imported_id = ?",
    )
    .bind(&fresh.id)
    .bind(&fresh.name)
    .bind(&fresh.original)
    .bind(fresh.imported_at)
    .bind(&fresh.fingerprint)
    .bind(serde_json::to_string(&fresh.warnings).expect("warnings serialize"))
    .bind(skill_id)
    .execute(store.pool())
    .await;
    if let Err(e) = done {
        remove_copy(board, &fresh.id).await;
        return Err(crate::store::StoreError::from(e).into());
    }
    remove_copy(board, skill_id).await;
    store
        .agent(agent_id)
        .await?
        .ok_or_else(|| BoardError::NotFound(format!("no agent {agent_id}")))
}

/// Deletes every folder under `skills/` that no agent holds (at service start): copies left by a crash between a
/// copy and its record, by a deleted agent or by the move-over's dropped project skills. A copy an open draft names
/// (a skill the Project agent wrote, FED-06, given to an agent on Apply) stays.
pub async fn remove_unreferenced(board: &Board) -> Result<usize, BoardError> {
    let _changing = board.lock_skill_changes().await;
    let held: std::collections::HashSet<String> = board
        .store()
        .imported_skill_ids()
        .await?
        .into_iter()
        .collect();
    let drafts: Vec<String> = sqlx::query_scalar("SELECT yaml FROM flow_proposals")
        .fetch_all(board.store().pool())
        .await
        .map_err(crate::store::StoreError::from)?;
    let dir = board.data().skills_dir();
    Ok(blocking(move || {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            // A draft names the copy by its path, written double-quoted when it needs quotes, so with each `\`
            // doubled (every Windows path).
            let path = entry.path().display().to_string();
            let quoted = path.replace('\\', "\\\\");
            if drafts
                .iter()
                .any(|yaml| yaml.contains(&path) || yaml.contains(&quoted))
            {
                continue;
            }
            if !held.contains(&*entry.file_name().to_string_lossy()) {
                folder::remove(&entry.path());
                removed += 1;
            }
        }
        removed
    })
    .await)
}
