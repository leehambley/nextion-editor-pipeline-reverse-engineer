//! Integration tests against the real `h5.tft` reference firmware (compiled
//! for the NX8048P050-011R-Y, the same file the original reverse-engineering
//! session's `tft_tool.py` was validated against).

use nextion_tft_toolkit::target::Target;
use nextion_tft_toolkit::tft;

fn load_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/h5.tft").expect("fixture must be present")
}

#[test]
fn parses_header_with_correct_dimensions_and_size() {
    let data = load_bytes();
    let header = tft::parse_header(&data, Target::Nx8048p050011rY).expect("header must parse");
    assert_eq!(header.width, 800);
    assert_eq!(header.height, 480);
    assert_eq!(header.total_size as usize, data.len());
}

#[test]
fn patches_unique_text_and_leaves_rest_of_file_untouched() {
    let mut data = load_bytes();
    let before = data.clone();

    let off = tft::patch_text(&mut data, "OFF", "AWY").expect("OFF must be a unique match");

    let diff_count = before
        .iter()
        .zip(data.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(diff_count, 3, "all 3 text bytes should change");
    assert_eq!(&data[off..off + 3], b"AWY");
}

#[test]
fn patches_shorter_text_and_zero_pads_remainder() {
    let mut data = load_bytes();
    let off = tft::patch_text(&mut data, "THREAD", "GO").expect("THREAD must be a unique match");
    assert_eq!(&data[off..off + 2], b"GO");
    assert_eq!(&data[off + 2..off + 6], &[0, 0, 0, 0]);
}

#[test]
fn rejects_longer_replacement_text() {
    let mut data = load_bytes();
    let err = tft::patch_text(&mut data, "OFF", "OFFLONGER").unwrap_err();
    assert!(matches!(
        err,
        nextion_tft_toolkit::TftError::TextTooLong(..)
    ));
}

#[test]
fn ellip_text_is_ambiguous_without_more_context() {
    // "ELLIP" appears twice in this real file -- patch_text must refuse
    // rather than guess which one to touch.
    let mut data = load_bytes();
    let err = tft::patch_text(&mut data, "ELLIP", "X").unwrap_err();
    assert!(matches!(
        err,
        nextion_tft_toolkit::TftError::Ambiguous { count: 2, .. }
    ));
}

#[test]
fn button_record_layout_matches_known_hmi_values_for_bd0() {
    // bD0 (digit-0 keypad button) is bco=0, bco2=52857 (0xce79), font=0 per
    // the .HMI decode -- confirms tft.rs's button_record_offset::{BCO,BCO2,
    // FONT} against real hardware output, not just a synthetic fixture.
    // Geometry (603,410,56,70) is ambiguous in this file (a coincidentally
    // identical-geometry button on a different page has different colors),
    // so this locates the record directly rather than via geometry search.
    let data = load_bytes();
    let rec_start = 0xc0818;

    let bco = u16::from_le_bytes([
        data[rec_start + tft::button_record_offset::BCO],
        data[rec_start + tft::button_record_offset::BCO + 1],
    ]);
    let bco2 = u16::from_le_bytes([
        data[rec_start + tft::button_record_offset::BCO2],
        data[rec_start + tft::button_record_offset::BCO2 + 1],
    ]);
    let font = data[rec_start + tft::button_record_offset::FONT];

    assert_eq!(bco, 0);
    assert_eq!(bco2, 52857);
    assert_eq!(font, 0);
}

#[test]
fn patches_button_background_color_at_confirmed_offset() {
    let mut data = load_bytes();
    let rec_start = 0xc0818;

    tft::patch_button_color_font(&mut data, rec_start, Some(0x1234), None, None).unwrap();

    let bco = u16::from_le_bytes([
        data[rec_start + tft::button_record_offset::BCO],
        data[rec_start + tft::button_record_offset::BCO + 1],
    ]);
    assert_eq!(bco, 0x1234);
    // bco2/font untouched.
    let bco2 = u16::from_le_bytes([
        data[rec_start + tft::button_record_offset::BCO2],
        data[rec_start + tft::button_record_offset::BCO2 + 1],
    ]);
    assert_eq!(bco2, 52857);
    assert_eq!(data[rec_start + tft::button_record_offset::FONT], 0);
}
