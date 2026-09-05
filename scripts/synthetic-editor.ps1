# Visible, short-lived synthetic test fixture. No user files or clipboard content are read.
$ErrorActionPreference='Stop'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
$form=[Windows.Forms.Form]::new()
$form.Text='Lossy synthetic capture verification'
$form.Width=580;$form.Height=220;$form.TopMost=$true
$box=[Windows.Forms.TextBox]::new()
$box.Multiline=$true;$box.Dock='Fill';$box.AccessibleName='Synthetic message'
$form.Controls.Add($box)
$form.Show();$form.Activate();$box.Focus() | Out-Null
function Pump([int]$Milliseconds) {
    $until=[DateTime]::UtcNow.AddMilliseconds($Milliseconds)
    while([DateTime]::UtcNow -lt $until) {[Windows.Forms.Application]::DoEvents();Start-Sleep -Milliseconds 20}
}
try {
    Pump 500
    $box.Text='Synthetic native draft';Pump 700
    $box.Text='Synthetic native draft continued';Pump 700
    [Windows.Forms.Clipboard]::SetText('Synthetic clipboard text');Pump 700
    $bitmap=[Drawing.Bitmap]::new(80,60)
    $graphics=[Drawing.Graphics]::FromImage($bitmap)
    $graphics.Clear([Drawing.Color]::Pink);$graphics.Dispose()
    [Windows.Forms.Clipboard]::SetImage($bitmap);Pump 900
    $bitmap.Dispose()
} finally {$form.Close();$form.Dispose()}
