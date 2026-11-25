Set-StrictMode -Version 2.0

function ConvertTo-PenGrabTerminalText {
    param([string]$Text)

    $Text -replace '[\p{Cc}\p{Cf}]', ' '
}

function Limit-PenGrabLine {
    param([string]$Text, [int]$Width)

    if ($Text.Length -le $Width) { return $Text }
    if ($Width -le 3) { return $Text.Substring(0, $Width) }
    $Text.Substring(0, $Width - 3) + '...'
}

function Split-PenGrabLine {
    param([string]$Text, [int]$Width)

    $remaining = ConvertTo-PenGrabTerminalText $Text
    while ($remaining.Length -gt $Width) {
        $length = $remaining.LastIndexOf(' ', $Width)
        if ($length -le 0) { $length = $Width }
        $remaining.Substring(0, $length)
        $remaining = $remaining.Substring($length).TrimStart()
    }
    $remaining
}

function Get-PenGrabTerminalGlyphs {
    $rules = [string][char]0x2500 + [char]0x2502
    $encoding = [Console]::OutputEncoding
    if ($encoding.GetString($encoding.GetBytes($rules)) -ceq $rules) {
        return @{ Horizontal = $rules[0]; Vertical = $rules[1] }
    }
    return @{ Horizontal = '-'; Vertical = '|' }
}

function Join-PenGrabColumns {
    param([string]$Left, [string]$Right, [int]$Width)

    $remaining = $Width - $Right.Length - 2
    if ($remaining -lt 4) { return (Limit-PenGrabLine $Left $Width) }
    (Limit-PenGrabLine $Left $remaining).PadRight($remaining + 2) + $Right
}

function Get-PenGrabFrame {
    param($State, $View, $Screen, [int]$Width, [int]$Height, [hashtable]$Glyphs)

    $lines = [Collections.Generic.List[object]]::new()
    $margin = 1
    if ($Width -ge 34) { $margin = 2 }
    $pad = ' ' * $margin
    $inside = $Width - 2 * $margin
    $rule = $pad + ([string]$Glyphs.Horizontal * $inside)
    $text = $State.Text
    $catalog = $View.Name -eq 'Browse'
    $firstChoice = [int]$catalog
    $choiceCount = $Screen.Choices.Count - $firstChoice
    $View.Focus = [Math]::Min($View.Focus, $Screen.Choices.Count - 1)

    $lines.Add(@{
        Text = $pad + (Join-PenGrabColumns 'PenGrab' ('v' + $State.Version) $inside)
        Style = 'brand'
    })
    $lines.Add(@{ Text = $rule; Style = 'rule' })
    $lines.Add(@{ Text = $pad + (Limit-PenGrabLine $Screen.Location $inside); Style = 'heading' })

    if ($catalog) {
        $searchText = $State.Query
        if ($View.Editing) { $searchText += '_' }
        elseif (-not $searchText) { $searchText = $text.SearchAll }

        $result = $text.ResultCount -f $Screen.ResultCount
        if ($Screen.ResultCount -eq 1) { $result = $text.ResultCountOne }
        $fieldWidth = $inside
        if ($inside -ge 44) { $fieldWidth -= $result.Length + 2 }
        $label = '{0}: ' -f $text.Search
        $maximum = $fieldWidth - $label.Length - 2
        if ($searchText.Length -gt $maximum -and $View.Editing) {
            $searchText = '...' + $searchText.Substring($searchText.Length - $maximum + 3)
        }
        $prefix = '  '
        $style = 'plain'
        if ($View.Focus -eq 0) { $prefix = '> '; $style = 'focus' }
        $search = $prefix + $label + (Limit-PenGrabLine $searchText $maximum)
        if ($fieldWidth -ne $inside) { $search = Join-PenGrabColumns $search $result $inside }
        $lines.Add(@{
            Text = $pad + $search
            Style = $style
            FocusStart = $margin
            FocusWidth = $fieldWidth
        })
    }

    if ($Height -ge 12) { $lines.Add(@{ Text = ''; Style = 'plain' }) }
    $footerRows = 1
    if ($Height -ge 10) { $footerRows = 2 }

    $rows = $Height - $lines.Count - $footerRows
    $detail = @(
        foreach ($paragraph in $Screen.Body) {
            Split-PenGrabLine $paragraph ([Math]::Min($inside, 78))
        }
    )
    $maxTextRows = $rows - [Math]::Min($choiceCount, 2)
    $textRows = [Math]::Min($detail.Count, $maxTextRows)
    $maxTextOffset = [Math]::Max(0, $detail.Count - $textRows)
    $View.TextOffset = [Math]::Min($View.TextOffset, $maxTextOffset)
    for ($index = 0; $index -lt $textRows; $index++) {
        $lines.Add(@{ Text = $pad + $detail[$View.TextOffset + $index]; Style = 'plain' })
    }

    $choiceRows = [Math]::Min($rows - $textRows, $choiceCount)
    $focusedChoice = [Math]::Max(0, $View.Focus - $firstChoice)
    $View.Offset = [Math]::Min($View.Offset, $choiceCount - $choiceRows)
    if ($focusedChoice -lt $View.Offset) { $View.Offset = $focusedChoice }
    if ($focusedChoice -ge $View.Offset + $choiceRows) {
        $View.Offset = $focusedChoice - $choiceRows + 1
    }

    $listWidth = $inside
    $preview = @()
    $sideBySide = $catalog -and $inside -ge 70 -and $rows -ge 6
    if ($sideBySide) {
        $listWidth = 27
        $previewWidth = [Math]::Min($inside - $listWidth - 3, 78)
        $preview = @(
            foreach ($paragraph in $Screen.Preview) {
                Split-PenGrabLine $paragraph $previewWidth
            }
        )
    }

    $contentRows = [Math]::Max($choiceRows, [Math]::Min($preview.Count, $rows - $textRows))
    for ($row = 0; $row -lt $contentRows; $row++) {
        $style = 'plain'
        $left = ''
        if ($row -lt $choiceRows) {
            $choiceIndex = $row + $View.Offset + $firstChoice
            $choice = $Screen.Choices[$choiceIndex]
            $prefix = '  '
            if ($choiceIndex -eq $View.Focus) { $style = 'focus'; $prefix = '> ' }
            $left = Limit-PenGrabLine ($prefix + $choice.Label) $listWidth
            if ($choice.Action -eq 'tool' -and -not $sideBySide) {
                $withCategory = '{0}  |  {1}' -f $left, $choice.Detail
                if ($withCategory.Length -le $listWidth) { $left = $withCategory }
            }
        }

        if ($preview.Count -gt 0) {
            $left = $left.PadRight($listWidth)
            if ($row -lt $preview.Count) { $left += ' ' + $Glyphs.Vertical + ' ' + $preview[$row] }
        }
        $lines.Add(@{
            Text = $pad + $left
            Style = $style
            FocusStart = $margin
            FocusWidth = [Math]::Min($listWidth, 34)
        })
    }

    if ($View.Name -eq 'Home' -and $lines.Count -lt $Height - $footerRows - 1) {
        $summary = $Screen.Choices[$View.Focus].Detail
        if ($summary) {
            $lines.Add(@{ Text = ''; Style = 'plain' })
            foreach ($line in (Split-PenGrabLine $summary ([Math]::Min($inside, 78)))) {
                if ($lines.Count -ge $Height - $footerRows) { break }
                $lines.Add(@{ Text = $pad + $line; Style = 'plain' })
            }
        }
    }

    while ($lines.Count -lt $Height - $footerRows) { $lines.Add(@{ Text = ''; Style = 'plain' }) }
    if ($footerRows -eq 2) { $lines.Add(@{ Text = $rule; Style = 'rule' }) }
    $hint = $Screen.Hint
    if ($maxTextOffset -gt 0) { $hint = $text.HintScroll + '  |  ' + $hint }
    if ($hint.Length -gt $inside) {
        if ($maxTextOffset -gt 0) { $hint = $text.HintScrollShort }
        elseif ($View.Editing) { $hint = $text.HintEditShort }
        elseif ($catalog) { $hint = $text.HintCatalogShort }
        else { $hint = $text.HintShort }
    }
    if ($hint.Length -gt $inside) { $hint = $text.HintKeys }
    $lines.Add(@{ Text = $pad + $hint; Style = 'plain' })

    [pscustomobject]@{ Lines = $lines; BodyPage = [Math]::Max(1, $textRows) }
}

function Get-PenGrabConsoleArea {
    [pscustomobject]@{
        Left = [Console]::WindowLeft
        Top = [Console]::WindowTop
        Width = [Console]::WindowWidth - 1
        Height = [Console]::WindowHeight
    }
}

function Write-PenGrabFrame {
    param($Frame, $Area, $Previous, [bool]$UseColor)

    $next = [Collections.Generic.List[string]]::new()
    $count = [Math]::Min($Area.Height, [Math]::Max($Frame.Lines.Count, $Previous.Count))
    for ($row = 0; $row -lt $count; $row++) {
        $style = 'plain'
        $line = ''
        $focusStart = 0
        $focusWidth = 0
        if ($row -lt $Frame.Lines.Count) {
            $cell = $Frame.Lines[$row]
            $style = $cell.Style
            $line = ConvertTo-PenGrabTerminalText $cell.Text
            $focusStart = [int]$cell['FocusStart']
            $focusWidth = [int]$cell['FocusWidth']
        }

        $line = (Limit-PenGrabLine $line $Area.Width).PadRight($Area.Width)
        $key = '{0}/{1}/{2}/{3}' -f $style, $focusStart, $focusWidth, $line
        $next.Add($key)
        if ($row -lt $Previous.Count -and $Previous[$row] -ceq $key) { continue }

        [Console]::SetCursorPosition($Area.Left, $Area.Top + $row)
        if (-not $UseColor) {
            [Console]::Write($line)
            continue
        }

        [Console]::BackgroundColor = 'Black'
        [Console]::ForegroundColor = switch ($style) {
            'brand' { 'Red' }
            'rule' { 'DarkGray' }
            'heading' { 'White' }
            default { 'Gray' }
        }

        if ($style -eq 'focus' -and $focusWidth -gt 0) {
            [Console]::Write($line.Substring(0, $focusStart))
            [Console]::BackgroundColor = 'DarkRed'
            [Console]::ForegroundColor = 'White'
            [Console]::Write($line.Substring($focusStart, $focusWidth))
            [Console]::BackgroundColor = 'Black'
            [Console]::ForegroundColor = 'Gray'
            [Console]::Write($line.Substring($focusStart + $focusWidth))
        }
        else {
            [Console]::Write($line)
        }
    }

    return ,$next
}

Export-ModuleMember -Function ConvertTo-PenGrabTerminalText, Get-PenGrabTerminalGlyphs,
    Get-PenGrabFrame, Get-PenGrabConsoleArea, Write-PenGrabFrame
