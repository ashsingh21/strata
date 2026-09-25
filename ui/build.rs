//! Generates the Strata CSS (`styles/base.css`, `styles/studio.css`,
//! `styles/daylight.css`) and a Rust palette module from
//! `../design/tokens.json`, so the token file is the single source of
//! truth for both the stylesheet and the custom-drawn canvas views.

use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let tokens_path = manifest_dir.join("../design/tokens.json");
    println!("cargo:rerun-if-changed={}", tokens_path.display());

    let raw = fs::read_to_string(&tokens_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", tokens_path.display()));
    let tokens: Value = serde_json::from_str(&raw).expect("tokens.json is not valid JSON");

    let (studio, daylight) = resolve_colors(&tokens);
    let scalars = resolve_scalars(&tokens);

    let styles_dir = manifest_dir.join("styles");
    fs::create_dir_all(&styles_dir).unwrap();
    fs::write(styles_dir.join("base.css"), render_base_css(&scalars)).unwrap();
    fs::write(styles_dir.join("studio.css"), render_theme_css(&studio, None)).unwrap();
    fs::write(styles_dir.join("daylight.css"), render_theme_css(&daylight, Some("theme-daylight")))
        .unwrap();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("tokens.rs"), render_tokens_rs(&studio, &daylight, &scalars)).unwrap();
}

/// name -> "#rrggbb" per theme.
type ColorMap = HashMap<String, String>;

fn resolve_colors(tokens: &Value) -> (ColorMap, ColorMap) {
    let entries = tokens["color"]["tokens"].as_array().expect("color.tokens array");

    let mut studio = ColorMap::new();
    let mut daylight = ColorMap::new();
    let mut aliases: Vec<(String, String)> = Vec::new();

    for entry in entries {
        let name = entry["name"].as_str().unwrap().to_string();
        match &entry["value"] {
            Value::Object(map) => {
                let s = map["studio"].as_str().unwrap_or_default().to_string();
                let d = map["daylight"].as_str().unwrap_or_default().to_string();
                studio.insert(name.clone(), s);
                daylight.insert(name.clone(), d);
            }
            Value::String(s) => {
                if let Some(target) = s.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
                    aliases.push((name, target.to_string()));
                } else {
                    studio.insert(name.clone(), s.clone());
                    daylight.insert(name.clone(), s.clone());
                }
            }
            other => panic!("unexpected value for token {name}: {other:?}"),
        }
    }

    // Aliases reference tokens that may themselves be defined later in the
    // file, so resolve in a few fixed-point passes rather than assuming
    // declaration order.
    for _ in 0..4 {
        let mut remaining = Vec::new();
        for (name, target) in aliases {
            match (studio.get(&target).cloned(), daylight.get(&target).cloned()) {
                (Some(s), Some(d)) => {
                    studio.insert(name.clone(), s);
                    daylight.insert(name, d);
                }
                _ => remaining.push((name, target)),
            }
        }
        aliases = remaining;
        if aliases.is_empty() {
            break;
        }
    }
    assert!(aliases.is_empty(), "unresolved color aliases: {aliases:?}");

    (studio, daylight)
}

/// name -> pixel value, for spacing/radius/size (theme-invariant).
type ScalarMap = HashMap<String, f32>;

fn resolve_scalars(tokens: &Value) -> ScalarMap {
    let mut out = ScalarMap::new();
    for group in ["spacing", "radius", "size"] {
        for entry in tokens[group]["tokens"].as_array().unwrap() {
            let name = entry["name"].as_str().unwrap().to_string();
            let value = entry["value"].as_str().unwrap();
            let px: f32 = value.trim_end_matches("px").parse().unwrap_or_else(|_| {
                if value == "999px" { 999.0 } else { panic!("bad scalar value {value}") }
            });
            out.insert(name, px);
        }
    }
    out
}

/// Parses either `#rrggbb` or `rgba(r, g, b, a)` (`a` in 0..1) into 8-bit
/// RGBA. Opaque hex colors get alpha 255.
fn parse_color(value: &str) -> (u8, u8, u8, u8) {
    if let Some(hex) = value.strip_prefix('#') {
        assert_eq!(hex.len(), 6, "expected 6-digit hex color, got {value}");
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap();
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap();
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap();
        (r, g, b, 255)
    } else if let Some(inner) = value.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<f32> = inner.split(',').map(|p| p.trim().parse().unwrap()).collect();
        assert_eq!(parts.len(), 4, "expected rgba(r,g,b,a), got {value}");
        (parts[0] as u8, parts[1] as u8, parts[2] as u8, (parts[3] * 255.0).round() as u8)
    } else {
        panic!("unrecognized color format: {value}");
    }
}

// --- CSS rendering ---------------------------------------------------

/// Structural rules shared by both themes: sizes, radii and type, plus the
/// handful of colours that never change between themes (clip colours and
/// their `on-clip` text).
fn render_base_css(scalars: &ScalarMap) -> String {
    let space = |n: &str| scalars[n];
    format!(
        r#"/* GENERATED by ui/build.rs from design/tokens.json. Do not edit by hand. */

.btn {{
  height: {control}px;
  min-width: {control}px;
  border-radius: {radius_sm}px;
  border-width: 1px;
  font-size: 12px;
  padding: 0px 8px;
}}
.btn.sm {{
  height: 18px;
  min-width: 18px;
  font-size: 10px;
  padding: 0px 4px;
}}
.readout {{
  height: {control}px;
  border-radius: {radius_sm}px;
  border-width: 1px;
  font-size: 11px;
  padding: 0px 8px;
}}
.readout .unit {{
  font-size: 10px;
}}
.label {{
  font-size: 10px;
}}
.meta {{
  font-size: 10px;
  font-family: monospace;
}}
.mono {{
  font-family: monospace;
  font-size: 11px;
}}
.control {{
  font-size: 12px;
}}
.panel {{
  border-radius: {radius_md}px;
  border-width: 1px;
}}
.pill {{
  height: {control}px;
  border-radius: {radius_pill}px;
  border-width: 1px;
  font-size: 11px;
  padding: 0px 8px 0px 6px;
}}
.hairline {{
  height: 1px;
}}
.transport {{
  border-bottom-width: 1px;
}}
.swatch {{
  width: {space1}px;
  height: {space1}px;
  border-radius: {radius_xs}px;
}}

.tl-corner {{
  height: {ruler}px;
  border-right-width: 1px;
  border-bottom-width: 1px;
  padding: 0px {space2}px;
}}
.tl-heads {{
  border-right-width: 1px;
}}
.tl-head {{
  height: {lane}px;
  border-bottom-width: 1px;
  padding: 6px {space2}px;
}}
.tl-head-auto {{
  height: {lane_auto}px;
  padding-left: {space6}px;
}}

.synth-devhead {{
  height: 32px;
  border-radius: {radius_md}px;
  border-width: 1px;
  padding: 0px {space2}px;
}}
.synth-sec {{
  border-radius: {radius_md}px;
  padding: {space2}px;
}}
.synth-disp {{
  border-radius: {radius_sm}px;
}}
.synth-seg {{
  border-width: 1px;
  border-radius: {radius_sm}px;
  height: 20px;
}}
.synth-seg-btn {{
  height: 20px;
  min-width: 22px;
  font-size: 10px;
  padding: 0px 6px;
}}
.synth-keys {{
  border-radius: {radius_sm}px;
  border-width: 1px;
}}
.synth-help-panel {{
  border-radius: {radius_md}px;
  border-width: 1px;
  z-index: 100;
}}
.interval-overlay {{
  border-radius: {radius_md}px;
  border-width: 1px;
  z-index: 200;
}}
.piano-roll-backdrop {{
  background-color: rgba(0, 0, 0, 0.6);
  z-index: 300;
}}
.drums-menu {{
  border-radius: {radius_md}px;
  border-width: 1px;
  z-index: 150;
}}

.hidden {{
  display: none;
}}

.clip-coral {{ background-color: #ff8a5c; }}
.clip-amber {{ background-color: #f5c84c; }}
.clip-teal {{ background-color: #4fd8bd; }}
.clip-blue {{ background-color: #7aa9ff; }}
.clip-violet {{ background-color: #c29bff; }}
.clip-pink {{ background-color: #ff86bd; }}
"#,
        control = space("size-control"),
        radius_sm = space("radius-sm"),
        radius_md = space("radius-md"),
        radius_pill = space("radius-pill"),
        radius_xs = space("radius-xs"),
        space1 = space("space-1"),
        space2 = space("space-2"),
        space6 = space("space-6"),
        ruler = space("size-ruler"),
        lane = space("size-lane"),
        lane_auto = space("size-lane-auto"),
    )
}

/// Colour rules for one theme. When `scope_class` is `Some(class)`, every
/// selector is prefixed with `.class ` so it only applies once that class is
/// toggled on the root (used for the daylight overrides); when `None` the
/// selectors are unscoped, i.e. the default (studio) theme.
fn render_theme_css(colors: &ColorMap, scope_class: Option<&str>) -> String {
    let prefix = match scope_class {
        Some(c) => format!(".{c} "),
        None => String::new(),
    };
    let c = |name: &str| colors.get(name).cloned().unwrap_or_else(|| panic!("missing color {name}"));

    let mut rules: Vec<(String, Vec<(&'static str, String)>)> = Vec::new();
    let mut rule = |selector: &str, decls: Vec<(&'static str, String)>| {
        rules.push((format!("{prefix}{selector}"), decls));
    };

    rule(".app", vec![("background-color", c("bg-000")), ("color", c("ink"))]);
    rule(".panel", vec![("background-color", c("bg-100")), ("border-color", c("line"))]);
    rule(".transport", vec![("background-color", c("bg-000")), ("border-bottom-color", c("line"))]);
    rule(".hairline", vec![("background-color", c("line"))]);

    rule(
        ".btn",
        vec![
            ("background-color", c("bg-200")),
            ("border-color", c("line-control")),
            ("color", c("ink")),
        ],
    );
    rule(".btn:hover", vec![("background-color", c("bg-300"))]);
    rule(
        ".btn.is-play",
        vec![
            ("background-color", c("volt")),
            ("border-color", c("volt")),
            ("color", c("on-volt")),
        ],
    );
    rule(
        ".btn.is-rec",
        vec![
            ("background-color", c("record")),
            ("border-color", c("record")),
            ("color", c("on-record")),
        ],
    );
    rule(
        ".btn.is-solo",
        vec![("background-color", c("hot")), ("border-color", c("hot")), ("color", c("on-hot"))],
    );
    rule(
        ".btn.is-mod",
        vec![("background-color", c("mod")), ("border-color", c("mod")), ("color", c("on-mod"))],
    );
    rule(".btn.is-mute", vec![("background-color", c("ink")), ("border-color", c("ink")), ("color", c("bg-100"))]);

    rule(
        ".readout",
        vec![
            ("background-color", c("bg-000")),
            ("border-color", c("line-control")),
            ("color", c("ink")),
        ],
    );
    rule(".readout .unit", vec![("color", c("ink-muted"))]);
    rule(".label", vec![("color", c("ink-muted"))]);
    rule(".meta", vec![("color", c("ink-muted"))]);

    rule(
        ".pill",
        vec![
            ("background-color", c("mod-soft")),
            ("border-color", c("mod")),
            ("color", c("mod")),
        ],
    );

    rule(".tl-corner", vec![("background-color", c("bg-000")), ("border-color", c("line"))]);
    rule(".tl-heads", vec![("border-color", c("line"))]);
    rule(".tl-head", vec![("border-color", c("line"))]);
    rule(".tl-head-auto", vec![("background-color", c("bg-000"))]);

    rule(".synth-devhead", vec![("background-color", c("bg-000")), ("border-color", c("line"))]);
    rule(".synth-sec", vec![("background-color", c("bg-200"))]);
    rule(".synth-disp", vec![("background-color", c("bg-000"))]);
    rule(".synth-seg", vec![("border-color", c("line-control"))]);
    rule(
        ".synth-seg-btn",
        vec![("background-color", c("bg-200")), ("color", c("ink-muted"))],
    );
    rule(
        ".synth-seg-btn.is-on",
        vec![("background-color", c("ink")), ("color", c("bg-100"))],
    );
    rule(".synth-keys", vec![("background-color", c("bg-000")), ("border-color", c("line"))]);

    let mut out = String::from("/* GENERATED by ui/build.rs from design/tokens.json. Do not edit by hand. */\n\n");
    for (selector, decls) in rules {
        out.push_str(&selector);
        out.push_str(" {\n");
        for (prop, value) in decls {
            out.push_str(&format!("  {prop}: {value};\n"));
        }
        out.push_str("}\n");
    }
    out
}

// --- Rust palette rendering -------------------------------------------

fn render_tokens_rs(studio: &ColorMap, daylight: &ColorMap, scalars: &ScalarMap) -> String {
    let color_fields = [
        ("bg_000", "bg-000"),
        ("bg_100", "bg-100"),
        ("bg_200", "bg-200"),
        ("bg_300", "bg-300"),
        ("line", "line"),
        ("line_control", "line-control"),
        ("ink", "ink"),
        ("ink_muted", "ink-muted"),
        ("ink_faint", "ink-faint"),
        ("volt", "volt"),
        ("on_volt", "on-volt"),
        ("volt_soft", "volt-soft"),
        ("md", "mod"),
        ("on_mod", "on-mod"),
        ("mod_soft", "mod-soft"),
        ("record", "record"),
        ("on_record", "on-record"),
        ("hot", "hot"),
        ("on_hot", "on-hot"),
        ("focus", "focus"),
        ("playhead", "playhead"),
        ("grid_bar", "grid-bar"),
        ("grid_beat", "grid-beat"),
        ("selection", "selection"),
        ("key_white", "key-white"),
        ("key_black", "key-black"),
    ];

    let color_lit = |map: &ColorMap, token: &str| {
        let (r, g, b, a) = parse_color(&map[token]);
        format!("Color::rgba(0x{r:02x}, 0x{g:02x}, 0x{b:02x}, 0x{a:02x})")
    };

    let mut out = String::new();
    out.push_str("// GENERATED by ui/build.rs from design/tokens.json. Do not edit by hand.\n");
    out.push_str("use vizia::prelude::Color;\n\n");
    out.push_str("#[derive(Clone, Copy)]\npub struct Palette {\n");
    for (field, _) in &color_fields {
        out.push_str(&format!("    pub {field}: Color,\n"));
    }
    out.push_str("}\n\n");

    for (theme_name, map) in [("STUDIO", studio), ("DAYLIGHT", daylight)] {
        out.push_str(&format!("pub const {theme_name}: Palette = Palette {{\n"));
        for (field, token) in &color_fields {
            out.push_str(&format!("    {field}: {},\n", color_lit(map, token)));
        }
        out.push_str("};\n\n");
    }

    // Theme-invariant clip colours + on-clip.
    for (name, hex) in [
        ("CLIP_CORAL", "#ff8a5c"),
        ("CLIP_AMBER", "#f5c84c"),
        ("CLIP_TEAL", "#4fd8bd"),
        ("CLIP_BLUE", "#7aa9ff"),
        ("CLIP_VIOLET", "#c29bff"),
        ("CLIP_PINK", "#ff86bd"),
        ("ON_CLIP", "#0e0f11"),
    ] {
        let (r, g, b, a) = parse_color(hex);
        out.push_str(&format!(
            "pub const {name}: Color = Color::rgba(0x{r:02x}, 0x{g:02x}, 0x{b:02x}, 0x{a:02x});\n"
        ));
    }
    out.push('\n');

    let scalar_names = [
        ("SPACE_1", "space-1"),
        ("SPACE_2", "space-2"),
        ("SPACE_3", "space-3"),
        ("SPACE_4", "space-4"),
        ("SPACE_6", "space-6"),
        ("SPACE_8", "space-8"),
        ("RADIUS_XS", "radius-xs"),
        ("RADIUS_SM", "radius-sm"),
        ("RADIUS_MD", "radius-md"),
        ("RADIUS_PILL", "radius-pill"),
        ("SIZE_CONTROL", "size-control"),
        ("SIZE_CLIP", "size-clip"),
        ("SIZE_ROW", "size-row"),
        ("SIZE_KNOB", "size-knob"),
        ("SIZE_LANE", "size-lane"),
        ("SIZE_LANE_AUTO", "size-lane-auto"),
        ("SIZE_RULER", "size-ruler"),
        ("SIZE_TRACK_HEAD", "size-track-head"),
    ];
    for (name, token) in scalar_names {
        out.push_str(&format!("pub const {name}: f32 = {:.1};\n", scalars[token]));
    }

    out
}
