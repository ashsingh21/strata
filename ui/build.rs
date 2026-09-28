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

    let themes = resolve_colors(&tokens);
    let scalars = resolve_scalars(&tokens);
    let fonts = resolve_fonts(&tokens);
    let shadows = resolve_shadow_pop(&tokens);

    // The first theme (Studio) is the default: unscoped rules. Every other
    // theme's rules are scoped under `.theme-<id>` on the root, all in one
    // sheet.
    let (_, studio) = &themes[0];
    let styles_dir = manifest_dir.join("styles");
    fs::create_dir_all(&styles_dir).unwrap();
    fs::write(styles_dir.join("base.css"), render_base_css(&scalars, &fonts, studio)).unwrap();
    fs::write(styles_dir.join("studio.css"), render_theme_css(studio, None, &shadows[0])).unwrap();
    let mut others = String::new();
    for ((id, colors), shadow) in themes.iter().zip(&shadows).skip(1) {
        others.push_str(&render_theme_css(colors, Some(&format!("theme-{id}")), shadow));
        others.push('\n');
    }
    fs::write(styles_dir.join("themes.css"), others).unwrap();
    // Replaced by themes.css.
    let _ = fs::remove_file(styles_dir.join("daylight.css"));

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("tokens.rs"), render_tokens_rs(&themes, &scalars)).unwrap();
}

/// name -> "#rrggbb" per theme.
type ColorMap = HashMap<String, String>;

/// The themes, in `color.themes` order: `(id, base)`. A theme with a
/// `base` takes every colour it doesn't list from that theme - Midnight
/// is Studio with blacker grounds, Paper is Daylight gone warm.
fn theme_list(tokens: &Value) -> Vec<(String, Option<String>)> {
    tokens["color"]["themes"]
        .as_array()
        .expect("color.themes array")
        .iter()
        .map(|t| (t["id"].as_str().unwrap().to_string(), t["base"].as_str().map(str::to_string)))
        .collect()
}

/// A token's raw value in `theme`: its own entry, else its base's (and so
/// on up), else the one value every theme shares.
fn value_in<'a>(value: &'a Value, theme: &str, themes: &[(String, Option<String>)]) -> &'a str {
    match value {
        Value::String(v) => v,
        Value::Object(map) => {
            let mut id = theme.to_string();
            loop {
                if let Some(v) = map.get(&id).and_then(|v| v.as_str()) {
                    return v;
                }
                let base = themes.iter().find(|(t, _)| *t == id).and_then(|(_, b)| b.clone());
                id = base.unwrap_or_else(|| panic!("no value for {theme} in {value:?}"));
            }
        }
        other => panic!("unexpected token value {other:?}"),
    }
}

fn resolve_colors(tokens: &Value) -> Vec<(String, ColorMap)> {
    let entries = tokens["color"]["tokens"].as_array().expect("color.tokens array");
    let themes = theme_list(tokens);
    themes.iter().map(|(id, _)| (id.clone(), resolve_theme_colors(entries, id, &themes))).collect()
}

/// One theme's colours. A value may be a `{other-token}` alias (Strata 2
/// aliases per theme, e.g. a `clip-*-line` is its clip colour in Studio
/// but its own darker hex in Daylight).
fn resolve_theme_colors(entries: &[Value], theme: &str, themes: &[(String, Option<String>)]) -> ColorMap {
    let raw: Vec<(String, String)> = entries
        .iter()
        .map(|e| (e["name"].as_str().unwrap().to_string(), value_in(&e["value"], theme, themes).to_string()))
        .collect();
    {
        let alias = |v: &str| v.strip_prefix('{').and_then(|v| v.strip_suffix('}')).map(str::to_string);
        let mut out = ColorMap::new();
        // Aliases may point at tokens declared later in the file, so
        // resolve in fixed-point passes rather than assuming order.
        for _ in 0..8 {
            for entry in &raw {
                let value = &entry.1;
                match alias(value) {
                    None => {
                        out.insert(entry.0.clone(), value.clone());
                    }
                    Some(target) => {
                        if let Some(resolved) = out.get(&target).cloned() {
                            out.insert(entry.0.clone(), resolved);
                        }
                    }
                }
            }
        }
        let missing: Vec<_> = raw.iter().filter(|e| !out.contains_key(&e.0)).map(|e| e.0.clone()).collect();
        assert!(missing.is_empty(), "unresolved color aliases in {theme}: {missing:?}");
        out
    }
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

struct Fonts {
    sans: String,
}

/// The two font stacks from `type.families` - never actually read before
/// this, so nothing anywhere set a font-family and every label rendered
/// in Skia's bare fallback instead of the spec's chosen stack.
fn resolve_fonts(tokens: &Value) -> Fonts {
    let families = &tokens["type"]["families"];
    Fonts {
        sans: families["sans"].as_str().expect("type.families.sans").to_string(),
    }
}

/// `shadow-pop`'s two theme values - "Popovers and menus only. Panels are
/// flat," per its own `usage` note - but until now nothing in the
/// generated CSS ever referenced it, so no popover actually got a shadow.
fn resolve_shadow_pop(tokens: &Value) -> Vec<String> {
    let entries = tokens["shadow"]["tokens"].as_array().expect("shadow.tokens array");
    let entry = entries.iter().find(|e| e["name"] == "shadow-pop").expect("shadow-pop token");
    let themes = theme_list(tokens);
    themes.iter().map(|(id, _)| value_in(&entry["value"], id, &themes).to_string()).collect()
}

/// Parses `#rrggbb`, `#rrggbbaa` or `rgba(r, g, b, a)` (`a` in 0..1) into
/// 8-bit RGBA. Opaque hex colors get alpha 255.
fn parse_color(value: &str) -> (u8, u8, u8, u8) {
    if let Some(hex) = value.strip_prefix('#') {
        assert!(hex.len() == 6 || hex.len() == 8, "expected #rrggbb or #rrggbbaa, got {value}");
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        let a = if hex.len() == 8 { byte(6) } else { 255 };
        (byte(0), byte(2), byte(4), a)
    } else if let Some(inner) = value.strip_prefix("rgba(").and_then(|s| s.strip_suffix(')')) {
        let parts: Vec<f32> = inner.split(',').map(|p| p.trim().parse().unwrap()).collect();
        assert_eq!(parts.len(), 4, "expected rgba(r,g,b,a), got {value}");
        (parts[0] as u8, parts[1] as u8, parts[2] as u8, (parts[3] * 255.0).round() as u8)
    } else {
        panic!("unrecognized color format: {value}");
    }
}

/// A token value as CSS Vizia is sure to parse: opaque colours stay hex,
/// translucent ones become `rgba()` rather than 8-digit hex.
fn css_color(value: &str) -> String {
    let (r, g, b, a) = parse_color(value);
    if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("rgba({r}, {g}, {b}, {:.3})", a as f32 / 255.0)
    }
}

// --- CSS rendering ---------------------------------------------------

/// Structural rules shared by both themes: sizes, radii and the type
/// scale, plus the clip colours (identical in both themes) as fill classes.
fn render_base_css(scalars: &ScalarMap, fonts: &Fonts, colors: &ColorMap) -> String {
    let space = |n: &str| scalars[n];
    let sans = &fonts.sans;
    let clip = |name: &str| css_color(&colors[name]);
    format!(
        r#"/* GENERATED by ui/build.rs from design/tokens.json. Do not edit by hand. */

.app {{
  font-family: {sans};
  font-size: 12px;
}}

/* Type scale: IBM Plex Sans only, in 11 / 12 / 13 / 15px. Values use
   Plex too (its figures are tabular), so `.meta`/`.mono` - the classes
   the views already put on values - are Plex `value` style now. */
.heading {{
  font-size: 15px;
  font-weight: 600;
}}
/* The Sound match score: the one big number on screen. */
.match-score {{
  font-size: 28px;
  font-weight: 600;
}}
.title {{
  font-size: 13px;
  font-weight: 600;
}}
.control {{
  font-size: 12px;
  font-weight: 500;
}}
.body {{
  font-size: 12px;
  font-weight: 400;
}}
.label {{
  font-size: 11px;
  font-weight: 500;
}}
.label-lg {{
  font-size: 12px;
  font-weight: 600;
}}
.meta {{
  font-size: 11px;
  font-weight: 400;
}}
.mono {{
  font-size: 11px;
  font-weight: 400;
}}
.value {{
  font-size: 11px;
  font-weight: 400;
}}

.btn {{
  height: {control}px;
  min-width: {control}px;
  border-radius: {radius_sm}px;
  border-width: 1px;
  font-size: 12px;
  font-weight: 500;
  padding: 0px 8px;
}}
.btn.sm {{
  height: 18px;
  min-width: 18px;
  font-size: 11px;
  padding: 0px 4px;
}}
.btn.quiet {{
  border-width: 0px;
  padding: 0px 4px;
}}
.readout {{
  height: {control}px;
  border-radius: {radius_sm}px;
  border-width: 1px;
  font-size: 13px;
  font-weight: 500;
  padding: 0px 8px;
}}
.readout .unit {{
  font-size: 11px;
  font-weight: 400;
}}
.readout-big {{
  font-size: 18px;
  font-weight: 600;
}}
.tgroup {{
  border-width: 1px;
  border-radius: {radius_sm}px;
  height: 30px;
  padding: 1px;
  gap: 1px;
}}
.tbtn {{
  width: 34px;
  height: 28px;
  border-width: 0px;
  border-radius: 2px;
  padding: 0px;
}}
.position {{
  height: 30px;
}}
.bar {{
  border-radius: 2px;
}}
.bar-fill {{
  height: 1s;
  border-radius: 2px;
}}
.statusbar {{
  height: 24px;
  padding: 0px {space3}px;
}}
.sidebar {{
  width: 200px;
}}
.side-row {{
  height: 24px;
  padding: 0px {space3}px;
  border-radius: 0px;
}}
.side-row.nested {{
  padding-left: 24px;
}}
.side-head {{
  height: 24px;
  padding: 8px {space3}px 0px {space3}px;
}}
.search {{
  height: {control}px;
  border-width: 1px;
  border-radius: {radius_sm}px;
  padding: 0px {space2}px;
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
  /* A fixed size, not {{toolbar}}: transport.rs's own HEADER_HEIGHT
     constant is the single source of truth here, deliberately bigger
     than the size-toolbar token every other device/editor header uses -
     see that constant's doc comment for why. */
  height: 48px;
  border-bottom-width: 1px;
}}
.swatch {{
  width: {space2}px;
  height: {space2}px;
  border-radius: {radius_xs}px;
}}

.tl-corner {{
  height: {ruler}px;
  padding: 0px {space2}px;
}}
.fx-pip {{
  width: 6px;
  height: 6px;
  border-radius: 3px;
  border-width: 1px;
}}
.readout.snap {{
  height: 20px;
  font-size: 12px;
  padding: 0px 6px;
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
}}

/* Devices: one flat panel, sections split by hairlines, never boxed. */
.lower-panel {{
  padding: {space2}px;
}}
.device {{
  border-radius: {radius_md}px;
  border-width: 1px;
}}
.fx-node {{
  border-radius: {radius_md}px;
  border-width: 1px;
}}
.fx-node.is-sel {{
  border-width: 2px;
}}
.fx-node.is-off {{
  opacity: 0.5;
}}
.synth-devhead {{
  padding: 0px {space3}px;
}}
.synth-sec {{
  padding: {space3}px;
}}
.synth-disp {{
  border-radius: {radius_sm}px;
}}
.synth-seg {{
  border-width: 1px;
  border-radius: {radius_sm}px;
  height: {control}px;
}}
.synth-seg-btn {{
  height: 20px;
  min-width: 22px;
  font-size: 11px;
  font-weight: 500;
  padding: 0px 6px;
  border-width: 0px;
}}
.synth-keys {{
  border-radius: {radius_sm}px;
  border-width: 1px;
}}
.knob-col {{
  border-radius: {radius_sm}px;
}}
.knob-col.drop-target {{
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
.context-menu-backdrop {{
  z-index: 170;
}}
.is-lesson-target {{
  border-width: 2px;
}}
/* Tooltips: Vizia's own tooltip styling (and the fade it animates) lives
   in the default theme this app ignores, so both are defined here. */
@keyframes tooltip_fade {{
  0% {{ opacity: 0; }}
  100% {{ opacity: 1; }}
}}
/* `.app tooltip`, not `tooltip`: Vizia's own layout sheet styles bare
   `tooltip` (a 160px max width, no padding) and won the tie. */
.app tooltip {{
  size: auto;
  max-width: 440px;
  /* One value per side: Vizia's `padding` shorthand takes a single
     length, and "6px 10px" was dropped. */
  padding-left: 10px;
  padding-right: 10px;
  padding-top: 6px;
  padding-bottom: 6px;
  border-width: 1px;
  font-size: 12px;
  /* Above everything: a rail tooltip opens over the panel beside it,
     which is drawn later and otherwise covered it. */
  z-index: 200;
}}
/* Long tips wrap inside the box instead of running past it. */
.app tooltip label {{
  max-width: 420px;
}}
/* Vizia's own popups (Dropdown): above everything drawn after them. Their
   look comes from the .context-menu inside. */
.app popup {{
  z-index: 180;
}}
.context-menu {{
  border-radius: {radius_md}px;
  border-width: 1px;
  z-index: 180;
}}
.menu-item {{
  border-radius: {radius_xs}px;
  padding: 0px {space3}px;
}}
.menu-sep {{
  /* Horizontal inset now comes from .context-menu's own padding, so this
     spans the same width as the items above/below it, edges aligned. */
  margin: {space1}px 0px;
}}

/* Vizia's own layout for ScrollView's inner content lives in its default
   theme, which main.rs opts out of - without this the content has no size.
   Vertical-only: width follows the container so wrapped text wraps to it. */
scrollview {{
  overflow: hidden;
}}
scrollview > scroll-content {{
  width: 1s;
  height: auto;
  min-height: 100%;
}}
.guide-h {{
  font-size: 15px;
  font-weight: 600;
}}
.guide-term {{
  font-size: 12px;
  font-weight: 600;
}}
.guide-body {{
  font-size: 12px;
}}
.guide-tip {{
  font-size: 11px;
}}

/* The sidebar (design/sidebar): rail, panel, results, preview dock.
   Square like the rest of Strata; only the type chips and the round
   preview buttons are pills. */
.brw-rail-btn {{
  border-width: 0px;
  padding: 0px;
}}
.brw-search {{
  border-width: 1px;
}}
.brw-search-field {{
  border-width: 0px;
  padding: 0px;
  font-size: 12px;
}}
.brw-kbd {{
  font-size: 11px;
  border-width: 1px;
  padding: 0px 4px;
}}
.brw-chip {{
  padding: 0px 8px;
  border-width: 1px;
  corner-radius: 11px;
  font-size: 11px;
  font-weight: 500;
}}
.brw-coll {{
}}
.brw-swatch {{
}}
.brw-row {{
}}
.brw-fav {{
  border-width: 0px;
  padding: 0px;
  opacity: 0;
}}
.brw-fav.is-fav {{
  opacity: 1;
}}
.brw-row:hover .brw-fav {{
  opacity: 1;
}}
.brw-mini {{
  border-width: 0px;
  padding: 0px;
}}
.brw-pv {{
  corner-radius: 9px;
  border-width: 0px;
  padding: 0px;
  opacity: 0;
}}
.brw-row:hover .brw-pv {{
  opacity: 1;
}}
.brw-row.is-sel .brw-pv {{
  opacity: 1;
}}
.brw-pv.is-on {{
  opacity: 1;
}}
.brw-fit {{
  padding: 0px 4px;
}}
.brw-fit-text {{
  font-size: 11px;
}}
.hidden {{
  display: none;
}}

.clip-coral {{ background-color: {coral}; }}
.clip-amber {{ background-color: {amber}; }}
.clip-teal {{ background-color: {teal}; }}
.clip-blue {{ background-color: {blue}; }}
.clip-violet {{ background-color: {violet}; }}
.clip-pink {{ background-color: {pink}; }}
"#,
        control = space("size-control"),
        radius_sm = space("radius-sm"),
        radius_md = space("radius-md"),
        radius_pill = space("radius-pill"),
        radius_xs = space("radius-xs"),
        space1 = space("space-1"),
        space2 = space("space-2"),
        space3 = space("space-3"),
        ruler = space("size-ruler"),
        lane = space("size-lane"),
        lane_auto = space("size-lane-auto"),
        coral = clip("clip-coral"),
        amber = clip("clip-amber"),
        teal = clip("clip-teal"),
        blue = clip("clip-blue"),
        violet = clip("clip-violet"),
        pink = clip("clip-pink"),
    )
}

/// Colour rules for one theme. When `scope_class` is `Some(class)`, every
/// selector is prefixed with `.class ` so it only applies once that class is
/// toggled on the root (used for the daylight overrides); when `None` the
/// selectors are unscoped, i.e. the default (studio) theme.
fn render_theme_css(colors: &ColorMap, scope_class: Option<&str>, shadow_pop: &str) -> String {
    let prefix = match scope_class {
        Some(c) => format!(".{c} "),
        None => String::new(),
    };
    let c = |name: &str| css_color(colors.get(name).unwrap_or_else(|| panic!("missing color {name}")));

    let mut rules: Vec<(String, Vec<(&'static str, String)>)> = Vec::new();
    let mut rule = |selector: &str, decls: Vec<(&'static str, String)>| {
        rules.push((format!("{prefix}{selector}"), decls));
    };

    rule(".app", vec![("background-color", c("bg-000")), ("color", c("ink"))]);
    // The theme class sits *on* the `.app` root, so the descendant rule
    // above (".theme-daylight .app") never matches it: the root needs a
    // compound selector too, or Daylight keeps Studio's ground and text.
    let root_rule = scope_class.map(|class| {
        (format!(".app.{class}"), vec![("background-color", c("bg-000")), ("color", c("ink"))])
    });
    rule(".panel", vec![("background-color", c("bg-100")), ("border-color", c("line"))]);
    rule(".transport", vec![("background-color", c("bg-000")), ("border-bottom-color", c("line"))]);
    rule(".hairline", vec![("background-color", c("line"))]);

    // shadow-pop: "Popovers and menus only. Panels are flat."
    for popover in [".synth-help-panel", ".interval-overlay", ".piano-roll-panel", ".context-menu"] {
        rule(popover, vec![("shadow", shadow_pop.to_string())]);
    }

    // A context menu reads as a distinct floating surface, not just a
    // shadowed copy of the panel it's sitting on: one step lighter than
    // `.panel`'s bg-100, with a more visible edge (line-control, the same
    // border every button and readout uses) rather than the faint `line`
    // hairline panels use between themselves.
    rule(".context-menu", vec![("background-color", c("bg-200")), ("border-color", c("line-control"))]);
    rule("tooltip", vec![("background-color", c("bg-300")), ("border-color", c("line-control")), ("color", c("ink"))]);

    rule(
        ".btn",
        vec![
            ("background-color", c("bg-200")),
            ("border-color", c("line-control")),
            ("color", c("ink")),
        ],
    );
    rule(".btn:hover", vec![("background-color", c("bg-300"))]);
    // TrackHeaderFx's pips: hollow (border only) when an effect is
    // bypassed, filled solid when it's on.
    rule(".fx-pip", vec![("background-color", "transparent".to_string()), ("border-color", c("ink-faint"))]);
    rule(".fx-pip.is-on", vec![("background-color", c("ink")), ("border-color", c("ink"))]);
    rule(".menu-item:hover", vec![("background-color", c("bg-300"))]);
    // The current choice in a menu (time signature, input device).
    rule(".menu-item.is-on", vec![("background-color", c("active-soft"))]);
    rule(".menu-item.is-on .body", vec![("color", c("active"))]);
    // `line` (the faint hairline .panel uses against its own bg-100) is
    // nearly invisible on the menu's bg-200: line-control (ink-faint) is
    // the same edge every button and readout already uses, and actually
    // reads as a deliberate divider instead of a stray gap.
    rule(".menu-sep", vec![("background-color", c("line-control"))]);
    rule(".btn.quiet", vec![("background-color", "transparent".to_string()), ("color", c("ink-muted"))]);
    rule(".btn.quiet:hover", vec![("background-color", c("bg-300")), ("color", c("ink"))]);
    // State buttons fill with their state's colour...
    rule(
        ".btn.is-play",
        vec![("background-color", c("signal")), ("border-color", c("signal")), ("color", c("on-signal"))],
    );
    rule(
        ".btn.is-rec",
        vec![("background-color", c("record")), ("border-color", c("record")), ("color", c("on-record"))],
    );
    rule(
        ".btn.is-solo",
        vec![("background-color", c("warn")), ("border-color", c("warn")), ("color", c("on-warn"))],
    );
    // ...Loop uses mod-soft with a mod edge and icon, matching the loop
    // range drawn in mod...
    rule(
        ".btn.is-mod",
        vec![("background-color", c("mod-soft")), ("border-color", c("mod")), ("color", c("mod"))],
    );
    // ...and neutral toggles (Mute, Sync, Click, Enabled, an open board...)
    // use the `active` accent: a grey step was easy to miss.
    for on in [".btn.is-mute", ".btn.is-on"] {
        rule(on, vec![("background-color", c("active-soft")), ("border-color", c("active")), ("color", c("active"))]);
    }

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
    rule(".label-lg", vec![("color", c("ink"))]);
    rule(".heading", vec![("color", c("ink"))]);
    rule(".title", vec![("color", c("ink"))]);
    rule(".control", vec![("color", c("ink"))]);
    rule(".body", vec![("color", c("ink"))]);
    rule(".meta", vec![("color", c("ink-muted"))]);
    rule(".value", vec![("color", c("ink-muted"))]);
    rule(".guide-h", vec![("color", c("ink"))]);
    rule(".guide-term", vec![("color", c("ink"))]);
    rule(".guide-body", vec![("color", c("ink"))]);
    rule(".guide-tip", vec![("color", c("ink-muted"))]);

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
    rule(".tl-head", vec![("background-color", c("bg-100")), ("border-color", c("line"))]);
    rule(".tl-head.is-selected", vec![("background-color", c("active-soft"))]);
    rule(".tl-head-auto", vec![("background-color", c("bg-000"))]);
    rule(".tl-head-auto.is-orphaned .control", vec![("color", c("ink-faint"))]);

    rule(".lower-panel", vec![("background-color", c("bg-000"))]);
    rule(".lower-scroll", vec![("background-color", c("bg-000"))]);
    rule(".tgroup", vec![("background-color", c("bg-200")), ("border-color", c("line-control"))]);
    rule(".tbtn", vec![("background-color", "transparent".to_string())]);
    rule(".tbtn:hover", vec![("background-color", c("bg-300"))]);
    rule(".readout-big", vec![("color", c("ink"))]);
    rule(".bar", vec![("background-color", c("bg-300"))]);
    rule(".bar-fill", vec![("background-color", c("ink-muted"))]);
    rule(".bar-fill.level", vec![("background-color", c("signal"))]);
    rule(".bar-fill.level.hot", vec![("background-color", c("warn"))]);
    rule(".statusbar", vec![("background-color", c("bg-000"))]);
    rule(".sidebar", vec![("background-color", c("bg-100"))]);
    rule(".side-row", vec![("background-color", "transparent".to_string()), ("color", c("ink"))]);
    rule(".side-row:hover", vec![("background-color", c("bg-300"))]);
    // A colored highlight, not another neutral bg-400 toggle: the sidebar
    // is navigation (which instrument/preset is active), not a device
    // control, so it reads better picking out a track-independent accent
    // (mod, already "this is the active/engaged thing" everywhere else -
    // Loop, LFO routing) than staying fully neutral.
    rule(".side-row.is-on", vec![("background-color", c("mod-soft"))]);
    rule(".side-row.is-on .body", vec![("color", c("mod"))]);
    rule(".side-row.nested", vec![("color", c("ink-muted"))]);
    rule(".side-head", vec![("color", c("ink-muted"))]);
    rule(".count", vec![("color", c("ink-faint"))]);
    rule(".search", vec![("background-color", c("bg-000")), ("border-color", c("line-control")), ("color", c("ink"))]);
    // A textbox that just became live/editable (e.g. the BPM readout's
    // right-click-to-type field) needs a stronger cue than `.search`'s
    // own quiet border - `focus` is the token's own purpose ("keyboard
    // focus ring").
    rule(".search.editing", vec![("background-color", c("bg-300")), ("border-color", c("focus"))]);
    rule(".device", vec![("background-color", c("bg-100")), ("border-color", c("line"))]);
    // FxBoard: the canvas reads as a recessed work surface (same bg-000
    // "sunken well" idiom `.synth-disp`/`.lower-panel`/`.tl-corner` already
    // use, no border - flat, like `.lower-panel`) so effect nodes - one
    // step lighter, like `.context-menu` sits a step above `.panel` -
    // visibly float on top of it instead of every surface in the board
    // being the same flat bg-100.
    rule(".fx-canvas", vec![("background-color", c("bg-000"))]);
    rule(".fx-node", vec![("background-color", c("bg-raised")), ("border-color", c("line-control"))]);
    // Draggable effect nodes (not the fixed Source/Out pills) and output
    // ports brighten on hover, so what can be grabbed reads as grabbable.
    // Before `.is-sel` so a selected node keeps its focus border.
    rule(".fx-node.fx-effect:hover", vec![("border-color", c("ink-muted"))]);
    rule(".fx-pip.fx-port:hover", vec![("background-color", c("focus")), ("border-color", c("focus"))]);
    // Selection needs to read on the node itself, not just its cables -
    // `focus` is the token's own purpose ("keyboard focus ring").
    rule(".fx-node.is-sel", vec![("background-color", c("active-soft")), ("border-color", c("active"))]);
    rule(".synth-disp", vec![("background-color", c("bg-000"))]);
    rule(".synth-seg", vec![("background-color", c("bg-200")), ("border-color", c("line-control"))]);
    rule(
        ".synth-seg-btn",
        vec![("background-color", "transparent".to_string()), ("color", c("ink-muted"))],
    );
    rule(".synth-seg-btn:hover", vec![("color", c("ink"))]);
    rule(".synth-seg-btn.is-on", vec![("background-color", c("active-soft")), ("color", c("active"))]);
    // The timeline's Add group: actions, not a choice, so a hover fill
    // shows which one you're about to press.
    rule(".add-track-btn:hover", vec![("background-color", c("bg-300")), ("color", c("ink"))]);
    // Dividers in the group's own border colour.
    rule(".add-track-divider", vec![("background-color", c("line-control"))]);
    rule(".synth-keys", vec![("background-color", c("bg-000")), ("border-color", c("line"))]);
    // An LFO pill being dragged: every knob it can land on lights up in mod.
    rule(".knob-col.drop-target", vec![("background-color", c("mod-soft")), ("border-color", c("mod"))]);
    // An automated knob: its label and value in the modulation accent -
    // "something else is moving this" - and it no longer takes drags.
    rule(".knob-col.is-automated .label", vec![("color", c("mod"))]);
    rule(".knob-col.is-automated .label-lg", vec![("color", c("mod"))]);
    rule(".knob-col.is-automated .value", vec![("color", c("mod"))]);
    rule(".meta.is-automated", vec![("color", c("mod"))]);

    // Lessons: the control a step points at, and the bar above the timeline.
    // Last, so a glow wins over any control's own same-specificity colours.
    rule(".is-lesson-target", vec![("background-color", c("signal-soft")), ("border-color", c("signal"))]);
    rule(".side-row.is-lesson-target .body", vec![("color", c("signal"))]);
    rule(".lesson-bar", vec![("background-color", c("bg-raised"))]);
    rule(".lesson-dots", vec![("color", c("signal"))]);
    rule(".side-row.side-next .body", vec![("color", c("signal"))]);
    // The sidebar. ON / selected states use the app's `active` accent
    // (design/tokens.json retired bg-400 for them); hover is bg-300.
    rule(".brw-rail", vec![("background-color", c("bg-000")), ("border-color", c("line"))]);
    rule(".brw-rail-btn", vec![("background-color", "transparent".to_string())]);
    rule(".brw-rail-btn:hover", vec![("background-color", c("bg-300"))]);
    rule(".brw-rail-btn.is-on", vec![("background-color", c("active-soft"))]);
    rule(".brw-panel", vec![("background-color", c("bg-100"))]);
    rule(".brw-search", vec![("background-color", c("bg-000")), ("border-color", c("line-control"))]);
    rule(".brw-search-field", vec![("background-color", "transparent".to_string()), ("color", c("ink"))]);
    rule(".brw-kbd", vec![("color", c("ink-muted")), ("border-color", c("line"))]);
    rule(".brw-chip", vec![("background-color", "transparent".to_string()), ("border-color", c("line-control")), ("color", c("ink-muted"))]);
    rule(".brw-chip:hover", vec![("background-color", c("bg-300")), ("color", c("ink"))]);
    rule(".brw-chip.is-on", vec![("background-color", c("active-soft")), ("border-color", c("active")), ("color", c("active"))]);
    rule(".brw-coll", vec![("background-color", "transparent".to_string())]);
    rule(".brw-coll:hover", vec![("background-color", c("bg-300"))]);
    rule(".brw-coll.is-on", vec![("background-color", c("active-soft"))]);
    rule(".brw-row", vec![("background-color", "transparent".to_string())]);
    rule(".brw-row:hover", vec![("background-color", c("bg-300"))]);
    rule(".brw-row.is-sel", vec![("background-color", c("active-soft"))]);
    rule(".brw-next", vec![("color", c("signal"))]);
    rule(".brw-fav", vec![("background-color", "transparent".to_string())]);
    rule(".brw-mini", vec![("background-color", "transparent".to_string())]);
    rule(".brw-mini:hover", vec![("background-color", c("bg-400"))]);
    rule(".brw-pv", vec![("background-color", c("bg-200"))]);
    rule(".brw-pv:hover", vec![("background-color", c("bg-400"))]);
    rule(".brw-pv.is-on", vec![("background-color", c("signal"))]);
    rule(".brw-fit", vec![("background-color", c("signal-soft"))]);
    rule(".brw-fit-text", vec![("color", c("ink"))]);
    rule(".brw-fit.no", vec![("background-color", "transparent".to_string())]);
    rule(".brw-fit.no .brw-fit-text", vec![("color", c("ink-muted"))]);
    rule(".brw-strike", vec![("background-color", c("ink-faint"))]);
    rule(".brw-dock", vec![("background-color", c("bg-100")), ("border-color", c("line"))]);
    rule(".brw-edge", vec![("background-color", "transparent".to_string())]);
    rule(".brw-edge:hover", vec![("background-color", c("line"))]);
    rule(".match-score.is-matched", vec![("color", c("signal"))]);

    let mut out = String::from("/* GENERATED by ui/build.rs from design/tokens.json. Do not edit by hand. */\n\n");
    for (selector, decls) in rules.into_iter().chain(root_rule) {
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

fn render_tokens_rs(themes: &[(String, ColorMap)], scalars: &ScalarMap) -> String {
    let studio = &themes[0].1;
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
        ("bg_400", "bg-400"),
        ("signal", "signal"),
        ("on_signal", "on-signal"),
        ("signal_soft", "signal-soft"),
        ("md", "mod"),
        ("on_mod", "on-mod"),
        ("mod_soft", "mod-soft"),
        ("record", "record"),
        ("on_record", "on-record"),
        ("warn", "warn"),
        ("on_warn", "on-warn"),
        ("focus", "focus"),
        ("playhead", "playhead"),
        ("grid_bar", "grid-bar"),
        ("grid_beat", "grid-beat"),
        ("selection", "selection"),
        ("key_white", "key-white"),
        ("key_black", "key-black"),
        ("clip_edge", "clip-edge"),
        ("clip_coral_line", "clip-coral-line"),
        ("clip_amber_line", "clip-amber-line"),
        ("clip_teal_line", "clip-teal-line"),
        ("clip_blue_line", "clip-blue-line"),
        ("clip_violet_line", "clip-violet-line"),
        ("clip_pink_line", "clip-pink-line"),
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

    for (id, map) in themes {
        let theme_name = id.to_uppercase().replace('-', "_");
        out.push_str(&format!("pub const {theme_name}: Palette = Palette {{\n"));
        for (field, token) in &color_fields {
            out.push_str(&format!("    {field}: {},\n", color_lit(map, token)));
        }
        out.push_str("};\n\n");
    }

    // Theme-invariant clip colours + on-clip, straight from the tokens.
    for (name, token) in [
        ("CLIP_CORAL", "clip-coral"),
        ("CLIP_AMBER", "clip-amber"),
        ("CLIP_TEAL", "clip-teal"),
        ("CLIP_BLUE", "clip-blue"),
        ("CLIP_VIOLET", "clip-violet"),
        ("CLIP_PINK", "clip-pink"),
        ("ON_CLIP", "on-clip"),
    ] {
        out.push_str(&format!("pub const {name}: Color = {};\n", color_lit(studio, token)));
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
        ("SIZE_KNOB_SM", "size-knob-sm"),
        ("SIZE_KNOB", "size-knob"),
        ("SIZE_KNOB_LG", "size-knob-lg"),
        ("SIZE_TOOLBAR", "size-toolbar"),
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
