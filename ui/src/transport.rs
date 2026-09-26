//! The header (top bar): the project and its state, the key, tempo and
//! meter, the transport (rewind, stop, play, record), the position with
//! elapsed time, loop and metronome, the input level, and CPU/output
//! meters. `size-toolbar` tall on `bg-000`.

use vizia::prelude::*;

use shared::arrangement::{position_to_ticks, Arrangement, PPQ};
use shared::theory::scale::note_name;
use shared::Position;

use crate::app::AppEvent;
use crate::bpm_field::BpmField;
use crate::glyph::{Glyph, GlyphKind, GlyphColor};
use crate::interval_input::state::{scale_name, IntervalInputEvent};
use crate::knob::Knob;
use crate::meter::{Meter, HOT_THRESHOLD};
use crate::recorder::RecorderModelEvent;
use crate::timeline::state::TimelineEvent;
use crate::tokens::{ThemeId, SPACE_2, SPACE_3};

/// A bit taller than `tokens::SIZE_TOOLBAR` (which every *other* header -
/// Carve's, the piano roll's, Interval Input's - still uses): the app's
/// own top bar reads as the one thing everything else sits below, so it
/// gets a size of its own rather than sharing the generic device-header
/// token.
const HEADER_HEIGHT: f32 = 48.0;

/// Everything the header shows or drives. All signals, so `Copy`.
#[derive(Clone, Copy)]
pub struct HeaderProps {
    pub theme: Signal<ThemeId>,
    pub playing: Signal<bool>,
    pub loop_on: Signal<bool>,
    pub record_armed: Signal<bool>,
    pub click_on: Signal<bool>,
    pub position: Signal<Position>,
    pub interval_open: Signal<bool>,
    pub key: Signal<u8>,
    pub scale_mask: Signal<u16>,
    pub input_level: Signal<f32>,
    pub input_gain_pos: Signal<f32>,
    pub cpu_load: Signal<f32>,
    pub output_db: Signal<f32>,
    pub arrangement: Signal<Arrangement>,
    /// "Saved" or "Edited".
    pub save_status: Memo<String>,
}

fn vsep(cx: &mut Context) {
    Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(22.0));
}

/// One transport button: a drawn glyph, its colour following its state.
fn transport_button<'a>(
    cx: &'a mut Context,
    theme: Signal<ThemeId>,
    kind: GlyphKind,
    on: Signal<bool>,
    color: GlyphColor,
) -> Handle<'a, Button> {
    Button::new(cx, move |cx| Glyph::new(cx, kind, on, theme, color).width(Pixels(15.0)).height(Pixels(15.0)))
        .class("btn")
        .class("tbtn")
}

/// "mm:ss.mmm" from a musical position at a constant tempo.
fn elapsed_text(position: Position, bpm: f64) -> String {
    let seconds = position_to_ticks(position) as f64 / PPQ as f64 * 60.0 / bpm.max(1.0);
    let minutes = (seconds / 60.0).floor();
    format!("{:02}:{:06.3}", minutes as u32, seconds - minutes * 60.0)
}

pub fn header(cx: &mut Context, props: HeaderProps, bpm: impl SignalGet<f64> + Copy + 'static) {
    let HeaderProps { theme, playing, loop_on, record_armed, click_on, position, interval_open, .. } = props;

    HStack::new(cx, move |cx| {
        // Project and what's happening to it.
        let status = Memo::new(move |_| {
            let arr = props.arrangement.get();
            if record_armed.get() && playing.get() {
                if let Some(track) = arr.tracks.iter().find(|t| t.arm) {
                    return format!("Recording to {}", track.name);
                }
            }
            props.save_status.get()
        });
        VStack::new(cx, move |cx| {
            Label::new(cx, crate::project::project_name()).class("title").font_size(14.0);
            Label::new(cx, status).class("value").font_size(12.0);
        })
        .width(Pixels(126.0))
        .height(Auto);

        vsep(cx);

        Label::new(cx, "Key").class("label").font_size(12.0);
        let key_text = Memo::new(move |_| {
            format!("{} {}  \u{2304}", note_name(props.key.get()), scale_name(props.scale_mask.get()).to_lowercase())
        });
        Button::new(cx, move |cx| Label::new(cx, key_text).font_size(13.0))
            .class("btn")
            .toggle_class("is-on", interval_open)
            .on_press(|cx| cx.emit(IntervalInputEvent::ToggleOpen));

        HStack::new(cx, move |cx| {
            BpmField::new(cx, bpm, theme, |cx, v| {
                cx.emit(TimelineEvent::SetTempo(v));
                cx.emit(AppEvent::SetBpm(v));
            })
            .width(Pixels(52.0))
            .height(Pixels(18.0));
        })
        .class("readout")
        .alignment(Alignment::Center)
        .size(Auto);
        Label::new(cx, "BPM").class("value").font_size(12.0);
        Button::new(cx, |cx| Label::new(cx, "Tap").font_size(13.0))
            .class("btn")
            .class("quiet")
            .on_press(|cx| cx.emit(AppEvent::Tap));
        Label::new(cx, "4/4").class("readout").font_size(13.0).size(Auto);

        // The transport, grouped.
        HStack::new(cx, move |cx| {
            let never = Signal::new(false);
            let ink = |p: &crate::tokens::Palette, _on: bool| p.ink;
            transport_button(cx, theme, GlyphKind::Rewind, never, ink).on_press(|cx| cx.emit(AppEvent::Rewind));
            transport_button(cx, theme, GlyphKind::Stop, never, ink).on_press(|cx| cx.emit(AppEvent::Stop));
            transport_button(cx, theme, GlyphKind::Play, playing, |p, on| if on { p.on_signal } else { p.ink })
                .toggle_class("is-play", playing)
                .on_press(|cx| cx.emit(AppEvent::TogglePlay));
            transport_button(cx, theme, GlyphKind::Record, record_armed, |p, on| if on { p.on_record } else { p.ink })
                .toggle_class("is-rec", record_armed)
                .on_press(|cx| cx.emit(AppEvent::ToggleArm));
        })
        .class("tgroup")
        .size(Auto);

        // Position: bars.beats.sixteenths, large, plus elapsed time - and
        // a record dot while a take is actually being recorded.
        let recording = Memo::new(move |_| record_armed.get() && playing.get());
        let bar_text = position.map(|p| format!("{}.{}.{}", p.bar, p.beat, p.sixteenth));
        let time_text = Memo::new(move |_| elapsed_text(position.get(), bpm.get()));
        HStack::new(cx, move |cx| {
            Glyph::new(cx, GlyphKind::Record, recording, theme, |p, _| p.record)
                .width(Pixels(13.0))
                .height(Pixels(13.0))
                .toggle_class("hidden", recording.map(|r| !*r));
            Label::new(cx, bar_text).class("readout-big").width(Pixels(70.0));
            Label::new(cx, time_text).class("value").font_size(12.0);
        })
        .class("readout")
        .class("position")
        .gap(Pixels(SPACE_2))
        .alignment(Alignment::Left)
        .size(Auto);

        transport_button(cx, theme, GlyphKind::Loop, loop_on, |p, on| if on { p.md } else { p.ink })
            .toggle_class("is-mod", loop_on)
            .on_press(|cx| cx.emit(AppEvent::ToggleLoop));
        transport_button(cx, theme, GlyphKind::Metronome, click_on, |p, _| p.ink)
            .toggle_class("is-on", click_on)
            .on_press(|cx| cx.emit(AppEvent::ToggleClick));

        vsep(cx);

        // Input gain-staging stays by the transport: that's when it matters.
        HStack::new(cx, move |cx| {
            Label::new(cx, "In").class("label").font_size(12.0);
            // Every other Meter call sets an explicit width (it has no
            // default) - this one didn't, so it was rendering at zero
            // width: invisible, not just narrow. 10px matches Carve's own
            // output meter, the other two-channel-width instance.
            Meter::new(cx, props.input_level, props.input_level, Signal::new(false), Signal::new(false), theme, |_cx| {})
                .width(Pixels(10.0))
                .height(Pixels(24.0));
            Knob::plain(cx, props.input_gain_pos, 0.5, theme, |cx, p| cx.emit(RecorderModelEvent::SetInputGain(p)))
                .size(Pixels(22.0));
        })
        .gap(Pixels(SPACE_2))
        .alignment(Alignment::Center)
        .size(Auto);

        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

        // CPU: the audio callback's share of its real-time budget.
        Label::new(cx, "CPU").class("label").font_size(12.0);
        let cpu = props.cpu_load;
        HStack::new(cx, move |cx| {
            Element::new(cx).class("bar-fill").width(cpu.map(|c| Percentage((c * 100.0).clamp(0.0, 100.0))));
        })
        .class("bar")
        .width(Pixels(32.0))
        .height(Pixels(4.0));
        Label::new(cx, cpu.map(|c| format!("{:.0}%", c * 100.0))).class("value").font_size(12.0).width(Pixels(32.0));

        // Output level: signal up to -12 dB, warn above.
        let out_db = props.output_db;
        let fraction = out_db.map(|db| ((db + 60.0) / 60.0).clamp(0.0, 1.0));
        HStack::new(cx, move |cx| {
            Element::new(cx)
                .class("bar-fill")
                .class("level")
                .toggle_class("hot", fraction.map(|f| *f > HOT_THRESHOLD))
                .width(fraction.map(|f| Percentage(f * 100.0)));
        })
        .class("bar")
        .width(Pixels(64.0))
        .height(Pixels(4.0));
        Label::new(cx, out_db.map(|db| if *db <= -59.9 { "\u{2212}\u{221e}".to_string() } else { format!("{db:.1}").replace('-', "\u{2212}") }))
            .class("value")
            .font_size(12.0)
            .width(Pixels(38.0));
    })
    .class("transport")
    .gap(Pixels(SPACE_2))
    .padding_left(Pixels(SPACE_3))
    .padding_right(Pixels(SPACE_3))
    .alignment(Alignment::Left)
    .height(Pixels(HEADER_HEIGHT))
    .width(Stretch(1.0));
}
