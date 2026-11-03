; Puts the install folder (which holds the bundled rowcraft.exe CLI) on the current user's PATH,
; and takes it off again on uninstall. PowerShell does the edit so only our entry is touched and
; the existing value keeps its REG_EXPAND_SZ type.

!macro NSIS_HOOK_POSTINSTALL
  nsExec::Exec `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d='$INSTDIR'; $$p=[Environment]::GetEnvironmentVariable('Path','User'); $$parts=@(); if($$p){$$parts=$$p -split ';' | Where-Object { $$_ }}; if(-not ($$parts | Where-Object { $$_.TrimEnd('\') -ieq $$d.TrimEnd('\') })){ [Environment]::SetEnvironmentVariable('Path', (($$parts + $$d) -join ';'), 'User') }"`
  Pop $0
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec `powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command "$$d='$INSTDIR'; $$p=[Environment]::GetEnvironmentVariable('Path','User'); if($$p){ $$parts=$$p -split ';' | Where-Object { $$_ -and ($$_.TrimEnd('\') -ine $$d.TrimEnd('\')) }; [Environment]::SetEnvironmentVariable('Path', ($$parts -join ';'), 'User') }"`
  Pop $0
!macroend
