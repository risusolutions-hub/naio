use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use flate2::read::GzDecoder;

use crate::error::{Result, TarError};
use crate::format::Compression;

pub fn open_read_file(path: &Path, compression: Compression) -> Result<Box<dyn Read + Send>> {
    let file = File::open(path)?;
    wrap_reader(file, compression)
}

pub fn wrap_reader<R: Read + Send + 'static>(
    reader: R,
    compression: Compression,
) -> Result<Box<dyn Read + Send>> {
    match compression {
        Compression::None => Ok(Box::new(BufReader::new(reader))),
        Compression::Gz => Ok(Box::new(BufReader::new(GzDecoder::new(reader)))),
        Compression::Zst => {
            let decoder = zstd::stream::read::Decoder::new(reader)
                .map_err(|e| TarError::Format(format!("zstd decoder: {e}")))?;
            Ok(Box::new(BufReader::new(decoder)))
        }
    }
}

pub fn read_to_end<R: Read>(mut reader: R, max: usize) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 64 * 1024];
    loop {
        if buf.len() >= max {
            return Err(TarError::Format(format!(
                "tar entry exceeds max read size ({max} bytes)"
            )));
        }
        let n = reader.read(&mut chunk).map_err(TarError::Io)?;
        if n == 0 {
            break;
        }
        if buf.len() + n > max {
            return Err(TarError::Format(format!(
                "tar entry exceeds max read size ({max} bytes)"
            )));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    Ok(buf)
}

pub fn copy_limited<R: Read, W: std::io::Write>(
    mut reader: R,
    mut writer: W,
    limit: u64,
) -> Result<u64> {
    let mut buf = [0u8; 64 * 1024];
    let mut copied = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(TarError::Io)?;
        if n == 0 {
            break;
        }
        copied += n as u64;
        if copied > limit {
            return Err(TarError::Format(format!(
                "tar entry exceeds declared size ({limit} bytes)"
            )));
        }
        writer.write_all(&buf[..n]).map_err(TarError::Io)?;
    }
    Ok(copied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression as GzLevel;
    use std::io::{Cursor, Write};

    #[test]
    fn gzip_roundtrip() {
        let data = b"hello gzip tar";
        let mut enc = Vec::new();
        {
            let mut w = GzEncoder::new(&mut enc, GzLevel::default());
            w.write_all(data).unwrap();
            w.finish().unwrap();
        }
        let mut dec = GzDecoder::new(Cursor::new(enc));
        let mut out = Vec::new();
        dec.read_to_end(&mut out).unwrap();
        assert_eq!(out, data);
    }
}
