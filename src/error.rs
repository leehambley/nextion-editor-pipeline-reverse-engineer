#[derive(Debug, thiserror::Error)]
pub enum HmiError {
    #[error("could not locate resource payload block: no non-zero block found past the mirrored directory")]
    PayloadNotFound,

    #[error("{selector}: could not find attribute {attr:?} on {objname:?} in page {page:?}")]
    AttrNotFound {
        selector: String,
        page: Option<String>,
        objname: Option<String>,
        attr: String,
    },

    #[error("--set must look like PAGE:OBJNAME:ATTR=VALUE, got: {0:?}")]
    BadSetSpec(String),

    #[error(
        "{selector}: value length mismatch -- old {old_len} bytes ({old_value:?}), new {new_len} bytes ({new_value:?}). \
         Structural (length-changing) edits aren't supported -- same-length values only."
    )]
    LengthMismatch {
        selector: String,
        old_len: usize,
        old_value: String,
        new_len: usize,
        new_value: String,
    },

    #[error("{selector}: attribute has zero width in the source file, can't determine how many bytes to write")]
    ZeroWidth { selector: String },

    #[error("{selector}: value {value} doesn't fit in the existing {width}-byte width")]
    ValueTooWide {
        selector: String,
        value: u64,
        width: usize,
    },

    #[error("{selector}: patching attribute kind {kind:?} isn't supported")]
    UnsupportedPatchKind { selector: String, kind: String },

    #[error("invalid integer value {0:?}: {1}")]
    InvalidInt(String, std::num::ParseIntError),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum TftError {
    #[error("file is too small to contain a .tft header ({0} bytes, need at least 64)")]
    TooSmall(usize),

    #[error("bad magic bytes at offset 0x02: expected 44 4e (\"DN\"), got {0:02x} {1:02x}")]
    BadMagic(u8, u8),

    #[error(
        "header dimensions {width}x{height} do not match target {target} ({target_width}x{target_height})"
    )]
    DimensionMismatch {
        width: u16,
        height: u16,
        target: String,
        target_width: u16,
        target_height: u16,
    },

    #[error(
        "header total-size field says {header_size} bytes but the file is {actual_size} bytes"
    )]
    SizeMismatch {
        header_size: u32,
        actual_size: usize,
    },

    #[error("--set must look like OLDTEXT=NEWTEXT, got: {0:?}")]
    BadTextSetSpec(String),

    #[error("--set must look like X,Y,W,H=NEWX,NEWY,NEWW,NEWH, got: {0:?}")]
    BadGeomSetSpec(String),

    #[error("{0:?}: new text is longer than a {1}-byte slot")]
    TextTooLong(String, usize),

    #[error("{0:?}: not found in the .tft file")]
    NotFound(String),

    #[error("{needle}: found {count} times (offsets {offsets:?}) -- not unique, refusing to guess. {hint}")]
    Ambiguous {
        needle: String,
        count: usize,
        offsets: Vec<usize>,
        hint: String,
    },

    #[error("--at {0:#x} isn't one of the found offsets: {1:?}")]
    OffsetNotAMatch(usize, Vec<String>),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum SpecError {
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Hmi(#[from] HmiError),

    #[error(transparent)]
    Tft(#[from] TftError),

    #[error("spec component {objname:?} not found in scaffold .HMI")]
    ComponentNotInScaffold { objname: String },

    #[error(
        "spec component {objname:?} requests a color/font change, but its scaffold type is {component_type:?} (not 't'); \
         the compiled-record layout for non-text components hasn't been reverse-engineered, so this can't be done safely"
    )]
    ColorFontUnsupportedForType {
        objname: String,
        component_type: String,
    },

    #[error("spec component {objname:?} has no current geometry in the scaffold to search for")]
    MissingScaffoldGeometry { objname: String },
}
