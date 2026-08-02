# Perfect the Niao install folder: versioned niao_libs, catalog.json, install.json, cleanup.
param(
    [string]$Source = (Join-Path (Split-Path -Parent $PSScriptRoot) "niao_libs"),
    [string]$NiaoHome = "",
    [switch]$AllVersions,
    [switch]$WhatIf
)

$ErrorActionPreference = "Stop"
$RepoRoot = Split-Path -Parent $PSScriptRoot

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

function Resolve-NiaoHome {
    if ($NiaoHome) { return $NiaoHome }
    $nm = Join-Path $RepoRoot "target\release\nm.exe"
    if (Test-Path $nm) {
        $detected = & $nm home 2>$null
        if ($detected) { return $detected.Trim() }
    }
    return Join-Path $env:USERPROFILE ".niao"
}

function Now-Ms { [string][DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds() }

function Read-PackageMeta([string]$PkgDir, [string]$ActiveVer) {
    $pkgPath = Join-Path $PkgDir "package.json"
    $libPath = Join-Path (Join-Path $PkgDir $ActiveVer) "lib.json"
    $pkg = $null
    $lib = $null
    if (Test-Path $pkgPath) { $pkg = Get-Content $pkgPath -Raw | ConvertFrom-Json }
    if (Test-Path $libPath) { $lib = Get-Content $libPath -Raw | ConvertFrom-Json }

    $name = if ($pkg.name) { $pkg.name } elseif ($lib.name) { $lib.name } else { Split-Path $PkgDir -Leaf }
    $version = if ($lib.version) { $lib.version } elseif ($pkg.version) { $pkg.version } else { $ActiveVer }
    $kind = if ($lib.kind) { $lib.kind } elseif ($pkg.kind) { $pkg.kind } else { "native" }
    $description = if ($lib.description) { $lib.description } elseif ($pkg.description) { $pkg.description } else { "" }
    $importPaths = @()
    if ($lib.import_paths) { $importPaths = @($lib.import_paths | Where-Object { $_ }) }
    elseif ($pkg.import_paths) { $importPaths = @($pkg.import_paths | Where-Object { $_ }) }
    $builtinCount = 0
    if ($null -ne $lib.builtin_count) { $builtinCount = [int]$lib.builtin_count }
    elseif ($null -ne $pkg.builtin_count) { $builtinCount = [int]$pkg.builtin_count }

    return [ordered]@{
        name          = $name
        version       = $version
        kind          = $kind
        description   = $description
        import_paths  = $importPaths
        builtin_count = $builtinCount
    }
}

function Write-JsonFile([string]$Path, $Object) {
    $json = $Object | ConvertTo-Json -Depth 8 -Compress:$false
    $utf8 = New-Object System.Text.UTF8Encoding $false
    [System.IO.File]::WriteAllText($Path, $json + "`n", $utf8)
}

$niaoHome = Resolve-NiaoHome
$destRoot = Join-Path $niaoHome "niao_libs"

if (-not (Test-Path $Source)) { throw "Source not found: $Source" }
New-Item -ItemType Directory -Force -Path $destRoot | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $niaoHome "bin") -ErrorAction SilentlyContinue | Out-Null

Write-Host "Perfecting Niao home"
Write-Host "  Home:        $niaoHome"
Write-Host "  Source:      $Source"
Write-Host "  niao_libs:   $destRoot"
Write-Host "  Mode:        $(if ($AllVersions) { 'all versions per package' } else { 'latest version per package' })"
Write-Host ""

# Remove orphan packages not in source
$sourceNames = @(Get-ChildItem $Source -Directory | Where-Object { $_.Name -ne 'node_modules' } | ForEach-Object { $_.Name })
if (-not $WhatIf) {
    Get-ChildItem $destRoot -Directory -ErrorAction SilentlyContinue | ForEach-Object {
        if ($sourceNames -notcontains $_.Name) {
            Write-Host "  remove orphan: $($_.Name)"
            Remove-Item $_.FullName -Recurse -Force
        }
    }
}

$pkgCount = 0
$verCount = 0
$libs = [ordered]@{}

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
        $latestVer = (Get-LatestVersionDir (Get-VersionDirs $srcPkg)).Name
        $libJson = Join-Path (Join-Path $srcPkg $latestVer) "lib.json"
        if (Test-Path $libJson) {
            Copy-Item $libJson (Join-Path $destPkg "package.json") -Force
        }
    }

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

    $activeVer = if ($AllVersions) { (Get-Content (Join-Path $destPkg "package.json") -Raw | ConvertFrom-Json).version } else { (Get-LatestVersionDir @(Get-VersionDirs $destPkg)).Name }
    if (-not $activeVer) { $activeVer = (Get-Content (Join-Path $destPkg "package.json") -Raw | ConvertFrom-Json).version }

    $meta = Read-PackageMeta $destPkg $activeVer
    $meta.installed_at = Now-Ms
    $libs[$name] = $meta
    $pkgCount++
}

if ($WhatIf) {
    Write-Host ""
    Write-Host "[whatif] would sync $pkgCount packages, $verCount version folders"
    return
}

# catalog.json
$niaoVersion = "0.2.3"
$catalogSrc = Join-Path $Source "catalog.json"
if (Test-Path $catalogSrc) {
    $srcCatalog = Get-Content $catalogSrc -Raw | ConvertFrom-Json
    if ($srcCatalog.niao_version) { $niaoVersion = $srcCatalog.niao_version }
}

$ts = Now-Ms
$catalog = [ordered]@{
    niao_version = $niaoVersion
    updated_at   = $ts
    libs         = $libs
}
Write-JsonFile (Join-Path $destRoot "catalog.json") $catalog

# install.json — must list every installed lib
$installPath = Join-Path $niaoHome "install.json"
$existing = $null
if (Test-Path $installPath) {
    try { $existing = Get-Content $installPath -Raw | ConvertFrom-Json } catch { }
}

$install = [ordered]@{
    niao_version = $niaoVersion
    mode         = if ($existing.mode) { $existing.mode } else { "global" }
    installed_at = $ts
    root         = $niaoHome
    source_root  = if ($existing.source_root) { $existing.source_root } else { $RepoRoot }
    libs         = $libs
}
Write-JsonFile $installPath $install

# Cleanup stale backup binaries in bin/
$binDir = Join-Path $niaoHome "bin"
if (Test-Path $binDir) {
    Get-ChildItem $binDir -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '\.old$' } |
        ForEach-Object {
            Write-Host "  remove stale: bin/$($_.Name)"
            Remove-Item $_.FullName -Force
        }
}

Write-Host ""
Write-Host "Done: $pkgCount packages, $verCount version folders"
Write-Host "  catalog.json  -> $destRoot\catalog.json"
Write-Host "  install.json  -> $installPath"
