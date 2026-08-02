//! Micro-benchmarks for nxlsx hot paths.
//! Run: cargo run -p niao_xlsx --bin nxlsx_bench --release

use niao_xlsx::{
    open_file, read_chunk_file, write_bytes, write_file, CellValue, ChunkReadOptions, ReadOptions,
    Table, WorkbookData, WriteOptions,
};
use std::collections::HashMap;
use std::time::Instant;

fn make_table(rows: usize) -> Table {
    let mut cols: HashMap<String, Vec<CellValue>> = HashMap::new();
    cols.insert(
        "id".to_string(),
        (0..rows).map(|i| CellValue::Int(i as i64)).collect(),
    );
    cols.insert(
        "score".to_string(),
        (0..rows)
            .map(|i| CellValue::Float((i as f64) * 0.01))
            .collect(),
    );
    cols.insert(
        "label".to_string(),
        (0..rows)
            .map(|i| CellValue::String(format!("row_{i}")))
            .collect(),
    );
    Table::from_columns(cols).expect("table")
}

fn make_workbook(rows: usize) -> WorkbookData {
    let table = make_table(rows);
    let mut wb = WorkbookData::new();
    niao_xlsx::write_table_to_sheet(&mut wb, "Sheet1", &table, true).expect("write table");
    wb
}

fn bench<F: Fn() -> usize>(name: &str, f: F, warmup: u32, iters: u32) {
    for _ in 0..warmup {
        let _ = f();
    }
    let mut samples = Vec::with_capacity(iters as usize);
    for _ in 0..iters {
        let t0 = Instant::now();
        let n = f();
        samples.push(t0.elapsed().as_nanos() as u64);
        let _ = n;
    }
    samples.sort_unstable();
    let mean: u64 = samples.iter().sum::<u64>() / iters as u64;
    let p50 = samples[samples.len() / 2];
    println!("{name}: mean={mean} ns p50={p50} ns (n={iters})");
}

fn main() {
    let rows = 50_000usize;
    let wb = make_workbook(rows);
    let path = std::env::temp_dir().join("nxlsx_bench_tmp.xlsx");
    write_file(&path, &wb, &WriteOptions::default()).expect("write file");
    let bytes = write_bytes(&wb, &WriteOptions::default()).expect("write bytes");
    println!(
        "payload: rows={rows} file={} bytes mem={} bytes",
        bytes.len(),
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0)
    );

    let warmup = 2u32;
    let iters = 10u32;
    let read_opts = ReadOptions::default();

    bench(
        "write_xlsx 50k rows",
        || {
            write_bytes(&wb, &WriteOptions::default())
                .map(|b| b.len())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "read_xlsx 50k rows full",
        || {
            open_file(&path, &read_opts)
                .map(|w| w.sheets[0].nrows())
                .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "read_chunk 1000 rows",
        || {
            read_chunk_file(
                &path,
                &ChunkReadOptions {
                    start_row: 1,
                    count: 1000,
                    sheet: None,
                },
            )
            .map(|c| c.len())
            .unwrap_or(0)
        },
        warmup,
        iters,
    );

    bench(
        "roundtrip 50k rows",
        || {
            let b = write_bytes(&wb, &WriteOptions::default()).unwrap();
            let tmp = std::env::temp_dir().join("nxlsx_bench_rt.xlsx");
            std::fs::write(&tmp, &b).unwrap();
            let loaded = open_file(&tmp, &read_opts).unwrap();
            let n = loaded.sheets[0].nrows();
            let _ = std::fs::remove_file(tmp);
            n
        },
        warmup,
        iters,
    );

    let _ = std::fs::remove_file(path);
}
