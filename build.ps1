<#
.SYNOPSIS
    Builds dbpadu for Windows with the GNU toolchain (MinGW-w64).

.DESCRIPTION
    Needs no Visual Studio or Microsoft build tools. Needs:
      - rustup (https://rustup.rs)
      - a 64-bit MinGW-w64 gcc on PATH, MSVCRT flavour
      - a JDK, version 17 or newer, for the query engine
        (Maven is downloaded by engine\mvnw.cmd)

    The script installs Rust's GNU toolchain through rustup when it is
    missing, builds the Java engine and a release of the program, packages
    both as a .zip in dist\, and copies dbpadu.exe and dbpadu-engine.jar to
    the folder this script is in. The two files stay together.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\build.ps1
#>

[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

$App = 'dbpadu'
$Jar = 'dbpadu-engine.jar'
$Toolchain = 'stable-x86_64-pc-windows-gnu'
$Target = 'x86_64-pc-windows-gnu'

function Fail([string] $Message) {
    Write-Host "error: $Message" -ForegroundColor Red
    exit 1
}

# Run a program and stop if it fails.
function Invoke-Checked([string] $Program, [string[]] $Arguments) {
    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        Fail "'$Program $($Arguments -join ' ')' failed with exit code $LASTEXITCODE"
    }
}

# 1. rustup
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Fail @"
rustup is not installed. Download rustup-init.exe from https://rustup.rs and run:

    .\rustup-init.exe --default-host x86_64-pc-windows-gnu -y

Then open a new terminal and run this script again.
"@
}

# 2. Rust's GNU toolchain. Build scripts and macros run on this computer, so
#    the whole toolchain (not only the target) must be GNU to avoid needing
#    Microsoft's linker.
$installed = & rustup toolchain list
if (-not ($installed | Where-Object { $_ -like "$Toolchain*" })) {
    Write-Host "Installing Rust toolchain $Toolchain ..."
    Invoke-Checked 'rustup' @('toolchain', 'install', $Toolchain, '--profile', 'minimal')
}

# 3. MinGW-w64 gcc links the program. Look in the usual MSYS2 folder if it
#    is not on PATH.
if (-not (Get-Command gcc -ErrorAction SilentlyContinue)) {
    $msys = 'C:\msys64\mingw64\bin'
    if (Test-Path -LiteralPath (Join-Path $msys 'gcc.exe')) {
        $env:PATH = "$msys;$env:PATH"
    }
}
if (-not (Get-Command gcc -ErrorAction SilentlyContinue)) {
    Fail @"
gcc (MinGW-w64) was not found. Install one of these, open a new terminal,
and run this script again:

    winget install -e --id BrechtSanders.WinLibs.POSIX.MSVCRT

or MSYS2 (https://www.msys2.org), then in the "MSYS2 MINGW64" shell:

    pacman -S mingw-w64-x86_64-gcc
"@
}
$machine = (& gcc -dumpmachine | Out-String).Trim()
if ($machine -ne 'x86_64-w64-mingw32') {
    Fail "the gcc on PATH builds for '$machine'; a 64-bit MinGW-w64 gcc (x86_64-w64-mingw32) is needed"
}
# Rust's Windows GNU target uses the older MSVCRT C runtime. A gcc built for
# UCRT can fail to link with it.
# gcc prints this on stderr, which Windows PowerShell 5.1 would turn into a
# stopping error under 'Stop'.
$previous = $ErrorActionPreference
$ErrorActionPreference = 'Continue'
$gccInfo = (& gcc -v 2>&1 | Out-String)
$ErrorActionPreference = $previous
if ($gccInfo -match 'msvcrt=ucrt') {
    Write-Warning ("This gcc uses the UCRT C runtime. If linking fails, install the MSVCRT build: " +
        "winget install -e --id BrechtSanders.WinLibs.POSIX.MSVCRT")
}

# 4. Java, for the engine.
if (-not $env:JAVA_HOME -and -not (Get-Command java -ErrorAction SilentlyContinue)) {
    Fail @"
Java was not found. Install a JDK, version 17 or newer, open a new terminal,
and run this script again:

    winget install -e --id EclipseAdoptium.Temurin.21.JDK
"@
}

# 5. Build the engine (its tests run too), then the program.
Write-Host "Building the engine ..."
Push-Location -LiteralPath 'engine'
& .\mvnw.cmd -q -B package
$code = $LASTEXITCODE
Pop-Location
if ($code -ne 0) { Fail "the engine could not be built (exit code $code)" }
$engineJar = "engine\target\$Jar"
if (-not (Test-Path -LiteralPath $engineJar)) { Fail "the engine was not built: $engineJar is missing" }

Invoke-Checked 'cargo' @("+$Toolchain", 'build', '--release', '--locked', '--target', $Target)

# 6. Package.
$versionLine = Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "(.+)"' | Select-Object -First 1
$version = $versionLine.Matches[0].Groups[1].Value
$name = "$App-$version-windows-x86_64"
$dist = Join-Path $PSScriptRoot 'dist'
$stage = Join-Path $dist $name
$zip = Join-Path $dist "$name.zip"

if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
New-Item -ItemType Directory -Path $stage -Force | Out-Null

$exe = "target\$Target\release\$App.exe"
Copy-Item -LiteralPath $exe, $engineJar, 'README.md' -Destination $stage
# Also put the program and the engine next to this script, so they can be
# run straight away. Windows locks a running .exe and the jar its engine
# has open, so this fails while dbpadu is open.
try {
    Copy-Item -LiteralPath $exe -Destination (Join-Path $PSScriptRoot "$App.exe") -Force
    Copy-Item -LiteralPath $engineJar -Destination (Join-Path $PSScriptRoot $Jar) -Force
} catch {
    Fail "could not replace $App.exe or $Jar in $PSScriptRoot. Close dbpadu and run this script again."
}
Compress-Archive -LiteralPath $stage -DestinationPath $zip
Remove-Item -LiteralPath $stage -Recurse -Force

Write-Host "Built $zip" -ForegroundColor Green
Write-Host "Copied $App.exe and $Jar to $PSScriptRoot. Keep the two files together; Java 17 or newer must be installed to run it."
