//! Parse and patch compiled Nextion `.tft` firmware files.
//!
//! Format reference: `docs/formats/nextion-tft-format.md`. Everything here
//! is a Rust port of `tft_tool.py`, plus one addition ([`patch_component`])
//! that uses the confirmed 84-byte text-component record layout to patch
//! color/font directly by offset instead of by pattern search. That layout
//! is confirmed **only for text-type (`t`) components** -- button records
//! are known to differ (paired normal/pressed fields) but haven't been
//! mapped, so [`patch_component`] refuses anything other than `t`.
//!
//! `patch_text` and `patch_geom` don't need to know a component's type at
//! all: they work by searching for the current bytes, so they apply to any
//! component that has a text pool slot or an `x,y,w,h` quad, regardless of
//! type.

use crate::error::TftError;
use crate::target::Target;

pub const HEADER_LEN: usize = 0x40;
pub const MAGIC_OFFSET: usize = 0x02;
pub const MAGIC: [u8; 2] = [0x44, 0x4e]; // "DN"
pub const WIDTH_OFFSET: usize = 0x0C;
pub const HEIGHT_OFFSET: usize = 0x0E;
pub const TOTAL_SIZE_OFFSET: usize = 0x3C;

pub const TEXT_SLOT_LEN: usize = 104;

/// `(x, y, w, h)` in pixels.
pub type Geometry = (u16, u16, u16, u16);

/// Byte offsets within the confirmed 84-byte text-component record
/// (`nextion-tft-format.md` §2), relative to the start of the `x,y,w,h`
/// quad minus `0x10`.
pub const TEXT_RECORD_LEN: usize = 0x54;
pub mod text_record_offset {
    pub const UNIQUE_TAG: usize = 0x00;
    pub const ALPHA: usize = 0x0A;
    pub const X: usize = 0x10;
    pub const Y: usize = 0x12;
    pub const W: usize = 0x14;
    pub const H: usize = 0x16;
    pub const ENDX: usize = 0x18;
    pub const ENDY: usize = 0x1A;
    pub const FLAG_0X20: usize = 0x20;
    pub const FONT: usize = 0x25;
    pub const PCO: usize = 0x28;
    pub const TXT_MAXL: usize = 0x2E;
    pub const TEXT_POOL_OFFSET: usize = 0x30;
}

#[derive(Debug, Clone, Copy)]
pub struct Header {
    pub width: u16,
    pub height: u16,
    pub total_size: u32,
}

/// Parse and validate the 64-byte `.tft` header against `data`, and check
/// its declared dimensions against `target`. This is the only place target
/// validation happens for whole-file operations -- see [`crate::target`] for
/// why only one target is supported at all.
pub fn parse_header(data: &[u8], target: Target) -> Result<Header, TftError> {
    if data.len() < HEADER_LEN {
        return Err(TftError::TooSmall(data.len()));
    }
    if data[MAGIC_OFFSET] != MAGIC[0] || data[MAGIC_OFFSET + 1] != MAGIC[1] {
        return Err(TftError::BadMagic(
            data[MAGIC_OFFSET],
            data[MAGIC_OFFSET + 1],
        ));
    }
    let width = u16::from_le_bytes([data[WIDTH_OFFSET], data[WIDTH_OFFSET + 1]]);
    let height = u16::from_le_bytes([data[HEIGHT_OFFSET], data[HEIGHT_OFFSET + 1]]);
    let total_size = u32::from_le_bytes([
        data[TOTAL_SIZE_OFFSET],
        data[TOTAL_SIZE_OFFSET + 1],
        data[TOTAL_SIZE_OFFSET + 2],
        data[TOTAL_SIZE_OFFSET + 3],
    ]);

    if width != target.width() || height != target.height() {
        return Err(TftError::DimensionMismatch {
            width,
            height,
            target: target.to_string(),
            target_width: target.width(),
            target_height: target.height(),
        });
    }
    if total_size as usize != data.len() {
        return Err(TftError::SizeMismatch {
            header_size: total_size,
            actual_size: data.len(),
        });
    }

    Ok(Header {
        width,
        height,
        total_size,
    })
}

fn find_all(haystack: &[u8], needle: &[u8]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    let mut i = 0;
    while i + needle.len() <= haystack.len() {
        if &haystack[i..i + needle.len()] == needle {
            hits.push(i);
        }
        i += 1;
    }
    hits
}

/// Rewrite a text-pool slot in place: search for `old` (ASCII), and if
/// exactly one match is found, overwrite it with `new` NUL-padded to the
/// same byte span `old` occupied.
///
/// `new` must not be longer than `old` -- the slot boundary isn't known
/// here (no page-base bookkeeping consulted, same caveat as `tft_tool.py`),
/// so a longer replacement would overflow into whatever follows `old` in
/// the file. Note: the original `tft_tool.py` this was ported from documents
/// this same rule but its actual length check only rejected replacements
/// longer than *both* the old text and the 104-byte slot -- a latent bug
/// that would have silently grown/shifted the file for a same-slot-fitting
/// but longer-than-`old` replacement. This port enforces the documented
/// rule for real.
pub fn patch_text(data: &mut [u8], old: &str, new: &str) -> Result<usize, TftError> {
    let old_b = old.as_bytes();
    let new_b = new.as_bytes();
    if new_b.len() > old_b.len() {
        return Err(TftError::TextTooLong(old.to_string(), TEXT_SLOT_LEN));
    }

    let hits = find_all(data, old_b);
    if hits.is_empty() {
        return Err(TftError::NotFound(old.to_string()));
    }
    if hits.len() > 1 {
        return Err(TftError::Ambiguous {
            needle: old.to_string(),
            count: hits.len(),
            offsets: hits,
            hint: "use a more specific/longer OLDTEXT".to_string(),
        });
    }

    let off = hits[0];
    data[off..off + new_b.len()].copy_from_slice(new_b);
    for b in &mut data[off + new_b.len()..off + old_b.len()] {
        *b = 0;
    }
    Ok(off)
}

/// Rewrite an `x,y,w,h` geometry quad in place, recomputing `endx`/`endy`
/// (`x+w-1`, `y+h-1`) at the following 8 bytes. If more than one match
/// exists, `at` disambiguates by absolute file offset (must be one of the
/// found offsets); otherwise ambiguity is an error.
pub fn patch_geom(
    data: &mut [u8],
    old: Geometry,
    new: Geometry,
    at: Option<usize>,
) -> Result<usize, TftError> {
    let (x, y, w, h) = old;
    let (nx, ny, nw, nh) = new;

    let mut needle = Vec::with_capacity(8);
    needle.extend_from_slice(&x.to_le_bytes());
    needle.extend_from_slice(&y.to_le_bytes());
    needle.extend_from_slice(&w.to_le_bytes());
    needle.extend_from_slice(&h.to_le_bytes());

    let hits = find_all(data, &needle);
    let old_desc = format!("{x},{y},{w},{h}");
    if hits.is_empty() {
        return Err(TftError::NotFound(old_desc));
    }

    let off = match at {
        Some(off) => {
            if !hits.contains(&off) {
                return Err(TftError::OffsetNotAMatch(
                    off,
                    hits.iter().map(|h| format!("{h:#x}")).collect(),
                ));
            }
            off
        }
        None => {
            if hits.len() > 1 {
                return Err(TftError::Ambiguous {
                    needle: old_desc,
                    count: hits.len(),
                    offsets: hits,
                    hint: "re-run with --at OFFSET to pick one".to_string(),
                });
            }
            hits[0]
        }
    };

    let endx = nx + nw - 1;
    let endy = ny + nh - 1;
    data[off..off + 2].copy_from_slice(&nx.to_le_bytes());
    data[off + 2..off + 4].copy_from_slice(&ny.to_le_bytes());
    data[off + 4..off + 6].copy_from_slice(&nw.to_le_bytes());
    data[off + 6..off + 8].copy_from_slice(&nh.to_le_bytes());
    data[off + 8..off + 10].copy_from_slice(&endx.to_le_bytes());
    data[off + 10..off + 12].copy_from_slice(&endy.to_le_bytes());

    Ok(off)
}

/// Find the (unique) record-start offset for a text-type component by its
/// `x,y,w,h` quad, i.e. `patch_geom`'s search step without the write --
/// used by the `compile` pipeline to locate a component's 84-byte record
/// before touching color/font.
pub fn find_text_record_by_geometry(data: &[u8], geom: Geometry) -> Result<usize, TftError> {
    let (x, y, w, h) = geom;
    let mut needle = Vec::with_capacity(8);
    needle.extend_from_slice(&x.to_le_bytes());
    needle.extend_from_slice(&y.to_le_bytes());
    needle.extend_from_slice(&w.to_le_bytes());
    needle.extend_from_slice(&h.to_le_bytes());

    let hits = find_all(data, &needle);
    let desc = format!("{x},{y},{w},{h}");
    if hits.is_empty() {
        return Err(TftError::NotFound(desc));
    }
    if hits.len() > 1 {
        return Err(TftError::Ambiguous {
            needle: desc,
            count: hits.len(),
            offsets: hits,
            hint: "geometry quad is not unique in this file".to_string(),
        });
    }
    // `patch_geom`'s needle offset *is* the x,y,w,h field's offset, which is
    // record_start + text_record_offset::X (0x10) per the format doc.
    Ok(hits[0] - text_record_offset::X)
}

/// Overwrite the confirmed-safe fields of a text-type (`t`) component's
/// 84-byte record, located by [`find_text_record_by_geometry`]. Only `pco`
/// (text color) and `font` (font id) are supported -- these are the two
/// fields the format doc confirms byte-for-byte against `.HMI`; nothing
/// about background color/picture for text components has been confirmed.
pub fn patch_component_color_font(
    data: &mut [u8],
    record_start: usize,
    pco: Option<u16>,
    font: Option<u8>,
) -> Result<(), TftError> {
    if record_start + TEXT_RECORD_LEN > data.len() {
        return Err(TftError::NotFound(format!(
            "record at {record_start:#x} would extend past end of file"
        )));
    }
    if let Some(pco) = pco {
        let off = record_start + text_record_offset::PCO;
        data[off..off + 2].copy_from_slice(&pco.to_le_bytes());
    }
    if let Some(font) = font {
        data[record_start + text_record_offset::FONT] = font;
    }
    Ok(())
}

pub fn parse_text_set_spec(spec: &str) -> Result<(String, String), TftError> {
    spec.split_once('=')
        .map(|(a, b)| (a.to_string(), b.to_string()))
        .ok_or_else(|| TftError::BadTextSetSpec(spec.to_string()))
}

pub fn parse_geom_set_spec(spec: &str) -> Result<(Geometry, Geometry), TftError> {
    let (old_s, new_s) = spec
        .split_once('=')
        .ok_or_else(|| TftError::BadGeomSetSpec(spec.to_string()))?;
    let parse_quad = |s: &str| -> Option<Geometry> {
        let parts: Vec<&str> = s.split(',').collect();
        if parts.len() != 4 {
            return None;
        }
        let mut vals = [0u16; 4];
        for (i, p) in parts.iter().enumerate() {
            vals[i] = p.trim().parse().ok()?;
        }
        Some((vals[0], vals[1], vals[2], vals[3]))
    };
    let old = parse_quad(old_s).ok_or_else(|| TftError::BadGeomSetSpec(spec.to_string()))?;
    let new = parse_quad(new_s).ok_or_else(|| TftError::BadGeomSetSpec(spec.to_string()))?;
    Ok((old, new))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_header(width: u16, height: u16, total_size: u32) -> Vec<u8> {
        let mut h = vec![0u8; HEADER_LEN];
        h[0] = 0x00;
        h[1] = 0x01;
        h[MAGIC_OFFSET] = MAGIC[0];
        h[MAGIC_OFFSET + 1] = MAGIC[1];
        h[WIDTH_OFFSET..WIDTH_OFFSET + 2].copy_from_slice(&width.to_le_bytes());
        h[HEIGHT_OFFSET..HEIGHT_OFFSET + 2].copy_from_slice(&height.to_le_bytes());
        h[TOTAL_SIZE_OFFSET..TOTAL_SIZE_OFFSET + 4].copy_from_slice(&total_size.to_le_bytes());
        h
    }

    fn text_component_record(x: u16, y: u16, w: u16, h: u16, pco: u16, font: u8) -> Vec<u8> {
        let mut rec = vec![0u8; TEXT_RECORD_LEN];
        rec[text_record_offset::X..text_record_offset::X + 2].copy_from_slice(&x.to_le_bytes());
        rec[text_record_offset::Y..text_record_offset::Y + 2].copy_from_slice(&y.to_le_bytes());
        rec[text_record_offset::W..text_record_offset::W + 2].copy_from_slice(&w.to_le_bytes());
        rec[text_record_offset::H..text_record_offset::H + 2].copy_from_slice(&h.to_le_bytes());
        let endx = x + w - 1;
        let endy = y + h - 1;
        rec[text_record_offset::ENDX..text_record_offset::ENDX + 2]
            .copy_from_slice(&endx.to_le_bytes());
        rec[text_record_offset::ENDY..text_record_offset::ENDY + 2]
            .copy_from_slice(&endy.to_le_bytes());
        rec[text_record_offset::PCO..text_record_offset::PCO + 2]
            .copy_from_slice(&pco.to_le_bytes());
        rec[text_record_offset::FONT] = font;
        rec
    }

    #[test]
    fn parse_header_accepts_matching_target() {
        let mut data = synthetic_header(800, 480, HEADER_LEN as u32);
        data.resize(HEADER_LEN, 0);
        let total = data.len() as u32;
        data[TOTAL_SIZE_OFFSET..TOTAL_SIZE_OFFSET + 4].copy_from_slice(&total.to_le_bytes());

        let header = parse_header(&data, Target::Nx8048p050011rY).unwrap();
        assert_eq!(header.width, 800);
        assert_eq!(header.height, 480);
    }

    #[test]
    fn parse_header_rejects_bad_magic() {
        let mut data = synthetic_header(800, 480, HEADER_LEN as u32);
        data[MAGIC_OFFSET] = 0xFF;
        assert!(matches!(
            parse_header(&data, Target::Nx8048p050011rY),
            Err(TftError::BadMagic(..))
        ));
    }

    #[test]
    fn parse_header_rejects_dimension_mismatch() {
        let data = synthetic_header(320, 240, HEADER_LEN as u32);
        assert!(matches!(
            parse_header(&data, Target::Nx8048p050011rY),
            Err(TftError::DimensionMismatch { .. })
        ));
    }

    #[test]
    fn parse_header_rejects_size_mismatch() {
        // total_size field says a different length than the actual buffer.
        let data = synthetic_header(800, 480, 999);
        assert!(matches!(
            parse_header(&data, Target::Nx8048p050011rY),
            Err(TftError::SizeMismatch { .. })
        ));
    }

    #[test]
    fn parse_header_rejects_too_small_buffer() {
        let data = vec![0u8; 10];
        assert!(matches!(
            parse_header(&data, Target::Nx8048p050011rY),
            Err(TftError::TooSmall(10))
        ));
    }

    #[test]
    fn patch_text_overwrites_unique_match_and_pads_with_nul() {
        let mut data = b"prefix OLDTEXT\0\0\0\0 suffix".to_vec();
        let off = patch_text(&mut data, "OLDTEXT", "NEW").unwrap();
        assert_eq!(&data[off..off + 3], b"NEW");
        assert_eq!(&data[off + 3..off + 7], b"\0\0\0\0");
    }

    #[test]
    fn patch_text_errors_when_not_found() {
        let mut data = b"nothing here".to_vec();
        assert!(matches!(
            patch_text(&mut data, "MISSING", "X"),
            Err(TftError::NotFound(_))
        ));
    }

    #[test]
    fn patch_text_errors_when_ambiguous() {
        let mut data = b"AAAA BBBB AAAA".to_vec();
        assert!(matches!(
            patch_text(&mut data, "AAAA", "ZZZZ"),
            Err(TftError::Ambiguous { count: 2, .. })
        ));
    }

    #[test]
    fn patch_text_rejects_longer_replacement() {
        let mut data = b"short".to_vec();
        assert!(matches!(
            patch_text(
                &mut data,
                "short",
                "much longer than five chars and over slot"
            ),
            Err(TftError::TextTooLong(..))
        ));
    }

    #[test]
    fn patch_geom_rewrites_quad_and_recomputes_endx_endy() {
        let mut data = vec![0u8; 20];
        data[4..12].copy_from_slice(&[72, 0, 22, 0, 180, 0, 60, 0]); // x,y,w,h = 72,22,180,60
        let off = patch_geom(&mut data, (72, 22, 180, 60), (100, 50, 200, 80), None).unwrap();
        assert_eq!(off, 4);
        let nx = u16::from_le_bytes([data[4], data[5]]);
        let ny = u16::from_le_bytes([data[6], data[7]]);
        let nw = u16::from_le_bytes([data[8], data[9]]);
        let nh = u16::from_le_bytes([data[10], data[11]]);
        let endx = u16::from_le_bytes([data[12], data[13]]);
        let endy = u16::from_le_bytes([data[14], data[15]]);
        assert_eq!((nx, ny, nw, nh), (100, 50, 200, 80));
        assert_eq!(endx, 299); // 100 + 200 - 1
        assert_eq!(endy, 129); // 50 + 80 - 1
    }

    #[test]
    fn patch_geom_errors_when_ambiguous_without_at() {
        let mut data = vec![0u8; 40];
        let quad = [10u8, 0, 10, 0, 10, 0, 10, 0];
        data[0..8].copy_from_slice(&quad);
        data[20..28].copy_from_slice(&quad);
        assert!(matches!(
            patch_geom(&mut data, (10, 10, 10, 10), (1, 1, 1, 1), None),
            Err(TftError::Ambiguous { count: 2, .. })
        ));
    }

    #[test]
    fn patch_geom_disambiguates_with_at() {
        let mut data = vec![0u8; 40];
        let quad = [10u8, 0, 10, 0, 10, 0, 10, 0];
        data[0..8].copy_from_slice(&quad);
        data[20..28].copy_from_slice(&quad);
        let off = patch_geom(&mut data, (10, 10, 10, 10), (1, 1, 1, 1), Some(20)).unwrap();
        assert_eq!(off, 20);
        // first occurrence untouched
        assert_eq!(&data[0..8], &quad);
    }

    #[test]
    fn patch_geom_rejects_at_offset_not_in_hits() {
        let mut data = vec![0u8; 20];
        data[4..12].copy_from_slice(&[10, 0, 10, 0, 10, 0, 10, 0]);
        assert!(matches!(
            patch_geom(&mut data, (10, 10, 10, 10), (1, 1, 1, 1), Some(999)),
            Err(TftError::OffsetNotAMatch(..))
        ));
    }

    #[test]
    fn find_text_record_by_geometry_locates_record_start() {
        let rec = text_component_record(72, 22, 180, 60, 0xffff, 2);
        let mut data = vec![0u8; 16];
        data.extend_from_slice(&rec);

        let start = find_text_record_by_geometry(&data, (72, 22, 180, 60)).unwrap();
        assert_eq!(start, 16);
    }

    #[test]
    fn patch_component_color_font_writes_expected_offsets() {
        let rec = text_component_record(72, 22, 180, 60, 0xffff, 2);
        let mut data = rec;
        patch_component_color_font(&mut data, 0, Some(0x049f), Some(5)).unwrap();

        let pco = u16::from_le_bytes([
            data[text_record_offset::PCO],
            data[text_record_offset::PCO + 1],
        ]);
        assert_eq!(pco, 0x049f);
        assert_eq!(data[text_record_offset::FONT], 5);
    }

    #[test]
    fn patch_component_color_font_errors_past_end_of_file() {
        let mut data = vec![0u8; 10];
        assert!(patch_component_color_font(&mut data, 5, Some(1), None).is_err());
    }

    #[test]
    fn parse_text_set_spec_splits_on_first_equals() {
        let (old, new) = parse_text_set_spec("OLD=NEW=WITH=EQUALS").unwrap();
        assert_eq!(old, "OLD");
        assert_eq!(new, "NEW=WITH=EQUALS");
    }

    #[test]
    fn parse_geom_set_spec_parses_both_quads() {
        let (old, new) = parse_geom_set_spec("1,2,3,4=5,6,7,8").unwrap();
        assert_eq!(old, (1, 2, 3, 4));
        assert_eq!(new, (5, 6, 7, 8));
    }

    #[test]
    fn parse_geom_set_spec_rejects_malformed_input() {
        assert!(parse_geom_set_spec("1,2,3=4,5,6,7").is_err());
        assert!(parse_geom_set_spec("no-equals").is_err());
    }
}
