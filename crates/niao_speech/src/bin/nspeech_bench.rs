//! Release-mode micro-benchmarks for niao_speech hot paths.
use niao_speech::{
    align_naive, detect_voice, engine_version, resample_linear, write_wav_f32, VadOptions,
    WHISPER_SAMPLE_RATE,
};
use std::f64::consts::PI;
use std::path::PathBuf;
use std::time::Instant;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn bench(name: &str, warmup: u32, iters: u32, mut f: impl FnMut()) {
    for _ in 0..warmup {
        f();
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        f();
    }
    let elapsed = t0.elapsed();
    let ns = elapsed.as_nanos() as f64 / iters as f64;
    let ops = if ns > 0.0 { 1_000_000_000.0 / ns } else { 0.0 };
    println!("{name}: {iters} runs, {ns:.0} ns/op ({ops:.1} ops/s)");
}

fn sine(len: usize, hz: f64, amp: f32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            (amp as f64 * (2.0 * PI * hz * i as f64 / WHISPER_SAMPLE_RATE as f64).sin()) as f32
        })
        .collect()
}

fn main() {
    println!("whisper.cpp {}", engine_version());

    let n = 160_000usize;
    let samples = sine(n, 440.0, 0.25);
    bench("resample_linear 16k->16k (copy path)", 3, 500, || {
        let _ = resample_linear(&samples, WHISPER_SAMPLE_RATE, WHISPER_SAMPLE_RATE).unwrap();
    });

    bench("resample_linear 48k->16k", 3, 200, || {
        let up: Vec<f32> = (0..n * 3)
            .map(|i| (0.2 * (2.0 * PI * 440.0 * i as f64 / 48000.0).sin()) as f32)
            .collect();
        let _ = resample_linear(&up, 48000, WHISPER_SAMPLE_RATE).unwrap();
    });

    let opts = VadOptions::default();
    bench("vad energy 10s mono", 3, 300, || {
        let _ = detect_voice(&samples, WHISPER_SAMPLE_RATE, &opts).unwrap();
    });

    let hay = "the quick brown fox jumps over the lazy dog ".repeat(100);
    bench("align_naive substring", 3, 50_000, || {
        let _ = align_naive(&hay, "lazy");
    });

    let wav = fixture("tone_1s.wav");
    if !wav.exists() {
        write_wav_f32(
            &wav,
            &sine(WHISPER_SAMPLE_RATE as usize, 440.0, 0.3),
            WHISPER_SAMPLE_RATE,
        )
        .expect("write tone fixture");
    }

    let model = fixture("ggml-tiny.en.bin");
    if model.exists() {
        use niao_speech::{load_model, LoadOptions, TranscribeOptions};
        let m = load_model(&model, &LoadOptions::default()).expect("load model");
        let audio = niao_speech::load_wav(&wav).expect("load wav").0;
        bench("transcribe tiny.en 1s tone", 0, 5, || {
            let _ = m
                .transcribe(
                    &audio,
                    &TranscribeOptions {
                        language: Some("en".into()),
                        token_timestamps: false,
                        ..TranscribeOptions::default()
                    },
                )
                .unwrap();
        });
    } else {
        println!("skip transcribe bench: fixture missing {}", model.display());
    }
}
