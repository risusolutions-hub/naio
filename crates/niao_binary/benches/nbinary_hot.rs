use niao_binary::{
    crc32, crc64, uvarint_decode, uvarint_encode, BitString, CompiledStruct, PackValue,
};

fn bench_pack(c: &mut criterion::Criterion) {
    let fmt = CompiledStruct::parse("<I").unwrap();
    let val = [PackValue::U32(42)];
    c.bench_function("pack_u32_le_10k", |b| {
        b.iter(|| {
            for _ in 0..10_000 {
                criterion::black_box(fmt.pack(&val).unwrap());
            }
        });
    });
}

fn bench_unpack(c: &mut criterion::Criterion) {
    let fmt = CompiledStruct::parse("<I").unwrap();
    let buf = fmt.pack(&[PackValue::U32(42)]).unwrap();
    c.bench_function("unpack_u32_le_10k", |b| {
        b.iter(|| {
            for _ in 0..10_000 {
                criterion::black_box(fmt.unpack(&buf, 0).unwrap());
            }
        });
    });
}

fn bench_crc32(c: &mut criterion::Criterion) {
    let data = vec![0u8; 1024];
    c.bench_function("crc32_1kib_x1k", |b| {
        b.iter(|| {
            for _ in 0..1_000 {
                criterion::black_box(crc32(&data));
            }
        });
    });
}

fn bench_crc64(c: &mut criterion::Criterion) {
    let data = vec![0u8; 1024];
    c.bench_function("crc64_1kib_x1k", |b| {
        b.iter(|| {
            for _ in 0..1_000 {
                criterion::black_box(crc64(&data));
            }
        });
    });
}

fn bench_uvarint(c: &mut criterion::Criterion) {
    c.bench_function("uvarint_roundtrip_10k", |b| {
        b.iter(|| {
            for i in 0..10_000u64 {
                let enc = uvarint_encode(i);
                criterion::black_box(uvarint_decode(&enc, 0).unwrap());
            }
        });
    });
}

fn bench_bits(c: &mut criterion::Criterion) {
    c.bench_function("bits_write_read_5k", |b| {
        b.iter(|| {
            let mut bs = BitString::new(64);
            for _ in 0..5_000 {
                bs.seek(0);
                bs.write(16, 0xABCD).unwrap();
                bs.seek(0);
                criterion::black_box(bs.read(16).unwrap());
            }
        });
    });
}

criterion::criterion_group!(
    benches,
    bench_pack,
    bench_unpack,
    bench_crc32,
    bench_crc64,
    bench_uvarint,
    bench_bits
);
criterion::criterion_main!(benches);
