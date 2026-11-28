#requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('Install','Update','Run','Check','Packages')][string] $Mode = 'Install',
    [string] $InstallPath,
    [switch] $ChoosePath,
    [string] $ReleaseDirectory,
    [string] $Version,
    [string] $PublicKeyPath,
    [string[]] $ToolId = @(),
    [string[]] $CategoryId = @(),
    [ValidateSet('Install','Upgrade')][string] $PackageAction = 'Install',
    [string] $ToolInstallPath,
    [switch] $NonInteractive,
    [switch] $DryRun,
    [switch] $Json,
    [switch] $NoLaunch,
    [switch] $Confirmed,
    [int] $WaitForProcessId,
    [string] $ResultPath
)
$ErrorActionPreference = 'Stop'
$embeddedKey = '@@PENGRAB_RELEASE_KEY@@'
$repository = 'https://github.com/saturnus25/PenGrab/releases'
$installerText = $MyInvocation.MyCommand.ScriptBlock.Ast.Extent.Text
$fromFile = $MyInvocation.MyCommand.CommandType -eq [Management.Automation.CommandTypes]::ExternalScript
$script:resultCode = 5
$oldTls = [Net.ServicePointManager]::SecurityProtocol
$work = $null
$verifiedResultPath = $null
$selection = @($ToolId).Count + @($CategoryId).Count -gt 0
$report = [ordered]@{ schemaVersion = 1; mode = $Mode.ToLowerInvariant(); outcome = 'complete' }

function Stop-PenGrab {
    param([int] $Code, [string] $Message)
    $script:resultCode = $Code
    throw $Message
}

function Resolve-AbsolutePath {
    param([string] $Path, [string] $Parameter)
    if ($Path -notmatch '^(?:[A-Za-z]:[\\/]|\\\\[^\\/]+[\\/][^\\/]+[\\/])' -or $Path -match '[\x00-\x1f]') {
        Stop-PenGrab 2 "$Parameter requires an absolute Windows path."
    }
    return [IO.Path]::GetFullPath($Path)
}

function Select-InstallPath {
    param([string] $DefaultPath, [switch] $BrowseFirst)
    $chooser = @'
$ErrorActionPreference = 'Stop'
if (-not [Environment]::UserInteractive) { exit 1 }
Add-Type -AssemblyName System.Windows.Forms
[Windows.Forms.Application]::EnableVisualStyles()

$form = New-Object Windows.Forms.Form -Property @{
    Text = 'Install PenGrab'
    ClientSize = [Drawing.Size]::new(560, 148)
    AutoScaleMode = 'Dpi'
    AutoScaleDimensions = [Drawing.SizeF]::new(96, 96)
    StartPosition = 'CenterScreen'
    FormBorderStyle = 'FixedDialog'
    MaximizeBox = $false
    MinimizeBox = $false
    ShowIcon = $false
}
$label = New-Object Windows.Forms.Label -Property @{
    Text = 'Choose where to install PenGrab.'
    Location = [Drawing.Point]::new(18, 18)
    AutoSize = $true
}
$folder = New-Object Windows.Forms.TextBox -Property @{
    AccessibleName = 'Installation folder'
    Text = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('@@DEFAULT_PATH@@'))
    Location = [Drawing.Point]::new(18, 50)
    Size = [Drawing.Size]::new(418, 24)
    ReadOnly = $true
    TabIndex = 0
}
$browse = New-Object Windows.Forms.Button -Property @{
    Text = '&Browse...'
    Location = [Drawing.Point]::new(446, 47)
    Size = [Drawing.Size]::new(96, 28)
    TabIndex = 1
}
$install = New-Object Windows.Forms.Button -Property @{
    Text = '&Install'
    Location = [Drawing.Point]::new(344, 105)
    Size = [Drawing.Size]::new(96, 28)
    DialogResult = 'OK'
    TabIndex = 2
}
$cancel = New-Object Windows.Forms.Button -Property @{
    Text = 'Cancel'
    Location = [Drawing.Point]::new(446, 105)
    Size = [Drawing.Size]::new(96, 28)
    DialogResult = 'Cancel'
    TabIndex = 3
}
$form.Controls.AddRange(@($label, $folder, $browse, $install, $cancel))
$form.AcceptButton = $install
$form.CancelButton = $cancel
$browse.Add_Click({
    $picker = New-Object Windows.Forms.FolderBrowserDialog
    try {
        $picker.Description = 'Choose or create the folder for PenGrab.'
        $start = $folder.Text
        while ($start -and -not [IO.Directory]::Exists($start)) {
            $start = [IO.Path]::GetDirectoryName($start)
        }
        if ($start) { $picker.SelectedPath = $start }
        if ($picker.ShowDialog($form) -eq [Windows.Forms.DialogResult]::OK) {
            $folder.Text = $picker.SelectedPath
        }
    } finally { $picker.Dispose() }
})
$form.Add_Shown({
    $form.Activate()
    if (@@BROWSE_FIRST@@) { $browse.PerformClick() }
})
try {
    if ($form.ShowDialog() -ne [Windows.Forms.DialogResult]::OK) { exit 7 }
    [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($folder.Text))
} finally { $form.Dispose() }
'@
    $defaultBytes = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($DefaultPath))
    $chooser = $chooser.Replace('@@DEFAULT_PATH@@', $defaultBytes).Replace('@@BROWSE_FIRST@@', [string][int][bool]$BrowseFirst)
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($chooser))
    $shell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $dialogExit = 1
    try {
        $chosen = & $shell -NoLogo -NoProfile -NonInteractive -Sta -EncodedCommand $encoded 2>$null
        $dialogExit = $LASTEXITCODE
    } catch {}
    if ($dialogExit -eq 7) { Stop-PenGrab 7 'Installation cancelled.' }
    if ($dialogExit -eq 0 -and $chosen) {
        return [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($chosen))
    }
    Write-Warning 'The installation window could not open. Choose a folder in this terminal.'
}

function Remove-OwnDirectory {
    param([string] $Path, [string] $Parent, [string] $Prefix)
    if (-not $Path -or -not (Test-Path -LiteralPath $Path -PathType Container)) { return }
    $actual = (Resolve-Path -LiteralPath $Path).ProviderPath
    $separators = [char[]]@('\', '/')
    $actualParent = (Split-Path -Parent $actual).TrimEnd($separators)
    if ($actual -eq $Path -and $actualParent -eq $Parent.TrimEnd($separators) -and
        (Split-Path -Leaf $actual).StartsWith($Prefix, [StringComparison]::Ordinal) -and
        -not ((Get-Item -LiteralPath $actual).Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        Remove-Item -LiteralPath $actual -Recurse -Force -ErrorAction SilentlyContinue
    }
}

function Get-ReleaseFile {
    param([string] $Name, [string] $Destination, [string] $Tag)
    if ($ReleaseDirectory) {
        Copy-Item -LiteralPath (Join-Path $ReleaseDirectory $Name) -Destination $Destination
    } else {
        $route = if ($Tag) { "download/v$Tag" } else { 'latest/download' }
        try {
            Invoke-WebRequest -Uri "$repository/$route/$Name" -UseBasicParsing -TimeoutSec 20 -OutFile $Destination
        } catch {
            try {
                $retry = $_.Exception.Response.Headers['Retry-After']
                if ($retry -match '^\d{1,8}$') { $report.retryAfterSeconds = [int]$retry }
            } catch {}
            throw
        }
    }
}

function Read-Release {
    param([string] $Folder)
    $key = if ($PublicKeyPath) { [IO.File]::ReadAllText($PublicKeyPath) } else { $embeddedKey }
    if ($key -notmatch '^<RSAKeyValue><Modulus>') { Stop-PenGrab 3 'A trusted release public key is required.' }
    $manifest = Join-Path $Folder 'PenGrab.release.json'
    $signature = Join-Path $Folder 'PenGrab.release.sig'
    Get-ReleaseFile 'PenGrab.release.json' $manifest $Version
    Get-ReleaseFile 'PenGrab.release.sig' $signature $Version
    $bytes = [IO.File]::ReadAllBytes($manifest)
    $signed = [Convert]::FromBase64String([IO.File]::ReadAllText($signature).Trim())
    $rsa = New-Object Security.Cryptography.RSACryptoServiceProvider
    try {
        $rsa.PersistKeyInCsp = $false
        $rsa.FromXmlString($key)
        if (-not $rsa.VerifyData($bytes, 'SHA256', $signed)) { Stop-PenGrab 3 'Invalid PenGrab release signature.' }
    } finally { $rsa.Dispose() }
    $release = [Text.Encoding]::UTF8.GetString($bytes).TrimStart([char]0xFEFF) | ConvertFrom-Json
    if ($release.schemaVersion -ne 1 -or $release.version -notmatch '^\d+\.\d+\.\d+$' -or
        ($Version -and $release.version -ne $Version) -or
        $release.archive -cne "PenGrab-$($release.version)-windows-x64.zip" -or
        $release.sha256 -notmatch '^[A-Fa-f0-9]{64}$' -or [long]$release.bytes -le 0) {
        Stop-PenGrab 3 'Invalid signed PenGrab release metadata.'
    }
    return $release
}

function Read-VerifiedArchive {
    param($Release, [string] $Folder)
    $archive = Join-Path $Folder $Release.archive
    Get-ReleaseFile $Release.archive $archive $Release.version
    if ((Get-Item -LiteralPath $archive).Length -ne [long]$Release.bytes -or $Release.bytes -gt 268435456) {
        Stop-PenGrab 3 'PenGrab archive length did not match.'
    }
    $bytes = [IO.File]::ReadAllBytes($archive)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $hex = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-','') }
    finally { $sha.Dispose() }
    if ($bytes.Length -ne [long]$Release.bytes -or $hex -ine $Release.sha256) {
        Stop-PenGrab 3 'PenGrab archive SHA-256 did not match.'
    }
    return ,$bytes
}

function Expand-VerifiedRelease {
    param([byte[]] $Bytes, [string] $Folder)
    Add-Type -AssemblyName System.IO.Compression
    $buffer = New-Object IO.MemoryStream(,$Bytes)
    $zip = New-Object IO.Compression.ZipArchive($buffer, [IO.Compression.ZipArchiveMode]::Read)
    $entries = @{}
    try {
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\','/')
            if ($name -notmatch '^PenGrab/(?:[A-Za-z0-9._-]+/)*[A-Za-z0-9._-]+/?$' -or
                ($name -split '/') -contains '..' -or ($name -split '/') -contains '.' -or
                $entries.ContainsKey($name.TrimEnd('/'))) {
                Stop-PenGrab 3 'Unexpected path in the signed archive.'
            }
            $entries.Add($name.TrimEnd('/'), $true)
            $destination = Join-Path $Folder ($name.Replace('/','\'))
            if ($name.EndsWith('/')) {
                [IO.Directory]::CreateDirectory($destination) | Out-Null
            } else {
                [IO.Directory]::CreateDirectory((Split-Path -Parent $destination)) | Out-Null
                $inputStream = $entry.Open()
                try {
                    $outputStream = [IO.File]::Open($destination, [IO.FileMode]::CreateNew)
                    try { $inputStream.CopyTo($outputStream) }
                    finally { $outputStream.Dispose() }
                } finally { $inputStream.Dispose() }
            }
        }
    } finally { $zip.Dispose(); $buffer.Dispose() }
    foreach ($required in 'PenGrab.exe','PenGrab.bat','data/tools.json','data/categories.json','install.ps1','config/release-version') {
        if (-not $entries.ContainsKey("PenGrab/$required")) { Stop-PenGrab 3 "Incomplete archive. Missing $required." }
    }
    return (Join-Path $Folder 'PenGrab')
}

function Read-InstalledVersion {
    param([string] $Folder)
    $exe = Join-Path $Folder 'PenGrab.exe'
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) { return $null }
    $versionPath = Join-Path $Folder 'config\release-version'
    if (Test-Path -LiteralPath $versionPath -PathType Leaf) {
        $value = [IO.File]::ReadAllText($versionPath).Trim()
        if ($value -match '^\d+\.\d+\.\d+$') { return $value }
    }
    if ((Test-Path -LiteralPath (Join-Path $Folder 'config\portable') -PathType Leaf) -and
        (Test-Path -LiteralPath (Join-Path $Folder 'data\tools.json') -PathType Leaf)) {
        return '0.0.0'
    }
    return $null
}

function Invoke-PackageSelection {
    param([string] $Folder)
    $exe = Join-Path $Folder 'PenGrab.exe'
    $cli = @('--apply', $PackageAction.ToLowerInvariant(), '--non-interactive', '--json')
    foreach ($id in $ToolId) { $cli += @('--tool', $id) }
    foreach ($id in $CategoryId) { $cli += @('--category', $id) }
    if ($ToolInstallPath) { $cli += @('--location', $ToolInstallPath) }
    if ($DryRun) { $cli += '--dry-run' }
    $lines = & $exe @cli
    $code = $LASTEXITCODE
    $report.packages = ($lines -join "`n") | ConvertFrom-Json
    if ($code -ne 0) {
        $report.outcome = 'incomplete'
        $script:resultCode = $code
    }
}

function Install-FromStage {
    param([byte[]] $Bytes, [string] $Destination, [string] $ExpectedVersion)
    $sha = [Security.Cryptography.SHA256]::Create()
    try { $id = [BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Destination.ToLowerInvariant()))).Replace('-','') }
    finally { $sha.Dispose() }
    $mutex = New-Object Threading.Mutex($false, "Local\PenGrab-install-$id")
    $held = $false
    try {
        try { $held = $mutex.WaitOne(0) }
        catch [Threading.AbandonedMutexException] { $held = $true }
        if (-not $held) { Stop-PenGrab 4 'Another PenGrab installer is using this destination.' }
        Set-InstalledFolder $Bytes $Destination $ExpectedVersion
    } finally {
        if ($held) { $mutex.ReleaseMutex() }
        $mutex.Dispose()
    }
}

function Set-InstalledFolder {
    param([byte[]] $Bytes, [string] $Destination, [string] $ExpectedVersion)
    $parent = Split-Path -Parent $Destination
    if (-not $parent -or $Destination -eq [IO.Path]::GetPathRoot($Destination)) {
        Stop-PenGrab 2 'Choose a PenGrab subdirectory.'
    }
    $token = [Guid]::NewGuid().ToString('N')
    $replacement = Join-Path $parent ".pengrab-stage-$token"
    $staged = Join-Path $replacement 'PenGrab'
    $backup = Join-Path $parent ".pengrab-previous-$token"
    $moved = $false
    $committed = $false
    if (Test-Path -LiteralPath $Destination) {
        $record = Get-Item -LiteralPath $Destination
        if (-not $record.PSIsContainer -or ($record.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
            Stop-PenGrab 4 'The destination must be a regular directory.'
        }
        if (@(Get-ChildItem -LiteralPath $Destination -Force).Count -gt 0 -and
            -not (Read-InstalledVersion $Destination)) {
            Stop-PenGrab 4 'Choose an empty directory or an existing PenGrab directory.'
        }
    }
    [IO.Directory]::CreateDirectory($parent) | Out-Null
    try {
        [IO.Directory]::CreateDirectory($replacement) | Out-Null
        $staged = Expand-VerifiedRelease $Bytes $replacement
        $preferences = Join-Path $Destination 'config\preferences.json'
        if (Test-Path -LiteralPath $preferences -PathType Leaf) {
            Copy-Item -LiteralPath $preferences -Destination (Join-Path $staged 'config\preferences.json') -Force
        }
        if (Test-Path -LiteralPath $Destination -PathType Container) {
            [IO.Directory]::Move($Destination, $backup)
            $moved = $true
        }
        [IO.Directory]::Move($staged, $Destination)
        $committed = $true
        if ((Read-InstalledVersion $Destination) -ne $ExpectedVersion) {
            throw 'The installed executable failed its version check.'
        }
        if ($moved) { $report.previousPath = $backup }
    } catch {
        if ($committed) { [IO.Directory]::Move($Destination, $staged) }
        if ($moved) { [IO.Directory]::Move($backup, $Destination) }
        throw
    } finally { Remove-OwnDirectory $replacement $parent '.pengrab-stage-' }
}

function Invoke-ElevatedInstall {
    $sourceBytes = [Text.Encoding]::UTF8.GetBytes($installerText)
    $memory = New-Object IO.MemoryStream
    $compressor = New-Object IO.Compression.GZipStream($memory, [IO.Compression.CompressionMode]::Compress, $true)
    try { $compressor.Write($sourceBytes, 0, $sourceBytes.Length) }
    finally { $compressor.Dispose() }
    $source = [Convert]::ToBase64String($memory.ToArray())
    $memory.Dispose()
    $payload = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes((@{
        InstallPath = $InstallPath; Mode = $Mode; Version = $Version; ReleaseDirectory = $ReleaseDirectory
        PublicKeyPath = $PublicKeyPath; NonInteractive = $true; NoLaunch = $true
    } | ConvertTo-Json -Compress)))
    if ($PublicKeyPath) { Stop-PenGrab 4 'Elevation requires the release installer with its embedded public key.' }
    $command = @'
$m=New-Object IO.MemoryStream(,[Convert]::FromBase64String('__SOURCE__'))
$z=New-Object IO.Compression.GZipStream($m,[IO.Compression.CompressionMode]::Decompress)
$r=New-Object IO.StreamReader($z)
$s=$r.ReadToEnd()
$r.Dispose()
$p=ConvertFrom-Json ([Text.Encoding]::Unicode.GetString([Convert]::FromBase64String('__ARGS__')))
$a=@{}
$p.PSObject.Properties | ForEach-Object { if ($_.Value) { $a[$_.Name]=$_.Value } }
& ([ScriptBlock]::Create($s)) @a
exit $LASTEXITCODE
'@
    $command = $command.Replace('__SOURCE__', $source).Replace('__ARGS__', $payload)
    $encoded = [Convert]::ToBase64String([Text.Encoding]::Unicode.GetBytes($command))
    $powershell = Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    try {
        $process = Start-Process -FilePath $powershell -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ArgumentList '-NoProfile','-EncodedCommand',$encoded
        return $process.ExitCode
    } catch { Stop-PenGrab 4 'Elevation was declined or unavailable.' }
}

try {
    if ($Version -and $Version -notmatch '^\d+\.\d+\.\d+$') { Stop-PenGrab 2 'Version requires major.minor.patch.' }
    if ($NonInteractive -and $Mode -eq 'Run' -and -not $selection -and -not $DryRun) {
        Stop-PenGrab 2 'Non-interactive Run requires ToolId or CategoryId.'
    }
    if ($Mode -eq 'Packages' -and -not $selection) { Stop-PenGrab 2 'Select ToolId or CategoryId.' }
    if ($ToolInstallPath -and -not $selection) { Stop-PenGrab 2 'ToolInstallPath requires a package selection.' }
    if ($Mode -eq 'Check' -and $selection) { Stop-PenGrab 2 'Check cannot execute a package selection.' }
    if ($Mode -eq 'Run' -and $InstallPath) { Stop-PenGrab 2 'Run stages PenGrab in a temporary directory.' }
    $interactiveInstall = $Mode -eq 'Install' -and -not ($NonInteractive -or $DryRun -or $Confirmed)
    $destinationConfirmed = $false
    if ($ChoosePath -and (-not $interactiveInstall -or $InstallPath)) {
        Stop-PenGrab 2 'ChoosePath requires interactive Install without InstallPath, DryRun or Confirmed.'
    }
    if (-not $InstallPath) {
        $InstallPath = Join-Path $env:LOCALAPPDATA 'Programs\PenGrab'
        if ($interactiveInstall) {
            $chosen = Select-InstallPath -DefaultPath $InstallPath -BrowseFirst:$ChoosePath
            if ($chosen) {
                $InstallPath = $chosen
                $destinationConfirmed = $true
            } else {
                $enteredPath = (Read-Host "Install folder (absolute path, Enter for $InstallPath)").Trim()
                if ($enteredPath) { $InstallPath = $enteredPath.Trim('"') }
            }
        }
    }
    $InstallPath = Resolve-AbsolutePath $InstallPath 'InstallPath'
    if ($ReleaseDirectory) { $ReleaseDirectory = Resolve-AbsolutePath $ReleaseDirectory 'ReleaseDirectory' }
    if ($PublicKeyPath) { $PublicKeyPath = Resolve-AbsolutePath $PublicKeyPath 'PublicKeyPath' }
    if ($ToolInstallPath) { $ToolInstallPath = Resolve-AbsolutePath $ToolInstallPath 'ToolInstallPath' }
    if ($ResultPath) { $verifiedResultPath = Resolve-AbsolutePath $ResultPath 'ResultPath' }

    if ($Mode -eq 'Packages') {
        if (-not (Read-InstalledVersion $InstallPath)) { Stop-PenGrab 2 'Point InstallPath to a PenGrab folder.' }
        if (-not $NonInteractive -and -not $DryRun -and -not $Confirmed) {
            if ((Read-Host 'Apply the selected user-scope portable packages? [y/N]') -notmatch '^(y|yes|s|si)$') {
                Stop-PenGrab 7 'Cancelled.'
            }
        }
        $script:resultCode = 0
        Invoke-PackageSelection $InstallPath
    } else {
        $tempParent = (Resolve-Path -LiteralPath ([IO.Path]::GetTempPath())).ProviderPath
        $work = Join-Path $tempParent ('PenGrab-release-' + [Guid]::NewGuid().ToString('N'))
        [IO.Directory]::CreateDirectory($work) | Out-Null
        [Net.ServicePointManager]::SecurityProtocol = $oldTls -bor [Net.SecurityProtocolType]::Tls12
        $script:resultCode = 3
        $release = Read-Release $work
        $report.version = $release.version
        $report.installedVersion = Read-InstalledVersion $InstallPath
        $update = $report.installedVersion -and ([version]$release.version -gt [version]$report.installedVersion)
        $report.updateAvailable = [bool]$update
        if ($Mode -eq 'Check') {
            $script:resultCode = 0
        } elseif ($Mode -eq 'Update' -and -not $report.installedVersion) {
            Stop-PenGrab 2 'Point InstallPath to an existing PenGrab installation.'
        } elseif ($Mode -eq 'Update' -and -not $update -and -not $selection) {
            $report.outcome = 'up_to_date'
            $script:resultCode = 0
        } else {
            $archiveBytes = Read-VerifiedArchive $release $work
            $bundle = $null
            $script:resultCode = 5
            if (-not ($DryRun -or $NonInteractive -or $Confirmed -or $destinationConfirmed) -and $Mode -ne 'Run') {
                if ((Read-Host "Install PenGrab $($release.version) in $InstallPath? [y/N]") -notmatch '^(y|yes|s|si)$') {
                    Stop-PenGrab 7 'Cancelled.'
                }
            }
            if ($selection -or $Mode -eq 'Run') {
                $bundle = Expand-VerifiedRelease $archiveBytes (Join-Path $work 'extract')
            }
            if ($selection) {
                $wasDryRun = $DryRun
                $DryRun = $true
                try { Invoke-PackageSelection $bundle } finally { $DryRun = $wasDryRun }
                if ($report.packages.outcome -ne 'planned') { Stop-PenGrab 2 'The package selection is invalid.' }
            }
            if ($DryRun) {
                $report.outcome = 'planned'
                $report.installPath = if ($Mode -ne 'Run') { $InstallPath } else { $null }
                $script:resultCode = 0
            } else {
                if ($WaitForProcessId) {
                    $existing = Get-Process -Id $WaitForProcessId -ErrorAction SilentlyContinue
                    if ($existing -and -not $existing.WaitForExit(15000)) { Stop-PenGrab 5 'PenGrab is still running.' }
                }
                $location = $bundle
                if ($Mode -ne 'Run') {
                    try {
                        if ($Mode -ne 'Update' -or $update) { Install-FromStage $archiveBytes $InstallPath $release.version }
                    } catch [UnauthorizedAccessException] {
                        if ($NonInteractive) { Stop-PenGrab 4 'The chosen path requires elevation.' }
                        if ((Read-Host 'The destination needs administrator permission. Continue with UAC? [y/N]') -notmatch '^(y|yes|s|si)$') {
                            Stop-PenGrab 4 'Elevation declined.'
                        }
                        $elevated = Invoke-ElevatedInstall
                        if ($elevated -ne 0) { Stop-PenGrab 4 "Elevated install returned $elevated." }
                    }
                    $location = $InstallPath
                    $report.installPath = $location
                }
                $script:resultCode = 0
                if ($selection) { Invoke-PackageSelection $location }
                if (-not $NonInteractive -and -not $Json -and -not $NoLaunch) {
                    if ($Mode -eq 'Run') {
                        [IO.File]::WriteAllText((Join-Path $location 'config\temporary'), '')
                    }
                    $exe = Join-Path $location 'PenGrab.exe'
                    & $exe
                    if ($LASTEXITCODE -ne 0) { Stop-PenGrab 5 "PenGrab exited with status $LASTEXITCODE." }
                }
            }
        }
    }
} catch {
    $report.outcome = 'error'
    $report.message = $_.Exception.Message
} finally {
    [Net.ServicePointManager]::SecurityProtocol = $oldTls
    if ($work) { Remove-OwnDirectory $work $tempParent 'PenGrab-release-' }
}
$report.exitCode = $script:resultCode
$resultJson = $report | ConvertTo-Json -Depth 10 -Compress
if ($verifiedResultPath) {
    try { [IO.File]::WriteAllText($verifiedResultPath, $resultJson) }
    catch {
        if ($script:resultCode -eq 0) { $script:resultCode = 5 }
        $report.reportError = $_.Exception.Message
        $report.exitCode = $script:resultCode
        $resultJson = $report | ConvertTo-Json -Depth 10 -Compress
    }
}
if ($Json) { $resultJson }
elseif ($report.message) { [Console]::Error.WriteLine($report.message) }
else { Write-Output "PenGrab $($report.outcome)" }
if ($fromFile) { exit $script:resultCode }
$global:LASTEXITCODE = $script:resultCode
