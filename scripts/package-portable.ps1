[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ExecutablePath,
    [Parameter(Mandatory = $true)]
    [string]$OutputDirectory
)

$ErrorActionPreference = "Stop"
$releaseVersion = "1.0.0"
$portableName = "Chronicle-$releaseVersion-windows-x64-portable"

if (-not (Test-Path -LiteralPath $ExecutablePath -PathType Leaf)) {
    Write-Error "Built executable was not found: $ExecutablePath"
    exit 1
}

$resolvedExecutable = (Resolve-Path -LiteralPath $ExecutablePath).Path
$resolvedOutput = [System.IO.Path]::GetFullPath($OutputDirectory)
$stagingDirectory = Join-Path $resolvedOutput $portableName
$archivePath = Join-Path $resolvedOutput "$portableName.zip"

if (Test-Path -LiteralPath $stagingDirectory) {
    throw "Portable staging directory already exists: $stagingDirectory"
}
if (Test-Path -LiteralPath $archivePath -PathType Leaf) {
    throw "Portable archive already exists: $archivePath"
}

New-Item -ItemType Directory -Force -Path $resolvedOutput | Out-Null
New-Item -ItemType Directory -Path $stagingDirectory | Out-Null

try {
    Copy-Item -LiteralPath $resolvedExecutable -Destination (Join-Path $stagingDirectory "Chronicle.exe")
    New-Item -ItemType File -Path (Join-Path $stagingDirectory "portable.marker") | Out-Null
    @(
        "Chronicle portable edition",
        "",
        "Extract the archive and run Chronicle.exe.",
        "On first launch, Chronicle-data is created beside the executable for the library, snapshots, and settings.",
        "The portable edition never reads or writes App Local Data; keep Chronicle.exe, portable.marker, and Chronicle-data together."
    ) | Set-Content -Encoding utf8 -Path (Join-Path $stagingDirectory "README.txt")

    Compress-Archive -Path (Join-Path $stagingDirectory "*") -DestinationPath $archivePath
}
finally {
    if (Test-Path -LiteralPath $stagingDirectory -PathType Container) {
        Remove-Item -LiteralPath $stagingDirectory -Recurse -Force
    }
}

Write-Output "Created portable archive: $archivePath"
