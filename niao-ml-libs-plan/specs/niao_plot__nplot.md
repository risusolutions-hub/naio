# Library spec: `nplot`  →  crate `niao_plot`

| | |
|---|---|
| Category | Visualization |
| Replaces (Python) | `matplotlib` / `seaborn` |
| Rust reference | `plotters` |
| Target Niao crate | `crates/niao_plot` |
| Niao import name | `nplot` |
| Difficulty | 3/5 — Hard |
| Wave | 1 (needs nnum; nframe optional) |
| Depends on Niao libs | `nnum` (+ `nframe` optional, `ncodec` for PNG) |
| Error block | 4040–4049 |

## Goal
Publication-plain charts for EDA and model diagnostics, rendered to **SVG** (text, zero deps) and optionally
**PNG** (rasterize → encode via `ncodec`). Not an interactive GUI — a "figure → file" library like matplotlib's
Agg backend. **Zero external deps** beyond `nnum`/`ncodec`.

## Scope (v1)
- **Chart types:** line, scatter, bar (grouped/stacked), horizontal bar, histogram, box plot, heatmap,
  **confusion matrix**, ROC curve, error bars, step, area, pie (basic).
- **Figure/axes model:** `Figure` → one or more `Axes` (subplots grid), title, x/y labels, legend, grid, limits,
  log scale, ticks + tick labels, multiple series per axes, color cycle, annotations/text.
- **Styling:** line width/style, marker shapes, color by palette (categorical + sequential), alpha, DPI for PNG.
- **Convenience (seaborn-ish):** `hist(data)`, `scatter(x, y, hue?)`, `heatmap(matrix)`,
  `confusion_matrix(cm, labels)`, `line(x, ys)`, `bar(cats, vals)` — one call → a saved figure.
- **Output:** `save_svg(path)`, `save_png(path, dpi)`, `to_svg_string()`.

## Implementation blueprint
- **SVG first.** Build an in-memory scene (rects, lines, polylines, circles, text, paths) then serialize to SVG
  string. This is exact, dependency-free, and diffable in tests.
- Axis engine: "nice" tick selection (1/2/5 × 10ⁿ), data→pixel transform per axes, autoscale with margins,
  shared handling for linear/log.
- Color palettes: a categorical cycle (tab10-like) + sequential ramps (viridis-like) as const tables.
- **PNG path:** software rasterizer — scanline fill for rects/polygons, Bresenham/Wu lines, simple bitmap font
  (or vector text → filled paths) into an RGBA buffer → hand to `ncodec` PNG encoder. Keep it modest; SVG is primary.
- Text metrics from a bundled fixed-width or single embedded font metric table (no font-shaping engine).

### Performance rules
- Stream SVG into a pre-sized `String`; avoid per-point `format!` — write numbers with a fast float formatter.
- Downsample obviously overplotted scatter/line (> ~50k points) with a note; don't emit 1M `<circle>` nodes.

## Public API surface
`Figure::new(w,h)`, `.subplots(r,c)`, `Axes::{line,scatter,bar,hist,box,heatmap,errorbar,step,pie}`,
`.set_title/xlabel/ylabel/legend/xlim/ylim/xscale`, `save_svg/save_png/to_svg_string`; plus the one-call
convenience fns. Expose to Niao via `niao_libs/nplot/` + builtins; a Niao program builds a figure and saves it.

## Performance target
Render a 10k-point line/scatter to SVG in **< 100 ms**; a 512×512 heatmap PNG in **< 300 ms**.

## Tests required
- **Golden SVG** tests: render fixed inputs, assert the SVG string matches a committed golden (normalize float
  formatting). Covers line, bar, scatter, histogram, heatmap, confusion matrix.
- Axis tick selection: known ranges → expected "nice" ticks.
- Autoscale + log scale transforms produce expected pixel coordinates on fixtures.
- PNG path: render a small known figure, decode it back (via ncodec), assert a few pixel colors.
- Degenerate: empty data → 4041; mismatched x/y lengths → 4042; bad save path → 4044.
- Plus: in-crate unit tests, `examples/nplot_demo.niao` (saves an SVG), `benchmarks/benchmark_nplot.py` (timing).

## Risk / notes
- Text layout is the classic time-sink — use a single metric table + left/center/right anchors; do **not** build a
  font-shaping engine. Vector text as filled paths is fine.
- Keep PNG rasterizer humble; if time-boxed, ship SVG-only in v1 and mark PNG as v2 (still meets EDA needs).
- Golden-file tests must normalize float precision or they'll be flaky across platforms.

## Done criteria
- `cargo check --workspace` and `cargo test -p niao_plot` green; golden SVGs stable.
- `niao_libs/nplot/` wrapper + `examples/nplot_demo.niao` saves a real `.svg` a user can open.
- Timing logged in `REPORT.md`; `CHANGELOG.md` updated; shared-file edits reported, not applied.
