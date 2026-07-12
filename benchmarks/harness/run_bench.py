#!/usr/bin/env python3
"""
Per-library micro-benchmark harness for the Niao stdlib review (v0.2.3 -> v0.2.4).

Runs each .niao snippet in ./benches/ several times via the real `niao` binary,
parses the VM-reported time, and writes results/<UTC-date>.json so you can diff
before/after landing an optimization from ROADMAP_v0.2.4.md / PERFORMANCE.md.

Usage (on Windows, where niao.exe lives):
    python run_bench.py                 # run every bench in ./benches
    python run_bench.py nvec nrand      # run only matching benches
    python run_bench.py --runs 7        # override run count

This harness is intentionally defensive: if a snippet errors (e.g. an API name
drifted between versions), it records the failure and keeps going instead of
crashing the whole run. Fix the snippet's API calls and re-run.
"""
import argparse, json, os, re, shutil, statistics, subprocess, sys, time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent
BENCH_DIR = ROOT / "benches"
RESULTS_DIR = ROOT / "results"
# "run: 12.3 ms (compile: 1.8 ms)"  OR  "finished in 12.3 ms"
TIME_RE = re.compile(r"(?:run:\s*([\d.]+)\s*ms|finished in\s*([\d.]+)\s*ms)")


def find_niao() -> str:
    up = os.environ.get("USERPROFILE", "")
    for c in (
        shutil.which("niao"),
        Path(up) / ".cargo" / "bin" / "niao.exe",
        Path(up) / ".cargo" / "bin" / "niao",
        ROOT.parents[1] / "target" / "release" / "niao.exe",   # repo/target/release
        ROOT.parents[1] / "target" / "release" / "niao",
    ):
        if c and Path(c).is_file():
            return str(c)
    raise FileNotFoundError(
        "niao binary not found. Install it or add ~/.cargo/bin to PATH."
    )


def parse_ms(stderr: str, stdout: str):
    for stream in (stderr, stdout):
        m = TIME_RE.search(stream or "")
        if m:
            return float(m.group(1) or m.group(2))
    return None


def run_one(niao: str, bench: Path, runs: int):
    # warm compile/cache once (ignored), then timed runs
    subprocess.run([niao, str(bench)], capture_output=True, text=True)
    times, err = [], None
    for _ in range(runs):
        t0 = time.perf_counter()
        proc = subprocess.run([niao, str(bench), "time"], capture_output=True, text=True)
        wall = (time.perf_counter() - t0) * 1000.0
        if proc.returncode != 0:
            err = (proc.stderr or proc.stdout or "non-zero exit").strip()[:300]
            break
        ms = parse_ms(proc.stderr, proc.stdout)
        times.append(ms if ms is not None else wall)  # fall back to wall clock
    if err:
        return {"status": "error", "detail": err}
    return {
        "status": "ok",
        "runs": len(times),
        "best_ms": round(min(times), 3),
        "avg_ms": round(statistics.mean(times), 3),
        "worst_ms": round(max(times), 3),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("filters", nargs="*", help="only run benches whose name contains one of these")
    ap.add_argument("--runs", type=int, default=5)
    args = ap.parse_args()

    niao = find_niao()
    RESULTS_DIR.mkdir(exist_ok=True)
    benches = sorted(BENCH_DIR.glob("*.niao"))
    if args.filters:
        benches = [b for b in benches if any(f in b.stem for f in args.filters)]
    if not benches:
        print("no benches matched", file=sys.stderr); sys.exit(1)

    print(f"niao: {niao}")
    print(f"running {len(benches)} bench(es), {args.runs} runs each\n")
    print(f"{'bench':<22}{'best ms':>10}{'avg ms':>10}   status")
    print("-" * 60)

    results = {}
    for b in benches:
        r = run_one(niao, b, args.runs)
        results[b.stem] = r
        if r["status"] == "ok":
            print(f"{b.stem:<22}{r['best_ms']:>10}{r['avg_ms']:>10}   ok")
        else:
            print(f"{b.stem:<22}{'—':>10}{'—':>10}   ERROR: {r['detail'][:40]}")

    stamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H%M%SZ")
    out = RESULTS_DIR / f"{stamp}.json"
    payload = {
        "timestamp": stamp,
        "niao": niao,
        "runs": args.runs,
        "results": results,
    }
    out.write_text(json.dumps(payload, indent=2))
    print(f"\nwrote {out}")
    print("Diff two result files to measure an optimization's effect.")


if __name__ == "__main__":
    main()
