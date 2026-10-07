//! The command palette (Ctrl/Cmd+K): one search box for every action, and
//! for everything the sidebar can list - instruments, effects, presets,
//! samples, lessons. A newcomer never has to know where a thing lives; an
//! old hand gets a keyboard path to all of it.

pub mod view;

use std::sync::Arc;

use vizia::prelude::*;

use crate::browser::{BrowserEvent, Item, Kind};

type Run = Arc<dyn Fn(&mut EventContext) + Send + Sync>;

/// One thing the palette can do.
pub struct Command {
    pub title: String,
    pub section: &'static str,
    pub shortcut: &'static str,
    /// Words people might type that the title doesn't contain.
    pub keywords: &'static str,
    pub run: Run,
}

/// What one result row does when chosen.
#[derive(Clone, Debug, PartialEq)]
pub enum Pick {
    Command(usize),
    /// A browser item: "use this" (the same as double-clicking it).
    Item(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub label: String,
    pub detail: String,
    pub shortcut: &'static str,
    pub pick: Pick,
}

/// The most rows shown; the palette is a shortcut, not a second browser.
pub const MAX_ROWS: usize = 9;

/// How well `query` matches `text`, or `None`. Each word of the query must
/// appear in order as letters of the text; runs of letters, word starts
/// and a prefix score higher.
pub fn score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut total = 0;
    for word in query.to_lowercase().split_whitespace() {
        total += score_word(word, &text)?;
    }
    Some(total)
}

fn score_word(word: &str, text: &[char]) -> Option<i32> {
    let mut at = 0;
    let mut score = 0;
    let mut last: Option<usize> = None;
    for c in word.chars() {
        let found = (at..text.len()).find(|&i| text[i] == c)?;
        score += 1;
        if last == Some(found.wrapping_sub(1)) {
            score += 5;
        }
        if found == 0 || !text[found - 1].is_alphanumeric() {
            score += 8;
        }
        last = Some(found);
        at = found + 1;
    }
    let word_chars: Vec<char> = word.chars().collect();
    if text.windows(word_chars.len().max(1)).any(|w| w == word_chars.as_slice()) {
        score += 20;
    }
    if text.starts_with(&word_chars) {
        score += 30;
    }
    // Shorter titles are the likelier meaning: "Save" before "Save As...".
    Some(score * 4 - (text.len() as i32 / 8).min(8))
}

fn kind_label(kind: &Kind) -> &'static str {
    match kind {
        Kind::Instrument(_) => "Instrument",
        Kind::Effect(_) => "Effect",
        Kind::Preset(_) => "Preset",
        Kind::Sample(_) => "Sample",
        Kind::Pattern(_) => "Pattern",
        Kind::Song(_) => "Demo song",
        Kind::Track(_) => "My track",
        Kind::Lesson(_) => "Lesson",
        Kind::ProjectAudio(_) => "Project audio",
    }
}

/// What to list for `query`: with nothing typed, the first few commands
/// (the table is ordered most useful first); otherwise the best matches
/// from commands and library items together.
pub fn results(query: &str, commands: &[Command], items: &[Item]) -> Vec<Row> {
    if query.trim().is_empty() {
        return commands
            .iter()
            .enumerate()
            .take(MAX_ROWS)
            .map(|(i, c)| Row { label: c.title.clone(), detail: c.section.to_string(), shortcut: c.shortcut, pick: Pick::Command(i) })
            .collect();
    }
    let mut scored: Vec<(i32, Row)> = Vec::new();
    for (i, c) in commands.iter().enumerate() {
        // A command wins ties against a library item of the same score.
        let by_title = score(query, &c.title).map(|s| s + 1);
        let by_keywords = score(query, c.keywords).map(|s| s / 2);
        if let Some(s) = by_title.max(by_keywords) {
            scored.push((s, Row { label: c.title.clone(), detail: c.section.to_string(), shortcut: c.shortcut, pick: Pick::Command(i) }));
        }
    }
    for item in items {
        let by_name = score(query, &item.name);
        let by_detail = score(query, &format!("{} {}", item.meta, item.group)).map(|s| s / 3);
        if let Some(s) = by_name.max(by_detail) {
            scored.push((s, Row { label: item.name.clone(), detail: kind_label(&item.kind).to_string(), shortcut: "", pick: Pick::Item(item.id.clone()) }));
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().take(MAX_ROWS).map(|(_, r)| r).collect()
}

fn command(title: impl Into<String>, section: &'static str, shortcut: &'static str, keywords: &'static str, run: impl Fn(&mut EventContext) + Send + Sync + 'static) -> Command {
    Command { title: title.into(), section, shortcut, keywords, run: Arc::new(run) }
}

/// Every action. The first few are what shows before anything is typed.
pub fn commands() -> Vec<Command> {
    use crate::analyzer::AnalyzerEvent;
    use crate::app::AppEvent;
    use crate::project::ProjectEvent;
    use crate::timeline::state::{TimelineEvent, TimelineTool};
    use shared::arrangement::{SnapGrid, TrackKind};

    let mut c = vec![
        command("Add MIDI track", "Tracks", "", "instrument synth keys new", |cx| cx.emit(TimelineEvent::AddTrack(TrackKind::Midi))),
        command("Add audio track", "Tracks", "", "record microphone import new", |cx| cx.emit(TimelineEvent::AddTrack(TrackKind::Audio))),
        command("Add guitar track", "Tracks", "", "amp cabinet tone input new", |cx| cx.emit(TimelineEvent::AddGuitarTrack)),
        command("Play / Pause", "Transport", "Space", "start stop", |cx| cx.emit(AppEvent::TogglePlay)),
        command("Stop", "Transport", "", "halt", |cx| cx.emit(AppEvent::Stop)),
        command("Return to start", "Transport", "Home", "rewind beginning playhead", |cx| cx.emit(AppEvent::Rewind)),
        command("Learn", "Learn", "", "lessons teach tutorial course help beginner goals practice", |cx| {
            cx.emit(BrowserEvent::ShowLearn);
            cx.emit(crate::learn::LearnEvent::Open(Some(crate::learn::Page::Home)));
        }),
        command("Today's practice", "Learn", "", "daily drill exercise five minutes streak", |cx| cx.emit(crate::learn::LearnEvent::StartToday)),
        command("Ear trainer", "Learn", "", "ear training intervals by ear guitar find the note", |cx| cx.emit(crate::tools::ToolsEvent::Show(crate::tools::Tool::Ear))),
        command("Rhythm and melody exercises", "Learn", "", "practice rhythm tap melody play back classics", |cx| cx.emit(crate::tools::ToolsEvent::Show(crate::tools::Tool::Exercises))),
        command("Theory ring", "Learn", "", "scale chord circle notes theory", |cx| cx.emit(crate::tools::ToolsEvent::Show(crate::tools::Tool::Theory))),
        command("Voice leading", "Learn", "", "chords voicing smooth progression", |cx| cx.emit(crate::tools::ToolsEvent::Show(crate::tools::Tool::Voicing))),
        command("Riyaz", "Learn", "", "sing tanpura swar pitch indian raag", |cx| cx.emit(crate::tools::ToolsEvent::Show(crate::tools::Tool::Riyaz))),
        command("Search the library", "View", "Ctrl+F", "find sounds samples browser", |cx| cx.emit(BrowserEvent::FocusSearch)),
        command("Save", "File", "Ctrl+S", "", |cx| cx.emit(ProjectEvent::Save)),
        command("Export audio...", "File", "", "render bounce wav mixdown", |cx| cx.emit(ProjectEvent::ExportDialog)),
        command("New project", "File", "", "blank clear", |cx| cx.emit(ProjectEvent::New)),
        command("Open project...", "File", "", "load", |cx| cx.emit(ProjectEvent::OpenDialog)),
        command("Save as...", "File", "", "copy rename", |cx| cx.emit(ProjectEvent::SaveAsDialog)),
        command("Undo", "Edit", "Ctrl+Z", "back", |cx| cx.emit(TimelineEvent::Undo)),
        command("Redo", "Edit", "Ctrl+Shift+Z", "forward", |cx| cx.emit(TimelineEvent::Redo)),
        command("Split clip at playhead", "Edit", "Ctrl+E", "cut", |cx| cx.emit(TimelineEvent::SplitAtPlayhead)),
        command("Duplicate selection", "Edit", "Ctrl+D", "copy", |cx| cx.emit(TimelineEvent::DuplicateSelected)),
        command("Delete selection", "Edit", "Delete", "remove", |cx| cx.emit(TimelineEvent::DeleteSelected)),
        command("Loop on / off", "Transport", "", "repeat cycle", |cx| cx.emit(AppEvent::ToggleLoop)),
        command("Metronome on / off", "Transport", "", "click beat", |cx| cx.emit(AppEvent::ToggleClick)),
        command("Record on / off", "Transport", "", "arm take", |cx| cx.emit(AppEvent::ToggleArm)),
        command("Tap tempo", "Transport", "", "bpm speed", |cx| cx.emit(AppEvent::Tap)),
        command("Draw tool", "Timeline", "", "pencil make clip", |cx| cx.emit(TimelineEvent::SetTool(TimelineTool::Draw))),
        command("Select tool", "Timeline", "", "pointer arrow", |cx| cx.emit(TimelineEvent::SetTool(TimelineTool::Select))),
        command("Follow playhead", "Timeline", "F", "scroll", |cx| cx.emit(TimelineEvent::ToggleFollow)),
        command("Show / hide sidebar", "View", "Ctrl+B", "browser panel library", |cx| cx.emit(AppEvent::ToggleSidebar)),
        command("Show / hide spectrum", "View", "", "analyzer frequency", |cx| cx.emit(AnalyzerEvent::Toggle)),
        command("Zoom in", "View", "Ctrl+=", "bigger", |cx| cx.emit(AppEvent::Zoom(1))),
        command("Zoom out", "View", "Ctrl+-", "smaller", |cx| cx.emit(AppEvent::Zoom(-1))),
        command("Reset zoom", "View", "Ctrl+0", "", |cx| cx.emit(AppEvent::ResetZoom)),
        command("Next theme", "View", "Ctrl+T", "colours dark light", |cx| cx.emit(AppEvent::ToggleTheme)),
        command("Show tips again", "Help", "", "hints reset", |cx| cx.emit(crate::hints::HintEvent::ResetAll)),
    ];
    for theme in crate::tokens::ThemeId::ALL {
        c.push(command(format!("Theme: {}", theme.name()), "View", "", "colours appearance dark light", move |cx| cx.emit(AppEvent::SetTheme(theme))));
    }
    for grid in SnapGrid::ALL {
        c.push(command(format!("Snap: {}", grid.label()), "Timeline", "", "grid quantize note length", move |cx| cx.emit(TimelineEvent::SetSnap(grid))));
    }
    c
}

pub enum PaletteEvent {
    Toggle,
    Close,
    SetQuery(String),
    Move(i32),
    /// Run the highlighted row.
    Run,
    /// Run this row (a click).
    RunRow(usize),
}

pub struct PaletteModel {
    pub open: Signal<bool>,
    pub query: Signal<String>,
    pub selected: Signal<usize>,
    pub rows: Memo<Vec<Row>>,
    commands: Arc<Vec<Command>>,
}

impl PaletteModel {
    pub fn new(all: Signal<Arc<Vec<Item>>>) -> Self {
        let query = Signal::new(String::new());
        let commands = Arc::new(commands());
        let for_rows = commands.clone();
        let rows = Memo::new(move |_| results(&query.get(), &for_rows, &all.get()));
        Self { open: Signal::new(false), query, selected: Signal::new(0), rows, commands }
    }

    fn run(&mut self, cx: &mut EventContext, row: Option<Row>) {
        self.close();
        match row.map(|r| r.pick) {
            Some(Pick::Command(i)) => (self.commands[i].run)(cx),
            Some(Pick::Item(id)) => cx.emit(BrowserEvent::Activate(id)),
            None => {}
        }
    }

    fn close(&mut self) {
        self.open.set(false);
    }
}

impl Model for PaletteModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|e, _| match e {
            PaletteEvent::Toggle => {
                if self.open.get() {
                    self.close();
                } else {
                    self.query.set(String::new());
                    self.selected.set(0);
                    self.open.set(true);
                    cx.emit(crate::hints::HintEvent::Dismiss("palette"));
                }
            }
            PaletteEvent::Close => self.close(),
            PaletteEvent::SetQuery(text) => {
                self.query.set(text.clone());
                self.selected.set(0);
            }
            PaletteEvent::Move(by) => {
                let n = self.rows.get().len();
                if n > 0 {
                    let now = self.selected.get() as i32 + by;
                    self.selected.set(now.rem_euclid(n as i32) as usize);
                }
            }
            PaletteEvent::Run => {
                let row = self.rows.get().get(self.selected.get()).cloned();
                self.run(cx, row);
            }
            PaletteEvent::RunRow(i) => {
                let row = self.rows.get().get(*i).cloned();
                self.run(cx, row);
            }
        });
        // The arrow keys and Enter bubble up from the search box.
        if self.open.get() {
            event.map(|e, meta| {
                if let WindowEvent::KeyDown(code, _) = e {
                    match code {
                        Code::ArrowDown => cx.emit(PaletteEvent::Move(1)),
                        Code::ArrowUp => cx.emit(PaletteEvent::Move(-1)),
                        Code::Enter | Code::NumpadEnter => cx.emit(PaletteEvent::Run),
                        Code::Escape => cx.emit(PaletteEvent::Close),
                        _ => return,
                    }
                    meta.consume();
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(title: &str, keywords: &'static str) -> Command {
        command(title, "Test", "", keywords, |_| {})
    }

    fn sample(name: &str) -> Item {
        Item {
            id: format!("sample:{name}"),
            kind: Kind::Sample(Arc::from(name)),
            name: name.to_string(),
            meta: String::new(),
            bpm: None,
            group: "Kicks".to_string(),
        }
    }

    #[test]
    fn letters_must_appear_in_order() {
        assert!(score("mdi", "Add MIDI track").is_some());
        assert!(score("idm", "Add MIDI track").is_none());
        assert!(score("add midi", "Add MIDI track").is_some());
        assert!(score("track add", "Add MIDI track").is_some());
        assert!(score("zzz", "Add MIDI track").is_none());
    }

    #[test]
    fn a_prefix_beats_a_scattered_match() {
        let prefix = score("sav", "Save").unwrap();
        let scattered = score("sav", "Show all variations").unwrap();
        assert!(prefix > scattered);
    }

    #[test]
    fn the_shorter_title_comes_first() {
        let commands = [cmd("Save as...", ""), cmd("Save", "")];
        let rows = results("save", &commands, &[]);
        assert_eq!(rows[0].label, "Save");
    }

    #[test]
    fn keywords_find_a_command_its_title_does_not_name() {
        let commands = [cmd("Export audio...", "render bounce"), cmd("Undo", "")];
        let rows = results("bounce", &commands, &[]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Export audio...");
    }

    #[test]
    fn nothing_typed_lists_the_first_commands_in_order() {
        let commands: Vec<Command> = (0..20).map(|i| cmd(&format!("Command {i}"), "")).collect();
        let rows = results("  ", &commands, &[]);
        assert_eq!(rows.len(), MAX_ROWS);
        assert_eq!(rows[0].pick, Pick::Command(0));
    }

    #[test]
    fn items_are_listed_with_what_they_are_and_activate_by_id() {
        let items = [sample("kick_deep.wav")];
        let rows = results("kick", &[cmd("Undo", "")], &items);
        assert_eq!(rows[0].detail, "Sample");
        assert_eq!(rows[0].pick, Pick::Item("sample:kick_deep.wav".to_string()));
    }

    #[test]
    fn a_command_outranks_an_item_with_the_same_name() {
        let items = [sample("Undo")];
        let rows = results("undo", &[cmd("Undo", "")], &items);
        assert_eq!(rows[0].pick, Pick::Command(0));
    }

    #[test]
    fn the_list_is_capped() {
        let items: Vec<Item> = (0..100).map(|i| sample(&format!("kick {i}"))).collect();
        assert_eq!(results("kick", &[], &items).len(), MAX_ROWS);
    }

    #[test]
    fn the_real_table_finds_the_obvious_things() {
        let table = commands();
        let top = |q: &str| results(q, &table, &[]).first().map(|r| r.label.clone());
        assert_eq!(top("save").as_deref(), Some("Save"));
        assert_eq!(top("export").as_deref(), Some("Export audio..."));
        assert_eq!(top("lesson").as_deref(), Some("Learn"));
        assert_eq!(top("ear").as_deref(), Some("Ear trainer"));
        assert_eq!(top("riyaz").as_deref(), Some("Riyaz"));
        assert_eq!(top("metronome").as_deref(), Some("Metronome on / off"));
        assert_eq!(top("snap 1/8").as_deref(), Some("Snap: 1/8"));
    }
}
