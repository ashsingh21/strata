//! The status bar along the bottom of the window. (The left sidebar -
//! the browser - lives in `crate::browser`.)

use vizia::prelude::*;

/// The status bar: audio settings on the left; on the right, the control
/// last touched with its live value, then whether the project is saved.
pub fn status_bar(
    cx: &mut Context,
    sample_rate: u32,
    block_frames: Signal<u32>,
    touched: Signal<Option<(String, Memo<String>)>>,
    save_status: Memo<String>,
    export_status: Signal<String>,
) {
    HStack::new(cx, move |cx| {
        let audio = block_frames.map(move |&frames| {
            let khz = sample_rate as f32 / 1000.0;
            if frames == 0 {
                format!("{khz:.0} kHz")
            } else {
                let ms = frames as f32 / sample_rate as f32 * 1000.0;
                format!("{khz:.0} kHz \u{b7} {frames} samples \u{b7} {ms:.1} ms")
            }
        });
        Label::new(cx, audio).class("value");
        Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
        let touched_text = Memo::new(move |_| match touched.get() {
            Some((name, value)) => format!("{name} {}", value.get()),
            None => String::new(),
        });
        Label::new(cx, touched_text).class("value");
        Element::new(cx).class("hairline").width(Pixels(1.0)).height(Pixels(12.0));
        Label::new(cx, export_status).class("value").toggle_class("hidden", export_status.map(|s| s.is_empty()));
        Label::new(cx, save_status).class("value");
    })
    .class("statusbar")
    // Inline: the stylesheet's `padding: 0px 12px` isn't valid Vizia CSS
    // (one length only), so the text sat against the window's edges - and
    // under the rounded corners on macOS.
    .padding_left(Pixels(crate::tokens::SPACE_3))
    .padding_right(Pixels(crate::tokens::SPACE_4))
    .gap(Pixels(crate::tokens::SPACE_3))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Pixels(24.0));
}
