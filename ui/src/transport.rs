//! The header (top bar): the project and its state, the key, tempo and
//! meter, the transport (rewind, stop, play, record), the position with
//! elapsed time, loop and metronome, the input level, and CPU/output
//! meters. `size-toolbar` tall on `bg-000`.

use crate::lessons::LessonTargetExt;
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
use crate::project::ProjectEvent;
use crate::recorder::RecorderModelEvent;
use crate::timeline::state::TimelineEvent;
use crate::tokens::{ThemeId, SPACE_1, SPACE_2, SPACE_3};

/// A bit taller than `tokens::SIZE_TOOLBAR` (which every *other* header -
/// Carve's, the piano roll's, Interval Input's - still uses): the app's
/// own top bar reads as the one thing everything else sits below, so it
/// gets a size of its own rather than sharing the generic device-header
/// token.
const INPUT_MENU_WIDTH: f32 = 220.0;
pub const HEADER_HEIGHT: f32 = 48.0;

/// The time signature picker's options - covers what anyone actually
/// picks; free-form numerator/denominator fields aren't worth the extra
/// UI for signatures this rare.
const TIME_SIGNATURE_PRESETS: &[(u8, u8)] = &[(4, 4), (3, 4), (2, 4), (6, 8), (5, 4), (7, 8), (9, 8), (12, 8)];

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
    /// The saved input device (`None` = OS default) and every device the
    /// host can currently see - see `recorder::RecorderModel`.
    pub selected_input_device: Signal<Option<std::sync::Arc<str>>>,
    pub available_input_devices: Signal<std::sync::Arc<[std::sync::Arc<str>]>>,
    pub cpu_load: Signal<f32>,
    pub output_db: Signal<f32>,
    pub arrangement: Signal<Arrangement>,
    /// "Saved" or "Edited".
    pub save_status: Memo<String>,
    /// The current project's display name (its file stem, or "Untitled"
    /// before its first save) - see `crate::project`.
    pub project_name: Signal<String>,
    pub menus: HeaderMenus,
    /// The live spectrum analyzer's strip is showing.
    pub analyzer_open: Signal<bool>,
}

/// Open/closed state of the header's drop-down menus (File, time
/// signature, input device). Created in `main.rs` rather than inside
/// `header()` so the click-outside backdrop can be mounted at the window
/// root - see `header_menu_backdrop`.
#[derive(Clone, Copy)]
pub struct HeaderMenus {
    pub file: Signal<bool>,
    pub time_sig: Signal<bool>,
    pub input_device: Signal<bool>,
}

impl HeaderMenus {
    pub fn new() -> Self {
        Self { file: Signal::new(false), time_sig: Signal::new(false), input_device: Signal::new(false) }
    }

    fn any_open(self) -> bool {
        self.file.get() || self.time_sig.get() || self.input_device.get()
    }

    fn close_all(self) {
        self.file.set(false);
        self.time_sig.set(false);
        self.input_device.set(false);
    }

    /// Opens/closes `which`, closing any other open menu - only one drop-
    /// down at a time.
    fn toggle(self, which: Signal<bool>) {
        let was_open = which.get();
        self.close_all();
        which.set(!was_open);
    }
}

/// Transparent full-window layer under the header's menus (z-index 170 vs
/// the menus' 180, same as the timeline's context menu): any click
/// outside an open menu closes it. Mounted at the window root, since the
/// header itself only spans the top strip.
pub fn header_menu_backdrop(cx: &mut Context, menus: HeaderMenus) {
    let any_open = Memo::new(move |_| menus.any_open());
    Element::new(cx)
        .class("context-menu-backdrop")
        .toggle_class("hidden", any_open.map(|o| !*o))
        .on_mouse_down(move |_cx, _| menus.close_all())
        .position_type(PositionType::Absolute)
        .top(Pixels(0.0))
        .left(Pixels(0.0))
        .width(Stretch(1.0))
        .height(Stretch(1.0));
}

fn vsep(cx: &mut Context) {
    Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(22.0));
}

/// One row of the File menu - same shape as the timeline's right-click
/// menu (`context_menu.rs`), but that one always closes via
/// `TimelineEvent::CloseContextMenu`, so this is its own copy rather than
/// a shared helper, parameterized on whichever `Signal<bool>` this menu
/// closes with.
fn file_menu_item(
    cx: &mut Context,
    label: &'static str,
    menu_open: Signal<bool>,
    action: impl Fn(&mut EventContext) + Send + Sync + Copy + 'static,
) {
    file_menu_item_with_shortcut(cx, label, "", menu_open, action);
}

/// A File-menu row with a right-aligned shortcut hint - same shape as the
/// timeline context menu's `item_with_shortcut`.
fn file_menu_item_with_shortcut(
    cx: &mut Context,
    label: &'static str,
    shortcut: &'static str,
    menu_open: Signal<bool>,
    action: impl Fn(&mut EventContext) + Send + Sync + Copy + 'static,
) {
    HStack::new(cx, move |cx| {
        // Children aren't hit-testable (same as Vizia's own Button does to
        // its content): `on_press` only fires when the press targets the row
        // itself, so a hoverable label made clicks on the text do nothing.
        Label::new(cx, label).class("body").hoverable(false);
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0)).hoverable(false);
        let shortcut = crate::shortcut(shortcut);
        if !shortcut.is_empty() {
            Label::new(cx, shortcut).class("value").hoverable(false);
        }
    })
    .class("menu-item")
    .gap(Pixels(SPACE_3))
    .on_press(move |cx| {
        action(cx);
        menu_open.set(false);
    })
    .cursor(CursorIcon::Hand)
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(28.0));
}

/// One row of the input-device menu - same shape as `file_menu_item`,
/// but the label is a runtime device name (not `&'static str`) and each
/// row lights up when it's the current selection. `device` is `None`
/// for the "Default" row, `Some(name)` for a specific device - both
/// just replace `RecorderModel::selected_input_device` wholesale.
fn input_device_menu_item(
    cx: &mut Context,
    label: &str,
    device: Option<std::sync::Arc<str>>,
    selected: Signal<Option<std::sync::Arc<str>>>,
    menu_open: Signal<bool>,
) {
    let label = label.to_string();
    let device_for_check = device.clone();
    let is_selected = Memo::new(move |_| selected.get() == device_for_check);
    HStack::new(cx, move |cx| {
        // Real device names ("HD-Audio Generic, ALC1220 Alt Analog") run
        // well past this menu's fixed width - without this they overflowed
        // straight off the edge of the window instead of staying inside
        // the menu's own box. Same `.text_wrap(false).text_overflow(...)`
        // pair Vizia's own `Select` widget uses for exactly this.
        // Not hit-testable, so a click on the text reaches the row's on_press.
        Label::new(cx, label.clone())
            .class("body")
            .hoverable(false)
            .text_wrap(false)
            .text_overflow(TextOverflow::Ellipsis)
            .width(Stretch(1.0));
    })
    .class("menu-item")
    .toggle_class("is-on", is_selected)
    .on_press(move |cx| {
        cx.emit(RecorderModelEvent::SetInputDevice(device.clone()));
        menu_open.set(false);
    })
    .cursor(CursorIcon::Hand)
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(28.0));
}

fn file_menu_sep(cx: &mut Context) {
    Element::new(cx).class("menu-sep").width(Stretch(1.0)).height(Pixels(1.0));
}

/// One transport button: a drawn glyph, its colour following its state.
/// A short hover label below `handle` - for icon-only or otherwise
/// unlabelled header controls.
fn with_tip<'a, V: View>(handle: Handle<'a, V>, text: &'static str) -> Handle<'a, V> {
    handle.tooltip(move |cx| {
        Tooltip::new(cx, move |cx| {
            Label::new(cx, text);
        })
        .placement(Placement::Bottom)
        .arrow(false)
    })
}

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
    let HeaderProps { theme, playing, loop_on, record_armed, click_on, position, interval_open, project_name, .. } =
        props;
    let menus = props.menus;
    let file_menu_open = menus.file;
    let input_device_menu_open = menus.input_device;
    // Where the input menu opens: under its button (header coordinates).
    let input_menu_left: Signal<f32> = Signal::new(0.0);
    let renaming: Signal<bool> = Signal::new(false);
    let rename_draft: Signal<String> = Signal::new(project_name.get());

    HStack::new(cx, move |cx| {
        // Project and what's happening to it.
        let status = Memo::new(move |_| {
            let arr = props.arrangement.get();
            if record_armed.get() {
                match arr.tracks.iter().find(|t| t.arm) {
                    Some(track) if playing.get() => return format!("Recording to {}", track.name),
                    Some(_) => {}
                    // Record on with no track armed records nothing - say
                    // so, instead of silently playing back.
                    None => return "Arm a track to record".to_string(),
                }
            }
            props.save_status.get()
        });
        VStack::new(cx, move |cx| {
            Binding::new(cx, renaming, move |cx| {
                if renaming.get() {
                    Textbox::new(cx, rename_draft)
                        .class("title")
                        .class("search")
                        .font_size(14.0)
                        .on_edit(move |_cx, text| rename_draft.set(text))
                        .on_submit(move |cx, text, _from_key| {
                            cx.emit(ProjectEvent::Rename(text));
                            renaming.set(false);
                        })
                        .on_cancel(move |_cx| renaming.set(false))
                        .width(Pixels(126.0));
                } else {
                    Button::new(cx, move |cx| Label::new(cx, project_name).class("title").font_size(14.0))
                        .class("btn")
                        .class("quiet")
                        .on_press(move |_cx| menus.toggle(file_menu_open));
                }
            });
            Label::new(cx, status).class("value").font_size(12.0);
        })
        .width(Pixels(120.0))
        .height(Auto);

        // The File menu. Built once and toggled with `.hidden` (`display: none`, same as
        // every other overlay in the app) rather than conditionally
        // constructed via `Binding` - a freshly built entity is a plausible
        // reason a click landing right as it appears wouldn't resolve to
        // it correctly.
        VStack::new(cx, move |cx| {
            file_menu_item(cx, "New", file_menu_open, |cx| cx.emit(ProjectEvent::New));
            file_menu_item(cx, "Open...", file_menu_open, |cx| cx.emit(ProjectEvent::OpenDialog));
            file_menu_sep(cx);
            file_menu_item_with_shortcut(cx, "Save", "Ctrl+S", file_menu_open, |cx| cx.emit(ProjectEvent::Save));
            file_menu_item(cx, "Save As...", file_menu_open, |cx| cx.emit(ProjectEvent::SaveAsDialog));
            file_menu_item(cx, "Export Audio...", file_menu_open, |cx| cx.emit(ProjectEvent::ExportDialog));
            file_menu_item(cx, "Start Lesson 1", file_menu_open, |cx| cx.emit(ProjectEvent::StartLesson(0)));
            file_menu_sep(cx);
            // Undo/Redo were keyboard-only; listed here (with their keys)
            // so they're discoverable.
            file_menu_item_with_shortcut(cx, "Undo", "Ctrl+Z", file_menu_open, |cx| cx.emit(TimelineEvent::Undo));
            file_menu_item_with_shortcut(cx, "Redo", "Ctrl+Shift+Z", file_menu_open, |cx| cx.emit(TimelineEvent::Redo));
            file_menu_sep(cx);
            file_menu_item(cx, "Rename...", file_menu_open, move |_cx| {
                rename_draft.set(project_name.get());
                renaming.set(true);
            });
        })
        .class("panel")
        .class("context-menu")
        .toggle_class("hidden", file_menu_open.map(|o| !*o))
        .position_type(PositionType::Absolute)
        .top(Pixels(HEADER_HEIGHT))
        .left(Pixels(SPACE_3))
        .gap(Pixels(2.0))
        .padding_top(Pixels(SPACE_2))
        .padding_bottom(Pixels(SPACE_2))
        .padding_left(Pixels(SPACE_1))
        .padding_right(Pixels(SPACE_1))
        .width(Pixels(200.0))
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

        // Right-click for an exact typed value - drag/scroll (BpmField's
        // own gesture) is great for coarse changes but painfully slow (or
        // imprecise) for jumping to a specific number like 174.
        let editing_bpm: Signal<bool> = Signal::new(false);
        let bpm_draft: Signal<String> = Signal::new(String::new());
        HStack::new(cx, move |cx| {
            // Nudge by 1 BPM - a real click target, not a drag gesture,
            // for the "just move it a couple BPM" case drag/scroll are
            // fiddly for.
            Button::new(cx, |cx| Label::new(cx, "\u{2212}").font_size(13.0))
                .class("btn")
                .class("quiet")
                .width(Pixels(16.0))
                .height(Pixels(18.0))
                .on_press(move |cx| {
                    let current = props.arrangement.get().tempo_map.bpm_at(0);
                    let v = (current - 1.0).clamp(crate::bpm_field::MIN_BPM, crate::bpm_field::MAX_BPM);
                    cx.emit(TimelineEvent::SetTempo(v));
                    cx.emit(AppEvent::SetBpm(v));
                });
            Binding::new(cx, editing_bpm, move |cx| {
                if editing_bpm.get() {
                    Textbox::new(cx, bpm_draft)
                        .class("search")
                        .class("editing")
                        .font_size(13.0)
                        .on_edit(move |_cx, text| bpm_draft.set(text))
                        .on_submit(move |cx, text, _from_key| {
                            if let Ok(v) = text.trim().parse::<f64>() {
                                let v = v.clamp(crate::bpm_field::MIN_BPM, crate::bpm_field::MAX_BPM);
                                cx.emit(TimelineEvent::SetTempo(v));
                                cx.emit(AppEvent::SetBpm(v));
                            }
                            editing_bpm.set(false);
                        })
                        .on_cancel(move |_cx| editing_bpm.set(false))
                        .width(Pixels(52.0))
                        .height(Pixels(18.0));
                } else {
                    BpmField::new(cx, bpm, theme, |cx, v| {
                        cx.emit(TimelineEvent::SetTempo(v));
                        cx.emit(AppEvent::SetBpm(v));
                    })
                    .width(Pixels(52.0))
                    .height(Pixels(18.0))
                    .on_mouse_down(move |_cx, button| {
                        if button == MouseButton::Right {
                            // Not `bpm.get()`: it's a generic
                            // `impl SignalGet<f64>` with no `Send`/`Sync`
                            // bound, which `on_mouse_down`'s callback
                            // requires - `props.arrangement` is a
                            // concrete `Signal<Arrangement>` (both), and
                            // reads the identical value either way.
                            let current = props.arrangement.get().tempo_map.bpm_at(0);
                            bpm_draft.set(format!("{current:.0}"));
                            editing_bpm.set(true);
                        }
                    });
                }
            });
            Button::new(cx, |cx| Label::new(cx, "+").font_size(13.0))
                .class("btn")
                .class("quiet")
                .width(Pixels(16.0))
                .height(Pixels(18.0))
                .on_press(move |cx| {
                    let current = props.arrangement.get().tempo_map.bpm_at(0);
                    let v = (current + 1.0).clamp(crate::bpm_field::MIN_BPM, crate::bpm_field::MAX_BPM);
                    cx.emit(TimelineEvent::SetTempo(v));
                    cx.emit(AppEvent::SetBpm(v));
                });
        })
        .class("readout")
        .alignment(Alignment::Center)
        .size(Auto)
        .gap(Pixels(2.0));
        Label::new(cx, "BPM").class("value").font_size(12.0);
        Button::new(cx, |cx| Label::new(cx, "Tap").font_size(13.0))
            .class("btn")
            .class("quiet")
            .on_press(|cx| cx.emit(AppEvent::Tap));

        let time_sig_menu_open = menus.time_sig;
        let time_sig_text = Memo::new(move |_| {
            let sig = props.arrangement.get().tempo_map.time_signature_at(0);
            format!("{}/{}", sig.numerator, sig.denominator)
        });
        Button::new(cx, move |cx| Label::new(cx, time_sig_text).font_size(13.0))
            .class("readout")
            .on_press(move |_cx| menus.toggle(time_sig_menu_open));

        // A common preset list rather
        // than free-form numerator/denominator fields, since those are
        // the overwhelming majority of what anyone actually picks.
        VStack::new(cx, move |cx| {
            for &(num, den) in TIME_SIGNATURE_PRESETS {
                let label = format!("{num}/{den}");
                let is_current = Memo::new(move |_| {
                    let sig = props.arrangement.get().tempo_map.time_signature_at(0);
                    sig.numerator == num && sig.denominator == den
                });
                HStack::new(cx, move |cx| {
                    // Not hit-testable, so a click on the text reaches the row.
                    Label::new(cx, label.clone()).class("body").hoverable(false);
                })
                .class("menu-item")
                .toggle_class("is-on", is_current)
                .on_press(move |cx| {
                    cx.emit(TimelineEvent::SetTimeSignature { numerator: num, denominator: den });
                    time_sig_menu_open.set(false);
                })
                .cursor(CursorIcon::Hand)
                .alignment(Alignment::Left)
                .width(Stretch(1.0))
                .height(Pixels(28.0));
            }
        })
        .class("panel")
        .class("context-menu")
        .toggle_class("hidden", time_sig_menu_open.map(|o| !*o))
        .position_type(PositionType::Absolute)
        .top(Pixels(HEADER_HEIGHT))
        .left(Pixels(320.0))
        .gap(Pixels(2.0))
        .padding_top(Pixels(SPACE_2))
        .padding_bottom(Pixels(SPACE_2))
        .padding_left(Pixels(SPACE_1))
        .padding_right(Pixels(SPACE_1))
        .width(Pixels(100.0))
        .height(Auto);

        // The transport, grouped.
        HStack::new(cx, move |cx| {
            let never = Signal::new(false);
            let ink = |p: &crate::tokens::Palette, _on: bool| p.ink;
            with_tip(transport_button(cx, theme, GlyphKind::Rewind, never, ink), "Return to start (Home)")
                .on_press(|cx| cx.emit(AppEvent::Rewind));
            with_tip(transport_button(cx, theme, GlyphKind::Stop, never, ink), "Stop").on_press(|cx| cx.emit(AppEvent::Stop));
            with_tip(transport_button(cx, theme, GlyphKind::Play, playing, |p, on| if on { p.on_signal } else { p.ink }), "Play / stop (Space)")
                .toggle_class("is-play", playing)
                .lesson_target(crate::lessons::Target::Play)
                .on_press(|cx| cx.emit(AppEvent::TogglePlay));
            with_tip(transport_button(cx, theme, GlyphKind::Record, record_armed, |p, on| if on { p.on_record } else { p.ink }), "Arm recording")
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

        with_tip(transport_button(cx, theme, GlyphKind::Loop, loop_on, |p, on| if on { p.md } else { p.ink }), "Loop")
            .toggle_class("is-mod", loop_on)
            .on_press(|cx| cx.emit(AppEvent::ToggleLoop));
        with_tip(transport_button(cx, theme, GlyphKind::Metronome, click_on, |p, _| p.ink), "Metronome")
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
            with_tip(
                Knob::plain(cx, props.input_gain_pos, 0.5, theme, |cx, p| cx.emit(RecorderModelEvent::SetInputGain(p))),
                "Input gain",
            )
            .size(Pixels(22.0));

            // Which physical input actually gets opened - the OS's own
            // "default" is otherwise the only option, which silently
            // records from the wrong interface if that's not the one
            // really wired up (e.g. a laptop's webcam mic outranking a
            // real audio interface). Switches immediately (see
            // `EngineHandle::switch_input`).
            let device_label = Memo::new(move |_| {
                props.selected_input_device.get().map(|d| d.to_string()).unwrap_or_else(|| "Default".to_string())
            });
            // Capped with an ellipsis: device names are arbitrary OS strings,
            // and an uncapped one widened the header past the window edge.
            // A drop-down like Key's: mic icon, the device, a chevron.
            with_tip(
                Button::new(cx, move |cx| {
                    HStack::new(cx, move |cx| {
                        Glyph::new(cx, GlyphKind::Mic, Signal::new(true), theme, crate::glyph::ink_when_on)
                            .size(Pixels(13.0))
                            .hoverable(false);
                        Label::new(cx, device_label)
                            .font_size(13.0)
                            .text_wrap(false)
                            .text_overflow(TextOverflow::Ellipsis)
                            .max_width(Pixels(130.0))
                            .hoverable(false);
                        Label::new(cx, "\u{2304}").font_size(13.0).hoverable(false);
                    })
                    .gap(Pixels(SPACE_2))
                    .alignment(Alignment::Center)
                    .size(Auto)
                    .hoverable(false)
                }),
                "Recording input device",
            )
                .class("btn")
                .on_press(move |cx| {
                    // Opening: list again, so a just-plugged-in interface is there.
                    if !input_device_menu_open.get() {
                        cx.emit(crate::recorder::RecorderModelEvent::RefreshInputDevices);
                        // Under the button, kept inside the window. The
                        // header starts right of the sidebar's rail.
                        let header_left = crate::browser::RAIL_WIDTH + 1.0;
                        use crate::hidpi::Logical;
                        let window_w = cx.with_current(Entity::root(), |cx| cx.lbounds().w);
                        let x = cx.lbounds().x - header_left;
                        input_menu_left.set(x.min(window_w - header_left - INPUT_MENU_WIDTH - SPACE_3).max(0.0));
                    }
                    menus.toggle(input_device_menu_open);
                });
        })
        .gap(Pixels(SPACE_2))
        .alignment(Alignment::Center)
        .size(Auto);

        // Opens under its button (see its on_press).
        VStack::new(cx, move |cx| {
            input_device_menu_item(cx, "Default", None, props.selected_input_device, input_device_menu_open);
            for device in props.available_input_devices.get().iter() {
                input_device_menu_item(
                    cx,
                    device,
                    Some(device.clone()),
                    props.selected_input_device,
                    input_device_menu_open,
                );
            }
        })
        .class("panel")
        .class("context-menu")
        .toggle_class("hidden", input_device_menu_open.map(|o| !*o))
        .position_type(PositionType::Absolute)
        .top(Pixels(HEADER_HEIGHT))
        .left(input_menu_left.map(|x| Pixels(*x)))
        .gap(Pixels(2.0))
        .padding_top(Pixels(SPACE_2))
        .padding_bottom(Pixels(SPACE_2))
        .padding_left(Pixels(SPACE_1))
        .padding_right(Pixels(SPACE_1))
        .width(Pixels(INPUT_MENU_WIDTH))
        .height(Auto);

        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

        // The live spectrum analyzer, beside the other output readouts.
        with_tip(
            Button::new(cx, |cx| Label::new(cx, "Spectrum")).class("btn").class("quiet").toggle_class("is-on", props.analyzer_open),
            "Show what's playing, from low to high pitch",
        )
        .on_press(|cx| cx.emit(crate::analyzer::AnalyzerEvent::Toggle));

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

        // Output level: signal up to -12 dB, warn above. Labelled like CPU
        // beside it - unlabelled, a bare bar and "-inf" didn't say what
        // they measured.
        Label::new(cx, "Out").class("label").font_size(12.0);
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
    // Tighter than SPACE_2: the sidebar's rail now runs up beside the
    // header, and everything still has to fit at the default 1440px.
    .gap(Pixels(6.0))
    .padding_left(Pixels(SPACE_3))
    .padding_right(Pixels(SPACE_3))
    .alignment(Alignment::Left)
    .height(Pixels(HEADER_HEIGHT))
    .width(Stretch(1.0));
}
