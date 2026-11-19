# Windows PowerShell 5.1. Capture the actual M4 issue list, following m1-native-window.ps1.
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [Parameter(Mandatory = $true)][string]$Log,
    [Parameter(Mandatory = $true)][string]$ProjectId
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class M4Window {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }
    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X, Y; }

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")]
    public static extern bool GetClientRect(IntPtr hwnd, out Rect rect);
    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hwnd);
    [DllImport("user32.dll")]
    public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags);
    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);
    [DllImport("user32.dll", SetLastError = true)]
    public static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint message, IntPtr wparam,
        IntPtr lparam, uint flags, uint timeoutMs, out IntPtr result);
}
'@
[void][M4Window]::SetProcessDPIAware()

function Capture-Window($hwnd, $file) {
    $rect = New-Object M4Window+Rect
    if (-not [M4Window]::GetWindowRect($hwnd, [ref]$rect)) { throw 'GetWindowRect failed' }
    $bitmap = New-Object Drawing.Bitmap(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try {
                if (-not [M4Window]::PrintWindow($hwnd, $dc, 2)) { throw 'PrintWindow failed' }
            } finally {
                $graphics.ReleaseHdc($dc)
            }
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save((Join-Path $Evidence $file), [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }
}

function Send-WindowMessage($hwnd, $message, $param, $position) {
    $result = [IntPtr]::Zero
    if ([M4Window]::SendMessageTimeout($hwnd, $message, $param, $position, 2, 1000, [ref]$result) -eq [IntPtr]::Zero) {
        throw "Win32 message $message timed out"
    }
}

function Read-Offsets($lines) {
    foreach ($line in $lines) {
        if ($line -match '\bm4 issue offset=(-?\d+)\b') {
            [int]$Matches[1]
        }
    }
}

$env:RUST_LOG = 'warn,nsh::perf=info,vibe_own_nsh=info,vibe_own_nsh::board_ui=debug'
$env:RUST_LOG_STYLE = 'never'
$startedAt = [DateTime]::UtcNow
$clock = [Diagnostics.Stopwatch]::StartNew()
$process = Start-Process -FilePath $Executable -ArgumentList '--fps' -RedirectStandardError $Log -PassThru
try {
    $hwnd = [IntPtr]::Zero
    while ($clock.ElapsedMilliseconds -lt 15000) {
        $process.Refresh()
        if ($process.HasExited) { throw "Native process exited with code $($process.ExitCode)" }
        $hwnd = $process.MainWindowHandle
        if ($hwnd -ne [IntPtr]::Zero -and $process.MainWindowTitle -like 'vibe-own*') { break }
        Start-Sleep -Milliseconds 15
    }
    if ($hwnd -eq [IntPtr]::Zero -or $process.MainWindowTitle -notlike 'vibe-own*') {
        throw 'Could not find the vibe-own Win32 window'
    }
    $windowMs = $clock.Elapsed.TotalMilliseconds
    $client = New-Object M4Window+Rect
    if (-not [M4Window]::GetClientRect($hwnd, [ref]$client)) { throw 'GetClientRect failed' }
    $point = New-Object M4Window+Point
    $point.X = [int]($client.Right * 0.60)
    $point.Y = [int]($client.Bottom * 0.40)
    $clientPosition = [IntPtr](($point.Y -shl 16) -bor $point.X)
    if (-not [M4Window]::ClientToScreen($hwnd, [ref]$point)) { throw 'ClientToScreen failed' }
    $screenPosition = [IntPtr]((($point.Y -band 65535) -shl 16) -bor ($point.X -band 65535))
    $wheelDown = [IntPtr](-120 * 65536)

    # HTTP completion alone is insufficient. Confirm an input advances the real issue list.
    Send-WindowMessage $hwnd 0x0200 ([IntPtr]::Zero) $clientPosition
    $coldMs = $null
    while ($clock.ElapsedMilliseconds -lt 20000) {
        if ($process.HasExited) { throw "Native process exited with code $($process.ExitCode)" }
        $lines = @(Get-Content -LiteralPath $Log)
        if ($lines -match "HTTP 200 /api/projects/${ProjectId}/issues(?:\s|\?|$)") {
            $offsets = @(Read-Offsets $lines)
            if ($offsets.Count -gt 0 -and $offsets[-1] -ne 0) {
                $coldMs = $clock.Elapsed.TotalMilliseconds
                break
            }
            Send-WindowMessage $hwnd 0x020A $wheelDown $screenPosition
        }
        Start-Sleep -Milliseconds 20
    }
    if ($null -eq $coldMs) { throw "No cold-start scroll on the actual issue list. See $Log" }
    Capture-Window $hwnd 'native-first-usable.png'
    $before = @(Get-Content -LiteralPath $Log)
    $offsetBefore = @(Read-Offsets $before)[-1]
    $scrollClock = [Diagnostics.Stopwatch]::StartNew()
    $wheelEvents = 120
    for ($frame = 0; $frame -lt $wheelEvents; $frame++) {
        Send-WindowMessage $hwnd 0x020A $wheelDown $screenPosition
        Start-Sleep -Milliseconds 50
    }
    $scrollMs = $scrollClock.Elapsed.TotalMilliseconds
    $after = @(Get-Content -LiteralPath $Log)
    $offsetAfter = @(Read-Offsets $after)[-1]
    if ([Math]::Abs($offsetAfter - $offsetBefore) -lt 500) { throw "Issue list scroll advanced less than 500 px. See $Log" }
    Capture-Window $hwnd 'native-after-scroll.png'
    $screen = [Windows.Forms.Screen]::FromHandle($hwnd).Bounds
    @{
        started_at_utc = $startedAt.ToString('o')
        process_to_window_ms = $windowMs
        cold_to_usable_ms = $coldMs
        cold_start_signal = 'First nonzero actual issue-list offset after HTTP load and Win32 wheel input'
        benchmark_start_log_line = $before.Count
        benchmark_end_log_line = $after.Count
        benchmark_elapsed_ms = $scrollMs
        wheel_events = $wheelEvents
        initial_offset_px = $offsetBefore
        final_offset_px = $offsetAfter
        client_px = @($client.Right, $client.Bottom)
        monitor_px = @($screen.Width, $screen.Height)
        dpi_scale = [M4Window]::GetDpiForWindow($hwnd) / 96.0
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $Evidence 'native-window.json') -Encoding UTF8

    # Send the list navigation keys directly to this isolated app window.
    foreach ($key in @(0x28, 0x0D)) {
        Send-WindowMessage $hwnd 0x0100 ([IntPtr]$key) ([IntPtr]1)
        Send-WindowMessage $hwnd 0x0101 ([IntPtr]$key) ([IntPtr]1)
    }
    $clock.Restart()
    $root = [Windows.Automation.AutomationElement]::FromHandle($hwnd)
    $title = [Windows.Automation.PropertyCondition]::new([Windows.Automation.AutomationElement]::NameProperty, 'M4 Keyboard entry')
    while ($clock.ElapsedMilliseconds -lt 5000) {
        if ($root.FindFirst([Windows.Automation.TreeScope]::Descendants, $title)) { break }
        Start-Sleep -Milliseconds 25
    }
    if ($clock.ElapsedMilliseconds -ge 5000) { throw 'Keyboard navigation did not open the first SQLite issue' }
    Capture-Window $hwnd 'native-keyboard-detail.png'
} finally {
    if (-not $process.HasExited) {
        if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(5000)) {
            $process.Kill()
            [void]$process.WaitForExit(5000)
        }
    }
    $process.Dispose()
}
