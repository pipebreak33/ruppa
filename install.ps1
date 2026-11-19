<#
 Ruppa för Windows (Windows PowerShell 5.1 eller PowerShell 7). Installera eller uppdatera:
   irm https://raw.githubusercontent.com/pipebreak33/ruppa/main/install.ps1 | iex

 Allt hamnar i %USERPROFILE%\.ruppa:
   bin\ruppa.exe                 programmet (plus uppa.cmd, puppa.cmd, muppa.cmd, euppa.cmd)
   uppa, puppa, muppa, euppa     kusinerna
 Dessutom en genväg i Startmenyn. Inga adminrättigheter behövs.

 $env:RUPPA_REPO = 'ägare/repo' och $env:RUPPA_VERSION = 'v1.2.3' styr källan.
 $env:RUPPA_BINARY = 'C:\sökväg\ruppa.exe' installerar en lokal binär. $env:RUPPA_HOME flyttar mappen.
#>
$ErrorActionPreference = 'Stop'
$Clock = [Diagnostics.Stopwatch]::StartNew()
$ProgressPreference = 'SilentlyContinue'
if ($PSVersionTable.PSEdition -ne 'Core') {
    [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
}
$OnWindows = ($PSVersionTable.PSEdition -ne 'Core') -or $IsWindows
if (-not $OnWindows) {
    throw (T 'On macOS and Linux: curl -fsSL https://raw.githubusercontent.com/pipebreak33/ruppa/main/install.sh | sh' 'På macOS och Linux: curl -fsSL https://raw.githubusercontent.com/pipebreak33/ruppa/main/install.sh | sh')
}
$Repo = if ($env:RUPPA_REPO) { $env:RUPPA_REPO } else { 'pipebreak33/ruppa' }
$Dir  = if ($env:RUPPA_HOME) { $env:RUPPA_HOME } else { Join-Path $HOME '.ruppa' }
$Bin  = Join-Path $Dir 'bin'
$Share = Join-Path $Dir 'share'

# Language: TRUPPA_LANG, else ~/.truppa/config.json (shared by the whole troupe), else English
$TL = $env:TRUPPA_LANG
if (-not $TL) { try { $TL = (Get-Content -Raw -LiteralPath (Join-Path $HOME '.truppa/config.json') -ErrorAction Stop | ConvertFrom-Json).lang } catch { } }
function T([string]$en, [string]$sv) { if ("$TL" -like 'sv*') { $sv } else { $en } }
function Say($m)  { Write-Host "  R› $m" -ForegroundColor DarkYellow }
function Warn($m) { Write-Host "  ! $m" -ForegroundColor Yellow }

$arch = if ($env:PROCESSOR_ARCHITECTURE -eq 'ARM64' -or $env:PROCESSOR_ARCHITEW6432 -eq 'ARM64') { 'aarch64' } else { 'x86_64' }
$asset = "ruppa-windows-$arch.exe"
New-Item -ItemType Directory -Force -Path $Bin, $Share | Out-Null
$tmp = Join-Path ([IO.Path]::GetTempPath()) ("ruppa-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $tmp | Out-Null
try {
    # 1. Programmet
    $new = Join-Path $tmp 'ruppa.exe'
    if ($env:RUPPA_BINARY) {
        Say "Installerar $env:RUPPA_BINARY"
        Copy-Item $env:RUPPA_BINARY $new
    } else {
        $base = if ($env:RUPPA_VERSION) { "https://github.com/$Repo/releases/download/$($env:RUPPA_VERSION)" }
                else { "https://github.com/$Repo/releases/latest/download" }
        Say (T "Fetching $asset from github.com/$Repo" "Hämtar $asset från github.com/$Repo")
        Invoke-WebRequest "$base/$asset" -OutFile $new -UseBasicParsing
        $sums = (Invoke-WebRequest "$base/SHA256SUMS" -UseBasicParsing).Content
        if ($sums -is [byte[]]) { $sums = [Text.Encoding]::UTF8.GetString($sums) }
        $want = ($sums -split "`n" | Where-Object { $_ -match "\s\*?$([regex]::Escape($asset))\s*$" } |
                 ForEach-Object { ($_ -split '\s+')[0] } | Select-Object -First 1)
        $got = (Get-FileHash $new -Algorithm SHA256).Hash.ToLower()
        if (-not $want -or $want.ToLower() -ne $got) { throw (T 'The checksum does not match. Nothing has changed.' 'Kontrollsumman stämmer inte. Inget har ändrats.') }
    }
    # ruppa.exe är en fönsterapp: kör den med -Wait för att få exitkoden
    $p = Start-Process $new -ArgumentList '--version' -Wait -PassThru -NoNewWindow
    if ($p.ExitCode -ne 0) { throw (T 'The downloaded binary does not run.' 'Den nedladdade binären går inte att köra.') }
    $target = Join-Path $Bin 'ruppa.exe'
    # en körande ruppa.exe kan inte skrivas över, men den kan döpas om
    if (Test-Path $target) {
        $old = "$target.old"
        Remove-Item $old -Force -ErrorAction SilentlyContinue
        Rename-Item $target (Split-Path $old -Leaf) -Force
    }
    Move-Item $new $target -Force
    Say (T "Ruppa installed in $Bin" "Ruppa installerad i $Bin")
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# 2. Ikonen (för genvägen; exe-filen har den redan inbakad)
$icon = Join-Path $Share 'ruppa.ico'
if ($PSScriptRoot -and (Test-Path (Join-Path $PSScriptRoot 'assets\icon\ruppa.ico'))) {
    Copy-Item (Join-Path $PSScriptRoot 'assets\icon\ruppa.ico') $icon -Force
} else {
    try { Invoke-WebRequest "https://raw.githubusercontent.com/$Repo/main/assets/icon/ruppa.ico" -OutFile $icon -UseBasicParsing } catch { }
}

# 3. Genväg i Startmenyn
$programs = [Environment]::GetFolderPath('Programs')
$lnk = Join-Path $programs 'Ruppa.lnk'
$sh = New-Object -ComObject WScript.Shell
$s = $sh.CreateShortcut($lnk)
$s.TargetPath = $target
$s.Arguments = 'gui'
$s.WorkingDirectory = $HOME
$s.Description = (T 'Uppa, Puppa, Muppa and Euppa in one window' 'Uppa, Puppa, Muppa och Euppa i ett fönster')
if (Test-Path $icon) { $s.IconLocation = "$icon,0" } else { $s.IconLocation = "$target,0" }
$s.Save()
Say (T 'Ruppa is in the Start menu' 'Ruppa finns i Startmenyn')

# 4. bin i användarens PATH (för uppa, puppa, muppa och euppa i terminalen)
$user = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not $user) { $user = '' }
if (($user -split ';') -notcontains $Bin) {
    [Environment]::SetEnvironmentVariable('Path', (($user.TrimEnd(';') + ';' + $Bin).TrimStart(';')), 'User')
    Say "La till $Bin i din PATH"
}
if (($env:Path -split ';') -notcontains $Bin) { $env:Path = "$Bin;$env:Path" }

# 5. Kusinerna (Python installeras per användare med winget om det saknas)
Write-Host ''
$p = Start-Process $target -ArgumentList 'install', '--no-anim' -Wait -PassThru -NoNewWindow
if ($p.ExitCode -ne 0) { Warn (T 'One of the tools could not be installed. Open Ruppa and try again.' 'Någon kusin gick inte att installera. Öppna Ruppa och försök igen.') }

$Clock.Stop()
$secs = [int]$Clock.Elapsed.TotalSeconds
$took = if ($secs -ge 60) { "$([int][math]::Floor($secs / 60)) min $($secs % 60) s" } else { "$secs s" }
Write-Host (T "`n  ✔ Done in $took! Start Ruppa from the Start menu." "`n  ✔ Klart på $took! Starta Ruppa från Startmenyn.") -ForegroundColor Green
Write-Host "  $(T 'Uninstall:' 'Avinstallera:') irm https://raw.githubusercontent.com/$Repo/main/uninstall.ps1 | iex`n" -ForegroundColor DarkGray
if (-not $env:RUPPA_NO_OPEN) { Start-Process $target -ArgumentList 'gui' }
