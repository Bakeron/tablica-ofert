; Uninstalling removes the hourly Task Scheduler task (zalozenia.md section 3).
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'schtasks.exe /Delete /TN "TablicaOfert\Pobieranie ofert" /F'
!macroend

; ---------- "No paid signature yet" notice (robota.md, SPRAWY pkt 8) ----------
; The first page of the installer: Windows or an antivirus may check the unsigned app for a few
; seconds. "I understand" must be ticked to go on. Skipped in silent (/S) and passive (/P) mode,
; e.g. updates. Remove this page once the installer is signed or ships through Microsoft Store.
; This file is included before the template's pages, so this page comes first.
!include nsDialogs.nsh

Var NoticeDialog
Var NoticeCheck

Page custom NoticeShow NoticeLeave

Function NoticeShow
  IfSilent 0 +2
    Abort
  ClearErrors
  ${GetOptions} $CMDLINE "/P" $0
  IfErrors +2 0
    Abort

  ${If} $LANGUAGE == 1045
    StrCpy $1 "Zanim zaczniesz"
    StrCpy $2 "Krótka informacja o bezpieczeństwie"
    StrCpy $3 "Aplikacja nie ma jeszcze płatnego podpisu cyfrowego, dlatego Windows albo antywirus (np. Avast) mogą przy pierwszym uruchomieniu sprawdzać ją przez kilka sekund albo pokazać ostrzeżenie. To normalne dla nowych, mało znanych programów."
    StrCpy $4 "Rozumiem"
  ${Else}
    StrCpy $1 "Before you start"
    StrCpy $2 "A short note about security"
    StrCpy $3 "The app does not have a paid digital signature yet, so Windows or an antivirus (e.g. Avast) may check it for a few seconds on first start or show a warning. This is normal for new, little-known programs."
    StrCpy $4 "I understand"
  ${EndIf}
  !insertmacro MUI_HEADER_TEXT $1 $2

  nsDialogs::Create 1018
  Pop $NoticeDialog
  ${If} $NoticeDialog == error
    Abort
  ${EndIf}
  ; Tall enough for the text at 150% system scaling and in English.
  ${NSD_CreateLabel} 0 0 100% 84u $3
  Pop $0
  ${NSD_CreateCheckbox} 0 92u 100% 14u $4
  Pop $NoticeCheck
  ${NSD_OnClick} $NoticeCheck NoticeToggle

  ; "Next" stays off until "I understand" is ticked.
  GetDlgItem $0 $HWNDPARENT 1
  EnableWindow $0 0
  nsDialogs::Show
FunctionEnd

Function NoticeToggle
  Pop $0 ; the checkbox handle nsDialogs passes to the callback
  ${NSD_GetState} $NoticeCheck $0
  GetDlgItem $1 $HWNDPARENT 1
  ${If} $0 == ${BST_CHECKED}
    EnableWindow $1 1
  ${Else}
    EnableWindow $1 0
  ${EndIf}
FunctionEnd

Function NoticeLeave
  ${NSD_GetState} $NoticeCheck $0
  ${If} $0 != ${BST_CHECKED}
    Abort
  ${EndIf}
FunctionEnd
