# ndsp standard library

Digital signal processing: FIR/IIR filters, windows, convolution, resampling, and spectrograms. Native Rust implementation — a practical **scipy.signal** subset. Designed to pair with `nnum` (FFT / arrays) and a future `naudio` I/O layer.

## Import

```niao
import "ndsp"
```

Paths `import "std/ndsp"` and `import "ndsp"` are equivalent.

## Quick start

```niao
import "ndsp"

// Design a low-pass FIR, filter a tone, inspect spectrum
let h = ndsp.firwin(63, 0.1, {"window": "hamming", "fs": 2.0})
let t = [0.0, 0.001, 0.002, 0.003, 0.004, 0.005]
let x = ndsp.chirp([0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7], 10.0, 1.0, 200.0)
let y = ndsp.lfilter(h, [1.0], x)
let spec = ndsp.spectrogram(y, {"fs": 8000.0, "nperseg": 4})
print(len(spec.Sxx), spec.shape)
```

Signals are plain number arrays or packed `float_array` values. Filter coefficients and spectral results use `float_array` / nested objects (`{b, a}`, `{f, t, Sxx}`, `{re, im}`).

## Convolution

| Method | Description |
|--------|-------------|
| `ndsp.convolve(a, b, mode?)` | Direct convolution. `mode`: `"full"` (default), `"same"`, `"valid"`. |
| `ndsp.correlate(a, b, mode?)` | Cross-correlation. |
| `ndsp.fftconvolve(a, b, mode?)` | FFT convolution (faster for long signals). |

## Windows

| Method | Description |
|--------|-------------|
| `ndsp.get_window(name, Nx, fftbins?)` | Named window (`hann`, `hamming`, `blackman`, `bartlett`, `boxcar`, `kaiser`, `tukey`). |
| `ndsp.hann(M)` / `hamming` / `blackman` / `bartlett` / `boxcar` | Symmetric windows. |
| `ndsp.kaiser(M, beta)` | Kaiser window. |
| `ndsp.tukey(M, alpha?)` | Tukey taper (`alpha` default `0.5`). |

## FIR / IIR design and filtering

| Method | Description |
|--------|-------------|
| `ndsp.firwin(numtaps, cutoff, opts?)` | Windowed-sinc FIR. `cutoff` is a frequency or `[lo, hi]`. Opts: `window`, `pass_zero`, `fs`. |
| `ndsp.butter(order, Wn, opts?)` | Butterworth IIR. Opts: `btype`, `fs`, `output` (`"ba"` / `"sos"`). |
| `ndsp.cheby1(order, rp, Wn, opts?)` | Chebyshev Type I. |
| `ndsp.iirfilter(order, Wn, opts?)` | Generic IIR (`ftype`: `butter` / `cheby1`). |
| `ndsp.lfilter(b, a, x)` | Direct Form II Transposed filter. |
| `ndsp.filtfilt(b, a, x)` | Zero-phase forward-backward. |
| `ndsp.sosfilt(sos, x)` / `sosfiltfilt(sos, x)` | Second-order sections. |
| `ndsp.tf2sos(b, a)` / `sos2tf(sos)` | Transfer-function ↔ SOS. |

`Wn` / cutoffs may be relative to Nyquist (`0..1` with default `fs=2`) or Hz when `fs` is set.

## Resampling

| Method | Description |
|--------|-------------|
| `ndsp.resample(x, num)` | FFT resampling to `num` samples. |
| `ndsp.resample_poly(x, up, down)` | Polyphase upsample/downsample. |
| `ndsp.decimate(x, q, n?)` | Low-pass then downsample by `q`. |
| `ndsp.upfirdn(h, x, up?, down?)` | Upsample → FIR → downsample. |

## Spectral analysis

Pass an options object: `fs`, `window`, `nperseg`, `noverlap`, `nfft`.

| Method | Description |
|--------|-------------|
| `ndsp.stft(x, opts?)` | Short-time Fourier transform → `{f, t, Zxx: {re, im, shape}}`. |
| `ndsp.istft(Zxx, opts?)` | Inverse STFT. |
| `ndsp.spectrogram(x, opts?)` | Power spectrogram → `{f, t, Sxx, shape}`. |
| `ndsp.welch(x, opts?)` | Welch PSD → `{f, Pxx}`. |
| `ndsp.periodogram(x, opts?)` | Periodogram PSD → `{f, Pxx}`. |

## Utilities and waves

| Method | Description |
|--------|-------------|
| `ndsp.detrend(x, type?)` | `"linear"` (default) or `"constant"`. |
| `ndsp.hilbert(x)` | Analytic signal `{re, im}`. |
| `ndsp.medfilt(x, kernel_size?)` | Median filter (odd kernel, default 3). |
| `ndsp.find_peaks(x, opts?)` | Local maxima → `{peaks, heights}` (`height`, `distance`). |
| `ndsp.freqz(b, a?, opts?)` | Frequency response (`worN`, `fs`). |
| `ndsp.sosfreqz(sos, opts?)` | SOS frequency response. |
| `ndsp.chirp(t, f0, t1, f1, method?)` | Linear / quadratic / logarithmic chirp. |
| `ndsp.sawtooth(t, width?)` | Sawtooth wave. |
| `ndsp.square(t, duty?)` | Square wave. |
| `ndsp.gausspulse(t, fc?, bw?)` | Gaussian-modulated sinusoid. |

## Errors

Catchable `ndsp_error` values (use `ntest.is_error` / `try`):

| Code | Meaning |
|------|---------|
| 4100 | Wrong argument count. |
| 4101 | General domain error. |
| 4102 | Type mismatch. |
| 4103 | Invalid parameter. |
| 4104 | Empty / length mismatch. |
| 4105 | Filter coefficient error. |

## Deferred / not in 0.1.0

- Chebyshev II, elliptic, Bessel prototypes; Remez / Parks–McClellan FIR.
- Wavelets / CWT; multi-channel and 2-D filters.
- `group_delay`, `wiener`, peak prominences.
- Direct `naudio` sample I/O (library not present yet).

## Notes

- Prefer `fftconvolve` when `len(a) * len(b)` is large; direct `convolve` wins for tiny kernels.
- For array math and standalone FFTs, use `nnum`; `ndsp` embeds its own FFT for STFT / resampling.
- SOS (`output: "sos"`) is numerically stabler for higher-order IIR — prefer it when cascading sections.
