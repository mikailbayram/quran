//! Shared building blocks: Quran word widgets, icon buttons, small helpers.

use crate::ctx::ctx;
use gtk::prelude::*;
use quran_core::{FontFile, QuranFont, Word};
use std::cell::RefCell;
use std::rc::Rc;

pub fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.set_tooltip_text(Some(tooltip));
    b.add_css_class("flat");
    b.set_valign(gtk::Align::Center);
    b
}

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    for c in classes {
        l.add_css_class(c);
    }
    l.set_xalign(0.0);
    l
}

/// QPC page fonts declare no languages, so fontconfig rejects them when the
/// text is tagged with the UI locale (e.g. en-us). Tag Quran glyphs as undetermined.
pub fn quran_glyph_attrs() -> pango::AttrList {
    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrLanguage::new(&pango::Language::from_string(
        "und",
    )));
    attrs
}

/// CSS class selecting a Quran font. GTK applies CSS `font-family` on top of
/// Pango attributes, so fonts must be chosen through classes (see theme.rs).
pub fn font_class(family: &str) -> String {
    if let Some(page) = family.strip_prefix("QCF_P") {
        format!("qf-v1-p{}", page.trim_start_matches('0'))
    } else if let Some(page) = family.strip_prefix("QCF2") {
        format!("qf-p{}", page.trim_start_matches('0'))
    } else if family == FontFile::IndoPak.family() {
        "qf-indopak".into()
    } else {
        "qf-hafs".into()
    }
}

pub fn is_rtl_language(lang: &str) -> bool {
    matches!(
        lang.to_ascii_lowercase().as_str(),
        "arabic"
            | "urdu"
            | "persian"
            | "farsi"
            | "hebrew"
            | "dhivehi"
            | "divehi"
            | "pashto"
            | "kurdish"
            | "sindhi"
            | "uyghur"
            | "kashmiri"
    )
}

/// What to draw for a Quran word given the current font, and the font file it
/// still needs (if any). Falls back to Unicode Uthmani while a page font downloads.
pub fn word_glyph(word: &Word, font: QuranFont) -> (String, String, Option<FontFile>) {
    let uthmani = || {
        (
            word.text_uthmani.clone().unwrap_or_default(),
            FontFile::UthmanicHafs.family(),
        )
    };
    match font {
        QuranFont::QpcV2 | QuranFont::QpcV1 => {
            let f = font.page_font(word.page_number).expect("page font");
            let code = if font == QuranFont::QpcV1 {
                &word.code_v1
            } else {
                &word.code_v2
            };
            match code {
                Some(code) if ctx().fonts.is_loaded(f) => (code.clone(), f.family(), None),
                Some(_) => {
                    let (t, fam) = uthmani();
                    (t, fam, Some(f))
                }
                None => {
                    let (t, fam) = uthmani();
                    (t, fam, None)
                }
            }
        }
        QuranFont::Uthmani => {
            let (t, fam) = uthmani();
            (t, fam, None)
        }
        QuranFont::IndoPak => (
            word.text_indopak
                .clone()
                .or_else(|| word.text_uthmani.clone())
                .unwrap_or_default(),
            FontFile::IndoPak.family(),
            None,
        ),
    }
}

#[derive(Clone)]
struct WordData {
    chapter: u32,
    verse: u32,
    audio_url: Option<String>,
    is_end: bool,
}

/// One Quran word: Arabic glyph plus optional word-by-word lines.
pub struct WordView {
    pub root: gtk::Box,
    arabic: gtk::Label,
    /// Word-by-word lines, created only when shown inline (most words never need them).
    translit: std::cell::OnceCell<gtk::Label>,
    trans: std::cell::OnceCell<gtk::Label>,
    font_class: RefCell<String>,
    data: RefCell<Option<WordData>>,
}

#[derive(Clone, Copy)]
pub struct WordOptions {
    pub font: QuranFont,
    pub inline_translation: bool,
    pub inline_transliteration: bool,
    pub tooltip: bool,
}

impl WordOptions {
    pub fn from_settings() -> Self {
        ctx().with_settings(|s| Self {
            font: s.quran_font,
            inline_translation: s.wbw_inline && s.wbw_translation,
            inline_transliteration: s.wbw_inline && s.wbw_transliteration,
            tooltip: !s.wbw_inline && (s.wbw_translation || s.wbw_transliteration),
        })
    }
}

impl WordView {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 2);
        root.add_css_class("word");
        root.set_cursor_from_name(Some("pointer"));
        let arabic = gtk::Label::new(None);
        arabic.add_css_class("arabic-word");
        arabic.set_attributes(Some(&quran_glyph_attrs()));
        root.append(&arabic);

        let view = Rc::new(Self {
            root,
            arabic,
            translit: Default::default(),
            trans: Default::default(),
            font_class: RefCell::default(),
            data: RefCell::new(None),
        });
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(&view);
        click.connect_released(move |g, _, _, _| {
            g.set_state(gtk::EventSequenceState::Claimed);
            let Some(v) = weak.upgrade() else { return };
            let Some(d) = v.data.borrow().clone() else {
                return;
            };
            if d.is_end {
                ctx().player.play_verse_only(d.chapter, d.verse);
            } else if let Some(url) = d.audio_url {
                ctx().player.play_word(&url);
            }
        });
        view.root.add_controller(click);
        view
    }

    /// Returns the font file still being fetched for this word, if any.
    pub fn set(
        &self,
        word: &Word,
        chapter: u32,
        verse: u32,
        opts: WordOptions,
    ) -> Option<FontFile> {
        let (text, family, missing) = word_glyph(word, opts.font);
        self.arabic.set_text(&text);
        let class = font_class(&family);
        if *self.font_class.borrow() != class {
            // class changes restyle the label; skip when unchanged
            self.arabic.set_css_classes(&["arabic-word", &class]);
            *self.font_class.borrow_mut() = class;
        }
        let is_end = word.is_end();
        let tr = word.translation_text().filter(|_| !is_end);
        let tl = word.transliteration_text().filter(|_| !is_end);
        self.set_wbw_line(
            &self.translit,
            true,
            opts.inline_transliteration.then_some(tl).flatten(),
        );
        self.set_wbw_line(
            &self.trans,
            false,
            opts.inline_translation.then_some(tr).flatten(),
        );
        let tip = if opts.tooltip && !is_end {
            ctx().with_settings(|s| {
                let mut parts = vec![];
                if s.wbw_transliteration {
                    if let Some(t) = tl {
                        parts.push(t.to_string());
                    }
                }
                if s.wbw_translation {
                    if let Some(t) = tr {
                        parts.push(t.to_string());
                    }
                }
                (!parts.is_empty()).then(|| parts.join("\n"))
            })
        } else if is_end {
            Some(format!("Play verse {verse}"))
        } else {
            None
        };
        self.root.set_tooltip_text(tip.as_deref());
        self.root.remove_css_class("active");
        *self.data.borrow_mut() = Some(WordData {
            chapter,
            verse,
            audio_url: word.audio_url.clone(),
            is_end,
        });
        missing
    }

    fn set_wbw_line(
        &self,
        cell: &std::cell::OnceCell<gtk::Label>,
        translit: bool,
        text: Option<&str>,
    ) {
        match text {
            Some(t) => {
                let l = cell.get_or_init(|| {
                    let l = gtk::Label::new(None);
                    l.add_css_class("wbw");
                    if translit {
                        l.add_css_class("transliteration");
                    }
                    l.set_wrap(true);
                    l.set_max_width_chars(14);
                    l.set_justify(gtk::Justification::Center);
                    l.set_direction(gtk::TextDirection::Ltr);
                    // transliteration sits above the translation
                    if translit {
                        l.insert_after(&self.root, Some(&self.arabic));
                    } else {
                        self.root.append(&l);
                    }
                    l
                });
                l.set_text(t);
                l.set_visible(true);
            }
            None => {
                if let Some(l) = cell.get() {
                    l.set_visible(false);
                }
            }
        }
    }

    pub fn set_active(&self, on: bool) {
        if on {
            self.root.add_css_class("active");
        } else {
            self.root.remove_css_class("active");
        }
    }
}

/// Attach Rust data to a widget (used for list item slot state).
pub fn set_widget_data<W: IsA<gtk::glib::Object>, T: 'static>(w: &W, key: &str, value: T) {
    unsafe { w.as_ref().set_data(key, value) }
}

pub fn widget_data<W: IsA<gtk::glib::Object>, T: Clone + 'static>(w: &W, key: &str) -> Option<T> {
    unsafe { w.as_ref().data::<T>(key).map(|p| p.as_ref().clone()) }
}

pub fn format_ms(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

pub fn clear_box(b: &gtk::Box) {
    while let Some(c) = b.first_child() {
        b.remove(&c);
    }
}

pub fn surah_glyph(id: u32) -> String {
    format!("{id:03}")
}

fn is_clickable(w: &gtk::Widget) -> bool {
    if w.is::<gtk::Button>()
        || w.is::<gtk::CheckButton>()
        || w.is::<gtk::Switch>()
        || w.is::<gtk::DropDown>()
        || w.is::<gtk::Scale>()
        || w.has_css_class("word")
    {
        return w.is_sensitive();
    }
    if let Some(row) = w.downcast_ref::<gtk::ListBoxRow>() {
        return row.is_activatable() && row.is_sensitive();
    }
    // the segments of AdwToggleGroup
    w.parent().is_some_and(|p| p.is::<adw::ToggleGroup>())
}

/// Show a hand cursor over anything clickable inside `root` (a window or popover).
/// GTK CSS has no `cursor` property, so this resolves the widget under the
/// pointer on motion and sets the cursor on the root.
pub fn install_pointer_cursor(root: &impl IsA<gtk::Widget>) {
    let root = root.as_ref().clone();
    let motion = gtk::EventControllerMotion::new();
    motion.set_propagation_phase(gtk::PropagationPhase::Capture);
    let pointing = std::rc::Rc::new(std::cell::Cell::new(false));
    let r = root.clone();
    let p = pointing.clone();
    motion.connect_motion(move |_, x, y| {
        let mut w = r.pick(x, y, gtk::PickFlags::DEFAULT);
        let mut hit = false;
        while let Some(cur) = w {
            if is_clickable(&cur) {
                hit = true;
                break;
            }
            if cur == r {
                break;
            }
            w = cur.parent();
        }
        if p.replace(hit) != hit {
            r.set_cursor_from_name(hit.then_some("pointer"));
        }
    });
    let r = root.clone();
    motion.connect_leave(move |_| {
        if pointing.replace(false) {
            r.set_cursor_from_name(None);
        }
    });
    root.add_controller(motion);
}
