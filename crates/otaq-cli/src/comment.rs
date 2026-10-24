//! Ticket comments and reports from the command line (CMT-06, d0109): `otaq comment [KEY] --stdin [--report-file
//! PATH] [--tell-agent]` posts a comment for the next stage's agent (and, with `--tell-agent`, tells the running one
//! to read it, d0123), `otaq comment edit [KEY] <N> --stdin` and `otaq comment delete
//! [KEY] <N>` change one, `otaq comments [KEY]` lists them, and `otaq report [KEY] <N>` prints comment N's report
//! (`--path`: only its file, so an agent can `grep` it without loading it). They work on `$OTAQ_TICKET` unless a key
//! is given; the service judges who may write (CMT-04).
//!
//! A report file is checked here before anything is sent: a missing or unreadable one is refused before posting. The
//! service copies it into its data folder (it is never moved). `otaq report` streams the file to stdout, so a 20 MB
//! report never sits in memory or in one message.
//!
//! # `--json` shapes
//!
//! `comment`, `comment edit` and `comment delete` print the comment with its ticket's key: `{"key", "n", "author":
//! {"kind": "user" | "agent" | "lead", …}, "stage_name"?, "body", "report"?: {"bytes", "lines"}, "posted_at",
//! "edited_at"?}`; `comments` prints `{"key", "comments": [...]}`; `report` prints `{"key", "n", "path", "info":
//! {"bytes", "lines"}, "deleted"?}` (never the report's text).

use std::io::Read;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use otaq_core::board::reports::{ReportSource, open_report};
use otaq_core::board::ticket_comments::{TicketComment, TicketComments};
use otaq_core::data_dir::DataDir;
use otaq_core::domain::TicketKey;
use otaq_core::protocol::{
    ErrorBody, ErrorKind, KeyParams, Reply, Request, TicketCommentEditParams, TicketCommentParams,
    TicketCommentPostParams,
};
use serde_json::{Value, json};

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::read::unexpected;
use crate::service::{self, client_error, internal};
use crate::target;

/// `otaq comment`: post one, or `edit` / `delete` one.
#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct CommentArgs {
    #[command(subcommand)]
    pub command: Option<CommentCommand>,
    /// SHOP-34, or 34 in the current project. Default: $OTAQ_TICKET
    pub key: Option<String>,
    /// Read the comment from stdin (at most 2,000 characters)
    #[arg(long)]
    pub stdin: bool,
    /// Attach a file as the comment's report (at most 20 MB): long output saved straight from a tool
    #[arg(long, value_name = "PATH")]
    pub report_file: Option<PathBuf>,
    /// Also tell the ticket's running agent (or its check) now: once it is idle, it is typed "The user added comment
    /// N. Run `otaq prime` to read it." Nothing without a running session, or from an agent
    #[arg(long)]
    pub tell_agent: bool,
}

#[derive(Debug, Subcommand)]
pub enum CommentCommand {
    /// Change comment N's text, read from stdin (an agent: only its own comments)
    Edit {
        /// [KEY] N: SHOP-34 2, or 2 on $OTAQ_TICKET
        #[arg(value_name = "KEY", num_args = 1..=2, required = true)]
        args: Vec<String>,
        /// Read the new text from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// Delete comment N: later agents won't read it; History keeps what it said (an agent: only its own)
    Delete {
        /// [KEY] N: SHOP-34 2, or 2 on $OTAQ_TICKET
        #[arg(value_name = "KEY", num_args = 1..=2, required = true)]
        args: Vec<String>,
    },
}

fn usage(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Usage, message)
}

fn read_stdin(stdin: bool) -> Result<String, ErrorBody> {
    let mut text = String::new();
    if stdin {
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| internal(format!("can't read stdin: {e}")))?;
    }
    Ok(text)
}

/// `[KEY] N`: the ticket (or `None` for `$OTAQ_TICKET`) and the comment's number.
fn key_and_n(args: Vec<String>) -> Result<(Option<String>, u32), ErrorBody> {
    let mut args = args.into_iter();
    let (key, n) = match (args.next(), args.next()) {
        (Some(n), None) => (None, n),
        (Some(key), Some(n)) => (Some(key), n),
        _ => return Err(usage("name the comment's number: 1, 2, 3…")),
    };
    let n = n
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .ok_or_else(|| usage(format!("{n} isn't a comment's number (1, 2, 3…)")))?;
    Ok((key, n))
}

/// The comment as `--json` prints it: with its ticket's key.
fn with_key(key: &TicketKey, comment: &TicketComment) -> Value {
    let mut value = serde_json::to_value(comment).expect("a comment serializes");
    if let Value::Object(map) = &mut value {
        map.insert("key".into(), json!(key));
    }
    value
}

/// Sends `request` for the ticket `key` (or `$OTAQ_TICKET`) and returns the ticket's key with the reply.
fn send(
    key: Option<String>,
    global: &GlobalArgs,
    request: impl FnOnce(String) -> Request,
) -> Result<(TicketKey, Reply), ErrorBody> {
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    service::runtime()?.block_on(async {
        let client = service::connect(&data, global).await?;
        let key = target::resolve_key(&client, &data, key, global).await?;
        let reply = client
            .request(request(key.to_string()))
            .await
            .map_err(client_error)?;
        Ok((key, reply))
    })
}

fn comment_reply(reply: Reply) -> Result<TicketComment, ErrorBody> {
    match reply {
        Reply::TicketComment(comment) => Ok(*comment),
        other => Err(unexpected(&other)),
    }
}

pub fn comment(args: CommentArgs, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    match args.command {
        Some(CommentCommand::Edit { args, stdin }) => edit(args, stdin, global, out),
        Some(CommentCommand::Delete { args }) => delete(args, global, out),
        None => post(
            args.key,
            args.stdin,
            args.report_file,
            args.tell_agent,
            global,
            out,
        ),
    }
}

/// A `--report-file PATH` as the request sends it: made absolute and checked (it exists, is a file, at most 20 MB)
/// before anything is read or sent, so a missing file posts and moves nothing (CMT-06). The comment, the stage ends
/// and the verdicts share it.
pub fn report_source(report_file: Option<PathBuf>) -> Result<Option<ReportSource>, ErrorBody> {
    let Some(path) = report_file else {
        return Ok(None);
    };
    let path = std::path::absolute(&path).map_err(|e| usage(format!("{}: {e}", path.display())))?;
    open_report(&path).map_err(|e| {
        let mut body = ErrorBody::new(ErrorKind::Refused, e.to_string());
        body.data = e.refusal().map(|code| json!({ "refusal": code }));
        body
    })?;
    Ok(Some(ReportSource::Path(path)))
}

/// `otaq comment [KEY] --stdin [--report-file PATH] [--tell-agent]`.
fn post(
    key: Option<String>,
    stdin: bool,
    report_file: Option<PathBuf>,
    tell_agent: bool,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    // The report is checked before anything is read or sent: a missing file posts nothing (CMT-06).
    let report = report_source(report_file)?;
    let body = read_stdin(stdin)?;
    let (key, reply) = send(key, global, |key| {
        Request::TicketCommentPost(TicketCommentPostParams {
            key,
            body,
            report,
            tell_agent,
        })
    })?;
    let comment = comment_reply(reply)?;
    let with = comment
        .report
        .map(|r| {
            format!(
                " with a report ({}, {} lines)",
                otaq_core::board::reports::size_label(r.bytes),
                otaq_core::board::reports::count_label(r.lines)
            )
        })
        .unwrap_or_default();
    out.print(
        &format!("Posted comment {} on {key}{with}.", comment.n),
        &with_key(&key, &comment),
        Some(&comment.n.to_string()),
    );
    Ok(())
}

/// `otaq comment edit [KEY] <N> --stdin`.
fn edit(
    args: Vec<String>,
    stdin: bool,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let (key, n) = key_and_n(args)?;
    let body = read_stdin(stdin)?;
    let (key, reply) = send(key, global, |key| {
        Request::TicketCommentEdit(TicketCommentEditParams { key, n, body })
    })?;
    let comment = comment_reply(reply)?;
    out.print(
        &format!("Edited comment {n} on {key}."),
        &with_key(&key, &comment),
        Some(&n.to_string()),
    );
    Ok(())
}

/// `otaq comment delete [KEY] <N>`.
fn delete(args: Vec<String>, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let (key, n) = key_and_n(args)?;
    let (key, reply) = send(key, global, |key| {
        Request::TicketCommentDelete(TicketCommentParams { key, n })
    })?;
    let comment = comment_reply(reply)?;
    out.print(
        &format!(
            "Deleted comment {n} on {key}. Later agents won't read it; History keeps what it said."
        ),
        &with_key(&key, &comment),
        Some(&n.to_string()),
    );
    Ok(())
}

/// `otaq comments [KEY]`.
pub fn list(key: Option<String>, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    let (key, reply) = send(key, global, |key| {
        Request::TicketComments(KeyParams { key })
    })?;
    let comments = match reply {
        Reply::TicketComments(comments) => comments,
        other => return Err(unexpected(&other)),
    };
    out.print(&render(&comments), &comments, Some(&key.to_string()));
    Ok(())
}

/// What `otaq comments` prints:
///
/// ```text
/// SHOP-34 · 2 comments
///
/// 1 · Griller · Grill · 2:14 pm
/// Agreed with you: CSV only, admins only.
///
/// 2 · Validator · Validate · round 2 · 3:05 pm · edited
/// Fail: the 10,000 row limit isn't enforced.
/// Report: 48 KB, 2,310 lines · `otaq report SHOP-34 2`
/// ```
pub fn render(comments: &TicketComments) -> String {
    let count = comments.comments.len();
    let mut out = format!(
        "{} \u{b7} {count} comment{}",
        comments.key,
        if count == 1 { "" } else { "s" }
    );
    for comment in &comments.comments {
        let edited = if comment.edited_at.is_some() {
            " \u{b7} edited"
        } else {
            ""
        };
        out.push_str(&format!(
            "\n\n{} \u{b7} {}{edited}\n{}",
            comment.n,
            comment.heading(),
            comment.body
        ));
        if let Some(line) = comment.report_line(&comments.key) {
            out.push('\n');
            out.push_str(&line);
        }
    }
    out
}

/// `otaq report [KEY] <N> [--path]`: the report's text, streamed to stdout, or with `--path` only its file.
pub fn report(
    args: Vec<String>,
    path_only: bool,
    global: &GlobalArgs,
    out: &Output,
) -> Result<(), ErrorBody> {
    let (key, n) = key_and_n(args)?;
    let (_, reply) = send(key, global, |key| {
        Request::TicketReport(TicketCommentParams { key, n })
    })?;
    let report = match reply {
        Reply::TicketReport(report) => *report,
        other => return Err(unexpected(&other)),
    };
    if path_only || !out.is_human() {
        // `--path`, `--json`, `--quiet` and `--silent` never print the text.
        out.print(&report.path, &report, Some(&report.path));
        return Ok(());
    }
    let mut file = std::fs::File::open(&report.path)
        .map_err(|e| internal(format!("can't read the report {}: {e}", report.path)))?;
    std::io::copy(&mut file, &mut std::io::stdout().lock())
        .map_err(|e| internal(format!("can't print the report: {e}")))?;
    Ok(())
}
