Set-StrictMode -Version 2.0

function Get-PenGrabWinGetStatus {
    $winget = Get-Command -Name 'winget.exe' -CommandType Application -ErrorAction SilentlyContinue |
        Select-Object -First 1
    $result = [pscustomobject]@{ Status = 'Missing'; Version = $null }
    if (-not $winget) { return $result }

    $result.Status = 'Unavailable'
    $process = [Diagnostics.Process]::new()
    $process.StartInfo.FileName = $winget.Source
    $process.StartInfo.Arguments = '--version'
    $process.StartInfo.UseShellExecute = $false
    $process.StartInfo.CreateNoWindow = $true
    $process.StartInfo.RedirectStandardOutput = $true
    $process.StartInfo.RedirectStandardError = $true

    try {
        if (-not $process.Start()) { return $result }

        $output = $process.StandardOutput.ReadToEndAsync()
        $errorOutput = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(4000)) {
            $result.Status = 'Timeout'
            try { $process.Kill() } catch [InvalidOperationException] {}
            return $result
        }

        if ($process.ExitCode -eq 0 -and $output.Wait(500) -and $errorOutput.Wait(500)) {
            $version = $output.Result.Trim()
            if ($version -match '\Av?\d+(?:\.\d+){1,4}(?:[-+][A-Za-z0-9.-]+)?\z') {
                $result.Status = 'Ready'
                $result.Version = $version
            }
        }
    }
    catch [System.ComponentModel.Win32Exception] {
        $result.Status = 'Unavailable'
    }
    catch [InvalidOperationException] {
        $result.Status = 'Unavailable'
    }
    finally {
        $process.Dispose()
    }

    return $result
}

function Get-PenGrabDiagnostics {
    $architecture = $env:PROCESSOR_ARCHITEW6432
    if (-not $architecture) { $architecture = $env:PROCESSOR_ARCHITECTURE }

    [pscustomobject]@{
        Windows = [Environment]::OSVersion.Version.ToString()
        PowerShell = $PSVersionTable.PSVersion.ToString()
        Architecture = $architecture
        WinGet = Get-PenGrabWinGetStatus
    }
}

Export-ModuleMember -Function Get-PenGrabDiagnostics
