//! The browser sidebar: search, then the library (Carve - click to add it
//! to the selected MIDI track - and its presets, which load into the
//! selected track's Carve) and files (the drum samples in `assets/drums/`). Every row does
//! something - a preset loads, a sample becomes a drum clip - and search
//! narrows the rows as you type. Ctrl/Cmd+B collapses it to zero width.

use vizia::prelude::*;

use shared::arrangement::{Arrangement, Effect, Instrument, TrackId};
use shared::synth::{SynthState, PRESETS};

use crate::synth::state::SynthEvent;
use crate::timeline::state::TimelineEvent;

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
    Button::new(cx, move |cx| {
        // Long sample names end in "…" instead of being cut off mid-word
        // at the sidebar's edge.
        Label::new(cx, name.clone())
            .class("body")
            .text_wrap(false)
            .text_overflow(TextOverflow::Ellipsis)
            .alignment(Alignment::Left)
            .width(Stretch(1.0))
    })
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
    let sample_categories = crate::timeline::drum_sample_categories();

    VStack::new(cx, move |cx| {
        Textbox::new(cx, query)
            .placeholder("Search")
            .on_edit(move |_cx, text| query.set(text))
            .class("search")
            .width(Stretch(1.0))
            .height(Pixels(crate::tokens::SIZE_CONTROL));

        ScrollView::new(cx, move |cx| {
            VStack::new(cx, move |cx| {
                section_head(cx, "Instruments", 2);
                // Puts the instrument on the selected MIDI track; lit when
                // that track already plays through it.
                for instrument in [Instrument::Carve, Instrument::Drums] {
                    let selected_has = Memo::new(move |_| {
                        selected_track
                            .get()
                            .and_then(|id| arrangement.get().track(id).map(|t| t.instrument == Some(instrument)))
                            .unwrap_or(false)
                    });
                    row(cx, instrument.name().to_string(), query, true)
                        .toggle_class("is-on", selected_has)
                        .on_press(move |cx| cx.emit(SynthEvent::AddInstrumentToSelected(instrument)));
                }

                section_head(cx, "Presets", PRESETS.len());
                for (name, build) in PRESETS {
                    let loaded = synth.map(move |s| s.name == name);
                    row(cx, name.to_string(), query, true)
                        .toggle_class("is-on", loaded)
                        .on_press(move |cx| cx.emit(SynthEvent::LoadPreset(build)));
                }

                for category in &sample_categories {
                    section_head(cx, category.label, category.files.len());
                    // Each row's (display name, import source) computed
                    // up front and moved in as owned data - the ScrollView
                    // closure has to be 'static, so it can't hold a
                    // borrow of `category`/`sample_categories` itself.
                    let entries: Vec<(String, std::sync::Arc<str>)> = category
                        .files
                        .iter()
                        .map(|filename| {
                            (crate::timeline::sample_display_name(category, filename), format!("drums/{filename}").into())
                        })
                        .collect();
                    // Its own bounded, bordered scroll box, not just more
                    // rows in the sidebar's own scroll - so a long list
                    // (a dropped-in pack) browses in place instead of
                    // pushing every section below it far down the page.
                    ScrollView::new(cx, move |cx| {
                        VStack::new(cx, move |cx| {
                            for (display, source) in entries.clone() {
                                row(cx, display, query, true)
                                    .on_press(move |cx| cx.emit(TimelineEvent::AddDrumSample(source.clone())));
                            }
                        })
                        .width(Stretch(1.0))
                        .height(Auto);
                    })
                    .class("panel")
                    .width(Stretch(1.0))
                    .height(Auto)
                    .max_height(Pixels(200.0));
                }

                // A finished multi-bar groove across new tracks in one
                // click, rather than placing each hit by hand - see
                // `timeline::beat_templates`.
                let templates = crate::timeline::beat_templates::TEMPLATES;
                section_head(cx, "Beat templates", templates.len() + 1);
                for (index, template) in templates.iter().enumerate() {
                    row(cx, template.name.to_string(), query, true)
                        .on_press(move |cx| cx.emit(TimelineEvent::AddDrumPattern(index)));
                }
                // A whole finished song, opened as a new project (asks
                // first if the current one has unsaved changes).
                row(cx, "House demo \u{b7} 2 min".to_string(), query, true)
                    .on_press(|cx| cx.emit(crate::project::ProjectEvent::OpenDemo));

                // Insert effects: works on any track kind (an audio track
                // has effects but no instrument). Lit when the selected
                // track already has one - clicking again is a no-op, same
                // as "Carve" above.
                let selected_has_compressor = Memo::new(move |_| {
                    selected_track
                        .get()
                        .and_then(|id| {
                            arrangement.get().track(id).map(|t| t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Compressor(_))))
                        })
                        .unwrap_or(false)
                });
                let selected_has_eq = Memo::new(move |_| {
                    selected_track
                        .get()
                        .and_then(|id| arrangement.get().track(id).map(|t| t.fx.ordered().iter().any(|n| matches!(n.effect, Effect::Eq(_)))))
                        .unwrap_or(false)
                });
                section_head(cx, "Audio effects", 2);
                row(cx, "Compressor".to_string(), query, true)
                    .toggle_class("is-on", selected_has_compressor)
                    .on_press(move |cx| {
                        if let Some(track) = selected_track.get() {
                            cx.emit(TimelineEvent::AddCompressorEffect(track));
                        }
                    });
                row(cx, "EQ".to_string(), query, true).toggle_class("is-on", selected_has_eq).on_press(move |cx| {
                    if let Some(track) = selected_track.get() {
                        cx.emit(TimelineEvent::AddEqEffect(track));
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
    export_status: Signal<String>,
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
        Label::new(cx, export_status).class("value").toggle_class("hidden", export_status.map(|s| s.is_empty()));
        Label::new(cx, save_status).class("value");
    })
    .class("statusbar")
    .gap(Pixels(crate::tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(24.0));
}
