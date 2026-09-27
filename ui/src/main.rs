mod app;
mod bpm_field;
mod canvas_text;
mod compressor_curve;
mod context_menu;
mod device_area;
mod eq_curve;
mod effect_panel;
mod fader;
mod fx_board;
mod glyph;
mod interval_input;
mod knob;
mod meter;
mod piano_roll;
mod pill;
mod project;
mod recorder;
mod settings;
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
use project::{ProjectEvent, ProjectModel};
use recorder::{RecorderModel, RecordingCoordinator};
use synth::state::{SynthEvent, SynthModel};
use timeline::state::{TimelineEvent, TimelineState};

fn main() -> Result<(), ApplicationError> {
    let (params, telemetry_tx, telemetry_rx) = shared::bridge();
    let synth_bridge = shared::synth::synth_bridge();
    let playback_bridge = shared::playback::playback_bridge();
    let recorder_bridge = shared::recorder::recorder_bridge();
    let record_params = std::sync::Arc::new(shared::recorder::RecordParams::new());
    let preferred_input_device = settings::load_input_device();
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
        preferred_input_device.as_deref(),
    );
    // Without audio there's nothing to run - but a panic is invisible when
    // launched from the desktop (the window just never appears), so say
    // why in a dialog first.
    let engine_handle = match engine_handle {
        Ok(handle) => handle,
        Err(e) => {
            let message = format!("Strata couldn't start audio: {}.\n\nCheck that an output device is connected and not held exclusively by another program.", e.to_string().trim_end_matches('.'));
            eprintln!("{message}");
            let _ = std::process::Command::new("zenity")
                .arg("--error")
                .arg("--title=Strata")
                .arg(format!("--text={message}"))
                .status();
            std::process::exit(1);
        }
    };
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
        let sample_counter = app_data.sample_counter;
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

        // The selected track: which instrument the panel shows, and where
        // pasted clips land. Shared by the synth and timeline models.
        let selected_track: Signal<Option<shared::arrangement::TrackId>> = Signal::new(None);
        // Which of the selected track's devices the lower panel shows -
        // Carve/empty by default; toggled to the Compressor by its own
        // chip. Not reset when the selection changes: if the newly
        // selected track has no Compressor, `device_area`'s own Panel
        // computation falls back to its instrument/empty state anyway.
        let viewing_effect: Signal<Option<shared::arrangement::EffectNodeId>> = Signal::new(None);
        // Which track's Effects Board is open in the lower panel, if any
        // - takes over from `device_area` there while set.
        // `Some(None)` is the master board open, `Some(Some(id))` is
        // track `id`'s, `None` is no board open at all.
        let board_open_track: Signal<Option<Option<shared::arrangement::TrackId>>> = Signal::new(None);
        let mut timeline_state =
            TimelineState::new(record_armed, playing, piano_roll_open_clip, piano_roll_selected, selected_track);
        let tl_arrangement = timeline_state.arrangement;
        let tl_transform = timeline_state.transform;
        let tl_snap = timeline_state.snap;
        let tl_selection = timeline_state.selection;
        let tl_playhead = timeline_state.playhead_ticks;
        let tl_tool = timeline_state.tool;
        let tl_drums_menu_open = timeline_state.drums_menu_open;
        let tl_context_menu = timeline_state.context_menu;
        let tl_clipboard_nonempty = timeline_state.clipboard_nonempty;
        let tl_missing_sources = timeline_state.missing_sources;
        let tl_fx_selected = timeline_state.fx_selected;
        let tl_renaming_marker = timeline_state.renaming_marker;
        let tl_renaming_track = timeline_state.renaming_track;

        // A saved project (if any) replaces the empty starting arrangement
        // before anything downstream reads it - the peak/decode loaders in
        // particular need the real clip list to know what to load.
        let startup_path = project::startup_path();
        let loaded_project = startup_path.as_deref().and_then(|p| shared::project::load(p).ok());
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
        let live_peaks = recorder_model.live_peaks;
        let selected_input_device = recorder_model.selected_input_device;
        let available_input_devices = recorder_model.available_input_devices;
        recorder_model.build(cx);

        let synth_model = SynthModel::new(
            synth_bridge.params_tx,
            synth_bridge.note_tx,
            synth_bridge.telemetry_rx,
            tl_arrangement,
            loaded_project.as_ref().map(|p| p.instruments.clone()).unwrap_or_default(),
            selected_track,
            tl_playhead,
        );
        let synth_state = synth_model.state;
        let synth_lfo_phases = (synth_model.lfo1_phase, synth_model.lfo2_phase);
        let synth_octave_shift = synth_model.octave_shift;
        let synth_meter_l = synth_model.meter_l;
        let synth_meter_r = synth_model.meter_r;
        let synth_help_open = synth_model.help_open;
        let synth_lfo_drag = synth_model.lfo_drag;
        let synth_patches = synth_model.patches;
        synth_model.build(cx);

        let project_model = ProjectModel::new(tl_arrangement, synth_patches, decode_request_tx.clone(), startup_path);
        let project_saved = project_model.saved;
        let project_name = project_model.display_name;
        project_model.build(cx);
        let save_status = Memo::new(move |_| {
            let edited = project::snapshot(&tl_arrangement.get(), &synth_patches.get()) != project_saved.get();
            if edited { "Edited".to_string() } else { "Saved".to_string() }
        });
        // "Project - Strata", with a leading "*" while there are unsaved
        // changes - the same edited-state the header shows, surfaced in the
        // taskbar/alt-tab too. The builder's static `.title()` is set
        // outside this closure, so the render timer pushes it as a window
        // event whenever it changes (a Binding's first emit lands before
        // the window exists, leaving the startup title stale).
        let window_title = Memo::new(move |_| {
            let dirty = if save_status.get() == "Edited" { "*" } else { "" };
            format!("{dirty}{} \u{2014} Strata", project_name.get())
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
        // the synth's animated modulation rings/scope, syncs
        // the timeline playhead from the transport's live position, and
        // schedules Carve to play whatever MIDI notes the playhead crossed.
        let last_tick = std::cell::Cell::new(Instant::now());
        let last_title = std::cell::RefCell::new(String::new());
        let loop_params = params.clone();
        let midi_scheduler = timeline::scheduler::MidiScheduler::new();
        let recording_coordinator = RecordingCoordinator::new();
        let render_timer = cx.add_timer(Duration::from_millis(16), None, move |cx, action| {
            if let TimerAction::Tick(_) = action {
                let now = Instant::now();
                let dt = (now - last_tick.get()).as_secs_f32().min(0.25);
                last_tick.set(now);

                cx.emit(AppEvent::Tick);
                let title = window_title.get();
                if *last_title.borrow() != title {
                    *last_title.borrow_mut() = title.clone();
                    cx.emit(WindowEvent::SetTitle(title));
                }
                // Sample-accurate, not `position_to_ticks(position.get())`
                // (that value only carries 16th-note resolution - fine
                // for the transport's text readout, but it made the
                // playhead visibly step once per 16th note instead of
                // gliding).
                let ticks = tl_arrangement.get().tempo_map.samples_to_ticks(sample_counter.get() as i64, engine_sample_rate);
                let is_playing = playing.get();
                cx.emit(TimelineEvent::SyncPlayhead { ticks, playing: is_playing });
                midi_scheduler.advance(cx, &tl_arrangement.get(), ticks, is_playing);
                cx.emit(SynthEvent::Tick(dt));

                // Latest-wins: cheap to rebuild every tick, and avoids
                // needing to dirty-track arrangement changes separately.
                let arr = tl_arrangement.get();
                // Automation applied at the playhead (a borrowed no-op when
                // no lane has a target).
                let plan = shared::playback::PlaybackPlan::from_arrangement(&arr.with_automation_at(ticks), engine_sample_rate);
                let _ = playback_plan_tx.borrow_mut().push(plan);

                // Same reasoning as the plan above: recomputed every tick
                // from ticks (which depend on the tempo map) rather than
                // threaded through as its own signal.
                if let Some(range) = arr.loop_range {
                    let start = arr.tempo_map.ticks_to_samples(range.start, engine_sample_rate);
                    let end = arr.tempo_map.ticks_to_samples(range.end, engine_sample_rate);
                    loop_params.set_loop(loop_on.get(), start, end);
                } else {
                    loop_params.set_loop(false, 0, 0);
                }

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
                KeymapEntry::new(10u8, |cx| if !text_input_focused(cx) { cx.emit(TimelineEvent::DeleteSelected) }),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::Backspace),
                KeymapEntry::new(11u8, |cx| if !text_input_focused(cx) { cx.emit(TimelineEvent::DeleteSelected) }),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::KeyF),
                KeymapEntry::new(12u8, |cx| if !text_input_focused(cx) { cx.emit(TimelineEvent::ToggleFollow) }),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::Escape),
                KeymapEntry::new(13u8, |cx| if !text_input_focused(cx) { cx.emit(PianoRollEvent::Close) }),
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
            (KeyChord::new(Modifiers::CTRL, Code::KeyC), KeymapEntry::new(18u8, |cx| cx.emit(TimelineEvent::Copy))),
            (KeyChord::new(Modifiers::SUPER, Code::KeyC), KeymapEntry::new(19u8, |cx| cx.emit(TimelineEvent::Copy))),
            (KeyChord::new(Modifiers::CTRL, Code::KeyX), KeymapEntry::new(20u8, |cx| cx.emit(TimelineEvent::Cut))),
            (KeyChord::new(Modifiers::SUPER, Code::KeyX), KeymapEntry::new(21u8, |cx| cx.emit(TimelineEvent::Cut))),
            (KeyChord::new(Modifiers::CTRL, Code::KeyV), KeymapEntry::new(22u8, |cx| cx.emit(TimelineEvent::Paste))),
            (KeyChord::new(Modifiers::SUPER, Code::KeyV), KeymapEntry::new(23u8, |cx| cx.emit(TimelineEvent::Paste))),
            (
                KeyChord::new(Modifiers::empty(), Code::Space),
                KeymapEntry::new(24u8, |cx| {
                    if !text_input_focused(cx) {
                        cx.emit(AppEvent::TogglePlay);
                        cx.emit(AppEvent::ReleaseButtonFocus);
                    }
                }),
            ),
            (
                KeyChord::new(Modifiers::empty(), Code::Home),
                KeymapEntry::new(25u8, |cx| if !text_input_focused(cx) { cx.emit(AppEvent::Rewind) }),
            ),
        ])
        .build(cx);

        let header_menus = transport::HeaderMenus::new();
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
                    selected_input_device,
                    available_input_devices,
                    cpu_load,
                    output_db,
                    arrangement: tl_arrangement,
                    save_status,
                    project_name,
                    menus: header_menus,
                },
                tl_bpm,
            );
            Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));

            HStack::new(cx, move |cx| {
                sidebar::sidebar(cx, synth_state, tl_arrangement, selected_track, sidebar_open);
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
                        live_peaks,
                        tl_missing_sources,
                        tl_tool,
                        selected_track,
                        loop_on,
                        tl_renaming_marker,
                        tl_renaming_track,
                        board_open_track,
                    );

                    Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));

                    // The lower panel spans the arrangement's width and sizes to
                    // its device; the device fills it rather than floating.
                    VStack::new(cx, move |cx| {
                        Binding::new(cx, board_open_track, move |cx| {
                            // A selection from a previous board (or none
                            // open) must never be what Delete removes.
                            tl_fx_selected.set(None);
                            if let Some(track) = board_open_track.get() {
                                fx_board::fx_board(
                                    cx,
                                    fx_board::FxBoardProps {
                                        theme,
                                        arrangement: tl_arrangement,
                                        track,
                                        board_open_track,
                                        selected: tl_fx_selected,
                                    },
                                );
                                return;
                            }
                            device_area::device_area(
                                cx,
                                device_area::DeviceAreaProps {
                                    theme,
                                    arrangement: tl_arrangement,
                                    selected_track,
                                    viewing_effect,
                                    synth_state,
                                    lfo_phases: synth_lfo_phases,
                                    octave_shift: synth_octave_shift,
                                    meter_l: synth_meter_l,
                                    meter_r: synth_meter_r,
                                    help_open: synth_help_open,
                                    lfo_drag: synth_lfo_drag,
                                    open_clip: piano_roll_open_clip,
                                    edit_mode: piano_roll_mode,
                                    label_mode: piano_roll_label_mode,
                                    selected_notes: piano_roll_selected,
                                    snap: tl_snap,
                                    playhead: tl_playhead,
                                    key: interval_key,
                                    scale_mask: interval_scale_mask,
                                    interval_open,
                                    show_note_names: interval_show_note_names,
                                },
                            );
                        });
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
            context_menu::context_menu_view(cx, tl_arrangement, tl_context_menu, tl_clipboard_nonempty);
            transport::header_menu_backdrop(cx, header_menus);
            // Selecting clips drops the Effects Board's node selection, so
            // Delete removes what was picked last (see TimelineState::fx_selected).
            Binding::new(cx, tl_selection, move |_cx| {
                if !tl_selection.get().clips.is_empty() {
                    tl_fx_selected.set(None);
                }
            });

        })
        .class("app")
        .toggle_class("theme-daylight", is_daylight)
        .height(Stretch(1.0))
        .width(Stretch(1.0));
    })
    .title("Strata")
    .inner_size((1600, 1360))
    // The header row is one fixed-content strip (no wrapping); below ~1400px
    // its right end (CPU/Out meters, input device) was cut off, and below
    // ~800px tall the timeline has no room once the 320px Effects Board
    // is open.
    .min_inner_size(Some((1400, 800)))
    .ignore_default_theme()
    .run()
}

/// True while a text field has keyboard focus - bare-key shortcuts (Space,
/// Delete, Backspace, F, Home, Escape) must not fire while the user is
/// typing a track name or a search. Every Textbox in the app is
/// `Textbox<Signal<String>, String>`, so one downcast covers them all.
pub(crate) fn text_input_focused(cx: &EventContext) -> bool {
    cx.get_view_with::<Textbox<Signal<String>, String>>(cx.focused()).is_some()
}
