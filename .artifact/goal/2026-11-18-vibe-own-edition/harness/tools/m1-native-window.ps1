# 只捕获探针进程的 Win32 窗口。文件使用带 BOM 的 UTF-8，供 Windows PowerShell 5.1 读取。
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$Evidence,
    [Parameter(Mandatory = $true)][string]$Log,
    [Parameter(Mandatory = $true)][string]$WorkspaceId,
    [Parameter(Mandatory = $true)][string]$ExecutionId
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;

public static class ProbeWindow {
    [StructLayout(LayoutKind.Sequential)]
    public struct Rect { public int Left, Top, Right, Bottom; }

    [DllImport("user32.dll")]
    public static extern bool SetProcessDPIAware();

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool GetWindowRect(IntPtr hwnd, out Rect rect);

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool PrintWindow(IntPtr hwnd, IntPtr dc, uint flags);

    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X, Y; }

    [DllImport("user32.dll")]
    public static extern bool ClientToScreen(IntPtr hwnd, ref Point point);

    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr hwnd);

    [DllImport("user32.dll")]
    public static extern IntPtr SendMessage(IntPtr hwnd, uint message, IntPtr wparam, IntPtr lparam);
}
'@
[void][ProbeWindow]::SetProcessDPIAware()

function Capture-Window($hwnd, $file) {
    $rect = New-Object ProbeWindow+Rect
    if (-not [ProbeWindow]::GetWindowRect($hwnd, [ref]$rect)) { throw 'GetWindowRect failed' }
    $bitmap = New-Object Drawing.Bitmap(($rect.Right - $rect.Left), ($rect.Bottom - $rect.Top))
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try {
                if (-not [ProbeWindow]::PrintWindow($hwnd, $dc, 2)) { throw 'PrintWindow failed' }
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

function Wait-Log($pattern) {
    $deadline = [DateTime]::UtcNow.AddSeconds(15)
    do {
        if (Select-String -Path $Log -Pattern $pattern -Quiet) { return }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    throw "Timed out waiting for $pattern in $Log"
}

$env:RUST_LOG = 'warn,nsh::perf=info,vibe_own_nsh=info'
$env:RUST_LOG_STYLE = 'never'
$process = Start-Process -FilePath $Executable -ArgumentList '--fps' -RedirectStandardError $Log -PassThru
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        $process.Refresh()
        if ($process.HasExited) { throw "探针提前退出，退出码 $($process.ExitCode)" }
        $hwnd = $process.MainWindowHandle
        if ($hwnd -ne [IntPtr]::Zero) { break }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    if ($hwnd -eq [IntPtr]::Zero -or $process.MainWindowTitle -ne 'vibe-own | NSH M1') {
        throw '未找到探针的 Win32 窗口'
    }

    Wait-Log 'HTTP 200 /api/workspaces'
    Start-Sleep -Milliseconds 500
    $scale = [ProbeWindow]::GetDpiForWindow($hwnd) / 96.0
    $x = [int](90 * $scale)
    $y = [int](115 * $scale)
    $click = [IntPtr](($y -shl 16) -bor $x)
    [void][ProbeWindow]::SendMessage($hwnd, 0x0201, [IntPtr]1, $click)
    [void][ProbeWindow]::SendMessage($hwnd, 0x0202, [IntPtr]0, $click)
    Wait-Log "diff ${WorkspaceId}: 1 changed files"
    Wait-Log "execution ${ExecutionId}: [1-9][0-9]* live log entries"
    Start-Sleep -Milliseconds 500
    Capture-Window $hwnd 'm1-native-live-before.png'
    Start-Sleep -Milliseconds 2200
    Capture-Window $hwnd 'm1-native-live-after.png'

    $point = New-Object ProbeWindow+Point
    $point.X = $x
    $point.Y = [int](430 * $scale)
    [void][ProbeWindow]::ClientToScreen($hwnd, [ref]$point)
    $position = [IntPtr]((($point.Y -band 65535) -shl 16) -bor ($point.X -band 65535))
    $wheelDown = [IntPtr](-120 * 65536)
    $screen = [Windows.Forms.Screen]::FromHandle($hwnd).Bounds
    $rect = New-Object ProbeWindow+Rect
    [void][ProbeWindow]::GetWindowRect($hwnd, [ref]$rect)
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $started = [DateTime]::UtcNow
    for ($frame = 0; $frame -lt 100; $frame++) {
        [void][ProbeWindow]::SendMessage($hwnd, 0x020A, $wheelDown, $position)
        Start-Sleep -Milliseconds 45
    }
    Start-Sleep -Milliseconds 400
    $elapsed = ([DateTime]::UtcNow - $started).TotalSeconds
    Write-Output "UTC $($started.ToString('o')); scroll input interval ${elapsed}s; window ${width}x${height}; monitor $($screen.Width)x$($screen.Height); DPI scale $scale"
    Capture-Window $hwnd 'm1-native-scroll.png'
    Write-Output "Captured $($process.MainWindowTitle) live and scrolled windows"
} finally {
    if (-not $process.HasExited) {
        if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(5000)) { $process.Kill() }
    }
    $process.Dispose()
}
