; Hooks into the installer that Tauri generates. After the app is copied, the installer offers
; VB-CABLE when it is missing, so the app opens with the virtual cable already in place. The app
; itself checks for the cable and installs it (`--check-cable`, `--install-cable`), with the same
; download and signature check as its Install VB-CABLE key.

!macro NSIS_HOOK_POSTINSTALL
  ; An update runs passively in the background and must never ask for administrator rights or
  ; a restart, and a silent install has nobody to ask.
  ${If} $UpdateMode <> 1
  ${AndIf} $PassiveMode <> 1
  ${AndIfNot} ${Silent}
    Push $0
    Push $1
    Push $2
    ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --check-cable' $0
    ${If} $0 <> 0
      ; Included before the languages load, so the texts are picked here rather than as LangStrings.
      ${If} $LANGUAGE = ${LANG_PORTUGUESEBR}
        StrCpy $1 "O InstantPad precisa do VB-CABLE, o cabo de áudio virtual que leva sua voz e seus sons para o jogo ou o Discord.$\r$\n$\r$\nInstalar agora? O Windows vai pedir permissão de administrador, e no fim será preciso reiniciar o PC.$\r$\n$\r$\nO VB-CABLE é da VB-Audio (www.vb-cable.com). VB-CABLE is a donationware, all participations are welcome."
        StrCpy $2 "O VB-CABLE não foi instalado. Você pode instalar depois no InstantPad, em Ajustes > Instalar VB-CABLE."
      ${Else}
        StrCpy $1 "InstantPad needs VB-CABLE, the virtual audio cable that carries your voice and your sounds into the game or Discord.$\r$\n$\r$\nInstall it now? Windows will ask for administrator rights, and the PC must restart at the end.$\r$\n$\r$\nVB-CABLE is made by VB-Audio (www.vb-cable.com). VB-CABLE is a donationware, all participations are welcome."
        StrCpy $2 "VB-CABLE was not installed. You can install it later in InstantPad, under Settings > Install VB-CABLE."
      ${EndIf}
      ${If} ${Cmd} `MessageBox MB_YESNO|MB_ICONQUESTION|MB_DEFBUTTON1 "$1" IDYES`
        ExecWait '"$INSTDIR\${MAINBINARYNAME}.exe" --install-cable' $0
        ${If} $0 = 0
          ; The driver only works after a restart. The finish page then offers it, and the app
          ; opens by itself at the next login.
          SetRebootFlag true
          WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\RunOnce" "${PRODUCTNAME}" '"$INSTDIR\${MAINBINARYNAME}.exe"'
        ${Else}
          MessageBox MB_OK|MB_ICONEXCLAMATION "$2"
        ${EndIf}
      ${EndIf}
    ${EndIf}
    Pop $2
    Pop $1
    Pop $0
  ${EndIf}
!macroend
