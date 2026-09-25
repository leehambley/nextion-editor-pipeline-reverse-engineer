//! Decode and patch Nextion `.HMI` project files.
//!
//! Format reference: `docs/formats/nextion-hmi-format.md`. This is a Rust
//! port of the reverse-engineering session's `hmi_tool.py`, kept faithful to
//! its behavior (including its limitations) rather than "improved" on top of
//! an unverified guess. In particular:
//!
//! - [`patch_attr`] only supports same-byte-length overwrites. Anything that
//!   would change a resource's total size (renaming to a longer/shorter
//!   string, adding/removing a component) is out of scope -- see the format
//!   doc's "what this does not yet enable" section for why.
//! - Large binary attribute values (embedded pictures, font blobs) are only
//!   byte-accurately delimited when a proper length prefix is known; this
//!   scanner delimits by "next recognizable 16-byte name pattern", which is
//!   exact for plain string/int attributes and best-effort for blobs (see
//!   the format doc's scanning caveat).

use std::fmt;
use std::path::Path;

use crate::error::HmiError;

/// Superblock/erase-block size used by the `.HMI` container (§1 of the format doc).
pub const BLOCK_SIZE: usize = 0x80000;

const NAME_LEN: usize = 16;
const TAIL_MIN_LEN: usize = 4; // type byte + 3 pad bytes
const PAD_BYTES: [u8; 3] = [0x00, 0x00, 0x00];

fn is_allowed_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

/// A decoded attribute value, tagged the same way `hmi_tool.py`'s
/// `decode_tail` tags it (used both for display and to decide how `patch`
/// may rewrite it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttrValue {
    /// type byte `0x11`: ASCII string.
    Str(String),
    /// type byte `0x12` (except `txt`): little-endian unsigned integer.
    Int(u64),
    /// type byte `0x11` or `0x12`/`txt` whose bytes aren't valid ASCII --
    /// most likely a large binary blob (picture/font data), see the format
    /// doc's scanning caveat.
    Blob(Vec<u8>),
    /// Tail shorter than the minimum type+pad size; only occurs at the very
    /// end of the payload.
    Raw(Vec<u8>),
    /// Tail's trailing 3 bytes weren't `00 00 00`.
    UnknownPad(Vec<u8>),
    /// A type byte other than `0x11`/`0x12`.
    Other(u8, Vec<u8>),
}

impl AttrValue {
    /// Short tag matching `hmi_tool.py`'s `kind` strings, used in patch
    /// error messages.
    pub fn kind(&self) -> String {
        match self {
            AttrValue::Str(_) => "str".to_string(),
            AttrValue::Int(_) => "int".to_string(),
            AttrValue::Blob(_) => "blob".to_string(),
            AttrValue::Raw(_) => "raw".to_string(),
            AttrValue::UnknownPad(_) => "unknown-pad".to_string(),
            AttrValue::Other(b, _) => format!("t0x{b:02x}"),
        }
    }
}

impl fmt::Display for AttrValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AttrValue::Str(s) => write!(f, "{s}"),
            AttrValue::Int(i) => write!(f, "{i}"),
            AttrValue::Blob(b) | AttrValue::Raw(b) | AttrValue::UnknownPad(b) => {
                write!(f, "{}", hex_encode(b))
            }
            AttrValue::Other(_, b) => write!(f, "{}", hex_encode(b)),
        }
    }
}

pub fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// One decoded attribute record: `name(16) + value(N) + type(1) + pad(3)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttrRecord {
    /// Offset of the *name* field's first byte, relative to the start of the file.
    pub file_offset: usize,
    pub name: String,
    pub value: AttrValue,
    /// Length in bytes of just the value portion (excludes type+pad), i.e.
    /// the fixed width `patch_attr` must preserve for integer attributes.
    pub value_len: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Component {
    /// Offset of this component's leading `type` record.
    pub file_offset: usize,
    pub objname: Option<String>,
    pub attrs: Vec<AttrRecord>,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub objname: Option<String>,
    /// Attributes of the page's own container ('y'-type) record, if one was
    /// found leading the page. Empty for an implicit page (malformed/absent
    /// lead-in -- see [`decode`]).
    pub attrs: Vec<AttrRecord>,
    pub components: Vec<Component>,
}

#[derive(Debug, Clone)]
pub struct Decoded {
    pub payload_start: usize,
    pub pages: Vec<Page>,
}

/// Locate the first block, past the mirrored directory blocks, that's mostly
/// non-zero (§1 of the format doc: blocks 0/1 are the mirrored directory,
/// blocks 2..N are reserved/erased flash, and the resource payload starts at
/// whichever block first breaks that pattern).
pub fn find_payload_start(data: &[u8]) -> Result<usize, HmiError> {
    let nblocks = data.len().div_ceil(BLOCK_SIZE);
    let dir_block = &data[0..BLOCK_SIZE.min(data.len())];
    for i in 2..nblocks {
        let start = i * BLOCK_SIZE;
        let end = (start + BLOCK_SIZE).min(data.len());
        let blk = &data[start..end];
        if blk == dir_block {
            continue;
        }
        let nonzero = blk.iter().filter(|&&b| b != 0).count();
        if nonzero > 1000 {
            return Ok(start);
        }
    }
    Err(HmiError::PayloadNotFound)
}

fn is_name_at(payload: &[u8], off: usize) -> Option<String> {
    if off + NAME_LEN > payload.len() {
        return None;
    }
    let window = &payload[off..off + NAME_LEN];
    let mut k = 0;
    while k < NAME_LEN && is_allowed_byte(window[k]) {
        k += 1;
    }
    if k == 0 {
        return None;
    }
    if window[k..].iter().any(|&b| b != 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&window[..k]).into_owned())
}

fn scan_records(payload: &[u8]) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    let mut off = 0usize;
    let n = payload.len();
    while off + NAME_LEN < n {
        if let Some(name) = is_name_at(payload, off) {
            out.push((off, name));
            off += NAME_LEN;
        } else {
            off += 1;
        }
    }
    out
}

fn decode_tail(tail: &[u8], attr_name: &str) -> (AttrValue, usize) {
    if tail.len() < TAIL_MIN_LEN {
        return (AttrValue::Raw(tail.to_vec()), tail.len());
    }
    let type_byte = tail[tail.len() - 4];
    let pad = &tail[tail.len() - 3..];
    let val = &tail[..tail.len() - 4];
    if pad != PAD_BYTES {
        return (AttrValue::UnknownPad(tail.to_vec()), tail.len());
    }
    match type_byte {
        0x11 => match std::str::from_utf8(val) {
            Ok(s) if s.is_ascii() => (AttrValue::Str(s.to_string()), val.len()),
            _ => (AttrValue::Blob(val.to_vec()), val.len()),
        },
        0x12 => {
            if attr_name == "txt" {
                match std::str::from_utf8(val) {
                    Ok(s) if s.is_ascii() => (AttrValue::Str(s.to_string()), val.len()),
                    _ => (AttrValue::Blob(val.to_vec()), val.len()),
                }
            } else {
                let mut buf = [0u8; 8];
                buf[..val.len().min(8)].copy_from_slice(&val[..val.len().min(8)]);
                let n = if val.is_empty() {
                    0
                } else {
                    u64::from_le_bytes(buf)
                };
                (AttrValue::Int(n), val.len())
            }
        }
        other => (AttrValue::Other(other, val.to_vec()), val.len()),
    }
}

/// Decode every page/component/attribute in an already-loaded `.HMI` buffer.
pub fn decode(data: &[u8]) -> Result<Decoded, HmiError> {
    let payload_start = find_payload_start(data)?;
    let payload = &data[payload_start..];
    let names = scan_records(payload);

    struct Rec {
        file_offset: usize,
        name: String,
        value: AttrValue,
        value_len: usize,
    }

    let mut records = Vec::with_capacity(names.len());
    for (i, (p, nm)) in names.iter().enumerate() {
        let next_p = names.get(i + 1).map(|(p, _)| *p).unwrap_or(payload.len());
        let tail = &payload[*p + NAME_LEN..next_p];
        let (value, value_len) = decode_tail(tail, nm);
        records.push(Rec {
            file_offset: payload_start + p,
            name: nm.clone(),
            value,
            value_len,
        });
    }

    let mut pages: Vec<Page> = Vec::new();
    let mut cur_page_idx: Option<usize> = None;
    // Track whether the current component is the active page's own
    // container record (mirrors Python's `cur_comp is cur_page['_container']`).
    let mut cur_is_container = false;

    for r in records {
        if r.name == "type" {
            let is_page_container = matches!(&r.value, AttrValue::Str(s) if s == "y");
            if is_page_container {
                pages.push(Page::default());
                cur_page_idx = Some(pages.len() - 1);
                cur_is_container = true;
            } else {
                if cur_page_idx.is_none() {
                    // Malformed/unknown lead-in: start an implicit page, as
                    // the reference decoder does.
                    pages.push(Page::default());
                    cur_page_idx = Some(pages.len() - 1);
                }
                let page = &mut pages[cur_page_idx.unwrap()];
                page.components.push(Component {
                    file_offset: r.file_offset,
                    ..Default::default()
                });
                cur_is_container = false;
            }
        }

        let Some(page_idx) = cur_page_idx else {
            continue;
        };
        let page = &mut pages[page_idx];

        let attr = AttrRecord {
            file_offset: r.file_offset,
            name: r.name.clone(),
            value: r.value.clone(),
            value_len: r.value_len,
        };

        if cur_is_container {
            page.attrs.push(attr);
            if r.name == "objname" {
                if let AttrValue::Str(s) = &r.value {
                    page.objname = Some(s.clone());
                }
            }
        } else if let Some(comp) = page.components.last_mut() {
            comp.attrs.push(attr);
            if r.name == "objname" {
                if let AttrValue::Str(s) = &r.value {
                    comp.objname = Some(s.clone());
                }
            }
        }
    }

    Ok(Decoded {
        payload_start,
        pages,
    })
}

/// Decode a `.HMI` file from disk.
pub fn decode_file(path: impl AsRef<Path>) -> Result<Decoded, HmiError> {
    let data = std::fs::read(path)?;
    decode(&data)
}

/// Selects one attribute record to patch: `page:objname:attr`, where
/// either `page` or `objname` may be absent (matching `hmi_tool.py`'s
/// `PAGE:OBJNAME:ATTR=VALUE` selector, split into its parts).
#[derive(Debug, Clone)]
pub struct Selector {
    pub page: Option<String>,
    pub objname: Option<String>,
    pub attr: String,
}

impl Selector {
    pub fn parse(spec: &str) -> Result<(Self, String), HmiError> {
        let (selector, new_value) = spec
            .split_once('=')
            .ok_or_else(|| HmiError::BadSetSpec(spec.to_string()))?;
        let parts: Vec<&str> = selector.splitn(3, ':').collect();
        let [page_sel, objname_sel, attr] = parts.as_slice() else {
            return Err(HmiError::BadSetSpec(spec.to_string()));
        };
        let page = if page_sel.is_empty() {
            None
        } else {
            Some(page_sel.to_string())
        };
        let objname = if objname_sel.is_empty() {
            None
        } else {
            Some(objname_sel.to_string())
        };
        Ok((
            Selector {
                page,
                objname,
                attr: attr.to_string(),
            },
            new_value.to_string(),
        ))
    }

    fn as_str(&self) -> String {
        format!(
            "{}:{}:{}",
            self.page.as_deref().unwrap_or(""),
            self.objname.as_deref().unwrap_or(""),
            self.attr
        )
    }
}

fn find_attr_record<'a>(decoded: &'a Decoded, sel: &Selector) -> Option<&'a AttrRecord> {
    for page in &decoded.pages {
        if let Some(page_sel) = &sel.page {
            if page.objname.as_deref() != Some(page_sel.as_str()) {
                continue;
            }
        }

        let page_matches_objname = sel.objname.is_none() || page.objname == sel.objname;
        if page_matches_objname {
            if let Some(rec) = page.attrs.iter().find(|a| a.name == sel.attr) {
                return Some(rec);
            }
        }

        for comp in &page.components {
            if comp.objname == sel.objname {
                if let Some(rec) = comp.attrs.iter().find(|a| a.name == sel.attr) {
                    return Some(rec);
                }
            }
        }
    }
    None
}

fn parse_int_literal(s: &str) -> Result<u64, std::num::ParseIntError> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(hex, 16)
    } else if let Some(oct) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        u64::from_str_radix(oct, 8)
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        u64::from_str_radix(bin, 2)
    } else {
        s.parse::<u64>()
    }
}

/// Rewrite one attribute's value in place. Only same-byte-length overwrites
/// are supported: a string of identical length, or an integer that still
/// fits the attribute's existing fixed width. See the module docs for why.
pub fn patch_attr(
    data: &mut [u8],
    decoded: &Decoded,
    sel: &Selector,
    new_value: &str,
) -> Result<(), HmiError> {
    let rec = find_attr_record(decoded, sel).ok_or_else(|| HmiError::AttrNotFound {
        selector: sel.as_str(),
        page: sel.page.clone(),
        objname: sel.objname.clone(),
        attr: sel.attr.clone(),
    })?;

    let off = rec.file_offset + NAME_LEN;

    match &rec.value {
        AttrValue::Str(old) => {
            let old_len = old.len();
            let new_bytes = new_value.as_bytes();
            if new_bytes.len() != old_len {
                return Err(HmiError::LengthMismatch {
                    selector: sel.as_str(),
                    old_len,
                    old_value: old.clone(),
                    new_len: new_bytes.len(),
                    new_value: new_value.to_string(),
                });
            }
            data[off..off + old_len].copy_from_slice(new_bytes);
        }
        AttrValue::Int(_) => {
            let width = rec.value_len;
            if width == 0 {
                return Err(HmiError::ZeroWidth {
                    selector: sel.as_str(),
                });
            }
            let iv = parse_int_literal(new_value)
                .map_err(|e| HmiError::InvalidInt(new_value.to_string(), e))?;
            if width < 8 && iv >= (1u64 << (8 * width)) {
                return Err(HmiError::ValueTooWide {
                    selector: sel.as_str(),
                    value: iv,
                    width,
                });
            }
            let bytes = iv.to_le_bytes();
            data[off..off + width].copy_from_slice(&bytes[..width]);
        }
        other => {
            return Err(HmiError::UnsupportedPatchKind {
                selector: sel.as_str(),
                kind: other.kind(),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one raw attribute record: `name(16) + value + type + pad(3)`.
    fn attr_record(name: &str, value: &[u8], type_byte: u8) -> Vec<u8> {
        let mut buf = vec![0u8; NAME_LEN];
        buf[..name.len()].copy_from_slice(name.as_bytes());
        buf.extend_from_slice(value);
        buf.push(type_byte);
        buf.extend_from_slice(&PAD_BYTES);
        buf
    }

    fn str_attr(name: &str, s: &str) -> Vec<u8> {
        attr_record(name, s.as_bytes(), 0x11)
    }

    fn int_attr(name: &str, value: u64, width: usize) -> Vec<u8> {
        let bytes = value.to_le_bytes();
        attr_record(name, &bytes[..width], 0x12)
    }

    fn wrap_in_payload(records: Vec<Vec<u8>>) -> Vec<u8> {
        let mut payload = Vec::new();
        for r in records {
            payload.extend_from_slice(&r);
        }
        payload
    }

    fn wrap_in_file(payload: Vec<u8>) -> Vec<u8> {
        let mut data = vec![0u8; BLOCK_SIZE * 2];
        data.extend_from_slice(&payload);
        // `find_payload_start` requires >1000 non-zero bytes in a block to
        // recognize it as the payload (vs. reserved/erased flash); pad with
        // bytes that can't be mistaken for a record name (0xFF isn't in
        // ALLOWED_BYTES, so the scanner just skips over it byte-by-byte).
        data.extend_from_slice(&[0xFFu8; 1200]);
        data
    }

    fn synthetic_page() -> Vec<u8> {
        wrap_in_payload(vec![
            str_attr("type", "y"),
            str_attr("objname", "page1"),
            int_attr("x", 0, 2),
            int_attr("y", 0, 2),
            str_attr("type", "t"),
            str_attr("objname", "textGear"),
            int_attr("id", 2, 1),
            int_attr("x", 72, 2),
            int_attr("y", 22, 2),
            str_attr("txt", "OFF"),
            str_attr("type", "b"),
            str_attr("objname", "bOff"),
            int_attr("id", 3, 1),
        ])
    }

    #[test]
    fn is_name_at_reads_nul_padded_ascii_name() {
        let payload = str_attr("objname", "bOff");
        assert_eq!(is_name_at(&payload, 0), Some("objname".to_string()));
    }

    #[test]
    fn is_name_at_rejects_partial_non_nul_tail() {
        // A 16-byte window whose "name" portion is followed by non-zero
        // bytes before byte 16 is not a name -- it's mid-value data.
        let mut window = vec![0u8; 16];
        window[0] = b'a';
        window[1] = b'b';
        window[5] = 1; // non-zero after the allowed-byte run
        assert_eq!(is_name_at(&window, 0), None);
    }

    #[test]
    fn is_name_at_rejects_out_of_bounds() {
        let payload = vec![b'a'; 10];
        assert_eq!(is_name_at(&payload, 0), None);
    }

    #[test]
    fn decode_tail_parses_string_type() {
        let tail = {
            let mut t = b"OFF".to_vec();
            t.push(0x11);
            t.extend_from_slice(&PAD_BYTES);
            t
        };
        let (val, len) = decode_tail(&tail, "txt");
        assert_eq!(val, AttrValue::Str("OFF".to_string()));
        assert_eq!(len, 3);
    }

    #[test]
    fn decode_tail_parses_int_type() {
        let tail = {
            let mut t = 800u16.to_le_bytes().to_vec();
            t.push(0x12);
            t.extend_from_slice(&PAD_BYTES);
            t
        };
        let (val, len) = decode_tail(&tail, "w");
        assert_eq!(val, AttrValue::Int(800));
        assert_eq!(len, 2);
    }

    #[test]
    fn decode_tail_special_cases_txt_as_string_even_with_int_type_byte() {
        let tail = {
            let mut t = b"THREAD".to_vec();
            t.push(0x12); // txt is documented to use the "int" type byte
            t.extend_from_slice(&PAD_BYTES);
            t
        };
        let (val, _) = decode_tail(&tail, "txt");
        assert_eq!(val, AttrValue::Str("THREAD".to_string()));
    }

    #[test]
    fn decode_tail_flags_bad_padding() {
        let tail = vec![1, 2, 0x11, 0xFF, 0x00, 0x00];
        let (val, _) = decode_tail(&tail, "x");
        assert!(matches!(val, AttrValue::UnknownPad(_)));
    }

    #[test]
    fn decode_tail_flags_short_tail_as_raw() {
        let tail = vec![0x11, 0x00];
        let (val, _) = decode_tail(&tail, "x");
        assert!(matches!(val, AttrValue::Raw(_)));
    }

    #[test]
    fn decode_tail_reports_unknown_type_byte() {
        let tail = vec![1, 2, 0x99, 0x00, 0x00, 0x00];
        let (val, _) = decode_tail(&tail, "pic");
        assert!(matches!(val, AttrValue::Other(0x99, _)));
    }

    #[test]
    fn decode_finds_page_and_components_with_objnames() {
        let payload = synthetic_page();
        let data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        assert_eq!(decoded.pages.len(), 1);
        let page = &decoded.pages[0];
        assert_eq!(page.objname, Some("page1".to_string()));
        assert_eq!(page.components.len(), 2);
        assert_eq!(page.components[0].objname, Some("textGear".to_string()));
        assert_eq!(page.components[1].objname, Some("bOff".to_string()));
    }

    #[test]
    fn decode_handles_missing_page_container_as_implicit_page() {
        // No leading 'type: y' record -- mirrors the real h5.HMI's first
        // page, which has no container record at all.
        let payload = wrap_in_payload(vec![
            str_attr("type", "t"),
            str_attr("objname", "t3"),
            int_attr("id", 2, 1),
        ]);
        let data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        assert_eq!(decoded.pages.len(), 1);
        assert_eq!(decoded.pages[0].objname, None);
        assert_eq!(decoded.pages[0].components.len(), 1);
        assert_eq!(
            decoded.pages[0].components[0].objname,
            Some("t3".to_string())
        );
    }

    #[test]
    fn find_payload_start_skips_mirrored_directory_and_zero_blocks() {
        let payload = synthetic_page();
        let data = wrap_in_file(payload);
        assert_eq!(find_payload_start(&data).unwrap(), BLOCK_SIZE * 2);
    }

    #[test]
    fn find_payload_start_errors_when_all_zero() {
        let data = vec![0u8; BLOCK_SIZE * 4];
        assert!(matches!(
            find_payload_start(&data),
            Err(HmiError::PayloadNotFound)
        ));
    }

    #[test]
    fn patch_string_attribute_same_length_succeeds() {
        let payload = synthetic_page();
        let mut data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        // "OFF" -> "ON!" is the same length (3 bytes).
        let (sel, _) = Selector::parse("page1:textGear:txt=ON!").unwrap();
        patch_attr(&mut data, &decoded, &sel, "ON!").expect("same-length patch should succeed");

        let redecoded = decode(&data).unwrap();
        let comp = &redecoded.pages[0].components[0];
        let txt = comp.attrs.iter().find(|a| a.name == "txt").unwrap();
        assert_eq!(txt.value, AttrValue::Str("ON!".to_string()));
    }

    #[test]
    fn patch_string_attribute_rejects_length_mismatch() {
        let payload = synthetic_page();
        let mut data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        let (sel, _) = Selector::parse("page1:textGear:txt=LONGER").unwrap();
        let err = patch_attr(&mut data, &decoded, &sel, "LONGER").unwrap_err();
        assert!(matches!(err, HmiError::LengthMismatch { .. }));
    }

    #[test]
    fn patch_int_attribute_within_width_succeeds() {
        let payload = synthetic_page();
        let mut data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        let (sel, _) = Selector::parse("page1:textGear:x=100").unwrap();
        patch_attr(&mut data, &decoded, &sel, "100").expect("int patch should succeed");

        let redecoded = decode(&data).unwrap();
        let comp = &redecoded.pages[0].components[0];
        let x = comp.attrs.iter().find(|a| a.name == "x").unwrap();
        assert_eq!(x.value, AttrValue::Int(100));
    }

    #[test]
    fn patch_int_attribute_rejects_value_too_wide() {
        let payload = synthetic_page();
        let mut data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        // `id` is a 1-byte width attribute in this synthetic fixture.
        let (sel, _) = Selector::parse("page1:textGear:id=9999").unwrap();
        let err = patch_attr(&mut data, &decoded, &sel, "9999").unwrap_err();
        assert!(matches!(err, HmiError::ValueTooWide { .. }));
    }

    #[test]
    fn patch_errors_when_attribute_not_found() {
        let payload = synthetic_page();
        let mut data = wrap_in_file(payload);
        let decoded = decode(&data).expect("decode should succeed");

        let (sel, _) = Selector::parse("page1:textGear:nope=1").unwrap();
        let err = patch_attr(&mut data, &decoded, &sel, "1").unwrap_err();
        assert!(matches!(err, HmiError::AttrNotFound { .. }));
    }

    #[test]
    fn selector_parse_rejects_malformed_spec() {
        assert!(Selector::parse("no-equals-sign").is_err());
        assert!(Selector::parse("only:two=parts").is_err());
    }
}
