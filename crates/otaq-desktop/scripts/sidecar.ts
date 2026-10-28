// Builds the `otaq` command for the setup files (d0140, UPD-01) and puts it where Tauri's `externalBin` looks for it:
// `binaries/otaq-<target triple>[.exe]`. Run by `tauri.bundle.conf.json`'s `beforeBuildCommand`, after the page is
// built and before the app is compiled (tauri-build copies the file next to the app then). Tauri names the target in
// `TAURI_ENV_TARGET_TRIPLE` and a debug build in `TAURI_ENV_DEBUG`. A universal macOS build compiles each half and
// joins them with `lipo`; tauri-build reads each half while it compiles the app's halves, the bundler the joined one.
// `OTAQ_RELEASE` passes through to the build unchanged.
//
// The command must share the app's compiled `otaq-core`, whose build script stamps the build ID (`<hash>.<seconds>`):
// compiled twice, the two get different IDs and the app shows "Service update ready" from the first open. So it goes
// into the app's target folder: `--target` only when the app has one (a universal or another system's build); for
// this machine's own system Tauri builds without it, and so does this. (A `--target` naming this machine's own system
// is the one case that still splits them: leave it out.)
//
// Windows: Microsoft's newer ConPTY (`conpty.dll` and `OpenConsole.exe`, d0117 point 4, Q601), fetched by
// `scripts/conpty.ps1` for the target's architecture, goes into `binaries/conpty/`, which `tauri.bundle.conf.json`
// puts next to `otaq.exe`; the service loads it from its own folder. On the other platforms the folder stays empty.
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync, rmSync } from "node:fs";
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
  const own = half === host && !universal;
  const args = [
    "build",
    "-p",
    "otaq-cli",
    ...(own ? [] : ["--target", half]),
    ...(debug ? [] : ["--release"]),
  ];
  console.log(`sidecar: cargo ${args.join(" ")}`);
  execFileSync("cargo", args, { cwd: root, stdio: "inherit" });
  const program = join(targetDir, ...(own ? [] : [half]), debug ? "debug" : "release", `otaq${exe}`);
  copyFileSync(program, join(binaries, `otaq-${half}${exe}`));
  built.push(program);
}
if (universal) {
  run("lipo", ["-create", "-output", join(binaries, `otaq-${triple}`), ...built]);
}
console.log(`sidecar: binaries/otaq-${triple}${exe}`);

const conpty = join(binaries, "conpty");
rmSync(conpty, { recursive: true, force: true });
mkdirSync(conpty);
if (exe !== "") {
  const arches: Record<string, string | undefined> = { x86_64: "x64", aarch64: "arm64", i686: "x86" };
  const arch = arches[triple.split("-")[0] ?? ""];
  if (arch === undefined) throw new Error(`sidecar: no ConPTY for ${triple}`);
  const script = join(root, "scripts", "conpty.ps1");
  const folder = run("powershell.exe", [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-File",
    script,
    "-Arch",
    arch,
  ])
    .trim()
    .split(/\r?\n/)
    .at(-1);
  if (folder === undefined || folder === "") throw new Error("sidecar: scripts/conpty.ps1 printed no folder");
  for (const name of ["conpty.dll", "OpenConsole.exe"]) copyFileSync(join(folder, name), join(conpty, name));
  console.log(`sidecar: binaries/conpty/ (${arch}, from ${folder})`);
}
