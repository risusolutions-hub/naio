# NPLOT — Niao Plotting

Publication-plain charts for EDA and model diagnostics. Renders to **SVG** (zero external deps). Not an interactive GUI — a figure-to-file library like matplotlib's Agg backend.

## Import

```niao
import "nplot"
```

Flat builtins (`nplot_line`, etc.) are also available after import.

## Quick start

```niao
import "nplot"

fn main() {
    let x = nnum.linspace(0.0, 6.28, 100)
    let y = nnum.to_float_array(x).map(fn(v) { v.sin() })

    let fig = nplot.line(x_data, y)
    nplot.save_svg(fig, "sine.svg")
    print("saved sine.svg")
}
```

## Figure / Axes API

| Builtin | Description |
|---------|-------------|
| `nplot_figure(w, h)` | New figure (pixels) |
| `nplot_subplots(fig, rows, cols)` | Subplot grid |
| `nplot_axes(fig, idx)` | Select axes |
| `nplot_set_title(ax, title)` | Axes title |
| `nplot_set_xlabel(ax, label)` | X axis label |
| `nplot_set_ylabel(ax, label)` | Y axis label |
| `nplot_set_xlim(ax, min, max)` | Fixed X limits |
| `nplot_set_ylim(ax, min, max)` | Fixed Y limits |
| `nplot_set_xscale(ax, log)` | Linear or log X |
| `nplot_set_grid(ax, on)` | Grid on/off |

## Chart types

| Builtin | Description |
|---------|-------------|
| `nplot_line(ax, x, y, label?)` | Line series |
| `nplot_scatter(ax, x, y, label?)` | Scatter plot |
| `nplot_bar(ax, cats, vals, label?)` | Vertical bar chart |
| `nplot_hbar(ax, cats, vals, label?)` | Horizontal bar |
| `nplot_hist(ax, data, bins, label?)` | Histogram |
| `nplot_box(ax, groups, label?)` | Box plot |
| `nplot_heatmap(ax, data, rows, cols)` | 2D heatmap |
| `nplot_confusion_matrix(ax, cm, n, labels)` | Confusion matrix |
| `nplot_roc(ax, fpr, tpr, label?)` | ROC curve |
| `nplot_errorbar(ax, x, y, yerr, label?)` | Error bars |
| `nplot_step(ax, x, y, label?)` | Step plot |
| `nplot_area(ax, x, y, label?)` | Filled area |
| `nplot_pie(ax, labels, vals, title?)` | Pie chart |

## One-call convenience

| Builtin | Description |
|---------|-------------|
| `nplot_quick_line(x, y)` | Line figure in one call |
| `nplot_quick_scatter(x, y)` | Scatter figure |
| `nplot_quick_bar(cats, vals)` | Bar figure |
| `nplot_quick_hist(data, bins)` | Histogram figure |
| `nplot_quick_heatmap(matrix, rows, cols)` | Heatmap figure |
| `nplot_quick_confusion(cm, n, labels)` | Confusion matrix figure |

## Output

```niao
let svg = nplot_to_svg_string(fig)
nplot_save_svg(fig, "chart.svg")
```

PNG export (`nplot_save_png`) is planned for v2 via `ncodec` raster encode. SVG is the primary v1 output.

## Styling

- Categorical colors: tab10-like cycle
- Sequential ramps: viridis-like for heatmaps
- Automatic "nice" ticks (1/2/5 × 10ⁿ)
- Downsamples scatter/line beyond ~50k points

## Errors (4040–4049)

| Code | Meaning |
|------|---------|
| 4040 | Arity mismatch |
| 4041 | Empty data |
| 4042 | Length mismatch (e.g. x/y) |
| 4043 | Invalid figure/axes handle |
| 4044 | Render or save failure |

## Performance

Target: 10k-point line/scatter to SVG in **< 100 ms**. Run `python benchmarks/benchmark_nplot.py`.

## See also

- `nnum` — array data for series
- `nvis` — lighter-weight charts for training curves
- `neval` — metrics for ROC/confusion inputs
