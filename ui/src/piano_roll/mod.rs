//! The piano roll: the MIDI clip editor. Opens (double-click a MIDI clip)
//! in the lower panel in place of the device - docked in the workspace,
//! not floating over it - and closes back to the device (Close, Esc, or
//! the device chain's instrument chip). Scale-aware like the rest of
//! Strata: its rows are scale degrees, and it shares Interval Input's
//! key/scale.

pub mod grid;
pub mod state;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipContent, ClipId, SnapGrid, Ticks, PPQ};
use shared::theory::{degree_name, note_name};

use crate::synth::segmented::segmented;
use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, ThemeId};
use grid::{grid_height, is_drum_clip, note_with_octave, row_pitches, ticks_to_bbs, Grid};
use state::{EditMode, LabelMode, NoteKey, PianoRollEvent};

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
            let meta_text = Memo::new(move |_| {
                let arr = arrangement.get();
                let Some(clip) = open_clip.get().and_then(|id| arr.clip(id)) else { return String::new() };
                let track = arr.track(clip.track).map(|t| t.name.clone()).unwrap_or_default();
                let bars_of = |t: Ticks| (t as f64 / (PPQ * 4) as f64).ceil().max(1.0) as i64;
                let plural = |n: i64| if n == 1 { "bar" } else { "bars" };
                let bars = bars_of(clip.length);
                let linked = clip
                    .link()
                    .map(|l| format!(" \u{b7} linked \u{d7}{}", arr.link_count(l)))
                    .unwrap_or_default();
                let base = match &clip.content {
                    ClipContent::Midi { loop_len: Some(len), .. } if *len < clip.length => {
                        let repeats = (clip.length as f64 / *len as f64).ceil() as i64;
                        format!("{track} \u{b7} MIDI \u{b7} {bars} {}, loops \u{d7}{repeats}", plural(bars))
                    }
                    _ => format!("{track} \u{b7} MIDI \u{b7} {bars} {}", plural(bars)),
                };
                format!("{base}{linked}")
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
                .on_press(move |cx| {
                    if let Some(clip) = open_clip.get() {
                        cx.emit(TimelineEvent::SetPatternBars { clip, bars: pattern_bars.get() + 1 });
                    }
                });

            Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(20.0));

            // Note naming and the key only mean something for pitched
            // instruments; a Drum Kit clip's rows are its pads.
            let drums = Memo::new(move |_| open_clip.get().is_some_and(|id| is_drum_clip(&arrangement.get(), id)));
            HStack::new(cx, move |cx| {
                let label_modes = [LabelMode::Notes, LabelMode::Intervals];
                segmented(
                    cx,
                    2,
                    |cx, i| Label::new(cx, if i == 0 { "Notes" } else { "Intervals" }),
                    move |i| label_mode.map(move |m| *m == label_modes[i]),
                    move |cx, i| cx.emit(PianoRollEvent::SetLabelMode(label_modes[i])),
                );
                let key_text = Memo::new(move |_| {
                    format!("{} {}", note_name(key.get()), crate::interval_input::state::scale_name(scale_mask.get()).to_lowercase())
                });
                Button::new(cx, move |cx| Label::new(cx, key_text))
                    .class("btn")
                    .class("sm")
                    .on_press(|cx| cx.emit(crate::interval_input::state::IntervalInputEvent::ToggleOpen));
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
            Pixels(grid_height(row_pitches(&notes, key.get(), scale_mask.get(), drums).len()))
        });
        Grid::new(cx, arrangement, open_clip, mode, label_mode, selected, snap, key, scale_mask, playhead, theme)
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
    .width(Stretch(1.0))
    .height(Auto);
}
