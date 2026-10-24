//! The `otaq` command: launches the app and provides the `daemon`, `attach`, `hook` and agent-facing
//! subcommands (d0002, d0031).

mod agent;
mod attach;
mod cli;
mod comment;
mod daemon;
mod decision;
mod deps;
mod flow;
mod hook;
mod launch;
mod lead;
mod notify_check;
mod output;
mod pr;
mod prime;
mod read;
mod service;
mod statusline;
mod target;
mod write;

use std::process::ExitCode;

use clap::error::ErrorKind as ClapErrorKind;
use otaq_core::protocol::{ErrorBody, ErrorKind};

use cli::Command;
use output::{EXIT_ERROR, EXIT_USAGE, Mode, Output};

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    let (cli, matches) = match cli::parse_from(&args) {
        Ok(parsed) => parsed,
        Err(e) => return usage_error(e, cli::wants_json(&args)),
    };
    let out = Output::new(cli.global.mode());
    let result = match cli.command {
        None => launch::launch_app(),
        Some(Command::Daemon(command)) => daemon::run(command, &out),
        Some(Command::Attach { key, stage }) => attach::run(key, stage, &cli.global),
        Some(Command::Prime {
            epic: Some(epic), ..
        }) => prime::epic(epic, &cli.global, &out),
        Some(Command::Prime {
            key: None,
            validate: false,
            ..
        }) if cli.global.bare_project() => prime::project(&cli.global, &out),
        Some(Command::Prime { key, validate, .. }) => prime::run(key, validate, &cli.global, &out),
        Some(Command::Show {
            key,
            terminal: true,
            stage,
            ..
        }) => read::terminal(key, stage, &cli.global, &out),
        Some(Command::Show {
            key,
            long,
            children,
            ..
        }) => read::show(
            read::ShowArgs {
                key,
                long,
                children,
            },
            &cli.global,
            &out,
        ),
        Some(Command::List {
            status,
            tree,
            parent,
            priority,
        }) => read::list(
            read::ListArgs {
                status,
                tree,
                parent,
                priority,
            },
            &cli.global,
            &out,
        ),
        Some(Command::Ready) => read::readiness(false, &cli.global, &out),
        Some(Command::Blocked) => read::readiness(true, &cli.global, &out),
        Some(Command::Create {
            title,
            description,
            priority,
            deps,
            stdin,
            file,
            kind,
            parent,
            choices,
        }) => write::create(
            write::CreateArgs {
                title,
                description,
                priority,
                deps,
                stdin,
                file,
                epic: kind.as_deref() == Some("epic"),
                parent,
                choices,
            },
            &cli.global,
            &out,
        ),
        Some(Command::Update {
            key,
            title,
            description,
            priority,
            stdin,
            choices,
        }) => write::update(
            write::UpdateArgs {
                key,
                title,
                description,
                priority,
                stdin,
                choices,
            },
            &cli.global,
            &out,
        ),
        Some(Command::Ask {
            target,
            stdin,
            options,
            notes: _,
            recommended,
            pass_on,
        }) => agent::ask(
            agent::AskArgs {
                target,
                stdin,
                options,
                notes: cli::option_notes(&matches),
                recommended,
                pass_on,
            },
            &cli.global,
            &out,
        ),
        Some(Command::Review(cli::ReviewCommand::Submit {
            key,
            stdin,
            report_file,
            manual_check,
        }))
        | Some(Command::Stage(cli::StageCommand::Next {
            key,
            stdin,
            report_file,
            manual_check,
        })) => agent::stage_next(
            key,
            agent::StageEnd {
                stdin,
                report_file,
                manual_checks: manual_check,
            },
            &cli.global,
            &out,
        ),
        Some(Command::Stage(cli::StageCommand::Done {
            key,
            stdin,
            report_file,
            manual_check,
        })) => agent::stage_done(
            key,
            agent::StageEnd {
                stdin,
                report_file,
                manual_checks: manual_check,
            },
            &cli.global,
            &out,
        ),
        Some(Command::Decision(command)) => decision::run(command, &cli.global, &out),
        Some(Command::Lead(command)) => lead::run(command, &cli.global, &out),
        Some(Command::Check(command)) => agent::check(command, &cli.global, &out),
        Some(Command::Agent(cli::AgentCommand::Name { name, rename })) => {
            agent::name(&name, rename, &cli.global, &out)
        }
        Some(Command::Review(cli::ReviewCommand::Feedback { key })) => {
            agent::feedback(key, &cli.global, &out)
        }
        Some(Command::Pr(command)) => pr::pr(command, &cli.global, &out),
        Some(Command::Branch(command)) => pr::branch(command, &cli.global, &out),
        Some(Command::Answer { id, stdin }) => agent::answer(id, stdin, &cli.global, &out),
        Some(Command::Questions { key }) => agent::questions(key, &cli.global, &out),
        Some(Command::Comment(args)) => comment::comment(args, &cli.global, &out),
        Some(Command::Comments { key }) => comment::list(key, &cli.global, &out),
        Some(Command::Report { args, path }) => comment::report(args, path, &cli.global, &out),
        Some(Command::Dep(command)) => deps::run(command, &cli.global, &out),
        Some(Command::Project(command)) => read::project(command, &cli.global, &out),
        Some(Command::Flow(command)) => flow::run(command, &cli.global, &out),
        Some(Command::Hook { tool, event }) => {
            hook::run(&tool, &event);
            Ok(())
        }
        Some(Command::Statusline) => std::process::exit(statusline::run()),
        Some(Command::NotifyCheck) => notify_check::run(),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => out.error(&error, EXIT_ERROR),
    }
}

/// Help and version go to stdout as usual. Other parse errors follow the output mode: with `--json` they are
/// `{"error": {"kind": "usage", ...}}`.
fn usage_error(error: clap::Error, json: bool) -> ExitCode {
    if matches!(
        error.kind(),
        ClapErrorKind::DisplayHelp | ClapErrorKind::DisplayVersion
    ) || !json
    {
        error.exit();
    }
    let rendered = error.render().to_string();
    let first_line = rendered.lines().next().unwrap_or_default();
    let message = first_line.strip_prefix("error: ").unwrap_or(first_line);
    Output::new(Mode::Json).error(&ErrorBody::new(ErrorKind::Usage, message), EXIT_USAGE)
}

#[cfg(test)]
mod tests {
    #[test]
    fn binary_is_named_after_the_command() {
        assert_eq!(env!("CARGO_BIN_NAME"), otaq_core::naming::COMMAND_NAME);
    }
}
