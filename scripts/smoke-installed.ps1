param([string]$Executable = "$env:LOCALAPPDATA\Programs\Lossy\lossy.exe", [switch]$Interactive)
$ErrorActionPreference = 'Stop'
Add-Type 'using System; using System.Text; using System.Runtime.InteropServices; public static class LossySmokeWindow { [StructLayout(LayoutKind.Sequential)] public struct Rect { public int Left,Top,Right,Bottom; } [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h,out Rect rect); [DllImport("user32.dll",CharSet=CharSet.Unicode)] public static extern int GetClassName(IntPtr h,StringBuilder b,int size); }'
if (Get-Process lossy -ErrorAction SilentlyContinue) { throw 'Quit Lossy before the isolated smoke test.' }
$testDirectory = Join-Path ([IO.Path]::GetTempPath()) ('lossy-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $testDirectory | Out-Null
$pipeName = 'lossy-' + [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
function Request-Lossy($Value) {
    $pipe = [IO.Pipes.NamedPipeClientStream]::new('.', $pipeName, [IO.Pipes.PipeDirection]::InOut)
    try {
        $pipe.Connect(1500)
        $bytes = [Text.Encoding]::UTF8.GetBytes(($Value | ConvertTo-Json -Depth 20 -Compress))
        $pipe.Write([BitConverter]::GetBytes([uint32]$bytes.Length),0,4)
        $pipe.Write($bytes,0,$bytes.Length); $pipe.Flush()
        $reader = [IO.BinaryReader]::new($pipe)
        $size = $reader.ReadUInt32()
        if($size -gt 12582912) { throw 'Oversized response' }
        $body = $reader.ReadBytes($size)
        if($body.Length -ne $size) { throw 'Incomplete response' }
        $result = [Text.Encoding]::UTF8.GetString($body) | ConvertFrom-Json
        if($result.error) { throw $result.error }
        return $result.ok
    } finally { $pipe.Dispose() }
}
function Start-TestAgent {
    $start = [Diagnostics.ProcessStartInfo]::new($Executable, '--agent')
    $start.UseShellExecute = $false; $start.CreateNoWindow = $true
    $start.Environment['LOSSY_TEST_DATA'] = $testDirectory
    $process = [Diagnostics.Process]::Start($start)
    for($attempt=0; $attempt -lt 40; $attempt++) {
        try { $null = Request-Lossy @{op='status'}; return $process }
        catch { $lastIssue = $_.Exception.Message; if($process.HasExited) { throw 'Agent exited before ready' }; Start-Sleep -Milliseconds 150 }
    }
    $process.Kill(); throw "Agent failed to start: $lastIssue"
}
function Capture-Test($Context,$Text) {
    $null = Request-Lossy @{op='browser_capture';context=$Context;text=$Text;source='Synthetic smoke editor';private=$false;secure=$false}
}
$agent = $null
try {
    $agent = Start-TestAgent
    $status = Request-Lossy @{op='status'}
    if($status.prefs.enabled) { throw 'Fresh install must require consent' }
    if($status.data_dir -ne $testDirectory) { throw 'Test archive isolation failed' }
    $prefs = $status.prefs
    $prefs.enabled=$true; $prefs.autostart=$false; $prefs.clipboard=$false; $prefs.allowed_apps=@()
    $null = Request-Lossy @{op='settings';prefs=$prefs}
    Capture-Test 'synthetic/chat-a' 'Hello A'
    Capture-Test 'synthetic/chat-b' 'Hello B'
    Capture-Test 'synthetic/chat-a' 'Hello A continued'
    $items = (Request-Lossy @{op='list'}).items
    if($items.Count -ne 2) { throw 'Conversation separation failed' }
    $a = $items | Where-Object text -eq 'Hello A continued'
    if(!$a -or $a.revision -ne 2) { throw 'Draft continuation failed' }
    $agent.Refresh()
    if($agent.MainWindowHandle -ne 0) {
        $rect=[LossySmokeWindow+Rect]::new()
        $null=[LossySmokeWindow]::GetWindowRect($agent.MainWindowHandle,[ref]$rect)
        $className=[Text.StringBuilder]::new(256)
        $null=[LossySmokeWindow]::GetClassName($agent.MainWindowHandle,$className,256)
        # Tao owns a 14x14 event-target infrastructure window, not an archive webview.
        if($className.ToString() -ne 'Tao Thread Event Target') { throw 'Background agent opened an unexpected window' }
    }

    # Kill only this spawned process, then prove acknowledged data/context survives.
    $agent.Kill(); $agent.WaitForExit(); $agent.Dispose()
    $agent = Start-TestAgent
    Capture-Test 'synthetic/chat-a' 'Hello A after restart'
    $restored = Request-Lossy @{op='get';id=$a.id}
    if($restored.revision -ne 3 -or $restored.text -ne 'Hello A after restart') { throw 'Restart recovery failed' }
    Capture-Test 'synthetic/chat-a' ''
    Capture-Test 'synthetic/chat-a' 'A new message'
    if((Request-Lossy @{op='list'}).items.Count -ne 3) { throw 'New message boundary failed' }
    $null = Request-Lossy @{op='browser_capture';context='private';text='Synthetic blocked';source='Test';private=$true;secure=$false}
    if((Request-Lossy @{op='list'}).items.Count -ne 3) { throw 'Private content was saved' }
    $null = Request-Lossy @{op='verify'}
    $null = Request-Lossy @{op='backup'}

    # Exercise the actual executable's browser native-messaging entry point and framing.
    $start = [Diagnostics.ProcessStartInfo]::new($Executable, '--native-host')
    $start.UseShellExecute=$false; $start.CreateNoWindow=$true
    $start.RedirectStandardInput=$true; $start.RedirectStandardOutput=$true
    $hostProcess = [Diagnostics.Process]::Start($start)
    try {
        $message = [Text.Encoding]::UTF8.GetBytes('{"op":"browser_capture","context":"synthetic/native-host","text":"Native framing works","source":"Synthetic browser","private":false,"secure":false}')
        $hostProcess.StandardInput.BaseStream.Write([BitConverter]::GetBytes([uint32]$message.Length),0,4)
        $hostProcess.StandardInput.BaseStream.Write($message,0,$message.Length)
        $hostProcess.StandardInput.BaseStream.Flush()
        $read = [IO.BinaryReader]::new($hostProcess.StandardOutput.BaseStream)
        $length = $read.ReadUInt32()
        $response = [Text.Encoding]::UTF8.GetString($read.ReadBytes($length)) | ConvertFrom-Json
        if(!$response.ok) { throw 'Native messaging failed' }
        $hostProcess.StandardInput.Close()
        if(!$hostProcess.WaitForExit(3000)) { $hostProcess.Kill() }
    } finally { if(!$hostProcess.HasExited) {$hostProcess.Kill()}; $hostProcess.Dispose() }
    if($Interactive) {
        $prefs.allowed_apps=@('pwsh.exe'); $prefs.clipboard=$true
        $null = Request-Lossy @{op='settings';prefs=$prefs}
        $fixtureScript = Join-Path $PSScriptRoot 'synthetic-editor.ps1'
        $fixture = Start-Process -FilePath (Get-Process -Id $PID).Path -ArgumentList '-NoProfile','-STA','-File',('"'+$fixtureScript+'"') -WindowStyle Hidden -PassThru -RedirectStandardOutput (Join-Path $testDirectory 'fixture-output.txt') -RedirectStandardError (Join-Path $testDirectory 'fixture-error.txt')
        try { if(!$fixture.WaitForExit(15000)) { throw 'Synthetic editor timed out' } }
        finally { if(!$fixture.HasExited) {$fixture.Kill()};$fixture.Dispose() }
        $prefs.allowed_apps=@();$prefs.clipboard=$false
        $null = Request-Lossy @{op='settings';prefs=$prefs}
        $archive = (Request-Lossy @{op='list'}).items
        Get-Content (Join-Path $testDirectory 'fixture-output.txt'),(Join-Path $testDirectory 'fixture-error.txt')
        $archive | Select-Object kind,source | Format-Table
        if(!($archive | Where-Object {$_.kind -eq 'draft' -and $_.text -eq 'Synthetic native draft continued'})) { throw 'Native UIA capture failed' }
        if(!($archive | Where-Object {$_.kind -eq 'clipboard' -and $_.text -eq 'Synthetic clipboard text'})) { throw 'Clipboard text capture failed' }
        $picture = $archive | Where-Object kind -eq 'image' | Select-Object -First 1
        if(!$picture -or !$picture.text) { throw 'Clipboard image capture failed' }
        $null = Request-Lossy @{op='copy';id=$picture.id}
        Write-Output 'PASS: real UI Automation text, native clipboard text/image capture, thumbnail and image copy command.'
    }
    Write-Output 'PASS: installed agent, no startup window, isolated contexts, crash recovery, new-message boundary, private exclusion, integrity, backup and native messaging.'
} finally {
    if($agent) { if(!$agent.HasExited) {$agent.Kill();$agent.WaitForExit()};$agent.Dispose() }
    # Synthetic encrypted data only; retain for inspection instead of recursively deleting.
    Write-Output "Synthetic test archive retained at $testDirectory"
}
