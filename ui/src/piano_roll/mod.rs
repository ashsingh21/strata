//! The piano roll: the MIDI clip editor. Opens (double-click a MIDI clip)
//! in the lower panel in place of the device - docked in the workspace,
//! not floating over it - and closes back to the device (Close, Esc, or
//! the device chain's instrument chip). Scale-aware like the rest of
//! Strata: its rows are scale degrees, and it shares Interval Input's
//! key/scale.

pub mod grid;
pub mod state;

use crate::lessons::LessonTargetExt;
use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipContent, ClipId, SnapGrid, Ticks, PPQ};
use shared::theory::{degree_name, note_name};

use crate::synth::segmented::segmented;
use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, ThemeId};
use grid::{grid_height, is_drum_clip, note_with_octave, row_pitches, ticks_to_bbs, Grid};
use state::{ChordShape, EditMode, LabelMode, NoteKey, PianoRollEvent};

/// The footer's description of the selection: how many, which degrees and
/// notes, where, how long and how hard - or the clip's note count.
fn selection_text(arr: &Arrangement, clip: Option<ClipId>, selected: &std::collections::HashSet<NoteKey>, key: u8) -> String {
    let Some(clip) = clip.and_then(|id| arr.clip(id)) else { return String::new() };
    let ClipContent::Midi { notes, .. } = &clip.content else { return String::new() };
    let mut chosen: Vec<_> = notes.iter().filter(|n| selected.contains(&(n.start, n.pitch))).collect();
    if chosen.is_empty() {
        // Notes past a shortened pattern are kept but neither shown nor
        // played - say so, so they don't look lost.
        let in_pattern = notes.iter().filter(|n| n.start < clip.content_len()).count();
        let hidden = notes.len() - in_pattern;
        return if hidden > 0 {
            format!("{in_pattern} notes \u{b7} {hidden} more past the pattern, kept")
        } else {
            format!("{in_pattern} notes")
        };
    }
    chosen.sort_by_key(|n| (n.start, n.pitch));
    let list = |f: &dyn Fn(&shared::arrangement::MidiNote) -> String| {
        let mut items: Vec<String> = chosen.iter().map(|n| f(n)).collect();
        items.dedup();
        items.join(", ")
    };
    let degrees = list(&|n| degree_name(((n.pitch as i32 - key as i32).rem_euclid(12)) as u8).to_string());
    let names = list(&|n| note_with_octave(n.pitch));
    let first = chosen.iter().map(|n| n.start).min().unwrap_or(0);
    let last_end = chosen.iter().map(|n| n.start + n.length).max().unwrap_or(0);
    let lengths: std::collections::BTreeSet<Ticks> = chosen.iter().map(|n| n.length).collect();
    let length = if lengths.len() == 1 {
        let len = *lengths.iter().next().unwrap();
        let per = if chosen.len() > 1 { " each" } else { "" };
        match PPQ * 4 / len.max(1) {
            d if len * d == PPQ * 4 => format!("1/{d}{per}"),
            _ => format!("{len} ticks{per}"),
        }
    } else {
        "mixed lengths".to_string()
    };
    let velocities = list(&|n| n.velocity.to_string());
    format!(
        "{} selected   \u{b7}   {degrees} \u{b7} {names} \u{b7} {} \u{2013} {} \u{b7} {length} \u{b7} vel {velocities}",
        chosen.len(),
        ticks_to_bbs(first),
        ticks_to_bbs((last_end - PPQ / 4).max(first)),
    )
}

#[allow(clippy::too_many_arguments)]
pub fn piano_roll_view(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    arrangement: Signal<Arrangement>,
    open_clip: Signal<Option<ClipId>>,
    mode: Signal<EditMode>,
    label_mode: Signal<LabelMode>,
    selected: Signal<std::collections::HashSet<NoteKey>>,
    snap: Signal<SnapGrid>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    playhead: Signal<Ticks>,
    octave: Signal<i32>,
    chord: Signal<ChordShape>,
) {
    VStack::new(cx, move |cx| {
        // Header: the clip, then how it's labelled, then how you edit it.
        HStack::new(cx, move |cx| {
            let track_color = Memo::new(move |_| {
                let arr = arrangement.get();
                open_clip
                    .get()
                    .and_then(|id| arr.clip(id))
                    .and_then(|c| arr.track(c.track))
                    .map(|t| crate::timeline::header::clip_color_to_rgb(t.color))
                    .unwrap_or(tokens::CLIP_VIOLET)
            });
            Element::new(cx).class("swatch").background_color(track_color);
            let name_text = Memo::new(move |_| {
                open_clip.get().and_then(|id| arrangement.get().clip(id).map(|c| c.name.clone())).unwrap_or_default()
            });
            Label::new(cx, name_text).class("heading");
            // Only what the rest of the header doesn't say: the track's name
            // is in the device chain above, the length in Length below.
            let meta_text = Memo::new(move |_| {
                let arr = arrangement.get();
                let Some(clip) = open_clip.get().and_then(|id| arr.clip(id)) else { return String::new() };
                let mut parts = Vec::new();
                if let ClipContent::Midi { loop_len: Some(len), .. } = &clip.content {
                    if *len < clip.length {
                        parts.push(format!("loops \u{d7}{}", (clip.length as f64 / *len as f64).ceil() as i64));
                    }
                }
                if let Some(l) = clip.link() {
                    parts.push(format!("linked \u{d7}{}", arr.link_count(l)));
                }
                parts.join(" \u{b7} ")
            });
            Label::new(cx, meta_text).class("value");

            // Pattern length: what the grid edits, and what repeats when
            // the clip is stretched on the timeline.
            let pattern_bars = Memo::new(move |_| {
                let arr = arrangement.get();
                open_clip
                    .get()
                    .and_then(|id| arr.clip(id))
                    .map(|c| (c.content_len() as f64 / (PPQ * 4) as f64).ceil().max(1.0) as i64)
                    .unwrap_or(1)
            });
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(20.0));
            Label::new(cx, "Pattern").class("label");
            Button::new(cx, |cx| Label::new(cx, "\u{2212}"))
                .class("btn")
                .class("sm")
                .class("quiet")
                .on_press(move |cx| {
                    if let Some(clip) = open_clip.get() {
                        cx.emit(TimelineEvent::SetPatternBars { clip, bars: pattern_bars.get() - 1 });
                    }
                });
            Label::new(cx, pattern_bars.map(|n| format!("{n} {}", if *n == 1 { "bar" } else { "bars" }))).class("value");
            Button::new(cx, |cx| Label::new(cx, "+"))
                .class("btn")
                .class("sm")
                .class("quiet")
                .lesson_target(crate::lessons::Target::PatternPlus)
                .on_press(move |cx| {
                    if let Some(clip) = open_clip.get() {
                        cx.emit(TimelineEvent::SetPatternBars { clip, bars: pattern_bars.get() + 1 });
                    }
                });

            // The clip's whole length, typed: dragging a clip's edge out to
            // bar 128 took a while. Longer repeats the pattern; shorter
            // trims.
            let clip_bars = Memo::new(move |_| {
                let arr = arrangement.get();
                open_clip
                    .get()
                    .and_then(|id| arr.clip(id))
                    .map(|c| ((c.length as f64 / (PPQ * 4) as f64).ceil().max(1.0) as i64).to_string())
                    .unwrap_or_default()
            });
            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(20.0));
            Label::new(cx, "Length").class("label");
            // Editable: a Textbox needs a signal it can write, kept in step
            // with the clip (a computed value made it read-only).
            let length_draft = Signal::new(clip_bars.get());
            Textbox::new(cx, length_draft)
                .bind(clip_bars, move |_| length_draft.set(clip_bars.get()))
                .on_edit(move |_, text| length_draft.set(text))
                .on_submit(move |cx, text, _| {
                    let Some(clip) = open_clip.get() else { return };
                    if let Ok(bars) = text.trim().parse::<i64>() {
                        cx.emit(TimelineEvent::SetClipBars { clip, bars: bars.clamp(1, 999) });
                    }
                })
                .class("search")
                .class("value")
                .tooltip(|cx| {
                    Tooltip::new(cx, |cx| {
                        Label::new(cx, "The clip's length in bars: type one and press Enter. Its pattern repeats to fill it.");
                    })
                    .arrow(false)
                })
                .width(Pixels(44.0))
                .height(Pixels(20.0));
            Label::new(cx, "bars").class("value");

            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(20.0));

            // Note naming and the key only mean something for pitched
            // instruments; a Drum Kit clip's rows are its pads.
            let drums = Memo::new(move |_| open_clip.get().is_some_and(|id| is_drum_clip(&arrangement.get(), id)));
            HStack::new(cx, move |cx| {
                // Which octaves the rows show - an empty clip can't reach a
                // low bass or a high lead otherwise.
                let range = Memo::new(move |_| {
                    let arr = arrangement.get();
                    let notes = open_clip
                        .get()
                        .and_then(|id| arr.clip(id))
                        .map(|c| match &c.content {
                            ClipContent::Midi { notes, .. } => notes.clone(),
                            ClipContent::Audio { .. } => Vec::new(),
                        })
                        .unwrap_or_default();
                    let rows = row_pitches(&notes, key.get(), scale_mask.get(), false, octave.get());
                    match (rows.last(), rows.first()) {
                        (Some(&lo), Some(&hi)) => format!("{}\u{2013}{}", note_with_octave(lo), note_with_octave(hi)),
                        _ => String::new(),
                    }
                });
                Label::new(cx, "Octave").class("label");
                let octave_tip = "Move the rows up or down an octave. Shift+\u{2191}/\u{2193} moves selected notes an octave; \u{2191}/\u{2193} a scale step.";
                Button::new(cx, |cx| Label::new(cx, "\u{2212}"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .tooltip(move |cx| {
                        Tooltip::new(cx, move |cx| {
                            Label::new(cx, octave_tip);
                        })
                        .arrow(false)
                    })
                    .on_press(|cx| cx.emit(PianoRollEvent::ShiftOctave(-1)));
                Label::new(cx, range).class("value");
                Button::new(cx, |cx| Label::new(cx, "+"))
                    .class("btn")
                    .class("sm")
                    .class("quiet")
                    .tooltip(move |cx| {
                        Tooltip::new(cx, move |cx| {
                            Label::new(cx, octave_tip);
                        })
                        .arrow(false)
                    })
                    .on_press(|cx| cx.emit(PianoRollEvent::ShiftOctave(1)));
                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(20.0));
                let key_text = Memo::new(move |_| {
                    format!("{} {}", note_name(key.get()), crate::interval_input::state::scale_name(scale_mask.get()).to_lowercase())
                });
                Button::new(cx, move |cx| Label::new(cx, key_text))
                    .class("btn")
                    .class("sm")
                    .lesson_target(crate::lessons::Target::KeyMenu)
                    .on_press(crate::key_menu::toggle_under);
            })
            .toggle_class("hidden", drums)
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .width(Auto)
            .height(Auto);

            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

            let modes = [EditMode::Select, EditMode::Draw];
            segmented(
                cx,
                2,
                |cx, i| Label::new(cx, if i == 0 { "Select" } else { "Draw" }),
                move |i| mode.map(move |m| *m == modes[i]),
                move |cx, i| cx.emit(PianoRollEvent::SetMode(modes[i])),
            );
            // What a Draw click writes - a chord for rhythm parts. Pitched
            // clips only.
            HStack::new(cx, move |cx| {
                segmented(
                    cx,
                    3,
                    |cx, i| Label::new(cx, ChordShape::ALL[i].label()),
                    move |i| chord.map(move |c| *c == ChordShape::ALL[i]),
                    move |cx, i| {
                        cx.emit(PianoRollEvent::SetChord(ChordShape::ALL[i]));
                        cx.emit(PianoRollEvent::SetMode(EditMode::Draw));
                    },
                );
            })
            .toggle_class("hidden", drums)
            .tooltip(|cx| {
                Tooltip::new(cx, |cx| {
                    Label::new(cx, "What one click in Draw writes: a note, or a chord built on it from the key's notes.");
                })
                .arrow(false)
            })
            .width(Auto)
            .height(Auto);
            Label::new(cx, "Snap").class("label");
            let snap_text = snap.map(|s| s.label().to_string());
            Button::new(cx, move |cx| Label::new(cx, snap_text))
                .class("readout")
                .class("snap")
                .on_press(|cx| cx.emit(TimelineEvent::CycleSnap));
            Button::new(cx, |cx| Label::new(cx, "Close"))
                .class("btn")
                .class("quiet")
                .on_press(|cx| cx.emit(PianoRollEvent::Close));
        })
        .class("synth-devhead")
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .padding_left(Pixels(tokens::SPACE_3))
        .padding_right(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Pixels(tokens::SIZE_TOOLBAR));
        Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));

        // The grid is exactly as tall as its rows: no dead space below.
        let height = Memo::new(move |_| {
            let arr = arrangement.get();
            let notes = open_clip
                .get()
                .and_then(|id| arr.clip(id))
                .map(|c| match &c.content {
                    ClipContent::Midi { notes, .. } => notes.clone(),
                    ClipContent::Audio { .. } => Vec::new(),
                })
                .unwrap_or_default();
            let drums = open_clip.get().is_some_and(|id| is_drum_clip(&arr, id));
            Pixels(grid_height(row_pitches(&notes, key.get(), scale_mask.get(), drums, octave.get()).len()))
        });
        Grid::new(cx, arrangement, open_clip, mode, label_mode, selected, snap, key, scale_mask, playhead, theme, octave, chord)
            .width(Stretch(1.0))
            .height(height);
        Element::new(cx).class("hairline").width(Stretch(1.0)).height(Pixels(1.0));

        // Footer: the selection in words, and where the playhead is.
        HStack::new(cx, move |cx| {
            let details = Memo::new(move |_| selection_text(&arrangement.get(), open_clip.get(), &selected.get(), key.get()));
            Label::new(cx, details).class("value");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            let position = Memo::new(move |_| {
                let arr = arrangement.get();
                let start = open_clip.get().and_then(|id| arr.clip(id)).map(|c| c.start).unwrap_or(0);
                ticks_to_bbs((playhead.get() - start).max(0))
            });
            Label::new(cx, position).class("value");
        })
        .alignment(Alignment::Left)
        .padding_left(Pixels(tokens::SPACE_3))
        .padding_right(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Pixels(26.0));
    })
    .class("device")
    // Breathing room so the grid's row labels don't sit on the edge.
    .padding_left(Pixels(tokens::SPACE_2))
    .padding_right(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);
}
