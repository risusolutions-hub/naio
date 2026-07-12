# Task 04 — nplot: matplotlib / seaborn (crate `niao_plot`)
Wave 1 (needs nnum). Read `../MASTER_PLAN.md` + `../specs/niao_plot__nplot.md`. Error block **4040–4049**.
Depends on: `nnum` (+ `ncodec` for PNG; `nframe` optional).

## Build (`crates/niao_plot`, zero new deps)
- SVG-first: build an in-memory scene (rect/line/polyline/circle/text/path) → serialize to SVG string (exact, diffable).
- Figure→Axes (subplot grid): title, x/y labels, legend, grid, limits, log scale, "nice" ticks (1/2/5×10ⁿ),
  data→pixel transform, autoscale, color cycle (tab10-like) + sequential ramp (viridis-like), annotations.
- Chart types: line, scatter, bar(grouped/stacked), hbar, histogram, box, heatmap, confusion_matrix, ROC, errorbar, step, area, pie.
- One-call convenience: hist/scatter(hue?)/heatmap/confusion_matrix/line/bar.
- Output: save_svg, to_svg_string; save_png(dpi) via a modest software rasterizer → ncodec PNG (PNG may be v2 if time-boxed).
- Downsample overplotted (>~50k points) scatter/line; fast float formatter, pre-sized String.

## Wire up
- `niao_libs/nplot/` wrapper + builtins; `docs/NPLOT.md`; `examples/nplot_demo.niao` (saves a real .svg).

## Acceptance
- Golden-SVG tests (normalize float precision) for line/bar/scatter/hist/heatmap/confusion; tick selection + autoscale +
  log transforms vs fixtures; PNG path decodes back to expected pixel colors (if shipped).
- empty data→4041, x/y length mismatch→4042, bad save path→4044.
- 10k-point SVG < 100 ms (`benchmarks/benchmark_nplot.py`). `cargo test -p niao_plot` green.

See `../cursor-rules.md`.
