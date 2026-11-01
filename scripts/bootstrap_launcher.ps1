$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..\launcher")
flutter pub get
flutter create --platforms=windows,linux --project-name backrooms_infinity_launcher .
