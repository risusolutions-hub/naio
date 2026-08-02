//! Baseline JPEG (SOF0) decode + simple encode (quality 1–100).
//! YCbCr 4:2:0 / 4:4:4, 8-bit. Huffman + 8×8 IDCT.

use crate::error::{VisionError, VisionResult};
use crate::image::{ColorMode, Image};

pub fn decode(bytes: &[u8]) -> VisionResult<Image> {
    let mut p = Parser::new(bytes)?;
    p.parse()?;
    p.decode_image()
}

pub fn encode(img: &Image, quality: u8) -> VisionResult<Vec<u8>> {
    encode_baseline(img, quality.clamp(1, 100))
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u32,
    bitcnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bitbuf: 0,
            bitcnt: 0,
        }
    }

    fn fill(&mut self) -> VisionResult<()> {
        while self.bitcnt < 16 {
            if self.pos >= self.data.len() {
                return Err(VisionError::Codec("JPEG bitstream exhausted".into()));
            }
            let mut b = self.data[self.pos];
            self.pos += 1;
            if b == 0xFF {
                if self.pos >= self.data.len() {
                    return Err(VisionError::Codec("JPEG stuffed FF truncated".into()));
                }
                let n = self.data[self.pos];
                self.pos += 1;
                if n != 0x00 {
                    return Err(VisionError::Codec(format!(
                        "unexpected JPEG marker in scan 0xFF{n:02X}"
                    )));
                }
                b = 0xFF;
            }
            self.bitbuf = (self.bitbuf << 8) | u32::from(b);
            self.bitcnt += 8;
        }
        Ok(())
    }

    fn peek(&mut self, n: u32) -> VisionResult<u32> {
        self.fill()?;
        Ok((self.bitbuf >> (self.bitcnt - n)) & ((1 << n) - 1))
    }

    fn consume(&mut self, n: u32) {
        self.bitcnt -= n;
        self.bitbuf &= (1u32 << self.bitcnt) - 1;
    }

    fn bits(&mut self, n: u32) -> VisionResult<u32> {
        let v = self.peek(n)?;
        self.consume(n);
        Ok(v)
    }

    fn receive_extend(&mut self, s: u32) -> VisionResult<i32> {
        if s == 0 {
            return Ok(0);
        }
        let v = self.bits(s)? as i32;
        let vt = 1i32 << (s as i32 - 1);
        if v < vt {
            Ok(v + (-1i32 << s as i32) + 1)
        } else {
            Ok(v)
        }
    }
}

#[derive(Clone, Default)]
struct HuffTable {
    // lookup: for codes up to 16 bits — slow tree via mincode/maxcode/valptr
    bits: [u8; 17],
    huffval: Vec<u8>,
    mincode: [i32; 17],
    maxcode: [i32; 17],
    valptr: [i32; 17],
}

impl HuffTable {
    fn build(bits: &[u8; 16], values: &[u8]) -> VisionResult<Self> {
        let mut t = Self::default();
        t.bits[1..].copy_from_slice(bits);
        t.huffval = values.to_vec();
        let mut code = 0i32;
        let mut k = 0i32;
        for i in 1..=16 {
            t.valptr[i] = k;
            let n = i32::from(t.bits[i]);
            if n > 0 {
                t.mincode[i] = code;
                code += n;
                t.maxcode[i] = code - 1;
                k += n;
                code <<= 1;
            } else {
                t.mincode[i] = -1;
                t.maxcode[i] = -1;
            }
        }
        Ok(t)
    }

    fn decode(&self, br: &mut BitReader<'_>) -> VisionResult<u8> {
        let mut code = br.bits(1)? as i32;
        let mut i = 1usize;
        while i <= 16 && code > self.maxcode[i] {
            code = (code << 1) | br.bits(1)? as i32;
            i += 1;
        }
        if i > 16 {
            return Err(VisionError::Codec("bad Huffman code".into()));
        }
        let j = self.valptr[i] + (code - self.mincode[i]);
        Ok(self.huffval[j as usize])
    }
}

struct Component {
    id: u8,
    h: u8,
    v: u8,
    tq: u8,
    td: u8,
    ta: u8,
}

struct Parser<'a> {
    data: &'a [u8],
    pos: usize,
    width: usize,
    height: usize,
    precision: u8,
    comps: Vec<Component>,
    quant: [Option<[u16; 64]>; 4],
    dc_huff: [Option<HuffTable>; 4],
    ac_huff: [Option<HuffTable>; 4],
    scan_data: Vec<u8>,
    restart_interval: u16,
}

impl<'a> Parser<'a> {
    fn new(data: &'a [u8]) -> VisionResult<Self> {
        if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
            return Err(VisionError::Codec("not JPEG".into()));
        }
        Ok(Self {
            data,
            pos: 2,
            width: 0,
            height: 0,
            precision: 8,
            comps: Vec::new(),
            quant: [None, None, None, None],
            dc_huff: [None, None, None, None],
            ac_huff: [None, None, None, None],
            scan_data: Vec::new(),
            restart_interval: 0,
        })
    }

    fn next_marker(&mut self) -> VisionResult<u8> {
        while self.pos < self.data.len() {
            if self.data[self.pos] != 0xFF {
                self.pos += 1;
                continue;
            }
            while self.pos < self.data.len() && self.data[self.pos] == 0xFF {
                self.pos += 1;
            }
            if self.pos >= self.data.len() {
                break;
            }
            let m = self.data[self.pos];
            self.pos += 1;
            if m != 0x00 && m != 0xFF {
                return Ok(m);
            }
        }
        Err(VisionError::Codec("JPEG EOF looking for marker".into()))
    }

    fn read_u16(&mut self) -> VisionResult<u16> {
        if self.pos + 2 > self.data.len() {
            return Err(VisionError::Codec("JPEG truncated".into()));
        }
        let v = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn parse(&mut self) -> VisionResult<()> {
        loop {
            let m = self.next_marker()?;
            match m {
                0xD9 => break, // EOI
                0xC0 => self.parse_sof()?,
                0xC4 => self.parse_dht()?,
                0xDB => self.parse_dqt()?,
                0xDD => {
                    let _len = self.read_u16()?;
                    self.restart_interval = self.read_u16()?;
                }
                0xDA => {
                    self.parse_sos()?;
                    // gather entropy-coded data until next marker
                    let start = self.pos;
                    while self.pos + 1 < self.data.len() {
                        if self.data[self.pos] == 0xFF && self.data[self.pos + 1] != 0x00 {
                            let nxt = self.data[self.pos + 1];
                            if nxt >= 0xD0 && nxt <= 0xD7 {
                                // RST — include and continue
                                self.pos += 2;
                                continue;
                            }
                            break;
                        }
                        self.pos += 1;
                    }
                    self.scan_data = self.data[start..self.pos].to_vec();
                    // strip stuffed 0x00 after 0xFF and RST markers for BitReader
                    // BitReader handles FF00; strip RST from stream for simplicity by
                    // replacing RST markers with nothing — handle in BitReader path.
                    // Actually leave FF D0-D7 in stream: BitReader will error. Remove them:
                    let mut cleaned = Vec::with_capacity(self.scan_data.len());
                    let mut i = 0;
                    while i < self.scan_data.len() {
                        if self.scan_data[i] == 0xFF
                            && i + 1 < self.scan_data.len()
                            && self.scan_data[i + 1] >= 0xD0
                            && self.scan_data[i + 1] <= 0xD7
                        {
                            i += 2;
                            continue;
                        }
                        cleaned.push(self.scan_data[i]);
                        i += 1;
                    }
                    self.scan_data = cleaned;
                }
                0xE0..=0xEF | 0xFE | 0xC1..=0xCF => {
                    // skip APP/COM/progressive
                    let len = self.read_u16()? as usize;
                    if len < 2 || self.pos + len - 2 > self.data.len() {
                        return Err(VisionError::Codec("JPEG segment truncated".into()));
                    }
                    if m >= 0xC1 && m <= 0xCF && m != 0xC4 {
                        return Err(VisionError::Codec(
                            "only baseline SOF0 JPEG supported".into(),
                        ));
                    }
                    self.pos += len - 2;
                }
                0xD8 => {}
                _ => {
                    let len = self.read_u16()? as usize;
                    if len < 2 || self.pos + len - 2 > self.data.len() {
                        return Err(VisionError::Codec("JPEG skip truncated".into()));
                    }
                    self.pos += len - 2;
                }
            }
            if m == 0xDA {
                // after SOS, consume until EOI
                let _ = self.next_marker()?;
                break;
            }
        }
        if self.width == 0 || self.comps.is_empty() {
            return Err(VisionError::Codec("incomplete JPEG".into()));
        }
        Ok(())
    }

    fn parse_sof(&mut self) -> VisionResult<()> {
        let len = self.read_u16()? as usize;
        let start = self.pos;
        self.precision = self.data[self.pos];
        self.pos += 1;
        self.height = self.read_u16()? as usize;
        self.width = self.read_u16()? as usize;
        let n = self.data[self.pos] as usize;
        self.pos += 1;
        self.comps.clear();
        for _ in 0..n {
            let id = self.data[self.pos];
            let hv = self.data[self.pos + 1];
            let tq = self.data[self.pos + 2];
            self.pos += 3;
            self.comps.push(Component {
                id,
                h: hv >> 4,
                v: hv & 0x0F,
                tq,
                td: 0,
                ta: 0,
            });
        }
        if self.pos - start != len - 2 {
            self.pos = start + len - 2;
        }
        if self.precision != 8 {
            return Err(VisionError::Codec("JPEG precision != 8".into()));
        }
        Ok(())
    }

    fn parse_dqt(&mut self) -> VisionResult<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        while self.pos < end {
            let info = self.data[self.pos];
            self.pos += 1;
            let pq = info >> 4;
            let tq = (info & 0x0F) as usize;
            if tq > 3 {
                return Err(VisionError::Codec("bad DQT id".into()));
            }
            let mut q = [0u16; 64];
            for i in 0..64 {
                if pq == 0 {
                    q[i] = u16::from(self.data[self.pos]);
                    self.pos += 1;
                } else {
                    q[i] = self.read_u16()?;
                }
            }
            self.quant[tq] = Some(q);
        }
        Ok(())
    }

    fn parse_dht(&mut self) -> VisionResult<()> {
        let len = self.read_u16()? as usize;
        let end = self.pos + len - 2;
        while self.pos < end {
            let info = self.data[self.pos];
            self.pos += 1;
            let class = info >> 4;
            let id = (info & 0x0F) as usize;
            let mut bits = [0u8; 16];
            bits.copy_from_slice(&self.data[self.pos..self.pos + 16]);
            self.pos += 16;
            let nvals: usize = bits.iter().map(|&b| b as usize).sum();
            let values = self.data[self.pos..self.pos + nvals].to_vec();
            self.pos += nvals;
            let table = HuffTable::build(&bits, &values)?;
            if class == 0 {
                self.dc_huff[id] = Some(table);
            } else {
                self.ac_huff[id] = Some(table);
            }
        }
        Ok(())
    }

    fn parse_sos(&mut self) -> VisionResult<()> {
        let len = self.read_u16()? as usize;
        let start = self.pos;
        let ns = self.data[self.pos] as usize;
        self.pos += 1;
        for _ in 0..ns {
            let id = self.data[self.pos];
            let ta_td = self.data[self.pos + 1];
            self.pos += 2;
            if let Some(c) = self.comps.iter_mut().find(|c| c.id == id) {
                c.td = ta_td >> 4;
                c.ta = ta_td & 0x0F;
            }
        }
        self.pos += 3; // Ss Se AhAl
        if self.pos - start < len - 2 {
            self.pos = start + len - 2;
        }
        Ok(())
    }

    fn decode_image(&self) -> VisionResult<Image> {
        let hmax = self.comps.iter().map(|c| c.h).max().unwrap_or(1) as usize;
        let vmax = self.comps.iter().map(|c| c.v).max().unwrap_or(1) as usize;
        let mcu_w = hmax * 8;
        let mcu_h = vmax * 8;
        let mcus_x = (self.width + mcu_w - 1) / mcu_w;
        let mcus_y = (self.height + mcu_h - 1) / mcu_h;

        // plane buffers at full resolution
        let mut planes: Vec<Vec<f32>> = self
            .comps
            .iter()
            .map(|_| vec![0.0f32; self.width * self.height])
            .collect();

        let mut br = BitReader::new(&self.scan_data);
        let mut dc_pred = vec![0i32; self.comps.len()];

        let zig: [usize; 64] = [
            0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34,
            27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37,
            44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
        ];

        for my in 0..mcus_y {
            for mx in 0..mcus_x {
                for (ci, comp) in self.comps.iter().enumerate() {
                    let q = self.quant[comp.tq as usize]
                        .as_ref()
                        .ok_or_else(|| VisionError::Codec("missing quant table".into()))?;
                    let dc_h = self.dc_huff[comp.td as usize]
                        .as_ref()
                        .ok_or_else(|| VisionError::Codec("missing DC Huffman".into()))?;
                    let ac_h = self.ac_huff[comp.ta as usize]
                        .as_ref()
                        .ok_or_else(|| VisionError::Codec("missing AC Huffman".into()))?;
                    for v in 0..comp.v as usize {
                        for h in 0..comp.h as usize {
                            let mut coef = [0i32; 64];
                            let t = dc_h.decode(&mut br)?;
                            let diff = br.receive_extend(u32::from(t))?;
                            dc_pred[ci] += diff;
                            coef[0] = dc_pred[ci];
                            let mut k = 1usize;
                            while k < 64 {
                                let rs = ac_h.decode(&mut br)?;
                                let s = rs & 0x0F;
                                let r = rs >> 4;
                                if s == 0 {
                                    if r == 15 {
                                        k += 16;
                                        continue;
                                    }
                                    break;
                                }
                                k += r as usize;
                                if k >= 64 {
                                    break;
                                }
                                coef[zig[k]] = br.receive_extend(u32::from(s))?;
                                k += 1;
                            }
                            // `q` is zigzag-ordered (JPEG DQT); `coef` is natural order.
                            let mut block = [0.0f32; 64];
                            for i in 0..64 {
                                let zi = zig[i];
                                block[zi] = (coef[zi] * q[i] as i32) as f32;
                            }
                            idct_block(&mut block);

                            let bx = mx * comp.h as usize + h;
                            let by = my * comp.v as usize + v;
                            let pw = (self.width * comp.h as usize + hmax - 1) / hmax;
                            let ph = (self.height * comp.v as usize + vmax - 1) / vmax;
                            // upsample into full-res plane
                            let x0 = bx * 8;
                            let y0 = by * 8;
                            for yy in 0..8 {
                                for xx in 0..8 {
                                    let sample = block[yy * 8 + xx] + 128.0;
                                    // map component sample to full image coords
                                    let sx = x0 + xx;
                                    let sy = y0 + yy;
                                    if sx >= pw || sy >= ph {
                                        continue;
                                    }
                                    let fx0 = sx * hmax / comp.h as usize;
                                    let fy0 = sy * vmax / comp.v as usize;
                                    let fx1 = ((sx + 1) * hmax / comp.h as usize).min(self.width);
                                    let fy1 = ((sy + 1) * vmax / comp.v as usize).min(self.height);
                                    for fy in fy0..fy1 {
                                        for fx in fx0..fx1 {
                                            planes[ci][fy * self.width + fx] = sample;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if self.comps.len() == 1 {
            let mut data = vec![0u8; self.width * self.height];
            for i in 0..data.len() {
                data[i] = planes[0][i].round().clamp(0.0, 255.0) as u8;
            }
            return Image::new(self.height, self.width, ColorMode::Gray, data);
        }

        // YCbCr → RGB
        let mut data = vec![0u8; self.width * self.height * 3];
        for i in 0..self.width * self.height {
            let y = planes[0][i];
            let cb = if self.comps.len() > 1 {
                planes[1][i] - 128.0
            } else {
                0.0
            };
            let cr = if self.comps.len() > 2 {
                planes[2][i] - 128.0
            } else {
                0.0
            };
            let r = y + 1.402 * cr;
            let g = y - 0.344136 * cb - 0.714136 * cr;
            let b = y + 1.772 * cb;
            data[i * 3] = r.round().clamp(0.0, 255.0) as u8;
            data[i * 3 + 1] = g.round().clamp(0.0, 255.0) as u8;
            data[i * 3 + 2] = b.round().clamp(0.0, 255.0) as u8;
        }
        Image::new(self.height, self.width, ColorMode::Rgb, data)
    }
}

fn idct_block(block: &mut [f32; 64]) {
    // Separable Chen-Wang-ish integer-ish float IDCT
    let mut tmp = [0.0f32; 64];
    for y in 0..8 {
        idct_1d(&mut block[y * 8..y * 8 + 8], &mut tmp[y * 8..y * 8 + 8]);
    }
    for x in 0..8 {
        let mut col = [0.0f32; 8];
        let mut out = [0.0f32; 8];
        for y in 0..8 {
            col[y] = tmp[y * 8 + x];
        }
        idct_1d(&mut col, &mut out);
        for y in 0..8 {
            block[y * 8 + x] = out[y];
        }
    }
}

fn idct_1d(s: &mut [f32], d: &mut [f32]) {
    // Loeffler–Ligtenberg–Moschytz approximate via naive DCT-III for correctness
    const PI: f32 = std::f32::consts::PI;
    for x in 0..8 {
        let mut sum = s[0] * 0.5;
        for u in 1..8 {
            sum += s[u] * ((PI * u as f32 * (2.0 * x as f32 + 1.0) / 16.0).cos());
        }
        d[x] = sum * 0.5; // scale ≈ 1/sqrt(2) adjustments folded; match AAN-ish
    }
    // Fix scale: standard JPEG IDCT output uses 0.25 factor overall for orthonormal
    for v in d.iter_mut() {
        *v *= 0.5;
    }
}

// ---- Encoder (grayscale or RGB → baseline 4:4:4) ----

fn encode_baseline(img: &Image, quality: u8) -> VisionResult<Vec<u8>> {
    let qscale = if quality < 50 {
        5000 / quality as u32
    } else {
        200 - quality as u32 * 2
    };
    let mut qy = STD_LUM_QUANT;
    let mut qc = STD_CHR_QUANT;
    for i in 0..64 {
        qy[i] = ((qy[i] as u32 * qscale + 50) / 100).clamp(1, 255) as u16;
        qc[i] = ((qc[i] as u32 * qscale + 50) / 100).clamp(1, 255) as u16;
    }

    let gray = img.mode == ColorMode::Gray;
    let w = img.width;
    let h = img.height;

    let mut out = Vec::new();
    out.extend_from_slice(&[0xFF, 0xD8]); // SOI
                                          // APP0 JFIF
    out.extend_from_slice(&[
        0xFF, 0xE0, 0x00, 0x10, b'J', b'F', b'I', b'F', 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
    ]);
    write_dqt(&mut out, 0, &qy);
    if !gray {
        write_dqt(&mut out, 1, &qc);
    }
    // SOF0
    let nf: u8 = if gray { 1 } else { 3 };
    let sof_len = 8 + 3 * nf as u16;
    out.extend_from_slice(&[0xFF, 0xC0]);
    out.extend_from_slice(&sof_len.to_be_bytes());
    out.push(8);
    out.extend_from_slice(&(h as u16).to_be_bytes());
    out.extend_from_slice(&(w as u16).to_be_bytes());
    out.push(nf);
    out.extend_from_slice(&[1, 0x11, 0]); // Y
    if !gray {
        out.extend_from_slice(&[2, 0x11, 1, 3, 0x11, 1]);
    }
    write_std_huffman(&mut out);
    // SOS
    let sos_len = 6 + 2 * nf as u16;
    out.extend_from_slice(&[0xFF, 0xDA]);
    out.extend_from_slice(&sos_len.to_be_bytes());
    out.push(nf);
    out.extend_from_slice(&[1, 0x00]);
    if !gray {
        out.extend_from_slice(&[2, 0x11, 3, 0x11]);
    }
    out.extend_from_slice(&[0, 63, 0]);

    let mut bw = BitWriter::new();
    let mut dc_y = 0i32;
    let mut dc_cb = 0i32;
    let mut dc_cr = 0i32;
    let mcus_x = (w + 7) / 8;
    let mcus_y = (h + 7) / 8;
    for my in 0..mcus_y {
        for mx in 0..mcus_x {
            let yb = sample_block(img, mx * 8, my * 8, Channel::Y);
            encode_block(&mut bw, &yb, &qy, &mut dc_y, true)?;
            if !gray {
                let cb = sample_block(img, mx * 8, my * 8, Channel::Cb);
                let cr = sample_block(img, mx * 8, my * 8, Channel::Cr);
                encode_block(&mut bw, &cb, &qc, &mut dc_cb, false)?;
                encode_block(&mut bw, &cr, &qc, &mut dc_cr, false)?;
            }
        }
    }
    bw.flush_byte();
    out.extend_from_slice(&bw.bytes);
    out.extend_from_slice(&[0xFF, 0xD9]);
    Ok(out)
}

enum Channel {
    Y,
    Cb,
    Cr,
}

fn sample_block(img: &Image, x0: usize, y0: usize, ch: Channel) -> [f32; 64] {
    let mut b = [0.0f32; 64];
    for yy in 0..8 {
        for xx in 0..8 {
            let x = (x0 + xx).min(img.width - 1);
            let y = (y0 + yy).min(img.height - 1);
            let (r, g, bl) = match img.mode {
                ColorMode::Gray => {
                    let v = img.data[y * img.width + x] as f32;
                    (v, v, v)
                }
                ColorMode::Rgb | ColorMode::Rgba => {
                    let o = img.pixel_offset(y, x);
                    (
                        img.data[o] as f32,
                        img.data[o + 1] as f32,
                        img.data[o + 2] as f32,
                    )
                }
            };
            let v = match ch {
                Channel::Y => 0.299 * r + 0.587 * g + 0.114 * bl,
                Channel::Cb => -0.168736 * r - 0.331264 * g + 0.5 * bl + 128.0,
                Channel::Cr => 0.5 * r - 0.418688 * g - 0.081312 * bl + 128.0,
            };
            b[yy * 8 + xx] = v - 128.0;
        }
    }
    b
}

fn fdct_block(block: &[f32; 64]) -> [f32; 64] {
    let mut out = [0.0f32; 64];
    const PI: f32 = std::f32::consts::PI;
    for v in 0..8 {
        for u in 0..8 {
            let mut sum = 0.0f32;
            for y in 0..8 {
                for x in 0..8 {
                    sum += block[y * 8 + x]
                        * ((PI * u as f32 * (2.0 * x as f32 + 1.0) / 16.0).cos())
                        * ((PI * v as f32 * (2.0 * y as f32 + 1.0) / 16.0).cos());
                }
            }
            let cu = if u == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            let cv = if v == 0 {
                std::f32::consts::FRAC_1_SQRT_2
            } else {
                1.0
            };
            out[v * 8 + u] = 0.25 * cu * cv * sum;
        }
    }
    out
}

fn encode_block(
    bw: &mut BitWriter,
    spatial: &[f32; 64],
    quant: &[u16; 64],
    dc_pred: &mut i32,
    is_y: bool,
) -> VisionResult<()> {
    let freq = fdct_block(spatial);
    let zig: [usize; 64] = [
        0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
        20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51,
        58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
    ];
    let mut zzg = [0i32; 64];
    for i in 0..64 {
        let v = freq[zig[i]] / quant[i] as f32;
        zzg[i] = if v >= 0.0 {
            (v + 0.5) as i32
        } else {
            (v - 0.5) as i32
        };
    }

    let dc = zzg[0];
    let diff = dc - *dc_pred;
    *dc_pred = dc;
    write_dc(bw, diff, is_y)?;

    let mut run = 0u8;
    for k in 1..64 {
        let ac = zzg[k];
        if ac == 0 {
            run += 1;
            if run == 16 {
                write_ac(bw, 0xF0, 0, is_y)?;
                run = 0;
            }
            continue;
        }
        while run > 15 {
            write_ac(bw, 0xF0, 0, is_y)?;
            run -= 16;
        }
        write_ac(bw, run << 4, ac, is_y)?;
        run = 0;
    }
    if run > 0 {
        write_ac(bw, 0x00, 0, is_y)?; // EOB
    }
    Ok(())
}

fn bit_count(v: i32) -> u8 {
    let a = v.unsigned_abs();
    if a == 0 {
        0
    } else {
        (32 - a.leading_zeros()) as u8
    }
}

fn write_dc(bw: &mut BitWriter, diff: i32, is_y: bool) -> VisionResult<()> {
    let s = bit_count(diff);
    let code = if is_y {
        STD_DC_Y_CODE[s as usize]
    } else {
        STD_DC_C_CODE[s as usize]
    };
    let nbits = if is_y {
        STD_DC_Y_LEN[s as usize]
    } else {
        STD_DC_C_LEN[s as usize]
    };
    bw.write_bits(code as u32, nbits);
    if s > 0 {
        let bits = if diff < 0 { diff - 1 } else { diff };
        bw.write_bits((bits as u32) & ((1 << s) - 1), s);
    }
    Ok(())
}

fn write_ac(bw: &mut BitWriter, rs_hi: u8, ac: i32, is_y: bool) -> VisionResult<()> {
    let s = bit_count(ac);
    let rs = (rs_hi | s) as usize;
    let (codes, lens) = if is_y { ac_y() } else { ac_c() };
    let nbits = lens[rs];
    if nbits == 0 {
        return Err(VisionError::Codec(format!(
            "missing AC Huffman for 0x{rs:02X}"
        )));
    }
    bw.write_bits(u32::from(codes[rs]), nbits);
    if s > 0 {
        let bits = if ac < 0 { ac - 1 } else { ac };
        bw.write_bits((bits as u32) & ((1 << s) - 1), s);
    }
    Ok(())
}

struct BitWriter {
    bytes: Vec<u8>,
    buf: u32,
    n: u32,
}

impl BitWriter {
    fn new() -> Self {
        Self {
            bytes: Vec::new(),
            buf: 0,
            n: 0,
        }
    }
    fn write_bits(&mut self, bits: u32, len: u8) {
        self.buf = (self.buf << len) | (bits & ((1 << len) - 1));
        self.n += u32::from(len);
        while self.n >= 8 {
            self.n -= 8;
            let b = ((self.buf >> self.n) & 0xFF) as u8;
            self.bytes.push(b);
            if b == 0xFF {
                self.bytes.push(0x00);
            }
        }
    }
    fn flush_byte(&mut self) {
        if self.n > 0 {
            let b = ((self.buf << (8 - self.n)) & 0xFF) as u8;
            self.bytes.push(b);
            if b == 0xFF {
                self.bytes.push(0x00);
            }
            self.n = 0;
        }
    }
}

fn write_dqt(out: &mut Vec<u8>, id: u8, q: &[u16; 64]) {
    out.extend_from_slice(&[0xFF, 0xDB, 0x00, 0x43, id]);
    for &v in q {
        out.push(v as u8);
    }
}

fn write_std_huffman(out: &mut Vec<u8>) {
    // DC Y
    out.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x00]);
    out.extend_from_slice(&STD_BITS_DC_Y);
    out.extend_from_slice(&STD_VAL_DC_Y);
    // DC C
    out.extend_from_slice(&[0xFF, 0xC4, 0x00, 0x1F, 0x01]);
    out.extend_from_slice(&STD_BITS_DC_C);
    out.extend_from_slice(&STD_VAL_DC_C);
    // AC Y
    out.extend_from_slice(&[0xFF, 0xC4]);
    let ac_y_len = 2 + 1 + 16 + STD_VAL_AC_Y.len();
    out.extend_from_slice(&(ac_y_len as u16).to_be_bytes());
    out.push(0x10);
    out.extend_from_slice(&STD_BITS_AC_Y);
    out.extend_from_slice(&STD_VAL_AC_Y);
    // AC C
    out.extend_from_slice(&[0xFF, 0xC4]);
    let ac_c_len = 2 + 1 + 16 + STD_VAL_AC_C.len();
    out.extend_from_slice(&(ac_c_len as u16).to_be_bytes());
    out.push(0x11);
    out.extend_from_slice(&STD_BITS_AC_C);
    out.extend_from_slice(&STD_VAL_AC_C);
}

const STD_LUM_QUANT: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const STD_CHR_QUANT: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

const STD_BITS_DC_Y: [u8; 16] = [0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0];
const STD_VAL_DC_Y: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const STD_BITS_DC_C: [u8; 16] = [0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0];
const STD_VAL_DC_C: [u8; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];

// Precomputed encode LUTs for standard Huffman (code, length) indexed by symbol.
// Generated from ITU T.81 Annex K tables.
const STD_DC_Y_LEN: [u8; 12] = [2, 3, 3, 3, 3, 3, 4, 5, 6, 7, 8, 9];
const STD_DC_Y_CODE: [u16; 12] = [0, 2, 3, 4, 5, 6, 14, 30, 62, 126, 254, 510];
const STD_DC_C_LEN: [u8; 12] = [2, 2, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11];
const STD_DC_C_CODE: [u16; 12] = [0, 1, 2, 6, 14, 30, 62, 126, 254, 510, 1022, 2046];

const STD_BITS_AC_Y: [u8; 16] = [0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125];
const STD_VAL_AC_Y: [u8; 162] = [
    0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
    0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xa1, 0x08, 0x23, 0x42, 0xb1, 0xc1, 0x15, 0x52, 0xd1, 0xf0,
    0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0a, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x25, 0x26, 0x27, 0x28,
    0x29, 0x2a, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
    0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
    0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
    0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7,
    0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3, 0xc4, 0xc5,
    0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda, 0xe1, 0xe2,
    0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];
const STD_BITS_AC_C: [u8; 16] = [0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119];
const STD_VAL_AC_C: [u8; 162] = [
    0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21, 0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
    0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91, 0xa1, 0xb1, 0xc1, 0x09, 0x23, 0x33, 0x52, 0xf0,
    0x15, 0x62, 0x72, 0xd1, 0x0a, 0x16, 0x24, 0x34, 0xe1, 0x25, 0xf1, 0x17, 0x18, 0x19, 0x1a, 0x26,
    0x27, 0x28, 0x29, 0x2a, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
    0x49, 0x4a, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59, 0x5a, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
    0x69, 0x6a, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79, 0x7a, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
    0x88, 0x89, 0x8a, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98, 0x99, 0x9a, 0xa2, 0xa3, 0xa4, 0xa5,
    0xa6, 0xa7, 0xa8, 0xa9, 0xaa, 0xb2, 0xb3, 0xb4, 0xb5, 0xb6, 0xb7, 0xb8, 0xb9, 0xba, 0xc2, 0xc3,
    0xc4, 0xc5, 0xc6, 0xc7, 0xc8, 0xc9, 0xca, 0xd2, 0xd3, 0xd4, 0xd5, 0xd6, 0xd7, 0xd8, 0xd9, 0xda,
    0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7, 0xe8, 0xe9, 0xea, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8,
    0xf9, 0xfa,
];

fn ac_code_tables(is_y: bool) -> (Vec<u16>, Vec<u8>) {
    let (bits, vals) = if is_y {
        (&STD_BITS_AC_Y[..], &STD_VAL_AC_Y[..])
    } else {
        (&STD_BITS_AC_C[..], &STD_VAL_AC_C[..])
    };
    let mut codes = vec![0u16; 256];
    let mut lens = vec![0u8; 256];
    let mut code = 0u16;
    let mut k = 0usize;
    for i in 0..16 {
        for _ in 0..bits[i] {
            let sym = vals[k] as usize;
            codes[sym] = code;
            lens[sym] = (i + 1) as u8;
            code += 1;
            k += 1;
        }
        code <<= 1;
    }
    (codes, lens)
}

use std::sync::OnceLock;
fn ac_y() -> &'static (Vec<u16>, Vec<u8>) {
    static T: OnceLock<(Vec<u16>, Vec<u8>)> = OnceLock::new();
    T.get_or_init(|| ac_code_tables(true))
}
fn ac_c() -> &'static (Vec<u16>, Vec<u8>) {
    static T: OnceLock<(Vec<u16>, Vec<u8>)> = OnceLock::new();
    T.get_or_init(|| ac_code_tables(false))
}
