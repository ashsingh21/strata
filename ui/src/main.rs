mod app;
mod bpm_field;
mod canvas_text;
mod fader;
mod glyph;
mod interval_input;
mod knob;
mod lfo_demo;
mod meter;
mod mixer;
mod piano_roll;
mod pill;
mod project;
mod recorder;
mod sidebar;
mod status;
mod synth;
mod timeline;
mod tokens;
mod transport;

use std::time::{Duration, Instant};

use vizia::prelude::*;

use app::{AppData, AppEvent};
use interval_input::state::IntervalInputModel;
use piano_roll::state::{PianoRollEvent, PianoRollModel};
use project::{project_path, ProjectEvent, ProjectModel};
use recorder::{RecorderModel, RecordingCoordinator};
use shared::arrangement::position_to_ticks;
use synth::state::{SynthEvent, SynthModel};
use timeline::state::{TimelineEvent, TimelineState};

fn main() -> Result<(), ApplicationError> {
    let (params, telemetry_tx, telemetry_rx) = shared::bridge();
    let synth_bridge = shared::synth::synth_bridge();
    let playback_bridge = shared::playback::playback_bridge();
    let recorder_bridge = shared::recorder::recorder_bridge();
    let record_params = std::sync::Arc::new(shared::recorder::RecordParams::new());
    let engine_handle = engine::start(
        params.clone(),
        telemetry_tx,
        synth_bridge.params_rx,
        synth_bridge.note_rx,
        synth_bridge.telemetry_tx,
        playback_bridge.plan_rx,
        playback_bridge.decode_rx,
        recorder_bridge.command_rx,
        recorder_bridge.telemetry_tx,
        record_params.clone(),
    )
    .expect("failed to start audio engine");
    let engine_sample_rate = engine_handle.sample_rate;
    let playback_plan_tx = std::cell::RefCell::new(playback_bridge.plan_tx);
    let playback_decode_tx = playback_bridge.decode_tx;
    let record_command_tx = std::cell::RefCell::new(recorder_bridge.command_tx);
    let input_telemetry_rx = std::cell::RefCell::new(recorder_bridge.telemetry_rx);

    Application::new(move |cx| {
        // Strata 2's one typeface; the stylesheet's `font-family` names it.
        for font in canvas_text::PLEX_SANS {
            cx.add_font_mem(font);
        }
        cx.add_stylesheet(include_style!("styles/base.css")).expect("failed to add base.css");
        cx.add_stylesheet(include_style!("styles/studio.css")).expect("failed to add studio.css");
        cx.add_stylesheet(include_style!("styles/daylight.css")).expect("failed to add daylight.css");

        let app_data = AppData::new(params.clone(), telemetry_rx, engine_handle);
        let theme = app_data.theme;
        let playing = app_data.playing;
        let loop_on = app_data.loop_on;
        let record_armed = app_data.record_armed;
        let click_on = app_data.click_on;
        let position = app_data.position;
        let sidebar_open = app_data.sidebar_open;
        let cpu_load = app_data.cpu_load;
        let output_db = app_data.output_db;
        let block_frames = app_data.block_frames;
        let sample_rate = app_data.sample_rate;

        app_data.build(cx);

        let piano_roll_model = PianoRollModel::new();
        let piano_roll_open_clip = piano_roll_model.open_clip;
        let piano_roll_mode = piano_roll_model.mode;
        let piano_roll_label_mode = piano_roll_model.label_mode;
        let piano_roll_selected = piano_roll_model.selected;
        piano_roll_model.build(cx);

        let mut timeline_state = TimelineState::new(record_armed, playing, piano_roll_open_clip, piano_roll_selected);
        let tl_arrangement = timeline_state.arrangement;
        let tl_transform = timeline_state.transform;
        let tl_snap = timeline_state.snap;
        let tl_selection = timeline_state.selection;
        let tl_playhead = timeline_state.playhead_ticks;
        let tl_tool = timeline_state.tool;
        let tl_drums_menu_open = timeline_state.drums_menu_open;

        // A saved project (if any) replaces the empty starting arrangement
        // before anything downstream reads it - the peak/decode loaders in
        // particular need the real clip list to know what to load.
        let loaded_project = shared::project::load(&project_path()).ok();
        if let Some(project) = &loaded_project {
            tl_arrangement.set(project.arrangement.clone());
            params.set_bpm(project.arrangement.tempo_map.bpm_at(0));
        }
        let tl_bpm = tl_arrangement.map(|arr| arr.tempo_map.bpm_at(0));

        timeline::peaks_loader::spawn_peak_loaders(
            cx,
            &timeline::assets_dir(),
            &tl_arrangement.get(),
        );
        // Decodes every audio source referenced by the starting
        // arrangement so it's audible from the first Play, and keeps the
        // request sender around so a freshly recorded take can be
        // decoded too, without restarting the app.
        let decode_request_tx = timeline::peaks_loader::spawn_audio_decoder_worker(
            &timeline::assets_dir(),
            &tl_arrangement.get(),
            playback_decode_tx,
        );
        timeline_state.set_decode_sender(decode_request_tx.clone());

        timeline_state.build(cx);

        let recorder_model = RecorderModel::new(record_params.clone());
        let recording_preview = recorder_model.preview;
        let input_level = recorder_model.input_level;
        let input_gain_pos = recorder_model.input_gain_pos;
        recorder_model.build(cx);

        let synth_model = SynthModel::new(synth_bridge.params_tx, synth_bridge.note_tx, synth_bridge.telemetry_rx);
        let synth_state = synth_model.state;
        let synth_lfo_phases = (synth_model.lfo1_phase, synth_model.lfo2_phase);
        let synth_octave_shift = synth_model.octave_shift;
        let synth_meter_l = synth_model.meter_l;
        let synth_meter_r = synth_model.meter_r;
        let synth_help_open = synth_model.help_open;
        let synth_lfo_drag = synth_model.lfo_drag;
        synth_model.build(cx);
        if let Some(project) = &loaded_project {
            synth_state.set(project.synth.clone());
        }

        let project_model = ProjectModel::new(tl_arrangement, synth_state);
        let project_saved = project_model.saved;
        project_model.build(cx);
        let save_status = Memo::new(move |_| {
            let edited = project::snapshot(&tl_arrangement.get(), &synth_state.get()) != project_saved.get();
            if edited { "Edited".to_string() } else { "Saved".to_string() }
        });

        let status_model = status::StatusModel::new();
        let status_touched = status_model.touched;
        status_model.build(cx);

        let interval_model = IntervalInputModel::new();
        let interval_key = interval_model.key;
        let interval_scale_mask = interval_model.scale_mask;
        let interval_open = interval_model.open;
        let interval_show_note_names = interval_model.show_note_names;
        interval_model.build(cx);

        // ~60 fps: drains engine telemetry, runs meter ballistics, advances
        // the LFO demo's and synth's animated modulation rings/scope, syncs
        // the timeline playhead from the transport's live position, and
        // schedules Carve to play whatever MIDI notes the playhead crossed.
        let last_tick = std::cell::Cell::new(Instant::now());
        let midi_scheduler = timeline::scheduler::MidiScheduler::new();
        let recording_coordinator = RecordingCoordinator::new();
        let render_timer = cx.add_timer(Duration::from_millis(16), None, move |cx, action| {
            if let TimerAction::Tick(_) = action {
                let now = Instant::now();
                let dt = (now - last_tick.get()).as_secs_f32().min(0.25);
                last_tick.set(now);

                cx.emit(AppEvent::Tick);
                let ticks = position_to_ticks(position.get());
                let is_playing = playing.get();
                cx.emit(TimelineEvent::SyncPlayhead { ticks, playing: is_playing });
                midi_scheduler.advance(cx, &tl_arrangement.get(), ticks, is_playing);
                cx.emit(SynthEvent::Tick(dt));

                // Latest-wins: cheap to rebuild every tick, and avoids
                // needing to dirty-track arrangement changes separately.
                let plan = shared::playback::PlaybackPlan::from_arrangement(&tl_arrangement.get(), engine_sample_rate);
                let _ = playback_plan_tx.borrow_mut().push(plan);

                recording_coordinator.advance(
                    cx,
                    &tl_arrangement.get(),
                    record_armed.get(),
                    is_playing,
                    ticks,
                    &record_command_tx,
                    &decode_request_tx,
                );
                recording_coordinator.drain_input_meter(cx, &input_telemetry_rx, dt);
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
            (
                KeyChord::new(Modifiers::empty(), Code::Escape),
                KeymapEntry::new(13u8, |cx| cx.emit(PianoRollEvent::Close)),
            ),
            (
                KeyChord::new(Modifiers::CTRL, Code::KeyS),
                KeymapEntry::new(14u8, |cx| cx.emit(ProjectEvent::Save)),
            ),
            (
                KeyChord::new(Modifiers::SUPER, Code::KeyS),
                KeymapEntry::new(15u8, |cx| cx.emit(ProjectEvent::Save)),
            ),
            (
                KeyChord::new(Modifiers::CTRL, Code::KeyB),
                KeymapEntry::new(16u8, |cx| cx.emit(AppEvent::ToggleSidebar)),
            ),
            (
                KeyChord::new(Modifiers::SUPER, Code::KeyB),
                KeymapEntry::new(17u8, |cx| cx.emit(AppEvent::ToggleSidebar)),
            ),
        ])
        .build(cx);

        VStack::new(cx, move |cx| {
            transport::header(
                cx,
                transport::HeaderProps {
                    theme,
                    playing,
                    loop_on,
                    record_armed,
                    click_on,
                    position,
                    interval_open,
                    key: interval_key,
                    scale_mask: interval_scale_mask,
                    input_level,
                    input_gain_pos,
                    cpu_load,
                    output_db,
                    arrangement: tl_arrangement,
                    save_status,
                },
                tl_bpm,
            );
            Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));

            HStack::new(cx, move |cx| {
                sidebar::sidebar(cx, synth_state, sidebar_open);
                Element::new(cx)
                    .class("hairline")
                    .toggle_class("hidden", sidebar_open.map(|o| !*o))
                    .width(Pixels(1.0))
                    .height(Stretch(1.0));

                VStack::new(cx, move |cx| {
                    timeline::timeline_view(
                        cx,
                        theme,
                        tl_arrangement,
                        tl_transform,
                        tl_snap,
                        tl_selection,
                        tl_playhead,
                        recording_preview,
                        tl_tool,
                    );

                    Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));

                    // The lower panel spans the arrangement's width and sizes to
                    // its device; the device fills it rather than floating.
                    VStack::new(cx, move |cx| {
                        // The device chain: the track's one device, raised,
                        // and the interval input's toggle as a quiet action.
                        HStack::new(cx, move |cx| {
                            // While a clip is open its editor is raised; the
                            // instrument chip goes back to the device.
                            let editing = piano_roll_open_clip.map(|c| c.is_some());
                            let clip_name = Memo::new(move |_| {
                                piano_roll_open_clip
                                    .get()
                                    .and_then(|id| tl_arrangement.get().clip(id).map(|c| c.name.clone()))
                                    .unwrap_or_default()
                            });
                            Button::new(cx, move |cx| Label::new(cx, clip_name))
                                .class("btn")
                                .class("is-on")
                                .toggle_class("hidden", editing.map(|e| !*e));
                            Button::new(cx, |cx| {
                                HStack::new(cx, |cx| {
                                    Element::new(cx).class("swatch").background_color(tokens::CLIP_VIOLET);
                                    Label::new(cx, "Carve");
                                })
                                .gap(Pixels(tokens::SPACE_1))
                                .alignment(Alignment::Center)
                                .size(Auto)
                            })
                            .class("btn")
                            .toggle_class("is-on", editing.map(|e| !*e))
                            .on_press(|cx| cx.emit(PianoRollEvent::Close));
                            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                            Button::new(cx, |cx| Label::new(cx, "Show input"))
                                .class("btn")
                                .class("quiet")
                                .toggle_class("is-on", interval_open)
                                .on_press(|cx| cx.emit(interval_input::state::IntervalInputEvent::ToggleOpen));
                        })
                        .gap(Pixels(tokens::SPACE_2))
                        .padding_left(Pixels(tokens::SPACE_1))
                        .alignment(Alignment::Left)
                        .width(Stretch(1.0))
                        .height(Pixels(tokens::SIZE_CONTROL + 6.0));

                        // The editor replaces the device while a clip is open.
                        Binding::new(cx, piano_roll_open_clip, move |cx| {
                            if piano_roll_open_clip.get().is_some() {
                                piano_roll::piano_roll_view(
                                    cx,
                                    theme,
                                    tl_arrangement,
                                    piano_roll_open_clip,
                                    piano_roll_mode,
                                    piano_roll_label_mode,
                                    piano_roll_selected,
                                    tl_snap,
                                    interval_key,
                                    interval_scale_mask,
                                    tl_playhead,
                                );
                            } else {
                                synth::synth_view(
                                    cx,
                                    theme,
                                    synth_state,
                                    synth_lfo_phases,
                                    synth_octave_shift,
                                    synth_meter_l,
                                    synth_meter_r,
                                    synth_help_open,
                                    synth_lfo_drag,
                                );
                            }
                        });

                        interval_input::interval_input_view(
                            cx,
                            theme,
                            synth_state,
                            interval_key,
                            interval_scale_mask,
                            interval_open,
                            interval_show_note_names,
                        );
                    })
                    .gap(Pixels(tokens::SPACE_2))
                    .class("lower-panel")
                    .width(Stretch(1.0))
                    .height(Auto);
                })
                .width(Stretch(1.0))
                .height(Stretch(1.0));
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));

            Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));
            sidebar::status_bar(cx, sample_rate, block_frames, status_touched, save_status);

            timeline::drums_menu_view(cx, tl_drums_menu_open);

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
