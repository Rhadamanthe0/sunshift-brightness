# Installeur NSIS — Custom Solar Screen Brightness (fork)
# Paramétrable au build via /D : BINDIR (dossier des .exe), ICON, OUTFILE.
Unicode True

!include "MUI2.nsh"

!ifndef BINDIR
  !define BINDIR "..\target\release"
!endif
!ifndef ICON
  !define ICON "..\assets\icon-256.ico"
!endif
!ifndef OUTFILE
  !define OUTFILE "ssb-installer.exe"
!endif

!define APP_NAME "Custom Solar Screen Brightness"
!define RUN_KEY "Software\Microsoft\Windows\CurrentVersion\Run"
!define RUN_VALUE "CustomSolarScreenBrightness"
!define SHORT_CUT "$SMPROGRAMS\${APP_NAME}.lnk"
!define SHORT_CUT_UNINSTALL "$SMPROGRAMS\Uninstall ${APP_NAME}.lnk"

RequestExecutionLevel user
OutFile "${OUTFILE}"
Name "${APP_NAME}"
InstallDir "$LocalAppData\solar-screen-brightness"

!define MUI_ICON "${ICON}"
!define MUI_WELCOMEPAGE_TITLE "${APP_NAME}"
!define MUI_WELCOMEPAGE_TEXT "Click Install to start the installation."
!define MUI_FINISHPAGE_RUN "$INSTDIR\ssb.exe"
!define MUI_FINISHPAGE_RUN_PARAMETERS "--minimised"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!define MUI_UNWELCOMEPAGE_TITLE "${APP_NAME}"
!define MUI_UNWELCOMEPAGE_TEXT "Click Uninstall to start the uninstallation."
!insertmacro MUI_UNPAGE_WELCOME
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section
    SetOutPath "$INSTDIR"

    # Arreter une instance en cours (ancien nom et nom courant)
    nsExec::ExecToLog 'taskkill /f /im solar-screen-brightness.exe'
    Pop $0
    nsExec::ExecToLog 'taskkill /f /im ssb.exe'
    Pop $0
    nsExec::ExecToLog 'taskkill /f /im ssb-cli.exe'
    Pop $0
    Sleep 500

    File "${BINDIR}\ssb.exe"
    File "${BINDIR}\ssb-cli.exe"
    WriteUninstaller "$INSTDIR\uninstall.exe"
    CreateShortcut "${SHORT_CUT}" "$INSTDIR\ssb.exe"
    CreateShortcut "${SHORT_CUT_UNINSTALL}" "$INSTDIR\uninstall.exe"

    # Demarrage auto au logon, minimise. La cle Run se declenche des que le
    # shell (donc la barre des taches) est pret : tot, mais pas au point que
    # l'icone de tray echoue a s'enregistrer.
    WriteRegStr HKCU "${RUN_KEY}" "${RUN_VALUE}" '"$INSTDIR\ssb.exe" --minimised'
SectionEnd

Section "uninstall"
    nsExec::ExecToLog 'taskkill /f /im ssb.exe'
    Pop $0
    nsExec::ExecToLog 'taskkill /f /im ssb-cli.exe'
    Pop $0
    Sleep 500
    DeleteRegValue HKCU "${RUN_KEY}" "${RUN_VALUE}"
    Delete "${SHORT_CUT}"
    Delete "${SHORT_CUT_UNINSTALL}"
    RMDir /r "$INSTDIR"
SectionEnd
