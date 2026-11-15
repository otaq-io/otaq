//! The rename of Otaq's skills in flows (tuq decisions 063, 075): a flow written before it names Otaq's skills
//! bare (`validate`, `product-spec`); from it on they are `otaq:<name>` and `tuq:<name>`, and a bare name is always
//! the user's skill. Each project's current flow gets a new version written by Otaq, once, at the first start of the
//! version that renames them ([`crate::store::Store::open`]); "Back to the previous flow" to a version stored before
//! it and importing a flow file written before it get the same rewrite ([`crate::board::Board::propose_from`]).

use super::model::FlowFile;
use crate::skills;

/// Renames every bare name of one of Otaq's skills in `file`, the stages' `skill:` and the delivery's, to its
/// namespaced name (`validate` → `otaq:validate`, `product-spec` → `tuq:product-spec`); every other name stays as
/// it is (`team-validate`, `my-pr`). Whether anything changed.
pub fn name_own_skills(file: &mut FlowFile) -> bool {
    let mut changed = false;
    let skills = file
        .flow
        .stages
        .iter_mut()
        .map(|s| &mut s.skill)
        .chain(std::iter::once(&mut file.flow.delivery.skill));
    for skill in skills.flatten() {
        if let Some(named) = skills::namespaced(skill.trim()) {
            *skill = named;
            changed = true;
        }
    }
    changed
}

/// Whether `file` reads as written before the rename: it names one of Otaq's skills bare and none in a namespace of
/// Otaq's (`otaq:`, `tuq:`). An imported flow file carries no date, so this is what tells an older one ([`name_own_skills`]
/// then applies); a file naming `otaq:validate` anywhere was written after it, and its bare names are the user's.
pub fn written_before_rename(file: &FlowFile) -> bool {
    let names: Vec<&str> = file
        .flow
        .stages
        .iter()
        .filter_map(|s| s.skill.as_deref())
        .chain(file.flow.delivery.skill.as_deref())
        .map(str::trim)
        .collect();
    let namespaced = names.iter().any(|name| {
        name.split_once(':').is_some_and(|(namespace, _)| {
            namespace == skills::OTAQ_NAMESPACE || namespace == skills::TUQ_NAMESPACE
        })
    });
    !namespaced && names.iter().any(|name| skills::namespaced(name).is_some())
}

/// The rename on a stored document's text, line by line: each `skill:` line naming one of Otaq's skills bare names it
/// in its namespace, and nothing else in the text changes (an open proposal or a waiting change keeps its layout and
/// whatever only its text carries). `None` when no line changed.
pub fn renamed_text(text: &str) -> Option<String> {
    let mut changed = false;
    let lines: Vec<String> = text
        .split_inclusive('\n')
        .map(|line| match renamed_line(line) {
            Some(renamed) => {
                changed = true;
                renamed
            }
            None => line.to_owned(),
        })
        .collect();
    changed.then(|| lines.concat())
}

/// One `skill: <name>` line (after its indent, or after a list's `- `), its name maybe quoted, renamed.
fn renamed_line(line: &str) -> Option<String> {
    let body = line.trim_end_matches(['\n', '\r']);
    let end = &line[body.len()..];
    let indent = body.len() - body.trim_start().len();
    let rest = &body[indent..];
    let (lead, rest) = match rest.strip_prefix("- ") {
        Some(after) => ("- ", after.trim_start()),
        None => ("", rest),
    };
    let value = rest.strip_prefix("skill:")?.trim();
    let unquoted = ['"', '\'']
        .iter()
        .find_map(|q| value.strip_prefix(*q).and_then(|v| v.strip_suffix(*q)))
        .unwrap_or(value);
    let named = skills::namespaced(unquoted)?;
    Some(format!("{}{lead}skill: {named}{end}", &body[..indent]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::yaml;

    const OLDER: &str = "flow:
  - id: spec
    name: Spec
    kind: agent
    agent: Developer
    skill: product-spec
  - id: dev
    name: Dev
    kind: agent
    agent: Developer
    skill: work-ticket
  - id: validate
    name: Validate
    kind: check
    by: agent
    agent: Checker
    skill: validate
    rounds: 3
  - id: team
    name: Team check
    kind: check
    by: agent
    agent: Checker
    skill: team-validate
    rounds: 2
delivery:
  delivers: pr
  mode: click
  agent: Developer
  skill: open-pr
";

    /// `skill: <name>` with Otaq's `bare` skill in its namespace: `skill: otaq:validate`.
    fn own_line(bare: &str) -> String {
        format!("skill: {}", skills::namespaced(bare).unwrap())
    }

    fn skills_of(file: &FlowFile) -> Vec<Option<String>> {
        file.flow
            .stages
            .iter()
            .map(|s| s.skill.clone())
            .chain(std::iter::once(file.flow.delivery.skill.clone()))
            .collect()
    }

    #[test]
    fn otaqs_bare_names_get_their_namespace_and_custom_ones_stay() {
        let mut file = yaml::read(OLDER).unwrap();
        assert!(written_before_rename(&file));
        assert!(name_own_skills(&mut file));
        let named = skills_of(&file);
        let named: Vec<Option<&str>> = named.iter().map(Option::as_deref).collect();
        assert_eq!(
            named,
            [
                Some("tuq:product-spec"),
                Some(crate::own_skill!("work-ticket")),
                Some(crate::own_skill!("validate")),
                Some("team-validate"),
                Some(crate::own_skill!("open-pr")),
            ]
        );
        // Once renamed, there is nothing left to rename, and the file reads as written after the rename.
        assert!(!name_own_skills(&mut file));
        assert!(!written_before_rename(&file));
    }

    #[test]
    fn a_file_naming_a_namespace_keeps_its_bare_names_as_the_users() {
        let text = OLDER.replace("skill: work-ticket", &own_line("work-ticket"));
        let file = yaml::read(&text).unwrap();
        assert!(!written_before_rename(&file));
        // A custom flow naming none of Otaq's skills has nothing to rename either way.
        let custom = OLDER
            .replace("skill: product-spec", "skill: my-spec")
            .replace("skill: work-ticket", "skill: my-work")
            .replace("skill: validate", "skill: my-check")
            .replace("skill: open-pr", "skill: my-pr");
        let mut file = yaml::read(&custom).unwrap();
        assert!(!written_before_rename(&file));
        assert!(!name_own_skills(&mut file));
    }

    #[test]
    fn the_text_rename_changes_only_the_skill_lines() {
        let renamed = renamed_text(OLDER).unwrap();
        assert_eq!(
            renamed,
            OLDER
                .replace("skill: product-spec", &own_line("product-spec"))
                .replace("skill: work-ticket", &own_line("work-ticket"))
                .replace("skill: validate\n", &format!("{}\n", own_line("validate")))
                .replace("skill: open-pr", &own_line("open-pr"))
        );
        let mut file = yaml::read(OLDER).unwrap();
        name_own_skills(&mut file);
        assert_eq!(yaml::read(&renamed).unwrap(), file);
        assert_eq!(renamed_text(&renamed), None);
        assert_eq!(
            renamed_text("  - skill: \"validate\"\r\n"),
            Some(format!("  - {}\r\n", own_line("validate")))
        );
    }
}
