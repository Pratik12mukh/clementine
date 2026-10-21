# Installs the free programs Clementine uses for conversions.
# Run in PowerShell (Windows 10/11 with winget). Safe to re-run.
$tools = @(
  @{ id = 'Gyan.FFmpeg';                        name = 'FFmpeg (images, audio, video)' },
  @{ id = 'JohnMacFarlane.Pandoc';              name = 'Pandoc (documents)' },
  @{ id = 'TheDocumentFoundation.LibreOffice';  name = 'LibreOffice (Office files, PDF import)' },
  @{ id = 'ArtifexSoftware.GhostScript';        name = 'Ghostscript (PDF pages, merge, split)' },
  @{ id = '7zip.7zip';                          name = '7-Zip (archives)' }
)
foreach ($t in $tools) {
  Write-Host "Installing $($t.name)..." -ForegroundColor Yellow
  winget install -e --id $t.id --silent --accept-package-agreements --accept-source-agreements
}
Write-Host "Done. Restart Clementine so it picks them up." -ForegroundColor Green
