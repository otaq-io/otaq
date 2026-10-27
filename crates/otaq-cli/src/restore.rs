//! `otaq restore-database [--yes]` (UPD-08, d0140 gap review G5): puts back the board's copy from before a newer
//! version changed it. It asks once (page 32's confirmation), then stops the service, moves the current database
//! aside, puts the copy back and starts the service ([`otaq_core::store::restore`]), printing each step as it starts.
//! `--yes` skips the question (the window's Restore button). Guarded like `uninstall`: from a development build or a
//! test it needs `OTAQ_HOME`.

use std::io::IsTerminal;

use otaq_core::data_dir::DataDir;
use otaq_core::naming;
use otaq_core::protocol::{ErrorBody, ErrorKind};
use otaq_core::store::restore::{self, RestoreError, RestoreStep, ServiceControl};
use otaq_core::store::versions;
use serde_json::json;

use crate::cli::GlobalArgs;
use crate::output::Output;
use crate::service::internal;

/// Stops and starts the service as `daemon stop` and `daemon start` do.
struct Daemon<'a>(&'a DataDir);

impl ServiceControl for Daemon<'_> {
    fn stop(&mut self) -> Result<(), String> {
        crate::daemon::stop_service(self.0)
            .map(drop)
            .map_err(|e| e.message)
    }

    fn start(&mut self) -> Result<(), String> {
        crate::daemon::ensure_started(self.0)
            .map(drop)
            .map_err(|e| e.message)
    }
}

fn refused(message: impl Into<String>) -> ErrorBody {
    ErrorBody::new(ErrorKind::Refused, message)
}

pub fn run(yes: bool, global: &GlobalArgs, out: &Output) -> Result<(), ErrorBody> {
    if let Some(why) = crate::uninstall::guard(
        "restore-database",
        &|name| std::env::var_os(name),
        cfg!(debug_assertions),
    ) {
        return Err(refused(why));
    }
    let data = DataDir::from_env().map_err(|e| internal(e.to_string()))?;
    let database = data.database_path();
    let Some(copy) = versions::newest_copy(&database) else {
        return Err(refused(RestoreError::NoCopy.to_string()));
    };
    if !yes {
        if global.json || !std::io::stdin().is_terminal() {
            return Err(refused(format!(
                "Run {} restore-database in a terminal to answer its question, or add --yes",
                naming::COMMAND_NAME
            )));
        }
        let (title, [what, untouched]) = restore::question(&copy);
        println!("{what}\n{untouched}\n");
        if !crate::uninstall::ask(&title)? {
            return Err(refused("Nothing changed."));
        }
    }

    let mut steps = Vec::new();
    let restored = restore::restore(&database, &mut Daemon(&data), &mut |step: &RestoreStep| {
        if out.is_human() {
            println!("{}", step.label());
        }
        steps.push(step.clone());
    });
    match restored {
        Ok(restored) => {
            let mut human = format!(
                "The board is back as it was before {} {}.",
                naming::PRODUCT_NAME,
                restored.copy.version
            );
            if let Some(aside) = &restored.replaced {
                human.push_str(&format!(
                    " The database it replaced is kept at {}.",
                    aside.display()
                ));
            }
            out.print(
                &human,
                &json!({
                    "restored": true,
                    "copy": restored.copy,
                    "replaced": restored.replaced,
                    "steps": steps,
                }),
                None,
            );
            Ok(())
        }
        Err(RestoreError::NoCopy) => Err(refused(RestoreError::NoCopy.to_string())),
        Err(failed) => {
            let message = failed.to_string();
            let left = match failed {
                RestoreError::Failed { left, .. } => left,
                RestoreError::NoCopy => String::new(),
            };
            Err(ErrorBody {
                kind: ErrorKind::Internal,
                message,
                data: Some(json!({ "steps": steps, "left": left })),
            })
        }
    }
}
