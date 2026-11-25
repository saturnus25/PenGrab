#requires -Version 5.1
[CmdletBinding()]
param(
    [string] $OutputDirectory,
    [string] $InstallerScript
)

$ErrorActionPreference = 'Stop'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $PSScriptRoot 'dist' }
$target = 'x86_64-pc-windows-msvc'
$targetDir = Join-Path $PSScriptRoot 'target'
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'Cargo.toml') -Raw -Encoding UTF8
if ($manifest -notmatch '(?m)^version = "([0-9]+\.[0-9]+\.[0-9]+)"\r?$') {
    throw 'Could not read the PenGrab release version.'
}
$version = $Matches[1]

Push-Location -LiteralPath $PSScriptRoot
try {
    & cargo build --release --locked --target $target --target-dir $targetDir
    if ($LASTEXITCODE -ne 0) { throw "Cargo exited with status $LASTEXITCODE." }
} finally {
    Pop-Location
}

$outputDir = [IO.Path]::GetFullPath($OutputDirectory)
[IO.Directory]::CreateDirectory($outputDir) | Out-Null
$archiveName = "PenGrab-$version-windows-x64.zip"
$archive = Join-Path $outputDir $archiveName
$tempRoot = (Resolve-Path -LiteralPath ([IO.Path]::GetTempPath())).ProviderPath
$stage = Join-Path $tempRoot ('PenGrab-package-' + [Guid]::NewGuid().ToString('N'))
$bundle = Join-Path $stage 'PenGrab'

try {
    $config = Join-Path $bundle 'config'
    [IO.Directory]::CreateDirectory($config) | Out-Null
    Copy-Item -LiteralPath (Join-Path $targetDir "$target/release/pengrab.exe") -Destination (Join-Path $bundle 'PenGrab.exe')
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'PenGrab.bat') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'data') -Destination $bundle -Recurse
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'docs/NonInteractive.md') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'LICENSE') -Destination $bundle
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'THIRD-PARTY-NOTICES.md') -Destination $bundle
    if ($InstallerScript) {
        Copy-Item -LiteralPath $InstallerScript -Destination (Join-Path $bundle 'install.ps1')
        [IO.File]::WriteAllText((Join-Path $config 'release-version'), $version)
    }
    $readme = @'
PenGrab for Windows x64

Extract the entire PenGrab folder to a writable location, then open PenGrab.bat
or PenGrab.exe. Move the folder as a unit to keep your preferences and catalog.

The catalog opens on Windows. Diagnostics uses WinGet when it is available.
'@
    [IO.File]::WriteAllText((Join-Path $bundle 'README.txt'), $readme)
    [IO.File]::WriteAllText((Join-Path $config 'portable'), '')
    [IO.File]::WriteAllText((Join-Path $config 'preferences.json'), '{"Language":"en"}')
    $temporaryArchive = Join-Path $stage $archiveName
    Compress-Archive -LiteralPath $bundle -DestinationPath $temporaryArchive -CompressionLevel Optimal
    Move-Item -LiteralPath $temporaryArchive -Destination $archive -Force
    Get-Item -LiteralPath $archive
} finally {
    if (Test-Path -LiteralPath $stage -PathType Container) {
        $resolvedStage = (Resolve-Path -LiteralPath $stage).ProviderPath
        if ($resolvedStage -eq $stage -and (Split-Path -Parent $resolvedStage) -eq $tempRoot) {
            Remove-Item -LiteralPath $resolvedStage -Recurse -Force
        }
    }
}
