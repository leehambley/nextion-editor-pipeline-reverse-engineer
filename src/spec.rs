//! YAML UI spec schema and the `compile` pipeline.
//!
//! This does **not** synthesize a `.tft` from nothing -- see
//! `docs/formats/nextion-tft-format.md` §6 ("recommended strategy given
//! one hardware target only") for why that's out of scope. Instead, it
//! automates exactly the manual workflow that doc recommends:
//!
//! 1. Build one scaffold `.HMI` in the real Nextion Editor containing every
//!    component the UI needs, and compile it once to get a scaffold `.tft`.
//! 2. From then on, describe the *desired* text/geometry/color/font for
//!    each named component in a YAML spec.
//! 3. `compile` diffs the spec against the scaffold's own `.HMI` (which
//!    still has `objname`s -- the `.tft` doesn't) to find what changed,
//!    locates each changed component's record in the scaffold `.tft` by
//!    searching for its *current* geometry, and patches only what differs.
//!
//! No component is ever added or removed and no font/image data is ever
//! touched -- both remain unsupported (see `docs/targets.md`).

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::error::SpecError;
use crate::hmi::{self, AttrValue, Decoded};
use crate::target::Target;
use crate::tft;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ComponentSpec {
    pub objname: String,
    /// Component class: `t` (text/label), `b` (button), `m` (hotspot -- see
    /// `docs/formats/nextion-hmi-format.md` §3.2). Not used by `compile`
    /// today (it reads type from the scaffold `.HMI` instead); used by the
    /// [`crate::html`] renderer, which has no scaffold to read it from.
    #[serde(rename = "type")]
    pub component_type: Option<String>,
    pub x: Option<u16>,
    pub y: Option<u16>,
    pub w: Option<u16>,
    pub h: Option<u16>,
    pub txt: Option<String>,
    /// Text color (RGB565), e.g. `0xffff` for white.
    pub pco: Option<u16>,
    /// Background color (RGB565), normal state -- `b` components only.
    pub bco: Option<u16>,
    /// Background color (RGB565), pressed state -- `b` components only.
    pub bco2: Option<u16>,
    pub font: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UiSpec {
    pub target: String,
    pub page: String,
    pub components: Vec<ComponentSpec>,
}

impl UiSpec {
    pub fn from_yaml_str(s: &str) -> Result<Self, SpecError> {
        Ok(serde_yaml::from_str(s)?)
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, SpecError> {
        let s = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&s)
    }

    pub fn target(&self) -> Result<Target, SpecError> {
        self.target
            .parse::<Target>()
            .map_err(|e| SpecError::Io(std::io::Error::new(std::io::ErrorKind::InvalidInput, e)))
    }
}

#[derive(Debug, Clone, Default)]
struct ScaffoldComponent {
    component_type: String,
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    txt: Option<String>,
}

fn attr_str(attrs: &[hmi::AttrRecord], name: &str) -> Option<String> {
    attrs
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            AttrValue::Str(s) => Some(s.clone()),
            _ => None,
        })
}

fn attr_int(attrs: &[hmi::AttrRecord], name: &str) -> Option<u64> {
    attrs
        .iter()
        .find(|a| a.name == name)
        .and_then(|a| match &a.value {
            AttrValue::Int(i) => Some(*i),
            _ => None,
        })
}

fn index_scaffold(decoded: &Decoded) -> BTreeMap<String, ScaffoldComponent> {
    let mut out = BTreeMap::new();
    for page in &decoded.pages {
        for comp in &page.components {
            let Some(objname) = &comp.objname else {
                continue;
            };
            let component_type = attr_str(&comp.attrs, "type").unwrap_or_default();
            let x = attr_int(&comp.attrs, "x").unwrap_or(0) as u16;
            let y = attr_int(&comp.attrs, "y").unwrap_or(0) as u16;
            let w = attr_int(&comp.attrs, "w").unwrap_or(0) as u16;
            let h = attr_int(&comp.attrs, "h").unwrap_or(0) as u16;
            let txt = attr_str(&comp.attrs, "txt");
            out.insert(
                objname.clone(),
                ScaffoldComponent {
                    component_type,
                    x,
                    y,
                    w,
                    h,
                    txt,
                },
            );
        }
    }
    out
}

/// One applied change, returned for CLI/log reporting.
#[derive(Debug, Clone)]
pub struct AppliedChange {
    pub objname: String,
    pub field: String,
    pub from: String,
    pub to: String,
}

/// Run the `compile` pipeline: patch `scaffold_tft_data` in place so it
/// matches `spec`, using `scaffold_hmi` (already decoded) to resolve each
/// spec component's current geometry/type/text.
pub fn compile(
    spec: &UiSpec,
    scaffold_hmi: &Decoded,
    scaffold_tft_data: &mut [u8],
    target: Target,
) -> Result<Vec<AppliedChange>, SpecError> {
    tft::parse_header(scaffold_tft_data, target)?;

    let index = index_scaffold(scaffold_hmi);
    let mut changes = Vec::new();

    for comp in &spec.components {
        let current =
            index
                .get(&comp.objname)
                .ok_or_else(|| SpecError::ComponentNotInScaffold {
                    objname: comp.objname.clone(),
                })?;

        let target_x = comp.x.unwrap_or(current.x);
        let target_y = comp.y.unwrap_or(current.y);
        let target_w = comp.w.unwrap_or(current.w);
        let target_h = comp.h.unwrap_or(current.h);

        let geometry_changed = (target_x, target_y, target_w, target_h)
            != (current.x, current.y, current.w, current.h);

        if current.w == 0 && current.h == 0 && (geometry_changed || comp.txt.is_some()) {
            return Err(SpecError::MissingScaffoldGeometry {
                objname: comp.objname.clone(),
            });
        }

        // Text patch: search using the *current* text, so it must happen
        // before geometry changes (which don't move text-pool bytes, but
        // ordering text-before-geometry keeps both searches independent and
        // matches the documented manual workflow).
        if let Some(new_txt) = &comp.txt {
            if let Some(old_txt) = &current.txt {
                if old_txt != new_txt {
                    tft::patch_text(scaffold_tft_data, old_txt, new_txt)?;
                    changes.push(AppliedChange {
                        objname: comp.objname.clone(),
                        field: "txt".to_string(),
                        from: old_txt.clone(),
                        to: new_txt.clone(),
                    });
                }
            }
        }

        if geometry_changed {
            tft::patch_geom(
                scaffold_tft_data,
                (current.x, current.y, current.w, current.h),
                (target_x, target_y, target_w, target_h),
                None,
            )?;
            changes.push(AppliedChange {
                objname: comp.objname.clone(),
                field: "geometry".to_string(),
                from: format!("{},{},{},{}", current.x, current.y, current.w, current.h),
                to: format!("{target_x},{target_y},{target_w},{target_h}"),
            });
        }

        if comp.pco.is_some() && current.component_type != "t" {
            return Err(SpecError::ColorFontUnsupportedForType {
                objname: comp.objname.clone(),
                field: "pco".to_string(),
                component_type: current.component_type.clone(),
            });
        }
        if (comp.bco.is_some() || comp.bco2.is_some()) && current.component_type != "b" {
            return Err(SpecError::ColorFontUnsupportedForType {
                objname: comp.objname.clone(),
                field: "bco/bco2".to_string(),
                component_type: current.component_type.clone(),
            });
        }

        if comp.pco.is_some() || comp.bco.is_some() || comp.bco2.is_some() || comp.font.is_some() {
            // Locate the record using the *post-geometry-patch* quad, since
            // patch_geom (if it ran above) already rewrote those bytes. Both
            // record layouts put x,y,w,h at the same offset (see tft.rs), so
            // either find_*_record_by_geometry call is equivalent here.
            let record_start = tft::find_text_record_by_geometry(
                scaffold_tft_data,
                (target_x, target_y, target_w, target_h),
            )?;

            match current.component_type.as_str() {
                "t" => {
                    tft::patch_component_color_font(
                        scaffold_tft_data,
                        record_start,
                        comp.pco,
                        comp.font,
                    )?;
                    if let Some(pco) = comp.pco {
                        changes.push(AppliedChange {
                            objname: comp.objname.clone(),
                            field: "pco".to_string(),
                            from: "?".to_string(),
                            to: format!("{pco:#06x}"),
                        });
                    }
                }
                "b" => {
                    tft::patch_button_color_font(
                        scaffold_tft_data,
                        record_start,
                        comp.bco,
                        comp.bco2,
                        comp.font,
                    )?;
                    if let Some(bco) = comp.bco {
                        changes.push(AppliedChange {
                            objname: comp.objname.clone(),
                            field: "bco".to_string(),
                            from: "?".to_string(),
                            to: format!("{bco:#06x}"),
                        });
                    }
                    if let Some(bco2) = comp.bco2 {
                        changes.push(AppliedChange {
                            objname: comp.objname.clone(),
                            field: "bco2".to_string(),
                            from: "?".to_string(),
                            to: format!("{bco2:#06x}"),
                        });
                    }
                }
                other => {
                    return Err(SpecError::ColorFontUnsupportedForType {
                        objname: comp.objname.clone(),
                        field: "font".to_string(),
                        component_type: other.to_string(),
                    });
                }
            }

            if let Some(font) = comp.font {
                changes.push(AppliedChange {
                    objname: comp.objname.clone(),
                    field: "font".to_string(),
                    from: "?".to_string(),
                    to: font.to_string(),
                });
            }
        }
    }

    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hmi::{AttrRecord, Component, Page};

    fn make_attr(name: &str, value: AttrValue) -> AttrRecord {
        AttrRecord {
            file_offset: 0,
            name: name.to_string(),
            value,
            value_len: 0,
        }
    }

    fn make_scaffold() -> Decoded {
        let comp = Component {
            file_offset: 0,
            objname: Some("textGear".to_string()),
            attrs: vec![
                make_attr("type", AttrValue::Str("t".to_string())),
                make_attr("objname", AttrValue::Str("textGear".to_string())),
                make_attr("x", AttrValue::Int(72)),
                make_attr("y", AttrValue::Int(22)),
                make_attr("w", AttrValue::Int(180)),
                make_attr("h", AttrValue::Int(60)),
                make_attr("txt", AttrValue::Str("OFF".to_string())),
            ],
        };
        Decoded {
            payload_start: 0,
            pages: vec![Page {
                objname: Some("page1".to_string()),
                attrs: vec![],
                components: vec![comp],
            }],
        }
    }

    fn make_tft_bytes() -> Vec<u8> {
        let mut header = vec![0u8; tft::HEADER_LEN];
        header[0] = 0x00;
        header[1] = 0x01;
        header[tft::MAGIC_OFFSET] = tft::MAGIC[0];
        header[tft::MAGIC_OFFSET + 1] = tft::MAGIC[1];
        header[tft::WIDTH_OFFSET..tft::WIDTH_OFFSET + 2].copy_from_slice(&800u16.to_le_bytes());
        header[tft::HEIGHT_OFFSET..tft::HEIGHT_OFFSET + 2].copy_from_slice(&480u16.to_le_bytes());

        let mut rec = vec![0u8; tft::TEXT_RECORD_LEN];
        use tft::text_record_offset as o;
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
        data[tft::TOTAL_SIZE_OFFSET..tft::TOTAL_SIZE_OFFSET + 4]
            .copy_from_slice(&total.to_le_bytes());
        data
    }

    #[test]
    fn compile_patches_text_when_changed() {
        let scaffold = make_scaffold();
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                x: None,
                y: None,
                w: None,
                h: None,
                txt: Some("ON!".to_string()),
                pco: None,
                font: None,
                ..Default::default()
            }],
        };

        let changes = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "txt");

        let rec_start = tft::HEADER_LEN;
        let text_pool_start = rec_start + tft::TEXT_RECORD_LEN;
        assert_eq!(&tft_data[text_pool_start..text_pool_start + 3], b"ON!");
    }

    #[test]
    fn compile_patches_geometry_when_changed() {
        let scaffold = make_scaffold();
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                x: Some(100),
                y: None,
                w: None,
                h: None,
                txt: None,
                pco: None,
                font: None,
                ..Default::default()
            }],
        };

        let changes = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "geometry");

        use tft::text_record_offset as o;
        let rec_start = tft::HEADER_LEN;
        let new_x = u16::from_le_bytes([
            data_at(&tft_data, rec_start + o::X),
            data_at(&tft_data, rec_start + o::X + 1),
        ]);
        assert_eq!(new_x, 100);
    }

    fn data_at(data: &[u8], off: usize) -> u8 {
        data[off]
    }

    #[test]
    fn compile_patches_color_and_font_for_text_component() {
        let scaffold = make_scaffold();
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                x: None,
                y: None,
                w: None,
                h: None,
                txt: None,
                pco: Some(0x049f),
                font: Some(5),
                ..Default::default()
            }],
        };

        let changes = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap();
        assert_eq!(changes.len(), 2);

        use tft::text_record_offset as o;
        let rec_start = tft::HEADER_LEN;
        let pco = u16::from_le_bytes([
            tft_data[rec_start + o::PCO],
            tft_data[rec_start + o::PCO + 1],
        ]);
        assert_eq!(pco, 0x049f);
        assert_eq!(tft_data[rec_start + o::FONT], 5);
    }

    #[test]
    fn compile_refuses_color_font_on_non_text_component() {
        let mut scaffold = make_scaffold();
        scaffold.pages[0].components[0].attrs[0] =
            make_attr("type", AttrValue::Str("b".to_string()));
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                x: None,
                y: None,
                w: None,
                h: None,
                txt: None,
                pco: Some(0x049f),
                font: None,
                ..Default::default()
            }],
        };

        let err = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap_err();
        assert!(matches!(err, SpecError::ColorFontUnsupportedForType { .. }));
    }

    #[test]
    fn compile_patches_button_bco_bco2_and_font() {
        let mut scaffold = make_scaffold();
        scaffold.pages[0].components[0].attrs[0] =
            make_attr("type", AttrValue::Str("b".to_string()));
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                bco: Some(0xf800),
                bco2: Some(0x0c80),
                font: Some(1),
                ..Default::default()
            }],
        };

        let changes = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap();
        assert_eq!(changes.len(), 3); // bco, bco2, font

        use tft::button_record_offset as o;
        let rec_start = tft::HEADER_LEN;
        let bco = u16::from_le_bytes([
            tft_data[rec_start + o::BCO],
            tft_data[rec_start + o::BCO + 1],
        ]);
        let bco2 = u16::from_le_bytes([
            tft_data[rec_start + o::BCO2],
            tft_data[rec_start + o::BCO2 + 1],
        ]);
        assert_eq!(bco, 0xf800);
        assert_eq!(bco2, 0x0c80);
        assert_eq!(tft_data[rec_start + o::FONT], 1);
    }

    #[test]
    fn compile_refuses_bco_on_text_component() {
        let scaffold = make_scaffold(); // textGear's scaffold type is "t"
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                bco: Some(0xf800),
                ..Default::default()
            }],
        };

        let err = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap_err();
        assert!(matches!(err, SpecError::ColorFontUnsupportedForType { .. }));
    }

    #[test]
    fn compile_errors_when_spec_component_missing_from_scaffold() {
        let scaffold = make_scaffold();
        let mut tft_data = make_tft_bytes();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "doesNotExist".to_string(),
                x: None,
                y: None,
                w: None,
                h: None,
                txt: Some("hi".to_string()),
                pco: None,
                font: None,
                ..Default::default()
            }],
        };

        let err = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap_err();
        assert!(matches!(err, SpecError::ComponentNotInScaffold { .. }));
    }

    #[test]
    fn compile_is_a_noop_when_spec_matches_scaffold_exactly() {
        let scaffold = make_scaffold();
        let mut tft_data = make_tft_bytes();
        let before = tft_data.clone();
        let spec = UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page1".to_string(),
            components: vec![ComponentSpec {
                objname: "textGear".to_string(),
                x: Some(72),
                y: Some(22),
                w: Some(180),
                h: Some(60),
                txt: Some("OFF".to_string()),
                pco: None,
                font: None,
                ..Default::default()
            }],
        };

        let changes = compile(&spec, &scaffold, &mut tft_data, Target::Nx8048p050011rY).unwrap();
        assert!(changes.is_empty());
        assert_eq!(tft_data, before);
    }

    #[test]
    fn ui_spec_parses_from_yaml() {
        let yaml = r#"
target: NX8048P050-011R-Y
page: page1
components:
  - objname: textGear
    txt: "ON!"
    x: 100
"#;
        let spec = UiSpec::from_yaml_str(yaml).unwrap();
        assert_eq!(spec.target().unwrap(), Target::Nx8048p050011rY);
        assert_eq!(spec.components.len(), 1);
        assert_eq!(spec.components[0].objname, "textGear");
        assert_eq!(spec.components[0].x, Some(100));
    }
}
