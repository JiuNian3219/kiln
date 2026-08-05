; Keep this registry value aligned with tauri-plugin-autostart's explicit
; `app_name("Codex Input Enhancer")` configuration in src/main.rs.
!define AUTOSTART_RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define AUTOSTART_INSTALLER_KEY "Software\Codex Input Enhancer\Installer"
!define AUTOSTART_VALUE_NAME "Codex Input Enhancer"

!macro NSIS_HOOK_PREINSTALL
  DeleteRegValue HKCU "${AUTOSTART_INSTALLER_KEY}" "EnableAutostart"
  IfSilent preserve_existing_autostart
  MessageBox MB_ICONQUESTION|MB_YESNO "安装完成后，是否在登录 Windows 时自动启动 Codex Input Enhancer？可随时在控制面板中更改。" IDYES enable_autostart
  Goto autostart_choice_complete

  enable_autostart:
    WriteRegStr HKCU "${AUTOSTART_INSTALLER_KEY}" "EnableAutostart" "1"
    Goto autostart_choice_complete

  preserve_existing_autostart:
    ReadRegStr $R8 HKCU "${AUTOSTART_RUN_KEY}" "${AUTOSTART_VALUE_NAME}"
    StrCmp $R8 "" autostart_choice_complete
    WriteRegStr HKCU "${AUTOSTART_INSTALLER_KEY}" "EnableAutostart" "1"

  autostart_choice_complete:
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ReadRegStr $R8 HKCU "${AUTOSTART_INSTALLER_KEY}" "EnableAutostart"
  DeleteRegValue HKCU "${AUTOSTART_INSTALLER_KEY}" "EnableAutostart"
  DeleteRegValue HKCU "${AUTOSTART_RUN_KEY}" "${AUTOSTART_VALUE_NAME}"
  StrCmp $R8 "1" 0 autostart_registration_complete
  WriteRegStr HKCU "${AUTOSTART_RUN_KEY}" "${AUTOSTART_VALUE_NAME}" "$\"$INSTDIR\${MAINBINARYNAME}.exe$\""

  autostart_registration_complete:
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  DeleteRegValue HKCU "${AUTOSTART_RUN_KEY}" "${AUTOSTART_VALUE_NAME}"
!macroend
