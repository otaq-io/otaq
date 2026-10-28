//! Codex's one-time setup repaired by a check (T-0351; ADP-07, d0086): a Codex that stays Connected but whose
//! setup is gone from its home (Otaq's hook trust removed) gets only its setup again at the next check; with the
//! setup in place a check runs nothing.
//!
//! No real tool runs: `codex` is a stand-in shell script that writes its plugin tables and the hook trust as Codex
//! does, in a temporary home folder (the service's tool home), never the real `~/.codex`.

use std::path::{Path, PathBuf};

use otaq_core::agent_tools::connected::{AgentToolsList, SetupAction};
use otaq_core::agent_tools::{LaunchContext, Tool, claude};
use otaq_core::client::{Client, ClientConfig};
use otaq_core::naming;
use otaq_core::protocol::Reply;
use otaq_core::service::testing::TestService;
use serde_json::{Value, json};

/// Otaq's five hooks, by the names Codex's trust keys use.
const EVENTS: [&str; 5] = [
    "session_start",
    "permission_request",
    "stop",
    "interrupt",
    "user_prompt_submit",
];

fn plugin_id() -> String {
    format!("{0}@{0}-local", naming::PLUGIN_NAMESPACE)
}

fn hook_key(event: &str) -> String {
    format!("{}:plugin.json#hooks[0]:{event}:0:0", plugin_id())
}

/// A `hooks/list` result listing Otaq's five hooks, untrusted.
fn hooks_list() -> Value {
    let hooks: Vec<Value> = EVENTS
        .iter()
        .map(|event| {
            json!({
                "key": hook_key(event),
                "pluginId": plugin_id(),
                "currentHash": format!("sha256:{event}"),
                "trustStatus": "untrusted",
            })
        })
        .collect();
    json!({ "data": [{ "cwd": "/", "hooks": hooks, "errors": [], "warnings": [] }] })
}

/// The stand-in: `plugin marketplace add` / `plugin add` append Codex's tables once; `app-server` answers
/// `initialize`, `hooks/list` and `config/batchWrite`, the last appending each hook's trust as Codex does.
fn stand_in(bin: &Path, codex_home: &Path, log: &Path) {
    let config = codex_home.join("config.toml");
    let answer = bin.join("hooks.json");
    std::fs::write(&answer, hooks_list().to_string()).unwrap();
    let trust: String = EVENTS
        .iter()
        .map(|e| {
            format!(
                "\\n[hooks.state.\"{}\"]\\ntrusted_hash = \"sha256:{e}\"\\n",
                hook_key(e)
            )
        })
        .collect();
    let id = plugin_id();
    let market = format!("{}-local", naming::PLUGIN_NAMESPACE);
    let script = format!(
        r#"echo "$*" >> '{log}'
f='{config}'
if [ "$1" = plugin ]; then
  case "$2 $3" in
    "marketplace add") /usr/bin/grep -q '^\[marketplaces.{market}\]' "$f" || printf '\n[marketplaces.{market}]\nsource_type = "local"\nsource = "%s"\n' "$(printf '%s' "$4" | /usr/bin/sed 's/\\/\\\\/g')" >> "$f" ;;
    "add {id}") /usr/bin/grep -q '^\[plugins."{id}"\]' "$f" || printf '\n[plugins."{id}"]\nenabled = true\n' >> "$f" ;;
  esac
  exit 0
fi
if [ "$1" = app-server ]; then
  while IFS= read -r line; do
    id=$(printf '%s' "$line" | /usr/bin/sed -n 's/^{{"id":\([0-9]*\),.*/\1/p')
    [ -z "$id" ] && continue
    case "$line" in
      *'"method":"hooks/list"'*) printf '{{"id":%s,"result":%s}}\n' "$id" "$(/bin/cat '{answer}')" ;;
      *'"method":"config/batchWrite"'*) printf '{trust}' >> "$f"; printf '{{"id":%s,"result":{{"status":"ok"}}}}\n' "$id" ;;
      *) printf '{{"id":%s,"result":{{}}}}\n' "$id" ;;
    esac
  done
  exit 0
fi
case "$1" in
  --version) echo "codex-cli 0.157.1" ;;
  login) echo "Logged in using ChatGPT" >&2 ;;
esac"#,
        log = log.display(),
        config = config.display(),
        answer = answer.display(),
    );
    otaq_core::process::testing::stub_command(bin, "codex", &script);
}

async fn recheck(client: &Client) -> AgentToolsList {
    let request =
        serde_json::from_value(json!({"method": "agent_tools.recheck", "params": {}})).unwrap();
    match client.request(request).await.unwrap() {
        Reply::AgentTools(list) => *list,
        other => panic!("{other:?}"),
    }
}

fn app_servers(log: &Path) -> usize {
    std::fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .filter(|l| *l == "app-server")
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_check_repairs_a_missing_setup_and_leaves_a_complete_one_alone() {
    if std::env::var_os("CODEX_HOME").is_some() {
        // Codex's adapter would follow it out of the temporary home: never risk the user's real one.
        eprintln!("CODEX_HOME is set: skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (bin, home) = (dir.path().join("bin"), dir.path().join("home"));
    let codex_home = home.join(".codex");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(&codex_home).unwrap();
    let user = "# mine\nmodel = \"gpt-5.6-terra\"\n";
    std::fs::write(codex_home.join("config.toml"), user).unwrap();
    let log = dir.path().join("log");
    stand_in(&bin, &codex_home, &log);
    let plugin: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin");
    let (path, tool_home, bin_dir) = (bin.clone().into_os_string(), home.clone(), bin.clone());
    let w = TestService::start_with(move |mut config| {
        config.tools.path = path.clone();
        config.tool_home = Some(tool_home);
        config.launch = Some(LaunchContext {
            bin_dir,
            plugin_dir: plugin,
            path,
            claude: claude::ClaudeLaunch {
                program: vec!["claude".into()],
                config_dir: None,
            },
        });
        config
    })
    .await;
    let client = Client::connect(ClientConfig::new(&w.data)).await.unwrap();

    // Connecting runs the setup once.
    let rows = recheck(&client).await;
    let codex = rows.tools.iter().find(|t| t.tool == Tool::Codex).unwrap();
    assert!(codex.state.is_connected(), "{:?}", codex.state);
    let setup = codex.setup.as_ref().expect("the setup ran");
    assert_eq!(setup.action, SetupAction::SetUp);
    assert!(setup.failed.is_empty(), "{:?}", setup.failed);
    let complete = std::fs::read_to_string(codex_home.join("config.toml")).unwrap();
    assert!(complete.starts_with(user));
    assert_eq!(app_servers(&log), 1);

    // Complete: a check runs nothing.
    recheck(&client).await;
    assert_eq!(app_servers(&log), 1);

    // Otaq's hook trust removed (by hand, or a Codex reset): the next check sets it up again.
    let without_trust = complete.split("\n[hooks.state.").next().unwrap().to_owned();
    std::fs::write(codex_home.join("config.toml"), &without_trust).unwrap();
    let rows = recheck(&client).await;
    assert_eq!(app_servers(&log), 2);
    let codex = rows.tools.iter().find(|t| t.tool == Tool::Codex).unwrap();
    assert!(codex.setup.as_ref().unwrap().failed.is_empty());
    let repaired = std::fs::read_to_string(codex_home.join("config.toml")).unwrap();
    for event in EVENTS {
        assert!(repaired.contains(&hook_key(event)), "{repaired}");
    }
    recheck(&client).await;
    assert_eq!(app_servers(&log), 2);
    w.stop().await;
}
