# Strata

A performance-first DAW UI built in Rust with [Vizia](https://github.com/vizia/vizia),
styled from the Strata design system (`design/`).

```
cargo run -p ui
```

Opens a window with the transport, a mixer strip wired to a real audio
engine, an LFO-modulated demo knob, and an arrangement timeline seeded with
a 16-bar, 4-track demo project.

## Workspace layout

- **`engine`** — the audio thread. Plays a 220 Hz test tone through a
  gain/pan stage (cpal), measures post-fader peak per channel, and reports a
  bar.beat.sixteenth transport position. Nothing in the audio callback
  allocates, locks, or calls a syscall.
- **`shared`** — the engine↔UI bridge (`Params`, `Telemetry`, `bridge()`),
  plus `arrangement/`: the timeline's data model, ticks-based time, the
  undo/redo command stack, waveform peak pyramids and the view transform.
  All pure logic, no Vizia dependency, so `cargo test -p shared` runs fast
  and covers all of it.
- **`ui`** — the Vizia app: the transport bar, the Strata `Knob`/`Fader`/
  `Meter` custom canvas views, the mixer strip, the LFO demo, and the
  `timeline/` module (track headers, ruler, lane area).
- **`design/tokens.json`** — the single source of truth for every colour,
  spacing, radius and size token. `ui/build.rs` reads it and generates
  `ui/styles/{base,studio,daylight}.css` (loaded via `include_style!`) and a
  Rust `Palette` (`ui/src/tokens.rs`) used directly by the custom-drawn
  views. Regenerate either by editing `design/tokens.json` and rebuilding —
  nothing is hand-copied.
- **`assets/`** — three generated test WAVs (`drums.wav`, `lead_hook.wav`,
  `lead_take3.wav`) the timeline's seed arrangement references.

## The engine↔UI bridge

Two directions, two mechanisms (`shared::{Params, Telemetry, bridge}`):

- **UI → engine**: `Params` holds plain atomics (fader gain, pan, playing,
  a stop-request flag). The audio callback reads them every block and
  smooths gain/pan with a one-pole filter to avoid zipper noise.
- **Engine → UI**: `Telemetry` (peak L/R, transport position) is pushed
  into a lock-free SPSC ring buffer (`rtrb`) once per audio callback. A
  60 fps UI timer drains it, runs the meter's attack-instant/release-at-
  20 dB/s ballistics, and updates the transport readout.

Vizia's reactive `Signal`s sit on top of this: the UI's `AppData` model
owns the `Params`/`Telemetry` ends and exposes plain `Signal<T>` fields
that the views bind to.

## Theming

Vizia 0.4's stylesheet parser has no CSS custom-property (`var()`) support,
so the generator can't emit one set of rules with runtime-swappable
variables. Instead: `base.css` holds structural rules (sizes, radii, type)
plus the studio (default) colours, unscoped; `daylight.css` holds the same
selectors' colours again, scoped under `.theme-daylight`. `Ctrl/Cmd+T`
toggles that class on the root — the same mechanism Vizia's own built-in
dark-mode support uses.

Canvas-drawn views (`Knob`, `Fader`, `Meter`, the timeline's `Ruler` and
`LaneArea`) don't go through CSS at all: they read the generated
`tokens::Palette` directly for both themes, selected by a `Signal<ThemeId>`,
since juggling five-plus distinct colours per shape through CSS-resolved
per-element colour slots doesn't scale the way it does for flat-coloured
buttons and text.

## Canvas text

`skia_safe::Font::default()` carries a null typeface and silently draws no
glyphs on this stack. `ui/src/canvas_text.rs` resolves the system default
typeface once via `FontMgr` instead — needed for the ruler's bar numbers,
loop markers, and clip name headers, none of which are real Vizia `Label`
views (they're canvas-drawn, one draw call per view, per the perf goals
below).

## The timeline

### Data model (`shared::arrangement`)

- **Time** (`time.rs`): positions and lengths are `Ticks` (`i64`) at 960
  PPQ, never floats or seconds. `TempoMap` converts ticks ↔ seconds ↔
  samples and supports multiple tempo/signature changes even though the
  seed data only installs one.
- **Model** (`model.rs`): `Arrangement` holds `Track`s, `Clip`s (audio
  clips carry a `source` file name + `source_offset_samples` into it, so
  several clips can reference one continuous recording), `AutomationLane`s
  with sorted `Breakpoint`s, an optional loop range and markers.
- **Commands** (`commands.rs`): every edit is a `Command`. `Command::apply`
  mutates the arrangement *and returns the command that undoes it*, built
  from the arrangement's actual prior state — so `CommandStack` is just two
  `Vec<Command>` stacks (undo/redo), with no separate inverse-authoring
  logic to keep in sync. `Command::Batch` composes multi-step edits (split,
  multi-clip delete) with the correct reversed-order inverse.
- **Peaks** (`peaks.rs`): a three-level min/max pyramid (256/1024/4096
  samples per entry). Built on a background thread (`ui/src/timeline/
  peaks_loader.rs`, spawned via `Context::spawn`, reported back through a
  `ContextProxy` — the `Send`-safe handle for emitting events from off the
  UI thread) from WAV files decoded with `hound`. `peaks_for_range` picks
  the coarsest level that still gives roughly one peak per pixel.
- **Transform** (`transform.rs`): `ViewTransform` (pixels-per-beat, scroll
  x/y) converts tick ↔ x and implements zoom-anchored-at-cursor and grid
  snapping. Pure struct, unit tested, no Vizia dependency.

### Rendering (`ui/src/timeline`)

Track headers are ordinary Vizia views (reusing the mixer strip's M/S/Arm
buttons and colour swatch). The ruler and the lane area are each **one**
custom canvas view (`Ruler`, `LaneArea`): every draw call walks only the
visible tick range and visible rows, culling anything off-screen before
issuing a single `skia` path per element. Grid density adapts to zoom (beat
lines drop below ~6px spacing; bar numbers thin to every 2/4/8 bars).

Interactive drags (move/trim a clip, drag a loop edge, drag a breakpoint)
are *not* applied as one command per mouse-move event — each view keeps a
local, non-reactive `Option<Drag>` field with a live delta, renders the
dragged element from that preview position, and commits exactly one
`Command` on mouse-up. That keeps undo history at one entry per drag
instead of one per pixel.

### What's implemented vs. deferred

Built: the full data/command/undo model with tests, the transform with
tests, the ruler, the lane area (grid, clips with real waveform peaks and
MIDI note bars, automation editing, selection incl. rubber-band + time
selection, move/trim/split/duplicate/delete), loop-range dragging,
Ctrl/Cmd+scroll zoom, snap cycling, and playhead display driven by the
existing engine bridge.

Since then, MIDI clip playback and step-entry recording have been added
(see "Carve <-> timeline" below) - the two items below are what's still
deferred:

- **Audio clips still don't play.** MIDI clips do (see below), but the
  Drums/Lead tracks' `.wav`-backed clips are still silent during
  playback - there's no sample-playback engine yet, only the peak-pyramid
  visualization. Recording clips (`Take 3`) are still seeded as static.
- **No `--stress` flag / formal 60fps-at-200-bars validation.** The
  renderer already does visible-range culling (the mechanism a stress test
  would exercise), but there's no generated stress arrangement or
  measurement harness.
- **Follow mode pages against an assumed viewport width**
  (`ASSUMED_LANE_WIDTH` in `timeline/state.rs`), not the lane area's live
  width — Vizia's model layer doesn't have layout access, only views do.
  Close enough at this app's window size; wrong if the window is resized.
- Track height is fixed to the `size-lane`/`size-lane-auto` tokens; the
  per-track `height` field exists in the model but isn't user-adjustable.
- Multi-clip drag preserves each clip's original track (time-shift only);
  only a single selected clip can be dragged to a different track.

## Carve (the synth) and its bridge to the timeline

Carve is a real polyphonic subtractive synth (`engine::synth`), not a mock:
2 oscillators (sync/FM), sub + noise, a state-variable filter (12/24 dB,
LP/BP/HP), per-voice filter+amp ADSR, 2 LFOs each patchable to Cutoff or
Pitch, mono legato/glide or poly voice-stealing. `shared::synth::bridge`
carries a `SynthParams` snapshot UI -> engine once per frame (latest wins,
same pattern as the transport's gain/pan) and an ordered `NoteEvent`
ring buffer for note on/off; `SynthTelemetry` reports Carve's own peak
level back for its Output meter.

**Playing it**: click the on-screen keyboard, or use the computer
keyboard - `Z X C V B N M , . /` are white keys, `S D G H J L ;` are the
black keys between them, `+`/`-` shift the whole row by an octave (shown
as the "Oct" readout). There's one Carve instance for the whole song, not
one per track.

**Timeline playback** (`ui/src/timeline/scheduler.rs`): every frame,
`MidiScheduler` looks at what tick range the playhead just crossed
(`shared::arrangement::notes_in_range`, which also respects track
mute/solo) and fires the matching note on/off through Carve - so playing
the arrangement actually sounds the Bass/Pad MIDI tracks. It's polling-
resolution (~16ms), not sample-accurate; fine for now since nothing else
in the timeline is sample-accurate yet either. It reference-counts
overlapping same-pitch notes so one clip's note ending doesn't cut off
another clip's still-sounding one of the same pitch.

**Step-entry recording** (`shared::arrangement::step_entry` +
`TimelineState::commit_step`): press the transport's Record button to arm
it, arm a MIDI track, and stop the transport. Playing a note (or holding
several for a chord) on Carve and releasing every key commits that chord
as one 16th-note step at the playhead on the armed track, then advances
the playhead by a step; `Space` commits a rest (advances with no note).
Each step either extends the clip currently being built (if it directly
abuts the playhead) or starts a new one - `step_entry_commit` is a pure
function, unit tested independently of the UI.

## Known cosmetic quirk

The **first** paint of a toggled-colour button (e.g. Play) occasionally
shows a stale/default fill until it's interacted with once, after which it
renders correctly and stays correct. Confirmed via screenshot diffing that
it's not a CSS or state bug — cosmetic only, and it self-corrects on the
first click.

## Keybindings

| Keys | Action |
| --- | --- |
| `Ctrl/Cmd+T` | Toggle Studio/Daylight theme |
| `Ctrl/Cmd+Z` | Undo |
| `Ctrl/Cmd+Shift+Z` | Redo |
| `Ctrl/Cmd+E` | Split selected/overlapping clips at the playhead |
| `Ctrl/Cmd+D` | Duplicate selection (placed right after it) |
| `Delete` / `Backspace` | Delete selected clips |
| `F` | Toggle Follow |
| `Z X C V B N M , . /` | Play Carve (white keys) |
| `S D G H J L ;` | Play Carve (black keys) |
| `+` / `-` | Shift Carve's computer-keyboard octave |
| `Space` | Step-entry rest (while armed and stopped) |

Mouse: click a clip to select (Shift extends); drag a clip body to move it
(vertically too, for a single selection); drag within 6px of an edge to
trim; drag on empty lane space for a rubber-band + time selection; click an
automation lane to add a breakpoint, drag an existing one to move it,
double-click to delete it; drag the ruler to scrub the (display-only)
playhead; drag a loop edge/middle in the ruler to resize/move the loop;
Alt/Option bypasses snapping while dragging; Ctrl/Cmd+scroll zooms anchored
at the cursor; Shift+scroll pans horizontally, plain scroll pans
vertically.

## Verifying it

```
cargo test --workspace     # 48 tests: time/transform/commands/peaks/schedule/step_entry/synth curves
cargo clippy --workspace --all-targets   # clean
cargo run -p ui
```
