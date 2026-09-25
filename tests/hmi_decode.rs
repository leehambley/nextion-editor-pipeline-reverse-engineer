//! Integration tests against the real `h5.HMI` reference project (a
//! NanoEls H5 lathe UI, the same file the original reverse-engineering
//! session's `hmi_tool.py` was validated against).

use nextion_tft_toolkit::hmi;

fn load() -> hmi::Decoded {
    let data = std::fs::read("tests/fixtures/h5.HMI").expect("fixture must be present");
    hmi::decode(&data).expect("decode must succeed on a real project file")
}

#[test]
fn decodes_all_six_pages() {
    let decoded = load();
    assert_eq!(decoded.pages.len(), 6);
}

#[test]
fn decodes_281_components_total() {
    let decoded = load();
    let total: usize = decoded.pages.iter().map(|p| p.components.len()).sum();
    assert_eq!(total, 281);
}

#[test]
fn finds_bmode_button_with_expected_geometry_and_text() {
    let decoded = load();
    let comp = decoded
        .pages
        .iter()
        .flat_map(|p| &p.components)
        .find(|c| c.objname.as_deref() == Some("bMode"))
        .expect("bMode must exist");

    let get_int = |name: &str| {
        comp.attrs
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| match &a.value {
                hmi::AttrValue::Int(i) => Some(*i),
                _ => None,
            })
    };
    let get_str = |name: &str| {
        comp.attrs
            .iter()
            .find(|a| a.name == name)
            .and_then(|a| match &a.value {
                hmi::AttrValue::Str(s) => Some(s.clone()),
                _ => None,
            })
    };

    assert_eq!(get_str("type"), Some("b".to_string()));
    assert_eq!(get_int("x"), Some(100));
    assert_eq!(get_int("y"), Some(0));
    assert_eq!(get_int("w"), Some(183));
    assert_eq!(get_int("h"), Some(50));
    assert_eq!(get_str("txt"), Some("THREAD".to_string()));
}

#[test]
fn finds_boff_button_with_expected_text() {
    let decoded = load();
    let comp = decoded
        .pages
        .iter()
        .flat_map(|p| &p.components)
        .find(|c| c.objname.as_deref() == Some("bOff"))
        .expect("bOff must exist");

    let txt = comp
        .attrs
        .iter()
        .find(|a| a.name == "txt")
        .map(|a| a.value.to_string());
    assert_eq!(txt, Some("OFF".to_string()));
}

#[test]
fn payload_start_is_block_aligned_past_the_mirrored_directory() {
    let decoded = load();
    // Directory blocks are 0 and 1 (0x80000 bytes each); payload must start
    // at some later 0x80000-aligned offset.
    assert!(decoded.payload_start >= hmi::BLOCK_SIZE * 2);
    assert_eq!(decoded.payload_start % hmi::BLOCK_SIZE, 0);
}
