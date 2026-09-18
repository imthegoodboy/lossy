param(
    [string]$Installer,
    [string]$InstallDirectory = "$env:LOCALAPPDATA\Programs\Lossy",
    [string]$ExpectedVersion = '0.1.1'
)
$ErrorActionPreference = 'Stop'
if ($Installer) {
    $installerPath = (Resolve-Path -LiteralPath $Installer).Path
    if (Get-Process lossy -ErrorAction SilentlyContinue) { throw 'Quit Lossy before installing or verifying an upgrade.' }
    $process = Start-Process -FilePath $installerPath -ArgumentList '/S',('/D=' + $InstallDirectory) -WindowStyle Hidden -PassThru
    if (!$process.WaitForExit(180000)) { throw 'Installer is still running. Inspect it before retrying.' }
    if ($process.ExitCode -ne 0) { throw "Installer failed: $($process.ExitCode)" }
    $process.Dispose()
}
$exe = Join-Path $InstallDirectory 'lossy.exe'
if (!(Test-Path -LiteralPath $exe -PathType Leaf)) { throw "Installed executable missing: $exe. Installation did not finish or the executable was removed." }
$version = (Get-Item -LiteralPath $exe).VersionInfo.ProductVersion
if ($version -ne $ExpectedVersion) { throw "Expected $ExpectedVersion, found $version" }
if (!(Test-Path -LiteralPath (Join-Path $InstallDirectory 'uninstall.exe'))) { throw 'Uninstaller missing' }
if (!(Test-Path -LiteralPath (Join-Path $InstallDirectory 'browser/manifest.json'))) { throw 'Bundled companion missing' }
$registrations = @(Get-ChildItem 'HKCU:/Software/Microsoft/Windows/CurrentVersion/Uninstall' | Get-ItemProperty | Where-Object DisplayName -eq 'Lossy')
if ($registrations.Count -ne 1) { throw "Expected one Installed Apps registration, found $($registrations.Count)" }
$registration = $registrations[0]
if ($registration.DisplayVersion -ne $ExpectedVersion -or $registration.InstallLocation.Trim('"') -ne $InstallDirectory) { throw 'Installed Apps registration points to a different version or folder' }
if ($registration.Publisher -ne 'Lossy') { throw 'Installer publisher metadata is incorrect' }
$shell = New-Object -ComObject WScript.Shell
foreach ($shortcutPath in @(
    (Join-Path ([Environment]::GetFolderPath('Desktop')) 'Lossy.lnk'),
    (Join-Path ([Environment]::GetFolderPath('Programs')) 'Lossy.lnk')
)) {
    if (!(Test-Path -LiteralPath $shortcutPath)) { throw "Shortcut missing: $shortcutPath. Finish setup with Create desktop shortcut selected, or rerun the installer." }
    $shortcut = $shell.CreateShortcut($shortcutPath)
    if ($shortcut.TargetPath -ne $exe) { throw "Shortcut points to the wrong executable: $shortcutPath" }
    Write-Output "PASS: $shortcutPath -> $exe"
}
Write-Output "PASS: Lossy $version executable, companion, uninstaller, Installed Apps registration, desktop and Start menu shortcuts."
