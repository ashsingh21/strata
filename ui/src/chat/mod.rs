//! Chat: ask an AI about what you're making - feedback on a melody, what
//! structure would suit the song, what to add next. Each question goes
//! with a plain-text picture of the project (`context`), so the answers
//! are about this song. Any provider through `genai` (Anthropic by
//! default; OpenAI, Gemini, Ollama, Groq, DeepSeek, xAI by model name),
//! the answer streaming in as it's written.

pub mod context;
pub mod view;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use vizia::prelude::*;

use shared::arrangement::{Arrangement, ClipId, TrackId};

/// The model asked when nothing else is chosen.
pub const DEFAULT_MODEL: &str = "claude-sonnet-5";
/// Models offered in Settings (any other name can be typed).
pub const MODELS: &[(&str, &str)] = &[
    ("claude-sonnet-5", "Claude Sonnet 5 (Anthropic)"),
    ("claude-opus-5-5", "Claude Opus 5.5 (Anthropic)"),
    ("claude-haiku-4-5", "Claude Haiku 4.5 (Anthropic, fast)"),
    ("gpt-5.4-mini", "GPT-5.4 mini (OpenAI)"),
    ("gemini-3-flash-preview", "Gemini 3 Flash (Google)"),
    ("llama3.2", "Llama 3.2 (Ollama, on this computer)"),
];

/// Questions to start from.
pub const QUICK: &[&str] = &[
    "Give me feedback on my melody.",
    "What song structure would suit this?",
    "What should I add next?",
];

const SYSTEM: &str = "You are the music tutor inside Shor, a DAW for learning to make music. \
The person is learning: they play guitar and want to turn melodies in their head into music, and they \
don't know much theory. Be concrete and encouraging, and honest. Talk in notes, beats, bars and how it \
sounds; explain any theory word in a few plain words the first time. When giving feedback, say what \
works first, then at most two or three specific changes (which bar, which note, what to try instead) \
and why each would sound better. For song structure, name sections with bar numbers (intro 1-4, \
verse 5-12...) and what changes in each. Keep answers short: a few short paragraphs or a short list. \
Plain text only: no tables, no headings, no bold, no code.\n\nThe project right now:\n";

#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    pub mine: bool,
    pub text: String,
}

pub enum ChatEvent {
    /// Ask this.
    Send(String),
    SetDraft(String),
    /// Text as it streams in, for the request `generation`.
    Chunk { generation: u64, text: String },
    Done { generation: u64 },
    Failed { generation: u64, message: String },
    /// Stop the answer being written.
    Stop,
    NewChat,
    ToggleSettings,
    SetModel(String),
    /// A key typed in Settings ("" forgets it).
    SetKey(String),
}

pub struct ChatModel {
    pub messages: Signal<Vec<Message>>,
    pub draft: Signal<String>,
    pub busy: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub model: Signal<String>,
    /// A key was pasted in (else the provider's environment variable).
    pub has_key: Signal<bool>,
    pub settings_open: Signal<bool>,
    generation: u64,
    stop: Arc<AtomicBool>,
    arrangement: Signal<Arrangement>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    open_clip: Signal<Option<ClipId>>,
    selected_track: Signal<Option<TrackId>>,
}

fn key_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("ai_key")
}

fn load_key() -> Option<String> {
    std::fs::read_to_string(key_path()).ok().map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

/// Kept in its own file, readable only by you (never in the project or
/// the settings).
fn save_key(key: &str) {
    let path = key_path();
    if key.is_empty() {
        let _ = std::fs::remove_file(path);
        return;
    }
    if std::fs::write(&path, key).is_ok() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

impl ChatModel {
    pub fn new(
        arrangement: Signal<Arrangement>,
        key: Signal<u8>,
        scale_mask: Signal<u16>,
        open_clip: Signal<Option<ClipId>>,
        selected_track: Signal<Option<TrackId>>,
    ) -> Self {
        Self {
            messages: Signal::new(Vec::new()),
            draft: Signal::new(String::new()),
            busy: Signal::new(false),
            error: Signal::new(None),
            model: Signal::new(crate::settings::load_ai_model().unwrap_or_else(|| DEFAULT_MODEL.to_string())),
            has_key: Signal::new(load_key().is_some()),
            settings_open: Signal::new(false),
            generation: 0,
            stop: Arc::new(AtomicBool::new(false)),
            arrangement,
            key,
            scale_mask,
            open_clip,
            selected_track,
        }
    }

    fn system_prompt(&self) -> String {
        let arr = self.arrangement.get();
        let scale = crate::interval_input::state::scale_name(self.scale_mask.get());
        let scene = context::Scene {
            arrangement: &arr,
            key: self.key.get(),
            scale,
            open_clip: self.open_clip.get(),
            selected_track: self.selected_track.get(),
        };
        format!("{SYSTEM}{}", context::describe(&scene))
    }

    fn send(&mut self, cx: &mut EventContext, text: String) {
        let text = text.trim().to_string();
        if text.is_empty() || self.busy.get() {
            return;
        }
        self.draft.set(String::new());
        self.error.set(None);
        self.messages.update(|m| {
            m.push(Message { mine: true, text });
            m.push(Message { mine: false, text: String::new() });
        });
        self.busy.set(true);
        self.generation += 1;
        self.stop = Arc::new(AtomicBool::new(false));
        let generation = self.generation;
        let stop = self.stop.clone();
        let model = self.model.get();
        let system = self.system_prompt();
        // Everything said so far, without the empty answer just added.
        let history: Vec<Message> = {
            let m = self.messages.get();
            m[..m.len() - 1].to_vec()
        };
        let key = load_key();
        cx.spawn(move |proxy| {
            let result = ask(&model, key, system, history, |chunk| {
                let _ = proxy.emit(ChatEvent::Chunk { generation, text: chunk.to_string() });
                !stop.load(Ordering::Relaxed)
            });
            let _ = match result {
                Ok(()) => proxy.emit(ChatEvent::Done { generation }),
                Err(message) => proxy.emit(ChatEvent::Failed { generation, message }),
            };
        });
    }
}

/// Asks `model`, calling `on_chunk` with the answer as it streams in
/// (return false from it to stop). Runs on its own thread: a one-off
/// runtime for the request.
fn ask(model: &str, key: Option<String>, system: String, history: Vec<Message>, mut on_chunk: impl FnMut(&str) -> bool) -> Result<(), String> {
    use futures::StreamExt;
    use genai::chat::{ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent};
    use genai::resolver::{AuthData, AuthResolver};

    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| e.to_string())?;
    runtime.block_on(async move {
        let mut builder = genai::Client::builder();
        if let Some(key) = key {
            let resolver = AuthResolver::from_resolver_fn(move |_: genai::ModelIden| -> Result<Option<AuthData>, genai::resolver::Error> {
                Ok(Some(AuthData::from_single(key.clone())))
            });
            builder = builder.with_auth_resolver(resolver);
        }
        let client = builder.build();
        let mut request = ChatRequest::default().with_system(system);
        for m in history {
            request = request.append_message(if m.mine { ChatMessage::user(m.text) } else { ChatMessage::assistant(m.text) });
        }
        let options = ChatOptions::default().with_max_tokens(1500);
        let response = client.exec_chat_stream(model, request, Some(&options)).await.map_err(|e| friendly(&e.to_string()))?;
        let mut stream = response.stream;
        while let Some(event) = stream.next().await {
            match event.map_err(|e| friendly(&e.to_string()))? {
                ChatStreamEvent::Chunk(chunk) => {
                    if !on_chunk(&chunk.content) {
                        break;
                    }
                }
                ChatStreamEvent::End(_) => break,
                _ => {}
            }
        }
        Ok(())
    })
}

/// An error, in words: a missing key is the usual one.
fn friendly(raw: &str) -> String {
    if raw.contains("ApiKeyEnvNotFound") || raw.contains("api_key") || raw.contains("API_KEY") || raw.contains("401") {
        format!(
            "No working API key for this model. Set ANTHROPIC_API_KEY (or the provider's own variable) before starting Shor, \
             or paste a key in the chat's Settings. ({raw})"
        )
    } else if raw.contains("Connection refused") || raw.contains("connect") {
        format!("Couldn't reach the AI. Check the internet connection (or that Ollama is running, for a local model). ({raw})")
    } else {
        raw.to_string()
    }
}

impl Model for ChatModel {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|event, _| match event {
            ChatEvent::Send(text) => self.send(cx, text.clone()),
            ChatEvent::SetDraft(text) => self.draft.set(text.clone()),
            ChatEvent::Chunk { generation, text } => {
                if *generation == self.generation {
                    self.messages.update(|m| {
                        if let Some(last) = m.last_mut() {
                            last.text.push_str(text);
                        }
                    });
                }
            }
            ChatEvent::Done { generation } => {
                if *generation == self.generation {
                    self.busy.set(false);
                }
            }
            ChatEvent::Failed { generation, message } => {
                if *generation == self.generation {
                    self.busy.set(false);
                    // Drop the empty answer; say what went wrong.
                    self.messages.update(|m| {
                        if m.last().is_some_and(|l| !l.mine && l.text.is_empty()) {
                            m.pop();
                        }
                    });
                    self.error.set(Some(message.clone()));
                }
            }
            ChatEvent::Stop => {
                self.stop.store(true, Ordering::Relaxed);
                self.busy.set(false);
                self.generation += 1;
            }
            ChatEvent::NewChat => {
                self.stop.store(true, Ordering::Relaxed);
                self.generation += 1;
                self.busy.set(false);
                self.error.set(None);
                self.messages.set(Vec::new());
            }
            ChatEvent::ToggleSettings => self.settings_open.set(!self.settings_open.get()),
            ChatEvent::SetModel(model) => {
                let model = model.trim().to_string();
                if !model.is_empty() {
                    crate::settings::save_ai_model(&model);
                    self.model.set(model);
                }
            }
            ChatEvent::SetKey(key) => {
                save_key(key.trim());
                self.has_key.set(load_key().is_some());
            }
        });
    }
}

#[derive(Clone, Copy)]
pub struct ChatProps {
    pub messages: Signal<Vec<Message>>,
    pub draft: Signal<String>,
    pub busy: Signal<bool>,
    pub error: Signal<Option<String>>,
    pub model: Signal<String>,
    pub has_key: Signal<bool>,
    pub settings_open: Signal<bool>,
}

thread_local! {
    static PROPS: std::cell::Cell<Option<ChatProps>> = const { std::cell::Cell::new(None) };
}

/// The chat's signals, for the browser's Chat section.
pub fn props() -> Option<ChatProps> {
    PROPS.get()
}

impl ChatProps {
    pub fn of(m: &ChatModel) -> Self {
        let p = Self {
            messages: m.messages,
            draft: m.draft,
            busy: m.busy,
            error: m.error,
            model: m.model,
            has_key: m.has_key,
            settings_open: m.settings_open,
        };
        PROPS.set(Some(p));
        p
    }
}
