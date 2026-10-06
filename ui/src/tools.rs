//! The practice tools - Theory, Voice leading, Riyaz and Exercises - behind
//! one rail button, shown one at a time under the devices with tabs to
//! switch. Each tool still opens and closes itself (Riyaz starts and stops
//! listening that way), so this only sends their own toggles.

use vizia::prelude::*;

use crate::tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Theory,
    Voicing,
    Riyaz,
    Exercises,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Theory, Tool::Voicing, Tool::Riyaz, Tool::Exercises];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Theory => "Theory",
            Tool::Voicing => "Voice leading",
            Tool::Riyaz => "Riyaz",
            Tool::Exercises => "Exercises",
        }
    }

    fn toggle(self, cx: &mut EventContext) {
        match self {
            Tool::Theory => cx.emit(crate::interval_input::state::IntervalInputEvent::ToggleOpen),
            Tool::Voicing => cx.emit(crate::voicing::VoicingEvent::ToggleOpen),
            Tool::Riyaz => cx.emit(crate::riyaz::RiyazEvent::ToggleOpen),
            Tool::Exercises => cx.emit(crate::practice::PracticeEvent::ToggleOpen),
        }
    }
}

pub enum ToolsEvent {
    /// Show this tool (closing whichever else is open).
    Show(Tool),
    /// The rail button: close what's open, or open the one used last.
    Toggle,
}

/// Each tool's open signal, in `Tool::ALL` order.
#[derive(Clone, Copy)]
pub struct ToolsProps {
    pub open: [Signal<bool>; 4],
}

impl ToolsProps {
    fn is_open(&self, tool: Tool) -> bool {
        self.open[index(tool)].get()
    }

    pub fn any_open(&self) -> bool {
        self.open.iter().any(|o| o.get())
    }
}

fn index(tool: Tool) -> usize {
    Tool::ALL.iter().position(|t| *t == tool).unwrap_or(0)
}

thread_local! {
    static PROPS: std::cell::Cell<Option<ToolsProps>> = const { std::cell::Cell::new(None) };
}

/// Whether any tool is open, for the rail button.
pub fn any_open() -> bool {
    PROPS.get().is_some_and(|p| p.any_open())
}

pub struct ToolsModel {
    props: ToolsProps,
    last: Tool,
}

impl ToolsModel {
    pub fn new(props: ToolsProps) -> Self {
        PROPS.set(Some(props));
        Self { props, last: Tool::Exercises }
    }

    fn show(&mut self, cx: &mut EventContext, tool: Tool) {
        for other in Tool::ALL {
            if other != tool && self.props.is_open(other) {
                other.toggle(cx);
            }
        }
        if !self.props.is_open(tool) {
            tool.toggle(cx);
        }
        self.last = tool;
    }
}

impl Model for ToolsModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ToolsEvent::Show(tool) => self.show(cx, *tool),
            ToolsEvent::Toggle => {
                if self.props.any_open() {
                    for tool in Tool::ALL {
                        if self.props.is_open(tool) {
                            self.last = tool;
                            tool.toggle(cx);
                        }
                    }
                } else {
                    self.show(cx, self.last);
                }
            }
        });
    }
}

/// The tabs over the open tool (nothing while none is open).
pub fn tabs(cx: &mut Context, p: ToolsProps) {
    let any = Memo::new(move |_| p.any_open());
    Binding::new(cx, any, move |cx| {
        if !any.get() {
            return;
        }
        HStack::new(cx, move |cx| {
            Label::new(cx, "Practice").class("label");
            crate::synth::segmented::segmented(
                cx,
                Tool::ALL.len(),
                |cx, i| Label::new(cx, Tool::ALL[i].name()),
                move |i| p.open[i].map(|o| *o),
                |cx, i| cx.emit(ToolsEvent::Show(Tool::ALL[i])),
            )
            .height(Pixels(tokens::SIZE_CONTROL));
        })
        .gap(Pixels(tokens::SPACE_2))
        .alignment(Alignment::Left)
        .padding_left(Pixels(tokens::SPACE_1))
        .width(Stretch(1.0))
        .height(Pixels(tokens::SIZE_CONTROL));
    });
}
