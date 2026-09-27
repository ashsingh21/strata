//! Track header column: plain Vizia views (not canvas) styled through the
//! stylesheet, reusing the milestone-1 M/S/Arm buttons and colour swatch.

use vizia::prelude::*;
use vizia::vg;

use shared::arrangement::{
    Arrangement, AutomationLaneId, ClipColor, EffectNode, TrackId, TrackKind, DEFAULT_TRACK_HEIGHT,
    MAX_TRACK_HEIGHT, MIN_TRACK_HEIGHT,
};

use crate::fader::{Fader, FaderModifiers};
use crate::timeline::state::{ContextMenu, ContextMenuTarget, TimelineEvent};
use crate::tokens::{self, ThemeId};

/// Height of the drag-to-resize strip at a track header's bottom edge.
const RESIZE_HANDLE_PX: f32 = 6.0;

/// A thin strip along a track header's bottom edge: drag to resize the
/// track's lane, double-click to reset it. Height in px isn't normalized
/// (unlike [`Fader`]/`Knob`) - the callback gets the new height directly.
struct TrackResizeHandle<V: SignalGet<f32> + Copy + 'static> {
    value: V,
    theme: Signal<ThemeId>,
    is_dragging: bool,
    hovered: bool,
    prev_drag_y: f32,
    on_changing: Option<Box<dyn Fn(&mut EventContext, f32)>>,
}

impl<V: SignalGet<f32> + Copy + 'static> TrackResizeHandle<V> {
    fn new(
        cx: &mut Context,
        value: V,
        theme: Signal<ThemeId>,
        on_changing: impl 'static + Fn(&mut EventContext, f32),
    ) -> Handle<'_, Self> {
        Self {
            value,
            theme,
            is_dragging: false,
            hovered: false,
            prev_drag_y: 0.0,
            on_changing: Some(Box::new(on_changing)),
        }
        .build(cx, |_| {})
        .bind(value, |mut h| h.needs_redraw())
        .bind(theme, |mut h| h.needs_redraw())
        .cursor(CursorIcon::RowResize)
    }
}

impl<V: SignalGet<f32> + Copy + 'static> View for TrackResizeHandle<V> {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|window_event, _| match window_event {
            WindowEvent::MouseDown(button) if *button == MouseButton::Left => {
                self.is_dragging = true;
                self.prev_drag_y = cx.mouse().left.pos_down.1;
                cx.capture();
                cx.focus_with_visibility(false);
            }
            WindowEvent::MouseUp(button) if *button == MouseButton::Left => {
                self.is_dragging = false;
                cx.release();
            }
            WindowEvent::MouseMove(_, y) => {
                if self.is_dragging {
                    let delta = *y - self.prev_drag_y;
                    self.prev_drag_y = *y;
                    let new_height = (self.value.get() + delta).clamp(MIN_TRACK_HEIGHT, MAX_TRACK_HEIGHT);
                    if let Some(cb) = &self.on_changing {
                        cb(cx, new_height);
                    }
                }
            }
            WindowEvent::MouseDoubleClick(button) if *button == MouseButton::Left => {
                self.is_dragging = false;
                if let Some(cb) = &self.on_changing {
                    cb(cx, DEFAULT_TRACK_HEIGHT);
                }
            }
            WindowEvent::MouseOver => {
                self.hovered = true;
                cx.needs_redraw();
            }
            WindowEvent::MouseOut => {
                self.hovered = false;
                cx.needs_redraw();
            }
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let bounds = cx.bounds();
        let palette = self.theme.get().palette();
        // A short grip mark, centred - brighter on hover/drag so the drag
        // affordance is a glance away rather than printed as a hint.
        let color = if self.hovered || self.is_dragging { palette.ink_muted } else { palette.line };
        let cx_px = bounds.x + bounds.w * 0.5;
        let cy_px = bounds.y + bounds.h * 0.5;
        let mut paint = vg::Paint::default();
        paint.set_color(color);
        paint.set_anti_alias(true);
        let rect = vg::Rect::new(cx_px - 12.0, cy_px - 1.0, cx_px + 12.0, cy_px + 1.0);
        canvas.draw_path(&vg::Path::rect(rect, None), &paint);
    }
}

/// Same taper as the mixer strip's fader (`app::fader_to_gain`, in dB
/// instead of linear gain): unity at 0.75, +6 dB at the top, -60..0 dB
/// below that.
fn fader_pos_to_gain_db(position: f32) -> f32 {
    let position = position.clamp(0.0, 1.0);
    if position <= 0.0 {
        -100.0
    } else if position >= 0.75 {
        (position - 0.75) / 0.25 * 6.0
    } else {
        (position / 0.75 - 1.0) * 60.0
    }
}

fn gain_db_to_fader_pos(db: f32) -> f32 {
    if db >= 0.0 {
        (0.75 + db / 24.0).clamp(0.75, 1.0)
    } else {
        (0.75 * (db / 60.0 + 1.0)).clamp(0.0, 0.75)
    }
}

pub fn clip_color_to_rgb(color: ClipColor) -> Color {
    match color {
        ClipColor::Coral => tokens::CLIP_CORAL,
        ClipColor::Amber => tokens::CLIP_AMBER,
        ClipColor::Teal => tokens::CLIP_TEAL,
        ClipColor::Blue => tokens::CLIP_BLUE,
        ClipColor::Violet => tokens::CLIP_VIOLET,
        ClipColor::Pink => tokens::CLIP_PINK,
    }
}

pub fn track_header<'a>(
    cx: &'a mut Context,
    arrangement: Signal<Arrangement>,
    theme: Signal<ThemeId>,
    selected_track: Signal<Option<TrackId>>,
    renaming_track: Signal<Option<TrackId>>,
    track_id: TrackId,
    viewing_effect: Signal<Option<shared::arrangement::EffectNodeId>>,
) -> Handle<'a, impl View> {
    let name = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| t.name.clone()).unwrap_or_default()
    });
    // An audio track says so; a MIDI track names what it plays through.
    let kind_label = arrangement.map(move |arr| match arr.track(track_id) {
        Some(t) if t.kind == TrackKind::Audio => "Audio",
        Some(t) => t.instrument.map(|i| i.name()).unwrap_or("No instrument"),
        None => "",
    });
    let color = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| t.color).unwrap_or(ClipColor::Coral)
    });
    let mute = arrangement.map(move |arr| arr.track(track_id).map(|t| t.mute).unwrap_or(false));
    let solo = arrangement.map(move |arr| arr.track(track_id).map(|t| t.solo).unwrap_or(false));
    let arm = arrangement.map(move |arr| arr.track(track_id).map(|t| t.arm).unwrap_or(false));
    let effect_nodes: Memo<Vec<EffectNode>> = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| t.fx.ordered().into_iter().copied().collect()).unwrap_or_default()
    });
    let has_effects = effect_nodes.map(|nodes| !nodes.is_empty());
    let all_bypassed = effect_nodes.map(|nodes| !nodes.is_empty() && nodes.iter().all(|n| !n.enabled));
    // Phase 1 of the effects-board plan: there's no real board to open
    // yet, so "open" degrades to the existing device-panel toggle - this
    // becomes a real board-open flag once `fx_board` exists.
    let board_open = Memo::new(move |_| viewing_effect.get().is_some() && selected_track.get() == Some(track_id));
    // The fader's own drag position is committed to the arrangement only
    // on release (see the `Fader::on_release` wiring below - committing on
    // every intermediate move would rebuild this whole header list mid-
    // drag). This local, non-arrangement signal is what lets the dB
    // readout still track the live drag instead of only updating at the
    // end of it.
    let gain_preview: Signal<Option<f32>> = Signal::new(None);
    let gain_text = arrangement.map(move |arr| {
        let committed = arr.track(track_id).map(|t| t.gain_db).unwrap_or(0.0);
        format!("{:+.1} dB", gain_preview.get().unwrap_or(committed))
    });
    let fader_pos = arrangement.map(move |arr| {
        gain_db_to_fader_pos(arr.track(track_id).map(|t| t.gain_db).unwrap_or(0.0))
    });
    let height = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| t.height).unwrap_or(DEFAULT_TRACK_HEIGHT)
    });

    HStack::new(cx, move |cx| {
    // A full-height color strip at the row's leading edge - the at-a-
    // glance track identifier every other DAW's track list has, instead
    // of (in addition to, previously) a small swatch buried in the title
    // row.
    Element::new(cx)
        .class("track-color-bar")
        .background_color(color.map(|c| clip_color_to_rgb(*c)))
        .width(Pixels(4.0))
        .height(Stretch(1.0));

    VStack::new(cx, move |cx| {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                // `renaming_track` lives outside `arrangement`, so it
                // needs its own `Binding` here to switch this one row
                // into rename mode - the outer header-list `Binding` (in
                // `timeline/mod.rs`) only reruns when the arrangement
                // itself changes.
                Binding::new(cx, renaming_track, move |cx| {
                    if renaming_track.get() == Some(track_id) {
                        let draft: Signal<String> = Signal::new(name.get());
                        Textbox::new(cx, draft)
                            .class("title")
                            // `.title` alone (a Label class: font only, no
                            // box) is why this rendered with no visible
                            // background/border/caret contrast - reuse the
                            // sidebar search field's box styling so this
                            // actually reads as an editable input.
                            .class("search")
                            .on_edit(move |_cx, text| draft.set(text))
                            .on_submit(move |cx, text, _from_key| {
                                cx.emit(TimelineEvent::CommitRenameTrack(track_id, text))
                            })
                            .on_cancel(move |cx| cx.emit(TimelineEvent::CancelRenameTrack))
                            .width(Stretch(1.0));
                    } else {
                        Label::new(cx, name)
                            .class("title")
                            .on_double_click(move |cx, _| cx.emit(TimelineEvent::BeginRenameTrack(track_id)));
                    }
                });

                // TrackHeaderFx: "FX" plus one pip per effect (filled =
                // on, hollow = bypassed), an at-a-glance "what's on this
                // track" without opening the panel. Click opens the
                // device panel and selects the track (stands in for a
                // real board, which a later phase adds); Alt-click
                // toggles the whole chain's bypass without opening
                // anything. Lives in the title row, not down with
                // M/S/Rec, so it reads as part of "what's on this track"
                // alongside its name.
                Button::new(cx, move |cx| {
                    HStack::new(cx, move |cx| {
                        Label::new(cx, "FX")
                            .class("meta")
                            .color(all_bypassed.map(move |b| if *b { theme.get().palette().ink_muted } else { theme.get().palette().ink }));
                        Binding::new(cx, effect_nodes, move |cx| {
                            let nodes = effect_nodes.get();
                            for node in nodes.iter().take(8) {
                                Element::new(cx).class("fx-pip").toggle_class("is-on", node.enabled);
                            }
                            if nodes.len() > 8 {
                                Label::new(cx, format!("+{}", nodes.len() - 8)).class("meta");
                            }
                        });
                    })
                    .gap(Pixels(2.0))
                    .alignment(Alignment::Center)
                    .size(Auto)
                })
                .class("btn")
                .class("sm")
                .toggle_class("quiet", has_effects.map(|h| !*h))
                .toggle_class("is-on", board_open)
                .on_press(move |cx| {
                    if cx.modifiers().alt() {
                        cx.emit(TimelineEvent::SetChainBypassed(track_id, !all_bypassed.get()));
                    } else if board_open.get() {
                        viewing_effect.set(None);
                    } else {
                        cx.emit(crate::synth::state::SynthEvent::SelectTrack(track_id));
                        viewing_effect.set(effect_nodes.get().first().map(|n| n.id));
                    }
                });

                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                Label::new(cx, kind_label).class("meta");
            })
            .gap(Pixels(tokens::SPACE_2))
            .alignment(Alignment::Left)
            .height(Auto);

            HStack::new(cx, move |cx| {
                Button::new(cx, |cx| Label::new(cx, "M"))
                    .class("btn")
                    .class("sm")
                    .toggle_class("is-mute", mute)
                    .on_press(move |cx| cx.emit(TimelineEvent::ToggleMute(track_id)));

                Button::new(cx, |cx| Label::new(cx, "S"))
                    .class("btn")
                    .class("sm")
                    .toggle_class("is-solo", solo)
                    .on_press(move |cx| cx.emit(TimelineEvent::ToggleSolo(track_id)));

                Button::new(cx, |cx| Label::new(cx, "\u{25CF}"))
                    .class("btn")
                    .class("sm")
                    .toggle_class("is-rec", arm)
                    .on_press(move |cx| cx.emit(TimelineEvent::ToggleArm(track_id)));

                Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));

                Label::new(cx, gain_text).class("meta");

                // Trailing edge, away from the gain readout and fader -
                // a destructive action sitting right next to those two
                // read as though it belonged to them. No confirmation
                // dialog: like every other destructive edit here
                // (DeleteClip, DeleteSelected, ...), undo is the safety
                // net, not a modal.
                Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
                    .class("btn")
                    .class("sm")
                    .on_press(move |cx| cx.emit(TimelineEvent::RemoveTrack(track_id)));
            })
            .gap(Pixels(2.0))
            .alignment(Alignment::Left)
            .height(Auto);
        })
        .gap(Stretch(1.0))
        .width(Stretch(1.0))
        .height(Stretch(1.0));

        // The track header list rebuilds wholesale (`Binding` on the whole
        // arrangement, in `timeline/mod.rs`) on any arrangement mutation -
        // including this fader's own commit. Committing on every
        // intermediate drag move would tear down and rebuild this very
        // `Fader` mid-drag, so the commit happens only on release
        // (`on_release`); the cap still tracks the live position via the
        // fader's own local drag state (see `Fader::draw`).
        Fader::new(cx, fader_pos, 0.75, theme, move |_, position| {
            gain_preview.set(Some(fader_pos_to_gain_db(position)));
        })
        .on_release(move |cx, position| {
            gain_preview.set(None);
            cx.emit(TimelineEvent::SetTrackGain {
                track: track_id,
                gain_db: fader_pos_to_gain_db(position),
            });
        })
        .width(Pixels(16.0))
        .height(Stretch(1.0));
    })
    .gap(Pixels(tokens::SPACE_3))
    .padding(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Stretch(1.0));

    // A thin drag strip at the row's bottom edge, inside the row (not
    // overlapping the next one) so it never fights that row's own clicks.
    TrackResizeHandle::new(cx, height, theme, move |cx, new_height| {
        cx.emit(TimelineEvent::SetTrackHeight { track: track_id, height: new_height });
    })
    .width(Stretch(1.0))
    .height(Pixels(RESIZE_HANDLE_PX));
    })
    .width(Stretch(1.0))
    .height(Stretch(1.0));
    })
    .class("tl-head")
    .toggle_class("is-selected", selected_track.map(move |s| *s == Some(track_id)))
    .on_press_down(move |cx| cx.emit(crate::synth::state::SynthEvent::SelectTrack(track_id)))
    .on_mouse_down(move |cx, button| {
        if button == MouseButton::Right {
            cx.emit(crate::synth::state::SynthEvent::SelectTrack(track_id));
            let (x, y) = (cx.mouse().cursor_x, cx.mouse().cursor_y);
            cx.emit(TimelineEvent::OpenContextMenu(ContextMenu { target: ContextMenuTarget::Track(track_id), x, y }));
        }
    })
    .width(Pixels(crate::timeline::HEAD_WIDTH))
    .height(height.map(|h| Pixels(*h)))
}

pub fn automation_header<'a>(
    cx: &'a mut Context,
    arrangement: Signal<Arrangement>,
    lane_id: AutomationLaneId,
) -> Handle<'a, impl View> {
    let name = arrangement.map(move |arr| {
        arr.automation_lane(lane_id).map(|l| l.parameter_name.clone()).unwrap_or_default()
    });
    let value = arrangement.map(move |arr| {
        arr.automation_lane(lane_id).map(|l| l.display_value.clone()).unwrap_or_default()
    });

    VStack::new(cx, move |cx| {
        Label::new(cx, name).class("control");
        Label::new(cx, value).class("meta");
    })
    .class("tl-head-auto")
    .gap(Pixels(2.0))
    .width(Pixels(crate::timeline::HEAD_WIDTH))
    .height(Pixels(crate::timeline::LANE_AUTO_HEIGHT))
}
