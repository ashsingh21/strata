# Sidebar

The left sidebar: a 44px icon rail plus a 236px panel. It replaces the plain browser list. The rail stays when the panel is collapsed, so the arrangement gets the width back without losing the way in.

**Rail** (`bg-000`)
- Top group: Browse, Samples, Presets, Project files, History.
- Bottom group: show/hide panel, and Settings.
- Buttons are 32px with 16px stroked icons in `ink-muted`. The active one is `bg-400` with `ink`, and every button has a tooltip.
- Clicking the active icon collapses the panel. Clicking another switches the panel.

**Panel**
- **Header:** the section title and the result count.
- **Search** (28px, ⌘F): searches everything in this section.
- **Type chips** (All, Instruments, Effects, Samples, Faust): pills that turn `bg-400` when on. They filter; they don't navigate.
- **Fits key:** a neutral toggle that keeps only samples and presets in the project key and interval set, shown next to it ("A min pent"). This is Strata's own touch: the browser knows the project key.
  - In-key results carry a small key tag on `signal-soft`.
  - Out-of-key results show their key struck through in `ink-muted`.
  - Sort sits at the right as a quiet dropdown.
- **Collections:** Favourites and user collections, each with a colour dot and a count. "+ New" makes one. The selected collection is `bg-200`, and it filters the results.
- **Results:** 28px rows with a type glyph, the name, a ★ if it's a favourite, and metadata (BPM for samples, device for presets, category for effects).
  - Hover is `bg-300` and selected is `bg-400`.
  - Hovering or selecting a row shows a round preview button, which fills `signal` while it plays.
  - Rows drag onto tracks, clips, devices and the effects board.
- **Preview player,** docked at the bottom:
  - The previewing item, with tempo and length.
  - A waveform on `bg-000`, with the played part in `ink` and the rest in `ink-faint`.
  - Stop (`signal` while playing), Sync to project tempo, Loop, and a preview volume.

**Sizing**
- The panel is resizable from 200 to 360px (default 236).
- Collapsed, only the rail shows, with the show-panel button on.
- The panel remembers its width and section per project.

The consumer provides the section, the results (type, name, metadata, key, favourite), collections, filter state and the preview's audio and progress.
