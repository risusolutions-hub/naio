use crate::block::{compress, decompress};
use crate::codec::{Codec, CompressOpts, DecompressOpts};
use crate::error::{check_len, CompressError, CompressResult, MAX_BYTES};

/// Incremental compression session — buffers input, emits compressed frame on `finish`.
pub struct CompressStream {
    codec: Codec,
    opts: CompressOpts,
    pending: Vec<u8>,
    finished: bool,
    emitted: Vec<u8>,
}

/// Incremental decompression session — accumulates compressed chunks, decodes on `read`/`finish`.
pub struct DecompressStream {
    codec: Codec,
    opts: DecompressOpts,
    input: Vec<u8>,
    output_pending: Vec<u8>,
    finished: bool,
    max_output: usize,
    total_out: usize,
}

impl CompressStream {
    pub fn new(codec: Codec, opts: CompressOpts) -> CompressResult<Self> {
        codec.validate_level(opts.level)?;
        Ok(Self {
            codec,
            opts,
            pending: Vec::new(),
            finished: false,
            emitted: Vec::new(),
        })
    }

    /// Feed uncompressed bytes. Returns newly emitted compressed bytes (empty until `finish` for most codecs).
    pub fn write(&mut self, chunk: &[u8]) -> CompressResult<Vec<u8>> {
        if self.finished {
            return Err(CompressError::Other(
                "compress stream already finished".into(),
            ));
        }
        check_len(self.pending.len() + chunk.len())?;
        self.pending.extend_from_slice(chunk);
        Ok(Vec::new())
    }

    /// Finalize stream and return compressed bytes.
    pub fn finish(&mut self) -> CompressResult<Vec<u8>> {
        if self.finished {
            return Ok(Vec::new());
        }
        self.finished = true;
        let out = compress(&self.pending, self.codec, &self.opts)?;
        self.emitted = out.clone();
        Ok(out)
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }

    pub fn compressed_so_far(&self) -> &[u8] {
        &self.emitted
    }
}

impl DecompressStream {
    pub fn new(codec: Codec, opts: DecompressOpts) -> CompressResult<Self> {
        let max_output = if opts.max_output == 0 {
            MAX_BYTES
        } else {
            opts.max_output.min(MAX_BYTES)
        };
        Ok(Self {
            codec,
            opts,
            input: Vec::new(),
            output_pending: Vec::new(),
            finished: false,
            max_output,
            total_out: 0,
        })
    }

    /// Feed compressed bytes.
    pub fn write(&mut self, chunk: &[u8]) -> CompressResult<()> {
        if self.finished {
            return Err(CompressError::Other(
                "decompress stream already finished".into(),
            ));
        }
        check_len(self.input.len() + chunk.len())?;
        self.input.extend_from_slice(chunk);
        Ok(())
    }

    /// Read up to `max_bytes` of decompressed output (requires complete frame in buffer).
    pub fn read(&mut self, max_bytes: usize) -> CompressResult<Vec<u8>> {
        if !self.output_pending.is_empty() {
            let n = max_bytes.min(self.output_pending.len());
            return Ok(self.output_pending.drain(..n).collect());
        }
        if self.input.is_empty() {
            return Ok(Vec::new());
        }
        match decompress(&self.input, self.codec, &self.opts) {
            Ok(full) => {
                self.input.clear();
                self.finished = true;
                let n = max_bytes.min(full.len());
                let out = full[..n].to_vec();
                if n < full.len() {
                    self.output_pending.extend_from_slice(&full[n..]);
                }
                self.total_out += out.len();
                if self.total_out > self.max_output {
                    return Err(CompressError::TooLarge(self.total_out));
                }
                Ok(out)
            }
            Err(CompressError::Corrupt(_)) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// Drain all remaining decompressed bytes.
    pub fn finish(&mut self) -> CompressResult<Vec<u8>> {
        let mut rest = std::mem::take(&mut self.output_pending);
        if !self.input.is_empty() {
            let full = decompress(&self.input, self.codec, &self.opts)?;
            self.input.clear();
            rest.extend_from_slice(&full);
        }
        self.finished = true;
        self.total_out += rest.len();
        if self.total_out > self.max_output {
            return Err(CompressError::TooLarge(self.total_out));
        }
        Ok(rest)
    }

    pub fn codec(&self) -> Codec {
        self.codec
    }
}
