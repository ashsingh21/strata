//! The Chat section of the sidebar: the conversation, questions to start
//! from, a box to ask in, and the model and key in Settings.

use vizia::prelude::*;

use super::{ChatEvent, ChatProps, MODELS, QUICK};
use crate::tokens;

pub fn chat_panel(cx: &mut Context, p: ChatProps) {
    VStack::new(cx, move |cx| {
        header(cx, p);
        settings(cx, p);
        conversation(cx, p);
        Label::new(cx, p.error.map(|e| e.clone().unwrap_or_default()))
            .class("value")
            .class("chat-error")
            .text_wrap(true)
            .toggle_class("hidden", p.error.map(|e| e.is_none()))
            .width(Stretch(1.0));
        input(cx, p);
    })
    .gap(Pixels(tokens::SPACE_2))
    .padding(Pixels(tokens::SPACE_3))
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

fn header(cx: &mut Context, p: ChatProps) {
    HStack::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Label::new(cx, "Chat").class("title");
            Label::new(cx, p.model.map(|m| m.clone())).class("value");
        })
        .gap(Pixels(2.0))
        .width(Stretch(1.0))
        .height(Auto);
        Button::new(cx, |cx| Label::new(cx, "New"))
            .class("btn")
            .class("sm")
            .class("quiet")
            .tooltip(|cx| Tooltip::new(cx, |cx| { Label::new(cx, "Start a new conversation"); }).arrow(false))
            .on_press(|cx| cx.emit(ChatEvent::NewChat));
        Button::new(cx, |cx| Label::new(cx, "Settings"))
            .class("btn")
            .class("sm")
            .class("quiet")
            .toggle_class("is-on", p.settings_open)
            .on_press(|cx| cx.emit(ChatEvent::ToggleSettings));
    })
    .gap(Pixels(tokens::SPACE_1))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Auto);
}

/// Which model, and its key.
fn settings(cx: &mut Context, p: ChatProps) {
    VStack::new(cx, move |cx| {
        Binding::new(cx, p.settings_open, move |cx| {
            if !p.settings_open.get() {
                return;
            }
            VStack::new(cx, move |cx| {
                Label::new(cx, "Model").class("label");
                for &(id, name) in MODELS {
                    Button::new(cx, move |cx| Label::new(cx, name).class("body"))
                        .class("menu-item")
                        .toggle_class("is-on", p.model.map(move |m| m == id))
                        .width(Stretch(1.0))
                        .on_press(move |cx| cx.emit(ChatEvent::SetModel(id.to_string())));
                }
                Label::new(cx, "Or type any model name (Enter to use it):").class("value").text_wrap(true).width(Stretch(1.0));
                let typed = Signal::new(p.model.get());
                Textbox::new(cx, typed)
                    .on_submit(|cx, text, _| cx.emit(ChatEvent::SetModel(text)))
                    .width(Stretch(1.0))
                    .height(Pixels(tokens::SIZE_CONTROL));
                Label::new(cx, "API key").class("label");
                Label::new(
                    cx,
                    p.has_key.map(|k| {
                        if *k {
                            "A key is saved on this computer.".to_string()
                        } else {
                            "None saved: using ANTHROPIC_API_KEY (or the provider\u{2019}s own variable) if it\u{2019}s set.".to_string()
                        }
                    }),
                )
                .class("value")
                .text_wrap(true)
                .width(Stretch(1.0));
                let key = Signal::new(String::new());
                Textbox::new(cx, key)
                    .placeholder("Paste a key, then Enter")
                    .on_submit(|cx, text, _| cx.emit(ChatEvent::SetKey(text)))
                    .width(Stretch(1.0))
                    .height(Pixels(tokens::SIZE_CONTROL));
                Button::new(cx, |cx| Label::new(cx, "Forget the saved key"))
                    .class("btn")
                    .class("sm")
                    .toggle_class("hidden", p.has_key.map(|k| !*k))
                    .on_press(|cx| cx.emit(ChatEvent::SetKey(String::new())));
                Label::new(cx, "Workspace ID").class("label");
                Label::new(cx, "Only for an Anthropic key that isn\u{2019}t in a workspace: its ID (wrkspc_\u{2026}), from the Console\u{2019}s Workspaces page. Enter to use it.")
                    .class("value")
                    .text_wrap(true)
                    .width(Stretch(1.0));
                let ws = Signal::new(p.workspace.get());
                Textbox::new(cx, ws)
                    .placeholder("wrkspc_...")
                    .on_submit(|cx, text, _| cx.emit(ChatEvent::SetWorkspace(text)))
                    .width(Stretch(1.0))
                    .height(Pixels(tokens::SIZE_CONTROL));
            })
            .class("chat-settings")
            .gap(Pixels(tokens::SPACE_1))
            .padding(Pixels(tokens::SPACE_2))
            .width(Stretch(1.0))
            .height(Auto);
        });
    })
    .width(Stretch(1.0))
    .height(Auto);
}

fn conversation(cx: &mut Context, p: ChatProps) {
    let count = Memo::new(move |_| p.messages.get().len());
    // Keeps the newest text in view as it streams in.
    let follow = Memo::new(move |_| {
        let chars: usize = p.messages.get().iter().map(|m| m.text.len()).sum();
        if chars % 2 == 0 { 1.0 } else { 0.99999 }
    });
    ScrollView::new(cx, move |cx| {
        VStack::new(cx, move |cx| {
            Binding::new(cx, count, move |cx| {
                let n = count.get();
                if n == 0 {
                    empty(cx);
                    return;
                }
                for i in 0..n {
                    let mine = p.messages.get().get(i).is_some_and(|m| m.mine);
                    let text = Memo::new(move |_| {
                        let m = p.messages.get();
                        match m.get(i) {
                            Some(msg) if msg.text.is_empty() && !msg.mine => "\u{2026}".to_string(),
                            Some(msg) => msg.text.clone(),
                            None => String::new(),
                        }
                    });
                    Label::new(cx, text)
                        .class("body")
                        .class(if mine { "chat-mine" } else { "chat-theirs" })
                        .text_wrap(true)
                        .padding(Pixels(tokens::SPACE_2))
                        .width(Stretch(1.0));
                }
            });
        })
        .gap(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Auto);
    })
    .scroll_y(follow)
    .show_horizontal_scrollbar(false)
    .width(Stretch(1.0))
    .height(Stretch(1.0));
}

/// Before the first question: what it's for, and questions to start from.
fn empty(cx: &mut Context) {
    VStack::new(cx, |cx| {
        Label::new(cx, "Ask about what you\u{2019}re making. It sees your tracks, chords and the melody you have open.")
            .class("body")
            .class("learn-muted")
            .text_wrap(true)
            .width(Stretch(1.0));
        for &q in QUICK {
            Button::new(cx, move |cx| Label::new(cx, q).class("body").text_wrap(true).width(Stretch(1.0)))
                .class("btn")
                .class("chat-quick")
                .width(Stretch(1.0))
                .height(Auto)
                .on_press(move |cx| cx.emit(ChatEvent::Send(q.to_string())));
        }
    })
    .gap(Pixels(tokens::SPACE_2))
    .width(Stretch(1.0))
    .height(Auto);
}

fn input(cx: &mut Context, p: ChatProps) {
    HStack::new(cx, move |cx| {
        Textbox::new(cx, p.draft)
            .placeholder("Ask anything about your song")
            .on_edit(|cx, text| cx.emit(ChatEvent::SetDraft(text)))
            .on_submit(|cx, text, from_key| {
                if from_key {
                    cx.emit(ChatEvent::Send(text));
                }
            })
            .width(Stretch(1.0))
            .height(Pixels(28.0));
        Button::new(cx, move |cx| Label::new(cx, p.busy.map(|b| if *b { "Stop" } else { "Send" })))
            .class("btn")
            .class("is-on")
            .on_press(move |cx| {
                if p.busy.get() {
                    cx.emit(ChatEvent::Stop);
                } else {
                    cx.emit(ChatEvent::Send(p.draft.get()));
                }
            });
    })
    .gap(Pixels(tokens::SPACE_1))
    .alignment(Alignment::Left)
    .width(Stretch(1.0))
    .height(Auto);
}
