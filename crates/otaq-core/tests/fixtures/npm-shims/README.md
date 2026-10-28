# npm launchers on Windows

Saved launchers for `tools::cmd_shim`'s tests, with their CRLF line endings (`.gitattributes` keeps them as they
are).

- `codex.cmd`: what `npm i -g @openai/codex` wrote (npm 11.19.1, `cmd-shim` 8.0.0, Node 24.22.1, Windows 11 ARM64,
  2026-10-28).
- `corepack.cmd`: Node 24.22.1's own, from `C:\Program Files\nodejs`: the older two-branch form (npm 6, Yarn 1).
- `npm.cmd`: Node 24.22.1's own `npm.cmd`, hand-written (a `FOR /F` loop): not a launcher Otaq reads.
- `pnpm-codex.cmd`: written from `@zkochan/cmd-shim`'s template (pnpm's) for a global install with `NODE_PATH`; not
  saved from a real install.
