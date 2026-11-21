$ErrorActionPreference = 'Stop'
if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
  Write-Error 'winget is not installed. Install FFmpeg manually and add ffmpeg.exe and ffprobe.exe to PATH.'
}
winget install --id Gyan.FFmpeg --exact --accept-package-agreements --accept-source-agreements
Write-Host ''
Write-Host 'Close and reopen your terminal, then verify:' -ForegroundColor Green
Write-Host '  ffmpeg -version'
Write-Host '  ffprobe -version'
