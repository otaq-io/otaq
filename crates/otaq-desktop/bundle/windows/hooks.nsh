; Otaq's NSIS hooks (d0140, UPD-09), included by Tauri's installer script (bundle > windows > nsis > installerHooks).
;
; Uninstalling from Settings → Apps removes only the program: it says so and points to `otaq uninstall` for the
; worktrees and data, then stops the background service (failure ignored) so its files can be removed. An update
; (the installer runs the old uninstaller with /UPDATE) asks nothing and leaves the service and its agents running:
; they run from the runtime copy in the data folder, not from the program folder.

!macro NSIS_HOOK_PREUNINSTALL
  ClearErrors
  ${GetOptions} $CMDLINE "/UPDATE" $R9
  IfErrors 0 otaq_hook_done
  IfSilent otaq_hook_stop
  MessageBox MB_OKCANCEL|MB_ICONINFORMATION "This removes only the program. Run `otaq uninstall` first to remove worktrees and data.$\r$\n$\r$\nRemove the program now?" IDOK otaq_hook_stop
  Abort
  otaq_hook_stop:
  nsExec::Exec '"$INSTDIR\otaq.exe" daemon stop'
  Pop $R9
  otaq_hook_done:
!macroend
