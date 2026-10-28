//! Tracker plugins through the service (the otaq-marketplace README): a Local GitHub Issues plugin run against a stand-in
//! `gh` that prints fixture JSON. Consent, project settings, search (with epics and closed issues left out), import,
//! refresh with its three-way merge, dismissed issues and Pull now.

use std::path::PathBuf;

use otaq_core::client::request_once;
use otaq_core::plugins::registry::{Pick, RefreshField};
use otaq_core::protocol::{ErrorBody, Reply, Request};
use otaq_core::service::testing::TestService;
use serde_json::json;

const MANIFEST: &str = r##"
id: github-issues
name: GitHub Issues
version: 1.0.0
protocol: 1
otaq: ">=0.1.0, <2.0.0"
license: MIT
provides: [ticket_source]
capabilities: [import, refresh, auto_pull]
requires:
  - id: gh
    name: GitHub CLI
    check: [gh, --version]
    login_check: [gh, auth, status]
    login: [gh, auth, login]
settings:
  - { key: repo, label: Repository, required: true, format: owner_repo, scope: project }
  - { key: query, label: Auto-pull search, default: "is:open" }
types:
  containers: [epic]
issue_url: "https://github.com/{settings.repo}/issues/{id}"
commands:
  search:
    run: [gh, issue, list, -R, "{settings.repo}", --search, "{query}", --limit, "{limit}", --json, "number,title,state,labels"]
    map:
      items: "$"
      id: number
      key: "#{number}"
      title: title
      state: state
      type: { from: "labels[].name", via: { epic: epic } }
  fetch:
    run: [gh, issue, view, "{id}", -R, "{settings.repo}", --json, "number,title,body,state,labels"]
    map:
      item: "$"
      id: number
      key: "#{number}"
      title: title
      body: body
      state: state
      type: { from: "labels[].name", via: { epic: epic } }
"##;

struct World {
    w: TestService,
    fixtures: PathBuf,
    folder: PathBuf,
}

async fn world() -> World {
    let tmp = tempfile::tempdir().unwrap().keep();
    let bin = tmp.join("bin");
    let fixtures = tmp.join("fixtures");
    let folder = tmp.join("github-issues");
    for dir in [&bin, &fixtures, &folder] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(folder.join("plugin.yaml"), MANIFEST).unwrap();
    std::fs::write(folder.join("LICENSE"), "MIT License").unwrap();
    // `gh issue list …` prints list.json; `gh issue view 12 …` prints view-12.json; each call is logged. Only shell
    // built-ins: the service's PATH holds just this folder.
    let script = format!(
        r#"echo "$@" >> '{log}'
show() {{ while IFS= read -r line || [ -n "$line" ]; do printf '%s\n' "$line"; done < "$1"; }}
case "$1 $2" in
  "issue list") show '{dir}/list.json';;
  "issue view") show "{dir}/view-$3.json";;
  *) exit 0;;
esac"#,
        log = fixtures.join("calls.log").display(),
        dir = fixtures.display()
    );
    otaq_core::process::testing::stub_command(&bin, "gh", &script);
    let path = bin.into_os_string();
    let w = TestService::start_with(move |mut c| {
        c.tools.path = path;
        c
    })
    .await;
    w.add_project("shop-web").await;
    World {
        w,
        fixtures,
        folder,
    }
}

impl World {
    async fn ask(&self, request: Request) -> Result<Reply, ErrorBody> {
        match request_once(&self.w.data.socket_path(), request).await {
            Ok(reply) => Ok(reply),
            Err(otaq_core::client::ClientError::Service(e)) => Err(e),
            Err(e) => panic!("{e}"),
        }
    }

    fn issues(&self, list: serde_json::Value) {
        std::fs::write(self.fixtures.join("list.json"), list.to_string()).unwrap();
    }

    fn issue(&self, number: u32, title: &str, body: &str) {
        let issue =
            json!({"number": number, "title": title, "body": body, "state": "OPEN", "labels": []});
        std::fs::write(
            self.fixtures.join(format!("view-{number}.json")),
            issue.to_string(),
        )
        .unwrap();
    }

    fn calls(&self) -> String {
        std::fs::read_to_string(self.fixtures.join("calls.log")).unwrap_or_default()
    }

    async fn install(&self) {
        let folder = self.folder.display().to_string();
        let Ok(Reply::PluginConsent(consent)) = self
            .ask(Request::PluginsLocal(
                otaq_core::plugins::params::LocalFolderParams {
                    folder: folder.clone(),
                    digest: None,
                },
            ))
            .await
        else {
            panic!("no consent screen")
        };
        assert!(
            consent
                .commands
                .iter()
                .any(|c| c.starts_with("gh issue list"))
        );
        assert!(consent.warnings.is_empty(), "{:?}", consent.warnings);
        let Ok(Reply::Plugins(view)) = self
            .ask(Request::PluginsLocal(
                otaq_core::plugins::params::LocalFolderParams {
                    folder,
                    digest: Some(consent.digest.clone()),
                },
            ))
            .await
        else {
            panic!("not installed")
        };
        assert_eq!(view.installed[0].id, "github-issues");
        let enabled = self
            .ask(Request::ProjectPluginsSet(
                otaq_core::plugins::params::ProjectPluginSetParams {
                    project: "SHOP".into(),
                    plugin: "github-issues".into(),
                    enabled: true,
                    values: json!({"repo": "acme/shop"}).as_object().unwrap().clone(),
                    auto_pull: false,
                    pull_minutes: 15,
                },
            ))
            .await;
        assert!(enabled.is_ok(), "{enabled:?}");
    }

    async fn search(&self) -> otaq_core::plugins::registry::SearchResult {
        match self
            .ask(Request::PluginsSearch(
                otaq_core::plugins::params::SearchParams {
                    project: "SHOP".into(),
                    plugin: "github-issues".into(),
                    query: Some("is:open label:\"good first\"".into()),
                },
            ))
            .await
        {
            Ok(Reply::PluginSearch(result)) => *result,
            other => panic!("{other:?}"),
        }
    }

    async fn import(&self, ids: &[&str]) -> otaq_core::plugins::registry::Imported {
        match self
            .ask(Request::PluginsImport(
                otaq_core::plugins::params::ImportParams {
                    project: "SHOP".into(),
                    plugin: "github-issues".into(),
                    ids: ids.iter().map(|s| (*s).to_owned()).collect(),
                },
            ))
            .await
        {
            Ok(Reply::PluginImported(imported)) => *imported,
            other => panic!("{other:?}"),
        }
    }
}

fn three_issues() -> serde_json::Value {
    json!([
        {"number": 12, "title": "Fix login", "state": "OPEN", "labels": []},
        {"number": 13, "title": "Add SSO", "state": "OPEN", "labels": [{"name": "bug"}]},
        {"number": 14, "title": "Checkout epic", "state": "OPEN", "labels": [{"name": "epic"}]},
        {"number": 15, "title": "Old", "state": "CLOSED", "labels": []}
    ])
}

#[tokio::test(flavor = "multi_thread")]
async fn search_import_and_dismiss() {
    let world = world().await;
    world.install().await;
    world.issues(three_issues());
    world.issue(12, "Fix login", "Users can't log in.");
    world.issue(13, "Add SSO", "SAML.");

    let found = world.search().await;
    let keys: Vec<&str> = found.hits.iter().map(|h| h.issue.key.as_str()).collect();
    assert_eq!(
        keys,
        ["#12", "#13"],
        "the epic and the closed issue are left out"
    );
    assert_eq!(found.hidden, 2);
    assert!(!found.truncated);
    assert!(
        world
            .calls()
            .contains("--search is:open label:\"good first\" --limit 100"),
        "the query is one argument: {}",
        world.calls()
    );

    let imported = world.import(&["12"]).await;
    assert_eq!(imported.tickets.len(), 1);
    let ticket = &imported.tickets[0];
    assert_eq!(ticket.title, "Fix login");
    assert_eq!(ticket.body, "Users can't log in.");
    assert_eq!(ticket.external_id.as_deref(), Some("#12"));

    let again = world.import(&["12"]).await;
    assert!(again.tickets.is_empty(), "already imported");
    assert_eq!(again.skipped.len(), 1);
    std::fs::write(
        world.fixtures.join("view-14.json"),
        json!({"number": 14, "title": "Checkout epic", "body": "", "state": "OPEN", "labels": [{"name": "epic"}]})
            .to_string(),
    )
    .unwrap();
    let epic = world.import(&["14"]).await;
    assert!(epic.tickets.is_empty(), "an epic isn't imported");

    let Ok(Reply::TrackerLinks(links)) = world
        .ask(Request::PluginsLinks(otaq_core::protocol::ProjectParams {
            project: "SHOP".into(),
        }))
        .await
    else {
        panic!()
    };
    assert_eq!(links[0].url, "https://github.com/acme/shop/issues/12");
    assert_eq!(links[0].tracker, "GitHub Issues");

    let found = world.search().await;
    assert_eq!(found.hits[0].ticket.as_ref(), Some(&ticket.key));

    // Deleted: the issue is dismissed, and Pull now never brings it back.
    world
        .w
        .board
        .delete_ticket(&ticket.key.to_string())
        .await
        .unwrap();
    let found = world.search().await;
    assert!(found.hits[0].dismissed);
    let Ok(Reply::PluginImported(pulled)) = world
        .ask(Request::PluginsPullNow(
            otaq_core::plugins::params::ProjectPluginParams {
                project: "SHOP".into(),
                plugin: "github-issues".into(),
            },
        ))
        .await
    else {
        panic!()
    };
    let titles: Vec<&str> = pulled.tickets.iter().map(|t| t.title.as_str()).collect();
    assert_eq!(titles, ["Add SSO"], "only the new open work item");

    // Picked again by hand, a dismissed issue is imported.
    let back = world.import(&["12"]).await;
    assert_eq!(back.tickets.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn refresh_merges_field_by_field() {
    let world = world().await;
    world.install().await;
    world.issues(three_issues());
    world.issue(12, "Fix login", "Users can't log in.");
    let ticket = world.import(&["12"]).await.tickets.remove(0);
    let key = ticket.key.to_string();

    // Only the tracker changed the title: it updates silently.
    world.issue(12, "Fix login on Safari", "Users can't log in.");
    let Ok(Reply::TrackerRefresh(outcome)) = world
        .ask(Request::TicketTrackerRefresh(
            otaq_core::protocol::KeyParams { key: key.clone() },
        ))
        .await
    else {
        panic!()
    };
    assert_eq!(outcome.updated, [RefreshField::Title]);
    assert!(outcome.conflicts.is_empty());
    assert_eq!(
        world.w.board.ticket(&key).await.unwrap().title,
        "Fix login on Safari"
    );

    // Both changed the body: a conflict, nothing overwritten.
    world
        .w
        .board
        .update_ticket(
            &key,
            &otaq_core::board::tickets::TicketUpdate {
                body: Some("Mine.".into()),
                ..Default::default()
            },
            &otaq_core::domain::Actor::current_user(),
        )
        .await
        .unwrap();
    world.issue(12, "Fix login on Safari", "Theirs.");
    let Ok(Reply::TrackerRefresh(outcome)) = world
        .ask(Request::TicketTrackerRefresh(
            otaq_core::protocol::KeyParams { key: key.clone() },
        ))
        .await
    else {
        panic!()
    };
    assert_eq!(outcome.conflicts.len(), 1);
    assert_eq!(outcome.conflicts[0].local, "Mine.");
    assert_eq!(world.w.board.ticket(&key).await.unwrap().body, "Mine.");

    let resolved = world
        .ask(Request::TicketTrackerResolve(
            otaq_core::plugins::params::ResolveParams {
                key: key.clone(),
                picks: [("body".to_owned(), Pick::Tracker)].into(),
            },
        ))
        .await;
    assert!(resolved.is_ok(), "{resolved:?}");
    assert_eq!(world.w.board.ticket(&key).await.unwrap().body, "Theirs.");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_changed_local_folder_asks_again() {
    let world = world().await;
    world.install().await;
    world.issues(three_issues());
    // A new command line since consent: refused until the folder is added again.
    let changed = MANIFEST.replace("--limit, \"{limit}\"", "--limit, \"{limit}\", --web");
    std::fs::write(world.folder.join("plugin.yaml"), changed).unwrap();
    let e = world
        .ask(Request::PluginsSearch(
            otaq_core::plugins::params::SearchParams {
                project: "SHOP".into(),
                plugin: "github-issues".into(),
                query: None,
            },
        ))
        .await
        .unwrap_err();
    assert!(e.message.contains("commands changed"), "{}", e.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_binary_in_a_local_folder_is_a_warning() {
    let world = world().await;
    std::fs::write(world.folder.join("helper"), [0xcf, 0xfa, 0xed, 0xfe, 0x07]).unwrap();
    let Ok(Reply::PluginConsent(consent)) = world
        .ask(Request::PluginsLocal(
            otaq_core::plugins::params::LocalFolderParams {
                folder: world.folder.display().to_string(),
                digest: None,
            },
        ))
        .await
    else {
        panic!()
    };
    assert!(
        consent.warnings.iter().any(|w| w.contains("Mach-O")),
        "{:?}",
        consent.warnings
    );
}

fn release(manifest: &str, extra: Option<(&str, &str)>) -> (Vec<u8>, String) {
    let mut files = vec![
        otaq_core::plugins::bundle::BundleFile {
            path: "plugin.yaml".into(),
            text: manifest.into(),
        },
        otaq_core::plugins::bundle::BundleFile {
            path: "LICENSE".into(),
            text: "MIT".into(),
        },
    ];
    if let Some((path, text)) = extra {
        files.push(otaq_core::plugins::bundle::BundleFile {
            path: path.into(),
            text: text.into(),
        });
    }
    otaq_core::plugins::bundle::Bundle {
        format: 1,
        id: "github-issues".into(),
        version: "1.0.0".into(),
        files,
        not_text: Vec::new(),
    }
    .to_release()
}

fn index(base: &str, sha256: &str) -> Vec<u8> {
    json!({"schema": 1, "kind": "plugins", "plugins": [{
        "id": "github-issues", "name": "GitHub Issues", "license": "MIT",
        "versions": [{"version": "1.0.0", "protocol": 1, "otaq": ">=0.1.0, <2.0.0",
                      "url": format!("{base}/releases/github-issues-1.0.0.json"), "sha256": sha256,
                      "tag": "plugins/github-issues-v1.0.0"}]
    }]})
    .to_string()
    .into_bytes()
}

#[tokio::test(flavor = "multi_thread")]
async fn installs_from_a_marketplace_after_consent() {
    let (bytes, sha) = release(MANIFEST, None);
    let base = serve_self_indexed(bytes.clone(), sha.clone()).await;
    let w = TestService::start_with(move |mut c| {
        c.plugin_marketplace = base;
        c
    })
    .await;
    let ask = |r: Request| {
        let socket = w.data.socket_path();
        async move { request_once(&socket, r).await }
    };
    let Ok(Reply::Plugins(view)) = ask(Request::PluginsList).await else {
        panic!()
    };
    assert_eq!(view.available.len(), 1, "{view:?}");
    let market = view.available[0].marketplace.clone();
    let Ok(Reply::PluginConsent(consent)) = ask(Request::PluginsConsent(
        otaq_core::plugins::params::ConsentParams {
            marketplace: market.clone(),
            id: "github-issues".into(),
            version: None,
        },
    ))
    .await
    else {
        panic!()
    };
    assert_eq!(consent.label, otaq_core::plugins::registry::Label::Official);
    let wrong = ask(Request::PluginsInstall(
        otaq_core::plugins::params::InstallParams {
            marketplace: market.clone(),
            id: "github-issues".into(),
            version: "1.0.0".into(),
            digest: "not it".into(),
        },
    ))
    .await;
    assert!(
        wrong.is_err(),
        "a digest that isn't the consent's is refused"
    );
    let Ok(Reply::Plugins(view)) = ask(Request::PluginsInstall(
        otaq_core::plugins::params::InstallParams {
            marketplace: market,
            id: "github-issues".into(),
            version: "1.0.0".into(),
            digest: consent.digest,
        },
    ))
    .await
    else {
        panic!()
    };
    assert_eq!(view.installed[0].version, "1.0.0");
    assert!(
        w.data
            .plugin_path("github-issues")
            .unwrap()
            .join("plugin.yaml")
            .is_file()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_marketplace_release_with_a_script_is_refused() {
    let (bytes, sha) = release(MANIFEST, Some(("scripts/fetch.js", "console.log(1)")));
    let base = serve_self_indexed(bytes, sha).await;
    let w = TestService::start_with(move |mut c| {
        c.plugin_marketplace = base;
        c
    })
    .await;
    let socket = w.data.socket_path();
    let Ok(Reply::Plugins(view)) = request_once(&socket, Request::PluginsList).await else {
        panic!()
    };
    let e = request_once(
        &socket,
        Request::PluginsConsent(otaq_core::plugins::params::ConsentParams {
            marketplace: view.available[0].marketplace.clone(),
            id: "github-issues".into(),
            version: None,
        }),
    )
    .await
    .unwrap_err();
    assert!(e.to_string().contains("open-source rules"), "{e}");
}

/// Serves `plugins/index.json` (pointing at its own address) and the release.
async fn serve_self_indexed(bytes: Vec<u8>, sha: String) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let files = vec![
        ("/plugins/index.json".to_owned(), index(&base, &sha)),
        ("/releases/github-issues-1.0.0.json".to_owned(), bytes),
    ];
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let files = files.clone();
            tokio::spawn(async move {
                let mut buf = vec![0; 4096];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]);
                let path = request.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, body) = match files.iter().find(|(p, _)| *p == path) {
                    Some((_, body)) => ("200 OK", body.clone()),
                    None => ("404 Not Found", Vec::new()),
                };
                let head = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes()).await;
                let _ = stream.write_all(&body).await;
            });
        }
    });
    base
}

/// Every plugin of a marketplace checkout (`OTAQ_MARKETPLACE=../otaq-marketplace`) reads, keeps the open-source rules,
/// and maps its recorded CLI output (`tests/fixtures/search.json`, `fetch.json`, with `settings.json`) to the issues
/// it expects (`*.expected.json`). The marketplace's CI runs it; skipped when the variable isn't set.
#[test]
fn marketplace_plugins_map_their_fixtures() {
    use otaq_core::plugins::{bundle::Bundle, mapping, values};
    let Some(root) = std::env::var_os("OTAQ_MARKETPLACE") else {
        eprintln!("OTAQ_MARKETPLACE isn't set: nothing to check");
        return;
    };
    let plugins = PathBuf::from(root).join("plugins");
    let mut checked = 0;
    for entry in std::fs::read_dir(&plugins).unwrap() {
        let dir = entry.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        let name = dir.file_name().unwrap().to_string_lossy().into_owned();
        let bundle = Bundle::from_dir(&dir).unwrap_or_else(|e| panic!("{name}: {e}"));
        let manifest = bundle.manifest().unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(manifest.id, name, "the id is the folder's name");
        let problems = bundle.open_source_problems(&manifest);
        assert!(problems.is_empty(), "{name}: {problems:?}");
        let fixtures = dir.join("tests").join("fixtures");
        let read = |file: &str| -> serde_json::Value {
            let text = std::fs::read_to_string(fixtures.join(file))
                .unwrap_or_else(|e| panic!("{name}/{file}: {e}"));
            serde_json::from_str(&text).unwrap()
        };
        let own = read("settings.json").as_object().unwrap().clone();
        let settings = values::effective(&manifest, &serde_json::Map::new(), &own);
        let found = mapping::search_results(&manifest, &read("search.json"), &settings).unwrap();
        assert_eq!(
            serde_json::to_value(&found).unwrap(),
            read("search.expected.json"),
            "{name}: search"
        );
        let fetched = mapping::fetched(&manifest, &read("fetch.json"), &settings).unwrap();
        assert_eq!(
            serde_json::to_value(&fetched).unwrap(),
            read("fetch.expected.json"),
            "{name}: fetch"
        );
        checked += 1;
    }
    assert!(checked > 0, "no plugins under {}", plugins.display());
}

/// An official marketplace stored by an older build at another address follows this build's (the repository moved).
#[tokio::test(flavor = "multi_thread")]
async fn the_official_marketplace_follows_its_new_address() {
    let w = TestService::start().await;
    let store = w.board.store();
    store
        .add_plugin_marketplace("https://github.com/old-owner/marketplace", "Old", true)
        .await
        .unwrap();
    let Ok(Reply::Plugins(view)) = request_once(&w.data.socket_path(), Request::PluginsList).await
    else {
        panic!()
    };
    let official: Vec<_> = view.marketplaces.iter().filter(|m| m.official).collect();
    assert_eq!(official.len(), 1);
    assert_eq!(official[0].url, otaq_core::service::NO_MARKETPLACE);
}
