Set-StrictMode -Version 2.0
Import-Module (Join-Path $PSScriptRoot 'Terminal.psm1') -ErrorAction Stop

function New-PenGrabView {
    param([string]$Name, $Tool)

    [pscustomobject]@{
        Name = $Name
        Tool = $Tool
        Focus = 0
        Offset = 0
        TextOffset = 0
        Editing = $false
        PreviousQuery = ''
        PreviousFocus = 0
    }
}

function New-PenGrabChoice {
    param([string]$Action, [string]$Label, [string]$Detail, $Tool)

    [pscustomobject]@{ Action = $Action; Label = $Label; Detail = $Detail; Tool = $Tool }
}

function Get-PenGrabToolBody {
    param($State, $Tool, [switch]$Full)

    $text = $State.Text
    Get-PenGrabToolDescription $Tool $State.Language
    ''
    '{0}: {1}' -f $text.Category, (Get-PenGrabCategoryLabel $Tool.category $text)
    $publisher = Get-PenGrabPublisherUrl $Tool
    if ($Tool.wingetId) {
        'WinGet: {0}' -f $Tool.wingetId
        if ($Full) { $text.PackageVerified }
    }
    elseif (-not $publisher) {
        $text.NoWinGetPackage
    }
    if ($publisher) { $publisher }
    $kali = $Tool.PSObject.Properties['kali']
    if ($Full -and $kali -and $kali.Value) {
        '{0}: {1}' -f $text.SourceKali, $kali.Value
    }
}

function Get-PenGrabScreen {
    param($State, $View)

    $text = $State.Text
    $body = @()
    $resultCount = 0
    $back = New-PenGrabChoice 'back' $text.Back
    $title = $text[$View.Name]
    $hint = $text.HintBack
    $preview = @()

    switch ($View.Name) {
        'Home' {
            $choices = @(
                New-PenGrabChoice 'browse' $text.Browse $text.BrowseSummary
                New-PenGrabChoice 'diagnostics' $text.Diagnostics $text.DiagnosticsSummary
                New-PenGrabChoice 'settings' $text.Settings $text.SettingsSummary
                New-PenGrabChoice 'back' $text.Exit
            )
            $hint = $text.HintHome
        }
        'Browse' {
            $tools = @(Find-PenGrabTool $State.Catalog $text $State.Language $State.Query)
            $resultCount = $tools.Count

            $choices = @(
                New-PenGrabChoice 'search' $text.Search
                foreach ($tool in $tools) {
                    $category = Get-PenGrabCategoryLabel $tool.category $text
                    New-PenGrabChoice 'tool' $tool.name $category $tool
                }
                if ($State.Query) {
                    New-PenGrabChoice 'clear' $text.ClearSearch
                }
                $back
            )
            if ($tools.Count -eq 0) {
                $body = @($text.NoTools)
            }
            $selected = $choices[[Math]::Min($View.Focus, $choices.Count - 1)]
            if ($selected.Tool -and -not $View.Editing) {
                $preview = @($selected.Tool.name; ''; Get-PenGrabToolBody $State $selected.Tool)
            }
            $hint = $text.HintCatalog
            if ($View.Editing) { $hint = $text.HintEdit }
        }
        'Tool' {
            $tool = $View.Tool
            $title = $tool.name
            $body = @(Get-PenGrabToolBody $State $tool -Full)
            $choices = @($back)
        }
        'Diagnostics' {
            if ($State.CheckRequested) { $body = @($text.Checking) }
            else { $body = @(Format-PenGrabDiagnostic $State.Diagnostic $text) }
            $choices = @(
                New-PenGrabChoice 'refresh' $text.Refresh
                $back
            )
        }
        'Settings' {
            $body = @($text.Language)
            if ($State.SaveFailed) { $body += $text.SaveFailed }
            $choices = @(
                foreach ($language in @('en', 'es')) {
                    $label = $text.English
                    if ($language -eq 'es') { $label = $text.Spanish }
                    if ($State.Language -eq $language) {
                        $label = '{0} ({1})' -f $label, $text.Current
                    }
                    New-PenGrabChoice $language $label
                }
                $back
            )
        }
        'Help' {
            $body = @($text.HelpNavigation, $text.HelpSearch, $text.HelpBack, $text.HelpScroll)
            $choices = @($back)
        }
    }

    [pscustomobject]@{
        Title = $title
        Location = switch ($View.Name) {
            'Home' { $title }
            'Tool' { '{0} / {1}' -f $text.Browse, $title }
            default { '{0} / {1}' -f $text.Home, $title }
        }
        ResultCount = $resultCount
        Preview = $preview
        Body = $body
        Choices = $choices
        Hint = $hint
    }
}

function Start-PenGrabSearch {
    param($State, $View)

    $View.PreviousQuery = $State.Query
    $View.PreviousFocus = $View.Focus
    $View.Editing = $true
    $View.Focus = 0
}

function Invoke-PenGrabChoice {
    param($State, $View, $Choice)

    switch ($Choice.Action) {
        'browse' {
            $catalogView = New-PenGrabView 'Browse'
            $catalogView.Focus = 1
            $State.Views.Push($catalogView)
        }
        'search' { Start-PenGrabSearch $State $View }
        'tool' { $State.Views.Push((New-PenGrabView 'Tool' -Tool $Choice.Tool)) }
        'clear' {
            $State.Query = ''
            $View.Focus = 1
            $View.Offset = 0
        }
        'diagnostics' {
            $State.CheckRequested = $true
            $State.Views.Push((New-PenGrabView 'Diagnostics'))
        }
        'refresh' { $State.CheckRequested = $true }
        'settings' { $State.Views.Push((New-PenGrabView 'Settings')) }
        { $_ -in @('en', 'es') } {
            $State.Language = $Choice.Action
            $State.Text = Import-PenGrabText -Root $State.Root -Language $State.Language
            $State.SaveFailed = -not (Save-PenGrabLanguage -Root $State.Root -Language $State.Language)
        }
        'back' { [void]$State.Views.Pop() }
    }
}

function Invoke-PenGrabKey {
    param($State, $View, $Screen, $Key, [int]$BodyPage)

    if ($View.Editing) {
        switch ($Key.Key) {
            'Escape' {
                $State.Query = $View.PreviousQuery
                $View.Focus = $View.PreviousFocus
                $View.Editing = $false
                return
            }
            'Enter' {
                $View.Editing = $false
                $View.Focus = 1
            }
            'Backspace' {
                if ($State.Query.Length -gt 0) {
                    $State.Query = $State.Query.Substring(0, $State.Query.Length - 1)
                }
            }
            'F1' { $State.Views.Push((New-PenGrabView 'Help')) }
            default {
                if ($State.Query.Length -lt 100 -and -not [char]::IsControl($Key.KeyChar) -and
                    -not [char]::IsSurrogate($Key.KeyChar)) {
                    $State.Query += $Key.KeyChar
                }
            }
        }
        return
    }

    if ($Key.Key -eq 'F1' -or $Key.KeyChar -eq '?') {
        if ($View.Name -ne 'Help') { $State.Views.Push((New-PenGrabView 'Help')) }
        return
    }

    $find = $Key.Key -eq 'F' -and ($Key.Modifiers -band [ConsoleModifiers]::Control)
    if ($View.Name -eq 'Browse' -and ($Key.KeyChar -eq '/' -or $find)) {
        Start-PenGrabSearch $State $View
        return
    }

    $count = $Screen.Choices.Count
    switch ($Key.Key) {
        'Escape' { [void]$State.Views.Pop() }
        'Enter' { Invoke-PenGrabChoice $State $View $Screen.Choices[$View.Focus] }
        'UpArrow' {
            if ($count -eq 1) { $View.TextOffset = [Math]::Max(0, $View.TextOffset - 1) }
            else { $View.Focus = ($View.Focus + $count - 1) % $count }
        }
        'DownArrow' {
            if ($count -eq 1) { $View.TextOffset++ }
            else { $View.Focus = ($View.Focus + 1) % $count }
        }
        'Tab' {
            $step = 1
            if ($Key.Modifiers -band [ConsoleModifiers]::Shift) { $step = -1 }
            $View.Focus = ($View.Focus + $count + $step) % $count
        }
        'Home' { $View.Focus = 0 }
        'End' { $View.Focus = $count - 1 }
        'PageUp' { $View.TextOffset = [Math]::Max(0, $View.TextOffset - $BodyPage) }
        'PageDown' { $View.TextOffset += $BodyPage }
    }
}

function Start-PenGrabLineMenu {
    param($State)

    while ($State.Views.Count -gt 0) {
        $view = $State.Views.Peek()
        if ($State.CheckRequested) {
            Write-Host $State.Text.Checking
            $State.Diagnostic = Get-PenGrabDiagnostics
            $State.CheckRequested = $false
        }
        $screen = Get-PenGrabScreen $State $view
        Write-Host ''
        Write-Host ('PenGrab {0} / {1}' -f $State.Version, $screen.Title)
        if ($view.Name -eq 'Browse' -and $State.Query) {
            Write-Host ('{0}: {1}' -f $State.Text.Search, (ConvertTo-PenGrabTerminalText $State.Query))
        }
        foreach ($line in $screen.Body) { Write-Host (ConvertTo-PenGrabTerminalText $line) }

        $lineChoices = @{}
        $index = 1
        foreach ($choice in $screen.Choices) {
            if ($choice.Action -eq 'search') { continue }
            if ($choice.Action -eq 'back') { $key = '0' }
            else { $key = [string]$index; $index++ }
            $lineChoices[$key] = $choice
            $label = $choice.Label
            if ($choice.Action -eq 'tool') { $label += ' | ' + $choice.Detail }
            Write-Host ('{0}. {1}' -f $key, (ConvertTo-PenGrabTerminalText $label))
        }

        $prompt = $State.Text.LinePrompt
        if ($view.Name -eq 'Browse') { $prompt = $State.Text.LineCatalogPrompt }
        $selection = Read-Host $prompt
        if ($null -eq $selection) { return }
        switch ($selection.Trim()) {
            '?' {
                if ($view.Name -ne 'Help') { $State.Views.Push((New-PenGrabView 'Help')) }
            }
            '/' {
                if ($view.Name -eq 'Browse') {
                    $State.Query = [string](Read-Host $State.Text.SearchPrompt)
                }
            }
            default {
                if ($lineChoices.ContainsKey($_)) {
                    Invoke-PenGrabChoice $State $view $lineChoices[$_]
                }
            }
        }
    }
}

function Start-PenGrabConsole {
    param($State, [switch]$NoColor)

    $style = [pscustomobject]@{
        Color = -not $NoColor
        Background = [Console]::BackgroundColor
        Foreground = [Console]::ForegroundColor
    }
    $cursorVisible = [Console]::CursorVisible
    $cursorLeft = [Console]::CursorLeft
    $cursorTop = [Console]::CursorTop
    $glyphs = Get-PenGrabTerminalGlyphs
    $previous = @()
    $previousArea = ''
    $dirty = $true
    $screen = $null
    $frame = $null

    try {
        [Console]::CursorVisible = $false
        if ($style.Color) {
            [Console]::BackgroundColor = 'Black'
            [Console]::ForegroundColor = 'Gray'
        }
        [Console]::Clear()

        while ($State.Views.Count -gt 0) {
            $area = Get-PenGrabConsoleArea
            $areaKey = '{0}/{1}/{2}/{3}' -f $area.Left, $area.Top, $area.Width, $area.Height
            if ($areaKey -ne $previousArea) {
                if ($previous.Count -gt 0) {
                    [Console]::Clear()
                    $area = Get-PenGrabConsoleArea
                    $areaKey = '{0}/{1}/{2}/{3}' -f $area.Left, $area.Top, $area.Width, $area.Height
                }
                $previous = @()
                $previousArea = $areaKey
                $dirty = $true
            }

            $small = $area.Width -lt 22 -or $area.Height -lt 8
            $view = $State.Views.Peek()
            if ($dirty -and $area.Width -gt 0) {
                if ($small) {
                    $frame = [pscustomobject]@{
                        Lines = @(@{ Text = $State.Text.Resize; Style = 'plain' })
                        BodyPage = 1
                    }
                }
                else {
                    $screen = Get-PenGrabScreen $State $view
                    $frame = Get-PenGrabFrame $State $view $screen $area.Width $area.Height $glyphs
                }
                try {
                    $previous = Write-PenGrabFrame $frame $area $previous $style.Color
                    $dirty = $false
                }
                catch [ArgumentOutOfRangeException] {
                    $previousArea = ''
                    continue
                }
            }

            if ($State.CheckRequested) {
                $State.Diagnostic = Get-PenGrabDiagnostics
                $State.CheckRequested = $false
                $dirty = $true
                continue
            }

            if (-not [Console]::KeyAvailable) {
                Start-Sleep -Milliseconds 65
                continue
            }
            $key = [Console]::ReadKey($true)
            if ($small) {
                if ($key.Key -eq 'Escape') { return }
                continue
            }

            Invoke-PenGrabKey $State $view $screen $key $frame.BodyPage
            $dirty = $true
        }
    }
    finally {
        if ($style.Color) {
            [Console]::ForegroundColor = $style.Foreground
            [Console]::BackgroundColor = $style.Background
        }
        [Console]::CursorVisible = $cursorVisible
        [Console]::Clear()
        $left = [Math]::Min($cursorLeft, [Console]::BufferWidth - 1)
        $top = [Math]::Min($cursorTop, [Console]::BufferHeight - 1)
        [Console]::SetCursorPosition($left, $top)
    }
}

function Start-PenGrabTui {
    param(
        [string]$Root, [string]$Version, [string]$Language, [hashtable]$Text,
        [object[]]$Catalog, [switch]$NoColor
    )

    if ([Console]::IsInputRedirected -or [Console]::IsOutputRedirected) {
        Write-Output $Text.Redirected
        return
    }
    $state = [pscustomobject]@{
        Root = $Root
        Version = $Version
        Language = $Language
        Text = $Text
        Catalog = $Catalog
        Query = ''
        Diagnostic = $null
        CheckRequested = $false
        SaveFailed = $false
        Views = [Collections.Generic.Stack[object]]::new()
    }
    $state.Views.Push((New-PenGrabView 'Home'))

    try {
        $null = [Console]::KeyAvailable
        $null = [Console]::CursorVisible
        $null = Get-PenGrabConsoleArea
        [Console]::SetCursorPosition([Console]::CursorLeft, [Console]::CursorTop)
        $lineMode = $env:TERM -eq 'dumb'
    }
    catch [System.Management.Automation.MethodInvocationException] {
        $lineMode = $true
    }

    if ($lineMode) { Start-PenGrabLineMenu $state }
    else {
        $disableColor = $NoColor -or -not [string]::IsNullOrEmpty($env:NO_COLOR)
        Start-PenGrabConsole $state -NoColor:$disableColor
    }
}

Export-ModuleMember -Function Start-PenGrabTui
