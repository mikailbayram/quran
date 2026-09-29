//! Home: hero search, continue reading, bookmarks, and the Surah / Juz /
//! Revelation Order index, styled after quran.com's landing page.

use super::common::*;
use crate::ctx::ctx;
use adw::prelude::*;
use quran_core::{Chapter, Juz, parse_verse_key};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Surah,
    Juz,
    Revelation,
}

pub struct Home {
    pub page: adw::NavigationPage,
    stack: gtk::Stack,
    grid: gtk::FlowBox,
    juz_grid: gtk::FlowBox,
    continue_box: gtk::Box,
    bookmarks_box: gtk::Box,
    bookmarks_flow: gtk::FlowBox,
    filter: gtk::SearchEntry,
    tab: Cell<Tab>,
    descending: Cell<bool>,
    cards: RefCell<Vec<(gtk::FlowBoxChild, Chapter)>>,
    juz_loaded: Cell<bool>,
}

impl Home {
    pub fn new() -> Rc<Self> {
        let header = adw::HeaderBar::new();
        let logo = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let mark = gtk::Label::new(Some("۝"));
        mark.add_css_class("logo");
        mark.add_css_class("logo-mark");
        let name = gtk::Label::new(Some("Quran"));
        name.add_css_class("logo");
        logo.append(&mark);
        logo.append(&name);
        header.set_title_widget(Some(&logo));
        let settings_btn = icon_button("emblem-system-symbolic", "Settings (Ctrl+,)");
        settings_btn.set_action_name(Some("app.settings"));
        let search_btn = icon_button("system-search-symbolic", "Search (Ctrl+K)");
        search_btn.set_action_name(Some("app.search"));
        header.pack_end(&settings_btn);
        header.pack_end(&search_btn);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content.set_margin_start(20);
        content.set_margin_end(20);
        content.set_margin_bottom(40);

        // hero
        let hero = gtk::Box::new(gtk::Orientation::Vertical, 16);
        hero.add_css_class("hero");
        let title = gtk::Label::new(Some("What do you want to read?"));
        title.add_css_class("hero-title");
        let filter = gtk::SearchEntry::new();
        filter.set_placeholder_text(Some(
            "Surah name, number, 2:255, or press Enter to search the Quran",
        ));
        filter.add_css_class("hero-search");
        filter.set_hexpand(true);
        let filter_clamp = adw::Clamp::new();
        filter_clamp.set_maximum_size(640);
        filter_clamp.set_child(Some(&filter));
        hero.append(&title);
        hero.append(&filter_clamp);
        content.append(&hero);

        // continue reading
        let continue_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.append(&continue_box);

        // bookmarks
        let bookmarks_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let bm_title = label("Bookmarks", &["section-title"]);
        let bookmarks_flow = gtk::FlowBox::new();
        bookmarks_flow.set_selection_mode(gtk::SelectionMode::None);
        bookmarks_flow.set_max_children_per_line(12);
        bookmarks_flow.set_column_spacing(8);
        bookmarks_flow.set_row_spacing(8);
        bookmarks_flow.set_homogeneous(false);
        bookmarks_box.append(&bm_title);
        bookmarks_box.append(&bookmarks_flow);
        content.append(&bookmarks_box);

        // tabs
        let tabs_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        tabs_row.add_css_class("tabs-row");
        let tabs = adw::ToggleGroup::new();
        for (n, l) in [
            ("surah", "Surah"),
            ("juz", "Juz"),
            ("revelation", "Revelation Order"),
        ] {
            let t = adw::Toggle::new();
            t.set_name(Some(n));
            t.set_label(Some(l));
            tabs.add(t);
        }
        tabs.set_active_name(Some("surah"));
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        let sort = gtk::Button::with_label("Sort by: Ascending");
        sort.add_css_class("flat");
        tabs_row.append(&tabs);
        tabs_row.append(&spacer);
        tabs_row.append(&sort);
        content.append(&tabs_row);

        let grid = gtk::FlowBox::new();
        grid.set_selection_mode(gtk::SelectionMode::None);
        grid.set_homogeneous(true);
        grid.set_min_children_per_line(1);
        grid.set_max_children_per_line(3);
        grid.set_column_spacing(12);
        grid.set_row_spacing(12);
        grid.set_valign(gtk::Align::Start);

        let juz_grid = gtk::FlowBox::new();
        juz_grid.set_selection_mode(gtk::SelectionMode::None);
        juz_grid.set_homogeneous(false);
        juz_grid.set_min_children_per_line(1);
        juz_grid.set_max_children_per_line(3);
        juz_grid.set_column_spacing(12);
        juz_grid.set_row_spacing(12);
        juz_grid.set_valign(gtk::Align::Start);

        let stack = gtk::Stack::new();
        stack.add_named(&grid, Some("grid"));
        stack.add_named(&juz_grid, Some("juz"));
        let spinner = adw::Spinner::new();
        spinner.set_size_request(32, 32);
        spinner.set_margin_top(60);
        stack.add_named(&spinner, Some("loading"));
        stack.set_visible_child_name("loading");
        stack.set_vhomogeneous(false);
        stack.set_interpolate_size(false);
        content.append(&stack);

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(1120);
        clamp.set_tightening_threshold(900);
        clamp.set_child(Some(&content));
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_child(Some(&clamp));
        scroller.set_vexpand(true);

        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&header);
        tv.set_content(Some(&scroller));
        let page = adw::NavigationPage::new(&tv, "Quran");
        page.set_tag(Some("home"));

        let home = Rc::new(Self {
            page,
            stack,
            grid,
            juz_grid,
            continue_box,
            bookmarks_box,
            bookmarks_flow,
            filter,
            tab: Cell::new(Tab::Surah),
            descending: Cell::new(false),
            cards: RefCell::default(),
            juz_loaded: Cell::new(false),
        });

        let w = Rc::downgrade(&home);
        tabs.connect_active_name_notify(move |t| {
            let Some(h) = w.upgrade() else { return };
            let tab = match t.active_name().as_deref() {
                Some("juz") => Tab::Juz,
                Some("revelation") => Tab::Revelation,
                _ => Tab::Surah,
            };
            h.tab.set(tab);
            h.apply_view();
        });
        let w = Rc::downgrade(&home);
        sort.connect_clicked(move |b| {
            let Some(h) = w.upgrade() else { return };
            h.descending.set(!h.descending.get());
            b.set_label(if h.descending.get() {
                "Sort by: Descending"
            } else {
                "Sort by: Ascending"
            });
            h.apply_view();
        });
        let w = Rc::downgrade(&home);
        home.filter.connect_search_changed(move |_| {
            if let Some(h) = w.upgrade() {
                h.grid.invalidate_filter();
            }
        });
        let w = Rc::downgrade(&home);
        home.filter.connect_activate(move |e| {
            let Some(h) = w.upgrade() else { return };
            let q = e.text().trim().to_string();
            if q.is_empty() {
                return;
            }
            if let Some((c, v)) = parse_verse_key(&q).filter(|(c, _)| (1..=114).contains(c)) {
                crate::window::open_chapter(c, Some(v));
                return;
            }
            if let Ok(n) = q.parse::<u32>() {
                if (1..=114).contains(&n) {
                    crate::window::open_chapter(n, None);
                    return;
                }
            }
            // exactly one surah matches: open it, otherwise search the text
            let matches: Vec<u32> = h
                .cards
                .borrow()
                .iter()
                .filter(|(_, c)| chapter_matches(c, &q))
                .map(|(_, c)| c.id)
                .collect();
            if matches.len() == 1 {
                crate::window::open_chapter(matches[0], None);
            } else {
                super::search::show(Some(&q));
            }
        });

        let w = Rc::downgrade(&home);
        ctx().with_chapters(move |chapters| {
            if let Some(h) = w.upgrade() {
                h.build_cards(&chapters);
                h.refresh();
            }
        });
        home
    }

    fn build_cards(self: &Rc<Self>, chapters: &[Chapter]) {
        let mut cards = Vec::with_capacity(chapters.len());
        for ch in chapters {
            let child = gtk::FlowBoxChild::new();
            child.set_child(Some(&surah_card(ch)));
            child.set_focusable(false);
            self.grid.append(&child);
            cards.push((child, ch.clone()));
        }
        *self.cards.borrow_mut() = cards;
        let filter = self.filter.clone();
        let w = Rc::downgrade(self);
        self.grid.set_filter_func(move |child| {
            let q = filter.text().trim().to_lowercase();
            if q.is_empty() {
                return true;
            }
            let Some(h) = w.upgrade() else { return true };
            h.cards
                .borrow()
                .iter()
                .find(|(c, _)| c == child)
                .map(|(_, ch)| chapter_matches(ch, &q))
                .unwrap_or(true)
        });
        self.apply_view();
    }

    fn apply_view(self: &Rc<Self>) {
        if self.cards.borrow().is_empty() {
            return;
        }
        match self.tab.get() {
            Tab::Juz => {
                self.ensure_juz();
                self.stack.set_visible_child_name("juz");
            }
            tab => {
                let desc = self.descending.get();
                let key: std::collections::HashMap<gtk::FlowBoxChild, u32> = self
                    .cards
                    .borrow()
                    .iter()
                    .map(|(c, ch)| {
                        (
                            c.clone(),
                            if tab == Tab::Revelation {
                                ch.revelation_order
                            } else {
                                ch.id
                            },
                        )
                    })
                    .collect();
                self.grid.set_sort_func(move |a, b| {
                    let (ka, kb) = (
                        key.get(a).copied().unwrap_or(0),
                        key.get(b).copied().unwrap_or(0),
                    );
                    let ord = if desc { kb.cmp(&ka) } else { ka.cmp(&kb) };
                    ord.into()
                });
                self.stack.set_visible_child_name("grid");
            }
        }
    }

    fn ensure_juz(self: &Rc<Self>) {
        if self.juz_loaded.replace(true) {
            return;
        }
        let client = ctx().client.clone();
        let w = Rc::downgrade(self);
        crate::runtime::spawn(async move { client.juzs().await }, move |res| {
            let Some(h) = w.upgrade() else { return };
            match res {
                Ok(juzs) => {
                    for j in juzs {
                        h.juz_grid.append(&juz_card(&j));
                    }
                }
                Err(e) => {
                    h.juz_loaded.set(false);
                    ctx().toast_error("Couldn't load the juz list", &e);
                }
            }
        });
    }

    /// Update "continue reading" and bookmarks (called when returning home).
    pub fn refresh(&self) {
        clear_box(&self.continue_box);
        if let Some(lr) = ctx().client.store().last_read() {
            if let Some(ch) = ctx().chapter(lr.chapter_id) {
                let title = label("Continue Reading", &["section-title"]);
                let card = gtk::Button::new();
                card.add_css_class("continue-card");
                card.add_css_class("flat");
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
                text.append(&label(&ch.name_simple, &["card-title"]));
                text.append(&label(
                    &format!("Verse {} of {}", lr.verse_number, ch.verses_count),
                    &["card-subtitle"],
                ));
                text.set_hexpand(true);
                let glyph = gtk::Label::new(Some(&surah_glyph(ch.id)));
                glyph.add_css_class("surah-glyph");
                let progress = gtk::ProgressBar::new();
                progress.set_fraction(lr.verse_number as f64 / ch.verses_count.max(1) as f64);
                progress.set_valign(gtk::Align::Center);
                progress.set_size_request(120, -1);
                b.append(&text);
                b.append(&progress);
                b.append(&glyph);
                card.set_child(Some(&b));
                card.set_halign(gtk::Align::Start);
                card.set_size_request(420, -1);
                let (c, v) = (lr.chapter_id, lr.verse_number);
                card.connect_clicked(move |_| crate::window::open_chapter(c, Some(v)));
                self.continue_box.append(&title);
                self.continue_box.append(&card);
            }
        }
        self.continue_box
            .set_visible(self.continue_box.first_child().is_some());
        self.refresh_bookmarks();
    }

    pub fn refresh_bookmarks(&self) {
        self.bookmarks_flow.remove_all();
        let bookmarks = ctx().client.store().bookmarks();
        for b in &bookmarks {
            let Some((c, v)) = parse_verse_key(&b.verse_key) else {
                continue;
            };
            let name = ctx()
                .chapter(c)
                .map(|ch| ch.name_simple)
                .unwrap_or_default();
            let chip = gtk::Button::with_label(&format!("{name} {c}:{v}"));
            chip.add_css_class("chip");
            chip.add_css_class("flat");
            chip.connect_clicked(move |_| crate::window::open_chapter(c, Some(v)));
            self.bookmarks_flow.append(&chip);
        }
        self.bookmarks_box.set_visible(!bookmarks.is_empty());
    }
}

fn chapter_matches(ch: &Chapter, q: &str) -> bool {
    let q = q.to_lowercase();
    let norm = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    let nq = norm(&q);
    ch.id.to_string() == q
        || norm(&ch.name_simple).contains(&nq)
        || norm(&ch.name_complex).contains(&nq)
        || ch.translated_name.name.to_lowercase().contains(&q)
        || ch.name_arabic.contains(&q)
}

fn surah_card(ch: &Chapter) -> gtk::Button {
    let btn = gtk::Button::new();
    btn.add_css_class("surah-card");
    btn.add_css_class("flat");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);

    let diamond = gtk::Overlay::new();
    let shape = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shape.add_css_class("diamond");
    shape.set_halign(gtk::Align::Center);
    shape.set_valign(gtk::Align::Center);
    diamond.set_child(Some(&shape));
    let num = gtk::Label::new(Some(&ch.id.to_string()));
    num.add_css_class("diamond-number");
    diamond.add_overlay(&num);
    diamond.set_size_request(50, 50);
    diamond.set_valign(gtk::Align::Center);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let name = label(&ch.name_simple, &["card-title"]);
    let sub = label(&ch.translated_name.name, &["card-subtitle"]);
    name.set_ellipsize(pango::EllipsizeMode::End);
    sub.set_ellipsize(pango::EllipsizeMode::End);
    text.append(&name);
    text.append(&sub);

    let end = gtk::Box::new(gtk::Orientation::Vertical, 0);
    end.set_valign(gtk::Align::Center);
    let glyph = gtk::Label::new(Some(&surah_glyph(ch.id)));
    glyph.add_css_class("surah-glyph");
    glyph.set_xalign(1.0);
    let count = label(&format!("{} Ayahs", ch.verses_count), &["card-subtitle"]);
    count.set_xalign(1.0);
    end.append(&glyph);
    end.append(&count);

    row.append(&diamond);
    row.append(&text);
    row.append(&end);
    btn.set_child(Some(&row));
    btn.set_tooltip_text(Some(&format!(
        "{} · {} · {}",
        ch.name_arabic,
        if ch.is_makki() { "Meccan" } else { "Medinan" },
        ch.translated_name.name
    )));
    let id = ch.id;
    btn.connect_clicked(move |_| crate::window::open_chapter(id, None));
    btn
}

fn juz_card(j: &Juz) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("juz-card");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    head.append(&label(&format!("Juz {}", j.juz_number), &["juz-title"]));
    card.append(&head);
    for (ch, from, to) in j.ranges() {
        let Some(c) = ctx().chapter(ch) else { continue };
        let b = gtk::Button::new();
        b.add_css_class("flat");
        b.add_css_class("juz-row");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let n = label(&ch.to_string(), &["nav-row-number"]);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
        text.set_hexpand(true);
        text.append(&label(&c.name_simple, &["card-title"]));
        text.append(&label(&format!("Verses {from}–{to}"), &["card-subtitle"]));
        let glyph = gtk::Label::new(Some(&surah_glyph(ch)));
        glyph.add_css_class("surah-glyph");
        row.append(&n);
        row.append(&text);
        row.append(&glyph);
        b.set_child(Some(&row));
        b.connect_clicked(move |_| crate::window::open_chapter(ch, Some(from)));
        card.append(&b);
    }
    card.set_size_request(320, -1);
    card
}
