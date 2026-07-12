# Runs all Niao cursor-agent tasks in order, with git checkpoints and build verification.
# Usage (from C:\Risu\Neko):  .\cursor-tasks\run_all.ps1          # all tasks
#        .\cursor-tasks\run_all.ps1 -From 3 -To 5                 # a range
param([int]$From = 1, [int]$To = 12)

$ErrorActionPreference = "Stop"
Set-Location "C:\Risu\Neko"

$tasks = Get-ChildItem "cursor-tasks\task-*.md" | Sort-Object Name
foreach ($t in $tasks) {
    $num = [int]($t.Name -replace 'task-(\d+).*','$1')
    if ($num -lt $From -or $num -gt $To) { continue }

    Write-Host "`n=== TASK $num : $($t.Name) ===" -ForegroundColor Cyan

    git add -A
    git commit -m "checkpoint before $($t.Name)" --allow-empty | Out-Null

    cursor-agent -p (Get-Content $t.FullName -Raw) --force
    if ($LASTEXITCODE -ne 0) { Write-Host "cursor-agent failed on task $num" -ForegroundColor Red; exit 1 }

    Write-Host "--- verifying build ---" -ForegroundColor Yellow
    cargo check --workspace
    if ($LASTEXITCODE -ne 0) {
        Write-Host "BUILD BROKEN after task $num - stopping. Fix or 'git reset --hard HEAD' to revert." -ForegroundColor Red
        exit 1
    }
    cargo test --workspace
    if ($LASTEXITCODE -ne 0) {
        Write-Host "TESTS FAILED after task $num - stopping. Fix or 'git reset --hard HEAD' to revert." -ForegroundColor Red
        exit 1
    }

    git add -A
    git commit -m "task $num complete: $($t.Name)" --allow-empty | Out-Null
    Write-Host "=== TASK $num DONE ===" -ForegroundColor Green
}
Write-Host "`nAll requested tasks complete." -ForegroundColor Green
