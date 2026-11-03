; Remove the TUN service (if it was ever installed) together with the app.
; The service lives outside $INSTDIR in an admin-only folder, so ask for
; elevation once; declining just leaves the service in place.
!macro NSIS_HOOK_PREUNINSTALL
  IfFileExists "$PROGRAMFILES64\Choclite\Service\clash-rs-desktop-service.exe" 0 choclite_no_service
    ExecShellWait "runas" "$PROGRAMFILES64\Choclite\Service\clash-rs-desktop-service.exe" "--uninstall" SW_HIDE
  choclite_no_service:
!macroend
