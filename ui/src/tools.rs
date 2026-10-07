//! The practice tools - Theory, Voice leading, Ear, Riyaz and Exercises -
//! docked under the devices, one at a time. They open from Learn (a goal's
//! drill, today's practice, the Learn home's tool links) and from Search;
//! each still opens and closes itself (Riyaz starts and stops listening
//! that way), so this only sends their own toggles.

use vizia::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Theory,
    Voicing,
    Ear,
    Riyaz,
    Exercises,
}

impl Tool {
    pub const ALL: [Tool; 5] = [Tool::Theory, Tool::Voicing, Tool::Ear, Tool::Riyaz, Tool::Exercises];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Theory => "Theory ring",
            Tool::Voicing => "Voice leading",
            Tool::Ear => "Ear trainer",
            Tool::Riyaz => "Riyaz",
            Tool::Exercises => "Rhythm and melody exercises",
        }
    }

    fn toggle(self, cx: &mut EventContext) {
        match self {
            Tool::Theory => cx.emit(crate::interval_input::state::IntervalInputEvent::ToggleOpen),
            Tool::Voicing => cx.emit(crate::voicing::VoicingEvent::ToggleOpen),
            Tool::Ear => cx.emit(crate::ear::EarEvent::ToggleOpen),
            Tool::Riyaz => cx.emit(crate::riyaz::RiyazEvent::ToggleOpen),
            Tool::Exercises => cx.emit(crate::practice::PracticeEvent::ToggleOpen),
        }
    }
}

pub enum ToolsEvent {
    /// Show this tool (closing whichever else is open).
    Show(Tool),
    /// Close whatever is open.
    CloseAll,
}

/// Each tool's open signal, in `Tool::ALL` order.
#[derive(Clone, Copy)]
pub struct ToolsProps {
    pub open: [Signal<bool>; 5],
}

impl ToolsProps {
    fn is_open(&self, tool: Tool) -> bool {
        self.open[index(tool)].get()
    }
}

fn index(tool: Tool) -> usize {
    Tool::ALL.iter().position(|t| *t == tool).unwrap_or(0)
}

thread_local! {
    static PROPS: std::cell::Cell<Option<ToolsProps>> = const { std::cell::Cell::new(None) };
}

/// Whether any tool is open.
pub fn any_open() -> bool {
    Tool::ALL.iter().any(|t| is_open(*t))
}

/// Whether `tool` is open.
pub fn is_open(tool: Tool) -> bool {
    PROPS.get().is_some_and(|p| p.is_open(tool))
}

pub struct ToolsModel {
    props: ToolsProps,
}

impl ToolsModel {
    pub fn new(props: ToolsProps) -> Self {
        PROPS.set(Some(props));
        Self { props }
    }
}

impl Model for ToolsModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ToolsEvent::Show(tool) => {
                for other in Tool::ALL {
                    if other != *tool && self.props.is_open(other) {
                        other.toggle(cx);
                    }
                }
                if !self.props.is_open(*tool) {
                    tool.toggle(cx);
                }
            }
            ToolsEvent::CloseAll => {
                for tool in Tool::ALL {
                    if self.props.is_open(tool) {
                        tool.toggle(cx);
                    }
                }
            }
        });
    }
}
