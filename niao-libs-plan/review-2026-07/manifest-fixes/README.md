# Manifest fixes (staged — not yet applied)

35 corrected JSON manifests for libs whose `package.json` / `lib.json` were corrupted. **Nothing here
has been copied into `niao_libs/` yet** — review, then apply.

## What was wrong

| Problem | How it broke | Libs |
|---|---|---|
| **Trailing NUL bytes** (`\x00\x00\x00`) after the closing `}` | JSON parsers fail with *"Extra data"* | core, dsa, io, json, nenv, net, nmongo, nos, npg, nsqlite, parallel, re, time |
| **UTF-8 BOM** (`﻿`) at start of `0.2.2/lib.json` | Some strict parsers reject BOM | same set |
| **Physically truncated** (cut mid-`builtin_count`) | Unparseable | `io` (both files) |
| **Mojibake** — em-dash double-encoded cp1252→utf-8 (`â€"`) | Garbled description text | ncl, nml, nvis |

## What the fix does

- Strips NUL bytes and the BOM; re-serializes clean UTF-8 with LF newlines and 4-space indent.
- Reconstructs the two truncated `io` manifests from their visible fields and sets
  **`builtin_count: 55`** (counted from `crates/niao_runtime/src/io.rs`).
- Re-decodes mojibake descriptions back to real em-dashes (`—`).

## Apply (after review)

Each file is named `<lib>__<which>.json`. Map it back:

- `core__package.json`        → `niao_libs/core/package.json`
- `core__lib_0.2.2.json`      → `niao_libs/core/0.2.2/lib.json`
- `ncl__lib_0.2.3.json`       → `niao_libs/ncl/0.2.3/lib.json`
- …and so on.

PowerShell, from repo root (dry-run first — **inspect the diff**):

```powershell
Get-ChildItem niao-libs-plan\review-2026-07\manifest-fixes\*.json | ForEach-Object {
    $n = $_.BaseName            # e.g. core__package  or  ncl__lib_0.2.3
    $lib, $which = $n -split '__', 2
    if ($which -eq 'package') { $dst = "niao_libs\$lib\package.json" }
    else { $ver = $which -replace '^lib_',''; $dst = "niao_libs\$lib\$ver\lib.json" }
    Write-Host "$($_.Name)  ->  $dst"
    # Copy-Item $_.FullName $dst        # <- uncomment to apply
}
```

Then confirm every manifest parses:

```powershell
Get-ChildItem niao_libs -Recurse -Include package.json,lib.json | ForEach-Object {
    try { Get-Content $_ -Raw | ConvertFrom-Json > $null }
    catch { Write-Host "STILL BROKEN: $($_.FullName)" }
}
```

Everything is under git, so `git diff` shows exactly what changed before you commit.
