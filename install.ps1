$ErrorActionPreference = "Stop"
$repo = "Endlad2/Infinity-Backrooms"
$root = Join-Path $env:APPDATA ".backrooms_infinity"
$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -eq "BackroomsInfinity-windows-x64.zip" } | Select-Object -First 1
if (-not $asset) { throw "Windows x64 release asset not found" }
$tmp = Join-Path $env:TEMP ("backrooms-infinity-" + [guid]::NewGuid())
New-Item -ItemType Directory -Force -Path $tmp, $root | Out-Null
$zip = Join-Path $tmp "release.zip"
Invoke-WebRequest $asset.browser_download_url -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $root -Force
Remove-Item $tmp -Recurse -Force
Write-Host "Installed to $root"
Write-Host "Run: $(Join-Path $root 'bin\backrooms_infinity_launcher.exe')"
