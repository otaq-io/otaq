//! The release workflow's scripts (UPD-10, d0140), run here as the workflow runs them: `check-release.sh` (the
//! versions and the updater key, refused while it's the placeholder), `latest-json.mjs` (`latest.json` with
//! `protocol_version`, and `SHA256SUMS`) and `smoke.sh` (a built bundle's command, plugin folder and service, here on
//! a fake `Otaq.app` holding this build's `otaq`). Every run uses a temporary folder: a copy of the two version files,
//! made-up setup files, and a throwaway data folder and home for the service. Unix only (the scripts are bash; the
//! workflow runs them in Git Bash on Windows).

#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use otaq_core::protocol::PROTOCOL_VERSION;
use serde_json::Value;

fn script(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/release")
        .join(name)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A checkout with only the two files `check-release.sh` reads.
fn checkout(cargo_version: &str, tauri_version: &str, pubkey: Option<&str>) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("Cargo.toml"),
        format!(
            "[workspace]\nmembers = []\n\n[workspace.package]\nedition = \"2024\"\nversion = \"{cargo_version}\"\n\n\
             [workspace.dependencies]\nversion = \"9.9.9\"\n"
        ),
    )
    .unwrap();
    let mut conf = serde_json::json!({"productName": "x", "version": tauri_version});
    if let Some(key) = pubkey {
        conf["plugins"] = serde_json::json!({"updater": {"pubkey": key}});
    }
    let desktop = root.path().join("crates/otaq-desktop");
    std::fs::create_dir_all(desktop.join("src")).unwrap();
    std::fs::write(desktop.join("tauri.conf.json"), conf.to_string()).unwrap();
    app_key(root.path(), pubkey.unwrap_or_default());
    root
}

/// The app's own copy of the key, as `updates.rs` declares it.
fn app_key(root: &Path, key: &str) {
    std::fs::write(
        root.join("crates/otaq-desktop/src/updates.rs"),
        format!("/// The key.\npub const PUBKEY: &str =\n    \"{key}\";\n"),
    )
    .unwrap();
}

fn check(root: &Path, args: &[&str]) -> Output {
    Command::new(script("check-release.sh"))
        .args(args)
        .env("RELEASE_ROOT", root)
        .output()
        .unwrap()
}

const PLACEHOLDER: &str =
    "PLACEHOLDER — replaced by the maintainer before the first release (T-0583)";

/// What `tauri signer generate` writes to the `.pub` file: a minisign public key ("untrusted comment: minisign public
/// key: …" and the key line), base64 encoded again. Made up.
const REAL_LOOKING_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDZGMUIyQzNENEU1RjZBN0IKUldSN2FsOWVQVXdyRzI4QUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUEK";

#[test]
fn the_placeholder_key_stops_a_release_and_only_warns_a_dry_run() {
    let root = checkout("0.3.1", "0.3.1", Some(PLACEHOLDER));
    let release = check(root.path(), &["v0.3.1"]);
    assert!(!release.status.success(), "{release:?}");
    let why = text(&release.stderr);
    assert!(why.contains("still the placeholder"), "{why}");
    assert!(why.contains("no release is made"), "{why}");
    assert!(why.contains("TAURI_SIGNING_PRIVATE_KEY"), "{why}");
    assert!(release.stdout.is_empty());

    let dry = check(root.path(), &["--dry-run"]);
    assert!(dry.status.success(), "{dry:?}");
    assert_eq!(text(&dry.stdout).trim(), "0.3.1");
    assert!(text(&dry.stderr).contains("warning"), "{dry:?}");

    // No key at all, or something that isn't a minisign key, is refused the same way.
    let missing = checkout("0.3.1", "0.3.1", None);
    assert!(text(&check(missing.path(), &["v0.3.1"]).stderr).contains("has no updater public key"));
    let wrong = checkout("0.3.1", "0.3.1", Some("not a key"));
    assert!(text(&check(wrong.path(), &["v0.3.1"]).stderr).contains("isn't a minisign public key"));
}

#[test]
fn the_tag_and_both_versions_must_agree() {
    let key = REAL_LOOKING_KEY;
    let good = checkout("0.3.1", "0.3.1", Some(key));
    let out = check(good.path(), &["v0.3.1"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(text(&out.stdout).trim(), "0.3.1");
    assert!(out.stderr.is_empty(), "{out:?}");

    // The app's copy must be the same key.
    let differ = checkout("0.3.1", "0.3.1", Some(key));
    app_key(differ.path(), PLACEHOLDER);
    let out = check(differ.path(), &["v0.3.1"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("differs between crates/otaq-desktop/src/updates.rs (PUBKEY)"),
        "{out:?}"
    );

    let tag = check(good.path(), &["v0.3.2"]);
    assert!(!tag.status.success());
    assert!(
        text(&tag.stderr).contains("the tag v0.3.2 doesn't match the version 0.3.1"),
        "{tag:?}"
    );

    let differ = checkout("0.3.1", "0.3.0", Some(key));
    let out = check(differ.path(), &["--dry-run"]);
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("Cargo.toml's [workspace.package] says 0.3.1, crates/otaq-desktop/tauri.conf.json says 0.3.0"),
        "{out:?}"
    );
}

/// The release's files: each updater file with its signature, the `.dmg` and the `.deb`.
fn release_files(dir: &Path) {
    for name in [
        "Otaq_0.3.1_universal.app.tar.gz",
        "Otaq_0.3.1_x64-setup.exe",
        "Otaq_0.3.1_arm64-setup.exe",
        "Otaq_0.3.1_amd64.AppImage",
        "Otaq_0.3.1_aarch64.AppImage",
    ] {
        std::fs::write(dir.join(name), format!("bytes of {name}")).unwrap();
        std::fs::write(dir.join(format!("{name}.sig")), format!("sig of {name}\n")).unwrap();
    }
    std::fs::write(dir.join("Otaq_0.3.1_universal.dmg"), "dmg").unwrap();
    std::fs::write(dir.join("Otaq_0.3.1_amd64.deb"), "deb").unwrap();
}

fn latest_json(dir: &Path) -> Output {
    Command::new("node")
        .arg(script("latest-json.mjs"))
        .arg(dir)
        .args(["v0.3.1", "otaq-io/example"])
        .output()
        .unwrap()
}

#[test]
fn latest_json_names_every_platform_and_the_protocol_number() {
    let dir = tempfile::tempdir().unwrap();
    release_files(dir.path());
    let out = latest_json(dir.path());
    assert!(out.status.success(), "{out:?}");
    let latest: Value =
        serde_json::from_str(&std::fs::read_to_string(dir.path().join("latest.json")).unwrap())
            .unwrap();
    assert_eq!(latest["version"], "0.3.1");
    assert_eq!(latest["protocol_version"], PROTOCOL_VERSION);
    assert_eq!(
        latest["notes_url"],
        "https://github.com/otaq-io/example/releases/tag/v0.3.1"
    );
    assert!(latest["pub_date"].as_str().unwrap().ends_with('Z'));
    let platforms = latest["platforms"].as_object().unwrap();
    let mut keys: Vec<&str> = platforms.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        [
            "darwin-aarch64",
            "darwin-universal",
            "darwin-x86_64",
            "linux-aarch64",
            "linux-x86_64",
            "windows-aarch64",
            "windows-x86_64"
        ]
    );
    assert_eq!(
        platforms["windows-aarch64"],
        serde_json::json!({
            "signature": "sig of Otaq_0.3.1_arm64-setup.exe",
            "url": "https://github.com/otaq-io/example/releases/download/v0.3.1/Otaq_0.3.1_arm64-setup.exe"
        })
    );
    assert_eq!(
        platforms["darwin-x86_64"]["url"], platforms["darwin-aarch64"]["url"],
        "one universal archive"
    );

    // SHA256SUMS: every file, latest.json included, itself not.
    let sums = std::fs::read_to_string(dir.path().join("SHA256SUMS")).unwrap();
    let listed: Vec<&str> = sums
        .lines()
        .map(|l| l.split_once("  ").unwrap().1)
        .collect();
    assert_eq!(listed.len(), 13, "{sums}");
    assert!(listed.contains(&"latest.json") && listed.contains(&"Otaq_0.3.1_amd64.deb"));
    assert!(!listed.contains(&"SHA256SUMS"));
    let check = Command::new("shasum")
        .args(["-a", "256", "-c", "SHA256SUMS"])
        .current_dir(dir.path())
        .output();
    if let Ok(check) = check {
        assert!(check.status.success(), "{check:?}");
    }
}

#[test]
fn latest_json_refuses_an_unsigned_updater_file() {
    let dir = tempfile::tempdir().unwrap();
    release_files(dir.path());
    std::fs::remove_file(dir.path().join("Otaq_0.3.1_amd64.AppImage.sig")).unwrap();
    let out = latest_json(dir.path());
    assert!(!out.status.success());
    assert!(
        text(&out.stderr).contains("Otaq_0.3.1_amd64.AppImage has no signature"),
        "{out:?}"
    );
    assert!(!dir.path().join("latest.json").exists());
}

fn smoke(kind: &str, bundle: &Path, version: &str, tmp: &Path) -> Output {
    Command::new(script("smoke.sh"))
        .args([kind])
        .arg(bundle)
        .arg(version)
        .env("TMPDIR", tmp)
        .env_remove("RUNNER_TEMP")
        // No agent tool is found: the service only checks for them.
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .output()
        .unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

/// An `Otaq.app` laid out as the bundler makes it, with this build's `otaq` and the repository's plugin folder.
fn fake_app(dir: &Path) -> PathBuf {
    let app = dir.join("Otaq.app");
    let macos = app.join("Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_otaq"), macos.join("otaq")).unwrap();
    copy_dir(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../plugin"),
        &app.join("Contents/Resources/plugin"),
    );
    app
}

#[test]
fn the_smoke_check_passes_a_whole_bundle_and_stops_its_service() {
    let dir = tempfile::tempdir().unwrap();
    let app = fake_app(dir.path());
    let tmp = dir.path().join("t");
    std::fs::create_dir(&tmp).unwrap();
    let out = smoke("app", &app, env!("CARGO_PKG_VERSION"), &tmp);
    assert!(out.status.success(), "{out:?}");
    let said = text(&out.stdout);
    assert!(said.contains("It stops"), "{said}");
    assert!(said.contains("smoke.sh: app passed"), "{said}");
}

#[test]
fn the_smoke_check_refuses_a_bundle_without_its_plugin_folder_or_version() {
    let dir = tempfile::tempdir().unwrap();
    let app = fake_app(dir.path());
    let tmp = dir.path().join("t");
    std::fs::create_dir(&tmp).unwrap();

    let wrong = smoke("app", &app, "9.9.9", &tmp);
    assert!(!wrong.status.success());
    assert!(text(&wrong.stderr).contains("not 9.9.9"), "{wrong:?}");

    std::fs::remove_dir_all(app.join("Contents/Resources/plugin")).unwrap();
    let out = smoke("app", &app, env!("CARGO_PKG_VERSION"), &tmp);
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("no plugin folder"), "{out:?}");

    let unknown = smoke("zip", &app, "", &tmp);
    assert!(!unknown.status.success());
    assert!(
        text(&unknown.stderr).contains("unknown kind zip"),
        "{unknown:?}"
    );
}
