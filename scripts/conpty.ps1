# Fetches Microsoft's newer ConPTY for Windows sessions (d0117 point 4, Q601): `conpty.dll` and its `OpenConsole.exe`
# from the NuGet package Microsoft.Windows.Console.ConPTY (microsoft/terminal, MIT), the version pinned below, checked
# against its SHA-256. Both files go into one folder, target\conpty\<version>\<arch> in the repository (gitignored),
# and the folder's path is the one line this prints. A folder there already is used as it is.
#
#   powershell -ExecutionPolicy Bypass -File scripts\conpty.ps1 [-Arch x64|arm64|x86]
#
# The architecture is the Rust host's (`rustc -vV`) unless given. Used by check.ps1 (the tests load the files from
# OTAQ_DEV_CONPTY_DIR) and by crates\otaq-desktop\scripts\sidecar.ts (the setup file puts them next to otaq.exe).

param(
    [ValidateSet('x64', 'arm64', 'x86')]
    [string] $Arch
)

$ErrorActionPreference = 'Stop'

# To move to another version: change both lines; the hash is the .nupkg's
# (`Get-FileHash -Algorithm SHA256`, or `shasum -a 256` on macOS).
$version = '1.25.260930003'
$sha256 = '02b07b349af66d801159bdf9e440d4a1ce78bb951f37fc8609731665afdae7ee'

if (-not $Arch) {
    $hostTriple = ((& rustc -vV) | Select-String '^host: (.+)$').Matches[0].Groups[1].Value
    $Arch = switch -Wildcard ($hostTriple) {
        'x86_64-*' { 'x64' }
        'aarch64-*' { 'arm64' }
        'i686-*' { 'x86' }
        default { throw "conpty.ps1: no ConPTY for the host $hostTriple" }
    }
}

$root = git -C $PSScriptRoot rev-parse --show-toplevel
if ($LASTEXITCODE -ne 0 -or -not $root) {
    throw 'conpty.ps1: not inside the Otaq repository'
}
$dest = Join-Path $root "target\conpty\$version\$Arch"
# Where each file is in the package.
$files = @{
    'conpty.dll'      = "runtimes/win-$Arch/native/conpty.dll"
    'OpenConsole.exe' = "build/native/runtimes/$Arch/OpenConsole.exe"
}

if (-not ($files.Keys | Where-Object { -not (Test-Path (Join-Path $dest $_)) })) {
    Write-Output $dest
    exit 0
}

$work = Join-Path ([IO.Path]::GetTempPath()) "otaq-conpty-$PID"
New-Item -ItemType Directory -Force $work | Out-Null
try {
    $package = Join-Path $work 'conpty.nupkg'
    $url = "https://api.nuget.org/v3-flatcontainer/microsoft.windows.console.conpty/$version/microsoft.windows.console.conpty.$version.nupkg"
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -UseBasicParsing -Uri $url -OutFile $package
    $found = (Get-FileHash -Algorithm SHA256 $package).Hash.ToLowerInvariant()
    if ($found -ne $sha256) {
        throw "conpty.ps1: the package's SHA-256 is $found, not $sha256"
    }

    # Into a folder of its own first, renamed into place whole, so a half-written folder is never used.
    $staged = Join-Path $work $Arch
    New-Item -ItemType Directory -Force $staged | Out-Null
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::OpenRead($package)
    try {
        foreach ($name in $files.Keys) {
            $entry = $zip.GetEntry($files[$name])
            if (-not $entry) {
                throw "conpty.ps1: the package has no $($files[$name])"
            }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, (Join-Path $staged $name), $true)
        }
    } finally {
        $zip.Dispose()
    }
    New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
    if (Test-Path $dest) {
        Remove-Item -Recurse -Force $dest
    }
    Move-Item $staged $dest
} finally {
    Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
}
Write-Output $dest
