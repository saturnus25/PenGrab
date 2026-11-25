Set-StrictMode -Version 2.0

function Import-PenGrabCatalog {
    param([Parameter(Mandatory)][string]$Root)

    $path = Join-Path $Root 'data\tools.json'
    $catalog = Get-Content -LiteralPath $path -Raw -Encoding UTF8 -ErrorAction Stop |
        ConvertFrom-Json -ErrorAction Stop

    foreach ($tool in $catalog) { $tool }
}

function Import-PenGrabText {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][ValidateSet('en', 'es')][string]$Language
    )

    $texts = @{}
    $languages = @('en')
    if ($Language -ne 'en') { $languages += $Language }

    foreach ($locale in $languages) {
        $path = Join-Path $Root ('data\locales\{0}.json' -f $locale)
        $strings = Get-Content -LiteralPath $path -Raw -Encoding UTF8 -ErrorAction Stop |
            ConvertFrom-Json -ErrorAction Stop

        foreach ($property in $strings.PSObject.Properties) {
            $texts[$property.Name] = [string]$property.Value
        }
    }

    return $texts
}

function Get-PenGrabPreferencePath {
    param([Parameter(Mandatory)][string]$Root)

    Join-Path $Root 'config\preferences.json'
    if ($env:LOCALAPPDATA) {
        Join-Path $env:LOCALAPPDATA 'PenGrab\preferences.json'
    }
}

function Get-PenGrabLanguage {
    param([Parameter(Mandatory)][string]$Root)

    $paths = @(Get-PenGrabPreferencePath -Root $Root)
    $preferences = Get-Item -LiteralPath $paths -ErrorAction SilentlyContinue |
        Where-Object { -not $_.PSIsContainer } |
        Sort-Object -Property LastWriteTimeUtc -Descending

    foreach ($preference in $preferences) {
        try {
            $settings = Get-Content -LiteralPath $preference.FullName -Raw -Encoding UTF8 -ErrorAction Stop |
                ConvertFrom-Json -ErrorAction Stop
            if ($settings.Language -in @('en', 'es')) { return $settings.Language }
        }
        catch [System.Management.Automation.RuntimeException] {
            continue
        }
        catch [System.IO.IOException] {
            continue
        }
        catch [System.UnauthorizedAccessException] {
            continue
        }
    }

    return 'en'
}

function Save-PenGrabLanguage {
    param(
        [Parameter(Mandatory)][string]$Root,
        [Parameter(Mandatory)][ValidateSet('en', 'es')][string]$Language
    )

    foreach ($path in (Get-PenGrabPreferencePath -Root $Root)) {
        $folder = Split-Path -Parent $path
        $temporaryPath = Join-Path $folder ([IO.Path]::GetRandomFileName())
        try {
            [void][IO.Directory]::CreateDirectory($folder)
            $settings = [pscustomobject]@{}
            if ([System.IO.File]::Exists($path)) {
                try {
                    $existing = Get-Content -LiteralPath $path -Raw -Encoding UTF8 -ErrorAction Stop |
                        ConvertFrom-Json -ErrorAction Stop
                    if ($existing -is [pscustomobject]) { $settings = $existing }
                }
                catch [System.Management.Automation.RuntimeException] {}
                catch [System.IO.IOException] {}
                catch [System.UnauthorizedAccessException] {}
            }
            Add-Member -InputObject $settings -NotePropertyName Language -NotePropertyValue $Language -Force
            $json = $settings | ConvertTo-Json -Compress
            [IO.File]::WriteAllText($temporaryPath, $json, [Text.UTF8Encoding]::new($false))

            if ([IO.File]::Exists($path)) {
                [IO.File]::Replace($temporaryPath, $path, [NullString]::Value)
            }
            else {
                [IO.File]::Move($temporaryPath, $path)
            }

            return $true
        }
        catch [System.IO.IOException] {
            continue
        }
        catch [System.UnauthorizedAccessException] {
            continue
        }
        finally {
            Remove-Item -LiteralPath $temporaryPath -ErrorAction SilentlyContinue
        }
    }

    return $false
}

function Get-PenGrabToolDescription {
    param($Tool, [string]$Language)

    $description = $Tool.description.PSObject.Properties[$Language]
    if ($description -and $description.Value) { return [string]$description.Value }
    return [string]$Tool.description.en
}

function Find-PenGrabTool {
    param([object[]]$Catalog, [hashtable]$Text, [string]$Language, [string]$Query)

    $queryText = $Query.Trim()
    $comparison = [Globalization.CultureInfo]::GetCultureInfo($Language).CompareInfo
    $options = [Globalization.CompareOptions]'IgnoreCase, IgnoreNonSpace'
    foreach ($tool in $Catalog) {
        $category = Get-PenGrabCategoryLabel $tool.category $Text
        $description = Get-PenGrabToolDescription -Tool $tool -Language $Language
        $fields = @($tool.name, $tool.category, $category, $tool.wingetId, $description) -join ' '
        if ($comparison.IndexOf($fields, $queryText, $options) -ge 0) { $tool }
    }
}

function Get-PenGrabCategoryLabel {
    param([string]$Category, [hashtable]$Text)

    $root = ($Category -split '\.', 2)[0]
    $label = $Text['Category' + $root]
    if ($label) { return $label }
    return $root
}

function Get-PenGrabPublisherUrl {
    param($Tool)
    foreach ($key in @('publisherDownload', 'publisherProject')) {
        $property = $Tool.PSObject.Properties[$key]
        if ($property -and $property.Value) { return $property.Value }
    }
}

function Format-PenGrabToolSummary {
    param($Tool, [hashtable]$Text)

    $package = $Tool.wingetId
    if (-not $package) { $package = Get-PenGrabPublisherUrl $Tool }
    if (-not $package) { $package = $Text.NoWinGetPackage }
    $category = Get-PenGrabCategoryLabel $Tool.category $Text
    '{0} | {1} | {2}' -f $Tool.name, $category, $package
}

function Format-PenGrabDiagnostic {
    param($Diagnostic, [hashtable]$Text)

    '{0}: {1}' -f $Text.WindowsVersion, $Diagnostic.Windows
    '{0}: {1}' -f $Text.PowerShellVersion, $Diagnostic.PowerShell
    '{0}: {1}' -f $Text.Architecture, $Diagnostic.Architecture
    'WinGet: {0}' -f $Text['WinGet' + $Diagnostic.WinGet.Status]
    if ($Diagnostic.WinGet.Version) {
        '{0}: {1}' -f $Text.WinGetVersion, $Diagnostic.WinGet.Version
    }
}

Export-ModuleMember -Function Import-PenGrabCatalog, Import-PenGrabText, Get-PenGrabLanguage,
    Save-PenGrabLanguage, Get-PenGrabToolDescription, Find-PenGrabTool, Format-PenGrabToolSummary,
    Get-PenGrabPublisherUrl,
    Format-PenGrabDiagnostic, Get-PenGrabCategoryLabel
