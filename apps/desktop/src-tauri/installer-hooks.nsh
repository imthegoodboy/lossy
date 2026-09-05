!macro NSIS_HOOK_POSTINSTALL
  IfFileExists "$INSTDIR\lossy.exe" +3 0
    MessageBox MB_OK|MB_ICONSTOP "Lossy could not be installed completely. Please run the installer again."
    Abort
  ; Repair a previous upgrade's sign-in task / native-host path without opening a UI.
  nsExec::ExecToStack '"$INSTDIR\lossy.exe" --repair-install'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "Existing integrations could not be restored. Check Capture setup in Lossy."
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  nsExec::Exec '"$INSTDIR\lossy.exe" --uninstall-startup'
  Pop $0
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\app.lossy.companion"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\app.lossy.companion"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lossy"
  ${EndIf}
  ; Local encrypted history is intentionally retained on uninstall.
!macroend
