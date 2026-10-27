#!/usr/bin/env node
// latest-json.mjs <folder> <tag> <owner/repo>: writes <folder>/latest.json, the file every installed copy's updater
// reads (`releases/latest/download/latest.json`, d0140), from the release's files in <folder>, then SHA256SUMS next
// to it. The release workflow runs it once, after every build, so the file is made in one place rather than merged
// by parallel jobs.
//
// Each updater file and its `.sig` (Tauri's signature, made with the updater key) gives the platforms:
//   Otaq_<v>_universal.app.tar.gz  darwin-universal, darwin-aarch64, darwin-x86_64
//   Otaq_<v>_x64-setup.exe         windows-x86_64        Otaq_<v>_arm64-setup.exe   windows-aarch64
//   Otaq_<v>_amd64.AppImage        linux-x86_64          Otaq_<v>_aarch64.AppImage  linux-aarch64
// The `.deb` has no entry: it never updates itself (a `.deb` install only says a newer version is out).
// Added to Tauri's fields: `protocol_version` (scripts/release/protocol-version.sh, so the app tells a compatible
// update from one that needs agents restarted, UPD-03) and `notes_url` (the release page, for What's new).
// An updater file without its `.sig` stops the script: the updater would refuse it anyway.
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  readdirSync,
  readFileSync,
  renameSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const [dir, tag, repo] = process.argv.slice(2);
if (!dir || !tag || !repo) {
  console.error("usage: latest-json.mjs <folder> <tag> <owner/repo>");
  process.exit(2);
}
const fail = (message) => {
  console.error(`latest-json.mjs: ${message}`);
  process.exit(1);
};

const here = dirname(fileURLToPath(import.meta.url));
const protocol = Number(
  execFileSync("bash", [join(here, "protocol-version.sh")], {
    encoding: "utf8",
  }).trim(),
);
if (!Number.isInteger(protocol)) fail("protocol-version.sh printed no number");

const PLATFORMS = [
  [
    /_universal\.app\.tar\.gz$/,
    ["darwin-universal", "darwin-aarch64", "darwin-x86_64"],
  ],
  [/_x64-setup\.exe$/, ["windows-x86_64"]],
  [/_arm64-setup\.exe$/, ["windows-aarch64"]],
  [/_amd64\.AppImage$/, ["linux-x86_64"]],
  [/_aarch64\.AppImage$/, ["linux-aarch64"]],
];
const updaterFile = /(\.app\.tar\.gz|-setup\.exe|\.AppImage)$/;

const names = readdirSync(dir).filter((name) =>
  statSync(join(dir, name)).isFile(),
);
const platforms = {};
for (const name of names.filter((n) => updaterFile.test(n)).sort()) {
  const keys = PLATFORMS.find(([pattern]) => pattern.test(name))?.[1];
  if (!keys) fail(`${name} isn't a file the updater knows`);
  if (!names.includes(`${name}.sig`)) {
    fail(
      `${name} has no signature (${name}.sig); was the build signed with the updater key?`,
    );
  }
  const signature = readFileSync(join(dir, `${name}.sig`), "utf8").trim();
  for (const key of keys) {
    if (platforms[key])
      fail(`two files for ${key}: ${platforms[key].file} and ${name}`);
    platforms[key] = {
      signature,
      url: `https://github.com/${repo}/releases/download/${tag}/${name}`,
      file: name,
    };
  }
}
if (Object.keys(platforms).length === 0) fail(`no updater files in ${dir}`);

const page = `https://github.com/${repo}/releases/tag/${tag}`;
const latest = {
  version: tag.replace(/^v/, ""),
  notes: `See the release notes: ${page}`,
  notes_url: page,
  pub_date: new Date().toISOString().replace(/\.\d{3}Z$/, "Z"),
  protocol_version: protocol,
  platforms: Object.fromEntries(
    Object.entries(platforms).map(([key, { signature, url }]) => [
      key,
      { signature, url },
    ]),
  ),
};
writeFileSync(join(dir, "latest.json"), `${JSON.stringify(latest, null, 2)}\n`);

// Every file of the release but SHA256SUMS itself, as `sha256sum -c` and `shasum -a 256 -c` read it.
const sums = [...new Set([...names, "latest.json"])]
  .filter((name) => name !== "SHA256SUMS")
  .sort()
  .map(
    (name) =>
      `${createHash("sha256")
        .update(readFileSync(join(dir, name)))
        .digest("hex")}  ${name}\n`,
  )
  .join("");
writeFileSync(join(dir, "SHA256SUMS.partial"), sums);
renameSync(join(dir, "SHA256SUMS.partial"), join(dir, "SHA256SUMS"));
console.log(
  `latest-json.mjs: latest.json (${Object.keys(platforms).sort().join(", ")}) and SHA256SUMS in ${dir}`,
);
