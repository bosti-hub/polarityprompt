# Windows version of backup.sh: copies the source tree (incl. local git history, excl. build
# artifacts and models) to OneDrive, plus a dated zip snapshot. Keeps the last 14 snapshots.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\backup.ps1   (run from anywhere)
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
$DestRoot = $env:POLARITYPROMPT_BACKUP_DIR
if (-not $DestRoot) {
    if (-not $env:OneDrive) { throw 'OneDrive not found. Set POLARITYPROMPT_BACKUP_DIR to the backup folder.' }
    $DestRoot = Join-Path $env:OneDrive 'Claude\PolarityPrompt\backup'
}
$Latest = Join-Path $DestRoot 'latest'
$Snapshots = Join-Path $DestRoot 'snapshots'
$Stamp = Get-Date -Format 'yyyy-MM-dd_HHmm'
New-Item -ItemType Directory -Force -Path $Latest, $Snapshots | Out-Null

# 1. Mirror: always the current state of the source. robocopy exit codes below 8 mean success.
robocopy $RepoRoot $Latest /MIR /XD target node_modules models dist /XF .DS_Store /NFL /NDL /NJH /NJS /NP | Out-Null
if ($LASTEXITCODE -ge 8) { throw "robocopy failed with exit code $LASTEXITCODE" }

# 2. Dated snapshot: a zip of the mirror, so an accidental bad state can be rolled back by date.
#    ZipFile instead of Compress-Archive, which skips hidden folders such as .git.
Add-Type -AssemblyName System.IO.Compression.FileSystem
$Zip = Join-Path $Snapshots "polarityprompt-$Stamp.zip"
if (Test-Path $Zip) { Remove-Item $Zip }
[System.IO.Compression.ZipFile]::CreateFromDirectory($Latest, $Zip)

# 3. Keep the last 14 snapshots.
Get-ChildItem $Snapshots -Filter 'polarityprompt-*.zip' | Sort-Object LastWriteTime -Descending |
    Select-Object -Skip 14 | Remove-Item

Write-Host "Backup done -> $Latest (mirror) and snapshots\polarityprompt-$Stamp.zip"
