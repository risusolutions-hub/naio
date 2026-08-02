# Sync niao_libs to canonical layout: <pkg>/package.json + <pkg>/<version>/lib.json
# Source: repo niao_libs (all version dirs). Destination: Niao home or custom path.
param(
    [string]$Source = (Join-Path (Split-Path -Parent $PSScriptRoot) "niao_libs"),
    [string]$Destination = "",
    [switch]$AllVersions,
    [switch]$WhatIf
)

$ErrorActionPreference = "Stop"

function Compare-Version([string]$a, [string]$b) {
    $pa = $a.Split('.') | ForEach-Object { [int]($_ -replace '\D.*$', '0') }
    $pb = $b.Split('.') | ForEach-Object { [int]($_ -replace '\D.*$', '0') }
    $len = [Math]::Max($pa.Count, $pb.Count)
    for ($i = 0; $i -lt $len; $i++) {
        $va = if ($i -lt $pa.Count) { $pa[$i] } else { 0 }
        $vb = if ($i -lt $pb.Count) { $pb[$i] } else { 0 }
        if ($va -ne $vb) { return $va - $vb }
    }
    return 0
}

function Get-VersionDirs([string]$PkgDir) {
    @(Get-ChildItem $PkgDir -Directory -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^\d' })
}

function Get-LatestVersionDir($versionDirs) {
    $latest = $null
    foreach ($v in $versionDirs) {
        if (-not $latest -or (Compare-Version $v.Name $latest.Name) -gt 0) {
            $latest = $v
        }
    }
    return $latest
}

function Resolve-Destination {
    if ($Destination) { return $Destination }
    $nm = Join-Path (Split-Path -Parent $PSScriptRoot) "target\release\nm.exe"
    if (Test-Path $nm) {
        $home = & $nm home 2>$null
        if ($home) { return Join-Path $home.Trim() "niao_libs" }
    }
    return Join-Path $env:USERPROFILE ".niao\niao_libs"
}

$destRoot = Resolve-Destination
if (-not (Test-Path $Source)) { throw "Source not found: $Source" }
New-Item -ItemType Directory -Force -Path $destRoot | Out-Null

Write-Host "Organizing Niao libraries"
Write-Host "  Source:      $Source"
Write-Host "  Destination: $destRoot"
Write-Host "  Mode:        $(if ($AllVersions) { 'all versions' } else { 'latest per package' })"
Write-Host ""

$pkgCount = 0
$verCount = 0

Get-ChildItem $Source -Directory | Where-Object { $_.Name -ne 'node_modules' } | ForEach-Object {
    $name = $_.Name
    $srcPkg = $_.FullName
    $destPkg = Join-Path $destRoot $name
    $pkgJsonSrc = Join-Path $srcPkg "package.json"

    $versionDirs = @(Get-VersionDirs $srcPkg)
    if ($versionDirs.Count -eq 0) {
        Write-Warning "skip $name : no version directories"
        return
    }

    if (-not $AllVersions) {
        $versionDirs = @(Get-LatestVersionDir $versionDirs)
    }

    if ($WhatIf) {
        Write-Host "[whatif] $name -> $($versionDirs.Name -join ', ')"
        $pkgCount++
        $verCount += $versionDirs.Count
        return
    }

    New-Item -ItemType Directory -Force -Path $destPkg | Out-Null

    if (Test-Path $pkgJsonSrc) {
        Copy-Item $pkgJsonSrc $destPkg -Force
    } else {
        $latestVer = (Get-LatestVersionDir $versionDirs).Name
        $libJson = Join-Path (Join-Path $srcPkg $latestVer) "lib.json"
        if (Test-Path $libJson) {
            Copy-Item $libJson (Join-Path $destPkg "package.json") -Force
        }
    }

    # Remove version dirs not in sync set (installed layout keeps only active versions)
    if (-not $AllVersions) {
        Get-ChildItem $destPkg -Directory -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -match '^\d' -and ($versionDirs.Name -notcontains $_.Name) } |
            ForEach-Object { Remove-Item $_.FullName -Recurse -Force }
    }

    foreach ($ver in $versionDirs) {
        $destVer = Join-Path $destPkg $ver.Name
        New-Item -ItemType Directory -Force -Path $destVer | Out-Null
        $libSrc = Join-Path $ver.FullName "lib.json"
        if (-not (Test-Path $libSrc)) {
            Write-Warning "skip $name/$($ver.Name) : missing lib.json"
            continue
        }
        Copy-Item $libSrc (Join-Path $destVer "lib.json") -Force
        $verCount++
    }
    $pkgCount++
}

# Rebuild catalog.json from installed packages
if (-not $WhatIf) {
    $libs = @{}
    $niaoVersion = "0.2.3"
    $catalogSrc = Join-Path $Source "catalog.json"
    if (Test-Path $catalogSrc) {
        $srcCatalog = Get-Content $catalogSrc -Raw | ConvertFrom-Json
        if ($srcCatalog.niao_version) { $niaoVersion = $srcCatalog.niao_version }
    }

    Get-ChildItem $destRoot -Directory | ForEach-Object {
        $pkgPath = Join-Path $_.FullName "package.json"
        if (-not (Test-Path $pkgPath)) { return }
        $pkg = Get-Content $pkgPath -Raw | ConvertFrom-Json
        $libs[$pkg.name] = @{
            name          = $pkg.name
            version       = $pkg.version
            kind          = $pkg.kind
            description   = $pkg.description
            import_paths  = @($pkg.import_paths)
            builtin_count = $pkg.builtin_count
            installed_at  = [string][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        }
    }

    $catalog = @{
        niao_version = $niaoVersion
        updated_at   = [string][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
        libs         = $libs
    }
    $catalog | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $destRoot "catalog.json") -Encoding UTF8
}

Write-Host ""
Write-Host "Done: $pkgCount packages, $verCount version folders"
