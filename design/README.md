# Strata

Strata is the design system for a professional DAW: an arrangement, a session grid, devices like the Carve synth, and editors for MIDI and audio. People spend hours in it, often late at night and next to other tools, so the interface stays quiet. Your attention should go to the music, and the UI should only raise its voice when something is actually happening.

## Principles

1. **The music is the colour.** Surfaces, controls, knob arcs and displays are neutral greys and ink. Colour appears in only two ways:
   - **State:** `signal` when sound is playing, `record`, `warn` for solo and hot levels, and `mod` for modulation.
   - **Clip colours**, chosen by the musician.

   If something is coloured, it is telling you something.
2. **Hierarchy by size.** Knobs come in three sizes:
   - `size-knob-lg` for the control a device is played with (Carve's Cutoff). A device has at most two.
   - `size-knob` for the main controls.
   - `size-knob-sm` for trims.

   You should be able to tell what matters on a device without reading a single label.
3. **Group with lines and space, not boxes.** A device is one flat panel. Its sections sit side by side, separated by a `line` hairline, and are named with a `title`. Never put a card inside a card.
4. **One typeface.** IBM Plex Sans in four sizes (11, 12, 13 and 15px). Labels are sentence case. Plex's figures are tabular, so values hold still while they change.
5. **The layout explains itself.** Nothing is printed to explain the interface: no arrows showing signal flow, no "drag here" hints. The order of sections, left to right, is the signal path. Tooltips and the manual carry the rest.

## Voice

Plain, precise, musician-first.
- **Controls are nouns in sentence case:** "Cutoff", "Resonance", "Send A".
- **Values always carry their unit**, spaced and abbreviated: `-6.2 dB`, `1.20 kHz`, `320 ms`, `+7 ct`, `1/8`. Use a true minus sign.
- **Menus are verbs:** "Duplicate scene", "Consolidate".
- **No exclamation marks**, no jokes in errors.

## Colour

Two themes: **Studio** (dark, the default) and **Daylight**. The greys step from `bg-000` (ground and wells) through `bg-100` (panels) and `bg-200` (controls at rest) to `bg-300` (hover and pressed). A neutral toggle that is on uses `bg-400` with `ink` text; it never uses an accent.

| Meaning | Fill | On it | Used for |
| --- | --- | --- | --- |
| Sound is happening | `signal` | `on-signal` | Play while running, playing clips, sounding notes, held pads, meter ≤ −12 dB |
| Recording | `record` | `on-record` | Record, arm, recording clips, clip LED |
| Attention | `warn` | `on-warn` | Solo, meter −12 to 0 dB |
| Modulation | `mod`, `mod-soft` | `on-mod` | Modulator chips, knob rings, ranges on displays, the loop range |
| Clips | `clip-*` | `on-clip` | The musician's colours: muted, the same in both themes |

The one place the track colour appears outside clips is **knob value arcs**. A device's knobs take its track's colour (`clip-*-line`), muted and one hue per device, so you can tell at a glance which track you're editing.

Everything else is ink:
- knob arcs with no track (global settings, the master)
- waveform and envelope lines
- the playhead
- selection outlines
- focus rings

Play and record differ in glyph (a triangle and a circle) as well as in colour and lightness. Never rely on hue alone.

## Type

- **IBM Plex Sans** at Regular, Medium and SemiBold. The font files are in `fonts/`. Plex has no ♭ or ♯ glyphs, so those come from a fallback font.
- **Styles:**
  - `heading` 15px: device names
  - `title` 13px: sections and tracks
  - `control` and `body` 12px
  - `label` and `value` 11px
  - `readout` 13px: transport numbers
- `display` is for the cover and onboarding only.
- **Mono** is reserved for code, such as the Faust editor.

## Shape and space

- **Corners are small:** 2px on clips and pads, 3px on controls, 4px on the device frame and menus. `radius-pill` is only for modulator chips.
- **Grid:** 4px (`space-1` to `space-8`). Controls are `size-control` (22px) tall.
- **Panels are flat.** `shadow-pop` is only for menus and popovers.

## Iconography

There's no logo yet; the wordmark is "Strata" in Plex Sans SemiBold. Icons are filled geometry on a 12px box (play triangle, stop square, record dot, loop arrows) drawn in the current text colour. Icons are never outlined, decorative or skeuomorphic.

## Changes from Strata 1

- **Renamed tokens:** `volt` → `signal`, `on-volt` → `on-signal`, `volt-soft` → `signal-soft`, `hot` → `warn`, `on-hot` → `on-warn`.
- **New tokens:** `bg-400`, `clip-edge`, `clip-*-line`, `size-knob-sm`, `size-knob-lg` and `size-toolbar`.
- **Ink instead of colour:** display lines and the playhead moved from the accent colour to ink.
- **Track-coloured knobs:** knob arcs take their track's muted colour via the new `clip-*-line` tokens.
- **Lighter structure:** sections are no longer cards, labels are no longer uppercase, and mono is no longer used for values.
