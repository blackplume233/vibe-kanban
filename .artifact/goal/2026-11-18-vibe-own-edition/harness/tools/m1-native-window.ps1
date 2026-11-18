# 只捕获探针进程的 Win32 窗口。文件使用带 BOM 的 UTF-8，供 Windows PowerShell 5.1 读取。
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$Screenshot
)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.Encoding]::UTF8

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
}
'@
[void][ProbeWindow]::SetProcessDPIAware()

$process = Start-Process -FilePath $Executable -PassThru
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

    # 窗口句柄先于 NSH 首次提交出现，给合成器留出绘制时间。
    Start-Sleep -Milliseconds 300
    $rect = New-Object ProbeWindow+Rect
    if (-not [ProbeWindow]::GetWindowRect($hwnd, [ref]$rect)) {
        throw '无法获取窗口尺寸'
    }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    $bitmap = New-Object Drawing.Bitmap($width, $height)
    try {
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $dc = $graphics.GetHdc()
            try {
                # PW_RENDERFULLCONTENT 可捕获 Direct2D 合成的内容。
                if (-not [ProbeWindow]::PrintWindow($hwnd, $dc, 2)) { throw '捕获窗口失败' }
            } finally {
                $graphics.ReleaseHdc($dc)
            }
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save($Screenshot, [Drawing.Imaging.ImageFormat]::Png)
    } finally {
        $bitmap.Dispose()
    }
    Write-Output "已捕获窗口 $($process.MainWindowTitle) (${width}x${height})"
} finally {
    if (-not $process.HasExited) {
        if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(5000)) { $process.Kill() }
    }
    $process.Dispose()
}
