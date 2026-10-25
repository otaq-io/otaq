//! The commands an agent uses to hand back control: `otaq ask human --stdin` (ATT-01, d0037), `otaq stage next
//! [KEY] --stdin` (`otaq review submit` is another name for it) and `otaq stage done [KEY] --stdin` (REV-07, AGM-10;
//! d0050, d0108), and an agent check's `otaq check pass|fail --stdin`. They take their text on stdin (PLG-05) and work
//! on `$OTAQ_TICKET` unless a key is given. `otaq review feedback [KEY]` (REV-05, d0017) prints what the
//! latest Send back carried; it only reads.
//!
//! `ask human` takes 2–4 answer options (`--option "Postmark" --option "SES" --recommended 1`, d0061); the
//! service checks them. Each option may carry a note, its pros and cons: `--note "…"` right after its `--option`
//! (ATT-17), stored whole.
//!
//! # `--json` shapes
//!
//! They print the ticket afterwards (`Ticket`); after `ask human` its `question` holds the question's `id`,
//! `text`, `options` and `recommended` (1-based).
//!
//! `review feedback --json` prints `{"key", "round", "note"?, "comments": [{"file", "line", "side", "body"}], "pr"?}`;
//! `pr` (a ticket with a PR) is `{"number", "comments": [{"file"?, "line"?, "author", "text"}], "note"?}`, the
//! unresolved PR comments fetched while the command runs (T-0148)
//! (`round` 0 and no comments when the ticket was never sent back; `side` is `old` for a removed line).

use std::io::Read;

use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::protocol::{
    AnswerParams, AskHumanParams, AskTicketParams, ErrorBody, ErrorKind, KeyParams, Reply, Request,
    StageEndParams,
};

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

fn usage(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Usage, message)
}

fn read_stdin() -> Result<String, ErrorBody> {
    let mut text = String::new();
    std::io::stdin()
        .read_to_string(&mut text)
        .map_err(|e| internal(format!("can't read stdin: {e}")))?;
    Ok(text.trim().to_owned())
}

pub struct AskArgs {
    pub target: String,
    pub stdin: bool,
    pub options: Vec<String>,
    /// Each option's note, paired by position ([`crate::cli::option_notes`]), or why the pairing is refused.
    pub notes: Result<Vec<Option<String>>, String>,
    pub recommended: Option<u32>,
}

pub fn ask(args: AskArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    let AskArgs {
        target: target_name,
        stdin,
        options,
        notes,
        recommended,
    } = args;
    let to_human = target_name.eq_ignore_ascii_case("human");
    let notes = check_notes(to_human, &options, notes)?;
    if !to_human && (!options.is_empty() || recommended.is_some()) {
        return Err(usage("Options are only for questions to the human"));
    }
    if !stdin {
        return Err(usage(format!(
            "give the question on stdin: `{cmd} ask {target_name} --stdin`"
        )));
    }
    let choices = Choices {
        options,
        notes,
        recommended,
    };
    if in_project_agent() {
        return ask_as_project_agent(&target_name, to_human, choices, global, out);
    }
    if !to_human {
        return ask_ticket(&target_name, global, out);
    }
    let question = read_stdin()?;
    if question.is_empty() {
        return Err(usage("the question on stdin is empty"));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global).await?;
        let ticket = match client
            .request(human_request(
                Route::Ticket(key.to_string()),
                question,
                choices,
            ))
            .await
            .map_err(client_error)?
        {
            Reply::Ticket(t) => *t,
            // A helper's question goes to the agent in charge first (d0133).
            Reply::Helper(reply) => {
                out.print(&reply.text, &reply, Some(&reply.helper.label()));
                return Ok(());
            }
            other => return Err(unexpected(&other)),
        };
        out.print(
            &format!(
                "{} asks you. The question is on the board; end your turn and wait for the answer.",
                ticket.key
            ),
            &ticket,
            Some(&ticket.key.to_string()),
        );
        Ok(())
    })
}

/// The notes of an `ask` (ATT-17), refused naming the rule: on `ask <KEY>`, on a free-text question, out of place
/// ([`crate::cli::option_notes`]). Empty when no option has one.
fn check_notes(
    to_human: bool,
    options: &[String],
    notes: Result<Vec<Option<String>>, String>,
) -> Result<Vec<Option<String>>, ErrorBody> {
    let any = notes
        .as_ref()
        .map_or(true, |n| n.iter().any(Option::is_some));
    if !any {
        return Ok(Vec::new());
    }
    if !to_human {
        return Err(usage(
            "--note is only for questions to the human: a question to a ticket's agent is free text",
        ));
    }
    if options.is_empty() {
        return Err(usage(
            "A free-text question takes no --note: a --note describes the --option before it",
        ));
    }
    notes.map_err(usage)
}

/// What the user picks from: the options, each one's note, the recommended one.
struct Choices {
    options: Vec<String>,
    notes: Vec<Option<String>>,
    recommended: Option<u32>,
}

/// Where `ask human` goes: a ticket's agent asks, or a Project agent asks.
enum Route {
    Ticket(String),
    ProjectAgent,
}

/// The request of each route; every one carries the options' notes (ATT-17).
fn human_request(route: Route, text: String, choices: Choices) -> Request {
    let Choices {
        options,
        notes,
        recommended,
    } = choices;
    match route {
        Route::Ticket(key) => Request::TicketAskHuman(AskHumanParams {
            key,
            question: text,
            options,
            notes,
            recommended,
        }),
        Route::ProjectAgent => Request::AgentAskHuman(otaq_core::protocol::AgentAskHumanParams {
            question: text,
            options,
            notes,
            recommended,
        }),
    }
}

/// `otaq ask <KEY> --stdin` (MSG-01): a question to a linked ticket's agent; returns at once with its ID.
/// A Project agent's session: the Project agent role and no ticket (d0068).
fn in_project_agent() -> bool {
    crate::service::ticket_env().is_none()
        && std::env::var(naming::role_env_var()).is_ok_and(|r| r == naming::ROLE_PROJECT_AGENT)
}

/// `ask human` / `ask <KEY>` from a Project agent: it has no ticket; the service knows it by its session.
fn ask_as_project_agent(
    target: &str,
    to_human: bool,
    choices: Choices,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let question = read_stdin()?;
    if question.is_empty() {
        return Err(usage("the question on stdin is empty"));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        if to_human {
            let agent = match client
                .request(human_request(Route::ProjectAgent, question, choices))
                .await
                .map_err(client_error)?
            {
                Reply::ProjectAgent(a) => *a,
                other => return Err(unexpected(&other)),
            };
            out.print(
                "The user is asked. End your turn and wait for the answer.",
                &agent,
                Some(&agent.id),
            );
            return Ok(());
        }
        let target = target::resolve_key(&client, &data, Some(target.to_owned()), global).await?;
        let message = match client
            .request(Request::AgentAsk(otaq_core::protocol::AgentAskParams {
                target: target.to_string(),
                question,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Message(m) => *m,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &format!(
                "Asked {} (question {}). Carry on: the answer is typed into your session when it arrives.",
                message.to, message.id
            ),
            &message,
            Some(&message.id),
        );
        Ok(())
    })
}

fn ask_ticket(target: &str, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let question = read_stdin()?;
    if question.is_empty() {
        return Err(usage("the question on stdin is empty"));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global).await?;
        let target = target::resolve_key(&client, &data, Some(target.to_owned()), global).await?;
        let message = match client
            .request(Request::TicketAsk(AskTicketParams {
                key: key.to_string(),
                target: target.to_string(),
                question,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Message(m) => *m,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &format!(
                "Asked {} (question {}). Carry on: the answer is typed into your session when it arrives.",
                message.to, message.id
            ),
            &message,
            Some(&message.id),
        );
        Ok(())
    })
}

/// `otaq answer <ID> --stdin`.
pub fn answer(id: String, stdin: bool, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    if !stdin {
        return Err(usage(format!(
            "give the answer on stdin: `{cmd} answer {id} --stdin`"
        )));
    }
    let answer = read_stdin()?;
    if answer.is_empty() {
        return Err(usage("the answer on stdin is empty"));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let message = match client
            .request(Request::MessageAnswer(AnswerParams { id, answer }))
            .await
            .map_err(client_error)?
        {
            Reply::Message(m) => *m,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &format!(
                "Answered {}'s question {}. It is typed into {}'s session when its agent is idle.",
                message.asker(),
                message.id,
                message.asker()
            ),
            &message,
            Some(&message.id),
        );
        Ok(())
    })
}

/// `otaq questions [KEY]`: the questions waiting for this ticket's answer.
pub fn questions(key: Option<String>, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, key, global).await?;
        let messages = match client
            .request(Request::TicketQuestions(KeyParams {
                key: key.to_string(),
            }))
            .await
            .map_err(client_error)?
        {
            Reply::Messages(m) => m,
            other => return Err(unexpected(&other)),
        };
        let text = if messages.is_empty() {
            format!("No questions wait for {key}.")
        } else {
            messages
                .iter()
                .map(|m| {
                    format!(
                        "{} · from {}\n  {}\n  answer: `{cmd} answer {} --stdin`",
                        m.id,
                        m.asker(),
                        m.question.replace('\n', "\n  "),
                        m.id
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        out.print(&text, &messages, None);
        Ok(())
    })
}

/// What a stage end is given: `--stdin`, `--report-file` and the `--manual-check` items (d0125).
pub struct StageEnd {
    pub stdin: bool,
    pub report_file: Option<std::path::PathBuf>,
    pub manual_checks: Vec<String>,
}

/// `otaq stage next [KEY] --stdin [--report-file PATH] [--manual-check TEXT]…` (and `otaq review submit`): the agent
/// ends its stage with a summary, its comment (CMT-05), and what it couldn't check itself (d0125); the answer names
/// the stage it finished and what comes next (AGM-10).
pub fn stage_next(
    key: Option<String>,
    end: StageEnd,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    end_stage(key, end, global, out, Request::TicketStageNext)
}

/// `otaq stage done [KEY] --stdin [--report-file PATH] [--manual-check TEXT]…`: nothing to deliver, the ticket is
/// Closed · nothing to merge with the reason as the agent's comment (AGM-10, CMT-05).
pub fn stage_done(
    key: Option<String>,
    end: StageEnd,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    end_stage(key, end, global, out, Request::TicketStageDone)
}

fn end_stage(
    key: Option<String>,
    StageEnd {
        stdin,
        report_file,
        manual_checks,
    }: StageEnd,
    global: &GlobalArgs,
    out: &Output,
    request: fn(StageEndParams) -> Request,
) -> Result<(), ErrorBody> {
    // A missing report refuses the stage end before anything is read or sent (CMT-06).
    let report_file = crate::comment::report_source(report_file)?;
    let summary = if stdin { read_stdin()? } else { String::new() };
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, key, global).await?;
        let ended = match client
            .request(request(StageEndParams {
                key: key.to_string(),
                summary,
                report_file,
                manual_checks,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::StageEnded(ended) => *ended,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &ended.text,
            &ended.ticket,
            Some(&ended.ticket.key.to_string()),
        );
        Ok(())
    })
}

pub fn feedback(key: Option<String>, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, key, global).await?;
        let feedback = match client
            .request(Request::ReviewFeedback(KeyParams {
                key: key.to_string(),
            }))
            .await
            .map_err(client_error)?
        {
            Reply::ReviewFeedback(f) => f,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &render_feedback(&feedback),
            &feedback,
            Some(&feedback.key.to_string()),
        );
        Ok(())
    })
}

/// The unresolved PR comments, after the in-app ones (REV-05):
///
/// ```text
/// PR #481 · unresolved comments
///
/// lib/totals.ts
///   12 (anna): Round per line, not per order.
/// ```
fn render_pr_comments(pr: &otaq_core::pr::comments::PrFeedback) -> String {
    let mut out = format!("\n\nPR #{} · unresolved comments\n", pr.number);
    if let Some(note) = &pr.note {
        out.push_str(&format!("\n{note}\n"));
        return out;
    }
    if pr.comments.is_empty() {
        out.push_str("\nNone.\n");
        return out;
    }
    let mut file: Option<Option<&str>> = None;
    for c in &pr.comments {
        if file != Some(c.file.as_deref()) {
            out.push_str(&format!(
                "\n{}\n",
                c.file.as_deref().unwrap_or("(the whole PR)")
            ));
            file = Some(c.file.as_deref());
        }
        let at = c.line.map(|l| format!("{l} ")).unwrap_or_default();
        let mut lines = c.text.lines();
        out.push_str(&format!(
            "  {at}({}): {}\n",
            c.author,
            lines.next().unwrap_or_default()
        ));
        for more in lines {
            out.push_str(&format!("    {more}\n"));
        }
    }
    out
}

/// ```text
/// SHOP-27 · feedback from round 2
///
/// lib/search.ts
///   12: Use the page size from settings.
///   40 (removed line): Why was this removed?
/// ```
pub fn render_feedback(f: &otaq_core::board::comments::ReviewFeedback) -> String {
    let mut out = render_review_feedback(f);
    if let Some(pr) = &f.pr {
        out.push_str(&render_pr_comments(pr));
    }
    out.trim_end().to_owned()
}

/// The in-app part: the latest Send back's line comments (its note is a ticket comment, in `prime`).
fn render_review_feedback(f: &otaq_core::board::comments::ReviewFeedback) -> String {
    use otaq_core::board::comments::Side;
    if f.round == 0 {
        return format!("{} has no review feedback: it was never sent back.", f.key);
    }
    let mut out = format!("{} · feedback from round {}\n", f.key, f.round);
    if f.comments.is_empty() {
        out.push_str("\nNo line comments.\n");
    }
    let mut file: Option<&str> = None;
    for c in &f.comments {
        if file != Some(c.file.as_str()) {
            out.push_str(&format!("\n{}\n", c.file));
            file = Some(&c.file);
        }
        let side = if c.side == Side::Old {
            " (removed line)"
        } else {
            ""
        };
        let mut lines = c.body.lines();
        out.push_str(&format!(
            "  {}{side}: {}\n",
            c.line,
            lines.next().unwrap_or_default()
        ));
        for more in lines {
            out.push_str(&format!("    {more}\n"));
        }
    }
    out.trim_end().to_owned()
}

/// `otaq check pass|fail --stdin [--report-file PATH] [--manual-check TEXT]…` (VAL-03, CLI-05, AGM-10): an agent
/// check's verdict on its ticket (`$OTAQ_TICKET`), its text the checker's comment, the file its report (CMT-05,
/// CMT-06) and what it couldn't check itself (d0125). Only the check's session may give one.
pub fn check(
    command: crate::cli::CheckCommand,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let (pass, stdin, report_file, manual_checks) = match command {
        crate::cli::CheckCommand::Pass {
            stdin,
            report_file,
            manual_check,
        } => (true, stdin, report_file, manual_check),
        crate::cli::CheckCommand::Fail {
            stdin,
            report_file,
            manual_check,
        } => (false, stdin, report_file, manual_check),
    };
    let report_file = crate::comment::report_source(report_file)?;
    let report = if stdin { read_stdin()? } else { String::new() };
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, None, global).await?;
        // The check judged, by name, before the verdict moves the ticket on (Q344).
        let check = check_being_judged(&client, &key.to_string()).await;
        let ticket = match client
            .request(Request::ValidationVerdict(
                otaq_core::protocol::VerdictParams {
                    key: key.to_string(),
                    pass,
                    report,
                    report_file,
                    manual_checks,
                },
            ))
            .await
            .map_err(client_error)?
        {
            Reply::Ticket(t) => *t,
            other => return Err(unexpected(&other)),
        };
        let verdict = if pass { "passed" } else { "failed" };
        let text = format!("{}: {check} {verdict}. End your turn.", ticket.key);
        out.print(&text, &ticket, Some(&ticket.key.to_string()));
        Ok(())
    })
}

/// The name of the agent check the ticket is in ("Validate", "Security"), from its project's flow; "The check" when
/// it can't be read.
async fn check_being_judged(client: &otaq_core::client::Client, key: &str) -> String {
    let Ok(ticket) = crate::read::get_ticket(client, key).await else {
        return "The check".to_owned();
    };
    let flow = match client
        .request(Request::FlowCurrent(otaq_core::protocol::ProjectParams {
            project: ticket.key.project.to_string(),
        }))
        .await
    {
        Ok(Reply::FlowVersion(version)) => version.file().ok().map(|f| f.flow),
        _ => None,
    };
    flow.map_or_else(
        || "The check".to_owned(),
        |flow| otaq_core::board::validation::check_name(&flow, &ticket.place),
    )
}

/// `otaq agent name "<name>" [--rename]` (PAG-04): a Project agent names its conversation; refused anywhere else, a
/// second time without `--rename`, and over the user's own name.
pub fn name(name: &str, rename: bool, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let agent = match client
            .request(Request::AgentName(otaq_core::protocol::AgentNameParams {
                name: name.to_owned(),
                rename,
            }))
            .await
            .map_err(client_error)?
        {
            Reply::ProjectAgent(a) => *a,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &format!("Named \"{}\".", agent.name),
            &agent,
            Some(&agent.id),
        );
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use otaq_core::board::comments::ReviewFeedback;
    use otaq_core::pr::comments::{PrComment, PrFeedback};

    fn feedback(pr: Option<PrFeedback>) -> ReviewFeedback {
        ReviewFeedback {
            key: "SHOP-26".parse().unwrap(),
            round: 0,
            comments: Vec::new(),
            pr,
        }
    }

    /// ATT-17: every route carries the notes.
    #[test]
    fn the_routes_carry_the_notes() {
        let notes = vec![Some("Opens in any editor.".to_owned()), None];
        let choices = || Choices {
            options: vec!["Markdown files".into(), "One ZIP".into()],
            notes: notes.clone(),
            recommended: Some(1),
        };
        let Request::TicketAskHuman(p) = human_request(
            Route::Ticket("NOTE-1".into()),
            "Which format?".into(),
            choices(),
        ) else {
            panic!("a ticket's agent asks with ticket.ask_human")
        };
        assert_eq!((p.key.as_str(), &p.notes), ("NOTE-1", &notes));
        let Request::AgentAskHuman(p) =
            human_request(Route::ProjectAgent, "Which format?".into(), choices())
        else {
            panic!("a Project agent asks with agent.ask_human")
        };
        assert_eq!(p.notes, notes);
    }

    /// ATT-17: the refusals of `--note`, each naming its rule.
    #[test]
    fn notes_out_of_place_are_refused() {
        let opts = vec!["A".to_owned(), "B".to_owned()];
        let one = Ok(vec![Some("a".to_owned()), None]);
        let message = |r: Result<Vec<Option<String>>, ErrorBody>| r.unwrap_err().message;
        assert_eq!(
            message(check_notes(
                false,
                &[],
                Err(crate::cli::NOTE_AFTER_OPTION.into())
            )),
            "--note is only for questions to the human: a question to a ticket's agent is free text"
        );
        assert!(
            check_notes(false, &opts, one.clone()).is_err(),
            "ask <KEY> with options too"
        );
        assert_eq!(
            message(check_notes(
                true,
                &[],
                Err(crate::cli::NOTE_AFTER_OPTION.into())
            )),
            "A free-text question takes no --note: a --note describes the --option before it"
        );
        assert_eq!(
            message(check_notes(
                true,
                &opts,
                Err(crate::cli::NOTE_AFTER_OPTION.into())
            )),
            "--note must follow the --option it describes"
        );
        assert_eq!(
            message(check_notes(
                true,
                &opts,
                Err("Give at most one --note per --option: \"A\" has two".into())
            )),
            "Give at most one --note per --option: \"A\" has two"
        );
        assert_eq!(
            check_notes(true, &opts, one.clone()),
            Ok(vec![Some("a".to_owned()), None])
        );
        assert_eq!(check_notes(true, &opts, Ok(vec![None, None])), Ok(vec![]));
        assert_eq!(
            check_notes(false, &[], Ok(vec![])),
            Ok(vec![]),
            "ask <KEY> without notes"
        );
    }

    /// REV-05: the unresolved PR comments come after the in-app feedback, by file.
    #[test]
    fn pr_comments_follow_the_in_app_feedback() {
        let text = render_feedback(&feedback(Some(PrFeedback {
            number: 481,
            comments: vec![
                PrComment {
                    file: Some("lib/totals.ts".into()),
                    line: Some(12),
                    author: "anna".into(),
                    text: "Round per line.\nNot per order.".into(),
                },
                PrComment {
                    file: None,
                    line: None,
                    author: "bo".into(),
                    text: "Add a changelog line.".into(),
                },
            ],
            note: None,
        })));
        assert_eq!(
            text,
            "SHOP-26 has no review feedback: it was never sent back.\n\nPR #481 · unresolved comments\n\n\
             lib/totals.ts\n  12 (anna): Round per line.\n    Not per order.\n\n(the whole PR)\n  (bo): Add a \
             changelog line."
        );
        let text = render_feedback(&feedback(Some(PrFeedback {
            number: 481,
            comments: Vec::new(),
            note: Some(
                "PR #481 comments weren't fetched: `gh` isn't signed in (`gh auth login`)".into(),
            ),
        })));
        assert!(
            text.ends_with("`gh` isn't signed in (`gh auth login`)"),
            "{text}"
        );
        assert!(!render_feedback(&feedback(None)).contains("PR #"));
    }
}
