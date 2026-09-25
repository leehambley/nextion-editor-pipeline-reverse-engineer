//! Render a [`crate::spec::UiSpec`] as a static HTML page, for previewing a
//! layout in a browser before committing to real hardware/toolchain work.
//!
//! This is a second, independent consumer of the same spec data `compile`
//! uses -- it needs no scaffold `.HMI`/`.tft` (there may not be one yet)
//! and never touches the Nextion formats at all. Each [`ComponentSpec`]'s
//! `x/y/w/h/txt/type/bco/pco` map directly to an absolutely-positioned
//! `<div>`; nothing about the compiled formats' record offsets or byte
//! layout is relevant here.
//!
//! Styling is deliberately crude: flat grey buttons with `outset`/`inset`
//! borders (Windows 3.1-era, not modern flat/material design) and no
//! shadows, gradients, or rounded corners -- this is a layout/spacing
//! check, not a visual mockup.

use crate::spec::{ComponentSpec, UiSpec};

/// Render `spec` as a complete, self-contained HTML document sized to the
/// spec's target resolution.
pub fn render(spec: &UiSpec) -> String {
    let (width, height) = spec
        .target()
        .map(|t| (t.width(), t.height()))
        .unwrap_or((800, 480));

    let mut body = String::new();
    for comp in &spec.components {
        body.push_str(&render_component(comp));
    }

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>{title} -- layout preview</title>
<style>
{style}
</style>
</head>
<body>
<div class="screen" style="width:{width}px;height:{height}px;">
{body}</div>
</body>
</html>
"#,
        title = html_escape(&spec.page),
        style = STYLE,
        width = width,
        height = height,
        body = body,
    )
}

const STYLE: &str = r#"
body {
  margin: 20px;
  background: #808080;
  font-family: "MS Sans Serif", Tahoma, sans-serif;
}
.screen {
  position: relative;
  background: #000000;
  overflow: hidden;
  outline: 2px solid #000000;
}
.comp {
  position: absolute;
  box-sizing: border-box;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: hidden;
  font-size: 14px;
  color: #000000;
  white-space: nowrap;
  text-overflow: ellipsis;
}
.comp-t {
  background: transparent;
  color: #ffffff;
  justify-content: flex-start;
}
.comp-b {
  background: #c0c0c0;
  border: 2px outset #c0c0c0;
}
.comp-b:active {
  border-style: inset;
}
.comp-m {
  background: rgba(255, 0, 0, 0.15);
  border: 1px dashed #ff0000;
  color: #ff0000;
  font-size: 10px;
}
"#;

fn render_component(comp: &ComponentSpec) -> String {
    let x = comp.x.unwrap_or(0);
    let y = comp.y.unwrap_or(0);
    let w = comp.w.unwrap_or(40);
    let h = comp.h.unwrap_or(20);
    let comp_type = comp.component_type.as_deref().unwrap_or("t");
    let type_class = match comp_type {
        "b" => "comp-b",
        "m" => "comp-m",
        _ => "comp-t",
    };

    let mut style = format!(
        "left:{x}px;top:{y}px;width:{w}px;height:{h}px;",
        x = x,
        y = y,
        w = w,
        h = h
    );
    if comp_type != "m" {
        if let Some(bco) = comp.bco {
            style.push_str(&format!("background:{};", rgb565_to_css(bco)));
        }
        if let Some(pco) = comp.pco {
            style.push_str(&format!("color:{};", rgb565_to_css(pco)));
        }
    }

    // Hotspots have no `txt` (see the format doc's "m = Hotspot" note) --
    // show the objname instead so the overlay is still identifiable.
    let label = if comp_type == "m" {
        &comp.objname
    } else {
        comp.txt.as_deref().unwrap_or("")
    };
    let label = html_escape(label);

    format!(
        "  <div class=\"comp {class}\" style=\"{style}\" title=\"{objname}\">{label}</div>\n",
        class = type_class,
        style = style,
        objname = html_escape(&comp.objname),
        label = label,
    )
}

/// Nextion colors are RGB565 (5 bits red, 6 bits green, 5 bits blue,
/// packed into a u16) -- convert to an 8-bit-per-channel CSS `rgb()`.
fn rgb565_to_css(color: u16) -> String {
    let r5 = (color >> 11) & 0x1F;
    let g6 = (color >> 5) & 0x3F;
    let b5 = color & 0x1F;
    // Scale each channel up to 0-255 by replicating its high bits into the
    // low bits, the standard RGB565->RGB888 expansion.
    let r = (r5 << 3) | (r5 >> 2);
    let g = (g6 << 2) | (g6 >> 4);
    let b = (b5 << 3) | (b5 >> 2);
    format!("rgb({r},{g},{b})")
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spec::UiSpec;

    fn spec_with(components: Vec<ComponentSpec>) -> UiSpec {
        UiSpec {
            target: "NX8048P050-011R-Y".to_string(),
            page: "page0".to_string(),
            components,
        }
    }

    #[test]
    fn rgb565_white_converts_to_255_255_255() {
        assert_eq!(rgb565_to_css(0xffff), "rgb(255,255,255)");
    }

    #[test]
    fn rgb565_black_converts_to_0_0_0() {
        assert_eq!(rgb565_to_css(0x0000), "rgb(0,0,0)");
    }

    #[test]
    fn rgb565_pure_red_converts_correctly() {
        // 0xF800 = 11111 000000 00000 -> full red, no green/blue.
        assert_eq!(rgb565_to_css(0xF800), "rgb(255,0,0)");
    }

    #[test]
    fn render_includes_screen_dimensions_for_target() {
        let spec = spec_with(vec![]);
        let html = render(&spec);
        assert!(html.contains("width:800px"));
        assert!(html.contains("height:480px"));
    }

    #[test]
    fn render_places_button_with_position_and_text() {
        let spec = spec_with(vec![ComponentSpec {
            objname: "bOn".to_string(),
            component_type: Some("b".to_string()),
            x: Some(10),
            y: Some(20),
            w: Some(100),
            h: Some(50),
            txt: Some("ON".to_string()),
            ..Default::default()
        }]);
        let html = render(&spec);
        assert!(html.contains("left:10px;top:20px;width:100px;height:50px;"));
        assert!(html.contains(">ON<"));
        assert!(html.contains("comp-b"));
    }

    #[test]
    fn render_applies_background_and_text_color() {
        let spec = spec_with(vec![ComponentSpec {
            objname: "bStop".to_string(),
            component_type: Some("b".to_string()),
            bco: Some(0xF800),
            pco: Some(0xffff),
            txt: Some("STOP".to_string()),
            ..Default::default()
        }]);
        let html = render(&spec);
        assert!(html.contains("background:rgb(255,0,0);"));
        assert!(html.contains("color:rgb(255,255,255);"));
    }

    #[test]
    fn render_shows_hotspot_as_dashed_overlay_with_objname_not_txt() {
        let spec = spec_with(vec![ComponentSpec {
            objname: "touchFace".to_string(),
            component_type: Some("m".to_string()),
            txt: Some("should not appear".to_string()),
            ..Default::default()
        }]);
        let html = render(&spec);
        assert!(html.contains("comp-m"));
        assert!(html.contains(">touchFace<"));
        assert!(!html.contains("should not appear"));
    }

    #[test]
    fn render_escapes_html_special_characters_in_text_and_objname() {
        let spec = spec_with(vec![ComponentSpec {
            objname: "b<script>".to_string(),
            component_type: Some("b".to_string()),
            txt: Some("<b>&\"".to_string()),
            ..Default::default()
        }]);
        let html = render(&spec);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(html.contains("&lt;b&gt;&amp;&quot;"));
    }

    #[test]
    fn render_defaults_missing_geometry_to_small_visible_box() {
        let spec = spec_with(vec![ComponentSpec {
            objname: "noGeom".to_string(),
            ..Default::default()
        }]);
        let html = render(&spec);
        assert!(html.contains("left:0px;top:0px;width:40px;height:20px;"));
    }
}
