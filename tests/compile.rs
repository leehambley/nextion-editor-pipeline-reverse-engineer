//! Integration tests for the `compile` pipeline, using small synthetic
//! scaffold fixtures built in-test.
//!
//! There's no real Nextion Editor available in this environment to produce
//! a genuine scaffold `.HMI`/`.tft` pair, so these fixtures are hand-built
//! to have the *shape* the pipeline expects: a `.HMI` with named
//! components (so `compile` can resolve `objname` -> current
//! geometry/type/text), and a `.tft` whose header and one 84-byte
//! text-component record match the confirmed layout. This complements
//! (does not replace) the real-fixture round-trip tests in `tft_patch.rs`
//! and `hmi_decode.rs`.

use nextion_tft_toolkit::hmi;
use nextion_tft_toolkit::spec::UiSpec;
use nextion_tft_toolkit::target::Target;
use nextion_tft_toolkit::tft;

const HMI_BLOCK_SIZE: usize = hmi::BLOCK_SIZE;

fn attr_record(name: &str, value: &[u8], type_byte: u8) -> Vec<u8> {
    let mut buf = vec![0u8; 16];
    buf[..name.len()].copy_from_slice(name.as_bytes());
    buf.extend_from_slice(value);
    buf.push(type_byte);
    buf.extend_from_slice(&[0, 0, 0]);
    buf
}

fn str_attr(name: &str, s: &str) -> Vec<u8> {
    attr_record(name, s.as_bytes(), 0x11)
}

fn int_attr(name: &str, value: u64, width: usize) -> Vec<u8> {
    let bytes = value.to_le_bytes();
    attr_record(name, &bytes[..width], 0x12)
}

/// Builds a scaffold `.HMI` with one page containing one text-type ('t')
/// component named `textGear`, geometry 72,22,180,60, text "OFF".
fn build_scaffold_hmi() -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&str_attr("type", "y"));
    payload.extend_from_slice(&str_attr("objname", "page1"));
    payload.extend_from_slice(&str_attr("type", "t"));
    payload.extend_from_slice(&str_attr("objname", "textGear"));
    payload.extend_from_slice(&int_attr("id", 2, 1));
    payload.extend_from_slice(&int_attr("x", 72, 2));
    payload.extend_from_slice(&int_attr("y", 22, 2));
    payload.extend_from_slice(&int_attr("w", 180, 2));
    payload.extend_from_slice(&int_attr("h", 60, 2));
    payload.extend_from_slice(&str_attr("txt", "OFF"));
    // Bounds "txt"'s tail to just "OFF" -- without a following record, its
    // tail would run to the end of the payload and swallow the filler
    // bytes below (decode_tail has no length prefix to stop at otherwise).
    payload.extend_from_slice(&int_attr("_pad", 0, 2));

    let mut data = vec![0u8; HMI_BLOCK_SIZE * 2];
    data.extend_from_slice(&payload);
    // `find_payload_start` requires >1000 non-zero bytes in a block to
    // recognize it as the payload; 0xFF isn't a valid record-name byte, so
    // the scanner just skips over this filler.
    data.extend_from_slice(&[0xFFu8; 1200]);
    data
}

/// Builds a scaffold `.tft`: 64-byte header + one 84-byte text-component
/// record matching `textGear`'s geometry + a 104-byte text-pool slot
/// containing "OFF".
fn build_scaffold_tft() -> Vec<u8> {
    use tft::text_record_offset as o;

    let mut header = vec![0u8; tft::HEADER_LEN];
    header[0] = 0x00;
    header[1] = 0x01;
    header[tft::MAGIC_OFFSET] = tft::MAGIC[0];
    header[tft::MAGIC_OFFSET + 1] = tft::MAGIC[1];
    header[tft::WIDTH_OFFSET..tft::WIDTH_OFFSET + 2].copy_from_slice(&800u16.to_le_bytes());
    header[tft::HEIGHT_OFFSET..tft::HEIGHT_OFFSET + 2].copy_from_slice(&480u16.to_le_bytes());

    let mut rec = vec![0u8; tft::TEXT_RECORD_LEN];
    rec[o::X..o::X + 2].copy_from_slice(&72u16.to_le_bytes());
    rec[o::Y..o::Y + 2].copy_from_slice(&22u16.to_le_bytes());
    rec[o::W..o::W + 2].copy_from_slice(&180u16.to_le_bytes());
    rec[o::H..o::H + 2].copy_from_slice(&60u16.to_le_bytes());
    rec[o::ENDX..o::ENDX + 2].copy_from_slice(&251u16.to_le_bytes());
    rec[o::ENDY..o::ENDY + 2].copy_from_slice(&81u16.to_le_bytes());
    rec[o::PCO..o::PCO + 2].copy_from_slice(&0xffffu16.to_le_bytes());
    rec[o::FONT] = 2;

    let mut text_pool = vec![0u8; tft::TEXT_SLOT_LEN];
    text_pool[..3].copy_from_slice(b"OFF");

    let mut data = header;
    data.extend_from_slice(&rec);
    data.extend_from_slice(&text_pool);

    let total = data.len() as u32;
    data[tft::TOTAL_SIZE_OFFSET..tft::TOTAL_SIZE_OFFSET + 4].copy_from_slice(&total.to_le_bytes());
    data
}

#[test]
fn compile_applies_text_geometry_and_color_changes_together() {
    let scaffold_hmi_bytes = build_scaffold_hmi();
    let scaffold_decoded = hmi::decode(&scaffold_hmi_bytes).unwrap();
    let mut tft_data = build_scaffold_tft();

    let yaml = r#"
target: NX8048P050-011R-Y
page: page1
components:
  - objname: textGear
    x: 90
    txt: "ON!"
    pco: 0x049f
    font: 5
"#;
    let spec = UiSpec::from_yaml_str(yaml).unwrap();

    let changes = nextion_tft_toolkit::spec::compile(
        &spec,
        &scaffold_decoded,
        &mut tft_data,
        Target::Nx8048p050011rY,
    )
    .expect("compile must succeed against a well-formed scaffold");

    assert_eq!(changes.len(), 4); // txt, geometry, pco, font

    use tft::text_record_offset as o;
    let rec_start = tft::HEADER_LEN;
    let new_x = u16::from_le_bytes([tft_data[rec_start + o::X], tft_data[rec_start + o::X + 1]]);
    assert_eq!(new_x, 90);

    let pco = u16::from_le_bytes([
        tft_data[rec_start + o::PCO],
        tft_data[rec_start + o::PCO + 1],
    ]);
    assert_eq!(pco, 0x049f);
    assert_eq!(tft_data[rec_start + o::FONT], 5);

    let text_pool_start = rec_start + tft::TEXT_RECORD_LEN;
    assert_eq!(&tft_data[text_pool_start..text_pool_start + 3], b"ON!");
}

#[test]
fn compile_refuses_color_font_change_on_button_component() {
    // Same scaffold, but override the component's type to 'b' (button) --
    // the compiled record layout for buttons isn't reverse-engineered, so
    // `compile` must refuse rather than silently write wrong-offset bytes.
    let scaffold_hmi_bytes = build_scaffold_hmi();
    let mut scaffold_decoded = hmi::decode(&scaffold_hmi_bytes).unwrap();
    for page in &mut scaffold_decoded.pages {
        for comp in &mut page.components {
            for attr in &mut comp.attrs {
                if attr.name == "type" {
                    attr.value = hmi::AttrValue::Str("b".to_string());
                }
            }
        }
    }
    let mut tft_data = build_scaffold_tft();

    let yaml = r#"
target: NX8048P050-011R-Y
page: page1
components:
  - objname: textGear
    pco: 0x049f
"#;
    let spec = UiSpec::from_yaml_str(yaml).unwrap();

    let err = nextion_tft_toolkit::spec::compile(
        &spec,
        &scaffold_decoded,
        &mut tft_data,
        Target::Nx8048p050011rY,
    )
    .unwrap_err();

    assert!(matches!(
        err,
        nextion_tft_toolkit::SpecError::ColorFontUnsupportedForType { .. }
    ));
}

#[test]
fn compile_errors_when_target_mismatches_tft_header() {
    let scaffold_hmi_bytes = build_scaffold_hmi();
    let scaffold_decoded = hmi::decode(&scaffold_hmi_bytes).unwrap();
    let mut tft_data = build_scaffold_tft();
    // Corrupt the header's declared width so it no longer matches the
    // target the spec claims.
    tft_data[tft::WIDTH_OFFSET..tft::WIDTH_OFFSET + 2].copy_from_slice(&320u16.to_le_bytes());

    let yaml = r#"
target: NX8048P050-011R-Y
page: page1
components:
  - objname: textGear
    txt: "ON!"
"#;
    let spec = UiSpec::from_yaml_str(yaml).unwrap();

    let err = nextion_tft_toolkit::spec::compile(
        &spec,
        &scaffold_decoded,
        &mut tft_data,
        Target::Nx8048p050011rY,
    )
    .unwrap_err();

    assert!(matches!(
        err,
        nextion_tft_toolkit::SpecError::Tft(
            nextion_tft_toolkit::TftError::DimensionMismatch { .. }
        )
    ));
}
