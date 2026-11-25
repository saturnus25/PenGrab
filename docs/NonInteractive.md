# PenGrab non-interactive contract

Use Windows PowerShell 5.1 or PowerShell 7 on Windows x64. Pass explicit catalog IDs when selecting tools. Automated callers should request JSON and check the process exit code.

## Discover and preview selections

~~~powershell
.\PenGrab.bat --catalog-json
.\PenGrab.bat --apply install --tool doggo --tool exiftool --dry-run --non-interactive --json
.\PenGrab.bat --apply upgrade --category Networking.Dns --dry-run --non-interactive --json
~~~

`--catalog-json` returns `schemaVersion`, `categories` and `tools`. Use each entry's `id` value. Category IDs include descendants. Multiple IDs are combined and deduplicated in catalog order. An unknown ID invalidates the request before it changes the installation.

## Installer parameters

| Parameter | Values and behavior |
| --- | --- |
| `-Mode` | `Install` copies the signed Windows bundle to an app directory. `Update` replaces an existing copy if the signed version is newer. `Check` reads signed version metadata. `Run` uses a temporary app directory. `Packages` applies a selection to an existing PenGrab directory. |
| `-InstallPath` | Absolute Windows path to the PenGrab directory. Defaults to `%LOCALAPPDATA%\Programs\PenGrab`. Omit this for `Run`. |
| `-ChoosePath` | Opens the native folder picker in interactive install mode. Omit this in unattended requests. |
| `-ReleaseDirectory` | Absolute path to a local directory containing the signed manifest, signature and archive. Without this parameter the installer uses the official release channel. |
| `-Version` | Exact version `major.minor.patch` without `v`. Defaults to the latest signed release. |
| `-PublicKeyPath` | Absolute path to an independently trusted RSA public key for the repository script. The release scripts carry the embedded public key. |
| `-ToolId` | Array of exact tool IDs, such as `@('doggo','exiftool')`. |
| `-CategoryId` | Array of exact category IDs, such as `@('Networking.Dns')`. |
| `-PackageAction` | `Install` by default. Use `Upgrade` to check for and apply updates to the selected packages. |
| `-ToolInstallPath` | Absolute preferred path passed to WinGet for the selected packages. An installer may ignore it. |
| `-NonInteractive` | Suppresses prompts, UAC and TUI launch. Package operations use user-scope portable installers and omit dependency installation. Pending agreements and unsupported installer types leave the affected items incomplete. |
| `-DryRun` | Validates a plan. Release modes can fetch and verify signed data in temporary storage. |
| `-Json` | Emits one JSON object on standard output for operational results. PowerShell parameter binding failures report through standard error. |
| `-NoLaunch` | Leaves the TUI closed after an interactive installation. |
| `-Confirmed` | Carries the interactive confirmation from a caller that already showed the destination and action. |
| `-WaitForProcessId` | Lets a self-update handoff wait briefly for the open PenGrab process to exit. |
| `-ResultPath` | Absolute path for an extra copy of the JSON report. Use a folder that already exists. |

A request without tool or category IDs installs or updates PenGrab alone. `Packages` requires a selection. Unattended `Run` requires a selection unless it is a dry run. For first-use automation, supply `-NonInteractive -Json -DryRun` with exactly the IDs intended for the actual request.

### Examples

~~~powershell
# Plan a local signed install with an explicit package selection.
.\install.ps1 -Mode Install -NonInteractive -Json -DryRun `
  -InstallPath 'C:\Tools\PenGrab' `
  -ReleaseDirectory 'C:\Signed releases\PenGrab' `
  -ToolId @('doggo','exiftool')
~~~

The PowerShell continuation character is the backtick. For long invocations, use splatting to preserve spaces and arrays.

~~~powershell
$request = @{
    Mode = 'Packages'
    InstallPath = 'C:\Tools\PenGrab'
    ToolId = @('doggo', 'exiftool')
    PackageAction = 'Upgrade'
    NonInteractive = $true
    DryRun = $true
    Json = $true
}
& "$($request.InstallPath)\install.ps1" @request
$LASTEXITCODE
~~~

For an already extracted signed release, the executable supports the same explicit selection.

~~~powershell
.\PenGrab.exe --apply install --non-interactive --tool doggo --dry-run --json
~~~

## Machine-readable results

Installer fields include `schemaVersion: 1`, `mode`, `outcome`, `exitCode` and optional `version`, `installedVersion`, `updateAvailable`, `installPath`, `previousPath`, `message` and `packages`.

The package report includes `schemaVersion`, `outcome`, `operation`, `unattended`, `location` and `items`. Each item contains `id`, `name`, `wingetId`, `status`, `version`, `detail` and `exitCode`. Treat `verified`, `already_installed` and `no_update` as complete. `manual`, `not_installed`, `failed`, `unverified` and `cancelled` require attention. `planned` is a dry-run status.

| Exit | Meaning |
| ---: | --- |
| 0 | Completed, up to date, or valid dry run. Check `outcome` to distinguish them. |
| 2 | Invalid parameters, mode or selection. |
| 3 | Release download, trust, signature or archive verification failed. |
| 4 | Destination or elevation blocked the operation. |
| 5 | Could not prepare, replace or start PenGrab. |
| 6 | At least one requested package operation is incomplete. |
| 7 | User cancelled in an interactive run. |

The installer preserves the current preferences on replacement and reports the previous folder for recovery. The WinGet queue reports the installed versions it can verify. Use the per-item result to decide whether to retry.
