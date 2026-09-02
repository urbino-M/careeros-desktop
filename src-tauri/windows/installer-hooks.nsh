; Upgrade the former PostdocOS installation without touching its application data.
!macro NSIS_HOOK_PREINSTALL
  SetShellVarContext current
  ${If} ${FileExists} "$LOCALAPPDATA\PostdocOS\uninstall.exe"
    ExecWait '"$LOCALAPPDATA\PostdocOS\uninstall.exe" /S'
  ${EndIf}
!macroend

; Remove obsolete visible shortcuts and repair an invalid CareerOS desktop link.
!macro NSIS_HOOK_POSTINSTALL
  SetShellVarContext current

  Delete "$DESKTOP\PostdocOS.lnk"
  Delete "$SMPROGRAMS\PostdocOS.lnk"
  Delete "$SMPROGRAMS\PostdocOS\PostdocOS.lnk"
  RMDir "$SMPROGRAMS\PostdocOS"

  ${If} ${FileExists} "$DESKTOP\${PRODUCTNAME}.lnk"
    !insertmacro IsShortcutTarget "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
    Pop $0
    ${If} $0 != 1
      Delete "$DESKTOP\${PRODUCTNAME}.lnk"
      CreateShortcut "$DESKTOP\${PRODUCTNAME}.lnk" "$INSTDIR\${MAINBINARYNAME}.exe"
      !insertmacro SetLnkAppUserModelId "$DESKTOP\${PRODUCTNAME}.lnk"
    ${EndIf}
  ${EndIf}
!macroend
