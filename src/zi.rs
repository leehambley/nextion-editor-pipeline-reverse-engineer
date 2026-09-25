//! Nextion `.zi` font files (versions 5 and 6): container parsing, glyph
//! raster decoding, and 3bpp glyph encoding.
//!
//! Format derived by studying a decompiled third-party .NET library (see
//! `docs/formats/nextion-zi-font-format.md` §7–§8). Every glyph in every
//! reference file under `examples/fonts/` decodes to exactly
//! `cell_w * line_px` pixels with this implementation.
//!
//! Pixels are 3-bit coverage values: `0` = background, `7` = full
//! foreground, `1..=6` = anti-aliasing levels.

use crate::error::ZiError;

pub const PREAMBLE_BYTES: usize = 44;
const INDEX_ENTRY_BYTES: usize = 10;

/// Packing schemes, selected by the first byte of each glyph's packed data.
pub const SCHEME_BILEVEL: u8 = 1;
pub const SCHEME_GREY3: u8 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontFile {
    /// Header byte 3: 10 = vertical, 11 = horizontal, 12/13 = rotated variants.
    pub layout: u8,
    /// Header byte 4: charset id (3 = iso-8859-1, 24 = utf-8, ...).
    pub charset_id: u8,
    /// Header byte 5: 0 = single-byte full charset, 1 = double-byte full, 2 = subset.
    pub charset_coverage: u8,
    /// Header byte 6: fixed advance width (0 for variable-width fonts).
    pub fixed_advance: u8,
    /// Header byte 7: glyph height in pixels.
    pub line_px: u8,
    /// Header byte 16: 5 or 6.
    pub format_rev: u8,
    /// `<label><encoding>` string, `header[17]` bytes long, not NUL-terminated.
    pub label: String,
    pub glyphs: Vec<Glyph>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Glyph {
    pub char_id: u16,
    pub ink_w: u8,
    pub pad_left: u8,
    pub pad_right: u8,
    /// Raw packed data, including the leading scheme byte.
    pub packed: Vec<u8>,
}

impl Glyph {
    /// Bitmap width: ink width plus left/right padding.
    pub fn cell_w(&self) -> usize {
        self.ink_w as usize + self.pad_left as usize + self.pad_right as usize
    }

    /// Decode to row-major 3-bit coverage values, `cell_w * line_px` long.
    pub fn unpack(&self, line_px: u8) -> Result<Vec<u8>, ZiError> {
        let expected = self.cell_w() * line_px as usize;
        let pixels = unpack_pixels(&self.packed)?;
        if pixels.len() != expected {
            return Err(ZiError::PixelCountMismatch {
                char_id: self.char_id,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(pixels)
    }
}

impl FontFile {
    pub fn parse(bytes: &[u8]) -> Result<Self, ZiError> {
        if bytes.len() < PREAMBLE_BYTES {
            return Err(ZiError::Truncated);
        }
        let h = &bytes[..PREAMBLE_BYTES];
        if h[0] != 4 {
            return Err(ZiError::BadMagic(h[0]));
        }
        let count = u32::from_le_bytes(h[12..16].try_into().unwrap()) as usize;
        let label_len = h[17] as usize;
        // header[33] == 1: glyph offsets are stored divided by 8 (used once
        // the packed exceeds the 24-bit offset field).
        let offset_scale = if h[33] == 1 { 8 } else { 1 };

        let table = PREAMBLE_BYTES + label_len;
        let table_end = table + count * INDEX_ENTRY_BYTES;
        if bytes.len() < table_end {
            return Err(ZiError::Truncated);
        }
        let label = String::from_utf8_lossy(&bytes[PREAMBLE_BYTES..table]).into_owned();

        let glyphs = bytes[table..table_end]
            .chunks_exact(INDEX_ENTRY_BYTES)
            .map(|e| {
                let char_id = u16::from_le_bytes([e[0], e[1]]);
                // Offset is a u24, relative to the start of the glyph table.
                let offset = u32::from_le_bytes([e[5], e[6], e[7], 0]) as usize * offset_scale;
                let len = u16::from_le_bytes([e[8], e[9]]) as usize;
                let start = table + offset;
                let packed = bytes
                    .get(start..start + len)
                    .ok_or(ZiError::GlyphOutOfBounds { char_id })?
                    .to_vec();
                Ok(Glyph {
                    char_id,
                    ink_w: e[2],
                    pad_left: e[3],
                    pad_right: e[4],
                    packed,
                })
            })
            .collect::<Result<_, ZiError>>()?;

        Ok(FontFile {
            layout: h[3],
            charset_id: h[4],
            charset_coverage: h[5],
            fixed_advance: h[6],
            line_px: h[7],
            format_rev: h[16],
            label,
            glyphs,
        })
    }

    pub fn find(&self, char_id: u16) -> Option<&Glyph> {
        self.glyphs.iter().find(|g| g.char_id == char_id)
    }
}

/// Decode one glyph's packed data into 3-bit coverage values.
///
/// Opcode byte layout: `tt f nnnnn` (top two bits select the op).
///
/// | tt | scheme 1 (bilevel)                | scheme 3 (grey3)                     |
/// |----|------------------------------------|--------------------------------------|
/// | 00 | `n` × (f ? 7 : 0)                  | same                                 |
/// | 01 | `n` × 0, then 1 + f × 7            | same                                 |
/// | 10 | `n` × 0, then 3 + f × 7            | `aaa` × 0, then `bbb` (`10 aaa bbb`) |
/// | 11 | `aaa` × 0, then `bbb` × 7          | `aaa`, `bbb` (`11 aaa bbb`)          |
pub fn unpack_pixels(packed: &[u8]) -> Result<Vec<u8>, ZiError> {
    let Some((&scheme, ops)) = packed.split_first() else {
        return Ok(Vec::new());
    };
    if scheme != SCHEME_BILEVEL && scheme != SCHEME_GREY3 {
        return Err(ZiError::UnknownPackingScheme(scheme));
    }
    let mono = scheme == SCHEME_BILEVEL;
    let mut px = Vec::new();
    let run = |px: &mut Vec<u8>, n: usize, v: u8| px.extend(std::iter::repeat(v).take(n));

    for &b in ops {
        let flag = (b >> 5) & 1 == 1;
        let n = (b & 0x1f) as usize;
        let hi = ((b >> 3) & 7) as usize;
        let lo = b & 7;
        match b >> 6 {
            0 => run(&mut px, n, if flag { 7 } else { 0 }),
            1 => {
                run(&mut px, n, 0);
                run(&mut px, 1 + flag as usize, 7);
            }
            2 if mono => {
                run(&mut px, n, 0);
                run(&mut px, 3 + flag as usize, 7);
            }
            2 => {
                run(&mut px, hi, 0);
                px.push(lo);
            }
            _ if mono => {
                run(&mut px, hi, 0);
                run(&mut px, lo as usize, 7);
            }
            _ => {
                px.push(hi as u8);
                px.push(lo);
            }
        }
    }
    Ok(px)
}

/// Encode 3-bit coverage values (row-major) as scheme-3 packed data, using
/// the same opcode set the Editor tooling writes. Not guaranteed byte-identical to the
/// Editor's output, but `unpack_pixels(&pack_pixels(p)) == p`.
pub fn pack_pixels(pixels: &[u8]) -> Vec<u8> {
    let mut out = vec![SCHEME_GREY3];
    let run_len = |i: usize, v: u8, max: usize| {
        pixels[i..].iter().take(max).take_while(|&&p| p == v).count()
    };
    let mut i = 0;
    while i < pixels.len() {
        let p = pixels[i] & 7;
        match p {
            7 => {
                let n = run_len(i, 7, 31);
                out.push(0x20 | n as u8);
                i += n;
            }
            0 => {
                let n = run_len(i, 0, 31);
                let next = pixels.get(i + n).copied();
                match next {
                    // n whites + 1-2 blacks in one byte.
                    Some(7) => {
                        let blacks = run_len(i + n, 7, 2);
                        out.push(0x40 | ((blacks as u8 - 1) << 5) | n as u8);
                        i += n + blacks;
                    }
                    // Up to 7 whites + one grey level in one byte.
                    Some(g) if n < 8 => {
                        out.push(0x80 | (n as u8) << 3 | g);
                        i += n + 1;
                    }
                    _ => {
                        out.push(n as u8);
                        i += n;
                    }
                }
            }
            g => match pixels.get(i + 1) {
                Some(&g2) => {
                    out.push(0xc0 | g << 3 | (g2 & 7));
                    i += 2;
                }
                None => {
                    out.push(0x80 | g);
                    i += 1;
                }
            },
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_levels() {
        let mut px: Vec<u8> = (0..400).map(|i| ((i * 7 + i / 13) % 8) as u8).collect();
        px.extend([0; 70]);
        px.extend([7; 70]);
        px.extend([0, 0, 7, 0, 7, 7, 7, 3]);
        assert_eq!(unpack_pixels(&pack_pixels(&px)).unwrap(), px);
    }
}
