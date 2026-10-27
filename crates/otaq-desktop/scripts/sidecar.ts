// Builds the `otaq` command for the setup files (d0140, UPD-01) and puts it where Tauri's `externalBin` looks for it:
// `binaries/otaq-<target triple>[.exe]`. Run by `tauri.bundle.conf.json`'s `beforeBuildCommand`, after the page is
// built and before the app is compiled (tauri-build copies the file next to the app then). Tauri names the target in
// `TAURI_ENV_TARGET_TRIPLE` and a debug build in `TAURI_ENV_DEBUG`. A universal macOS build compiles each half and
// joins them with `lipo`; tauri-build reads each half while it compiles the app's halves, the bundler the joined one.
// `OTAQ_RELEASE` passes through to the build unchanged.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { join, resolve } from "node:path";

const here = import.meta.dirname;
const root = resolve(here, "../../..");
const binaries = join(here, "..", "binaries");

function run(program: string, args: string[]): string {
  return execFileSync(program, args, { cwd: root, stdio: ["ignore", "pipe", "inherit"], encoding: "utf8" });
}

const host = /^host: (.+)$/m.exec(run("rustc", ["-vV"]))?.[1];
const triple = process.env.TAURI_ENV_TARGET_TRIPLE ?? host;
if (triple === undefined) throw new Error("sidecar: no target triple");
const debug = process.env.TAURI_ENV_DEBUG === "true";
const universal = triple === "universal-apple-darwin";
const halves = universal ? ["aarch64-apple-darwin", "x86_64-apple-darwin"] : [triple];
const targetDir = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const exe = triple.includes("windows") ? ".exe" : "";

mkdirSync(binaries, { recursive: true });
const built: string[] = [];
for (const half of halves) {
  const args = ["build", "-p", "otaq-cli", "--target", half, ...(debug ? [] : ["--release"])];
  console.log(`sidecar: cargo ${args.join(" ")}`);
  execFileSync("cargo", args, { cwd: root, stdio: "inherit" });
  const program = join(targetDir, half, debug ? "debug" : "release", `otaq${exe}`);
  copyFileSync(program, join(binaries, `otaq-${half}${exe}`));
  built.push(program);
}
if (universal) {
  run("lipo", ["-create", "-output", join(binaries, `otaq-${triple}`), ...built]);
}
console.log(`sidecar: binaries/otaq-${triple}${exe}`);
