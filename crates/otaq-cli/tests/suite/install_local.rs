//! `scripts/install-local.sh` (T-0370, T-0578, d0140) on a fake build: what the script itself does to a built
//! `Otaq.app` (Tauri's bundler makes the layout): the bundle ID, `otaq` signed under it, the bundle sealed again,
//! and the old bundle's service stopped only when it runs from that bundle. The fake programs are copies of
//! `/usr/bin/true` and a tiny C program compiled here, so no Rust build runs here; the real build is the hand-run script. `--dest` installs
//! into a temporary folder, and `HOME` and `OTAQ_HOME` point into it too, so nothing reaches the user's folders or
//! service. macOS only (the script builds `Otaq.app`).

#![cfg(target_os = "macos")]

use std::path::{Path, PathBuf};
use std::process::Command;

use otaq_core::attention::bundle;
use otaq_core::data_dir::DataDir;
use otaq_core::naming;

const CHECK_ID: &str = "io.example.check";

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/install-local.sh")
}

fn plist_value(plist: &Path, key: &str) -> String {
    let out = Command::new("plutil")
        .args(["-extract", key, "raw", "-o", "-"])
        .arg(plist)
        .output()
        .unwrap();
    assert!(out.status.success(), "{key}: {out:?}");
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}

/// A built `Otaq.app` as Tauri lays it out, with fake programs: `otaq` is a script that records `daemon stop`.
fn fake_build(tmp: &Path) -> PathBuf {
    let build = tmp.join("build");
    let contents = build.join(naming::bundle_folder_name()).join("Contents");
    let macos = contents.join("MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    std::fs::create_dir_all(contents.join("Resources/plugin")).unwrap();
    std::fs::copy("/usr/bin/true", macos.join(naming::app_binary_name())).unwrap();
    // The fake service: a program of its own, never a copy of a system one (macOS kills a copy of an Apple program
    // run from elsewhere, at some point after it starts: launch constraints).
    let mut cc = Command::new("cc")
        .args(["-x", "c", "-o"])
        .arg(macos.join("sleeper"))
        .arg("-")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        cc.stdin
            .take()
            .unwrap()
            .write_all(b"#include <unistd.h>\nint main(void) { for (;;) pause(); }\n")
            .unwrap();
    }
    assert!(cc.wait().unwrap().success(), "cc");
    let otaq = macos.join(naming::COMMAND_NAME);
    std::fs::write(
        &otaq,
        format!(
            "#!/bin/sh\necho \"$* $OTAQ_HOME\" >> {}\n",
            tmp.join("stops").display()
        ),
    )
    .unwrap();
    Command::new("chmod").arg("+x").arg(&otaq).status().unwrap();
    std::fs::write(
        contents.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>{}</string>
<key>CFBundleIdentifier</key><string>{}</string>
<key>CFBundleName</key><string>{}</string>
<key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"#,
            naming::app_binary_name(),
            naming::BUNDLE_ID,
            naming::PRODUCT_NAME
        ),
    )
    .unwrap();
    build
}

/// Installs the fake build into `<tmp>/dest` with `bundle_id`; returns the script's stdout.
fn install(tmp: &Path, bundle_id: &str) -> String {
    let build = fake_build(tmp);
    let out = Command::new("bash")
        .arg(script())
        .args(["--from".as_ref(), build.as_os_str()])
        .args(["--dest".as_ref(), tmp.join("dest").as_os_str()])
        .args(["--bundle-id", bundle_id])
        .env("HOME", tmp.join("home"))
        .env(DataDir::env_var(), tmp.join("data"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn the_bundle_gets_its_id_and_signature() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path().canonicalize().unwrap();
    let id = format!("{}.check", naming::BUNDLE_ID);
    install(&tmp, &id);

    let app = tmp.join("dest").join(naming::bundle_folder_name());
    let macos = app.join("Contents").join("MacOS");
    assert_eq!(
        plist_value(&app.join("Contents/Info.plist"), "CFBundleIdentifier"),
        id
    );
    let verify = Command::new("codesign")
        .args(["--verify", "--deep", "--strict"])
        .arg(&app)
        .output()
        .unwrap();
    assert!(verify.status.success(), "{verify:?}");
    let details = Command::new("codesign")
        .args(["--display", "--verbose=2"])
        .arg(macos.join(naming::COMMAND_NAME))
        .output()
        .unwrap();
    let details = String::from_utf8_lossy(&details.stderr);
    assert!(details.contains(&format!("Identifier={id}\n")), "{details}");
    assert!(details.contains("Signature=adhoc"), "{details}");
    assert_eq!(
        bundle::bundle_of(&macos.join(naming::COMMAND_NAME)).map(|b| b.id),
        Some(id)
    );
    assert!(!tmp.join("home/.local").exists(), "no link in ~/.local/bin");
}

#[test]
fn only_a_service_running_from_the_old_bundle_is_stopped() {
    let tmp = tempfile::tempdir().unwrap();
    let tmp = tmp.path().canonicalize().unwrap();
    let first = install(&tmp, CHECK_ID);
    assert!(!first.contains("Stopping"), "{first}");
    // The data folder's lock names a process that isn't the bundle's (a stale PID): nothing is stopped.
    let data = tmp.join("data");
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(
        data.join("service.lock"),
        format!("{}\n", std::process::id()),
    )
    .unwrap();
    let second = install(&tmp, CHECK_ID);
    assert!(!second.contains("Stopping"), "{second}");
    assert!(!tmp.join("stops").exists());

    // A service of this data folder running from the installed bundle: stopped with that data folder.
    let app = tmp.join("dest").join(naming::bundle_folder_name());
    let mut service = Command::new(app.join("Contents/MacOS/sleeper"))
        .spawn()
        .unwrap();
    std::fs::write(data.join("service.lock"), format!("{}\n", service.id())).unwrap();
    let third = install(&tmp, CHECK_ID);
    assert!(
        service.try_wait().unwrap().is_none(),
        "the fake service ran throughout"
    );
    let _ = service.kill();
    let _ = service.wait();
    assert!(third.contains("Stopping the background service"), "{third}");
    assert_eq!(
        std::fs::read_to_string(tmp.join("stops")).unwrap(),
        format!("daemon stop {}\n", data.display())
    );
    let left: Vec<_> = std::fs::read_dir(tmp.join("dest"))
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        left,
        [naming::bundle_folder_name().as_str()],
        "no staging folder left"
    );
}
