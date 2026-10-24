//! The commands an agent uses to hand back control: `otaq ask human --stdin` (ATT-01, d0037), `otaq stage next
//! [KEY] --stdin` (`otaq review submit` is another name for it) and `otaq stage done [KEY] --stdin` (REV-07, AGM-10;
//! d0050, d0108), and an agent check's `otaq check pass|fail --stdin`. They take their text on stdin (PLG-05) and work
//! on `$OTAQ_TICKET` unless a key is given. `otaq review feedback [KEY]` (REV-05, d0017) prints what the
//! latest Send back carried; it only reads.
//!
//! `ask human` takes 2–4 answer options (`--option "Postmark" --option "SES" --recommended 1`, d0061); the
//! service checks them. Each option may carry a note, its pros and cons: `--note "…"` right after its `--option`
//! (ATT-17), stored whole. An epic child's `ask human` goes to its epic's live lead first (LEAD-06); the lead answers
//! it with `otaq answer <QUESTION-ID> --stdin` or passes it on with `otaq ask human --pass-on <QUESTION-ID>
//! [--option …] [--recommended N] --stdin` (its reason on stdin), and `otaq questions` in its session lists the
//! questions waiting for it.
//!
//! # `--json` shapes
//!
//! They print the ticket afterwards (`Ticket`); after `ask human` its `question` holds the question's `id`,
//! `text`, `options` and `recommended` (1-based). A lead's `answer` and `ask human --pass-on` print the question
//! (`otaq_core::epic_lead::questions::LeadQuestion`: `{"id", "child", "epic", "lead", "question", "options",
//! "recommended", "state": "answered" | "passed_on", "reason"?, "answer"?, …}`), and its `questions` a list of them.
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
    /// `--pass-on <QUESTION-ID>`: an epic lead passes a child's question on (LEAD-06).
    pub pass_on: Option<String>,
}

pub fn ask(args: AskArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    let AskArgs {
        target: target_name,
        stdin,
        options,
        notes,
        recommended,
        pass_on,
    } = args;
    let to_human = target_name.eq_ignore_ascii_case("human");
    let notes = check_notes(to_human, &options, pass_on.is_some(), notes)?;
    if !to_human && (!options.is_empty() || recommended.is_some()) {
        return Err(usage("Options are only for questions to the human"));
    }
    if let Some(id) = pass_on {
        if !to_human {
            return Err(usage(format!(
                "--pass-on passes a question on to the human: `{cmd} ask human --pass-on {id} --stdin`"
            )));
        }
        if !stdin {
            return Err(usage(format!(
                "give your reason on stdin: `{cmd} ask human --pass-on {id} --stdin`"
            )));
        }
        let choices = Choices {
            options,
            notes,
            recommended,
        };
        return pass_on_question(id, choices, global, out);
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
    pass_on: bool,
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
    if options.is_empty() && !pass_on {
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

/// Where `ask human` goes: a ticket's agent asks, a Project agent (or lead) asks, or a lead passes a child's question
/// on (`text` is then its reason).
enum Route {
    Ticket(String),
    ProjectAgent,
    PassOn(String),
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
        Route::PassOn(id) => Request::LeadPassOn(otaq_core::protocol::PassOnParams {
            id,
            options,
            notes,
            recommended,
            reason: text,
        }),
    }
}

/// `otaq ask human --pass-on <QUESTION-ID> [--option … [--note …]] [--recommended N] --stdin` (LEAD-06): the epic's
/// lead passes a child's question on to the user, its reason on stdin. Without options the child's options and notes
/// stay.
fn pass_on_question(
    id: String,
    choices: Choices,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let reason = read_stdin()?;
    if reason.is_empty() {
        return Err(usage("your reason on stdin is empty"));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let question = match client
            .request(human_request(Route::PassOn(id), reason, choices))
            .await
            .map_err(client_error)?
        {
            Reply::LeadQuestion(q) => *q,
            other => return Err(unexpected(&other)),
        };
        out.print(
            &otaq_core::epic_lead::questions::passed_on_message(&question),
            &question,
            Some(&question.id),
        );
        Ok(())
    })
}

/// `otaq ask <KEY> --stdin` (MSG-01): a question to a linked ticket's agent; returns at once with its ID.
/// A Project agent's session: the Project agent (or epic lead, d0083) role and no ticket (d0068).
fn in_project_agent() -> bool {
    crate::service::ticket_env().is_none()
        && std::env::var(naming::role_env_var())
            .is_ok_and(|r| r == naming::ROLE_PROJECT_AGENT || r == naming::ROLE_EPIC_LEAD)
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
            // An epic lead answered a child's question (LEAD-06).
            Reply::LeadQuestion(q) => {
                out.print(
                    &otaq_core::epic_lead::questions::answered_message(&q),
                    &q,
                    Some(&q.id),
                );
                return Ok(());
            }
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

/// `otaq questions [KEY]`: the questions waiting for this ticket's answer; in an epic lead's session, the children's
/// questions waiting for the lead (LEAD-06).
pub fn questions(key: Option<String>, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let cmd = naming::COMMAND_NAME;
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        if key.is_none() && in_epic_lead() {
            let waiting = match client
                .request(Request::LeadQuestions)
                .await
                .map_err(client_error)?
            {
                Reply::LeadQuestions(q) => q,
                other => return Err(unexpected(&other)),
            };
            out.print(&render_lead_questions(&waiting), &waiting, None);
            return Ok(());
        }
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

/// An epic lead's session: the epic lead role and no ticket (d0083).
fn in_epic_lead() -> bool {
    crate::service::ticket_env().is_none()
        && std::env::var(naming::role_env_var()).is_ok_and(|r| r == naming::ROLE_EPIC_LEAD)
}

/// ```text
/// 0199… · from SHOP-40.2 Wishlist button
///   Can guests save items without logging in?
///   options: 1. Logged-in users only · 2. Guests too, kept in the browser
///   answer: `otaq answer 0199… --stdin` · pass on: `otaq ask human --pass-on 0199… --stdin`
/// ```
///
/// When an option has a note (ATT-17), the options are listed one per line, each note whole under its label:
///
/// ```text
///   options:
///     1. Markdown files
///        Opens in any editor; one file per note, no attachments.
///     2. One ZIP
/// ```
fn render_lead_questions(waiting: &[otaq_core::epic_lead::questions::LeadQuestion]) -> String {
    let cmd = naming::COMMAND_NAME;
    if waiting.is_empty() {
        return "No questions wait for you.".to_owned();
    }
    waiting
        .iter()
        .map(|q| {
            let options = if q.options.is_empty() {
                String::new()
            } else if q.notes.iter().any(Option::is_some) {
                let listed: Vec<String> = q
                    .options
                    .iter()
                    .enumerate()
                    .map(|(n, o)| match q.notes.get(n).cloned().flatten() {
                        Some(note) => format!(
                            "\n    {}. {o}\n       {}",
                            n + 1,
                            note.replace('\n', "\n       ")
                        ),
                        None => format!("\n    {}. {o}", n + 1),
                    })
                    .collect();
                format!("\n  options:{}", listed.concat())
            } else {
                let listed: Vec<String> = q
                    .options
                    .iter()
                    .enumerate()
                    .map(|(n, o)| format!("{}. {o}", n + 1))
                    .collect();
                format!("\n  options: {}", listed.join(" \u{b7} "))
            };
            format!(
                "{id} \u{b7} from {} {}\n  {}{options}\n  answer: `{cmd} answer {id} --stdin` \u{b7} pass on: \
                 `{cmd} ask human --pass-on {id} --stdin`",
                q.child,
                q.child_title,
                q.question.replace('\n', "\n  "),
                id = q.id,
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
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

/// `otaq agent name "<name>" [--rename]` (PAG-04): a Project agent names itself; refused anywhere else, a
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

    /// LEAD-06: `otaq questions` in a lead's session lists its children's questions with both ways out.
    #[test]
    fn a_leads_waiting_questions() {
        use otaq_core::epic_lead::questions::{LeadQuestion, LeadQuestionState};
        let cmd = naming::COMMAND_NAME;
        assert_eq!(render_lead_questions(&[]), "No questions wait for you.");
        let q = LeadQuestion {
            id: "q7".into(),
            child: "SHOP-40.2".parse().unwrap(),
            child_title: "Wishlist button".into(),
            epic: "SHOP-40".parse().unwrap(),
            lead: "Wishlist lead".into(),
            lead_agent: None,
            question: "Can guests save items?".into(),
            options: vec!["Logged-in users only".into(), "Guests too".into()],
            recommended: None,
            notes: Vec::new(),
            state: LeadQuestionState::WithLead,
            reason: None,
            answer: None,
            asked_at: 0,
            lead_at: None,
            answered_at: None,
        };
        assert_eq!(
            render_lead_questions(&[q]),
            format!(
                "q7 \u{b7} from SHOP-40.2 Wishlist button\n  Can guests save items?\n  options: 1. Logged-in users \
                 only \u{b7} 2. Guests too\n  answer: `{cmd} answer q7 --stdin` \u{b7} pass on: `{cmd} ask human \
                 --pass-on q7 --stdin`"
            )
        );
    }

    /// ATT-17: with notes, each option's note is listed whole under it; options without one have none.
    #[test]
    fn a_leads_waiting_questions_show_the_notes() {
        use otaq_core::epic_lead::questions::{LeadQuestion, LeadQuestionState};
        let long = "y".repeat(600);
        let q = LeadQuestion {
            id: "q8".into(),
            child: "NOTE-1".parse().unwrap(),
            child_title: "Add export".into(),
            epic: "NOTE-10".parse().unwrap(),
            lead: "Sharing lead".into(),
            lead_agent: None,
            question: "Which export format?".into(),
            options: vec!["Markdown files".into(), "One ZIP".into(), "PDF".into()],
            recommended: Some(1),
            notes: vec![
                Some("Opens in any editor; one file per note, no attachments.".into()),
                None,
                Some(long.clone()),
            ],
            state: LeadQuestionState::WithLead,
            reason: None,
            answer: None,
            asked_at: 0,
            lead_at: None,
            answered_at: None,
        };
        let text = render_lead_questions(&[q]);
        assert!(
            text.contains(&format!(
                "  Which export format?\n  options:\n    1. Markdown files\n       Opens in any editor; one file per \
                 note, no attachments.\n    2. One ZIP\n    3. PDF\n       {long}\n  answer: "
            )),
            "{text}"
        );
    }

    /// ATT-17: every route carries the notes; a pass-on without options sends none, so the child's stay.
    #[test]
    fn the_three_routes_carry_the_notes() {
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
        let Request::LeadPassOn(p) = human_request(
            Route::PassOn("q8".into()),
            "PAY-1 says so.".into(),
            choices(),
        ) else {
            panic!("a lead passes on with lead.pass_on")
        };
        assert_eq!((p.reason.as_str(), &p.notes), ("PAY-1 says so.", &notes));
        let keep = Choices {
            options: Vec::new(),
            notes: Vec::new(),
            recommended: None,
        };
        let Request::LeadPassOn(p) = human_request(Route::PassOn("q8".into()), "r".into(), keep)
        else {
            panic!("lead.pass_on")
        };
        let sent = serde_json::to_value(&p).unwrap();
        assert!(
            sent.get("options").is_none() && sent.get("notes").is_none(),
            "{sent}"
        );
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
                false,
                Err(crate::cli::NOTE_AFTER_OPTION.into())
            )),
            "--note is only for questions to the human: a question to a ticket's agent is free text"
        );
        assert!(
            check_notes(false, &opts, false, one.clone()).is_err(),
            "ask <KEY> with options too"
        );
        assert_eq!(
            message(check_notes(
                true,
                &[],
                false,
                Err(crate::cli::NOTE_AFTER_OPTION.into())
            )),
            "A free-text question takes no --note: a --note describes the --option before it"
        );
        assert_eq!(
            message(check_notes(
                true,
                &[],
                true,
                Err(crate::cli::NOTE_AFTER_OPTION.into())
            )),
            "--note must follow the --option it describes"
        );
        assert_eq!(
            message(check_notes(
                true,
                &opts,
                false,
                Err("Give at most one --note per --option: \"A\" has two".into())
            )),
            "Give at most one --note per --option: \"A\" has two"
        );
        assert_eq!(
            check_notes(true, &opts, false, one.clone()),
            Ok(vec![Some("a".to_owned()), None])
        );
        assert_eq!(
            check_notes(true, &opts, false, Ok(vec![None, None])),
            Ok(vec![])
        );
        assert_eq!(
            check_notes(false, &[], false, Ok(vec![])),
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
