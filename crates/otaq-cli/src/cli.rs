//! Argument parsing. The command set and the global flags follow beads (`bd`) conventions (d0032); the CLI is a
//! thin client of the background service (d0031). Commands appear here only once they are built.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{ArgMatches, Args, CommandFactory, FromArgMatches, Parser, Subcommand};
use otaq_core::domain::Priority;
use otaq_core::naming;

use crate::output::Mode;

#[derive(Debug, Parser)]
#[command(
    name = naming::COMMAND_NAME,
    version,
    about = "Kanban board that drives coding agents: Claude Code, Codex, Copilot and OpenCode. Without a command, opens the app.",
    disable_help_subcommand = true
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,

    #[command(subcommand)]
    pub command: Option<Command>,
}

/// The stage choices of `create` and `update` (AGM-11, d0108): a stage with options (by name or id) and one of its
/// options.
#[derive(Debug, Default, Args)]
pub struct ChoiceFlags {
    /// A stage choice, repeatable: "Dev=Quick developer", "QA=required", or "QA=skip" where the flow lets QA be skipped
    #[arg(long = "stage", value_name = "STAGE=AGENT|skip")]
    pub stages: Vec<String>,
    /// Why you chose (agents only: "typo fix")
    #[arg(long, value_name = "TEXT")]
    pub reason: Option<String>,
    /// The user named these choices themselves (agents only): they show as set by you
    #[arg(long)]
    pub by_user: bool,
}

/// Flags accepted by every command (d0032).
#[derive(Debug, Default, Args)]
pub struct GlobalArgs {
    /// Print machine-readable JSON; errors become {"error": {...}}
    #[arg(long, global = true)]
    pub json: bool,

    /// Print errors only
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Print only the ID of what was created or changed
    #[arg(long, global = true)]
    pub silent: bool,

    /// The project to work in, by key (SHOP). Bare `prime --project`: a Project agent's context
    #[arg(
        short = 'P',
        long,
        global = true,
        value_name = "KEY",
        num_args = 0..=1,
        default_missing_value = ""
    )]
    pub project: Option<String>,

    /// Who is acting, recorded on changes (for example agent:SHOP-18)
    #[arg(long, global = true, value_name = "ACTOR")]
    pub actor: Option<String>,
}

impl GlobalArgs {
    /// The project named with `-P KEY`; a bare `--project` names none.
    pub fn project_key(&self) -> Option<&str> {
        self.project.as_deref().filter(|p| !p.trim().is_empty())
    }

    /// A bare `--project` (`otaq prime --project`, d0068).
    pub fn bare_project(&self) -> bool {
        self.project.as_deref().is_some_and(|p| p.trim().is_empty())
    }

    /// The output mode. When several flags are given, `--json` wins, then `--silent`, then `--quiet`.
    pub fn mode(&self) -> Mode {
        if self.json {
            Mode::Json
        } else if self.silent {
            Mode::Silent
        } else if self.quiet {
            Mode::Quiet
        } else {
            Mode::Human
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Start, stop or check the background service
    #[command(subcommand)]
    Daemon(DaemonCommand),
    /// Open a ticket's live agent session in this terminal (detach with Ctrl+])
    Attach {
        /// The ticket: SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
        /// The agent running this stage of the ticket's flow (its name or id): `--stage security` while it checks
        #[arg(long, value_name = "STAGE")]
        stage: Option<String>,
    },
    /// The current ticket's context for its agent: details, parents, rules and your Agent instructions
    Prime {
        /// SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
        /// What a validator works from: the ticket, the summary, the diff, earlier findings, linked tickets
        #[arg(long)]
        validate: bool,
        /// An epic lead's context: the epic, every child, overlaps, open questions, notes, suggestions. Bare
        /// `--epic`: $OTAQ_EPIC
        #[arg(
            long,
            value_name = "EPIC",
            num_args = 0..=1,
            default_missing_value = "",
            conflicts_with_all = ["key", "validate"]
        )]
        epic: Option<String>,
    },
    /// Show a ticket: details, description, dependencies (default: $OTAQ_TICKET)
    #[command(visible_alias = "view")]
    Show {
        /// SHOP-18, or 18 in the current project
        key: Option<String>,
        /// Also show the ticket's history
        #[arg(long)]
        long: bool,
        /// For an epic: list its tickets with their statuses
        #[arg(long)]
        children: bool,
        /// Print the ticket's terminal as the user sees it, as plain text (read-only)
        #[arg(long, conflicts_with_all = ["long", "children"])]
        terminal: bool,
        /// With --terminal: the terminal of the agent running this stage (its name or id) instead
        #[arg(long, value_name = "STAGE", requires = "terminal")]
        stage: Option<String>,
    },
    /// List the current project's tickets (-P to name the project)
    List {
        /// Only this status (working, asks_you, …) or column (backlog, ready_for_dev, dev, ready_for_review,
        /// approved, done)
        #[arg(long)]
        status: Option<String>,
        /// Show epics with their tickets indented under them
        #[arg(long)]
        tree: bool,
        /// Only the tickets of this epic
        #[arg(long, value_name = "EPIC")]
        parent: Option<String>,
        /// Only this priority: 1 Urgent, 2 High, 3 Normal, 4 Low
        #[arg(long, value_name = "N", value_parser = parse_priority)]
        priority: Option<Priority>,
    },
    /// Tickets whose dependencies are all merged
    Ready,
    /// Tickets that wait for a dependency to merge
    Blocked,
    /// Create a ticket in Backlog and print its key (long text: --stdin)
    #[command(visible_alias = "new")]
    Create {
        /// The title
        title: Option<String>,
        /// A short description; use --stdin for anything longer
        #[arg(short = 'd', long = "description")]
        description: Option<String>,
        /// 1 Urgent, 2 High, 3 Normal (default), 4 Low
        #[arg(short = 'p', long, value_name = "N", value_parser = parse_priority)]
        priority: Option<Priority>,
        /// Tickets it depends on: SHOP-18,SHOP-19
        #[arg(long, value_delimiter = ',', value_name = "KEYS")]
        deps: Vec<String>,
        /// Read the description (Markdown) from stdin
        #[arg(long)]
        stdin: bool,
        /// Create every ticket of a Markdown file (### headings), all or nothing
        #[arg(short = 'f', long = "file", value_name = "FILE")]
        file: Option<PathBuf>,
        /// ticket (default) or epic
        #[arg(long = "type", value_name = "TYPE", value_parser = ["ticket", "epic"])]
        kind: Option<String>,
        /// Create it inside this epic (SHOP-40 → SHOP-40.4)
        #[arg(long, value_name = "EPIC")]
        parent: Option<String>,
        #[command(flatten)]
        choices: ChoiceFlags,
    },
    /// Change a ticket's title, description, priority or stage choices (default: $OTAQ_TICKET)
    Update {
        /// SHOP-18, or 18 in the current project
        key: Option<String>,
        /// The new title
        #[arg(long)]
        title: Option<String>,
        /// The new description (short); use --stdin for anything longer
        #[arg(short = 'd', long = "description")]
        description: Option<String>,
        /// The new priority: 1 Urgent, 2 High, 3 Normal, 4 Low
        #[arg(short = 'p', long, value_name = "N", value_parser = parse_priority)]
        priority: Option<Priority>,
        /// Read the new description (Markdown) from stdin
        #[arg(long)]
        stdin: bool,
        #[command(flatten)]
        choices: ChoiceFlags,
    },
    /// Ask the user (`ask human --stdin`; the ticket shows "Asks you") or a linked ticket's agent (`ask SHOP-18
    /// --stdin`)
    Ask {
        /// `human`, or the key of a ticket linked to this one
        #[arg(value_name = "human|KEY")]
        target: String,
        /// Read the question from stdin
        #[arg(long)]
        stdin: bool,
        /// An answer option (repeatable, 2–4 short labels); without options the question is free text
        #[arg(long = "option", value_name = "LABEL")]
        options: Vec<String>,
        /// The pros and cons of the --option right before it (optional, at most one per option)
        #[arg(long = "note", value_name = "TEXT")]
        notes: Vec<String>,
        /// The recommended option, 1-based
        #[arg(long, value_name = "N")]
        recommended: Option<u32>,
        /// Epic leads: pass a child's question on to the user (`ask human --pass-on <QUESTION-ID> --stdin`, your
        /// reason on stdin); the options default to the child's
        #[arg(long, value_name = "QUESTION-ID")]
        pass_on: Option<String>,
    },
    /// Answer a question another ticket's agent asked this one (`answer <ID> --stdin`)
    Answer {
        /// The question's ID, as `prime` or `questions` shows it
        id: String,
        /// Read the answer from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// The questions other tickets' agents asked this one that wait for an answer
    Questions {
        /// SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
    },
    /// Record a product choice you made instead of asking (`decision record --stdin`); the user sees it in the review.
    /// Epic leads: mark a child's decision a written source contradicts (`decision mark <ID> --source … --stdin`)
    #[command(subcommand)]
    Decision(DecisionCommand),
    /// An epic lead's commands (`suggest close|delete <KEY> --stdin`)
    #[command(flatten)]
    Lead(crate::lead::LeadCommand),
    /// End your stage: `stage next --stdin` (the default: on to the next stage with a summary) or `stage done
    /// --stdin` (nothing to deliver: Closed · nothing to merge, with your reason)
    #[command(subcommand)]
    Stage(StageCommand),
    /// An agent check's verdict on its ticket, the report on stdin (`check pass|fail --stdin`, check sessions only)
    #[command(subcommand)]
    Check(CheckCommand),
    /// Comment on a ticket for the next stage's agent (`comment [KEY] --stdin [--report-file PATH]`), or `comment
    /// edit|delete <N>`
    Comment(crate::comment::CommentArgs),
    /// A ticket's comments, oldest first (default: $OTAQ_TICKET)
    Comments {
        /// SHOP-34, or 34 in the current project
        key: Option<String>,
    },
    /// Print the report of comment N (`report [KEY] <N>`); `--path` prints only its file, to search it with grep
    Report {
        /// [KEY] N: SHOP-34 4, or 4 on $OTAQ_TICKET
        #[arg(value_name = "KEY", num_args = 1..=2, required = true)]
        args: Vec<String>,
        /// Print only the report's path
        #[arg(long)]
        path: bool,
    },
    /// Read the review feedback (`review feedback`); `review submit --stdin` is another name for `stage next`
    #[command(subcommand)]
    Review(ReviewCommand),
    /// A Project agent's own commands: `agent name "<3–4 words>"` (Project agent sessions only)
    #[command(subcommand)]
    Agent(AgentCommand),
    /// The Open PR and Push skills' commands: `pr propose --stdin`, `pr opened <url>`
    #[command(subcommand)]
    Pr(PrCommand),
    /// Rename the ticket's branch (`branch rename <name>`); refused once it was pushed
    #[command(subcommand)]
    Branch(BranchCommand),
    /// Add, remove or show dependencies
    #[command(subcommand)]
    Dep(DepCommand),
    /// List or show projects
    #[command(subcommand)]
    Project(ProjectCommand),
    /// The project's flow: `flow show`, `flow presets`, `flow propose -f <file> | --stdin | --from <project>`
    #[command(subcommand)]
    Flow(FlowCommand),
    /// Internal: what an agent tool's hooks run (`otaq hook --tool claude Stop`, payload on stdin)
    #[command(hide = true)]
    Hook {
        /// The agent tool whose hook this is (`claude`)
        #[arg(long)]
        tool: String,
        /// The tool's own event name (Claude Code: Notification, Stop, StopFailure or UserPromptSubmit)
        event: String,
    },
    /// Internal: the status line agents run with (status-line JSON on stdin; runs your own status line)
    #[command(hide = true)]
    Statusline,
    /// Internal: checks macOS notifications from Otaq.app (whether this runs in the bundle, its permission, one
    /// test notification and where its click arrives)
    #[command(hide = true)]
    NotifyCheck,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum AgentCommand {
    /// Name this Project agent (once, right after its first request)
    Name {
        /// 3–4 words, e.g. "Checkout slowness"
        name: String,
        /// Rename it: only when the user asked for a new name
        #[arg(long)]
        rename: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum DecisionCommand {
    /// Record a decision on the ticket: labelled lines on stdin (Question:, Built:, Other option: (repeatable), Why:)
    Record {
        /// SHOP-23, or 23 in the current project. Default: $OTAQ_TICKET (an agent records on its own ticket only)
        key: Option<String>,
        /// Read the decision from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// Epic leads: mark a child's recorded decision that a written source contradicts; one line on stdin
    Mark {
        /// The decision's ID, as `show` or `prime` prints it: SHOP-40.1#1
        id: String,
        /// The written source it contradicts: PAY-123, the epic's plan, SHOP-40.2's description…
        #[arg(long)]
        source: String,
        /// Read the mark from stdin
        #[arg(long)]
        stdin: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum StageCommand {
    /// Finish your stage with a summary on stdin, posted as your comment: the ticket moves to the next stage (an
    /// agent check, Review…); refused while files are uncommitted or a lead note is unread
    Next {
        /// SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
        /// Read the summary from stdin (at most 2,000 characters)
        #[arg(long)]
        stdin: bool,
        /// Attach a file as the comment's report (at most 20 MB): long output saved straight from a tool
        #[arg(long, value_name = "PATH")]
        report_file: Option<PathBuf>,
        /// Something you couldn't check yourself (a real browser, a device, a service), for the user to try at
        /// review: one line; repeat it for each check
        #[arg(long = "manual-check", value_name = "TEXT")]
        manual_check: Vec<String>,
    },
    /// Nothing to deliver (no commits on the branch): the ticket is Closed · nothing to merge, your reason on stdin
    /// posted as your comment; refused when the branch has commits
    Done {
        /// SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
        /// Read the reason from stdin (at most 2,000 characters)
        #[arg(long)]
        stdin: bool,
        /// Attach a file as the comment's report (at most 20 MB): long output saved straight from a tool
        #[arg(long, value_name = "PATH")]
        report_file: Option<PathBuf>,
        /// Something you couldn't check yourself (a real browser, a device, a service), for the user to try at
        /// review: one line; repeat it for each check
        #[arg(long = "manual-check", value_name = "TEXT")]
        manual_check: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum CheckCommand {
    /// The change does what the ticket asks: on to the next stage (Review, or Auto-merge)
    Pass {
        /// Read the report from stdin, posted as your comment (at most 2,000 characters)
        #[arg(long)]
        stdin: bool,
        /// Attach a file as the comment's report (at most 20 MB): long output saved straight from a tool
        #[arg(long, value_name = "PATH")]
        report_file: Option<PathBuf>,
        /// Something you couldn't check yourself (a real browser, a device, a service), for the user to try at
        /// review: one line; repeat it for each check
        #[arg(long = "manual-check", value_name = "TEXT")]
        manual_check: Vec<String>,
    },
    /// It doesn't: the findings go back to the agent
    Fail {
        /// Read the report (one finding per line, with file and line) from stdin, posted as your comment (at most
        /// 2,000 characters)
        #[arg(long)]
        stdin: bool,
        /// Attach a file as the comment's report (at most 20 MB): long output saved straight from a tool
        #[arg(long, value_name = "PATH")]
        report_file: Option<PathBuf>,
        /// Something you couldn't check yourself (a real browser, a device, a service), for the user to try at
        /// review: one line; repeat it for each check
        #[arg(long = "manual-check", value_name = "TEXT")]
        manual_check: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ReviewCommand {
    /// Another name for `stage next`: finish your stage with a summary on stdin
    Submit {
        /// SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
        /// Read the summary from stdin (at most 2,000 characters)
        #[arg(long)]
        stdin: bool,
        /// Attach a file as the comment's report (at most 20 MB): long output saved straight from a tool
        #[arg(long, value_name = "PATH")]
        report_file: Option<PathBuf>,
        /// Something you couldn't check yourself (a real browser, a device, a service), for the user to try at
        /// review: one line; repeat it for each check
        #[arg(long = "manual-check", value_name = "TEXT")]
        manual_check: Vec<String>,
    },
    /// The latest Send back's line comments, by file, and its PR's comments (reading it changes nothing)
    Feedback {
        /// SHOP-18, or 18 in the current project. Default: $OTAQ_TICKET
        key: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum PrCommand {
    /// Propose the branch name, commit messages, PR title and description (JSON on stdin); nothing is pushed
    Propose {
        /// Read the proposal from stdin
        #[arg(long)]
        stdin: bool,
    },
    /// Report the pull request the skill opened (or pushed to)
    Opened {
        /// The PR's URL, e.g. https://github.com/acme/shop-web/pull/481
        url: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum BranchCommand {
    /// Rename the ticket's temporary branch; refused after the first push
    Rename {
        /// The new branch name, e.g. bugfix/SHOP-18-login-redirect
        name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum DepCommand {
    /// Make KEY depend on DEPENDS-ON (KEY waits until DEPENDS-ON is merged)
    Add {
        key: String,
        #[arg(value_name = "DEPENDS-ON")]
        depends_on: String,
    },
    /// Remove the link "KEY depends on DEPENDS-ON"
    Remove {
        key: String,
        #[arg(value_name = "DEPENDS-ON")]
        depends_on: String,
    },
    /// The ticket, what it waits for and what waits for it, each once, with statuses (default: $OTAQ_TICKET)
    Tree { key: Option<String> },
    /// List dependency loops in the current project (none on a healthy project)
    Cycles,
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum ProjectCommand {
    /// Every project: key, name, base branch, repository
    List,
    /// One project (default: the current one)
    Show {
        /// The project's key (SHOP)
        key: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Subcommand)]
pub enum FlowCommand {
    /// The project's applied flow and the agents it names, as the YAML `flow propose` reads
    Show,
    /// The ready-made flows Otaq ships, one line each
    Presets,
    /// Propose a flow to the user: it opens in Otaq's flow editor and nothing changes until they apply it
    #[command(group(clap::ArgGroup::new("source").required(true).args(["file", "stdin", "from"])))]
    Propose {
        /// A flow file (YAML: agents and flow)
        #[arg(short = 'f', long = "file", value_name = "FILE")]
        file: Option<PathBuf>,
        /// Read the flow (YAML) from stdin
        #[arg(long)]
        stdin: bool,
        /// Propose another project's stages (its key or name); agents are shared
        #[arg(long, value_name = "PROJECT")]
        from: Option<String>,
    },
    /// Write a stage's skill for the flow being edited: prints the only folder to write it in; `--done` hands it in
    WriteSkill {
        /// The stage's id or name, e.g. plan
        stage: String,
        /// The skill is written: Otaq imports the folder as its own copy and puts it in the stage
        #[arg(long)]
        done: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Subcommand)]
pub enum DaemonCommand {
    /// Start the background service if it isn't running
    Start,
    /// Stop the background service (working agents are interrupted)
    Stop,
    /// Show whether the background service runs, with its PID, build ID and uptime
    Status,
    /// Run the service in the foreground (what `daemon start` launches in the background)
    #[command(hide = true)]
    Run,
}

/// Whether the raw arguments ask for JSON output. Used to format errors clap reports before parsing succeeds.
/// `-p 2` / `--priority 2`: 1–4, anything else refused with the allowed values (TKT-09).
fn parse_priority(text: &str) -> Result<Priority, String> {
    text.parse()
}

/// Parses the arguments, keeping clap's matches: `ask`'s notes are paired with its options by where they stand on
/// the command line ([`option_notes`]), which the derived struct loses.
pub fn parse_from(args: &[OsString]) -> Result<(Cli, ArgMatches), clap::Error> {
    let matches = Cli::command().try_get_matches_from(args)?;
    let cli = Cli::from_arg_matches(&matches)?;
    Ok((cli, matches))
}

/// `ask`'s notes (ATT-17), one entry per `--option` in order, `None` for an option without one: each `--note`
/// belongs to the `--option` right before it on the command line (`--option A --option B --note x`: the note is
/// B's). Refused, naming the rule: a `--note` with no `--option` before it, two for one option.
pub fn option_notes(matches: &ArgMatches) -> Result<Vec<Option<String>>, String> {
    let Some(ask) = matches.subcommand_matches("ask") else {
        return Ok(Vec::new());
    };
    let at = |id: &str| -> Vec<usize> {
        ask.indices_of(id)
            .map(Iterator::collect)
            .unwrap_or_default()
    };
    let options: Vec<&String> = ask
        .get_many("options")
        .map(Iterator::collect)
        .unwrap_or_default();
    let notes: Vec<&String> = ask
        .get_many("notes")
        .map(Iterator::collect)
        .unwrap_or_default();
    let (option_at, note_at) = (at("options"), at("notes"));
    let mut paired: Vec<Option<String>> = vec![None; options.len()];
    for (note, index) in notes.into_iter().zip(note_at) {
        let Some(n) = option_at.iter().rposition(|&o| o < index) else {
            return Err(NOTE_AFTER_OPTION.to_owned());
        };
        if paired[n].is_some() {
            return Err(format!(
                "Give at most one --note per --option: \"{}\" has two",
                options[n]
            ));
        }
        paired[n] = Some(note.clone());
    }
    Ok(paired)
}

/// The refusal of a `--note` with no `--option` before it.
pub const NOTE_AFTER_OPTION: &str = "--note must follow the --option it describes";

pub fn wants_json(args: &[OsString]) -> bool {
    args.iter()
        .skip(1)
        .take_while(|a| *a != "--")
        .any(|a| a == "--json")
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory;

    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once(naming::COMMAND_NAME).chain(args.iter().copied()))
    }

    fn notes(args: &[&str]) -> Result<Vec<Option<String>>, String> {
        let args: Vec<OsString> = std::iter::once(naming::COMMAND_NAME)
            .chain(args.iter().copied())
            .map(OsString::from)
            .collect();
        option_notes(&parse_from(&args).unwrap().1)
    }

    fn some(text: &str) -> Option<String> {
        Some(text.to_owned())
    }

    /// T-0498 (d0125): `--manual-check` repeats on `stage next`, `stage done`, `review submit` and `check pass|fail`.
    #[test]
    fn manual_checks_repeat_on_every_stage_end() {
        let items = vec!["Reload offline".to_owned(), "Install on a phone".to_owned()];
        let flags = [
            "--manual-check",
            "Reload offline",
            "--manual-check",
            "Install on a phone",
        ];
        let with =
            |command: &[&str]| parse(&[command, &["--stdin"][..], &flags[..]].concat()).unwrap();
        for command in [
            &["stage", "next"][..],
            &["stage", "done"],
            &["review", "submit"],
        ] {
            let manual = match with(command).command {
                Some(Command::Stage(StageCommand::Next { manual_check, .. }))
                | Some(Command::Stage(StageCommand::Done { manual_check, .. }))
                | Some(Command::Review(ReviewCommand::Submit { manual_check, .. })) => manual_check,
                other => panic!("{other:?}"),
            };
            assert_eq!(manual, items, "{command:?}");
        }
        for command in [&["check", "pass"][..], &["check", "fail"]] {
            let manual = match with(command).command {
                Some(Command::Check(
                    CheckCommand::Pass { manual_check, .. }
                    | CheckCommand::Fail { manual_check, .. },
                )) => manual_check,
                other => panic!("{other:?}"),
            };
            assert_eq!(manual, items, "{command:?}");
        }
        let none = match parse(&["stage", "next", "--stdin"]).unwrap().command {
            Some(Command::Stage(StageCommand::Next { manual_check, .. })) => manual_check,
            other => panic!("{other:?}"),
        };
        assert!(none.is_empty());
    }

    const MD: &str = "Opens in any editor; one file per note, no attachments.";
    const ZIP: &str = "One download with everything, but needs a ZIP library and an import later.";

    /// ATT-17: a `--note` belongs to the `--option` right before it on the command line, not by count.
    #[test]
    fn notes_pair_with_the_option_before_them() {
        let ask = ["ask", "human", "--stdin"];
        let with = |rest: &[&str]| notes(&[&ask[..], rest].concat());
        assert_eq!(
            with(&[
                "--option",
                "Markdown files",
                "--note",
                MD,
                "--option",
                "One ZIP",
                "--note",
                ZIP,
                "--recommended",
                "1"
            ]),
            Ok(vec![some(MD), some(ZIP)])
        );
        assert_eq!(
            with(&[
                "--option", "A", "--note", "a", "--option", "B", "--option", "C", "--note", "c"
            ]),
            Ok(vec![some("a"), None, some("c")])
        );
        assert_eq!(
            with(&["--option", "A", "--option", "B", "--option", "C"]),
            Ok(vec![None, None, None])
        );
        assert_eq!(
            with(&[
                "--option", "A", "--note", "a", "--option", "B", "--note", "b", "--option", "C",
                "--note", "c"
            ]),
            Ok(vec![some("a"), some("b"), some("c")])
        );
        assert_eq!(
            with(&["--option", "A", "--option", "B", "--note", "x"]),
            Ok(vec![None, some("x")])
        );
        assert_eq!(
            with(&["--option=A", "--note=a", "--option=B"]),
            Ok(vec![some("a"), None])
        );
        assert_eq!(with(&[]), Ok(vec![]));
        let long = "x".repeat(600);
        assert_eq!(
            with(&["--option", "A", "--note", &long, "--option", "B"]),
            Ok(vec![Some(long.clone()), None])
        );
        assert!(
            notes(&["list"]).unwrap().is_empty(),
            "other commands have none"
        );
    }

    #[test]
    fn a_note_out_of_place_is_refused_naming_the_rule() {
        let ask = ["ask", "human", "--stdin"];
        let with = |rest: &[&str]| notes(&[&ask[..], rest].concat());
        assert_eq!(
            with(&["--note", "a", "--option", "A", "--option", "B"]).unwrap_err(),
            "--note must follow the --option it describes"
        );
        assert_eq!(with(&["--note", "a"]).unwrap_err(), NOTE_AFTER_OPTION);
        assert_eq!(
            with(&[
                "--option", "A", "--note", "a", "--note", "b", "--option", "B"
            ])
            .unwrap_err(),
            "Give at most one --note per --option: \"A\" has two"
        );
    }

    /// #62: `create --help` doesn't suggest skipping Grill (a stage only a flow can make skippable).
    #[test]
    fn the_help_of_create_suggests_no_grill_skip() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("create")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(!help.contains("Grill=skip"), "{help}");
        assert!(
            help.contains("\"QA=skip\" where the flow lets QA be skipped"),
            "{help}"
        );
    }

    #[test]
    fn the_help_of_ask_describes_note_in_one_line() {
        let mut command = Cli::command();
        let help = command
            .find_subcommand_mut("ask")
            .unwrap()
            .render_help()
            .to_string();
        let line = help
            .lines()
            .find(|l| l.trim_start().starts_with("--note <TEXT>"))
            .unwrap_or_else(|| panic!("{help}"));
        assert_eq!(
            line.trim_start_matches([' '])
                .trim_start_matches("--note <TEXT>")
                .trim(),
            "The pros and cons of the --option right before it (optional, at most one per option)"
        );
    }

    /// The `otaq ask human …` commands in a skill's fenced code blocks, as argument lists: a `\` line end joins the
    /// next line, quotes group words, and the command ends at its heredoc (`<<'QUESTION'`).
    fn fenced_ask_examples(text: &str) -> Vec<Vec<String>> {
        let lead = format!("{} ask human", naming::COMMAND_NAME);
        let (mut fenced, mut found, mut command) = (false, Vec::new(), None::<String>);
        for line in text.lines().map(str::trim) {
            if line.starts_with("```") {
                fenced = !fenced;
                continue;
            }
            if !fenced {
                continue;
            }
            let Some(mut so_far) = command
                .take()
                .or_else(|| line.starts_with(&lead).then(String::new))
            else {
                continue;
            };
            let (part, more) = match line.strip_suffix('\\') {
                Some(part) => (part, true),
                None => (line, false),
            };
            so_far.push_str(part);
            so_far.push(' ');
            if more {
                command = Some(so_far);
            } else {
                found.push(shell_words(&so_far));
            }
        }
        found
    }

    /// Words as a shell splits them, enough for the skills' examples: `"…"` and `'…'` group, `<<` ends the command.
    fn shell_words(command: &str) -> Vec<String> {
        let command = command.split("<<").next().unwrap_or_default();
        let (mut words, mut word, mut quote, mut started) =
            (Vec::new(), String::new(), None, false);
        for c in command.chars() {
            match (quote, c) {
                (Some(q), c) if c == q => quote = None,
                (Some(_), c) => word.push(c),
                (None, '"' | '\'') => (quote, started) = (Some(c), true),
                (None, c) if c.is_whitespace() => {
                    if started {
                        words.push(std::mem::take(&mut word));
                    }
                    started = false;
                }
                (None, c) => {
                    word.push(c);
                    started = true;
                }
            }
        }
        if started {
            words.push(word);
        }
        words
    }

    /// The skills only teach `ask` flags the CLI has (ATT-17, DEC-07): every fenced `otaq ask human …` example in
    /// plugin/skills/*/SKILL.md parses, and its notes pair with its options.
    #[test]
    fn every_ask_example_in_the_skills_parses() {
        let skills =
            otaq_core::agent_tools::claude::find_plugin_dir(std::path::Path::new("/nonexistent"))
                .expect("the repository's plugin folder")
                .join("skills");
        let mut checked = 0;
        for entry in std::fs::read_dir(&skills).unwrap() {
            let file = entry.unwrap().path().join("SKILL.md");
            let Ok(text) = std::fs::read_to_string(&file) else {
                continue;
            };
            for example in fenced_ask_examples(&text) {
                let args: Vec<OsString> = example.iter().map(OsString::from).collect();
                let (_, matches) = parse_from(&args).unwrap_or_else(|e| {
                    panic!("{}: {example:?} doesn't parse:\n{e}", file.display())
                });
                if let Err(e) = option_notes(&matches) {
                    panic!("{}: {example:?}: {e}", file.display());
                }
                checked += 1;
            }
        }
        assert!(checked >= 10, "only {checked} examples found");
    }

    #[test]
    fn ask_examples_are_found_in_fenced_blocks() {
        let cmd = naming::COMMAND_NAME;
        let text = format!(
            "Run `{cmd} ask human --stdin` here.\n\n  ```sh\n  {cmd} ask human \\\n    --option \"Yes, two\" \
             --note 'It''s fine' \\\n    --recommended 1 --stdin <<'QUESTION'\n  Which?\n  QUESTION\n  ```\n"
        );
        assert_eq!(
            fenced_ask_examples(&text),
            vec![vec![
                cmd,
                "ask",
                "human",
                "--option",
                "Yes, two",
                "--note",
                "Its fine",
                "--recommended",
                "1",
                "--stdin"
            ]]
        );
    }

    #[test]
    fn definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn no_command_means_open_the_app() {
        let cli = parse(&[]).unwrap();
        assert!(cli.command.is_none());
        assert_eq!(cli.global.mode(), Mode::Human);
    }

    #[test]
    fn daemon_subcommands() {
        for (arg, expected) in [
            ("start", DaemonCommand::Start),
            ("stop", DaemonCommand::Stop),
            ("status", DaemonCommand::Status),
        ] {
            let cli = parse(&["daemon", arg]).unwrap();
            assert!(matches!(cli.command, Some(Command::Daemon(d)) if d == expected));
        }
        assert!(parse(&["daemon"]).is_err());
        assert!(parse(&["daemon", "restart"]).is_err());
    }

    #[test]
    fn attach_takes_an_optional_key() {
        let cli = parse(&["attach", "SHOP-18"]).unwrap();
        assert!(
            matches!(cli.command, Some(Command::Attach { key: Some(k), stage: None }) if k == "SHOP-18")
        );
        let cli = parse(&["attach"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Attach {
                key: None,
                stage: None
            })
        ));
        let cli = parse(&["attach", "SHOP-29", "--stage", "security"]).unwrap();
        assert!(
            matches!(cli.command, Some(Command::Attach { key: Some(k), stage: Some(s) }) if k == "SHOP-29" && s == "security")
        );
        let cli = parse(&["-P", "SHOP", "attach", "18"]).unwrap();
        assert_eq!(cli.global.project.as_deref(), Some("SHOP"));
        // FLW-20: `--validator` is gone, with no alias.
        assert!(parse(&["attach", "SHOP-29", "--validator"]).is_err());
    }

    /// `show --terminal` (SHL-10): not with `--long` or `--children`; `--stage` only with it; `--validator` is gone
    /// (FLW-20).
    #[test]
    fn show_terminal_stands_alone() {
        let cli = parse(&["show", "NOTE-6", "--terminal", "--stage", "Security"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Show { key: Some(ref k), terminal: true, stage: Some(ref s), .. }) if k == "NOTE-6" && s == "Security"
        ));
        for wrong in [
            &["show", "--terminal", "--long"][..],
            &["show", "--terminal", "--children"],
            &["show", "--stage", "security"],
            &["show", "NOTE-6", "--terminal", "--validator"],
        ] {
            assert!(parse(wrong).is_err(), "{wrong:?}");
        }
    }

    /// `prime --project` (d0068) is the bare global `--project`; `-P SHOP` still names a project.
    #[test]
    fn prime_project_is_a_bare_project_flag() {
        let cli = parse(&["prime", "--project"]).unwrap();
        assert!(cli.global.bare_project());
        assert_eq!(cli.global.project_key(), None);
        assert!(matches!(
            cli.command,
            Some(Command::Prime {
                key: None,
                validate: false,
                epic: None
            })
        ));
        let cli = parse(&["-P", "SHOP", "prime", "18"]).unwrap();
        assert!(!cli.global.bare_project());
        assert_eq!(cli.global.project_key(), Some("SHOP"));
    }

    /// `prime --epic SHOP-40` names the epic; a bare `--epic` leaves it to $OTAQ_EPIC (LEAD-03).
    #[test]
    fn prime_epic_takes_an_optional_key() {
        let cli = parse(&["prime", "--epic", "SHOP-40"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::Prime { key: None, epic: Some(ref e), .. }) if e == "SHOP-40"
        ));
        let cli = parse(&["prime", "--epic", "--json"]).unwrap();
        assert!(cli.global.json);
        assert!(matches!(
            cli.command,
            Some(Command::Prime { epic: Some(ref e), .. }) if e.is_empty()
        ));
        assert!(parse(&["prime", "--epic", "SHOP-40", "--validate"]).is_err());
    }

    #[test]
    fn global_flags_parse_before_and_after_the_subcommand() {
        let cli = parse(&["--json", "daemon", "status"]).unwrap();
        assert_eq!(cli.global.mode(), Mode::Json);
        let cli = parse(&[
            "daemon",
            "status",
            "--json",
            "-P",
            "SHOP",
            "--actor",
            "agent:SHOP-18",
        ])
        .unwrap();
        assert_eq!(cli.global.mode(), Mode::Json);
        assert_eq!(cli.global.project.as_deref(), Some("SHOP"));
        assert_eq!(cli.global.actor.as_deref(), Some("agent:SHOP-18"));
        let cli = parse(&["daemon", "-q", "start"]).unwrap();
        assert_eq!(cli.global.mode(), Mode::Quiet);
        let cli = parse(&["daemon", "start", "--project", "BILL", "--silent"]).unwrap();
        assert_eq!(cli.global.mode(), Mode::Silent);
        assert_eq!(cli.global.project.as_deref(), Some("BILL"));
    }

    #[test]
    fn mode_precedence() {
        let all = GlobalArgs {
            json: true,
            quiet: true,
            silent: true,
            ..Default::default()
        };
        assert_eq!(all.mode(), Mode::Json);
        let silent_quiet = GlobalArgs {
            quiet: true,
            silent: true,
            ..Default::default()
        };
        assert_eq!(silent_quiet.mode(), Mode::Silent);
    }

    #[test]
    fn help_lists_only_built_commands() {
        let help = Cli::command().render_long_help().to_string();
        assert!(help.contains("daemon"), "{help}");
        assert!(help.contains("attach"), "{help}");
        for shown in [
            "prime",
            "show",
            "list",
            "ready",
            "blocked",
            "create",
            "update",
            "ask",
            "answer",
            "questions",
            "review",
            "pr",
            "branch",
            "dep",
            "project",
        ] {
            assert!(help.contains(&format!("\n  {shown}")), "{shown}:\n{help}");
        }
        // `hook` and `statusline` are internal (hidden).
        for later in ["hook", "statusline"] {
            assert!(
                !help.contains(&format!("\n  {later} ")),
                "{later} shown:\n{help}"
            );
        }
    }

    #[test]
    fn json_detection() {
        let args = |a: &[&str]| -> Vec<OsString> { a.iter().map(OsString::from).collect() };
        assert!(wants_json(&args(&["x", "daemon", "--json"])));
        assert!(!wants_json(&args(&["x", "daemon"])));
        assert!(!wants_json(&args(&["x", "--", "--json"])));
    }
}
