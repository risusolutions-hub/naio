//! Digital signal processing for Niao (~scipy.signal subset).
//!
//! FIR/IIR filters, windows, convolution, resampling, and spectrograms.
//! Pairs with `nnum` FFT; intended to complement a future `naudio` I/O layer.

mod convolve;
mod error;
mod fft;
mod filter;
mod fir;
mod iir;
mod resample;
mod spectral;
mod util;
mod waves;
mod windows;

pub use convolve::{convolve, correlate, fftconvolve, ConvMode};
pub use error::{DspError, DspResult};
pub use fft::{fft, ifft, rfft, Complex};
pub use filter::{filtfilt, lfilter, sosfilt, sosfiltfilt};
pub use fir::firwin;
pub use iir::{butter, cheby1, iirfilter, sos2tf, tf2sos, Btype, Ftype, IirOut, Sos, Tf};
pub use resample::{decimate, resample, resample_poly, upfirdn};
pub use spectral::{
    istft, periodogram, spectrogram, stft, welch, PsdResult, SpecResult, SpectralOpts, StftResult,
};
pub use util::{detrend, find_peaks, freqz, hilbert, medfilt, sosfreqz, FreqzResult, Peaks};
pub use waves::{chirp, gausspulse, sawtooth, square};
pub use windows::{bartlett, blackman, boxcar, get_window, hamming, hann, kaiser, tukey};
