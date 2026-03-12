<#
.SYNOPSIS
    Build Docker image and push to repository.

.DESCRIPTION
    Builds the Docker image from Dockerfile in the script directory,
    tags it, and pushes to the given repository.
    Tag defaults to package version from Cargo.toml if not specified.

.PARAMETER Repository
    Optional. Registry/repository prefix (e.g. "ghcr.io/myorg" or "myregistry.azurecr.io/myrepo").
    If omitted, image is only built and tagged locally without push.

.PARAMETER ImageName
    Optional. Name of the image. Default: "rust-core-transaction".

.PARAMETER Tag
    Optional. Image tag. Defaults to version from Cargo.toml (e.g. "0.1.9").

.EXAMPLE
    .\docker-build-push.ps1 -ImageName rust-core-transaction
    Builds and tags as rust-core-transaction:<version>, no push.

.EXAMPLE
    .\docker-build-push.ps1 -Repository ghcr.io/myorg -ImageName rust-core-transaction
    Builds, tags as ghcr.io/myorg/rust-core-transaction:0.1.9, pushes.

.EXAMPLE
    .\docker-build-push.ps1 -Repository myregistry.azurecr.io/apps -ImageName etc-middleware -Tag 1.0.0
    Builds, tags as myregistry.azurecr.io/apps/etc-middleware:1.0.0, pushes.
#>

param(
    [Parameter(Mandatory = $false)]
    [string]$Repository,

    [Parameter(Mandatory = $false)]
    [string]$ImageName = "rust-core-transaction",

    [Parameter(Mandatory = $false)]
    [string]$Tag
)

$ErrorActionPreference = "Stop"
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
Set-Location $ScriptDir

# Resolve tag: use parameter or read version from Cargo.toml
if ([string]::IsNullOrWhiteSpace($Tag)) {
    $cargoPath = Join-Path $ScriptDir "Cargo.toml"
    if (-not (Test-Path $cargoPath)) {
        Write-Error "Cargo.toml not found at $cargoPath"
    }
    $content = Get-Content $cargoPath -Raw
    if ($content -match 'version\s*=\s*"([^"]+)"') {
        $Tag = $Matches[1].Trim()
    } else {
        Write-Error "Could not parse version from Cargo.toml"
    }
    Write-Host "Using tag from Cargo.toml: $Tag" -ForegroundColor Cyan
} else {
    Write-Host "Using tag: $Tag" -ForegroundColor Cyan
}

# Full image reference for build (local name)
$localImage = "${ImageName}:${Tag}"

Write-Host "Building Docker image: $localImage" -ForegroundColor Green
docker build -t $localImage .

if ($LASTEXITCODE -ne 0) {
    Write-Error "Docker build failed."
}

# If repository provided, tag for remote and push
if (-not [string]::IsNullOrWhiteSpace($Repository)) {
    $remoteImage = $Repository.TrimEnd('/') + "/" + $ImageName + ":" + $Tag
    Write-Host "Tagging as: $remoteImage" -ForegroundColor Green
    docker tag $localImage $remoteImage
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Docker tag failed."
    }
    Write-Host "Pushing to repository: $remoteImage" -ForegroundColor Green
    docker push $remoteImage
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Docker push failed."
    }
    Write-Host "Done. Image pushed: $remoteImage" -ForegroundColor Green
    Write-Host "Removing local image(s): $remoteImage, $localImage" -ForegroundColor Cyan
    docker rmi $remoteImage $localImage
    if ($LASTEXITCODE -eq 0) {
        Write-Host "Local image(s) removed." -ForegroundColor Green
    }
} else {
    Write-Host "Done. Image built locally: $localImage (no repository specified, skip push)" -ForegroundColor Green
}
