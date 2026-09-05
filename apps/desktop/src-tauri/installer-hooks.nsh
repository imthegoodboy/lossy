!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$INSTDIR\lossy.exe" --uninstall-startup'
  DeleteRegKey HKCU "Software\Google\Chrome\NativeMessagingHosts\app.lossy.companion"
  DeleteRegKey HKCU "Software\Microsoft\Edge\NativeMessagingHosts\app.lossy.companion"
  DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "Lossy"
  ; Local encrypted history is intentionally retained on uninstall.
!macroend
