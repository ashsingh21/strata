mod app;
mod canvas_text;
mod fader;
mod knob;
mod lfo_demo;
mod meter;
mod mixer;
mod pill;
mod synth;
mod timeline;
mod tokens;
mod transport;

use std::time::{Duration, Instant};

use vizia::prelude::*;

use app::{AppData, AppEvent};
use shared::arrangement::position_to_ticks;
use synth::state::{SynthEvent, SynthModel};
use timeline::state::{TimelineEvent, TimelineState};

fn main() -> Result<(), ApplicationError> {
    let (params, telemetry_tx, telemetry_rx) = shared::bridge();
    let engine_handle = engine::start(params.clone(), telemetry_tx)
        .expect("failed to start audio engine");

    Application::new(move |cx| {
        cx.add_stylesheet(include_style!("styles/base.css")).expect("failed to add base.css");
        cx.add_stylesheet(include_style!("styles/studio.css")).expect("failed to add studio.css");
        cx.add_stylesheet(include_style!("styles/daylight.css")).expect("failed to add daylight.css");

        let app_data = AppData::new(params.clone(), telemetry_rx, engine_handle);
        let theme = app_data.theme;
        let playing = app_data.playing;
        let loop_on = app_data.loop_on;
        let record_armed = app_data.record_armed;
        let position = app_data.position;

        app_data.build(cx);

        let timeline_state = TimelineState::new();
        let tl_arrangement = timeline_state.arrangement;
        let tl_transform = timeline_state.transform;
        let tl_snap = timeline_state.snap;
        let tl_selection = timeline_state.selection;
        let tl_playhead = timeline_state.playhead_ticks;

        timeline::peaks_loader::spawn_peak_loaders(
            cx,
            &timeline::assets_dir(),
            &tl_arrangement.get(),
        );

        timeline_state.build(cx);

        let synth_model = SynthModel::new();
        let synth_state = synth_model.state;
        let synth_lfo_phase = synth_model.lfo_scope_phase;
        synth_model.build(cx);

        // ~60 fps: drains engine telemetry, runs meter ballistics, advances
        // the LFO demo's and synth's animated modulation rings/scope, and
        // syncs the timeline playhead from the transport's live position.
        let last_tick = std::cell::Cell::new(Instant::now());
        let render_timer = cx.add_timer(Duration::from_millis(16), None, move |cx, action| {
            if let TimerAction::Tick(_) = action {
                let now = Instant::now();
                let dt = (now - last_tick.get()).as_secs_f32().min(0.25);
                last_tick.set(now);

                cx.emit(AppEvent::Tick);
                let ticks = position_to_ticks(position.get());
                cx.emit(TimelineEvent::SyncPlayhead { ticks, playing: playing.get() });
                cx.emit(SynthEvent::Tick(dt));
            }
        });
        cx.start_timer(render_timer);

        let is_daylight = theme.map(|t| t.is_daylight());

        Keymap::from(vec![
            (
                KeyChord::new(Modifiers::CTRL, Code::KeyT),
                KeymapEntry::new(0u8, |cx| cx.emit(AppEvent::ToggleTheme)),
            ),
            (
                KeyChord::new(Modifiers::SUPER, Code::KeyT),
                KeymapEntry::new(1u8, |cx| cx.emit(AppEvent::ToggleTheme)),
            ),
            (
                KeyChord::new(Modifiers::CTRL, Code::KeyZ),
                KeymapEntry::new(2u8, |cx| cx.emit(TimelineEvent::Undo)),
            ),
            (
                KeyChord::new(Modifiers::SUPER, Code::KeyZ),
                KeymapEntry::new(3u8, |cx| cx.emit(TimelineEvent::Undo)),
            ),
            (
                KeyChord::new(Modifiers::CTRL | Modifiers::SHIFT, Code::KeyZ),
                KeymapEntry::new(4u8, |cx| cx.emit(TimelineEvent::Redo)),
            ),
            (
                KeyChord::new(Modifiers::SUPER | Modifiers::SHIFT, Code::KeyZ),
                KeymapEntry::new(5u8, |cx| cx.emit(TimelineEvent::Redo)),
            ),
            (
                KeyChord::new(Modifiers::CTRL, Code::KeyE),
                KeymapEntry::new(6u8, |cx| cx.emit(TimelineEvent::SplitAtPlayhead)),
            ),
            (
                KeyChord::new(Modifiers::SUPER, Code::KeyE),
                KeymapEntry::new(7u8, |cx| cx.emit(TimelineEvent::SplitAtPlayhead)),
            ),
            (
                KeyChord::new(Modifiers::CTRL, Code::KeyD),
                KeymapEntry::new(8u8, |cx| cx.emit(TimelineEvent::DuplicateSelected)),
            ),
            (
                KeyChord::new(Modifiers::SUPER, Code::KeyD),
                KeymapEntry::new(9u8, |cx| cx.emit(TimelineEvent::DuplicateSelected)),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::Delete),
                KeymapEntry::new(10u8, |cx| cx.emit(TimelineEvent::DeleteSelected)),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::Backspace),
                KeymapEntry::new(11u8, |cx| cx.emit(TimelineEvent::DeleteSelected)),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::KeyF),
                KeymapEntry::new(12u8, |cx| cx.emit(TimelineEvent::ToggleFollow)),
            ),
        ])
        .build(cx);

        VStack::new(cx, move |cx| {
            transport::transport_bar(cx, playing, loop_on, record_armed, position);

            timeline::timeline_view(
                cx,
                theme,
                tl_arrangement,
                tl_transform,
                tl_snap,
                tl_selection,
                tl_playhead,
            );

            Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));

            synth::synth_view(cx, theme, synth_state, synth_lfo_phase);
        })
        .class("app")
        .toggle_class("theme-daylight", is_daylight)
        .height(Stretch(1.0))
        .width(Stretch(1.0));
    })
    .title("Strata")
    .inner_size((1600, 1360))
    .ignore_default_theme()
    .run()
}
