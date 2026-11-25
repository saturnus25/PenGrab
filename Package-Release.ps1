#requires -Version 5.1
[CmdletBinding()]
param(
    [string] $OutputDirectory,
    [string] $KeyPath,
    [switch] $InitializeKey
)
$ErrorActionPreference = 'Stop'
if (-not $OutputDirectory) { $OutputDirectory = Join-Path $PSScriptRoot 'dist' }
if (-not $KeyPath) { $KeyPath = Join-Path $PSScriptRoot 'LocalDocs\ReleaseSigningKey.dpapi' }
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
$KeyPath = [IO.Path]::GetFullPath($KeyPath)
[IO.Directory]::CreateDirectory($OutputDirectory) | Out-Null
$utf8 = New-Object Text.UTF8Encoding($false)
if ($PSVersionTable.PSEdition -eq 'Desktop') {
    Add-Type -AssemblyName System.Security
} else {
    Add-Type -AssemblyName System.Security.Cryptography.ProtectedData
}
$scope = [Security.Cryptography.DataProtectionScope]::CurrentUser
$rsa = New-Object Security.Cryptography.RSACryptoServiceProvider 3072
try {
    $rsa.PersistKeyInCsp = $false
    if (Test-Path -LiteralPath $KeyPath -PathType Leaf) {
        $hex = [IO.File]::ReadAllText($KeyPath).Trim()
        if ($hex -notmatch '^(?:[A-Fa-f0-9]{2})+$') { throw 'Invalid DPAPI signing key file.' }
        $protected = New-Object byte[] ($hex.Length / 2)
        for ($i = 0; $i -lt $protected.Length; $i++) {
            $protected[$i] = [Convert]::ToByte($hex.Substring($i * 2, 2), 16)
        }
        $clear = [Security.Cryptography.ProtectedData]::Unprotect($protected, $null, $scope)
        try { $rsa.FromXmlString([Text.Encoding]::Unicode.GetString($clear)) }
        finally { [Array]::Clear($clear, 0, $clear.Length) }
    } elseif ($InitializeKey) {
        [IO.Directory]::CreateDirectory((Split-Path -Parent $KeyPath)) | Out-Null
        $clear = [Text.Encoding]::Unicode.GetBytes($rsa.ToXmlString($true))
        try { $protected = [Security.Cryptography.ProtectedData]::Protect($clear, $null, $scope) }
        finally { [Array]::Clear($clear, 0, $clear.Length) }
        $stream = [IO.File]::Open($KeyPath, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write, [IO.FileShare]::None)
        try {
            $keyBytes = $utf8.GetBytes([BitConverter]::ToString($protected).Replace('-',''))
            $stream.Write($keyBytes, 0, $keyBytes.Length)
        } finally { $stream.Dispose() }
    } else {
        throw 'Signing key missing. Initialize the key explicitly, then retain it for future releases.'
    }

    $publicKey = $rsa.ToXmlString($false)
    $source = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'install.ps1'))
    if (([regex]::Matches($source, '@@PENGRAB_RELEASE_KEY@@')).Count -ne 1) {
        throw 'The installer must have exactly one release key slot.'
    }
    $install = Join-Path $OutputDirectory 'install.ps1'
    $signedInstaller = $source.Replace('@@PENGRAB_RELEASE_KEY@@', $publicKey)
    [IO.File]::WriteAllText($install, $signedInstaller, $utf8)
    $bootstrap = $signedInstaller.Replace('[string] $Mode = ''Install''','[string] $Mode = ''Run''')
    if ($bootstrap -ceq $signedInstaller) { throw 'Could not prepare the in-memory bootstrap.' }
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'bootstrap.ps1'), $bootstrap, $utf8)
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'PenGrab.release-public.xml'), $publicKey, $utf8)

    $cargo = [IO.File]::ReadAllText((Join-Path $PSScriptRoot 'Cargo.toml'))
    $versionMatch = [regex]::Match($cargo, '(?m)^version = "(\d+\.\d+\.\d+)"\r?$')
    if (-not $versionMatch.Success) { throw 'Could not read the PenGrab release version.' }
    $version = $versionMatch.Groups[1].Value
    & (Join-Path $PSScriptRoot 'Package-Portable.ps1') -OutputDirectory $OutputDirectory -InstallerScript $install | Out-Host
    $bundle = Get-Item -LiteralPath (Join-Path $OutputDirectory "PenGrab-$version-windows-x64.zip")
    $hash = [Security.Cryptography.SHA256]::Create()
    $stream = [IO.File]::OpenRead($bundle.FullName)
    try { $checksum = [BitConverter]::ToString($hash.ComputeHash($stream)).Replace('-','').ToLowerInvariant() }
    finally { $stream.Dispose(); $hash.Dispose() }
    $manifest = [ordered]@{
        schemaVersion = 1
        version = $version
        archive = $bundle.Name
        bytes = $bundle.Length
        sha256 = $checksum
    } | ConvertTo-Json -Compress
    $bytes = $utf8.GetBytes($manifest)
    [IO.File]::WriteAllBytes((Join-Path $OutputDirectory 'PenGrab.release.json'), $bytes)
    $signature = $rsa.SignData($bytes, 'SHA256')
    [IO.File]::WriteAllText((Join-Path $OutputDirectory 'PenGrab.release.sig'), [Convert]::ToBase64String($signature), $utf8)
} finally { $rsa.Dispose() }
