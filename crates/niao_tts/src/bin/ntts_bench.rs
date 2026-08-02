//! Release-mode micro-benchmarks for niao_tts hot paths.
use niao_tts::{encode_wav, list_voices, EspeakEngine, SynthOptions, TtsEngine};
use std::time::Instant;

fn main() {
    bench_wav_encode();
    bench_espeak_or_skip();
}

fn bench_wav_encode() {
    let samples: Vec<f32> = (0..22_050)
        .map(|i| (i as f32 * 0.001).sin() * 0.3)
        .collect();
    let iters = 500usize;
    let _ = encode_wav(&samples, 22_050).unwrap();
    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = encode_wav(&samples, 22_050).unwrap();
    }
    let ns = t0.elapsed().as_nanos() as f64 / iters as f64;
    let mbps = (samples.len() * 4) as f64 / (ns * 1e-9) / 1e6;
    println!("encode_wav 1s@22050Hz: {iters} runs, {ns:.0} ns/op, {mbps:.1} MB/s f32 input");
}

fn bench_espeak_or_skip() {
    let eng = match init_espeak_engine() {
        Some(e) => e,
        None => {
            println!("espeak synth: skipped (espeak-ng-data not available)");
            return;
        }
    };
    let text = "Hello from ntts benchmark.";
    let _ = eng.synth(text).unwrap();
    let iters = 20usize;
    let t0 = Instant::now();
    for _ in 0..iters {
        let _ = eng.synth(text).unwrap();
    }
    let ns = t0.elapsed().as_nanos() as f64 / iters as f64;
    println!("espeak synth short phrase: {iters} runs, {ns:.0} ns/op");

    let voices = list_voices().unwrap_or_default();
    println!("espeak voices listed: {}", voices.len());
}

fn init_espeak_engine() -> Option<EspeakEngine> {
    let mut e = TtsEngine::init_espeak(Some("en")).ok()?;
    let opts = SynthOptions::default();
    e.synth("warmup", &opts).ok()?;
    Some(EspeakEngine::default())
}
