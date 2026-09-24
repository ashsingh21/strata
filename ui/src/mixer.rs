//! One mixer strip: pan knob, fader + meter, dB readout, M/S/Arm, name.
//!
//! Not currently mounted: redundant with the timeline's own per-track
//! M/S/Arm header. Kept as milestone-1 infrastructure.
#![allow(dead_code)]

use vizia::prelude::*;

use crate::app::AppEvent;
use crate::fader::Fader;
use crate::knob::Knob;
use crate::meter::Meter;
use crate::tokens::{ThemeId, CLIP_TEAL, SPACE_2};

#[allow(clippy::too_many_arguments)]
pub fn mixer_strip(
    cx: &mut Context,
    theme: Signal<ThemeId>,
    fader: Signal<f32>,
    pan: Signal<f32>,
    mute: Signal<bool>,
    solo: Signal<bool>,
    arm: Signal<bool>,
    gain_db: Signal<f32>,
    meter_level_l: Signal<f32>,
    meter_level_r: Signal<f32>,
    meter_clip_l: Signal<bool>,
    meter_clip_r: Signal<bool>,
) {
    VStack::new(cx, move |cx| {
        Knob::new(cx, pan, 0.5, theme, None, |cx, value| cx.emit(AppEvent::SetPan(value)))
            .size(Pixels(24.0));

        HStack::new(cx, move |cx| {
            Fader::new(cx, fader, 0.75, theme, |cx, value| cx.emit(AppEvent::SetFader(value)))
                .width(Pixels(16.0))
                .height(Stretch(1.0));

            Meter::new(
                cx,
                meter_level_l,
                meter_level_r,
                meter_clip_l,
                meter_clip_r,
                theme,
                |cx| cx.emit(AppEvent::ResetClip),
            )
            .width(Pixels(10.0))
            .height(Stretch(1.0));
        })
        .gap(Pixels(6.0))
        .height(Pixels(112.0))
        .width(Auto);

        let db_text = gain_db.map(|db| {
            if *db <= -99.0 { "-inf dB".to_string() } else { format!("{db:+.1} dB") }
        });
        Label::new(cx, db_text).class("mono");

        HStack::new(cx, move |cx| {
            Button::new(cx, |cx| Label::new(cx, "M"))
                .class("btn")
                .class("sm")
                .toggle_class("is-mute", mute)
                .on_press(|cx| cx.emit(AppEvent::ToggleMute));

            Button::new(cx, |cx| Label::new(cx, "S"))
                .class("btn")
                .class("sm")
                .toggle_class("is-solo", solo)
                .on_press(|cx| cx.emit(AppEvent::ToggleSolo));

            Button::new(cx, |cx| Label::new(cx, "\u{25CF}"))
                .class("btn")
                .class("sm")
                .toggle_class("is-rec", arm)
                .on_press(|cx| cx.emit(AppEvent::ToggleArm));
        })
        .gap(Pixels(2.0))
        .size(Auto);

        HStack::new(cx, |cx| {
            Element::new(cx).background_color(CLIP_TEAL).class("swatch");
            Label::new(cx, "Drums").class("control");
        })
        .gap(Pixels(4.0))
        .alignment(Alignment::Center)
        .size(Auto);
    })
    .gap(Pixels(SPACE_2))
    .alignment(Alignment::Center)
    .width(Pixels(64.0))
    .height(Auto);
}
