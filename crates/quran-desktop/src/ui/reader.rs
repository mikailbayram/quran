//! The reader: translation (verse-by-verse) and reading (Mushaf page) modes.
//!
//! Both views are virtualised `GtkListView`s so even Al-Baqarah (286 verses,
//! 48 pages) only keeps a screenful of widgets alive. Verse data arrives
//! progressively: placeholders are shown immediately and filled per 50-verse batch.

use super::common::*;
use super::mushaf_line::{LineWord, MushafLine};
use super::verse_row::{ChapterHeader, VerseRow};
use crate::audio::Snapshot;
use crate::ctx::ctx;
use crate::runtime;
use adw::prelude::*;
use gtk::{gio, glib};
use quran_core::{Chapter, LastRead, QuranFont, ReadingMode, Settings, Verse, VerseQuery};
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::{Rc, Weak};
use std::time::Duration;

#[derive(Clone)]
enum Item {
    Header(u32),
    Verse(Rc<Verse>),
    Pending,
    Footer(u32),
}

fn boxed(item: Item) -> glib::BoxedAnyObject {
    glib::BoxedAnyObject::new(item)
}

#[derive(Default)]
struct Slot {
    header: OnceCell<Rc<ChapterHeader>>,
    row: OnceCell<Rc<VerseRow>>,
    placeholder: OnceCell<gtk::Box>,
    footer: OnceCell<Rc<Footer>>,
}

pub struct Reader {
    pub page: adw::NavigationPage,
    title: adw::WindowTitle,
    split: adw::OverlaySplitView,
    mode: adw::ToggleGroup,
    stack: gtk::Stack,
    list: gtk::ListView,
    model: gio::ListStore,
    scroller: gtk::ScrolledWindow,
    pages_list: gtk::ListView,
    pages_model: gio::ListStore,
    pages_scroller: gtk::ScrolledWindow,
    chapter: Cell<u32>,
    verses: RefCell<Vec<Option<Rc<Verse>>>>,
    token: Cell<u64>,
    pending_scroll: Cell<Option<u32>>,
    rows: RefCell<HashMap<u32, Weak<VerseRow>>>,
    page_views: RefCell<HashMap<u32, Weak<PageView>>>,
    page_cache: RefCell<HashMap<u32, Rc<Vec<Verse>>>>,
    page_inflight: RefCell<HashSet<u32>>,
    last_snap: RefCell<Snapshot>,
    scroll_debounce: Cell<Option<glib::SourceId>>,
    render_queued: Cell<bool>,
    nav: NavSidebar,
    top_verse: Cell<u32>,
    suppress_mode_signal: Cell<bool>,
}

impl Reader {
    pub fn new() -> Rc<Self> {
        let model = gio::ListStore::new::<glib::BoxedAnyObject>();
        let factory = gtk::SignalListItemFactory::new();
        // Models are attached only while visible; see `attach_when_shown`.
        let list = gtk::ListView::new(None::<gtk::NoSelection>, Some(factory.clone()));
        list.set_single_click_activate(false);
        list.add_css_class("reader-list");
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        // Clamp inside each row: wrapping the ListView in AdwClampScrollable
        // made it realise far more rows than are visible.
        scroller.set_child(Some(&list));
        scroller.set_vexpand(true);

        let pages_model = gio::ListStore::new::<glib::BoxedAnyObject>();
        let pages_factory = gtk::SignalListItemFactory::new();
        let pages_list = gtk::ListView::new(None::<gtk::NoSelection>, Some(pages_factory.clone()));
        let pages_scroller = gtk::ScrolledWindow::new();
        pages_scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        pages_scroller.set_child(Some(&pages_list));
        pages_scroller.set_vexpand(true);

        let stack = gtk::Stack::new();
        // No transition and no homogeneous sizing: otherwise the hidden mode's
        // list gets measured/allocated too and binds rows nobody sees.
        stack.set_transition_type(gtk::StackTransitionType::None);
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.add_named(&scroller, Some("translation"));
        stack.add_named(&pages_scroller, Some("reading"));

        // header bar
        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("", "");
        let title_button = gtk::Button::new();
        title_button.set_child(Some(&title));
        title_button.add_css_class("flat");
        title_button.set_tooltip_text(Some("Browse surahs and verses"));
        header.set_title_widget(Some(&title_button));
        let sidebar_btn = gtk::ToggleButton::new();
        sidebar_btn.set_icon_name("sidebar-show-symbolic");
        sidebar_btn.set_tooltip_text(Some("Surahs and verses"));
        header.pack_start(&sidebar_btn);

        let mode = adw::ToggleGroup::new();
        let t1 = adw::Toggle::new();
        t1.set_name(Some("translation"));
        t1.set_label(Some("Translation"));
        let t2 = adw::Toggle::new();
        t2.set_name(Some("reading"));
        t2.set_label(Some("Reading"));
        mode.add(t1);
        mode.add(t2);
        mode.add_css_class("round");
        header.pack_start(&mode);

        let settings_btn = icon_button("emblem-system-symbolic", "Settings (Ctrl+,)");
        settings_btn.set_action_name(Some("app.settings"));
        let search_btn = icon_button("system-search-symbolic", "Search (Ctrl+K)");
        search_btn.set_action_name(Some("app.search"));
        header.pack_end(&settings_btn);
        header.pack_end(&search_btn);

        let nav = NavSidebar::new();
        let split = adw::OverlaySplitView::new();
        split.set_sidebar(Some(&nav.root));
        split.set_content(Some(&stack));
        split.set_show_sidebar(false);
        split.set_collapsed(true);
        split.set_max_sidebar_width(380.0);
        split.set_sidebar_width_fraction(0.3);
        sidebar_btn
            .bind_property("active", &split, "show-sidebar")
            .bidirectional()
            .sync_create()
            .build();
        title_button.connect_clicked(glib::clone!(
            #[weak]
            split,
            move |_| split.set_show_sidebar(!split.shows_sidebar())
        ));

        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&header);
        tv.set_content(Some(&split));

        let page = adw::NavigationPage::new(&tv, "Quran");
        page.set_tag(Some("reader"));

        let reader = Rc::new(Self {
            page,
            title,
            split,
            mode,
            stack,
            list,
            model,
            scroller,
            pages_list,
            pages_model,
            pages_scroller,
            chapter: Cell::new(0),
            verses: RefCell::default(),
            token: Cell::new(0),
            pending_scroll: Cell::new(None),
            rows: RefCell::default(),
            page_views: RefCell::default(),
            page_cache: RefCell::default(),
            page_inflight: RefCell::default(),
            last_snap: RefCell::default(),
            scroll_debounce: Cell::new(None),
            render_queued: Cell::new(false),
            nav,
            top_verse: Cell::new(1),
            suppress_mode_signal: Cell::new(false),
        });

        reader.setup_translation_factory(&factory);
        reader.setup_pages_factory(&pages_factory);
        reader.attach_when_shown(&reader.list, &reader.scroller, &reader.model);
        reader.attach_when_shown(
            &reader.pages_list,
            &reader.pages_scroller,
            &reader.pages_model,
        );

        let w = Rc::downgrade(&reader);
        reader.mode.connect_active_name_notify(move |g| {
            let Some(r) = w.upgrade() else { return };
            if r.suppress_mode_signal.get() {
                return;
            }
            let m = if g.active_name().as_deref() == Some("reading") {
                ReadingMode::Reading
            } else {
                ReadingMode::Translation
            };
            ctx().update_settings(|s| s.reading_mode = m);
        });

        for adj in [
            reader.scroller.vadjustment(),
            reader.pages_scroller.vadjustment(),
        ] {
            let w = Rc::downgrade(&reader);
            adj.connect_value_changed(move |_| {
                if let Some(r) = w.upgrade() {
                    r.render_visible();
                    r.schedule_scroll_tracking();
                }
            });
            // viewport resized or content re-laid out
            let w = Rc::downgrade(&reader);
            adj.connect_changed(move |_| {
                if let Some(r) = w.upgrade() {
                    r.queue_render_visible();
                }
            });
        }

        let w = Rc::downgrade(&reader);
        ctx().connect_settings(move |old, new| {
            if let Some(r) = w.upgrade() {
                r.settings_changed(old, new);
            }
        });
        let w = Rc::downgrade(&reader);
        ctx().player.connect(move |snap| {
            if let Some(r) = w.upgrade() {
                r.on_player(snap);
            }
        });

        let w = Rc::downgrade(&reader);
        reader.nav.on_verse(move |v| {
            if let Some(r) = w.upgrade() {
                r.scroll_to_verse(v);
                if r.split.is_collapsed() {
                    r.split.set_show_sidebar(false);
                }
            }
        });
        reader.apply_mode(ctx().with_settings(|s| s.reading_mode));
        reader
    }

    fn setup_translation_factory(self: &Rc<Self>, factory: &gtk::SignalListItemFactory) {
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
            set_widget_data(&slot, "slot", Rc::new(Slot::default()));
            let clamp = adw::Clamp::new();
            clamp.set_maximum_size(980);
            clamp.set_tightening_threshold(700);
            clamp.set_child(Some(&slot));
            item.set_child(Some(&clamp));
            item.set_activatable(false);
            item.set_focusable(false);
        });
        let w = Rc::downgrade(self);
        factory.connect_bind(move |_, item| {
            let Some(reader) = w.upgrade() else { return };
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(slot_w) = slot_box(item) else {
                return;
            };
            let Some(slot) = widget_data::<_, Rc<Slot>>(&slot_w, "slot") else {
                return;
            };
            let Some(obj) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            let it = obj.borrow::<Item>().clone();
            for c in [
                slot.header
                    .get()
                    .map(|h| h.root.clone().upcast::<gtk::Widget>()),
                slot.row.get().map(|r| r.root.clone().upcast()),
                slot.placeholder.get().map(|p| p.clone().upcast()),
                slot.footer.get().map(|f| f.root.clone().upcast()),
            ]
            .into_iter()
            .flatten()
            {
                c.set_visible(false);
            }
            match it {
                Item::Header(ch) => {
                    let h = slot.header.get_or_init(|| {
                        let h = ChapterHeader::new(false);
                        slot_w.append(&h.root);
                        h
                    });
                    if let Some(chapter) = ctx().chapter(ch) {
                        h.bind(&chapter);
                    }
                    h.root.set_visible(true);
                }
                Item::Verse(v) => {
                    let row = slot.row.get_or_init(|| {
                        let r = VerseRow::new();
                        slot_w.append(&r.root);
                        r
                    });
                    reader
                        .rows
                        .borrow_mut()
                        .insert(v.verse_number, Rc::downgrade(row));
                    row.bind(v);
                    reader.queue_render_visible();
                    row.root.set_visible(true);
                }
                Item::Pending => {
                    let p = slot.placeholder.get_or_init(|| {
                        let p = placeholder_row();
                        slot_w.append(&p);
                        p
                    });
                    p.set_visible(true);
                }
                Item::Footer(ch) => {
                    let f = slot.footer.get_or_init(|| {
                        let f = Footer::new();
                        slot_w.append(&f.root);
                        f
                    });
                    f.bind(ch);
                    f.root.set_visible(true);
                }
            }
        });
        let w = Rc::downgrade(self);
        factory.connect_unbind(move |_, item| {
            let Some(reader) = w.upgrade() else { return };
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(slot_w) = slot_box(item) else {
                return;
            };
            let Some(slot) = widget_data::<_, Rc<Slot>>(&slot_w, "slot") else {
                return;
            };
            if let Some(row) = slot.row.get() {
                if let Some(n) = row.verse_number() {
                    let mut rows = reader.rows.borrow_mut();
                    if rows
                        .get(&n)
                        .map(|w| w.as_ptr() == Rc::as_ptr(row))
                        .unwrap_or(false)
                    {
                        rows.remove(&n);
                    }
                }
            }
        });
    }

    fn setup_pages_factory(self: &Rc<Self>, factory: &gtk::SignalListItemFactory) {
        factory.connect_setup(|_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let view = PageView::new();
            let clamp = adw::Clamp::new();
            clamp.set_maximum_size(820);
            clamp.set_child(Some(&view.root));
            set_widget_data(&clamp, "page", view.clone());
            item.set_child(Some(&clamp));
            item.set_activatable(false);
            item.set_focusable(false);
        });
        let w = Rc::downgrade(self);
        factory.connect_bind(move |_, item| {
            let Some(reader) = w.upgrade() else { return };
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(root) = item.child() else { return };
            let Some(view) = widget_data::<_, Rc<PageView>>(&root, "page") else {
                return;
            };
            let Some(obj) = item.item().and_downcast::<glib::BoxedAnyObject>() else {
                return;
            };
            let page = *obj.borrow::<u32>();
            reader
                .page_views
                .borrow_mut()
                .insert(page, Rc::downgrade(&view));
            view.bind(&reader, page);
            reader.queue_render_visible();
        });
        let w = Rc::downgrade(self);
        factory.connect_unbind(move |_, item| {
            let Some(reader) = w.upgrade() else { return };
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let Some(root) = item.child() else { return };
            let Some(view) = widget_data::<_, Rc<PageView>>(&root, "page") else {
                return;
            };
            let page = view.page.get();
            let mut map = reader.page_views.borrow_mut();
            if map
                .get(&page)
                .map(|w| w.as_ptr() == Rc::as_ptr(&view))
                .unwrap_or(false)
            {
                map.remove(&page);
            }
        });
    }

    /// Give a list its model only once its viewport has a real size, and take
    /// it away when hidden. A ListView that receives items while unallocated
    /// binds almost all of them (hundreds of verse rows / Mushaf pages).
    fn attach_when_shown(
        self: &Rc<Self>,
        list: &gtk::ListView,
        scroller: &gtk::ScrolledWindow,
        model: &gio::ListStore,
    ) {
        let w = Rc::downgrade(self);
        let (l, m) = (list.clone(), model.clone());
        scroller.connect_map(move |sc| {
            let (w, l, m) = (w.clone(), l.clone(), m.clone());
            sc.add_tick_callback(move |sc, _| {
                if sc.height() <= 0 {
                    return glib::ControlFlow::Continue;
                }
                if l.model().is_none() {
                    l.set_model(Some(&gtk::NoSelection::new(Some(m.clone()))));
                    if std::env::var_os("QURAN_NOSCROLL").is_none() {
                        if let Some(r) = w.upgrade() {
                            r.scroll_to_verse(r.top_verse.get());
                        }
                    }
                }
                glib::ControlFlow::Break
            });
        });
        let l = list.clone();
        scroller.connect_unmap(move |_| l.set_model(None::<&gtk::NoSelection>));
    }

    /// The scrolled window of the visible mode (used by the debug benchmark).
    pub fn active_scroller(&self) -> gtk::ScrolledWindow {
        if self.stack.visible_child_name().as_deref() == Some("reading") {
            self.pages_scroller.clone()
        } else {
            self.scroller.clone()
        }
    }

    pub fn chapter(&self) -> u32 {
        self.chapter.get()
    }

    /// Open a chapter, optionally scrolled to a verse.
    pub fn open(self: &Rc<Self>, chapter_id: u32, verse: Option<u32>) {
        let Some(ch) = ctx().chapter(chapter_id) else {
            return;
        };
        let same = self.chapter.get() == chapter_id && !self.verses.borrow().is_empty();
        self.title
            .set_title(&format!("{}. {}", ch.id, ch.name_simple));
        self.title.set_subtitle(&ch.translated_name.name);
        self.page.set_title(&ch.name_simple);
        self.nav.set_chapter(&ch);
        if same {
            self.scroll_to_verse(verse.unwrap_or(1));
            return;
        }
        self.chapter.set(chapter_id);
        self.load(&ch, verse);
    }

    fn load(self: &Rc<Self>, ch: &Chapter, verse: Option<u32>) {
        let token = self.token.get() + 1;
        self.token.set(token);
        let count = ch.verses_count;
        *self.verses.borrow_mut() = vec![None; count as usize];
        self.rows.borrow_mut().clear();
        // pages are filtered per surah, so a shared page must be rebuilt
        self.page_cache.borrow_mut().clear();

        // Already-downloaded text is parsed synchronously so the first frame
        // shows real verses instead of placeholders.
        let q = ctx().verse_query();
        let batches = count.div_ceil(50);
        let mut missing = Vec::new();
        {
            let _p = crate::prof::span("reader.load_cached");
            let mut verses = self.verses.borrow_mut();
            for b in 1..=batches {
                match ctx().client.cached_verses_by_chapter(ch.id, b, &q) {
                    Some(page) => {
                        for v in page.verses {
                            let i = v.verse_number.saturating_sub(1) as usize;
                            if i < verses.len() {
                                verses[i] = Some(Rc::new(v));
                            }
                        }
                    }
                    None => missing.push(b),
                }
            }
        }
        let mut items: Vec<glib::BoxedAnyObject> = Vec::with_capacity(count as usize + 2);
        items.push(boxed(Item::Header(ch.id)));
        items.extend(self.verses.borrow().iter().map(|v| {
            boxed(match v {
                Some(v) => Item::Verse(v.clone()),
                None => Item::Pending,
            })
        }));
        items.push(boxed(Item::Footer(ch.id)));
        self.model.splice(0, self.model.n_items(), &items);

        let pages: Vec<glib::BoxedAnyObject> = (ch.first_page()..=ch.last_page())
            .map(glib::BoxedAnyObject::new)
            .collect();
        self.pages_model
            .splice(0, self.pages_model.n_items(), &pages);

        // Fetch this chapter's page fonts up front so glyphs are ready when scrolled to.
        let font = ctx().with_settings(|s| s.quran_font);
        ctx().fonts.prefetch_pages(
            font,
            ch.first_page()..=ch.last_page().min(ch.first_page() + 8),
        );

        // Scroll once the target verse has real content; scrolling into
        // placeholders makes GtkListView lose its anchor.
        let target = verse.filter(|v| *v > 1);
        let target_ready = target
            .map(|v| {
                self.verses
                    .borrow()
                    .get(v as usize - 1)
                    .is_some_and(|x| x.is_some())
            })
            .unwrap_or(true);
        self.top_verse.set(verse.unwrap_or(1));
        if target_ready {
            self.pending_scroll.set(None);
            match target {
                Some(v) => self.scroll_to_verse(v),
                None => self.scroll_top(),
            }
        } else {
            self.pending_scroll.set(target);
            self.scroll_top();
        }

        let chapter = ch.id;
        // Batch that contains the requested verse first, then the rest in order.
        let first_batch = verse.map(|v| (v.saturating_sub(1)) / 50 + 1).unwrap_or(1);
        missing.sort_by_key(|b| *b != first_batch);
        for (i, batch) in missing.into_iter().enumerate() {
            let client = ctx().client.clone();
            let q = q.clone();
            let w = Rc::downgrade(self);
            // stagger slightly so the first batch wins the race for bandwidth
            let delay = if i == 0 { 0 } else { 30 + i as u64 * 10 };
            runtime::spawn(
                async move {
                    if delay > 0 {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                    client.verses_by_chapter(chapter, batch, &q).await
                },
                move |res| {
                    let Some(r) = w.upgrade() else { return };
                    if r.token.get() != token {
                        return;
                    }
                    match res {
                        Ok(page) => r.fill(page.verses),
                        Err(e) => ctx().toast_error("Couldn't load verses", &e),
                    }
                },
            );
        }
        // warm the recitation timings so pressing play is instant
        let reciter = ctx().with_settings(|s| s.reciter);
        let client = ctx().client.clone();
        runtime::spawn_bg(async move {
            let _ = client.chapter_audio(reciter, chapter).await;
        });
    }

    fn fill(self: &Rc<Self>, batch: Vec<Verse>) {
        if batch.is_empty() {
            return;
        }
        let start = batch[0].verse_number;
        let items: Vec<glib::BoxedAnyObject> = batch
            .into_iter()
            .map(|v| {
                let v = Rc::new(v);
                if let Some(slot) = self
                    .verses
                    .borrow_mut()
                    .get_mut(v.verse_number.saturating_sub(1) as usize)
                {
                    *slot = Some(v.clone());
                }
                boxed(Item::Verse(v))
            })
            .collect();
        let n = items.len() as u32;
        // index 0 is the header, so verse k lives at index k
        self.model.splice(start, n, &items);
        if let Some(target) = self.pending_scroll.get() {
            if target >= start && target < start + n {
                self.pending_scroll.set(None);
                let w = Rc::downgrade(self);
                glib::idle_add_local_once(move || {
                    if let Some(r) = w.upgrade() {
                        r.scroll_to_verse(target);
                    }
                });
            }
        }
    }

    pub fn reload(self: &Rc<Self>) {
        let Some(ch) = ctx().chapter(self.chapter.get()) else {
            return;
        };
        let v = self.top_verse.get();
        self.page_cache.borrow_mut().clear();
        self.load(&ch, Some(v));
    }

    fn scroll_top(&self) {
        if self.model.n_items() > 0 && self.list.model().is_some() {
            self.list.scroll_to(0, gtk::ListScrollFlags::NONE, None);
        }
        if self.pages_model.n_items() > 0 && self.pages_list.model().is_some() {
            self.pages_list
                .scroll_to(0, gtk::ListScrollFlags::NONE, None);
        }
        self.scroller.vadjustment().set_value(0.0);
        self.pages_scroller.vadjustment().set_value(0.0);
    }

    pub fn scroll_to_verse(&self, verse: u32) {
        self.top_verse.set(verse);
        let reading = self.stack.visible_child_name().as_deref() == Some("reading");
        let list = if reading {
            &self.pages_list
        } else {
            &self.list
        };
        if list.model().is_none() {
            return; // applied by `attach_when_shown` once the list is visible
        }
        if reading {
            if let Some(page) = self.page_of_verse(verse) {
                let first = ctx()
                    .chapter(self.chapter.get())
                    .map(|c| c.first_page())
                    .unwrap_or(1);
                let idx = page.saturating_sub(first);
                if idx < self.pages_model.n_items() {
                    self.pages_list
                        .scroll_to(idx, gtk::ListScrollFlags::NONE, Some(top_aligned()));
                }
            }
        } else if verse <= 1 {
            self.list
                .scroll_to(0, gtk::ListScrollFlags::NONE, Some(top_aligned()));
        } else if verse < self.model.n_items() {
            self.list
                .scroll_to(verse, gtk::ListScrollFlags::NONE, Some(top_aligned()));
        }
    }

    fn page_of_verse(&self, verse: u32) -> Option<u32> {
        self.verses
            .borrow()
            .get(verse.saturating_sub(1) as usize)
            .and_then(|v| v.as_ref().map(|v| v.page_number))
            .or_else(|| ctx().chapter(self.chapter.get()).map(|c| c.first_page()))
    }

    fn apply_mode(self: &Rc<Self>, mode: ReadingMode) {
        let name = match mode {
            ReadingMode::Translation => "translation",
            ReadingMode::Reading => "reading",
        };
        self.suppress_mode_signal.set(true);
        self.mode.set_active_name(Some(name));
        self.suppress_mode_signal.set(false);
        // The newly shown list gets its model (and scrolls to `top_verse`)
        // in `attach_when_shown` once it has been laid out.
        self.stack.set_visible_child_name(name);
    }

    fn settings_changed(self: &Rc<Self>, old: &Settings, new: &Settings) {
        if old.reading_mode != new.reading_mode {
            self.apply_mode(new.reading_mode);
        }
        if self.chapter.get() == 0 {
            return;
        }
        if old.translations != new.translations || old.wbw_language != new.wbw_language {
            self.reload();
            return;
        }
        let words_changed = old.quran_font != new.quran_font
            || old.mushaf_word_spacing != new.mushaf_word_spacing
            || old.wbw_inline != new.wbw_inline
            || old.wbw_translation != new.wbw_translation
            || old.wbw_transliteration != new.wbw_transliteration;
        if words_changed {
            // Mark everything stale; only rows/pages near the viewport redraw now.
            for r in self.rows.borrow().values().filter_map(Weak::upgrade) {
                r.invalidate();
            }
            for v in self.page_views.borrow().values().filter_map(Weak::upgrade) {
                v.dirty.set(true);
            }
            self.page_cache.borrow_mut().clear();
            self.render_visible();
        }
    }

    fn on_player(&self, snap: &Snapshot) {
        let prev = self.last_snap.replace(snap.clone());
        let ch = self.chapter.get();
        let here = snap.active && snap.chapter == ch;
        let was_here = prev.active && prev.chapter == ch;
        if !here && !was_here {
            return;
        }
        let rows = self.rows.borrow();
        if was_here && (prev.verse != snap.verse || !here) {
            if let Some(r) = rows.get(&prev.verse).and_then(Weak::upgrade) {
                r.set_playing(false, None, false);
            }
        }
        if here {
            if let Some(r) = rows.get(&snap.verse).and_then(Weak::upgrade) {
                r.set_playing(true, snap.word, snap.playing);
            }
        }
        drop(rows);
        let pages: Vec<_> = self
            .page_views
            .borrow()
            .values()
            .filter_map(Weak::upgrade)
            .collect();
        for p in &pages {
            p.highlight(
                was_here.then_some((prev.chapter, prev.verse, prev.word)),
                here.then_some((snap.chapter, snap.verse, snap.word)),
            );
        }
        let started = snap.playing && (!prev.playing || !was_here);
        if here
            && snap.playing
            && (prev.verse != snap.verse || started)
            && ctx().with_settings(|s| s.auto_scroll)
        {
            self.follow_verse(snap.verse);
        }
    }

    /// Keep the recited verse in view without jumping if it's already visible.
    fn follow_verse(&self, verse: u32) {
        let reading = self.stack.visible_child_name().as_deref() == Some("reading");
        if reading {
            if let Some(page) = self.page_of_verse(verse) {
                if !self.page_views.borrow().contains_key(&page) || !self.page_visible(page) {
                    self.scroll_to_verse(verse);
                }
            }
            return;
        }
        let visible = self
            .rows
            .borrow()
            .get(&verse)
            .and_then(Weak::upgrade)
            .map(|r| self.fully_visible(&r.root, &self.scroller))
            .unwrap_or(false);
        if !visible {
            self.list
                .scroll_to(verse, gtk::ListScrollFlags::NONE, Some(top_aligned()));
        }
    }

    fn page_visible(&self, page: u32) -> bool {
        self.page_views
            .borrow()
            .get(&page)
            .and_then(Weak::upgrade)
            .map(|p| {
                let Some(pt) = p
                    .root
                    .compute_point(&self.pages_scroller, &gtk::graphene::Point::new(0.0, 0.0))
                else {
                    return false;
                };
                let h = self.pages_scroller.height() as f32;
                pt.y() < h * 0.6 && pt.y() + p.root.height() as f32 > h * 0.2
            })
            .unwrap_or(false)
    }

    fn fully_visible(&self, w: &impl IsA<gtk::Widget>, viewport: &gtk::ScrolledWindow) -> bool {
        let Some(pt) = w
            .as_ref()
            .compute_point(viewport, &gtk::graphene::Point::new(0.0, 0.0))
        else {
            return false;
        };
        let h = viewport.height() as f32;
        let wh = w.as_ref().height() as f32;
        pt.y() >= 0.0 && pt.y() + wh.min(h * 0.8) <= h
    }

    fn queue_render_visible(self: &Rc<Self>) {
        if self.render_queued.replace(true) {
            return;
        }
        // Run on the next frame tick: by then the freshly bound rows have been
        // laid out and have real positions.
        let w = Rc::downgrade(self);
        self.stack.add_tick_callback(move |_, _| {
            if let Some(r) = w.upgrade() {
                r.render_queued.set(false);
                r.render_visible();
            }
            glib::ControlFlow::Break
        });
    }

    /// Build the widgets of rows within about one screen of the viewport.
    /// GtkListView binds ~200 rows at once; rendering only these keeps opening a
    /// surah or switching modes to a handful of rows' work.
    fn render_visible(self: &Rc<Self>) {
        let _p = crate::prof::span("reader.render_visible");
        let reading = self.stack.visible_child_name().as_deref() == Some("reading");
        let viewport = if reading {
            &self.pages_scroller
        } else {
            &self.scroller
        };
        let vh = viewport.height() as f32;
        if vh <= 0.0 {
            return;
        }
        let mut unplaced = false;
        let mut near = |w: &gtk::Widget| {
            if w.height() <= 0 {
                unplaced = true; // not laid out yet; look again next frame
                return false;
            }
            w.compute_point(viewport, &gtk::graphene::Point::new(0.0, 0.0))
                .map(|p| p.y() + w.height() as f32 > -vh && p.y() < vh * 2.0)
                .unwrap_or(false)
        };
        if reading {
            let views: Vec<_> = self
                .page_views
                .borrow()
                .values()
                .filter_map(Weak::upgrade)
                .filter(|v| v.dirty.get() && near(v.root.upcast_ref()))
                .collect();
            for v in views {
                v.load();
            }
        } else {
            let rows: Vec<_> = self
                .rows
                .borrow()
                .values()
                .filter_map(Weak::upgrade)
                .filter(|r| r.is_dirty() && near(r.root.upcast_ref()))
                .collect();
            for r in rows {
                r.render();
            }
        }
        if unplaced {
            self.queue_render_visible();
        }
    }

    fn schedule_scroll_tracking(self: &Rc<Self>) {
        if let Some(id) = self.scroll_debounce.take() {
            id.remove();
        }
        let w = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(Duration::from_millis(250), move || {
            if let Some(r) = w.upgrade() {
                r.scroll_debounce.set(None);
                r.track_position();
            }
        });
        self.scroll_debounce.set(Some(id));
    }

    /// Work out the verse at the top of the viewport; save it as reading progress.
    fn track_position(&self) {
        let reading = self.stack.visible_child_name().as_deref() == Some("reading");
        let mut best: Option<(f32, u32)> = None;
        if reading {
            for view in self.page_views.borrow().values() {
                let Some(v) = view.upgrade() else { continue };
                if let Some(pt) = v
                    .root
                    .compute_point(&self.pages_scroller, &gtk::graphene::Point::new(0.0, 0.0))
                {
                    let bottom = pt.y() + v.root.height() as f32;
                    let visible = v.root.is_mapped()
                        && bottom > 40.0
                        && pt.y() < self.pages_scroller.height() as f32;
                    if visible && best.map(|b| pt.y() < b.0).unwrap_or(true) {
                        if let Some(first) = v.first_verse() {
                            best = Some((pt.y(), first));
                        }
                    }
                }
            }
        } else {
            for (n, row) in self.rows.borrow().iter() {
                let Some(r) = row.upgrade() else { continue };
                if let Some(pt) = r
                    .root
                    .compute_point(&self.scroller, &gtk::graphene::Point::new(0.0, 0.0))
                {
                    let bottom = pt.y() + r.root.height() as f32;
                    let visible = r.root.is_mapped()
                        && bottom > 60.0
                        && pt.y() < self.scroller.height() as f32;
                    if visible && best.map(|b| pt.y() < b.0).unwrap_or(true) {
                        best = Some((pt.y(), *n));
                    }
                }
            }
        }
        let Some((_, verse)) = best else { return };
        self.top_verse.set(verse);
        let ch = self.chapter.get();
        ctx().client.store().set_last_read(&LastRead {
            chapter_id: ch,
            verse_number: verse,
        });
        if let Some(v) = self
            .verses
            .borrow()
            .get(verse.saturating_sub(1) as usize)
            .and_then(|v| v.clone())
        {
            if let Some(c) = ctx().chapter(ch) {
                self.title.set_subtitle(&format!(
                    "{} · Juz {} · Page {}",
                    c.translated_name.name, v.juz_number, v.page_number
                ));
            }
        }
        self.nav.select_verse(verse);
    }

    // ----- reading-mode page data -----

    fn page_verses(self: &Rc<Self>, page: u32, ready: impl FnOnce(Rc<Vec<Verse>>) + 'static) {
        if let Some(v) = self.page_cache.borrow().get(&page).cloned() {
            ready(v);
            return;
        }
        // The surah's own verses (already loaded) carry page and line numbers,
        // so a page of this surah needs no request at all.
        let from_chapter: Vec<Verse> = self
            .verses
            .borrow()
            .iter()
            .flatten()
            .filter(|v| v.page_number == page)
            .map(|v| (**v).clone())
            .collect();
        // Edge pages can hold other surahs too; those come from the page endpoint.
        let interior = ctx()
            .chapter(self.chapter.get())
            .is_some_and(|c| page > c.first_page() && page < c.last_page());
        let complete = interior
            && !from_chapter.is_empty()
            && self.verses.borrow().iter().all(|v| v.is_some());
        if complete {
            let v = Rc::new(from_chapter);
            self.page_cache.borrow_mut().insert(page, v.clone());
            ready(v);
            return;
        }
        let q = VerseQuery {
            translations: vec![],
            wbw_language: ctx().with_settings(|s| s.wbw_language.clone()),
        };
        if let Some(v) = ctx().client.cached_verses_by_page(page, &q) {
            let v = Rc::new(v);
            self.page_cache.borrow_mut().insert(page, v.clone());
            ready(v);
            return;
        }
        if !self.page_inflight.borrow_mut().insert(page) {
            return; // the view will be refreshed when the in-flight request lands
        }
        let client = ctx().client.clone();
        let w = Rc::downgrade(self);
        runtime::spawn(
            async move { client.verses_by_page(page, &q).await },
            move |res| {
                let Some(r) = w.upgrade() else { return };
                r.page_inflight.borrow_mut().remove(&page);
                match res {
                    Ok(v) => {
                        let v = Rc::new(v);
                        r.page_cache.borrow_mut().insert(page, v.clone());
                        ready(v.clone());
                        // other views bound to the page while fetching
                        if let Some(view) = r.page_views.borrow().get(&page).and_then(Weak::upgrade)
                        {
                            if view.page.get() == page && !view.rendered.get() && !view.dirty.get()
                            {
                                view.render(&r, &v);
                            }
                        }
                    }
                    Err(e) => ctx().toast_error("Couldn't load this page", &e),
                }
            },
        );
    }
}

/// Pages and lines the printed Madani Mushaf centres instead of justifying
/// (from quran.com's `pageUtils.ts`).
fn is_center_aligned(page: u32, line: u32) -> bool {
    let lines: &[u32] = match page {
        1 | 2 => return true,
        255 => &[2],
        528 => &[9],
        534 => &[6],
        545 => &[6],
        586 => &[1],
        593 => &[2],
        594 => &[5],
        600 => &[10],
        602 => &[5, 15],
        603 => &[10, 15],
        604 => &[4, 9, 14, 15],
        _ => &[],
    };
    lines.contains(&line)
}

/// Decorated surah title line of a Mushaf page.
fn surah_banner(chapter: u32) -> gtk::Widget {
    let banner = gtk::CenterBox::new();
    banner.add_css_class("mushaf-surah-banner");
    banner.set_valign(gtk::Align::Center);
    let glyph = gtk::Label::new(Some(&surah_glyph(chapter)));
    glyph.add_css_class("mushaf-surah-glyph");
    banner.set_center_widget(Some(&glyph));
    if let Some(c) = ctx().chapter(chapter) {
        let count = gtk::Label::new(Some(&format!("{} Ayahs", c.verses_count)));
        count.add_css_class("mushaf-banner-side");
        let place = gtk::Label::new(Some(if c.is_makki() { "Meccan" } else { "Medinan" }));
        place.add_css_class("mushaf-banner-side");
        banner.set_start_widget(Some(&place));
        banner.set_end_widget(Some(&count));
        banner.set_tooltip_text(Some(&format!("{}. {}", c.id, c.name_simple)));
    }
    banner.upcast()
}

/// Bismillah line for surahs that open with it (not Al-Fatihah, where it is
/// verse 1, nor At-Tawbah).
fn bismillah_line(chapter: u32) -> Option<gtk::Widget> {
    let has = ctx()
        .chapter(chapter)
        .map(|c| c.bismillah_pre && c.id != 1 && c.id != 9)
        .unwrap_or(false);
    has.then(|| {
        let pic = gtk::Picture::for_paintable(&super::verse_row::bismillah_art());
        pic.add_css_class("mushaf-bismillah");
        pic.set_content_fit(gtk::ContentFit::Contain);
        pic.set_can_shrink(true);
        pic.set_size_request(-1, 40);
        pic.upcast()
    })
}

enum Built {
    Run(Rc<MushafLine>),
    Boxed(gtk::Box),
}

fn word_tooltip(w: &quran_core::Word, verse: u32, enabled: bool) -> Option<String> {
    if w.is_end() {
        return Some(format!("Play verse {verse}"));
    }
    if !enabled {
        return None;
    }
    ctx().with_settings(|s| {
        let mut parts = vec![];
        if s.wbw_transliteration {
            parts.extend(w.transliteration_text().map(str::to_string));
        }
        if s.wbw_translation {
            parts.extend(w.translation_text().map(str::to_string));
        }
        (!parts.is_empty()).then(|| parts.join("\n"))
    })
}

fn slot_box(item: &gtk::ListItem) -> Option<gtk::Box> {
    item.child()
        .and_downcast::<adw::Clamp>()?
        .child()
        .and_downcast::<gtk::Box>()
}

fn top_aligned() -> gtk::ScrollInfo {
    let info = gtk::ScrollInfo::new();
    info.set_enable_vertical(true);
    info.set_enable_horizontal(false);
    info
}

fn placeholder_row() -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 12);
    b.add_css_class("verse-placeholder");
    b.set_margin_top(22);
    b.set_margin_bottom(22);
    for (w, align) in [
        (0.7, gtk::Align::End),
        (0.95, gtk::Align::Start),
        (0.6, gtk::Align::Start),
    ] {
        let s = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        s.add_css_class("skeleton");
        s.set_halign(align);
        s.set_size_request(
            (600.0 * w) as i32,
            if align == gtk::Align::End { 28 } else { 14 },
        );
        b.append(&s);
    }
    b
}

// ---------------------------------------------------------------------------
// Mushaf page

pub struct PageView {
    pub root: gtk::Box,
    body: gtk::Box,
    footer: gtk::Label,
    running_juz: gtk::Label,
    running_surah: gtk::Label,
    page: Cell<u32>,
    generation: Cell<u64>,
    rendered: Cell<bool>,
    /// (chapter, verse, word position) -> word, for recitation highlighting
    words: RefCell<HashMap<(u32, u32, u32), Rc<WordView>>>,
    runs: RefCell<Vec<Rc<MushafLine>>>,
    first: Cell<Option<u32>>,
    reader: RefCell<Weak<Reader>>,
    /// Bound but not yet drawn (off screen); rendered on map.
    dirty: Cell<bool>,
}

impl PageView {
    fn new() -> Rc<Self> {
        // A printed page: paper sheet, double frame, running header, 15 lines,
        // page number in an ornament.
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("mushaf-page");
        let sheet = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sheet.add_css_class("mushaf-sheet");
        sheet.set_halign(gtk::Align::Center);
        let running = gtk::CenterBox::new();
        running.add_css_class("mushaf-running");
        let running_juz = gtk::Label::new(None);
        let running_surah = gtk::Label::new(None);
        running.set_start_widget(Some(&running_juz));
        running.set_end_widget(Some(&running_surah));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_halign(gtk::Align::Center);
        body.add_css_class("mushaf-body");
        let footer = gtk::Label::new(None);
        footer.add_css_class("mushaf-page-number");
        footer.set_halign(gtk::Align::Center);
        sheet.append(&running);
        sheet.append(&body);
        sheet.append(&footer);
        root.append(&sheet);
        let view = Rc::new(Self {
            root,
            body,
            footer,
            running_juz,
            running_surah,
            page: Cell::new(0),
            generation: Cell::new(0),
            rendered: Cell::new(false),
            words: RefCell::default(),
            runs: RefCell::default(),
            first: Cell::new(None),
            reader: RefCell::new(Weak::new()),
            dirty: Cell::new(false),
        });
        view
    }

    fn first_verse(&self) -> Option<u32> {
        self.first.get()
    }

    fn bind(self: &Rc<Self>, reader: &Rc<Reader>, page: u32) {
        self.page.set(page);
        self.generation.set(self.generation.get() + 1);
        self.rendered.set(false);
        self.first.set(None);
        self.footer.set_text(&quran_core::text::arabic_digits(page));
        self.running_juz.set_text("");
        self.running_surah.set_text("");
        clear_box(&self.body);
        self.words.borrow_mut().clear();
        let spinner = adw::Spinner::new();
        spinner.set_size_request(32, 32);
        // roughly a page tall so scroll positions stay stable before rendering
        spinner.set_margin_top(420);
        spinner.set_margin_bottom(420);
        self.body.append(&spinner);
        *self.reader.borrow_mut() = Rc::downgrade(reader);
        self.dirty.set(true);
    }

    /// Fetch (cached) verses for the page and render; only for visible pages.
    fn load(self: &Rc<Self>) {
        self.dirty.set(false);
        let Some(reader) = self.reader.borrow().upgrade() else {
            return;
        };
        let page = self.page.get();
        let generation = self.generation.get();
        let w = Rc::downgrade(self);
        let rw = Rc::downgrade(&reader);
        reader.page_verses(page, move |verses| {
            let (Some(v), Some(r)) = (w.upgrade(), rw.upgrade()) else {
                return;
            };
            if v.generation.get() == generation {
                v.render(&r, &verses);
            }
        });
    }

    fn render(self: &Rc<Self>, reader: &Rc<Reader>, verses: &Rc<Vec<Verse>>) {
        let _p = crate::prof::span("page.render");
        let page = self.page.get();
        let font = ctx().with_settings(|s| s.quran_font);
        let page_font = font.page_font(page);
        if let Some(pf) = page_font.filter(|f| !ctx().fonts.is_loaded(*f)) {
            let generation = self.generation.get();
            let w = Rc::downgrade(self);
            let rw = Rc::downgrade(reader);
            let verses = verses.clone();
            ctx().fonts.ensure(pf, move || {
                let (Some(v), Some(r)) = (w.upgrade(), rw.upgrade()) else {
                    return;
                };
                if v.generation.get() == generation {
                    v.render(&r, &verses);
                }
            });
            return;
        }
        self.rendered.set(true);
        clear_box(&self.body);
        let mut words_map = HashMap::new();
        let mut built_runs: Vec<Rc<MushafLine>> = Vec::new();
        let chapter = reader.chapter.get();
        let opts = WordOptions {
            font,
            inline_translation: false,
            inline_transliteration: false,
            tooltip: ctx().with_settings(|s| s.wbw_translation || s.wbw_transliteration),
        };
        // The whole printed page, including neighbouring short surahs.
        let verses: Vec<&Verse> = verses.iter().collect();
        self.first.set(
            verses
                .iter()
                .find(|v| v.chapter_id() == chapter)
                .map(|v| v.verse_number),
        );
        if let Some(v) = verses.first() {
            self.running_juz.set_text(&format!("Juz {}", v.juz_number));
        }
        if let Some(c) = verses.last().and_then(|v| ctx().chapter(v.chapter_id())) {
            self.running_surah.set_text(&c.name_simple);
        }

        if font == QuranFont::IndoPak {
            // IndoPak Mushafs use a different page layout; flow the text.
            let mut flow: Option<adw::WrapBox> = None;
            for v in &verses {
                if v.verse_number == 1 {
                    self.body.append(&surah_banner(v.chapter_id()));
                    if let Some(b) = bismillah_line(v.chapter_id()) {
                        self.body.append(&b);
                    }
                    flow = None;
                }
                let wrap = flow.get_or_insert_with(|| {
                    let f = adw::WrapBox::new();
                    f.set_direction(gtk::TextDirection::Rtl);
                    f.set_child_spacing(6);
                    f.set_line_spacing(12);
                    f.set_justify(adw::JustifyMode::Spread);
                    f.set_justify_last_line(false);
                    f.set_size_request(640, -1);
                    self.body.append(&f);
                    f.clone()
                });
                for w in &v.words {
                    let view = WordView::new();
                    view.set(w, v.chapter_id(), v.verse_number, opts);
                    wrap.append(&view.root);
                    words_map.insert((v.chapter_id(), v.verse_number, w.position), view);
                }
            }
        } else {
            // Madani 15-line page. Lines without words are the surah banner and
            // bismillah of a surah that starts on this page.
            let mut lines: BTreeMap<u32, Vec<(&Verse, &quran_core::Word)>> = BTreeMap::new();
            let mut starts: BTreeMap<u32, u32> = BTreeMap::new(); // first line -> chapter
            for v in &verses {
                for w in &v.words {
                    lines.entry(w.line_number).or_default().push((v, w));
                }
                if v.verse_number == 1 {
                    if let Some(w) = v.words.first() {
                        starts.insert(w.line_number, v.chapter_id());
                    }
                }
            }
            let last_line = lines.keys().max().copied().unwrap_or(15).max(15);
            let mut specials: HashMap<u32, gtk::Widget> = HashMap::new();
            for (&line, &ch) in &starts {
                let has_bismillah = bismillah_line(ch).is_some();
                let mut slot = line.saturating_sub(1);
                if has_bismillah && slot >= 1 && !lines.contains_key(&slot) {
                    specials.insert(slot, bismillah_line(ch).unwrap());
                    slot = slot.saturating_sub(1);
                }
                if slot >= 1 && !lines.contains_key(&slot) {
                    specials.insert(slot, surah_banner(ch));
                } else if !specials.contains_key(&line.saturating_sub(1)) {
                    // no free line (e.g. pages 1–2): put the banner above
                    specials.insert(line.saturating_sub(1).max(0), surah_banner(ch));
                }
            }
            let first_line = lines
                .keys()
                .chain(specials.keys())
                .min()
                .copied()
                .unwrap_or(1);
            // QPC fonts draw each word as one glyph: those lines are single text
            // runs whose word gap can be set exactly (see `MushafLine`).
            let qpc = font.page_font(page).is_some();
            let mut built: Vec<(Built, i32, i32, u32)> = Vec::new();
            for line_no in first_line..=last_line {
                if let Some(w) = specials.remove(&line_no) {
                    self.body.append(&w);
                    continue;
                }
                let Some(words) = lines.get(&line_no) else {
                    continue;
                };
                let glyphs: Option<Vec<String>> = qpc
                    .then(|| {
                        words
                            .iter()
                            .map(|(_, w)| {
                                let code = if font == QuranFont::QpcV1 {
                                    &w.code_v1
                                } else {
                                    &w.code_v2
                                };
                                code.clone().filter(|c| !c.is_empty())
                            })
                            .collect()
                    })
                    .flatten();
                if let Some(glyphs) = glyphs {
                    let run_words: Vec<LineWord> = words
                        .iter()
                        .zip(glyphs)
                        .map(|((v, w), glyph)| LineWord {
                            chapter: v.chapter_id(),
                            verse: v.verse_number,
                            position: w.position,
                            glyph,
                            audio_url: w.audio_url.clone(),
                            is_end: w.is_end(),
                            tooltip: word_tooltip(w, v.verse_number, opts.tooltip),
                        })
                        .collect();
                    let family = font.page_font(page).map(|f| f.family()).unwrap_or_default();
                    let run = MushafLine::new(run_words, &font_class(&family));
                    run.label.set_halign(gtk::Align::Center);
                    self.body.append(&run.label);
                    let width = run.natural_width();
                    let n = run.len() as i32;
                    built.push((Built::Run(run), width, n, line_no));
                    continue;
                }
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                line.add_css_class("mushaf-line");
                line.set_direction(gtk::TextDirection::Rtl);
                line.set_halign(gtk::Align::Center);
                let mut width = 0;
                for (v, w) in words {
                    let view = WordView::new();
                    view.set(w, v.chapter_id(), v.verse_number, opts);
                    line.append(&view.root);
                    width += view.root.measure(gtk::Orientation::Horizontal, -1).1;
                    words_map.insert((v.chapter_id(), v.verse_number, w.position), view);
                }
                self.body.append(&line);
                built.push((Built::Boxed(line), width, words.len() as i32, line_no));
            }
            // Lay lines out the way quran.com does for the Madani Mushaf: a
            // fixed line width proportional to the font size, words spread with
            // `space-between`, except for centre-aligned pages/lines. The
            // "Word spacing" setting tightens or loosens every gap uniformly.
            let (px, spacing) = ctx().with_settings(|s| (s.quran_font_px(), s.mushaf_word_spacing));
            let (width_ratio, height_ratio) = match font {
                QuranFont::QpcV1 => (59.5 / 4.0, 5.8 / 4.0),
                QuranFont::QpcV2 => (56.0 / 3.2, 6.1 / 3.2),
                _ => (0.0, 1.9),
            };
            let offset = spacing as f64 * px * 0.02; // 0.02em per step
            let natural_max = built.iter().map(|b| b.1).max().unwrap_or(0);
            // Same line width on every page (quran.com's ratio) so the margins
            // don't shift from page to page; lines slightly wider than it are
            // tightened below. Hafs has no ratio and uses its widest line.
            let fixed = (px * width_ratio) as i32;
            let target = if fixed > 0 { fixed } else { natural_max };
            let line_height = (px * height_ratio) as i32;
            for (line, width, count, line_no) in &built {
                let centered = is_center_aligned(page, *line_no) || *count < 2;
                let justify = if centered {
                    0.0
                } else {
                    (target - width) as f64 / (count - 1) as f64
                };
                let gap = justify + offset;
                match line {
                    Built::Run(run) => {
                        // Squeeze gaps a little at most; beyond that shrink the
                        // glyphs of this one line so the page edge stays straight.
                        let min_gap = -px * 0.05;
                        if gap < min_gap && !centered {
                            let fit = target as f64 - min_gap * (count - 1) as f64;
                            let scale = (fit / *width as f64).clamp(0.85, 1.0);
                            run.set_scale(scale);
                            let scaled = *width as f64 * scale;
                            run.set_gap(((target as f64 - scaled) / (count - 1) as f64).max(min_gap));
                        } else {
                            run.set_gap(gap.max(min_gap));
                        }
                        run.label.set_size_request(-1, line_height);
                        if std::env::var_os("QURAN_LAYOUT_DEBUG").is_some() {
                            eprintln!("  run width after gap: {}", run.natural_width());
                        }
                    }
                    Built::Boxed(b) => {
                        b.set_spacing(gap.max(0.0) as i32);
                        b.set_size_request(-1, line_height);
                    }
                }
                if std::env::var_os("QURAN_LAYOUT_DEBUG").is_some() {
                    eprintln!(
                        "page {page} line {line_no} target={target} width={width} words={count} gap={gap:.1}"
                    );
                }
            }
            built_runs = built
                .iter()
                .filter_map(|b| match &b.0 {
                    Built::Run(r) => Some(r.clone()),
                    _ => None,
                })
                .collect();
            // every line slot (text, banner, bismillah) gets the same height
            self.body.set_homogeneous(true);
            if target > 0 {
                self.body.set_size_request(target, -1);
            }
        }
        *self.words.borrow_mut() = words_map;
        *self.runs.borrow_mut() = built_runs;
        let snap = ctx().player.snapshot();
        if snap.active {
            self.highlight(None, Some((snap.chapter, snap.verse, snap.word)));
        }
    }

    fn highlight(
        &self,
        old: Option<(u32, u32, Option<u32>)>,
        new: Option<(u32, u32, Option<u32>)>,
    ) {
        for run in self.runs.borrow().iter() {
            let hit = new.and_then(|(c, v, w)| w.and_then(|w| run.word_index(c, v, w)));
            run.set_active(hit);
        }
        let words = self.words.borrow();
        if let Some((c, v, Some(w))) = old {
            if let Some(view) = words.get(&(c, v, w)) {
                view.set_active(false);
            }
        }
        if let Some((c, v, Some(w))) = new {
            if let Some(view) = words.get(&(c, v, w)) {
                view.set_active(true);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Footer: previous / next surah

struct Footer {
    root: gtk::Box,
    prev: gtk::Button,
    next: gtk::Button,
    chapter: Cell<u32>,
}

impl Footer {
    fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        root.add_css_class("chapter-footer");
        root.set_halign(gtk::Align::Center);
        let prev = gtk::Button::with_label("Previous Surah");
        let top = gtk::Button::with_label("Beginning of Surah");
        let next = gtk::Button::with_label("Next Surah");
        for b in [&prev, &top, &next] {
            b.add_css_class("pill-button");
            root.append(b);
        }
        next.add_css_class("accent");
        let f = Rc::new(Self {
            root,
            prev,
            next,
            chapter: Cell::new(0),
        });
        let w = Rc::downgrade(&f);
        f.prev.connect_clicked(move |_| {
            if let Some(f) = w.upgrade() {
                crate::window::open_chapter(f.chapter.get().saturating_sub(1).max(1), None);
            }
        });
        let w = Rc::downgrade(&f);
        f.next.connect_clicked(move |_| {
            if let Some(f) = w.upgrade() {
                crate::window::open_chapter((f.chapter.get() + 1).min(114), None);
            }
        });
        top.connect_clicked(|_| ctx().ui().reader.scroll_to_verse(1));
        f
    }

    fn bind(&self, chapter: u32) {
        self.chapter.set(chapter);
        self.prev.set_visible(chapter > 1);
        self.next.set_visible(chapter < 114);
    }
}

// ---------------------------------------------------------------------------
// Sidebar: surah + verse navigation

struct NavSidebar {
    root: gtk::Box,
    surahs: gtk::ListBox,
    verses: gtk::ListBox,
    built: Cell<bool>,
}

impl NavSidebar {
    fn new() -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
        root.add_css_class("nav-sidebar");
        root.set_margin_top(8);
        let entry = gtk::SearchEntry::new();
        entry.set_placeholder_text(Some("Search surah"));
        entry.set_margin_start(8);
        entry.set_margin_end(8);
        root.append(&entry);

        let cols = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        cols.set_vexpand(true);
        let surahs = gtk::ListBox::new();
        surahs.add_css_class("nav-list");
        surahs.set_selection_mode(gtk::SelectionMode::Single);
        let s1 = gtk::ScrolledWindow::new();
        s1.set_child(Some(&surahs));
        s1.set_hexpand(true);
        s1.set_hscrollbar_policy(gtk::PolicyType::Never);
        let verses = gtk::ListBox::new();
        verses.add_css_class("nav-list");
        let s2 = gtk::ScrolledWindow::new();
        s2.set_child(Some(&verses));
        s2.set_size_request(84, -1);
        s2.set_hscrollbar_policy(gtk::PolicyType::Never);
        cols.append(&s1);
        cols.append(&gtk::Separator::new(gtk::Orientation::Vertical));
        cols.append(&s2);
        root.append(&cols);

        let filter_entry = entry.clone();
        surahs.set_filter_func(move |row| {
            let q = filter_entry.text().to_lowercase();
            if q.is_empty() {
                return true;
            }
            let name: String = row.widget_name().into();
            name.to_lowercase().contains(&q)
        });
        let s = surahs.clone();
        entry.connect_search_changed(move |_| s.invalidate_filter());
        surahs.connect_row_activated(|_, row| {
            let id = row.index() as u32 + 1;
            crate::window::open_chapter(id, None);
        });

        Self {
            root,
            surahs,
            verses,
            built: Cell::new(false),
        }
    }

    fn on_verse(&self, f: impl Fn(u32) + 'static) {
        self.verses.connect_row_activated(move |_, row| {
            f(row.index() as u32 + 1);
        });
    }

    fn set_chapter(&self, ch: &Chapter) {
        if !self.built.replace(true) {
            for c in ctx().chapters().iter() {
                let row = gtk::ListBoxRow::new();
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                b.add_css_class("nav-row");
                let n = label(&c.id.to_string(), &["nav-row-number"]);
                let name = label(&c.name_simple, &[]);
                name.set_ellipsize(pango::EllipsizeMode::End);
                b.append(&n);
                b.append(&name);
                row.set_child(Some(&b));
                row.set_widget_name(&format!(
                    "{} {} {} {}",
                    c.id, c.name_simple, c.name_complex, c.translated_name.name
                ));
                self.surahs.append(&row);
            }
        }
        if let Some(row) = self.surahs.row_at_index(ch.id as i32 - 1) {
            self.surahs.select_row(Some(&row));
        }
        self.verses.remove_all();
        for n in 1..=ch.verses_count {
            let l = label(&n.to_string(), &["verse-number-row"]);
            l.set_xalign(0.5);
            self.verses.append(&l);
        }
    }

    fn select_verse(&self, verse: u32) {
        if let Some(row) = self.verses.row_at_index(verse as i32 - 1) {
            self.verses.select_row(Some(&row));
        }
    }
}
