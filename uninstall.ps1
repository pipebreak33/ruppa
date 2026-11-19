<#
 Avinstallera Ruppa på Windows:
   irm https://raw.githubusercontent.com/pipebreak33/ruppa/main/uninstall.ps1 | iex

 Tar bort %USERPROFILE%\.ruppa (Ruppa, kusinerna och deras startare), genvägen i Startmenyn och PATH-posten.
 Behåller dina data: Puppas tenants (.puppa), Muppas mallar (.muppa), Euppas tenants (.euppa), Uppas cache (.uppa) och skapade skript
 i Dokument. $env:RUPPA_YES = '1' hoppar över frågan. $env:RUPPA_HOME om Ruppa installerades någon annanstans.
#>
$ErrorActionPreference = 'Stop'
$Clock = [Diagnostics.Stopwatch]::StartNew()
$Dir = if ($env:RUPPA_HOME) { $env:RUPPA_HOME } else { Join-Path $HOME '.ruppa' }
$Bin = Join-Path $Dir 'bin'
$Lnk = Join-Path ([Environment]::GetFolderPath('Programs')) 'Ruppa.lnk'

# Language: TRUPPA_LANG, else ~/.truppa/config.json (shared by the whole troupe), else English
$TL = $env:TRUPPA_LANG
if (-not $TL) { try { $TL = (Get-Content -Raw -LiteralPath (Join-Path $HOME '.truppa/config.json') -ErrorAction Stop | ConvertFrom-Json).lang } catch { } }
function T([string]$en, [string]$sv) { if ("$TL" -like 'sv*') { $sv } else { $en } }
function Say($m) { Write-Host "  R› $m" -ForegroundColor DarkYellow }

if ((Test-Path $Dir) -and -not (Test-Path (Join-Path $Bin 'ruppa.exe')) -and ((Split-Path $Dir -Leaf) -ne '.ruppa')) {
    throw (T "$Dir does not look like a Ruppa folder. Nothing has been removed." "$Dir ser inte ut som en Ruppa-mapp. Ingenting har tagits bort.")
}
$user = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $user) { $user = '' }
$inPath = ($user -split ';') -contains $Bin

$found = @()
if (Test-Path $Dir) { $found += "    $Dir" }
if (Test-Path $Lnk) { $found += "    $Lnk" }
if ($inPath) { $found += "    $Bin i din PATH" }
if (-not $found) { Say (T 'Ruppa is not installed here. Nothing to remove.' 'Ruppa är inte installerad här. Inget att ta bort.'); return }

Write-Host (T "`n  This is removed:" "`n  Det här tas bort:")
$found | ForEach-Object { Write-Host $_ }
Write-Host (T "  Your data in .puppa, .muppa, .euppa, .uppa and Documents is kept.`n" "  Dina data i .puppa, .muppa, .euppa, .uppa och Dokument behålls.`n")
if ($env:RUPPA_YES -ne '1') {
    $svar = Read-Host (T '  Uninstall Ruppa? [y/N]' '  Avinstallera Ruppa? [j/N]')
    if ($svar -notmatch '^(j|ja|y|yes)$') { Say (T 'Cancelled. Nothing has been removed.' 'Avbrutet. Ingenting har tagits bort.'); return }
}

# stäng ett öppet Ruppa-fönster (exe-filen kan inte raderas medan den körs)
Get-Process ruppa -ErrorAction SilentlyContinue | Where-Object { $_.Path -like "$Bin*" } | Stop-Process -Force
Start-Sleep -Milliseconds 300

if (Test-Path $Dir) { Remove-Item $Dir -Recurse -Force }
if (Test-Path $Lnk) { Remove-Item $Lnk -Force }
if ($inPath) {
    $rest = ($user -split ';' | Where-Object { $_ -and $_ -ne $Bin }) -join ';'
    [Environment]::SetEnvironmentVariable('Path', $rest, 'User')
}

$Clock.Stop()
Write-Host (T "`n  ✔ Ruppa is uninstalled ($([int]$Clock.Elapsed.TotalSeconds) s).`n" "`n  ✔ Ruppa är avinstallerad ($([int]$Clock.Elapsed.TotalSeconds) s).`n") -ForegroundColor Green
