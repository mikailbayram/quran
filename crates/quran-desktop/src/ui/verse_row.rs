//! Translation-view verse row, laid out like quran.com: actions on the left,
//! Arabic words (RTL, wrapping) and translations on the right.

use super::common::*;
use crate::ctx::ctx;
use gtk::prelude::*;
use quran_core::text::{HtmlOptions, html_to_pango, html_to_plain};
use quran_core::{Chapter, FontFile, Verse};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct VerseRow {
    pub root: gtk::Box,
    key: gtk::Label,
    play: gtk::Button,
    bookmark: gtk::Button,
    words_box: adw::WrapBox,
    words: RefCell<Vec<Rc<WordView>>>,
    translations: gtk::Box,
    verse: RefCell<Option<Rc<Verse>>>,
    generation: Cell<u64>,
    active_word: Cell<Option<u32>>,
    /// Bound to a verse that hasn't been drawn yet (row not on screen).
    dirty: Cell<bool>,
}

impl VerseRow {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        root.add_css_class("verse-row");

        let actions = gtk::Box::new(gtk::Orientation::Vertical, 2);
        actions.set_valign(gtk::Align::Start);
        let key = label("", &["verse-key"]);
        key.set_xalign(0.5);
        let play = icon_button("media-playback-start-symbolic", "Play verse");
        let bookmark = icon_button("bookmark-new-symbolic", "Bookmark");
        let copy = icon_button("edit-copy-symbolic", "Copy");
        let tafsir = icon_button("accessories-dictionary-symbolic", "Tafsir");
        let more = gtk::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.add_css_class("flat");
        more.set_tooltip_text(Some("More"));
        for b in [&play, &bookmark, &copy, &tafsir] {
            b.add_css_class("verse-action");
        }
        more.add_css_class("verse-action");
        actions.append(&key);
        actions.append(&play);
        actions.append(&bookmark);
        actions.append(&copy);
        actions.append(&tafsir);
        actions.append(&more);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        content.set_hexpand(true);
        let words_box = adw::WrapBox::new();
        words_box.set_direction(gtk::TextDirection::Rtl);
        words_box.set_child_spacing(6);
        words_box.set_line_spacing(14);
        words_box.set_halign(gtk::Align::Fill);
        let translations = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.append(&words_box);
        content.append(&translations);

        root.append(&actions);
        root.append(&content);

        let row = Rc::new(Self {
            root,
            key,
            play,
            bookmark,
            words_box,
            words: RefCell::default(),
            translations,
            verse: RefCell::default(),
            generation: Cell::new(0),
            active_word: Cell::new(None),
            dirty: Cell::new(false),
        });

        let w = Rc::downgrade(&row);
        row.play.connect_clicked(move |_| {
            let Some(r) = w.upgrade() else { return };
            let Some(v) = r.verse.borrow().clone() else {
                return;
            };
            let snap = ctx().player.snapshot();
            if snap.active
                && snap.playing
                && snap.chapter == v.chapter_id()
                && snap.verse == v.verse_number
            {
                ctx().player.pause();
            } else {
                ctx()
                    .player
                    .play_chapter(v.chapter_id(), Some(v.verse_number));
            }
        });
        let w = Rc::downgrade(&row);
        row.bookmark.connect_clicked(move |_| {
            let Some(r) = w.upgrade() else { return };
            let Some(v) = r.verse.borrow().clone() else {
                return;
            };
            let on = ctx().client.store().toggle_bookmark(&v.verse_key);
            r.set_bookmarked(on);
            ctx().toast(if on {
                "Bookmark added"
            } else {
                "Bookmark removed"
            });
            ctx().ui().home.refresh_bookmarks();
        });
        let w = Rc::downgrade(&row);
        copy.connect_clicked(move |_| {
            let Some(r) = w.upgrade() else { return };
            let Some(v) = r.verse.borrow().clone() else {
                return;
            };
            ctx().copy_to_clipboard(&verse_copy_text(&v));
        });
        let w = Rc::downgrade(&row);
        tafsir.connect_clicked(move |_| {
            let Some(r) = w.upgrade() else { return };
            let Some(v) = r.verse.borrow().clone() else {
                return;
            };
            super::dialogs::show_tafsir(&v.verse_key);
        });

        // "More" popover
        let pop_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let pop = gtk::Popover::new();
        pop.set_child(Some(&pop_box));
        install_pointer_cursor(&pop);
        more.set_popover(Some(&pop));
        let add = |text: &str, f: Box<dyn Fn(Rc<Verse>)>| {
            let b = gtk::Button::with_label(text);
            b.add_css_class("flat");
            if let Some(l) = b.child().and_downcast::<gtk::Label>() {
                l.set_xalign(0.0);
            }
            let w = Rc::downgrade(&row);
            let pop = pop.clone();
            b.connect_clicked(move |_| {
                pop.popdown();
                let Some(r) = w.upgrade() else { return };
                let Some(v) = r.verse.borrow().clone() else {
                    return;
                };
                f(v);
            });
            pop_box.append(&b);
        };
        add(
            "Play this verse only",
            Box::new(|v| ctx().player.play_verse_only(v.chapter_id(), v.verse_number)),
        );
        add(
            "Repeat this verse",
            Box::new(|v| {
                ctx().update_settings(|s| s.repeat = quran_core::RepeatMode::Verse);
                ctx()
                    .player
                    .play_chapter(v.chapter_id(), Some(v.verse_number));
            }),
        );
        add(
            "Copy Arabic only",
            Box::new(|v| ctx().copy_to_clipboard(v.text_uthmani.as_deref().unwrap_or(""))),
        );
        add(
            "Copy link",
            Box::new(|v| {
                let (c, n) = (v.chapter_id(), v.verse_number);
                ctx().copy_to_clipboard(&format!("https://quran.com/{c}/{n}"))
            }),
        );
        add(
            "Surah info",
            Box::new(|v| super::dialogs::show_chapter_info(v.chapter_id())),
        );
        row
    }

    fn set_bookmarked(&self, on: bool) {
        self.bookmark.set_icon_name(if on {
            "user-bookmarks-symbolic"
        } else {
            "bookmark-new-symbolic"
        });
        if on {
            self.bookmark.add_css_class("on");
        } else {
            self.bookmark.remove_css_class("on");
        }
    }

    pub fn verse_number(&self) -> Option<u32> {
        self.verse.borrow().as_ref().map(|v| v.verse_number)
    }

    /// GtkListView binds ~200 rows around the viewport (all of them mapped), so
    /// binding only records the verse. The reader calls `render` for rows that
    /// are actually near the viewport.
    pub fn bind(self: &Rc<Self>, verse: Rc<Verse>) {
        let _p = crate::prof::span("verse_row.bind");
        let same = self
            .verse
            .borrow()
            .as_ref()
            .map(|v| Rc::ptr_eq(v, &verse))
            .unwrap_or(false);
        if same {
            return; // ListView re-binds rows often while scrolling
        }
        self.generation.set(self.generation.get() + 1);
        self.key.set_text(&verse.verse_key);
        *self.verse.borrow_mut() = Some(verse);
        self.dirty.set(true);
    }

    /// Settings changed: redraw when next near the viewport.
    pub fn invalidate(&self) {
        self.dirty.set(true);
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.get()
    }

    pub fn render(self: &Rc<Self>) {
        let _p = crate::prof::span("verse_row.render");
        self.dirty.set(false);
        let Some(verse) = self.verse.borrow().clone() else {
            return;
        };
        self.set_bookmarked(ctx().client.store().is_bookmarked(&verse.verse_key));
        self.render_words();
        self.render_translations(&verse);
        self.active_word.set(None);
        let snap = ctx().player.snapshot();
        let playing_here =
            snap.active && snap.chapter == verse.chapter_id() && snap.verse == verse.verse_number;
        self.set_playing(
            playing_here,
            if playing_here { snap.word } else { None },
            snap.playing,
        );
    }

    pub fn render_words(self: &Rc<Self>) {
        let _p = crate::prof::span("verse_row.words");
        let Some(verse) = self.verse.borrow().clone() else {
            return;
        };
        let opts = WordOptions::from_settings();
        let chapter = verse.chapter_id();
        let mut words = self.words.borrow_mut();
        while words.len() < verse.words.len() {
            let w = WordView::new();
            self.words_box.append(&w.root);
            words.push(w);
        }
        let mut missing: Option<FontFile> = None;
        for (i, wv) in words.iter().enumerate() {
            match verse.words.get(i) {
                Some(word) => {
                    if let Some(f) = wv.set(word, chapter, verse.verse_number, opts) {
                        missing = Some(f);
                    }
                    wv.root.set_visible(true);
                }
                None => wv.root.set_visible(false),
            }
        }
        drop(words);
        if let Some(f) = missing {
            let generation = self.generation.get();
            let weak = Rc::downgrade(self);
            ctx().fonts.ensure(f, move || {
                if let Some(r) = weak.upgrade() {
                    if r.generation.get() == generation {
                        r.render_words();
                    }
                }
            });
        }
    }

    fn render_translations(&self, verse: &Verse) {
        let _p = crate::prof::span("verse_row.translations");
        clear_box(&self.translations);
        for t in &verse.translations {
            let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
            let text = gtk::Label::new(None);
            text.set_markup(&html_to_pango(&t.text, HtmlOptions::default()));
            text.set_wrap(true);
            text.set_wrap_mode(pango::WrapMode::WordChar);
            text.set_xalign(0.0);
            text.set_selectable(true);
            text.set_can_focus(false);
            text.add_css_class("translation-text");
            let rtl = t
                .language_name
                .as_deref()
                .map(is_rtl_language)
                .unwrap_or(false);
            if rtl {
                text.set_direction(gtk::TextDirection::Rtl);
                text.set_xalign(1.0);
            }
            let name = label(
                &format!("— {}", t.resource_name.as_deref().unwrap_or("")),
                &["translator"],
            );
            if rtl {
                name.set_xalign(1.0);
            }
            b.append(&text);
            b.append(&name);
            self.translations.append(&b);
        }
    }

    pub fn set_playing(&self, on: bool, word: Option<u32>, playing: bool) {
        if on {
            self.root.add_css_class("playing");
        } else {
            self.root.remove_css_class("playing");
        }
        self.play.set_icon_name(if on && playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        let word = if on { word } else { None };
        if self.active_word.get() != word {
            let words = self.words.borrow();
            if let Some(old) = self.active_word.get() {
                if let Some(w) = words.get(old.saturating_sub(1) as usize) {
                    w.set_active(false);
                }
            }
            if let Some(new) = word {
                if let Some(w) = words.get(new.saturating_sub(1) as usize) {
                    w.set_active(true);
                }
            }
            self.active_word.set(word);
        }
    }
}

pub fn verse_copy_text(v: &Verse) -> String {
    let mut out = String::new();
    if let Some(t) = &v.text_uthmani {
        out.push_str(t);
        out.push_str(&format!(" ({})\n\n", v.verse_key));
    }
    for t in &v.translations {
        out.push_str(&html_to_plain(&t.text));
        if let Some(n) = &t.resource_name {
            out.push_str(&format!(" — {n}"));
        }
        out.push_str("\n\n");
    }
    out.trim_end().to_string()
}

/// Big surah title shown at the top of a chapter (calligraphy + bismillah).
pub struct ChapterHeader {
    pub root: gtk::Box,
    glyph: gtk::Label,
    name: gtk::Label,
    subtitle: gtk::Label,
    bismillah: gtk::Picture,
    chapter: Cell<u32>,
}

impl ChapterHeader {
    pub fn new(compact: bool) -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.add_css_class("chapter-header");
        if compact {
            root.add_css_class("mushaf-surah-frame");
        }
        let glyph = gtk::Label::new(None);
        glyph.add_css_class("chapter-glyph");
        let name = gtk::Label::new(None);
        name.add_css_class("title-2");
        let subtitle = gtk::Label::new(None);
        subtitle.add_css_class("faded");
        let bismillah = gtk::Picture::new();
        bismillah.add_css_class("bismillah");
        bismillah.set_content_fit(gtk::ContentFit::Contain);
        bismillah.set_can_shrink(true);
        bismillah.set_halign(gtk::Align::Center);
        // artwork is 176x36
        bismillah.set_size_request(
            if compact { 300 } else { 340 },
            if compact { 61 } else { 70 },
        );
        bismillah.set_alternative_text(Some("Bismillah ir-Rahman ir-Rahim"));
        bismillah.set_paintable(Some(&bismillah_art()));

        root.append(&glyph);
        if !compact {
            root.append(&name);
            root.append(&subtitle);
        }

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::Center);
        buttons.set_margin_top(8);
        let play = gtk::Button::new();
        let play_content = adw::ButtonContent::new();
        play_content.set_icon_name("media-playback-start-symbolic");
        play_content.set_label("Play Audio");
        play.set_child(Some(&play_content));
        play.add_css_class("pill-button");
        play.add_css_class("accent");
        let info = gtk::Button::new();
        let info_content = adw::ButtonContent::new();
        info_content.set_icon_name("dialog-information-symbolic");
        info_content.set_label("Surah Info");
        info.set_child(Some(&info_content));
        info.add_css_class("pill-button");
        buttons.append(&play);
        buttons.append(&info);
        if !compact {
            root.append(&buttons);
        }
        root.append(&bismillah);

        let header = Rc::new(Self {
            root,
            glyph,
            name,
            subtitle,
            bismillah,
            chapter: Cell::new(0),
        });
        // recolour the calligraphy when the theme changes
        let w = Rc::downgrade(&header);
        ctx().connect_settings(move |old, new| {
            if old.theme != new.theme {
                if let Some(h) = w.upgrade() {
                    h.bismillah.set_paintable(Some(&bismillah_art()));
                }
            }
        });
        let w = Rc::downgrade(&header);
        play.connect_clicked(move |_| {
            if let Some(h) = w.upgrade() {
                ctx().player.play_chapter(h.chapter.get(), None);
            }
        });
        let w = Rc::downgrade(&header);
        info.connect_clicked(move |_| {
            if let Some(h) = w.upgrade() {
                super::dialogs::show_chapter_info(h.chapter.get());
            }
        });
        header
    }

    pub fn bind(&self, ch: &Chapter) {
        self.chapter.set(ch.id);
        self.glyph.set_text(&surah_glyph(ch.id));
        self.glyph.set_tooltip_text(Some(&ch.name_arabic));
        self.name
            .set_text(&format!("{}. {}", ch.id, ch.name_simple));
        self.subtitle.set_text(&format!(
            "{} · {} · {} Ayahs",
            ch.translated_name.name,
            if ch.is_makki() { "Meccan" } else { "Medinan" },
            ch.verses_count
        ));
        // Al-Fatihah's bismillah is its first verse; At-Tawbah has none.
        self.bismillah
            .set_visible(ch.bismillah_pre && ch.id != 1 && ch.id != 9);
    }
}

/// quran.com's calligraphic Bismillah, tinted with the theme's text colour.
pub fn bismillah_art() -> gtk::Svg {
    thread_local! {
        static CACHE: RefCell<Option<(&'static str, gtk::Svg)>> = const { RefCell::new(None) };
    }
    let color = crate::theme::text_color(&ctx().settings());
    CACHE.with(|c| {
        if let Some((col, svg)) = c.borrow().as_ref() {
            if *col == color {
                return svg.clone();
            }
        }
        let src = include_str!("../../resources/bismillah.svg").replace("currentColor", color);
        let svg = gtk::Svg::from_bytes(&gtk::glib::Bytes::from_owned(src.into_bytes()));
        *c.borrow_mut() = Some((color, svg.clone()));
        svg
    })
}
