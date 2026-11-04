//! The user's own terminals and processes (SDD §19; d0019, d0026, d0035): **Run** (the project's run command in a
//! ticket's worktree), the **Shell** tab and the **Project terminal**. All three are PTY sessions owned by the
//! background service (d0022), so they survive closing the window; all three start only on the user's click and
//! are never restarted on their own (d0045). Their callers are the user, with full `otaq` access (CLI-03):
//! none of them runs `claude` for a ticket.

pub mod run;
pub mod shell;
pub mod suggest;
pub mod tool;

use std::ffi::OsString;
use std::path::PathBuf;

use crate::data_dir::DataDir;
use crate::domain::TicketKey;
use crate::naming;

/// What the user's terminals start with on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEnv {
    /// The folder of the `otaq` command, put first on PATH so the terminals can call it (CLI-03): the build's runtime
    /// copy (UPD-02, [`crate::runtime`]) once the service has made it.
    pub bin_dir: Option<PathBuf>,
    /// The PATH the terminals extend (the service's).
    pub path: OsString,
    /// The program Shell and the Project terminal run: the user's login shell (`$SHELL`, else `/bin/sh`);
    /// PowerShell on Windows ([`crate::shell::interactive`]).
    pub shell: String,
    /// Its arguments: `-l` for a login shell (`-NoLogo` for PowerShell).
    pub shell_args: Vec<String>,
}

impl TerminalEnv {
    /// The running service's: its binary's folder, its PATH and the user's shell.
    pub fn from_process() -> Self {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let (shell, shell_args) =
            crate::shell::interactive(std::env::var("SHELL").ok().as_deref(), &path);
        Self {
            bin_dir: std::env::current_exe()
                .ok()
                .and_then(|exe| exe.parent().map(PathBuf::from)),
            path,
            shell,
            shell_args,
        }
    }

    /// The environment a terminal adds to the service's: the data folder (so `otaq` there talks to this
    /// service), PATH with the `otaq` folder first, and for a ticket's Run and Shell its key and `$PORT` =
    /// the Run port (d0026).
    pub fn env(
        &self,
        data: &DataDir,
        ticket: Option<&TicketKey>,
        port: Option<u16>,
    ) -> Vec<(String, String)> {
        let path = crate::shell::prepend_path(self.bin_dir.as_deref(), &self.path);
        let mut env = vec![
            (DataDir::env_var(), data.root().display().to_string()),
            ("PATH".to_owned(), path.to_string_lossy().into_owned()),
        ];
        if let Some(ticket) = ticket {
            env.push((naming::ticket_env_var(), ticket.to_string()));
        }
        if let Some(port) = port {
            env.push(("PORT".to_owned(), port.to_string()));
        }
        env
    }
}

/// `$PORT` (and `${PORT}`) in a command or URL replaced by the port, for showing: "pnpm dev --port 4020".
pub fn with_port(text: &str, port: u16) -> String {
    let port = port.to_string();
    text.replace("${PORT}", &port).replace("$PORT", &port)
}

/// `OTAQ_PORT_<NAME>` = the port of each of the ticket's apps (d{RUN}): set in every app's Run and in the Shell.
pub fn app_port_vars(apps: &[(String, u16)]) -> Vec<(String, String)> {
    apps.iter()
        .map(|(app, port)| (naming::app_port_env_var(app), port.to_string()))
        .collect()
}

/// [`with_port`], and each `$OTAQ_PORT_<NAME>` (or `${…}`) replaced by that app's port (d{RUN}): what a command or URL
/// shows, and what Windows' shell, which has no `$` variables, runs. Longer names first, so `OTAQ_PORT_API` never
/// eats the start of `OTAQ_PORT_API_V2`.
pub fn with_ports(text: &str, port: Option<u16>, apps: &[(String, u16)]) -> String {
    let mut vars = app_port_vars(apps);
    vars.sort_by_key(|(name, _)| std::cmp::Reverse(name.len()));
    let mut out = text.to_owned();
    for (name, value) in vars {
        out = out
            .replace(&format!("${{{name}}}"), &value)
            .replace(&format!("${name}"), &value);
    }
    match port {
        Some(port) => with_port(&out, port),
        None => out,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn port_substitution() {
        assert_eq!(
            with_port("pnpm dev --port $PORT", 4020),
            "pnpm dev --port 4020"
        );
        assert_eq!(
            with_port("http://localhost:${PORT}/swagger", 4004),
            "http://localhost:4004/swagger"
        );
        assert_eq!(with_port("cargo run", 4004), "cargo run");
    }

    #[test]
    fn app_ports_become_variables_and_are_filled_in() {
        let apps = vec![("api".to_owned(), 4021), ("api-v2".to_owned(), 4022)];
        let vars = app_port_vars(&apps);
        assert_eq!(
            vars,
            vec![
                (naming::app_port_env_var("api"), "4021".to_owned()),
                (naming::app_port_env_var("api-v2"), "4022".to_owned()),
            ]
        );
        let api = format!("${}", naming::app_port_env_var("api"));
        let v2 = format!("${{{}}}", naming::app_port_env_var("api-v2"));
        assert_eq!(
            with_ports(
                &format!("serve --port $PORT --api {api} --v2 {v2}"),
                Some(4020),
                &apps
            ),
            "serve --port 4020 --api 4021 --v2 4022"
        );
        assert_eq!(
            with_ports("serve --port $PORT", None, &apps),
            "serve --port $PORT"
        );
    }

    #[test]
    fn env_has_the_data_folder_path_ticket_and_port() {
        let ctx = TerminalEnv {
            bin_dir: Some(PathBuf::from("/opt/cb/bin")),
            path: std::env::join_paths(["/usr/bin", "/bin"]).unwrap(),
            shell: "/bin/sh".into(),
            shell_args: vec![],
        };
        let data = DataDir::new("/tmp/h");
        let env = ctx.env(&data, Some(&"SHOP-19".parse().unwrap()), Some(4020));
        let get = |k: &str| env.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());
        let path = std::env::join_paths(["/opt/cb/bin", "/usr/bin", "/bin"]).unwrap();
        assert_eq!(get("PATH"), path.to_str());
        assert_eq!(get("PORT"), Some("4020"));
        assert_eq!(get(&naming::ticket_env_var()), Some("SHOP-19"));
        assert_eq!(get(&DataDir::env_var()), Some("/tmp/h"));
        let plain = ctx.env(&data, None, None);
        assert!(plain.iter().all(|(n, _)| n != "PORT"));
        assert!(plain.iter().all(|(n, _)| n != &naming::ticket_env_var()));
    }
}
