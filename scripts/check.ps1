# Otaq local checks on Windows: the frontend steps (Node version, npm ci, ts-rs freshness, tsc, ESLint, Prettier,
# Vitest, the page build), then fmt, clippy, tests (cargo-nextest, then the doc examples). The same steps as
# scripts/check.sh, in the same order (decision 0056, SDD section 23.2), plus fetching Microsoft's newer ConPTY for
# the tests' sessions (scripts\conpty.ps1).
#
# Run this before every commit made from a Windows machine. Works from any folder inside the repository. Stops at
# the first failing step and exits non-zero. Needs Node 24 (the version in crates\otaq-desktop\.nvmrc) and npm on
# PATH. Setup of a Windows machine or VM: scripts/README.md.
#
#   powershell -ExecutionPolicy Bypass -File scripts\check.ps1

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Write-Error 'check.ps1: git is required (core tests run against temporary git repositories)'
    exit 1
}

cargo nextest --version *> $null
if ($LASTEXITCODE -ne 0) {
    Write-Host 'check.ps1: cargo-nextest is required: cargo install cargo-nextest --locked'
    exit 1
}

$root = git -C $PSScriptRoot rev-parse --show-toplevel 2>$null
if ($LASTEXITCODE -ne 0 -or -not $root) {
    Write-Error 'check.ps1: not inside the Otaq repository'
    exit 1
}
Set-Location $root

# The tests run ordinary Unix commands (sh, cat) in place of claude, as on macOS and Linux (SDD section 23.1). Git for
# Windows ships them in usr\bin; they go at the end of PATH, so Windows' own commands keep precedence.
$gitRoot = Split-Path (Split-Path (Get-Command git).Source)
$usrBin = Join-Path $gitRoot 'usr\bin'
if (-not (Test-Path (Join-Path $usrBin 'sh.exe'))) {
    Write-Error "check.ps1: Git for Windows' sh.exe wasn't found in $usrBin"
    exit 1
}
$env:PATH = "$env:PATH;$usrBin"

# The tests' sessions run in Microsoft's newer ConPTY, as the installed app's do (d0117 point 4, Q601): fetched once
# into target\conpty, loaded from OTAQ_DEV_CONPTY_DIR.
Write-Host '==> conpty: scripts\conpty.ps1'
try {
    $env:OTAQ_DEV_CONPTY_DIR = & (Join-Path $PSScriptRoot 'conpty.ps1') | Select-Object -Last 1
} catch {
    Write-Host "check.ps1: FAILED at step 'conpty': $_"
    exit 1
}
Write-Host $env:OTAQ_DEV_CONPTY_DIR

function Invoke-Step([string] $Name, [string[]] $Command) {
    Write-Host "==> ${Name}: $($Command -join ' ')"
    & $Command[0] $Command[1..($Command.Length - 1)]
    if ($LASTEXITCODE -ne 0) {
        Write-Host "check.ps1: FAILED at step '$Name'"
        exit 1
    }
}

# The frontend steps of SDD section 5.5 (decision 0075 point 7). The page lives inside the desktop crate; every npm
# command runs there. npm.cmd rather than npm, so the exit code comes through whichever npm shim is first on PATH.
$desktop = 'crates/otaq-desktop'
$nodeWanted = ((Get-Content (Join-Path $desktop '.nvmrc') -Raw).Trim().TrimStart('v') -split '\.')[0]
Write-Host "==> node: Node $nodeWanted (from $desktop/.nvmrc)"
if (-not (Get-Command node -ErrorAction SilentlyContinue) -or -not (Get-Command npm.cmd -ErrorAction SilentlyContinue)) {
    Write-Host "check.ps1: Node $nodeWanted is required (node or npm not found on PATH); see $desktop/.nvmrc"
    exit 1
}
$nodeFound = (& node --version).Trim()
$nodeMajor = ($nodeFound.TrimStart('v') -split '\.')[0]
if ($nodeMajor -ne $nodeWanted) {
    Write-Host "check.ps1: Node $nodeWanted is required (found $nodeFound); see $desktop/.nvmrc"
    exit 1
}

Invoke-Step 'npm-ci' @('npm.cmd', '--prefix', $desktop, 'ci', '--no-audit', '--no-fund')
# Before tsc, so a stale generated type is reported as stale rather than as a type error.
Invoke-Step 'types' @('cargo', 'test', '-p', 'otaq-desktop', 'generated_types_are_fresh')
Invoke-Step 'tsc' @('npm.cmd', '--prefix', $desktop, 'run', 'typecheck')
Invoke-Step 'eslint' @('npm.cmd', '--prefix', $desktop, 'run', 'lint')
Invoke-Step 'prettier' @('npm.cmd', '--prefix', $desktop, 'run', 'format:check')
Invoke-Step 'vitest' @('npm.cmd', '--prefix', $desktop, 'run', 'test')
# Plain cargo builds don't embed ui/dist (only the Tauri CLI's release build does), but building the page here catches
# what tsc doesn't, such as an import Vite can't resolve, before the cargo steps.
Invoke-Step 'page' @('npm.cmd', '--prefix', $desktop, 'run', 'build:page')

Invoke-Step 'fmt' @('cargo', 'fmt', '--all', '--check')
Invoke-Step 'clippy' @('cargo', 'clippy', '--workspace', '--all-targets', '--', '-D', 'warnings')
# Each test in its own process, in parallel, no retries (.config/nextest.toml); nextest skips doc examples.
Invoke-Step 'test' @('cargo', 'nextest', 'run', '--workspace')
Invoke-Step 'doctest' @('cargo', 'test', '--workspace', '--doc')

Write-Host 'check.ps1: all checks passed'
