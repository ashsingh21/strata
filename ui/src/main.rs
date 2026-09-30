mod analyzer;
mod app;
mod browser;
mod preview_player;
mod bpm_field;
mod canvas_text;
mod compressor_curve;
mod context_menu;
mod device_area;
mod dialogs;
mod drum_kit_panel;
mod eq_curve;
mod effect_panel;
mod fader;
mod fx_board;
mod glyph;
mod hidpi;
mod interval_input;
mod key_menu;
mod menu;
mod paths;
mod knob;
mod lessons;
mod logging;
mod meter;
mod piano_roll;
mod pill;
mod project;
mod recorder;
mod settings;
mod sidebar;
mod splitter;
mod status;
mod synth;
mod timeline;
mod tokens;
mod user_presets;
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
    // First: the log file and crash reports, so anything after is recorded.
    logging::init();
    let (params, telemetry_tx, telemetry_rx) = shared::bridge();
    let synth_bridge = shared::synth::synth_bridge();
    let playback_bridge = shared::playback::playback_bridge();
    let recorder_bridge = shared::recorder::recorder_bridge();
    let preview_bridge = shared::playback::preview_bridge();
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
        shared::playback::PreviewEnds {
            play_rx: preview_bridge.play_rx,
            retired_tx: preview_bridge.retired_tx,
            analyzer_tx: preview_bridge.analyzer_tx,
            gain: preview_bridge.gain.clone(),
        },
    );
    // Without audio there's nothing to run - but a panic is invisible when
    // launched from the desktop (the window just never appears), so say
    // why in a dialog first.
    let engine_handle = match engine_handle {
        Ok(handle) => handle,
        Err(e) => {
            let message = format!("Shor couldn't start audio: {}.\n\nCheck that an output device is connected and not held exclusively by another program.", e.to_string().trim_end_matches('.'));
            eprintln!("{message}");
            dialogs::error(&message);
            std::process::exit(1);
        }
    };
    let engine_sample_rate = engine_handle.sample_rate;
    let analyzer_rx = std::cell::Cell::new(Some(preview_bridge.analyzer_rx));
    let preview_player = crate::preview_player::PreviewPlayer::new(shared::playback::PreviewSender {
        play_tx: preview_bridge.play_tx,
        retired_rx: preview_bridge.retired_rx,
        gain: preview_bridge.gain,
    }, engine_sample_rate);
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
        cx.add_stylesheet(include_style!("styles/themes.css")).expect("failed to add themes.css");
        // Tooltips after 0.4 s, not Vizia's 1.5 s: they're how the icon
        // rail and the quieter controls explain themselves.
        cx.emit(EnvironmentEvent::SetTooltipDelay(Duration::from_millis(400)));

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

        let zoom = app_data.zoom;
        let app_data_bus_peaks = app_data.bus_peaks;
        app_data.build(cx);
        // The zoom from last time.
        cx.emit(AppEvent::ApplyZoom);

        let piano_roll_model = PianoRollModel::new();
        let piano_roll_open_clip = piano_roll_model.open_clip;
        let piano_roll_mode = piano_roll_model.mode;
        let piano_roll_label_mode = piano_roll_model.label_mode;
        let piano_roll_selected = piano_roll_model.selected;
        let piano_roll_octave = piano_roll_model.octave;
        let piano_roll_chord = piano_roll_model.chord;
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
        let tl_context_menu = timeline_state.context_menu;
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
            engine_sample_rate,
        );
        // The Drum Kit's samples, loaded up front: a kit can be added to
        // any track at any time and must play on its first note.
        for pad in &shared::drums::DRUM_KIT {
            let _ = decode_request_tx.send(pad.sample.into());
        }
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
            app_data_bus_peaks,
        );
        let synth_state = synth_model.state;
        timeline::header::set_track_levels(synth_model.track_levels);
        let synth_lfo_phases = (synth_model.lfo1_phase, synth_model.lfo2_phase);
        let synth_octave_shift = synth_model.octave_shift;
        let synth_meter_l = synth_model.meter_l;
        let synth_meter_r = synth_model.meter_r;
        let synth_help_open = synth_model.help_open;
        let synth_lfo_drag = synth_model.lfo_drag;
        let synth_user_presets = synth_model.user_presets;
        let synth_patches = synth_model.patches;
        synth_model.build(cx);

        let project_model = ProjectModel::new(tl_arrangement, synth_patches, decode_request_tx.clone(), startup_path);
        let project_saved = project_model.saved;
        let project_name = project_model.display_name;
        let export_status = project_model.export_status;
        let my_tracks = project_model.my_tracks;
        let project_path = project_model.current_path;
        project_model.build(cx);
        // Crashed last time? Unsaved work left behind? (Asked once the
        // window is up.)
        cx.emit(project::ProjectEvent::StartupChecks);
        let save_status = Memo::new(move |_| {
            let edited = project::snapshot(&tl_arrangement.get(), &synth_patches.get()) != project_saved.get();
            if edited { "Edited".to_string() } else { "Saved".to_string() }
        });
        // "Project - Shor", with a leading "*" while there are unsaved
        // changes - the same edited-state the header shows, surfaced in the
        // taskbar/alt-tab too. The builder's static `.title()` is set
        // outside this closure, so the render timer pushes it as a window
        // event whenever it changes (a Binding's first emit lands before
        // the window exists, leaving the startup title stale).
        let window_title = Memo::new(move |_| {
            let dirty = if save_status.get() == "Edited" { "*" } else { "" };
            format!("{dirty}{} \u{2014} Shor", project_name.get())
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

        // Before any view is built: it also publishes the highlight signal
        // that `lesson_target` glows read.
        let analyzer_model = analyzer::AnalyzerModel::new(analyzer_rx.take().expect("the app is built once"), engine_sample_rate);
        let analyzer_open = analyzer_model.open;
        let analyzer_props = analyzer::AnalyzerProps::of(&analyzer_model);
        analyzer_model.build(cx);
        let lesson_model = lessons::LessonModel::new(
            tl_arrangement,
            selected_track,
            playing,
            synth_state,
            piano_roll_open_clip,
            tl_playhead,
            synth_patches,
            interval_key,
            interval_scale_mask,
            export_status,
            analyzer_open,
            tl_snap,
            preview_player.clone(),
            engine_sample_rate,
        );
        let lesson_bar_props = lessons::bar::LessonBarProps::of(&lesson_model, theme);
        let lower_panel_height =
            Signal::new(settings::load_lower_panel_height().unwrap_or(splitter::DEFAULT_PANEL_HEIGHT));
        let lessons_active = lesson_model.active;
        let lessons_done = lesson_model.done;
        lesson_model.build(cx);

        // The sidebar: rail, browser panel, preview dock.
        let browser_model = browser::BrowserModel::new(
            sidebar_open,
            tl_arrangement,
            selected_track,
            my_tracks,
            project_path,
            browser::preview::BrowserPreview::new(preview_player.clone(), tl_arrangement, interval_key, interval_scale_mask),
        );
        let browser_props =
            browser::view::BrowserProps::of(&browser_model, theme, interval_key, interval_scale_mask, interval_open, lessons_active, lessons_done, zoom, lesson_bar_props);
        // Whether the lesson panel (the sidebar on Learn) is on screen.
        let (browser_open, browser_section) = (browser_model.open, browser_model.section);
        let lesson_panel_shown = Memo::new(move |_| browser_open.get() && browser_section.get() == browser::Section::Learn);
        browser_model.build(cx);
        browser::view::DragTracker.build(cx);
        browser::start_key_analysis(cx);


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
                cx.emit(analyzer::AnalyzerEvent::Tick);
                cx.emit(browser::BrowserEvent::Tick);
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
                cx.emit(lessons::LessonEvent::Tick);
                cx.emit(project::ProjectEvent::Tick);

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
                KeyChord::new(Modifiers::CTRL | Modifiers::SHIFT, Code::KeyD),
                KeymapEntry::new(26u8, |cx| cx.emit(TimelineEvent::DuplicateLinked)),
            ),
            (
                KeyChord::new(Modifiers::SUPER | Modifiers::SHIFT, Code::KeyD),
                KeymapEntry::new(27u8, |cx| cx.emit(TimelineEvent::DuplicateLinked)),
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
            (KeyChord::new(Modifiers::CTRL, Code::KeyF), KeymapEntry::new(28u8, |cx| cx.emit(browser::BrowserEvent::FocusSearch))),
            (KeyChord::new(Modifiers::SUPER, Code::KeyF), KeymapEntry::new(29u8, |cx| cx.emit(browser::BrowserEvent::FocusSearch))),
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
            // Zoom the whole UI: Ctrl/Cmd + (or =), -, 0.
            (KeyChord::new(Modifiers::CTRL, Code::Equal), KeymapEntry::new(30u8, |cx| cx.emit(AppEvent::Zoom(1)))),
            (KeyChord::new(Modifiers::SUPER, Code::Equal), KeymapEntry::new(31u8, |cx| cx.emit(AppEvent::Zoom(1)))),
            (KeyChord::new(Modifiers::CTRL | Modifiers::SHIFT, Code::Equal), KeymapEntry::new(32u8, |cx| cx.emit(AppEvent::Zoom(1)))),
            (KeyChord::new(Modifiers::SUPER | Modifiers::SHIFT, Code::Equal), KeymapEntry::new(33u8, |cx| cx.emit(AppEvent::Zoom(1)))),
            (KeyChord::new(Modifiers::CTRL, Code::Minus), KeymapEntry::new(34u8, |cx| cx.emit(AppEvent::Zoom(-1)))),
            (KeyChord::new(Modifiers::SUPER, Code::Minus), KeymapEntry::new(35u8, |cx| cx.emit(AppEvent::Zoom(-1)))),
            (KeyChord::new(Modifiers::CTRL, Code::Digit0), KeymapEntry::new(36u8, |cx| cx.emit(AppEvent::ResetZoom))),
            (KeyChord::new(Modifiers::SUPER, Code::Digit0), KeymapEntry::new(37u8, |cx| cx.emit(AppEvent::ResetZoom))),
            (
                KeyChord::new(Modifiers::empty(), Code::Home),
                KeymapEntry::new(25u8, |cx| if !text_input_focused(cx) { cx.emit(AppEvent::Rewind) }),
            ),
        ])
        .build(cx);

        VStack::new(cx, move |cx| {
            // The rail runs the full height; the header and everything
            // under it sit to its right.
            HStack::new(cx, move |cx| {
            browser::view::rail_column(cx, browser_props, transport::HEADER_HEIGHT);
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
                    analyzer_open,
                },
                tl_bpm,
            );
            Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));

            HStack::new(cx, move |cx| {
                browser::view::panel_area(cx, browser_props);

                VStack::new(cx, move |cx| {
                    lessons::bar::lesson_bar(cx, lesson_bar_props, lesson_panel_shown);
                    analyzer::analyzer_strip(cx, analyzer_props, theme);
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

                    // Drag to trade height between the timeline and the panel.
                    splitter::PanelSplitter::new(cx, lower_panel_height, theme);

                    // The lower panel spans the arrangement's width, is as
                    // tall as the divider says, and scrolls when its device
                    // is taller than that (a laptop screen).
                    ScrollView::new(cx, move |cx| {
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
                                        playhead: tl_playhead,
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
                                    user_presets: synth_user_presets,
                                    open_clip: piano_roll_open_clip,
                                    edit_mode: piano_roll_mode,
                                    label_mode: piano_roll_label_mode,
                                    selected_notes: piano_roll_selected,
                                    rows_octave: piano_roll_octave,
                                    draw_chord: piano_roll_chord,
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
                    .show_horizontal_scrollbar(false)
                    .class("lower-scroll")
                    // A browser result dropped on the device panel: onto
                    // the selected track.
                    .on_drop(move |cx, _| {
                        if let Some(item) = browser::view::dragged() {
                            cx.emit(browser::BrowserEvent::DropOnTrack { item: item.id, track: selected_track.get(), tick: 0 });
                        }
                    })
                    .width(Stretch(1.0))
                    .height(lower_panel_height.map(|h| Pixels(*h)));
                })
                .width(Stretch(1.0))
                .height(Stretch(1.0));
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));

            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));
            })
            .width(Stretch(1.0))
            .height(Stretch(1.0));

            Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0));
            sidebar::status_bar(cx, sample_rate, block_frames, status_touched, save_status, export_status);

            context_menu::context_menu_view(cx, tl_arrangement, tl_context_menu, synth_state);
            // A dragged browser result's name, following the pointer.
            browser::view::DragGhost::new(cx, browser_props);
            // The key menu's one dropdown, over everything (see key_menu).
            key_menu::host(cx, interval_key, interval_scale_mask, interval_open);
            timeline::snap_menu_host(cx, tl_snap);
            synth::preset_menu_host(cx, synth_state, synth_user_presets);
            app::restyle_anchor(cx);
            // Selecting clips drops the Effects Board's node selection, so
            // Delete removes what was picked last (see TimelineState::fx_selected).
            Binding::new(cx, tl_selection, move |_cx| {
                if !tl_selection.get().clips.is_empty() {
                    tl_fx_selected.set(None);
                }
            });

        })
        .class("app")
        // The theme's class on the root switches its colours on.
        .toggle_class("theme-daylight", theme.map(|t| *t == tokens::ThemeId::Daylight))
        .toggle_class("theme-midnight", theme.map(|t| *t == tokens::ThemeId::Midnight))
        .toggle_class("theme-contrast", theme.map(|t| *t == tokens::ThemeId::Contrast))
        .toggle_class("theme-paper", theme.map(|t| *t == tokens::ThemeId::Paper))
        .height(Stretch(1.0))
        .width(Stretch(1.0));
    })
    .title("Shor")
    // Opens filling the screen (the space between the menu bar and the
    // Dock on a Mac), whatever its size: 1440x900 ran off the bottom of a
    // 13" MacBook. The size is for when it's un-maximized.
    .inner_size((1440, 900))
    .maximized(true)
    // The header row is one fixed-content strip (no wrapping): it drops
    // its elapsed time and CPU meter when narrow, which fits it down to
    // 1280px - the smallest a 13" Mac offers. Below ~700px tall the
    // timeline has no room once the 320px Effects Board is open.
    .min_inner_size(Some((1280, 700)))
    .ignore_default_theme()
    .run()
}

/// True while a text field has keyboard focus - bare-key shortcuts (Space,
/// Delete, Backspace, F, Home, Escape) must not fire while the user is
/// typing a track name or a search. Every Textbox in the app is
/// `Textbox<Signal<String>, String>`, so one downcast covers them all.
/// A menu's shortcut hint in the platform's own style: "Ctrl+Shift+Z"
/// everywhere but macOS, "shift-cmd-Z" as symbols there (every binding
/// exists with both Ctrl and Cmd; see the keymap).
pub fn shortcut(label: &'static str) -> &'static str {
    #[cfg(target_os = "macos")]
    {
        return match label {
            "Ctrl+S" => "\u{2318}S",
            "Ctrl+Z" => "\u{2318}Z",
            "Ctrl+Shift+Z" => "\u{21e7}\u{2318}Z",
            "Ctrl+X" => "\u{2318}X",
            "Ctrl+C" => "\u{2318}C",
            "Ctrl+V" => "\u{2318}V",
            "Ctrl+D" => "\u{2318}D",
            "Ctrl+Shift+D" => "\u{21e7}\u{2318}D",
            "Ctrl+F" => "\u{2318}F",
            other => other,
        };
    }
    #[cfg(not(target_os = "macos"))]
    label
}

pub(crate) fn text_input_focused(cx: &EventContext) -> bool {
    cx.get_view_with::<Textbox<Signal<String>, String>>(cx.focused()).is_some()
}
