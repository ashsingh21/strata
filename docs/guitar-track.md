# Guitar track: brief for Opus

Written by Fable, 2026-09-29, from the user's request and four Guitar Rig
screenshots (a Fender-style "Twang Reverb" amp with Bass/Mid/Treble/
Reverb/Speed/Vibrato/Volume, a Cabinet + Room strip, a Boss CE-style
chorus/vibrato, a compressor with Detector HP / Sidechain / Threshold /
Compression / Gain / Dry, and a tape echo with Echo/Verb sections plus
Dropouts/Noise/Warble/Headroom/Head Mix/Reverb Time/Spring Length). A
noise gate at -71 dB sits in the header.

The ask: a guitar track whose FX offer guitar effects and an amp sim, with
Neural Amp Modeler (NAM) for the amp.

This extends the parked "Guitar track with live FX + input monitoring"
backlog item; its ordering still holds. Read that first.

## Decisions to keep

- **Not a new `TrackKind`.** A guitar track is an audio track: "+ Guitar"
  under Add makes an audio track named Guitar, armed, monitoring on, with a
  starter chain (Gate -> Amp -> Cab). A new kind would touch every match on
  `TrackKind` for nothing the chain doesn't already give us; bass and
  vocals want the same effects anyway.
- **Effects are `Effect` variants**, like Compressor and Eq
  (`shared/src/arrangement/model.rs`, `Effect`, `EffectParam::for_effect`;
  engine `engine/src/effects.rs` `EffectUnit`; UI `ui/src/effect_panel.rs`).
  Same pattern, same knob, same panel. In the add menu, group them under
  "Guitar" so the menu doesn't become one long list.
- **Shor's look, not Guitar Rig's.** No tolex, grille cloth or script
  logos. Flat panels, our knobs, a colour tag per device is plenty. What
  we take from the screenshots is which knobs matter, not the faceplates.
- **Real-time rules stand:** nothing allocates in the audio callback.
  A NAM model or an IR is decoded on the UI thread and handed over the way
  decoded samples are (`DecodedSource` through the ring buffer), never
  loaded in the callback.

## Phase 0 (prerequisite): hear yourself

The engine has no input monitoring (`engine/src/input.rs` records only;
input and output are separate cpal streams). A guitar track is pointless
until the player hears the amp while playing.

1. Input samples -> a small rtrb ring -> the output callback, mixed
   through the armed track's chain and fader. One buffer of latency is
   unavoidable with two streams; request small buffers (64-128 frames) on
   both. Check whether cpal on CoreAudio lets us open a duplex stream on
   the user's Behringer; if so, prefer it.
2. The master limiter's ~2 ms lookahead adds to the round trip. Either
   bypass lookahead while a track monitors or accept it; measure first.
3. **Measure round-trip latency** (loopback cable, or clap test) on the
   user's Mac + Behringer before building anything else on top. Under
   ~10 ms feels direct; 20 ms is playable; more is not. Show the measured
   figure in the status bar while monitoring.
4. Record dry, monitor wet: the take on disk is the raw input, the chain
   is applied on playback, so a tone can be changed after recording.

Stop here and confirm the numbers with the user before Phase 1.

## Phase 1: playable end to end

- **"+ Guitar"** button and the starter chain.
- **Tuner:** a panel, not an effect node: pitch detection (YIN or
  McLeod) on the input, note name + cents, a needle. Guitarists use this
  every time they pick up the instrument; it's the first thing a lesson
  will teach. Test: 110.0 Hz sine reads A2 within ±1 cent.
- **Noise gate:** Threshold, Release (Attack fixed and fast). The header
  "Gate" in the screenshots. Test: silence below threshold, unity above,
  no click on open/close.
- **Amp:** one device with three stages so it works with or without a NAM
  file:
  - Drive: a plain waveshaper (tanh with a pre-gain) as the fallback when
    no model is loaded; Gain knob.
  - Model slot: a `.nam` file (Phase 2); when loaded it replaces the
    waveshaper, Gain becomes the model's input trim.
  - Tone stack after it: Bass / Mid / Treble (three shelves/peaks from
    `shared::eq`, fixed frequencies ~100 Hz / 800 Hz / 3 kHz) and Volume.
    NAM captures bake the amp's own knobs in at one setting; this stack is
    what gives the player something to turn.
- **Cabinet:** an impulse-response convolver with a bundled IR and a file
  slot. Zero-latency partitioned convolution (direct convolution for the
  first ~256 taps, uniform-partition FFT for the tail) - per-track FX
  have no latency compensation, so the cab must not add any. Plus a
  "Room" knob: a short algorithmic ambience (the existing `Reverb` in
  `engine/src/fx.rs` at small size, low mix) so a dry IR doesn't sound
  like headphones. Bundled IRs must be CC0 or made by us (a synthetic
  4x12-ish IR from a filtered decaying noise burst is an acceptable
  placeholder; say so in the file's name). Test: convolving an impulse
  returns the IR; a 2048-tap IR costs under ~3% of a core at 48 kHz.

## Phase 2: NAM

- Format: a `.nam` file is JSON - `architecture` ("WaveNet", "LSTM",
  "ConvNet"), `config`, and `weights` (a flat float array). Reference
  implementation: NeuralAmpModelerCore (C++, MIT). Check crates.io for a
  maintained Rust inference crate first; if none, port the WaveNet path
  (dilated 1-D convolutions with gated activations, two stacks in the
  "standard" config - a few hundred lines). LSTM second. Skip ConvNet.
- Sample rate: models are captured at 48 kHz; resample the input when the
  engine runs at 44.1 kHz (the engine already converts decoded sources,
  reuse that path for the model's I/O, or run the engine at 48 kHz when a
  NAM track exists).
- CPU: a "standard" WaveNet is the most expensive thing in the app by far
  (tens of µs per sample on a laptop core). Support "lite"/"feather"
  models, show the load in the existing CPU meter, and cap it at one NAM
  instance per project in v1 with a clear message rather than crackling.
- Correctness: NeuralAmpModelerCore's repo has test models; if we can run
  the C++ reference once to produce expected output for a fixed input,
  check ours against it to 1e-4. Otherwise: DC in -> near-DC out,
  bounded output, no NaNs.
- Models: don't ship other people's captures (ToneHub/Tone3000 licences
  vary). Ship none, or one captured by the user from their own gear.
  "Load a .nam" is the feature; point at where to find free ones.

## Phase 3: the rest of the board

In Shor's idiom, one node each:

- **Chorus / Vibrato** (the blue CE-style unit): Blend (vibrato <-> chorus,
  i.e. dry mix 0..50%), Rate, Depth ("Mode I/II" = two depth/rate
  presets; make it a Depth knob), Spread (stereo). Reuse `Chorus` from
  `engine/src/fx.rs`.
- **Compressor:** already exists. Add **Dry** (parallel mix - the one
  knob guitarists actually use on it) and an optional **Detector HP**
  (sidechain high-pass; the backlog note that an HPF hurts bass tracks is
  why it's a knob at 0 by default, not always on).
- **Tape echo:** Time (with tap tempo and a sync toggle to the project
  tempo), Feedback, Mix, Echo tone (Bass/Treble on the repeats), and one
  "Tape" knob that scales warble + noise + dropouts together - the
  screenshot's four tape knobs are more than a learner wants; one knob
  from clean digital to worn tape is the teachable version.
- **Spring reverb:** the Dattorro plate (`Reverb`) with short predelay,
  bandpassed input (~300 Hz - 4 kHz) and a "drip" allpass chain on the
  input gives a usable spring; a physically modelled spring is not worth
  it in v1. Knobs: Length, Tone, Mix.

Each: `Effect` variant + state struct with serde defaults, `EffectParam`s
(norm/apply_norm/format), engine unit, panel, automation works for free,
an engine test that the effect does what its name says.

## Phase 4: presets and lessons

- Starter chains as named presets on the Amp device: Clean, Crunch,
  Lead, Ambient. Each is a whole chain (gate/amp/cab/echo/verb settings).
- Lessons, group "Guitar", in the panel format:
  1. Plug in and tune up (monitoring, the tuner, the gate).
  2. Your first tone (drive, tone stack, cab: what each knob does, with
     Before/After).
  3. Space and movement (echo, chorus, spring).
  4. Record a take (dry take, change the tone afterwards, comp two takes).
  Same tests as the rest of the course: Show me per action step, checks
  false at start.

## Not in scope

- A pedalboard canvas with cables (the Effects Board already is one).
- Amp/cab *capture* (making .nam files) - that's NAM's own trainer.
- Impulse-response *creation* tools.
- MIDI control of pedals, expression pedals.
- A "guitar" MIDI instrument (that's a Carve preset, not this).

## Size and order

Phase 0 is a few days and the risky part (latency depends on hardware we
can't test here). Phases 1-2 together are two to three weeks; 3-4 another
one to two. Do 0 alone, measure, report, then 1.

Sequencing against everything else: the user was advised to ship the DMG
to friends before adding features. This is the one feature that would
make the user play their own guitar through Shor daily, which is where
bugs get found, so it's a reasonable "next big thing" after that first
round of feedback. Not before the DMG works on a Mac.

## Questions for the user before starting

1. Audio interface: the Behringer's model and whether it works with the
   Mac's built-in audio as a duplex device (for the latency test).
2. Engine sample rate: fix at 48 kHz when a guitar track exists, or
   resample per model?
3. Does the user have a `.nam` capture of their own amp, or should Phase
   2 be validated purely on the NAM repo's test models?
