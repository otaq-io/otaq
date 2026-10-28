//! Every plugin is open source, and all of it can be read (the otaq-marketplace README, "Trust and safety"):
//! an OSI-approved or FSL license, a `LICENSE` file, and only text files. Protocol 1 plugins run no code of their
//! own, so a plugin holds its manifest, docs, license and test fixtures and nothing else.
//!
//! The marketplace's CI applies the same list (`plugins/plugin.schema.json`). Otaq refuses an Official or Community
//! plugin that breaks a rule; a Local folder gets the problems as warnings.

/// The licenses a plugin may use: OSI-approved SPDX ids, and FSL.
pub const LICENSES: &[&str] = &[
    "MIT",
    "MIT-0",
    "Apache-2.0",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "0BSD",
    "Zlib",
    "MPL-2.0",
    "LGPL-2.1-only",
    "LGPL-2.1-or-later",
    "LGPL-3.0-only",
    "LGPL-3.0-or-later",
    "GPL-2.0-only",
    "GPL-2.0-or-later",
    "GPL-3.0-only",
    "GPL-3.0-or-later",
    "AGPL-3.0-only",
    "AGPL-3.0-or-later",
    "EPL-2.0",
    "EUPL-1.2",
    "Unlicense",
    "FSL-1.1-MIT",
    "FSL-1.1-ALv2",
    "FSL-1.1-Apache-2.0",
];

/// The files a protocol 1 plugin may hold, by extension (or a known name without one).
const TEXT_EXTENSIONS: &[&str] = &["yaml", "yml", "md", "txt", "json"];
const PLAIN_NAMES: &[&str] = &["LICENSE", "NOTICE", "COPYING", "AUTHORS"];

/// The largest file a plugin may hold.
pub const MAX_FILE_BYTES: usize = 1024 * 1024;

/// Whether `id` is on [`LICENSES`] (ignoring case, as SPDX does).
pub fn allowed_license(id: &str) -> bool {
    LICENSES.iter().any(|l| l.eq_ignore_ascii_case(id.trim()))
}

/// What breaks the rules in a plugin's files (paths relative to its folder, `/`-separated). Empty when it keeps
/// them all.
pub fn problems<'a>(
    license: &str,
    files: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Vec<String> {
    let mut problems = Vec::new();
    if license.trim().is_empty() {
        problems.push("The manifest names no license".to_owned());
    } else if !allowed_license(license) {
        problems.push(format!("{license} isn't an OSI-approved or FSL license"));
    }
    let mut has_license_file = false;
    for (path, bytes) in files {
        let name = path.rsplit('/').next().unwrap_or(path);
        if path.split('/').count() == 1 && license_file(name) {
            has_license_file = true;
        }
        if let Some(kind) = native_executable(bytes) {
            problems.push(format!(
                "{path} is a {kind} program: plugins contain no binaries"
            ));
        } else if std::str::from_utf8(bytes).is_err() {
            problems.push(format!("{path} isn't text: plugins contain no binaries"));
        } else if !text_file(name) {
            problems.push(format!(
                "{path}: a plugin holds only its manifest, docs, license and test fixtures \
                 (.yaml, .md, .txt, .json)"
            ));
        }
        if bytes.len() > MAX_FILE_BYTES {
            problems.push(format!("{path} is over 1 MB"));
        }
    }
    if !has_license_file {
        problems.push("The plugin has no LICENSE file".to_owned());
    }
    problems
}

fn license_file(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    ["LICENSE", "LICENCE", "COPYING"]
        .iter()
        .any(|l| l.eq_ignore_ascii_case(stem))
}

fn text_file(name: &str) -> bool {
    match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => {
            TEXT_EXTENSIONS.iter().any(|e| e.eq_ignore_ascii_case(ext))
        }
        _ => PLAIN_NAMES.iter().any(|n| n.eq_ignore_ascii_case(name)) || license_file(name),
    }
}

/// ELF, Mach-O or a Windows program, by its first bytes.
fn native_executable(bytes: &[u8]) -> Option<&'static str> {
    let magic = |m: &[u8]| bytes.starts_with(m);
    if magic(b"\x7fELF") {
        Some("Linux (ELF)")
    } else if magic(&[0xfe, 0xed, 0xfa, 0xce])
        || magic(&[0xfe, 0xed, 0xfa, 0xcf])
        || magic(&[0xce, 0xfa, 0xed, 0xfe])
        || magic(&[0xcf, 0xfa, 0xed, 0xfe])
        || magic(&[0xca, 0xfe, 0xba, 0xbe])
    {
        Some("macOS (Mach-O)")
    } else if magic(b"MZ") {
        Some("Windows")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(license: &str, files: &[(&str, &[u8])]) -> Vec<String> {
        problems(license, files.iter().copied())
    }

    #[test]
    fn a_text_plugin_with_a_license_passes() {
        let files: &[(&str, &[u8])] = &[
            ("plugin.yaml", b"id: x"),
            ("README.md", b"# X"),
            ("LICENSE", b"MIT License"),
            ("tests/fixtures/search.json", b"[]"),
        ];
        assert!(check("MIT", files).is_empty());
        assert!(check("fsl-1.1-mit", files).is_empty(), "SPDX ignores case");
    }

    #[test]
    fn binaries_scripts_and_missing_licenses_are_problems() {
        let base: Vec<(&str, &[u8])> = vec![("plugin.yaml", b"id: x"), ("LICENSE", b"MIT")];
        let with = |extra: (&'static str, &'static [u8])| {
            let mut files = base.clone();
            files.push(extra);
            check("MIT", &files)
        };
        assert!(
            with(("bin/tool", b"\x7fELF\x02"))
                .iter()
                .any(|p| p.contains("ELF"))
        );
        assert!(
            with(("tool", &[0xcf, 0xfa, 0xed, 0xfe]))
                .iter()
                .any(|p| p.contains("Mach-O"))
        );
        assert!(
            with(("tool.exe", b"MZ\x90"))
                .iter()
                .any(|p| p.contains("Windows"))
        );
        assert!(
            with(("data.bin", &[0xff, 0xfe, 0x00]))
                .iter()
                .any(|p| p.contains("isn't text"))
        );
        assert!(
            with(("scripts/fetch.js", b"console.log(1)"))
                .iter()
                .any(|p| p.contains("only its manifest"))
        );
        assert!(
            check("Proprietary", &base)
                .iter()
                .any(|p| p.contains("OSI"))
        );
        assert!(check("", &base).iter().any(|p| p.contains("no license")));
        assert!(
            check("MIT", &[("plugin.yaml", b"id: x")])
                .iter()
                .any(|p| p.contains("LICENSE file"))
        );
    }
}
