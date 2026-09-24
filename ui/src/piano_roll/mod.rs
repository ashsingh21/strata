//! The piano roll: opens as a centred modal (not another fixed-position
//! popup) when you double-click a MIDI clip, so it always shows up in the
//! same predictable place regardless of where that clip happened to be
//! scrolled to. Scale-aware like the rest of Strata - its rows are scale
//! degrees, not every semitone, and it shares Interval Input's key/scale.

pub mod grid;
pub mod state;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipId, SnapGrid, Ticks};
use shared::theory::note_name;

use crate::tokens::{self, ThemeId};
use grid::Grid;
use state::{EditMode, LabelMode, NoteKey, PianoRollEvent};

const PANEL_W: f32 = 920.0;
const GRID_H: f32 = 380.0;

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
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                let name_text = open_clip.map(move |id| {
                    id.and_then(|id| arrangement.get().clip(id).map(|c| c.name.clone())).unwrap_or_default()
                });
                Label::new(cx, name_text).class("control");

                let track_text = open_clip.map(move |id| {
                    id.and_then(|id| {
                        let arr = arrangement.get();
                        let clip = arr.clip(id)?;
                        arr.track(clip.track).map(|t| t.name.clone())
                    })
                    .unwrap_or_default()
                });
                Label::new(cx, track_text).class("meta");

                let length_text = open_clip.map(move |id| {
                    id.and_then(|id| arrangement.get().clip(id).map(|c| c.length))
                        .map(|len| format!("{} bars", (len / (shared::arrangement::PPQ * 4)).max(1)))
                        .unwrap_or_default()
                });
                Label::new(cx, length_text).class("meta");

                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

                mode_button(cx, "Draw", EditMode::Draw, mode);
                mode_button(cx, "Select", EditMode::Select, mode);

                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));

                label_mode_button(cx, "Notes", LabelMode::Notes, label_mode);
                label_mode_button(cx, "Intervals", LabelMode::Intervals, label_mode);

                Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(16.0));

                let key_text = key.map(|k| note_name(*k).to_string());
                Label::new(cx, key_text).class("meta");
                let scale_text = scale_mask.map(|m| crate::interval_input::state::scale_name(*m).to_string());
                Label::new(cx, scale_text).class("meta");

                let snap_text = snap.map(|s| format!("Snap {}", s.label()));
                Label::new(cx, snap_text).class("meta");

                Button::new(cx, |cx| Label::new(cx, "Close"))
                    .class("btn")
                    .class("sm")
                    .on_press(|cx| cx.emit(PianoRollEvent::Close));
            })
            .class("synth-devhead")
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .width(Stretch(1.0));

            Grid::new(cx, arrangement, open_clip, mode, label_mode, selected, snap, key, scale_mask, playhead, theme)
                .class("synth-disp")
                .width(Pixels(PANEL_W - tokens::SPACE_2 * 2.0))
                .height(Pixels(GRID_H));
        })
        .class("panel")
        .gap(Pixels(tokens::SPACE_2))
        .padding(Pixels(tokens::SPACE_2))
        .width(Pixels(PANEL_W))
        .height(Auto);
    })
    .class("piano-roll-backdrop")
    .toggle_class("hidden", open_clip.map(|c| c.is_none()))
    .position_type(PositionType::Absolute)
    .top(Pixels(0.0))
    .left(Pixels(0.0))
    .width(Stretch(1.0))
    .height(Stretch(1.0))
    .alignment(Alignment::Center);
}

fn mode_button(cx: &mut Context, label: &'static str, this: EditMode, mode: Signal<EditMode>) {
    let on = mode.map(move |m| *m == this);
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("btn")
        .class("sm")
        .toggle_class("is-mute", on)
        .on_press(move |cx| cx.emit(PianoRollEvent::SetMode(this)));
}

fn label_mode_button(cx: &mut Context, label: &'static str, this: LabelMode, mode: Signal<LabelMode>) {
    let on = mode.map(move |m| *m == this);
    Button::new(cx, move |cx| Label::new(cx, label))
        .class("btn")
        .class("sm")
        .toggle_class("is-mute", on)
        .on_press(move |cx| cx.emit(PianoRollEvent::SetLabelMode(this)));
}
