; Otaq's NSIS hooks (d0140, UPD-09), included by Tauri's installer script (bundle > windows > nsis > installerHooks).
;
; Uninstalling from Settings → Apps removes only the program: it says so and points to `otaq uninstall` for the
; worktrees and data, then stops the background service (failure ignored) so its files can be removed. An update
; (the installer runs the old uninstaller with /UPDATE) asks nothing and leaves the service and its agents running:
; they run from the runtime copy in the data folder, not from the program folder.
;
; The install registers the `otaq://` links a click on a Windows notification opens (`otaq://open/SHOP-22`): they start
; the app with the link, which opens Otaq on that ticket. Uninstalling removes them; an update writes them again.
;
; Installing over 0.1/0.2 removes their skill folders. Those versions shipped each skill as a top-level
; `plugin\skills\<name>` folder; 0.3 ships the otaq-skills repository there (`plugin\skills\skills\<name>`,
; `.claude-plugin`, README.md, LICENSE). NSIS only copies files over the old ones, and the old uninstaller (the one
; that would delete them) doesn't run on an update (/UPDATE), a passive install or "don't uninstall", so the old
; folders stayed and were copied into every runtime copy. The list below is exactly 0.1/0.2's layout and is fixed:
; never add a name the new layout has at the top of `plugin\skills`. It runs after the files are written, so a
; cancelled install leaves the old version whole, and only when the new layout is there.

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\otaq" "" "URL:Otaq"
  WriteRegStr HKCU "Software\Classes\otaq" "URL Protocol" ""
  WriteRegStr HKCU "Software\Classes\otaq\DefaultIcon" "" '"$INSTDIR\otaq-app.exe",0'
  WriteRegStr HKCU "Software\Classes\otaq\shell\open\command" "" '"$INSTDIR\otaq-app.exe" "%1"'
  ${If} $INSTDIR != ""
  ${AndIf} ${FileExists} "$INSTDIR\plugin\skills\skills\*.*"
    RMDir /r "$INSTDIR\plugin\skills\address-review"
    RMDir /r "$INSTDIR\plugin\skills\answer-question"
    RMDir /r "$INSTDIR\plugin\skills\commit"
    RMDir /r "$INSTDIR\plugin\skills\flow"
    RMDir /r "$INSTDIR\plugin\skills\milestone-plan"
    RMDir /r "$INSTDIR\plugin\skills\noyan"
    RMDir /r "$INSTDIR\plugin\skills\open-pr"
    RMDir /r "$INSTDIR\plugin\skills\product-spec"
    RMDir /r "$INSTDIR\plugin\skills\push"
    RMDir /r "$INSTDIR\plugin\skills\restack"
    RMDir /r "$INSTDIR\plugin\skills\tech-spec"
    RMDir /r "$INSTDIR\plugin\skills\validate"
    RMDir /r "$INSTDIR\plugin\skills\work-ticket"
  ${EndIf}
!macroend

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
  DeleteRegKey HKCU "Software\Classes\otaq"
  otaq_hook_done:
!macroend
