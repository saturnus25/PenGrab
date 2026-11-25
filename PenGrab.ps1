#Requires -Version 5.1
[CmdletBinding()]
param(
    [ValidateSet('Menu', 'Catalog', 'Diagnostics')]
    [string]$Mode = 'Menu',

    [ValidateSet('en', 'es')]
    [string]$Language,

    [switch]$NoColor
)

Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
    Write-Error 'PenGrab requires Windows and PowerShell 5.1 or later.'
    return
}

$version = '0.1.0'
$text = @{}
try {
    foreach ($name in @('Core', 'WinGet', 'TUI')) {
        Import-Module (Join-Path $PSScriptRoot "modules\$name.psm1") -ErrorAction Stop
    }

    if (-not $PSBoundParameters.ContainsKey('Language')) {
        $Language = Get-PenGrabLanguage -Root $PSScriptRoot
    }
    $text = Import-PenGrabText -Root $PSScriptRoot -Language $Language

    switch ($Mode) {
        'Diagnostics' {
            Format-PenGrabDiagnostic -Diagnostic (Get-PenGrabDiagnostics) -Text $text
        }
        'Catalog' {
            foreach ($tool in (Import-PenGrabCatalog -Root $PSScriptRoot)) {
                Format-PenGrabToolSummary -Tool $tool -Text $text
            }
        }
        'Menu' {
            $catalog = @(Import-PenGrabCatalog -Root $PSScriptRoot)
            $tui = @{
                Root = $PSScriptRoot
                Version = $version
                Language = $Language
                Text = $text
                Catalog = $catalog
                NoColor = $NoColor
            }
            Start-PenGrabTui @tui
        }
    }
}
catch {
    $message = $text['Fatal']
    if (-not $message) { $message = 'PenGrab could not start' }
    Write-Error ('{0}: {1}' -f $message, $_.Exception.Message)
}
