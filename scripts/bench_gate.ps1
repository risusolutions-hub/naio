# Compare key release benchmarks against benchmarks/baseline.json (5% regression gate).
param(
    [string]$BaselinePath = "benchmarks/baseline.json",
    [double]$TolerancePct = 5
)

$ErrorActionPreference = "Stop"
$Root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $Root

if (-not (Test-Path $BaselinePath)) {
    Write-Error "Missing baseline: $BaselinePath"
}

$baseline = Get-Content $BaselinePath -Raw | ConvertFrom-Json
$tol = $TolerancePct / 100.0
$failures = @()

function Test-Minimum([string]$Name, [double]$Value, [double]$BaselineMin) {
    $floor = $BaselineMin * (1.0 - $tol)
    Write-Host ("{0}: {1} (baseline min {2}, floor {3:N2})" -f $Name, $Value, $BaselineMin, $floor)
    if ($Value -lt $floor) {
        $script:failures += "$Name regressed ($Value < $floor)"
    }
}

function Test-Maximum([string]$Name, [double]$Value, [double]$BaselineMax) {
    $ceil = $BaselineMax * (1.0 + $tol)
    Write-Host ("{0}: {1}s (baseline max {2}s, ceiling {3:N3}s)" -f $Name, $Value, $BaselineMax, $ceil)
    if ($Value -gt $ceil) {
        $script:failures += "$Name regressed ($Value > $ceil)"
    }
}

Write-Host "=== archive bench ==="
$archiveOut = (cmd /c "cargo run --release -p niao_archive --bin archive_bench 2>&1") -join "`n"
if ($LASTEXITCODE -ne 0) { throw "archive_bench failed: $archiveOut" }
if ($archiveOut -match "deflate_inflate:\s+([\d.]+)") {
    Test-Minimum "niao_archive_deflate_inflate_mib_s" ([double]$Matches[1]) $baseline.metrics.niao_archive_deflate_inflate_mib_s
} else {
    throw "could not parse archive_bench output"
}

Write-Host "=== vm math stress ==="
cmd /c "cargo test -p niao_vm --release vm_runs_math_stress --no-run 2>nul" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "vm build failed" }
$sw = [System.Diagnostics.Stopwatch]::StartNew()
cmd /c "cargo test -p niao_vm --release vm_runs_math_stress -- --nocapture 2>nul" | Out-Null
if ($LASTEXITCODE -ne 0) { throw "vm_runs_math_stress failed" }
$sw.Stop()
Test-Maximum "niao_vm_math_stress_secs" ($sw.Elapsed.TotalSeconds) $baseline.metrics.niao_vm_math_stress_secs

Write-Host "=== io bench (optional) ==="
if (Get-Command python -ErrorAction SilentlyContinue) {
    $ioOut = (cmd /c "python benchmarks/benchmark_io.py 2>&1") -join "`n"
    if ($ioOut -match "spawn:\s+([\d.]+)M") {
        $jobs = [double]$Matches[1] * 1000000
        Test-Minimum "niao_io_spawn_jobs_per_s" $jobs $baseline.metrics.niao_io_spawn_jobs_per_s
    }
}

if ($failures.Count -gt 0) {
    Write-Host "REGRESSIONS:"
    $failures | ForEach-Object { Write-Host "  - $_" }
    exit 1
}

Write-Host "bench_gate: OK"
exit 0
