$ErrorActionPreference = "Stop"

$Repo = "madzarm/ccsearch"
$Target = "x86_64-pc-windows-msvc"
$AssetName = "ccsearch-$Target"
$InstallDir = "$env:USERPROFILE\.ccsearch\bin"

Write-Host "Fetching latest release..."
$Release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$Tag = $Release.tag_name

if (-not $Tag) {
    Write-Error "Could not determine latest release"
    exit 1
}

Write-Host "Installing ccsearch $Tag..."

$Url = "https://github.com/$Repo/releases/download/$Tag/$AssetName.zip"
$TmpDir = New-TemporaryFile | ForEach-Object { Remove-Item $_; New-Item -ItemType Directory -Path $_ }
$ZipPath = Join-Path $TmpDir "$AssetName.zip"

Invoke-WebRequest -Uri $Url -OutFile $ZipPath
Expand-Archive -Path $ZipPath -DestinationPath $TmpDir -Force

if (-not (Test-Path $InstallDir)) {
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
}

Copy-Item (Join-Path $TmpDir "ccsearch.exe") (Join-Path $InstallDir "ccsearch.exe") -Force
Remove-Item $TmpDir -Recurse -Force

# Add to PATH if not already there
$UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($UserPath -notlike "*$InstallDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$UserPath;$InstallDir", "User")
    Write-Host "Added $InstallDir to PATH (restart your terminal to use)"
}

Write-Host ""
Write-Host "ccsearch $Tag installed to $InstallDir\ccsearch.exe"
Write-Host "Run 'ccsearch --help' to get started."
