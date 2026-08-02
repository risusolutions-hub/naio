# nspeech standard library

Speech-to-text via **whisper.cpp**: transcribe WAV files or microphone input with segment timestamps and energy-based VAD. Native Rust implementation — a practical **openai-whisper** / **speechrecognition** subset tuned for edge devices.

## Import

```niao
import "nspeech"
```

Paths `import "std/nspeech"` and `import "nspeech"` are equivalent.

## Quick start

```niao
import "nspeech"

// List mics, record 3 seconds, transcribe with a local GGML model
let devs = nspeech.mic_devices()
print(devs[0].name)

let model = nspeech.load("models/ggml-base.en.bin", {gpu: false})
let result = nspeech.mic_transcribe(model, 3.0, {
    language: "en",
    token_timestamps: true,
})
print(result.text)
for seg in result.segments {
    print(seg.start, seg.end, seg.text)
}
nspeech.close(model)
```

File-based transcription:

```niao
import "nspeech"

let h = nspeech.load("models/ggml-small.bin")
let out = nspeech.transcribe_file(h, "interview.wav", {
    language: "en",
    translate: false,
})
print(out.text)
nspeech.close(h)
```

Models are standard [whisper.cpp GGML/GGUF checkpoints](https://github.com/ggerganov/whisper.cpp) (e.g. `ggml-tiny.en.bin`). Audio is converted to **16 kHz mono float** internally.

## Model lifecycle

| Method | Description |
|--------|-------------|
| `nspeech.version()` | Linked whisper.cpp version string. |
| `nspeech.languages()` | Supported ISO language codes. |
| `nspeech.load(path, opts?)` | Load model; returns handle. Opts: `{gpu, dtw}`. |
| `nspeech.close(handle)` | Release model; returns `true` on success. |

## Transcription

Pass an options object: `language`, `translate`, `detect_language`, `token_timestamps`, `offset_ms`, `duration_ms`, `temperature`, `suppress_blank`, `max_len`, `initial_prompt`, `no_speech_threshold`.

| Method | Description |
|--------|-------------|
| `nspeech.transcribe(handle, samples, opts?)` | Transcribe `float_array` / numeric array @ 16 kHz. Returns `{text, language?, duration, segments, tokens?}`. |
| `nspeech.transcribe_file(handle, path, opts?)` | Load WAV + transcribe. |
| `nspeech.detect_language(handle, samples)` | Auto-detect language code. |

Each segment has `{start, end, text, no_speech_prob}` (times in **seconds**). Token entries (when enabled) include `{text, start, end, prob}`.

## Audio utilities

| Method | Description |
|--------|-------------|
| `nspeech.load_audio(path)` | Decode WAV → `{samples, sample_rate, channels}`. |
| `nspeech.resample(samples, from_rate, to_rate?)` | Linear resample; default target 16 kHz. |

## Voice activity (VAD)

Energy-threshold VAD (not Silero) — fast, no extra model. Options: `frame_ms`, `threshold`, `min_speech`, `pad`, `min_silence`.

| Method | Description |
|--------|-------------|
| `nspeech.vad(samples, opts?)` | Segment speech regions on mono f32 @ 16 kHz. Returns `[{start, end}, ...]`. |
| `nspeech.vad_file(path, opts?)` | Load WAV then run VAD. |

## Microphone

Uses the system default input device unless `{device: index}` is set (see `mic_devices()`).

| Method | Description |
|--------|-------------|
| `nspeech.mic_devices()` | `[{index, name, default}, ...]`. |
| `nspeech.mic_record(seconds, opts?)` | Record mono f32 @ 16 kHz. |
| `nspeech.mic_transcribe(handle, seconds, opts?)` | Record + transcribe; merges mic + transcribe options. |

## Errors

Catchable `nspeech_error` values (use `is_error()` / `try`):

| Code | Meaning |
|------|---------|
| 4130 | Wrong argument count. |
| 4131 | General domain error. |
| 4132 | Type mismatch. |
| 4133 | Invalid parameter. |
| 4134 | Invalid or closed model handle. |
| 4135 | Audio / I/O error. |
| 4136 | Model / whisper.cpp error. |
| 4137 | Microphone error. |

## Deferred / not in 0.1.0

- MP3/FLAC/OGG decode (WAV only via `load_audio`; use external tools to convert).
- Silero / whisper.cpp standalone VAD model (energy VAD only in v0.1).
- Streaming partial hypotheses and WebRTC-style endpointing.
- GPU backend selection beyond `{gpu: true/false}`.
- Real-time continuous dictation session object.

## Notes

- Minimum audio length for whisper.cpp is **201 samples** (~12.5 ms @ 16 kHz).
- For signal conditioning (filters, spectrograms), pair with [`ndsp`](NDSP.md).
- On headless CI, skip mic tests; file + VAD tests use checked-in WAV fixtures.
