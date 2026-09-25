//! Integration tests for same-length `.HMI` attribute patching against the
//! real `h5.HMI` reference project.

use nextion_tft_toolkit::hmi::{self, AttrValue, Selector};
use nextion_tft_toolkit::HmiError;

fn load_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/h5.HMI").expect("fixture must be present")
}

#[test]
fn patches_bmode_text_in_place_same_length() {
    let mut data = load_bytes();
    let decoded = hmi::decode(&data).unwrap();

    // "THREAD" (6 chars) -> "GEARBX" (6 chars, all bytes differ): same
    // length, should succeed and touch nothing else in the file.
    let before = data.clone();
    let (sel, _) = Selector::parse(":bMode:txt=GEARBX").unwrap();
    hmi::patch_attr(&mut data, &decoded, &sel, "GEARBX").expect("same-length patch must succeed");

    let diff_count = before
        .iter()
        .zip(data.iter())
        .filter(|(a, b)| a != b)
        .count();
    assert_eq!(diff_count, 6, "only the 6 text bytes should change");

    let redecoded = hmi::decode(&data).unwrap();
    let comp = redecoded
        .pages
        .iter()
        .flat_map(|p| &p.components)
        .find(|c| c.objname.as_deref() == Some("bMode"))
        .unwrap();
    let txt = comp.attrs.iter().find(|a| a.name == "txt").unwrap();
    assert_eq!(txt.value, AttrValue::Str("GEARBX".to_string()));
}

#[test]
fn rejects_length_changing_patch_on_real_file() {
    let mut data = load_bytes();
    let decoded = hmi::decode(&data).unwrap();

    let (sel, _) = Selector::parse(":bMode:txt=LONGERTEXT").unwrap();
    let err = hmi::patch_attr(&mut data, &decoded, &sel, "LONGERTEXT").unwrap_err();
    assert!(matches!(err, HmiError::LengthMismatch { .. }));
}

#[test]
fn patches_geometry_int_attribute_in_place() {
    let mut data = load_bytes();
    let decoded = hmi::decode(&data).unwrap();

    let (sel, _) = Selector::parse(":bMode:x=200").unwrap();
    hmi::patch_attr(&mut data, &decoded, &sel, "200").expect("int patch must succeed");

    let redecoded = hmi::decode(&data).unwrap();
    let comp = redecoded
        .pages
        .iter()
        .flat_map(|p| &p.components)
        .find(|c| c.objname.as_deref() == Some("bMode"))
        .unwrap();
    let x = comp.attrs.iter().find(|a| a.name == "x").unwrap();
    assert_eq!(x.value, AttrValue::Int(200));
}
