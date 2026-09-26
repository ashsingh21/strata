# Strata 2: what changed

Strata 1 looked like a generic dark "tech" template: an acid accent on every control, cards inside cards, and uppercase mono labels. Strata 2 is quieter and more hierarchical, like a professional DAW.

## Token renames (find and replace)

| Strata 1 | Strata 2 |
| --- | --- |
| `volt` | `signal` |
| `on-volt` | `on-signal` |
| `volt-soft` | `signal-soft` |
| `hot` | `warn` |
| `on-hot` | `on-warn` |

## New tokens

- `bg-400`: the neutral "on" state for toggles and selected segments.
- `clip-coral-line` … `clip-pink-line`: each track colour as a line, for knob value arcs. In Studio these alias the clip colour; in Daylight they are darker so the arc holds 3:1.
- `clip-edge`: a 1px inner edge on clips.
- `size-knob-sm` (24px), `size-knob-lg` (48px) and `size-toolbar` (40px).

## Changed values

- **Colours:** every colour value changed, with neutral greys that are less contrasty, muted clip colours, and a green `signal`.
- **Sizes:**
  - `size-control` is 22px (was 24).
  - `size-clip` is 24px (was 28).
  - `size-ruler` is 24px (was 28).
  - `size-track-head` is 184px (was 176).
- **Radii** are 2 / 3 / 4px.

## Rule changes

1. **Colour only for state:**
   - Display lines (waveforms, filter curve, envelopes), the playhead and the focus ring are now `ink`, not the accent.
   - Knob value arcs take their track's colour: each device frame and mixer strip sets `--knob-accent` to its track's `clip-*-line` token. Knobs with no track (global settings, the master) stay `ink`. Use one hue per device, with no glow or gradient.
   - Colour appears only for `signal` (sound playing), `record`, `warn` (solo and level), `mod` (modulation) and clip colours.
2. **Neutral toggles:** an enabled toggle (Sync, Warp, Follow, Mute) or a selected segment is `bg-400` with `ink` text. It is no longer inverted to full ink, and never uses an accent.
3. **One typeface:**
   - IBM Plex Sans in 11 / 12 / 13 / 15px.
   - Labels are sentence case, with no uppercase or letter-spacing.
   - Values use Plex, not mono. Plex figures are tabular by default.
   - Plex has no ♭ or ♯ glyphs, so those need a fallback font.
4. **Knob hierarchy:** there are three sizes. A device's main performance control is `lg` (Carve: Cutoff), trims are `sm`, and knob rows align on their labels.
5. **Devices are one flat panel:**
   - Sections are separated by `line` hairlines, not boxed cards.
   - Section titles are `title` (13px SemiBold), with mode controls right-aligned in the section header.
   - Remove printed explanations: arrows like "→ mix", the Osc → Mix → Filter flow strip, and hints like "click target to reassign".
6. **Workspace layout:**
   - The top bar is 40px.
   - The lower panel spans the arrangement width, and the device fills it. Header controls sit inside the device header.
   - Track headers are neutral, with an 8px colour swatch; they are no longer filled with the clip colour.
   - Add-track actions are quiet buttons under the last track.
   - There is a status bar at the bottom.
