//! Track header column: plain Vizia views (not canvas) styled through the
//! stylesheet, reusing the milestone-1 M/S/Arm buttons and colour swatch.

use vizia::prelude::*;

use shared::arrangement::{Arrangement, AutomationLaneId, ClipColor, TrackId, TrackKind};

use crate::fader::Fader;
use crate::timeline::state::TimelineEvent;
use crate::tokens::{self, ThemeId};

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

/// A track's clip colour at low alpha, for tinting its header row so it
/// reads as the same track at a glance next to its (fully-saturated) clips.
fn clip_color_to_soft_rgb(color: ClipColor) -> Color {
    let c = clip_color_to_rgb(color);
    Color::rgba(c.r(), c.g(), c.b(), 70)
}

pub fn track_header<'a>(
    cx: &'a mut Context,
    arrangement: Signal<Arrangement>,
    theme: Signal<ThemeId>,
    track_id: TrackId,
) -> Handle<'a, impl View> {
    let name = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| t.name.clone()).unwrap_or_default()
    });
    let kind_label = arrangement.map(move |arr| match arr.track(track_id).map(|t| t.kind) {
        Some(TrackKind::Audio) => "AUDIO",
        Some(TrackKind::Midi) => "MIDI",
        None => "",
    });
    let color = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| t.color).unwrap_or(ClipColor::Coral)
    });
    let mute = arrangement.map(move |arr| arr.track(track_id).map(|t| t.mute).unwrap_or(false));
    let solo = arrangement.map(move |arr| arr.track(track_id).map(|t| t.solo).unwrap_or(false));
    let arm = arrangement.map(move |arr| arr.track(track_id).map(|t| t.arm).unwrap_or(false));
    let gain_text = arrangement.map(move |arr| {
        arr.track(track_id).map(|t| format!("{:+.1} dB", t.gain_db)).unwrap_or_default()
    });
    let fader_pos = arrangement.map(move |arr| {
        gain_db_to_fader_pos(arr.track(track_id).map(|t| t.gain_db).unwrap_or(0.0))
    });

    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            HStack::new(cx, move |cx| {
                Element::new(cx)
                    .class("swatch")
                    .background_color(color.map(|c| clip_color_to_rgb(*c)));
                Label::new(cx, name).class("control");
                Label::new(cx, kind_label).class("meta");
            })
            .gap(Pixels(tokens::SPACE_1))
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

                // No confirmation dialog: like every other destructive
                // edit here (DeleteClip, DeleteSelected, ...), undo is
                // the safety net, not a modal.
                Button::new(cx, |cx| Label::new(cx, "\u{2715}"))
                    .class("btn")
                    .class("sm")
                    .on_press(move |cx| cx.emit(TimelineEvent::RemoveTrack(track_id)));

                Label::new(cx, gain_text).class("meta");
            })
            .gap(Pixels(2.0))
            .alignment(Alignment::Left)
            .height(Auto);
        })
        .gap(Stretch(1.0))
        .width(Stretch(1.0))
        .height(Stretch(1.0));

        Fader::new(cx, fader_pos, 0.75, theme, move |cx, position| {
            cx.emit(TimelineEvent::SetTrackGain {
                track: track_id,
                gain_db: fader_pos_to_gain_db(position),
            });
        })
        .width(Pixels(16.0))
        .height(Stretch(1.0));
    })
    .class("tl-head")
    .gap(Pixels(tokens::SPACE_3))
    .padding(Pixels(tokens::SPACE_2))
    .background_color(color.map(|c| clip_color_to_soft_rgb(*c)))
    .width(Pixels(crate::timeline::HEAD_WIDTH))
    .height(Pixels(crate::timeline::LANE_HEIGHT))
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
