# Dot-source before running cargo in PowerShell:  . .\tools\env.ps1
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
