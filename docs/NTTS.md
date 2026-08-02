# ntts standard library

Text-to-speech via **Piper ONNX** (neural voices) and **eSpeak NG** (lightweight formant synthesis). Native Rust implementation — a practical **pyttsx3** subset: synthesize to WAV, select voices, adjust rate/volume, and play audio.

## Import

```niao
import "ntts"
```

Paths `import "std/ntts"` and `import "ntts"` are equivalent.

## Quick start

### eSpeak (no model file)

```niao
import "ntts"

let eng = ntts.init_espeak({voice: "en"})
let audio = ntts.synth(eng, "Hello from Niao.", {rate: 1.0, volume: 1.0})
print(audio.duration, "seconds at", audio.sample_rate, "Hz")

ntts.save(eng, "Saved to disk.", "hello.wav")
ntts.speak(eng, "Playing through speakers.")
ntts.close(eng)
```

### Piper (ONNX model)

Download a voice from [rhasspy/piper-voices](https://huggingface.co/rhasspy/piper-voices) (`.onnx` + `.onnx.json`).

```niao
import "ntts"

let eng = ntts.load("models/en_US-lessac-medium.onnx")
let wav = ntts.synth_wav(eng, "Neural speech sounds great.", {
    length_scale: 1.0,
    noise_scale: 0.667,
})
// wav is byte_array — write with io or ntts.save
ntts.save(eng, "Same text.", "out.wav")
ntts.close(eng)
```

## Engines

| Method | Description |
|--------|-------------|
| `ntts.version()` | Linked Piper + eSpeak version string. |
| `ntts.engines()` | `[{id, name, description}, ...]` — `"piper"` and `"espeak"`. |
| `ntts.list_voices()` | All installed eSpeak voices `{name, language, id}`. |

## Engine lifecycle

| Method | Description |
|--------|-------------|
| `ntts.load(path)` | Load Piper `.onnx` (config `path.onnx.json` auto-resolved) or scan a directory. Returns handle. |
| `ntts.init_espeak(opts?)` | Lightweight eSpeak engine. Opts: `{voice: "en"}`. |
| `ntts.close(handle)` | Release engine; returns `true` on success. |
| `ntts.voices(handle)` | Voices for this engine: Piper speaker map or eSpeak list. |

## Synthesis

Pass an options object: `voice`, `speaker`, `rate`, `volume`, `length_scale`, `noise_scale`, `noise_w`.

| Method | Description |
|--------|-------------|
| `ntts.synth(handle, text, opts?)` | Returns `{samples, sample_rate, duration}` — `samples` is `float_array`. |
| `ntts.synth_wav(handle, text, opts?)` | Returns `byte_array` (16-bit mono WAV). |
| `ntts.save(handle, text, path, opts?)` | Write WAV file; returns `true`. |
| `ntts.speak(handle, text, opts?)` | Synthesize and play on default output device; returns `true`. |

## Properties (~pyttsx3)

| Method | Description |
|--------|-------------|
| `ntts.get(handle, property)` | Read `voice`, `rate`, `volume`, `pitch` (espeak), `length_scale`, `engine`, etc. |
| `ntts.set(handle, property, value)` | Set property; returns `true`. |

Piper `rate` maps to `length_scale` (`<1` faster, `>1` slower). eSpeak `rate` is words-per-minute (default 175).

## Errors

Catchable `ntts_error` values (use `is_error()` / `try`):

| Code | When |
|------|------|
| e4138 | Arity / type mistakes (raises `RuntimeError`) |
| e4139 | Generic domain error |
| e4141 | Bad parameter or unknown property |
| e4142 | Invalid or closed handle |
| e4143 | Model / I/O failure (missing ONNX, bad path) |
| e4144 | Synthesis / phonemization failure |
| e4145 | Playback / WAV encode failure |

## Environment

Piper phonemization and eSpeak both use **espeak-ng** data. If initialization fails, set:

```
PIPER_ESPEAKNG_DATA_DIRECTORY=/path/to/dir-containing-espeak-ng-data
```

The `espeak-ng-data` folder is bundled when building via `espeak-rs-sys`.

## See also

- [`nspeech`](NSPEECH.md) — speech-to-text (Whisper)
- [`ndsp`](NDSP.md) — signal processing for audio post-processing
- [`ntest`](NTEST.md) — test harness

Run tests: `niao run tests/ntts.niao`
