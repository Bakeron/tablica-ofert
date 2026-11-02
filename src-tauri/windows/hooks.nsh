; Uninstalling removes the hourly Task Scheduler task (zalozenia.md section 3).
!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec 'schtasks.exe /Delete /TN "TablicaOfert\Pobieranie ofert" /F'
!macroend
