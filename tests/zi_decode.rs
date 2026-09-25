use nextion_tft_toolkit::zi::{unpack_pixels, pack_pixels, FontFile};
use std::path::Path;

fn fonts() -> Vec<(String, FontFile)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/fonts");
    let mut out = Vec::new();
    for dir in [root.clone(), root.join("single-glyph")] {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "zi") {
                let font = FontFile::parse(&std::fs::read(&path).unwrap()).unwrap();
                out.push((path.display().to_string(), font));
            }
        }
    }
    assert_eq!(out.len(), 7);
    out
}

#[test]
fn every_reference_glyph_decodes_to_exact_size_and_roundtrips() {
    for (path, font) in fonts() {
        for g in &font.glyphs {
            let px = g.unpack(font.line_px).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert_eq!(unpack_pixels(&pack_pixels(&px)).unwrap(), px, "{path} {:#x}", g.char_id);
        }
    }
}

#[test]
fn mono72_header_and_capital_m() {
    let (_, font) = fonts().into_iter().find(|(p, _)| p.ends_with("mono72.zi")).unwrap();
    assert_eq!((font.line_px, font.charset_id, font.label.as_str()), (72, 24, "mono72utf-8"));
    let m = font.find(b'M' as u16).unwrap();
    let px = m.unpack(font.line_px).unwrap();
    // A capital M has ink on both outer strokes across most rows.
    let w = m.cell_w();
    let inked_rows = px.chunks(w).filter(|r| r.iter().filter(|&&p| p >= 4).count() >= 2).count();
    assert!(inked_rows > 30, "only {inked_rows} inked rows");
}
