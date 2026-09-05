# Visible, short-lived synthetic test fixture. No user files or clipboard content are read.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type 'using System; using System.Runtime.InteropServices; public static class SyntheticFocus { [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow(); [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid); [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h); [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a,uint b,bool attach); [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId(); [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h,int command); }'
$form=[Windows.Forms.Form]::new()
$form.Text='Lossy synthetic capture verification'
$form.Width=580;$form.Height=220;$form.TopMost=$true
$box=[Windows.Forms.TextBox]::new()
$box.Multiline=$true;$box.Dock='Fill';$box.AccessibleName='Synthetic message'
$form.Controls.Add($box)
$form.Show();$form.Activate();$box.Focus() | Out-Null
[SyntheticFocus]::SetForegroundWindow($form.Handle) | Out-Null
[uint32]$foregroundPid=0
$foregroundThread=[SyntheticFocus]::GetWindowThreadProcessId([SyntheticFocus]::GetForegroundWindow(),[ref]$foregroundPid)
$fixtureThread=[SyntheticFocus]::GetCurrentThreadId()
[SyntheticFocus]::AttachThreadInput($fixtureThread,$foregroundThread,$true) | Out-Null
try { [SyntheticFocus]::ShowWindow($form.Handle,5) | Out-Null; [SyntheticFocus]::SetForegroundWindow($form.Handle) | Out-Null; $box.Focus() | Out-Null }
finally { [SyntheticFocus]::AttachThreadInput($fixtureThread,$foregroundThread,$false) | Out-Null }
function Pump([int]$Milliseconds) {
    $until=[DateTime]::UtcNow.AddMilliseconds($Milliseconds)
    while([DateTime]::UtcNow -lt $until) {[Windows.Forms.Application]::DoEvents();Start-Sleep -Milliseconds 20}
}
try {
    Pump 500
    [uint32]$foregroundPid=0
    [SyntheticFocus]::GetWindowThreadProcessId([SyntheticFocus]::GetForegroundWindow(),[ref]$foregroundPid) | Out-Null
    Write-Output "Synthetic editor focused: $($foregroundPid -eq $PID), textbox focused: $($box.Focused)"
    $box.Text='Synthetic native draft';Pump 700
    $box.Text='Synthetic native draft continued';Pump 700
    [Windows.Forms.Clipboard]::SetText('Synthetic clipboard text');Pump 700
    $bitmap=[Drawing.Bitmap]::new(80,60)
    $graphics=[Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([Drawing.Color]::Pink);$graphics.Dispose()
    [Windows.Forms.Clipboard]::SetImage($bitmap);Pump 900
    $bitmap.Dispose()
} finally {$form.Close();$form.Dispose()}
