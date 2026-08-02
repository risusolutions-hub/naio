//! Micro-benchmark: block compress/decompress across codecs.
use niao_compress::{compress, decompress, Codec, CompressOpts, DecompressOpts};
use std::time::Instant;

fn make_payload(n: usize) -> Vec<u8> {
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        v.push(((i * 17 + 31) % 251) as u8);
    }
    v
}

fn bench_codec(data: &[u8], codec: Codec, iters: usize) -> (f64, f64, f64) {
    let opts = CompressOpts::for_codec(codec);
    let dopts = DecompressOpts::default();

    // warmup compress
    let compressed = compress(data, codec, &opts).unwrap();

    let start = Instant::now();
    let mut size = 0usize;
    for _ in 0..iters {
        size += compress(data, codec, &opts).unwrap().len();
    }
    let compress_ns = start.elapsed().as_nanos() as f64 / iters as f64;

    let start = Instant::now();
    let mut out_len = 0usize;
    for _ in 0..iters {
        out_len += decompress(&compressed, codec, &dopts).unwrap().len();
    }
    let decompress_ns = start.elapsed().as_nanos() as f64 / iters as f64;

    let ratio = compressed.len() as f64 / data.len() as f64;
    let _ = (size, out_len);
    (compress_ns, decompress_ns, ratio)
}

fn main() {
    let sizes = [64 * 1024, 1024 * 1024];
    let iters = 50;

    for &size in &sizes {
        let data = make_payload(size);
        println!("\n=== payload {} KiB ===", size / 1024);
        for codec in [Codec::Zstd, Codec::Lz4, Codec::Brotli, Codec::Xz] {
            let (c_ns, d_ns, ratio) = bench_codec(&data, codec, iters);
            println!(
                "{:6}  compress {:.0} ns ({:.2} MB/s)  decompress {:.0} ns ({:.2} MB/s)  ratio {:.3}",
                codec.as_str(),
                c_ns,
                size as f64 / c_ns * 1000.0,
                d_ns,
                size as f64 / d_ns * 1000.0,
                ratio,
            );
        }
    }
}
