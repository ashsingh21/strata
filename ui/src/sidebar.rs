//! The browser sidebar: search, then the library (Carve - click to add it
//! to the selected MIDI track - and its presets, which load into the
//! selected track's Carve) and files (the drum samples in `assets/drums/`). Every row does
//! something - a preset loads, a sample becomes a drum clip - and search
//! narrows the rows as you type. Ctrl/Cmd+B collapses it to zero width.

use vizia::prelude::*;

use shared::arrangement::{Arrangement, Effect, TrackId};
use shared::synth::{SynthState, PRESETS};

use crate::synth::state::SynthEvent;
use crate::timeline::state::{display_name_from_stem, TimelineEvent};

fn section_head(cx: &mut Context, title: &'static str, count: usize) {
    HStack::new(cx, move |cx| {
        Label::new(cx, title).class("label");
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        Label::new(cx, count.to_string()).class("value").class("count");
    })
    .class("side-head")
    .alignment(Alignment::BottomLeft)
    .padding_left(Pixels(crate::tokens::SPACE_3))
    .padding_right(Pixels(crate::tokens::SPACE_3))
    .padding_bottom(Pixels(crate::tokens::SPACE_1))
    .width(Stretch(1.0))
    .height(Pixels(30.0));
}

/// A clickable row, hidden while the search text doesn't match its name.
fn row<'a>(cx: &'a mut Context, name: String, query: Signal<String>, nested: bool) -> Handle<'a, Button> {
    let needle = name.to_lowercase();
    Button::new(cx, move |cx| Label::new(cx, name.clone()).class("body"))
        .class("side-row")
        .toggle_class("nested", nested)
        .toggle_class("hidden", query.map(move |q| !q.is_empty() && !needle.contains(&q.to_lowercase())))
        .alignment(Alignment::Left)
        .padding_left(Pixels(if nested { 24.0 } else { crate::tokens::SPACE_3 }))
        .width(Stretch(1.0))
        .height(Pixels(24.0))
}

pub fn sidebar(
    cx: &mut Context,
    synth: Signal<SynthState>,
    arrangement: Signal<Arrangement>,
    selected_track: Signal<Option<TrackId>>,
    open: Signal<bool>,
) {
    let query = Signal::new(String::new());
    let samples = crate::timeline::drum_samples();

    VStack::new(cx, move |cx| {
        Textbox::new(cx, query)
            .placeholder("Search")
            .on_edit(move |_cx, text| query.set(text))
            .class("search")
            .width(Stretch(1.0))
            .height(Pixels(crate::tokens::SIZE_CONTROL));

        ScrollView::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                section_head(cx, "Instruments", 1);
                // Adds Carve to the selected MIDI track; lit when that
                // track already plays through Carve.
                let selected_has_carve = Memo::new(move |_| {
                    selected_track
                        .get()
                        .and_then(|id| arrangement.get().track(id).map(|t| t.instrument.is_some()))
                        .unwrap_or(false)
                });
                row(cx, "Carve".to_string(), query, true)
                    .toggle_class("is-on", selected_has_carve)
                    .on_press(|cx| cx.emit(SynthEvent::AddCarveToSelected));

                section_head(cx, "Presets", PRESETS.len());
                for (name, build) in PRESETS {
                    let loaded = synth.map(move |s| s.name == name);
                    row(cx, name.to_string(), query, true)
                        .toggle_class("is-on", loaded)
                        .on_press(move |cx| cx.emit(SynthEvent::LoadPreset(build)));
                }

                section_head(cx, "Drum samples", samples.len());
                for filename in &samples {
                    let source: std::sync::Arc<str> = format!("drums/{filename}").into();
                    let display = display_name_from_stem(filename.strip_suffix(".wav").unwrap_or(filename));
                    row(cx, display, query, true)
                        .on_press(move |cx| cx.emit(TimelineEvent::AddDrumSample(source.clone())));
                }

                // A finished multi-bar groove across new tracks in one
                // click, rather than placing each hit by hand - see
                // `timeline::beat_templates`.
                let templates = crate::timeline::beat_templates::TEMPLATES;
                section_head(cx, "Beat templates", templates.len());
                for (index, template) in templates.iter().enumerate() {
                    row(cx, template.name.to_string(), query, true)
                        .on_press(move |cx| cx.emit(TimelineEvent::AddDrumPattern(index)));
                }

                // Insert effects: works on any track kind (an audio track
                // has effects but no instrument). Lit when the selected
                // track already has one - clicking again is a no-op, same
                // as "Carve" above.
                let selected_has_compressor = Memo::new(move |_| {
                    selected_track
                        .get()
                        .and_then(|id| {
                            arrangement.get().track(id).map(|t| t.effects.iter().any(|e| matches!(e, Effect::Compressor(_))))
                        })
                        .unwrap_or(false)
                });
                section_head(cx, "Audio effects", 1);
                row(cx, "Compressor".to_string(), query, true)
                    .toggle_class("is-on", selected_has_compressor)
                    .on_press(move |cx| {
                        if let Some(track) = selected_track.get() {
                            cx.emit(TimelineEvent::AddCompressorEffect(track));
                        }
                    });
            })
            .width(Stretch(1.0))
            .height(Auto);
        })
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(false)
        .width(Stretch(1.0))
        .height(Stretch(1.0));
    })
    .class("sidebar")
    .toggle_class("hidden", open.map(|o| !*o))
    .gap(Pixels(crate::tokens::SPACE_2))
    .padding(Pixels(crate::tokens::SPACE_2))
    .width(Pixels(200.0))
    .height(Stretch(1.0));
}

/// The status bar: audio settings on the left; on the right, the control
/// last touched with its live value, then whether the project is saved.
pub fn status_bar(
    cx: &mut Context,
    sample_rate: u32,
    block_frames: Signal<u32>,
    touched: Signal<Option<(String, Memo<String>)>>,
    save_status: Memo<String>,
) {
    HStack::new(cx, move |cx| {
        let audio = block_frames.map(move |&frames| {
            let khz = sample_rate as f32 / 1000.0;
            if frames == 0 {
                format!("{khz:.0} kHz")
            } else {
                let ms = frames as f32 / sample_rate as f32 * 1000.0;
                format!("{khz:.0} kHz \u{b7} {frames} samples \u{b7} {ms:.1} ms")
            }
        });
        Label::new(cx, audio).class("value");
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        let touched_text = Memo::new(move |_| match touched.get() {
            Some((name, value)) => format!("{name} {}", value.get()),
            None => String::new(),
        });
        Label::new(cx, touched_text).class("value");
        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(12.0));
        Label::new(cx, save_status).class("value");
    })
    .class("statusbar")
    .gap(Pixels(crate::tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(24.0));
}
