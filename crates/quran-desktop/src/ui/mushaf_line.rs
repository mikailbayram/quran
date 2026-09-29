//! One printed Mushaf line drawn as a single text run.
//!
//! QPC page fonts encode every word as one glyph. Laying a line out as one
//! label (instead of a widget per word) lets the word gap be set exactly —
//! even below the glyphs' built-in side bearings — via Pango letter spacing,
//! the way the printed page is set. Hover, tooltips, click-to-play and
//! recitation highlighting are resolved from the pointer position.

use crate::ctx::ctx;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct LineWord {
    pub chapter: u32,
    pub verse: u32,
    pub position: u32,
    /// The word's glyph string (one or more page-font characters).
    pub glyph: String,
    pub audio_url: Option<String>,
    pub is_end: bool,
    pub tooltip: Option<String>,
}

pub struct MushafLine {
    pub label: gtk::Label,
    /// In reading order (right to left).
    words: Vec<LineWord>,
    /// Byte range of each word in the text (both in reading order).
    ranges: Vec<(u32, u32)>,
    gap_px: Cell<f64>,
    hover: Cell<Option<usize>>,
    active: Cell<Option<usize>>,
    tooltip_word: RefCell<Option<usize>>,
}

fn rgb(hex: &str) -> (u16, u16, u16) {
    let h = hex.trim_start_matches('#');
    let p = |i: usize| u16::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) * 257;
    if h.len() >= 6 {
        (p(0), p(2), p(4))
    } else {
        (0, 0, 0)
    }
}

impl MushafLine {
    pub fn new(words: Vec<LineWord>, font_class: &str) -> Rc<Self> {
        // QPC glyphs are Arabic presentation-form code points (bidi class AL),
        // so the text is kept in reading order and laid out right-to-left.
        let mut text = String::new();
        let mut ranges = vec![(0u32, 0u32); words.len()];
        for (i, w) in words.iter().enumerate() {
            let start = text.len() as u32;
            text.push_str(&w.glyph);
            ranges[i] = (start, text.len() as u32);
        }
        let label = gtk::Label::new(Some(&text));
        label.set_css_classes(&["arabic-word", font_class, "mushaf-run"]);
        label.set_direction(gtk::TextDirection::Rtl);
        label.set_single_line_mode(true);
        label.set_has_tooltip(true);
        label.set_cursor_from_name(Some("pointer"));
        let line = Rc::new(Self {
            label,
            words,
            ranges,
            gap_px: Cell::new(0.0),
            hover: Cell::new(None),
            active: Cell::new(None),
            tooltip_word: RefCell::new(None),
        });
        line.apply_attrs();

        let motion = gtk::EventControllerMotion::new();
        let w = Rc::downgrade(&line);
        motion.connect_motion(move |_, x, y| {
            if let Some(l) = w.upgrade() {
                let hit = l.word_at(x, y);
                if l.hover.replace(hit) != hit {
                    l.apply_attrs();
                }
            }
        });
        let w = Rc::downgrade(&line);
        motion.connect_leave(move |_| {
            if let Some(l) = w.upgrade() {
                if l.hover.replace(None).is_some() {
                    l.apply_attrs();
                }
            }
        });
        line.label.add_controller(motion);

        let click = gtk::GestureClick::new();
        let w = Rc::downgrade(&line);
        click.connect_released(move |g, _, x, y| {
            let Some(l) = w.upgrade() else { return };
            let Some(i) = l.word_at(x, y) else { return };
            g.set_state(gtk::EventSequenceState::Claimed);
            let word = &l.words[i];
            if word.is_end {
                ctx().player.play_verse_only(word.chapter, word.verse);
            } else if let Some(url) = &word.audio_url {
                ctx().player.play_word(url);
            }
        });
        line.label.add_controller(click);

        let w = Rc::downgrade(&line);
        line.label.connect_query_tooltip(move |_, x, y, _, tip| {
            let Some(l) = w.upgrade() else { return false };
            let Some(i) = l.word_at(x as f64, y as f64) else {
                return false;
            };
            *l.tooltip_word.borrow_mut() = Some(i);
            match &l.words[i].tooltip {
                Some(t) => {
                    tip.set_text(Some(t));
                    true
                }
                None => false,
            }
        });
        line
    }

    pub fn len(&self) -> usize {
        self.words.len()
    }

    /// Natural width without extra word spacing.
    pub fn natural_width(&self) -> i32 {
        self.label.measure(gtk::Orientation::Horizontal, -1).1
    }

    /// Set the gap added between neighbouring words (may be negative).
    pub fn set_gap(&self, px: f64) {
        self.gap_px.set(px);
        self.apply_attrs();
    }

    pub fn word_index(&self, chapter: u32, verse: u32, position: u32) -> Option<usize> {
        self.words
            .iter()
            .position(|w| w.chapter == chapter && w.verse == verse && w.position == position)
    }

    pub fn set_active(&self, word: Option<usize>) {
        if self.active.replace(word) != word {
            self.apply_attrs();
        }
    }

    fn word_at(&self, x: f64, y: f64) -> Option<usize> {
        let layout = self.label.layout();
        let (ox, oy) = self.label.layout_offsets();
        let px = ((x - ox as f64) * pango::SCALE as f64) as i32;
        let py = ((y - oy as f64) * pango::SCALE as f64) as i32;
        let (inside, byte, _) = layout.xy_to_index(px, py);
        if !inside {
            return None;
        }
        let byte = byte as u32;
        self.ranges
            .iter()
            .position(|(s, e)| byte >= *s && byte < *e)
    }

    fn apply_attrs(&self) {
        let attrs = crate::ui::common::quran_glyph_attrs();
        let text = self.label.text();
        let n = self.words.len();
        // The gap goes after the final character of every word except the
        // line's last word, so spacing never falls inside a word.
        let gap = (self.gap_px.get() * pango::SCALE as f64) as i32;
        if gap != 0 && n > 1 {
            for (i, (s, e)) in self.ranges.iter().enumerate() {
                if i + 1 == n {
                    continue; // last word of the line (leftmost)
                }
                let last = text[*s as usize..*e as usize]
                    .char_indices()
                    .last()
                    .map(|(b, _)| *s + b as u32)
                    .unwrap_or(*s);
                let mut a = pango::AttrInt::new_letter_spacing(gap);
                a.set_start_index(last);
                a.set_end_index(*e);
                attrs.insert(a);
            }
        }
        let (accent, faint) = ctx().with_settings(|s| crate::theme::accent_colors(s));
        let range = |i: usize| self.ranges[i];
        for (i, bg) in [(self.hover.get(), false), (self.active.get(), true)] {
            let Some(i) = i.filter(|i| *i < n) else {
                continue;
            };
            let (s, e) = range(i);
            let (r, g, b) = rgb(accent);
            let mut fg = pango::AttrColor::new_foreground(r, g, b);
            fg.set_start_index(s);
            fg.set_end_index(e);
            attrs.insert(fg);
            if bg {
                let (r, g, b) = rgb(faint);
                let mut back = pango::AttrColor::new_background(r, g, b);
                back.set_start_index(s);
                back.set_end_index(e);
                attrs.insert(back);
            }
        }
        self.label.set_attributes(Some(&attrs));
    }
}
