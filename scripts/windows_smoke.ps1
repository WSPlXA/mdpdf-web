param(
    [string]$ExePath = "target\release\mdpdf-desktop.exe",
    [switch]$RequireNoNetwork
)

$ErrorActionPreference = "Stop"
$resolvedExe = (Resolve-Path -LiteralPath $ExePath).Path
$process = Start-Process -FilePath $resolvedExe -PassThru -WindowStyle Minimized

try {
    Start-Sleep -Seconds 3
    $process.Refresh()
    if ($process.HasExited) {
        throw "desktop process exited during startup with code $($process.ExitCode)"
    }

    $processIds = @($process.Id)
    do {
        $children = @(Get-CimInstance Win32_Process | Where-Object {
            $_.ParentProcessId -in $processIds -and $_.ProcessId -notin $processIds
        } | Select-Object -ExpandProperty ProcessId)
        $processIds += $children
    } while ($children.Count -gt 0)

    $tcp = @($processIds | ForEach-Object {
        Get-NetTCPConnection -OwningProcess $_ -ErrorAction SilentlyContinue |
            Where-Object { $_.State -ne "Closed" } |
            Select-Object OwningProcess, State, LocalAddress, LocalPort, RemoteAddress, RemotePort
    })
    $udp = @($processIds | ForEach-Object {
        Get-NetUDPEndpoint -OwningProcess $_ -ErrorAction SilentlyContinue |
            Select-Object OwningProcess, LocalAddress, LocalPort
    })
    $appTcp = @($tcp | Where-Object { $_.OwningProcess -eq $process.Id })
    $appUdp = @($udp | Where-Object { $_.OwningProcess -eq $process.Id })
    if ($appTcp.Count -ne 0 -or $appUdp.Count -ne 0) {
        throw "desktop host process unexpectedly owns network endpoints"
    }
    if ($RequireNoNetwork -and ($tcp.Count -ne 0 -or $udp.Count -ne 0)) {
        $processInfo = @(Get-CimInstance Win32_Process | Where-Object {
            $_.ProcessId -in $processIds
        } | Select-Object ProcessId, ParentProcessId, Name, CommandLine)
        $details = @{ tcp = $tcp; udp = $udp; processes = $processInfo } |
            ConvertTo-Json -Compress -Depth 4
        throw "desktop process tree unexpectedly owns network endpoints: $details"
    }

    [pscustomobject]@{
        executable = $resolvedExe
        process_id = $process.Id
        startup_alive = $true
        process_tree_size = $processIds.Count
        host_tcp_endpoints = $appTcp.Count
        host_udp_endpoints = $appUdp.Count
        webview_tcp_endpoints = $tcp.Count - $appTcp.Count
        webview_udp_endpoints = $udp.Count - $appUdp.Count
    } | ConvertTo-Json
} finally {
    if (!$process.HasExited) {
        Stop-Process -Id $process.Id -Force
    }
}
