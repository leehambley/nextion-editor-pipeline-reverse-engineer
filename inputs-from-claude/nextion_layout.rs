//! Typed layout constants for the Nextion `.HMI` and `.tft` formats, as
//! reverse-engineered in `nextion_hmi_format.md` and `nextion_tft_format.md`.
//!
//! This is *not* a port of hmi_tool.py/tft_tool.py -- it's the struct-level
//! scaffolding so a Rust implementation starts from checked offsets instead
//! of re-deriving them from the markdown prose. `hmi_tool.py` and
//! `tft_tool.py` are the reference implementation: known-good against the
//! real h5.HMI/h5.tft, so diff a Rust port's output against theirs
//! byte-for-byte before trusting it.
//!
//! No I/O, no parsing logic here on purpose -- just the shapes and the
//! offsets, each cited back to the doc section it came from.

// ============================================================================
// .HMI -- component attribute record (nextion_hmi_format.md section 3)
// ============================================================================
//
// Not a fixed-size struct: name is a fixed 16 bytes, but the value is
// variable-width (0..N bytes) depending on the attribute. Model as a parsed
// enum rather than a #[repr(C)] struct.

/// Every `.HMI` attribute record is `name(16) + value(N) + type(1) + pad(3)`.
pub const HMI_ATTR_NAME_LEN: usize = 16;
pub const HMI_ATTR_PAD_LEN: usize = 3;
pub const HMI_ATTR_PAD_BYTES: [u8; HMI_ATTR_PAD_LEN] = [0x00, 0x00, 0x00];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HmiAttrType {
    /// type byte 0x11 -- value is ASCII bytes, length = value_len.
    Str,
    /// type byte 0x12 -- value is a little-endian unsigned int, EXCEPT the
    /// `txt` attribute, which uses this type byte for packed ASCII bytes
    /// (see nextion_hmi_format.md section 3, decode_tail's special case).
    /// Callers must check the attribute *name*, not just the type byte, to
    /// know which interpretation applies.
    Int,
}

impl HmiAttrType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0x11 => Some(Self::Str),
            0x12 => Some(Self::Int),
            _ => None,
        }
    }
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Str => 0x11,
            Self::Int => 0x12,
        }
    }
}

/// One decoded `.HMI` attribute record.
#[derive(Debug, Clone)]
pub struct HmiAttrRecord {
    pub name: String,       // <= 16 ASCII chars in practice (longest seen: "groupid0", "txt_maxl")
    pub kind: HmiAttrType,
    pub value: Vec<u8>,     // raw value bytes, pre-interpretation
    pub file_offset: usize, // offset of the *name* field's first byte
}

/// A component is a run of HmiAttrRecord starting at a "type" record.
/// The component's own `type` value is a single ASCII letter:
///   't' = text/label, 'b' = button, 'p' = picture, 'y' = page container.
/// See nextion_hmi_format.md section 3.1 for the full attribute vocabulary
/// (objname, x/y/w/h, endx/endy, bco/bco2, pco/pco2, pic/pic2, picc/picc2,
/// font, txt, txt_maxl, borderc/borderw, ...).

// ============================================================================
// .HMI -- container block layout (nextion_hmi_format.md section 1)
// ============================================================================

pub const HMI_BLOCK_SIZE: usize = 0x80000; // 512 KiB
pub const HMI_DIRECTORY_BLOCK_A: usize = 0;
pub const HMI_DIRECTORY_BLOCK_B: usize = 1; // must be byte-identical to block A
                                             // (see format doc section 1 -- A/B
                                             // superblock redundancy, not a checksum)

// ============================================================================
// .tft -- file header (nextion_tft_format.md section 1)
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct TftHeader {
    pub width: u16,      // offset 0x0C, LE
    pub height: u16,     // offset 0x0E, LE
    pub total_size: u32, // offset 0x3C, LE -- verified == actual file length
}

pub const TFT_HEADER_MAGIC: [u8; 2] = [0x44, 0x4e]; // "DN" at offset 0x02
pub const TFT_WIDTH_OFFSET: usize = 0x0C;
pub const TFT_HEIGHT_OFFSET: usize = 0x0E;
pub const TFT_TOTAL_SIZE_OFFSET: usize = 0x3C;

// ============================================================================
// .tft -- text-component record, 84 bytes (nextion_tft_format.md section 2)
// ============================================================================
//
// CONFIRMED for text-type ('t') components only. Button-type ('b') records
// are NOT yet characterized -- expect paired normal/pressed fields
// (bco/bco2, pco/pco2, pic/pic2) that this layout doesn't have room for.
// Diff two sibling buttons the same way section 2 diffed textGear/textTurn
// before trusting a button struct.
//
// Record start = (offset of the x,y,w,h u16 quad) - 0x10.

pub const TFT_TEXT_RECORD_LEN: usize = 0x54; // 84 bytes

#[derive(Debug, Clone, Copy)]
pub struct TftTextRecord {
    /// +0x00, 2 bytes. Unique per component; purpose not yet known.
    /// Not needed to patch an existing component's value/position.
    pub unique_tag: u16,
    /// +0x0A, 2 bytes. Alpha/opacity, matches .HMI's `aph`.
    pub alpha: u16,
    /// +0x10, 2 bytes each, in order: x, y, w, h, endx, endy.
    /// endx/endy are stored explicitly, NOT recomputed at render time --
    /// if you change w/h you must also rewrite endx/endy yourself.
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
    pub endx: u16, // must equal x + w - 1
    pub endy: u16, // must equal y + h - 1
    /// +0x20, 1 byte. Flag, seen as 1 on all samples so far (sta/vscope?).
    pub flag_0x20: u8,
    /// +0x25, 1 byte. Font id, matches .HMI's `font`.
    pub font: u8,
    /// +0x28, 2 bytes. Text color (RGB565), matches .HMI's `pco`.
    pub pco: u16,
    /// +0x2E, 2 bytes. Max text length, matches .HMI's `txt_maxl`.
    pub txt_maxl: u16,
    /// +0x30, 2 bytes. Byte offset into the page's text pool, relative to
    /// a fixed per-page base address (see TftTextPool below). NOT the
    /// text itself.
    pub text_pool_offset: u16,
    /// +0x3D, 1 byte. Increments per component in the page, roughly id+1.
    pub component_index: u8,
}

// Byte offsets within the 84-byte record, named for direct use when
// slicing a &[u8] without deserializing the whole struct (e.g. for a
// surgical in-place patch, mirroring what tft_tool.py's patch-geom does).
pub mod tft_text_record_offset {
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
    pub const TRAILER_MARKER: usize = 0x3C; // constant 0x74
    pub const COMPONENT_INDEX: usize = 0x3D;
    pub const TRAILER_CONST_1: usize = 0x3E; // constant 0x01
    pub const TRAILER_TYPE_TAG: usize = 0x3F; // constant 0x37 (only 't' type sampled)
}

// ============================================================================
// .tft -- text pool (nextion_tft_format.md section 3)
// ============================================================================

pub const TFT_TEXT_SLOT_LEN: usize = 104;

/// text_address = page_base + record.text_pool_offset
///
/// page_base is a fixed per-page constant (verified 0xC002C for page1 in
/// h5.tft, solved by cross-checking two different components' pointers
/// resolve to the same base). How page_base itself is *stored* in the
/// file (vs. hand-derived the way we did it) is not yet located -- fine
/// for patching existing slots (find by current text content instead),
/// not yet enough to compute the pointer for a brand-new slot.
pub struct TftTextPool;

impl TftTextPool {
    pub fn slot_address(page_base: usize, record_pointer: u16) -> usize {
        page_base + record_pointer as usize
    }
}
