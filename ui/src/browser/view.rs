//! The sidebar's views: the icon rail, the panel (header, search, chips,
//! Fits key and sort, collections, results or the lessons, the preview
//! dock) and its resize edge. Drag and drop uses Vizia's own: a row's
//! `on_drag` starts it, and `on_drop` on a lane, a track header, the
//! device panel or a collection receives it; `DRAGGED` says what item is
//! in flight (Vizia only passes the row's entity), and `DragGhost` draws
//! its name by the pointer.

use std::cell::{Cell, RefCell};
use std::sync::Arc;

use vizia::prelude::*;
use vizia::vg;

use super::icon::{muted_or_ink, Icon, IconKind};
use super::items::{Chip, Item, Kind};
use super::preview::PreviewEvent;
use super::{results, BrowserEvent, BrowserModel, Coll, Collection, Filter, Section, Sort};
use crate::hidpi::Logical;
use crate::lessons::LessonTargetExt;
use crate::tokens::{self, ThemeId};

thread_local! {
    /// The search box, for Ctrl/Cmd+F.
    pub static SEARCH: Cell<Option<Entity>> = const { Cell::new(None) };
    /// The item being dragged, if any.
    static DRAGGED: RefCell<Option<Item>> = const { RefCell::new(None) };
    /// Where the pointer is while dragging (logical), for the ghost.
    static GHOST: Cell<Option<Signal<Option<(f32, f32, String)>>>> = const { Cell::new(None) };
}

/// The item being dragged from the browser, if one is.
pub fn dragged() -> Option<Item> {
    DRAGGED.with(|d| d.borrow().clone())
}

fn end_drag() {
    DRAGGED.with(|d| *d.borrow_mut() = None);
    if let Some(g) = GHOST.get() {
        if g.get().is_some() {
            g.set(None);
        }
    }
}

fn tip<'a>(cx: &'a mut Context, text: &'static str) -> Handle<'a, Tooltip> {
    Tooltip::new(cx, move |cx| {
        Label::new(cx, text);
    })
    .placement(Placement::Bottom)
    .arrow(false)
}

/// Everything the views read, copied out of the model before it's built.
#[derive(Clone, Copy)]
pub struct BrowserProps {
    open: Signal<bool>,
    section: Signal<Section>,
    width: Signal<f32>,
    query: Signal<String>,
    chip: Signal<Chip>,
    fits_key: Signal<bool>,
    sort: Signal<Sort>,
    sort_open: Signal<bool>,
    settings_open: Signal<bool>,
    favourites: Signal<Vec<String>>,
    collections: Signal<Vec<Collection>>,
    collection: Signal<Option<Coll>>,
    renaming: Signal<Option<usize>>,
    history: Signal<Vec<String>>,
    selected: Signal<Option<String>>,
    keys: Signal<std::collections::HashMap<Arc<str>, super::keys::SampleKey>>,
    all: Signal<Arc<Vec<Item>>>,
    pv_item: Signal<Option<Item>>,
    pv_playing: Signal<bool>,
    pv_loading: Signal<bool>,
    pv_progress: Signal<f32>,
    pv_looping: Signal<bool>,
    pv_sync: Signal<bool>,
    pv_volume: Signal<f32>,
    pv_peaks: Signal<Arc<Vec<f32>>>,
    pv_info: Signal<String>,
    pub theme: Signal<ThemeId>,
    key: Signal<u8>,
    scale_mask: Signal<u16>,
    lessons_active: Signal<Option<(usize, usize)>>,
    lessons_done: Signal<Vec<String>>,
    ghost: Signal<Option<(f32, f32, String)>>,
}

impl BrowserProps {
    #[allow(clippy::too_many_arguments)]
    pub fn of(
        m: &BrowserModel,
        theme: Signal<ThemeId>,
        key: Signal<u8>,
        scale_mask: Signal<u16>,
        lessons_active: Signal<Option<(usize, usize)>>,
        lessons_done: Signal<Vec<String>>,
    ) -> Self {
        let ghost = Signal::new(None);
        GHOST.set(Some(ghost));
        Self {
            open: m.open,
            section: m.section,
            width: m.width,
            query: m.query,
            chip: m.chip,
            fits_key: m.fits_key,
            sort: m.sort,
            sort_open: m.sort_open,
            settings_open: m.settings_open,
            favourites: m.favourites,
            collections: m.collections,
            collection: m.collection,
            renaming: m.renaming,
            history: m.history,
            selected: m.selected,
            keys: m.keys,
            all: m.all,
            pv_item: m.preview.item,
            pv_playing: m.preview.playing,
            pv_loading: m.preview.loading,
            pv_progress: m.preview.progress,
            pv_looping: m.preview.looping,
            pv_sync: m.preview.sync,
            pv_volume: m.preview.volume_db,
            pv_peaks: m.preview.peaks,
            pv_info: m.preview.info,
            theme,
            key,
            scale_mask,
            lessons_active,
            lessons_done,
            ghost,
        }
    }
}

/// The whole sidebar: rail, then (while open) the panel and its edge.
pub fn sidebar(cx: &mut Context, p: BrowserProps) {
    HStack::new(cx, move |cx| {
        rail(cx, p);
        hairline(cx);
        panel(cx, p);
        EdgeHandle::new(cx, p.width).toggle_class("hidden", p.open.map(|o| !*o));
        hairline(cx).toggle_class("hidden", p.open.map(|o| !*o));
    })
    .width(Auto)
    .height(Stretch(1.0));
}

/// A 1px divider (one-sided borders don't draw in this Vizia).
fn hairline(cx: &mut Context) -> Handle<'_, Element> {
    Element::new(cx).class("hairline").width(Pixels(1.0)).height(Stretch(1.0))
}

fn rail_button<'a>(cx: &'a mut Context, p: BrowserProps, kind: IconKind, label: &'static str, on: Memo<bool>) -> Handle<'a, Button> {
    Button::new(cx, move |cx| Icon::new(cx, kind, 16.0, on, p.theme, muted_or_ink))
        .class("brw-rail-btn")
        .toggle_class("is-on", on)
        .alignment(Alignment::Center)
        .width(Pixels(32.0))
        .height(Pixels(32.0))
        .tooltip(move |cx| tip(cx, label).placement(Placement::Right))
}

fn rail(cx: &mut Context, p: BrowserProps) {
    VStack::new(cx, move |cx| {
        for section in Section::RAIL {
            let on = Memo::new(move |_| p.open.get() && p.section.get() == section);
            rail_button(cx, p, section.icon(), section.title(), on).on_press(move |cx| cx.emit(BrowserEvent::Rail(section)));
        }
        Element::new(cx).height(Stretch(1.0)).width(Pixels(1.0));
        let open = Memo::new(move |_| p.open.get());
        rail_button(cx, p, IconKind::Panel, "Show or hide the panel", open).on_press(|cx| cx.emit(crate::app::AppEvent::ToggleSidebar));
        let settings = Memo::new(move |_| p.settings_open.get());
        rail_button(cx, p, IconKind::Settings, "Settings", settings).on_press(|cx| cx.emit(BrowserEvent::ToggleSettings));
        settings_menu(cx, p);
    })
    .class("brw-rail")
    .alignment(Alignment::TopCenter)
    .gap(Pixels(4.0))
    .padding_top(Pixels(8.0))
    .padding_bottom(Pixels(8.0))
    .width(Pixels(super::RAIL_WIDTH))
    .height(Stretch(1.0));
}

/// Settings: the theme (otherwise only a keyboard shortcut).
fn settings_menu(cx: &mut Context, p: BrowserProps) {
    VStack::new(cx, move |cx| {
        Label::new(cx, "Theme").class("label");
        HStack::new(cx, move |cx| {
            for (name, daylight) in [("Studio", false), ("Daylight", true)] {
                Button::new(cx, move |cx| Label::new(cx, name))
                    .class("btn")
                    .toggle_class("is-on", p.theme.map(move |t| t.is_daylight() == daylight))
                    .on_press(move |cx| {
                        if p.theme.get().is_daylight() != daylight {
                            cx.emit(crate::app::AppEvent::ToggleTheme);
                        }
                        cx.emit(BrowserEvent::ToggleSettings);
                    });
            }
        })
        .gap(Pixels(4.0))
        .size(Auto);
    })
    .class("panel")
    .class("context-menu")
    .toggle_class("hidden", p.settings_open.map(|o| !*o))
    .gap(Pixels(6.0))
    .padding(Pixels(tokens::SPACE_2))
    .position_type(PositionType::Absolute)
    .left(Pixels(44.0))
    .bottom(Pixels(8.0))
    .z_index(10)
    .size(Auto);
}

fn results_memo(p: BrowserProps) -> Memo<Vec<Item>> {
    Memo::new(move |_| {
        let (favourites, collections, history, keys) = (p.favourites.get(), p.collections.get(), p.history.get(), p.keys.get());
        let query = p.query.get();
        results(
            &p.all.get(),
            &Filter {
                section: p.section.get(),
                query: &query,
                chip: p.chip.get(),
                fits_key: p.fits_key.get(),
                sort: p.sort.get(),
                collection: p.collection.get(),
                favourites: &favourites,
                collections: &collections,
                history: &history,
                keys: &keys,
                project_key: (p.key.get(), p.scale_mask.get()),
            },
        )
    })
}

fn panel(cx: &mut Context, p: BrowserProps) {
    let shown = results_memo(p);
    VStack::new(cx, move |cx| {
        // Header: the section and how many results.
        HStack::new(cx, move |cx| {
            Label::new(cx, p.section.map(|s| s.title())).class("title");
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, shown.map(|r| r.len().to_string())).class("value");
        })
        .alignment(Alignment::Left)
        .padding_left(Pixels(tokens::SPACE_3))
        .padding_right(Pixels(tokens::SPACE_3))
        .height(Pixels(40.0));

        // Search.
        HStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Icon::new(cx, IconKind::Search, 12.0, Signal::new(false), p.theme, muted_or_ink);
            let search = Textbox::new(cx, p.query)
                .placeholder(p.section.map(|s| if *s == Section::Browse { "Search everything".to_string() } else { format!("Search {}", s.title().to_lowercase()) }))
                .on_edit(|cx, text| cx.emit(BrowserEvent::SetQuery(text)))
                .class("brw-search-field")
                .width(Stretch(1.0))
                .height(Stretch(1.0));
            SEARCH.set(Some(search.entity()));
            Label::new(cx, crate::shortcut("Ctrl+F")).class("brw-kbd");
        })
        .class("brw-search")
        .alignment(Alignment::Left)
        .gap(Pixels(6.0))
        .padding_left(Pixels(8.0))
        .padding_right(Pixels(6.0))
        .width(Stretch(1.0))
        .height(Pixels(28.0));
        })
        .padding_left(Pixels(tokens::SPACE_3))
        .padding_right(Pixels(tokens::SPACE_3))
        .width(Stretch(1.0))
        .height(Pixels(28.0));

        // The filters, rebuilt when the section changes (hiding and
        // re-showing them left them blank).
        let layout = Memo::new(move |_| p.section.get());
        Binding::new(cx, layout, move |cx| {
            let section = layout.get();
                // Type chips (Browse only), in two rows.
                VStack::new(cx, move |cx| {
                    for row in [&Chip::ALL[..3], &Chip::ALL[3..]] {
                        HStack::new(cx, move |cx| {
                            for &chip in row {
                                Button::new(cx, move |cx| Label::new(cx, chip.label()))
                                    .class("brw-chip")
                                    .height(Pixels(22.0))
                                    .toggle_class("is-on", p.chip.map(move |c| *c == chip))
                                    .on_press(move |cx| cx.emit(BrowserEvent::SetChip(chip)));
                            }
                        })
                        .gap(Pixels(4.0))
                        .size(Auto);
                    }
                })
                .toggle_class("hidden", section != Section::Browse)
                .gap(Pixels(4.0))
                .padding_left(Pixels(tokens::SPACE_3))
                .padding_top(Pixels(tokens::SPACE_2))
                .height(Auto);

                // Fits key, and the sort.
                HStack::new(cx, move |cx| {
                    Button::new(cx, |cx| Label::new(cx, "Fits key"))
                        .class("btn")
                        .class("sm")
                        .toggle_class("is-on", p.fits_key)
                        .tooltip(move |cx| tip(cx, "Only samples in the project's key (drums and presets always fit)"))
                        .on_press(|cx| cx.emit(BrowserEvent::ToggleFitsKey));
                    Label::new(cx, Memo::new(move |_| super::key_label(p.key.get(), p.scale_mask.get()))).class("value").text_wrap(false);
                    Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                    Button::new(cx, move |cx| Label::new(cx, p.sort.map(|s| format!("{} \u{25be}", s.label()))))
                        .class("btn")
                        .class("sm")
                        .class("quiet")
                        .on_press(|cx| cx.emit(BrowserEvent::ToggleSortMenu));
                    VStack::new(cx, move |cx| {
                            for sort in Sort::ALL {
                                Button::new(cx, move |cx| Label::new(cx, sort.label()).class("body"))
                                    .class("menu-item")
                                    .toggle_class("is-on", p.sort.map(move |s| *s == sort))
                                    .width(Stretch(1.0))
                                    .on_press(move |cx| cx.emit(BrowserEvent::SetSort(sort)));
                            }
                        })
                    .class("panel")
                    .class("context-menu")
                    .toggle_class("hidden", p.sort_open.map(|o| !*o))
                    .position_type(PositionType::Absolute)
                    .top(Pixels(28.0))
                    .right(Pixels(tokens::SPACE_3))
                    .z_index(10)
                    .width(Pixels(96.0))
                    .height(Auto);
                })
                .toggle_class("hidden", !section.filters())
                .alignment(Alignment::Left)
                .gap(Pixels(6.0))
                .padding_left(Pixels(tokens::SPACE_3))
                .padding_right(Pixels(tokens::SPACE_3))
                .padding_top(Pixels(tokens::SPACE_2))
                .z_index(5)
                .height(Pixels(26.0));

                // Collections.
                VStack::new(cx, move |cx| collections(cx, p))
                    .toggle_class("hidden", !section.filters())
                    .height(Auto);

                // Results (or, in Learn, the course).
                HStack::new(cx, move |cx| {
                    Label::new(cx, "Results").class("label");
                    Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
                    Label::new(
                        cx,
                        Memo::new(move |_| match p.collection.get() {
                            Some(Coll::Favourites) => "Favourites".to_string(),
                            Some(Coll::User(i)) => p.collections.get().get(i).map(|c| c.name.clone()).unwrap_or_default(),
                            None => String::new(),
                        }),
                    )
                    .class("value");
                })
                .toggle_class("hidden", !section.filters())
                .alignment(Alignment::BottomLeft)
                .padding_left(Pixels(tokens::SPACE_3))
                .padding_right(Pixels(tokens::SPACE_3))
                .padding_top(Pixels(tokens::SPACE_3))
                .padding_bottom(Pixels(tokens::SPACE_1))
                .height(Auto);
        });

        // Rows also show favourites and key tags: rebuilt when those
        // change too, not just the list.
        let decorations = Memo::new(move |_| (p.keys.get().len(), p.favourites.get(), p.key.get(), p.scale_mask.get()));
        ScrollView::new(cx, move |cx| {
            Binding::new(cx, decorations, move |cx| {
            Binding::new(cx, shown, move |cx| {
                let items = shown.get();
                if p.section.get() == Section::Learn {
                    learn_list(cx, p);
                    return;
                }
                if items.is_empty() {
                    let empty = match p.section.get() {
                        Section::History => "Things you use show up here.",
                        _ => "Nothing matches.",
                    };
                    Label::new(cx, empty).class("value").padding(Pixels(tokens::SPACE_3));
                }
                for item in items {
                    result_row(cx, p, item);
                }
            });
            });
        })
        .show_horizontal_scrollbar(false)
        .show_vertical_scrollbar(false)
        .padding_left(Pixels(tokens::SPACE_2))
        .padding_right(Pixels(tokens::SPACE_2))
        .width(Stretch(1.0))
        .height(Stretch(1.0));

        Element::new(cx).class("hairline").height(Pixels(1.0)).width(Stretch(1.0)).toggle_class("hidden", p.pv_item.map(|i| i.is_none()));
        preview_dock(cx, p);
    })
    .class("brw-panel")
    .toggle_class("hidden", p.open.map(|o| !*o))
    .width(p.width.map(|w| Pixels(*w)))
    .height(Stretch(1.0));
}

fn collection_row<'a>(cx: &'a mut Context, p: BrowserProps, coll: Coll, name: String, swatch: Color, count: usize) -> Handle<'a, HStack> {
    HStack::new(cx, move |cx| {
        Element::new(cx).class("brw-swatch").background_color(swatch).width(Pixels(8.0)).height(Pixels(8.0));
        let editing = Memo::new(move |_| matches!(coll, Coll::User(i) if p.renaming.get() == Some(i)));
        Label::new(cx, name.clone()).class("body").text_wrap(false).text_overflow(TextOverflow::Ellipsis).width(Stretch(1.0)).toggle_class("hidden", editing);
        if let Coll::User(i) = coll {
            let draft = Signal::new(name.clone());
            Textbox::new(cx, draft)
                .on_edit(move |_, t| draft.set(t))
                .on_submit(move |cx, t, _| cx.emit(BrowserEvent::Rename(i, t)))
                .on_blur(move |cx| cx.emit(BrowserEvent::Rename(i, draft.get())))
                .class("search")
                .width(Stretch(1.0))
                .toggle_class("hidden", editing.map(|e| !*e));
        }
        Label::new(cx, count.to_string()).class("value");
        if let Coll::User(i) = coll {
            Button::new(cx, move |cx| Icon::new(cx, IconKind::Close, 10.0, Signal::new(false), p.theme, muted_or_ink))
                .class("brw-mini")
                .alignment(Alignment::Center)
                .width(Pixels(16.0))
                .height(Pixels(16.0))
                .tooltip(move |cx| tip(cx, "Delete this collection"))
                .on_press(move |cx| cx.emit(BrowserEvent::DeleteCollection(i)));
        }
    })
    .class("brw-coll")
    .toggle_class("is-on", p.collection.map(move |c| *c == Some(coll)))
    .alignment(Alignment::Left)
    .gap(Pixels(8.0))
    .padding_left(Pixels(tokens::SPACE_2))
    .padding_right(Pixels(tokens::SPACE_2))
    .height(Pixels(26.0))
    .on_press(move |cx| cx.emit(BrowserEvent::SelectCollection(coll)))
    .on_double_click(move |cx, _| {
        if let Coll::User(i) = coll {
            cx.emit(BrowserEvent::BeginRename(i));
        }
    })
    // Drop a result here to add it.
    .on_drop(move |cx, _| {
        if let Some(item) = dragged() {
            cx.emit(BrowserEvent::AddToCollection(coll, item.id));
        }
        end_drag();
    })
    .tooltip(move |cx| tip(cx, "Click to show only these. Drag results here to add them."))
}

fn collections(cx: &mut Context, p: BrowserProps) {
    HStack::new(cx, |cx| {
        Label::new(cx, "Collections").class("label");
    })
    .padding_left(Pixels(tokens::SPACE_3))
    .padding_top(Pixels(tokens::SPACE_3))
    .padding_bottom(Pixels(tokens::SPACE_1))
    .height(Auto);
    Binding::new(cx, p.collections, move |cx| {
        Binding::new(cx, p.favourites, move |cx| {
            VStack::new(cx, move |cx| {
                let pal = p.theme.get().palette();
                collection_row(cx, p, Coll::Favourites, "Favourites".into(), pal.ink, p.favourites.get().len());
                for (i, c) in p.collections.get().iter().enumerate() {
                    collection_row(cx, p, Coll::User(i), c.name.clone(), crate::timeline::header::clip_color_to_rgb(c.color), c.items.len());
                }
                Button::new(cx, |cx| Label::new(cx, "+ New").class("value"))
                    .class("brw-coll")
                    .class("add")
                    .alignment(Alignment::Left)
                    .padding_left(Pixels(tokens::SPACE_2))
                    .width(Stretch(1.0))
                    .height(Pixels(26.0))
                    .on_press(|cx| cx.emit(BrowserEvent::NewCollection));
            })
            .padding_left(Pixels(tokens::SPACE_2))
            .padding_right(Pixels(tokens::SPACE_2))
            .height(Auto);
        });
    });
}

/// A result: type mark, name, ★ if a favourite, key tag, metadata, and
/// (on hover, or selected) a round preview button.
fn result_row(cx: &mut Context, p: BrowserProps, item: Item) {
    let id = item.id.clone();
    let is_fav = p.favourites.get().contains(&id);
    let draggable = item.draggable();
    let lesson_target = match item.kind {
        Kind::Instrument(i) => Some(crate::lessons::Target::SidebarInstrument(i)),
        _ => None,
    };
    let key_tag = item.source().and_then(|s| p.keys.get().get(s).copied());
    let fits = key_tag.map(|k| k.fits(p.key.get(), p.scale_mask.get()));
    let previewing = {
        let id = id.clone();
        Memo::new(move |_| p.pv_item.get().is_some_and(|i| i.id == id) && (p.pv_playing.get() || p.pv_loading.get()))
    };
    let row_item = item.clone();
    let handle = HStack::new(cx, move |cx| {
        Icon::new(cx, row_item.icon(), 12.0, Signal::new(false), p.theme, muted_or_ink);
        // Takes the free width, so a long name ends in "…" rather than
        // pushing the metadata out of the panel.
        Label::new(cx, row_item.name.clone())
            .class("body")
            .text_wrap(false)
            .text_overflow(TextOverflow::Ellipsis)
            .hoverable(false)
            .width(Stretch(1.0));
        // ★: filled for a favourite (click to un-favourite); an outline on
        // hover otherwise.
        let fav_id = row_item.id.clone();
        Button::new(cx, move |cx| Icon::new(cx, if is_fav { IconKind::Star } else { IconKind::StarOutline }, 10.0, Signal::new(false), p.theme, muted_or_ink))
            .class("brw-fav")
            .toggle_class("is-fav", is_fav)
            .alignment(Alignment::Center)
            .width(Pixels(12.0))
            .height(Pixels(12.0))
            .on_press(move |cx| cx.emit(BrowserEvent::ToggleFavourite(fav_id.clone())));
        if let (Some(k), Some(fits)) = (key_tag, fits) {
            HStack::new(cx, move |cx| {
                Label::new(cx, k.label()).class("brw-fit-text").text_wrap(false).hoverable(false);
                // Out of key: struck through.
                if !fits {
                    Element::new(cx)
                        .class("brw-strike")
                        .hoverable(false)
                        .position_type(PositionType::Absolute)
                        .top(Pixels(7.0))
                        .left(Pixels(3.0))
                        .right(Pixels(3.0))
                        .height(Pixels(1.0));
                }
            })
            .class("brw-fit")
            .toggle_class("no", !fits)
            .alignment(Alignment::Center)
            .padding_left(Pixels(4.0))
            .padding_right(Pixels(4.0))
            .width(Auto)
            .height(Pixels(15.0))
            .tooltip(move |cx| tip(cx, if fits { "In the project's key" } else { "Outside the project's key" }));
        }
        Label::new(cx, row_item.meta.clone()).class("value").text_wrap(false).hoverable(false);
        if row_item.previewable() {
            let pv_item = row_item.clone();
            Button::new(cx, move |cx| {
                ZStack::new(cx, move |cx| {
                    Icon::new(cx, IconKind::Play, 8.0, Signal::new(false), p.theme, |pal, _| pal.ink).space(Stretch(1.0)).toggle_class("hidden", previewing);
                    Icon::new(cx, IconKind::Stop, 8.0, Signal::new(true), p.theme, |pal, _| pal.on_signal).space(Stretch(1.0)).toggle_class("hidden", previewing.map(|v| !*v));
                })
                .alignment(Alignment::Center)
                .size(Stretch(1.0))
            })
            .class("brw-pv")
            .toggle_class("is-on", previewing)
            .width(Pixels(18.0))
            .height(Pixels(18.0))
            .tooltip(move |cx| tip(cx, "Preview"))
            .on_press(move |cx| cx.emit(PreviewEvent::Toggle(pv_item.clone())));
        }
        // In a collection: take it out again.
        if let Some(coll) = p.collection.get() {
            let coll_id = row_item.id.clone();
            Button::new(cx, move |cx| Icon::new(cx, IconKind::Close, 10.0, Signal::new(false), p.theme, muted_or_ink))
                .class("brw-mini")
                .alignment(Alignment::Center)
                .width(Pixels(16.0))
                .height(Pixels(16.0))
                .tooltip(move |cx| tip(cx, "Remove from this collection"))
                .on_press(move |cx| cx.emit(BrowserEvent::RemoveFromCollection(coll, coll_id.clone())));
        }
    })
    .class("brw-row")
    .toggle_class("is-sel", {
        let id = id.clone();
        p.selected.map(move |s| s.as_deref() == Some(id.as_str()))
    })
    .alignment(Alignment::Left)
    .gap(Pixels(8.0))
    .padding_left(Pixels(tokens::SPACE_2))
    .padding_right(Pixels(6.0))
    .width(Stretch(1.0))
    .height(Pixels(28.0))
    .cursor(if draggable { CursorIcon::Grab } else { CursorIcon::Hand })
    .tooltip(move |cx| tip(cx, if draggable { "Double-click to use, or drag onto a track" } else { "Double-click to open" }));
    let (sel_id, act_id) = (id.clone(), id.clone());
    let handle = handle
        .on_mouse_down(move |cx, button| {
            if button == MouseButton::Left {
                cx.emit(BrowserEvent::Select(sel_id.clone()));
            }
        })
        .on_double_click(move |cx, _| cx.emit(BrowserEvent::Activate(act_id.clone())));
    let handle = if draggable {
        handle.on_drag(move |cx| {
            let item = item.clone();
            let name = item.name.clone();
            DRAGGED.with(|d| *d.borrow_mut() = Some(item));
            let (x, y) = cx.lmouse();
            if let Some(g) = GHOST.get() {
                g.set(Some((x, y, name)));
            }
            cx.set_drop_data(cx.current());
        })
    } else {
        handle
    };
    if let Some(target) = lesson_target {
        handle.lesson_target(target);
    }
}

/// Learn: where to start, then every lesson by group, "Done" on the
/// finished ones. A single click starts one.
fn learn_list(cx: &mut Context, p: BrowserProps) {
    Binding::new(cx, p.lessons_done, move |cx| {
        let done = p.lessons_done.get();
        let lessons = crate::lessons::course::LESSONS;
        let query = p.query.get().to_lowercase();
        let lesson_row = |cx: &mut Context, i: usize, title: String, finished: bool, next: bool| {
            HStack::new(cx, move |cx| {
                Icon::new(cx, IconKind::Lesson, 12.0, Signal::new(next), p.theme, |pal, next| if next { pal.signal } else { pal.ink_muted });
                Label::new(cx, title.clone()).class("body").toggle_class("brw-next", next).text_wrap(false).text_overflow(TextOverflow::Ellipsis).width(Stretch(1.0)).hoverable(false);
                if finished {
                    Label::new(cx, "Done").class("value").hoverable(false);
                }
            })
            .class("brw-row")
            .toggle_class("is-sel", p.lessons_active.map(move |a| !next && a.is_some_and(|(l, _)| l == i)))
            .alignment(Alignment::Left)
            .gap(Pixels(8.0))
            .padding_left(Pixels(tokens::SPACE_2))
            .padding_right(Pixels(6.0))
            .width(Stretch(1.0))
            .height(Pixels(28.0))
            .cursor(CursorIcon::Hand)
            .on_press(move |cx| cx.emit(crate::project::ProjectEvent::StartLesson(i)));
        };
        if query.is_empty() {
            if let Some(next) = crate::lessons::course::next_lesson(&done) {
                let label = if done.is_empty() { "Start here" } else { "Up next" };
                lesson_row(cx, next, format!("{label}: {}", lessons[next].title), false, true);
            }
        }
        let mut group = "";
        for (i, lesson) in lessons.iter().enumerate() {
            if !query.is_empty() && !lesson.title.to_lowercase().contains(&query) && !lesson.group.to_lowercase().contains(&query) {
                continue;
            }
            if lesson.group != group {
                group = lesson.group;
                Label::new(cx, group).class("label").padding_left(Pixels(tokens::SPACE_2)).padding_top(Pixels(tokens::SPACE_3)).height(Auto);
            }
            lesson_row(cx, i, lesson.title.to_string(), done.iter().any(|d| d == lesson.id), false);
        }
    });
}

/// The docked player: what's previewing, its waveform, and Stop / Sync /
/// Loop / volume.
fn preview_dock(cx: &mut Context, p: BrowserProps) {
    VStack::new(cx, move |cx| {
        HStack::new(cx, move |cx| {
            Binding::new(cx, p.pv_item, move |cx| {
                if let Some(item) = p.pv_item.get() {
                    Icon::new(cx, item.icon(), 12.0, Signal::new(false), p.theme, muted_or_ink);
                    Label::new(cx, item.name.clone()).class("title").text_wrap(false).text_overflow(TextOverflow::Ellipsis).width(Stretch(1.0));
                }
            });
            Label::new(cx, Memo::new(move |_| if p.pv_loading.get() { "\u{2026}".to_string() } else { p.pv_info.get() })).class("value").text_wrap(false);
        })
        .alignment(Alignment::Left)
        .gap(Pixels(6.0))
        .height(Pixels(18.0));
        Waveform::new(cx, p).height(Pixels(40.0)).width(Stretch(1.0));
        HStack::new(cx, move |cx| {
            Button::new(cx, move |cx| Icon::new(cx, IconKind::Stop, 10.0, p.pv_playing, p.theme, |pal, on| if on { pal.on_signal } else { pal.ink }))
                .class("btn")
                .class("sm")
                .toggle_class("is-play", p.pv_playing)
                .tooltip(move |cx| tip(cx, "Stop"))
                .on_press(|cx| cx.emit(PreviewEvent::Stop));
            Button::new(cx, |cx| Label::new(cx, "Sync"))
                .class("btn")
                .class("sm")
                .toggle_class("is-on", p.pv_sync)
                .tooltip(move |cx| tip(cx, "Play loops at the project's tempo"))
                .on_press(|cx| cx.emit(PreviewEvent::ToggleSync));
            Button::new(cx, |cx| Label::new(cx, "Loop"))
                .class("btn")
                .class("sm")
                .toggle_class("is-on", p.pv_looping)
                .on_press(|cx| cx.emit(PreviewEvent::ToggleLoop));
            Element::new(cx).width(Stretch(1.0)).height(Pixels(1.0));
            Label::new(cx, p.pv_volume.map(|db| if *db <= -47.9 { "\u{2212}\u{221e}".to_string() } else { format!("{db:.0} dB").replace('-', "\u{2212}") })).class("value").text_wrap(false);
            VolumeBar::new(cx, p).width(Pixels(40.0)).height(Pixels(12.0));
        })
        .alignment(Alignment::Left)
        .gap(Pixels(6.0))
        .height(Auto);
    })
    .class("brw-dock")
    .toggle_class("hidden", p.pv_item.map(|i| i.is_none()))
    .gap(Pixels(tokens::SPACE_2))
    .padding(Pixels(tokens::SPACE_3))
    .padding_top(Pixels(tokens::SPACE_2))
    .height(Auto);
}

/// The previewing item's waveform: the played part in ink, the rest faint.
struct Waveform {
    p: BrowserProps,
}

impl Waveform {
    fn new(cx: &mut Context, p: BrowserProps) -> Handle<'_, Self> {
        Self { p }
            .build(cx, |_| {})
            .bind(p.pv_peaks, |mut h| h.needs_redraw())
            .bind(p.pv_progress, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
    }
}

impl View for Waveform {
    fn element(&self) -> Option<&'static str> {
        Some("brw-waveform")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        let pal = self.p.theme.get().palette();
        let mut bg = vg::Paint::default();
        bg.set_color(pal.bg_000);
        bg.set_anti_alias(true);
        canvas.draw_rrect(vg::RRect::new_rect_xy(vg::Rect::new(b.x, b.y, b.x + b.w, b.y + b.h), 3.0, 3.0), &bg);
        let peaks = self.p.pv_peaks.get();
        if peaks.is_empty() {
            return;
        }
        let played = self.p.pv_progress.get();
        let mid = b.y + b.h / 2.0;
        let col_w = b.w / peaks.len() as f32;
        let (mut done, mut rest) = (vg::Paint::default(), vg::Paint::default());
        done.set_color(pal.ink);
        rest.set_color(pal.ink_faint);
        for (i, &peak) in peaks.iter().enumerate() {
            let x = b.x + i as f32 * col_w;
            let h = (peak * (b.h / 2.0 - 2.0)).max(0.5);
            let paint = if (i as f32 + 0.5) / peaks.len() as f32 <= played { &done } else { &rest };
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, mid - h, x + col_w.max(1.0), mid + h), None), paint);
        }
        if played > 0.0 {
            let x = b.x + played * b.w;
            let mut head = vg::Paint::default();
            head.set_color(pal.ink);
            canvas.draw_path(&vg::Path::rect(vg::Rect::new(x, b.y, x + 1.0, b.y + b.h), None), &head);
        }
    }
}

/// The preview volume: a small bar, dragged or clicked.
struct VolumeBar {
    p: BrowserProps,
    dragging: bool,
}

impl VolumeBar {
    fn new(cx: &mut Context, p: BrowserProps) -> Handle<'_, Self> {
        Self { p, dragging: false }
            .build(cx, |_| {})
            .bind(p.pv_volume, |mut h| h.needs_redraw())
            .bind(p.theme, |mut h| h.needs_redraw())
            .cursor(CursorIcon::EwResize)
    }

    fn set_from_pointer(&self, cx: &mut EventContext) {
        let b = cx.lbounds();
        let f = ((cx.lmouse().0 - b.x) / b.w.max(1.0)).clamp(0.0, 1.0);
        cx.emit(PreviewEvent::SetVolume(-48.0 + 48.0 * f));
    }
}

impl View for VolumeBar {
    fn element(&self) -> Option<&'static str> {
        Some("brw-volume")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|e, _| match e {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.dragging = true;
                cx.capture();
                self.set_from_pointer(cx);
            }
            WindowEvent::MouseMove(..) if self.dragging => self.set_from_pointer(cx),
            WindowEvent::MouseUp(MouseButton::Left) => {
                self.dragging = false;
                cx.release();
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => cx.emit(PreviewEvent::SetVolume(-12.0)),
            _ => {}
        });
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let b = cx.lbounds();
        let pal = self.p.theme.get().palette();
        let y = b.y + b.h / 2.0 - 2.0;
        let mut track = vg::Paint::default();
        track.set_color(pal.bg_300);
        track.set_anti_alias(true);
        canvas.draw_rrect(vg::RRect::new_rect_xy(vg::Rect::new(b.x, y, b.x + b.w, y + 4.0), 2.0, 2.0), &track);
        let f = ((self.p.pv_volume.get() + 48.0) / 48.0).clamp(0.0, 1.0);
        let mut fill = vg::Paint::default();
        fill.set_color(pal.ink_muted);
        fill.set_anti_alias(true);
        canvas.draw_rrect(vg::RRect::new_rect_xy(vg::Rect::new(b.x, y, b.x + b.w * f, y + 4.0), 2.0, 2.0), &fill);
    }
}

/// The panel's right edge: drag to resize (200-360px), double-click for
/// the default.
struct EdgeHandle {
    width: Signal<f32>,
    dragging: Option<(f32, f32)>,
}

impl EdgeHandle {
    fn new(cx: &mut Context, width: Signal<f32>) -> Handle<'_, Self> {
        Self { width, dragging: None }.build(cx, |_| {}).class("brw-edge").cursor(CursorIcon::ColResize).width(Pixels(5.0)).height(Stretch(1.0))
    }
}

impl View for EdgeHandle {
    fn element(&self) -> Option<&'static str> {
        Some("brw-edge")
    }

    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|e, _| match e {
            WindowEvent::MouseDown(MouseButton::Left) => {
                self.dragging = Some((cx.lmouse().0, self.width.get()));
                cx.capture();
            }
            WindowEvent::MouseMove(x, _) => {
                if let Some((x0, w0)) = self.dragging {
                    let x = crate::hidpi::l(cx, *x);
                    cx.emit(BrowserEvent::SetWidth(w0 + x - x0));
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => {
                if self.dragging.take().is_some() {
                    cx.release();
                    cx.emit(BrowserEvent::CommitWidth);
                }
            }
            WindowEvent::MouseDoubleClick(MouseButton::Left) => {
                cx.emit(BrowserEvent::SetWidth(super::DEFAULT_WIDTH));
                cx.emit(BrowserEvent::CommitWidth);
            }
            _ => {}
        });
    }
}

/// While a result is being dragged: its name by the pointer. Lives over
/// the whole window without taking any clicks, and ends the drag when the
/// button comes up anywhere (after the drop target has had it).
pub struct DragGhost {
    ghost: Signal<Option<(f32, f32, String)>>,
    theme: Signal<ThemeId>,
}

impl DragGhost {
    pub fn new(cx: &mut Context, p: BrowserProps) -> Handle<'_, Self> {
        Self { ghost: p.ghost, theme: p.theme }
            .build(cx, |_| {})
            .bind(p.ghost, |mut h| h.needs_redraw())
            .hoverable(false)
            .position_type(PositionType::Absolute)
            .width(Stretch(1.0))
            .height(Stretch(1.0))
    }
}

impl View for DragGhost {
    fn element(&self) -> Option<&'static str> {
        Some("brw-ghost")
    }

    fn draw(&self, cx: &mut DrawContext, canvas: &Canvas) {
        let Some((x, y, name)) = self.ghost.get() else { return };
        let _hidpi = crate::hidpi::scale(cx, canvas);
        let pal = self.theme.get().palette();
        let font = crate::canvas_text::canvas_font(12.0);
        let (w, _) = font.measure_str(&name, None);
        let (bx, by) = (x + 12.0, y + 8.0);
        let mut bg = vg::Paint::default();
        bg.set_color(pal.bg_300);
        bg.set_anti_alias(true);
        canvas.draw_rrect(vg::RRect::new_rect_xy(vg::Rect::new(bx, by, bx + w + 16.0, by + 22.0), 3.0, 3.0), &bg);
        let mut text = vg::Paint::default();
        text.set_color(pal.ink);
        text.set_anti_alias(true);
        canvas.draw_str(&name, vg::Point::new(bx + 8.0, by + 15.0), &font, &text);
    }
}

/// Follows the pointer for the ghost, and ends drags (lives on the root,
/// where every mouse event ends up).
pub struct DragTracker;

impl Model for DragTracker {
    fn event(&mut self, cx: &mut EventContext, event: &mut Event) {
        event.map(|e, _| match e {
            WindowEvent::MouseMove(..) => {
                if let (Some(item), Some(g)) = (dragged(), GHOST.get()) {
                    let (x, y) = cx.lmouse();
                    g.set(Some((x, y, item.name)));
                }
            }
            WindowEvent::MouseUp(MouseButton::Left) => end_drag(),
            _ => {}
        });
    }
}
