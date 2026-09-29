//! Settings drawer (right side), like quran.com's: theme, Quran font & size,
//! translations, word-by-word, audio and tafsir.

use crate::ctx::ctx;
use adw::prelude::*;
use quran_core::{QuranFont, RepeatMode, Settings, Theme};
use std::cell::Cell;
use std::rc::Rc;

pub struct SettingsPanel {
    pub root: gtk::Box,
    theme: adw::ToggleGroup,
    script: adw::ToggleGroup,
    style_row: adw::ActionRow,
    style: adw::ToggleGroup,
    quran_size: adw::SpinRow,
    word_spacing: adw::SpinRow,
    tr_size: adw::SpinRow,
    translations_row: adw::ActionRow,
    wbw_translation: adw::SwitchRow,
    wbw_transliteration: adw::SwitchRow,
    wbw_inline: adw::SwitchRow,
    wbw_language: adw::ComboRow,
    reciter_row: adw::ActionRow,
    repeat: adw::ComboRow,
    auto_scroll: adw::SwitchRow,
    word_highlight: adw::SwitchRow,
    tafsir: adw::ComboRow,
    tafsir_ids: std::cell::RefCell<Vec<u32>>,
    cache_row: adw::ActionRow,
    syncing: Cell<bool>,
}

/// Word-by-word languages offered by quran.com.
const WBW_LANGUAGES: &[(&str, &str)] = &[
    ("en", "English"),
    ("ur", "Urdu"),
    ("id", "Indonesian"),
    ("bn", "Bengali"),
    ("tr", "Turkish"),
    ("fa", "Persian"),
    ("hi", "Hindi"),
    ("ta", "Tamil"),
    ("inh", "Ingush"),
];

fn toggle_group(items: &[(&str, &str)]) -> adw::ToggleGroup {
    let g = adw::ToggleGroup::new();
    for (name, label) in items {
        let t = adw::Toggle::new();
        t.set_name(Some(name));
        t.set_label(Some(label));
        g.add(t);
    }
    g.set_valign(gtk::Align::Center);
    g
}

impl SettingsPanel {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.add_css_class("settings-panel");
        root.set_size_request(360, -1);

        let header = adw::HeaderBar::new();
        header.set_show_start_title_buttons(false);
        header.set_show_end_title_buttons(false);
        let title = gtk::Label::new(Some("Settings"));
        title.add_css_class("settings-title");
        header.set_title_widget(Some(&title));
        let close = gtk::Button::from_icon_name("window-close-symbolic");
        close.add_css_class("flat");
        close.set_action_name(Some("app.settings"));
        header.pack_end(&close);
        root.append(&header);

        let page = adw::PreferencesPage::new();
        page.set_vexpand(true);
        root.append(&page);

        // Theme
        let g = adw::PreferencesGroup::new();
        g.set_title("Theme");
        let theme = toggle_group(&[
            ("auto", "Auto"),
            ("light", "Light"),
            ("sepia", "Sepia"),
            ("dark", "Dark"),
        ]);
        theme.set_hexpand(true);
        let theme_row = adw::PreferencesRow::new();
        theme_row.set_activatable(false);
        theme.set_margin_top(8);
        theme.set_margin_bottom(8);
        theme.set_margin_start(8);
        theme.set_margin_end(8);
        theme_row.set_child(Some(&theme));
        g.add(&theme_row);
        page.add(&g);

        // Quran font
        let g = adw::PreferencesGroup::new();
        g.set_title("Quran Font");
        let script = toggle_group(&[("uthmani", "Uthmani"), ("indopak", "IndoPak")]);
        let script_row = adw::ActionRow::new();
        script_row.set_title("Script");
        script_row.add_suffix(&script);
        g.add(&script_row);
        let style = toggle_group(&[
            ("v1", "King Fahad V1"),
            ("v2", "King Fahad V2"),
            ("hafs", "Uthmanic Hafs"),
        ]);
        let style_row = adw::ActionRow::new();
        style_row.set_title("Style");
        style_row.add_suffix(&style);
        g.add(&style_row);
        let quran_size = adw::SpinRow::with_range(1.0, 10.0, 1.0);
        quran_size.set_title("Font size");
        g.add(&quran_size);
        let word_spacing = adw::SpinRow::with_range(-10.0, 10.0, 1.0);
        word_spacing.set_title("Word spacing");
        word_spacing.set_subtitle("Reading view: tighter or looser gaps between words");
        g.add(&word_spacing);
        page.add(&g);

        // Translation
        let g = adw::PreferencesGroup::new();
        g.set_title("Translation");
        let translations_row = adw::ActionRow::new();
        translations_row.set_title("Selected translations");
        translations_row.set_activatable(true);
        translations_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        translations_row.connect_activated(|_| super::pickers::show_translations());
        g.add(&translations_row);
        let tr_size = adw::SpinRow::with_range(1.0, 10.0, 1.0);
        tr_size.set_title("Font size");
        g.add(&tr_size);
        page.add(&g);

        // Word by word
        let g = adw::PreferencesGroup::new();
        g.set_title("Word By Word");
        let wbw_translation = adw::SwitchRow::new();
        wbw_translation.set_title("Translation");
        let wbw_transliteration = adw::SwitchRow::new();
        wbw_transliteration.set_title("Transliteration");
        let wbw_inline = adw::SwitchRow::new();
        wbw_inline.set_title("Show under each word");
        wbw_inline.set_subtitle("Otherwise shown when hovering a word");
        let langs = gtk::StringList::new(&WBW_LANGUAGES.iter().map(|l| l.1).collect::<Vec<_>>());
        let wbw_language = adw::ComboRow::new();
        wbw_language.set_title("Language");
        wbw_language.set_model(Some(&langs));
        g.add(&wbw_translation);
        g.add(&wbw_transliteration);
        g.add(&wbw_inline);
        g.add(&wbw_language);
        page.add(&g);

        // Audio
        let g = adw::PreferencesGroup::new();
        g.set_title("Audio");
        let reciter_row = adw::ActionRow::new();
        reciter_row.set_title("Reciter");
        reciter_row.set_activatable(true);
        reciter_row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        reciter_row.connect_activated(|_| super::pickers::show_reciters());
        let repeat = adw::ComboRow::new();
        repeat.set_title("Repeat");
        repeat.set_model(Some(&gtk::StringList::new(&["Off", "Verse", "Surah"])));
        let auto_scroll = adw::SwitchRow::new();
        auto_scroll.set_title("Follow the recitation");
        auto_scroll.set_subtitle("Scroll to the verse being recited");
        let word_highlight = adw::SwitchRow::new();
        word_highlight.set_title("Highlight words");
        word_highlight.set_subtitle("Highlight each word as it is recited");
        g.add(&reciter_row);
        g.add(&repeat);
        g.add(&auto_scroll);
        g.add(&word_highlight);
        page.add(&g);

        // Tafsir
        let g = adw::PreferencesGroup::new();
        g.set_title("Tafsir");
        let tafsir = adw::ComboRow::new();
        tafsir.set_title("Default tafsir");
        tafsir.set_enable_search(true);
        g.add(&tafsir);
        page.add(&g);

        // Storage
        let g = adw::PreferencesGroup::new();
        g.set_title("Offline &amp; Storage");
        let cache_row = adw::ActionRow::new();
        cache_row.set_title("Cached text");
        let clear = gtk::Button::with_label("Clear");
        clear.set_valign(gtk::Align::Center);
        cache_row.add_suffix(&clear);
        let reset_row = adw::ActionRow::new();
        reset_row.set_title("Reset settings");
        let reset = gtk::Button::with_label("Reset");
        reset.add_css_class("destructive-action");
        reset.set_valign(gtk::Align::Center);
        reset_row.add_suffix(&reset);
        g.add(&cache_row);
        g.add(&reset_row);
        page.add(&g);

        let panel = Rc::new(Self {
            root,
            theme,
            script,
            style_row,
            style,
            quran_size,
            word_spacing,
            tr_size,
            translations_row,
            wbw_translation,
            wbw_transliteration,
            wbw_inline,
            wbw_language,
            reciter_row,
            repeat,
            auto_scroll,
            word_highlight,
            tafsir,
            tafsir_ids: Default::default(),
            cache_row,
            syncing: Cell::new(false),
        });
        panel.connect_signals();
        clear.connect_clicked(glib_clone_panel(&panel, |p| {
            ctx().client.store().cache_clear();
            p.update_cache_size();
            ctx().toast("Cache cleared");
        }));
        reset.connect_clicked(|_| ctx().update_settings(|s| *s = Settings::default()));

        let w = Rc::downgrade(&panel);
        ctx().connect_settings(move |_, new| {
            if let Some(p) = w.upgrade() {
                p.sync(new);
            }
        });
        panel.sync(&ctx().settings());
        panel
    }

    fn connect_signals(self: &Rc<Self>) {
        let guard = |p: &Rc<Self>| !p.syncing.get();
        let w = Rc::downgrade(self);
        self.theme.connect_active_name_notify(move |g| {
            let Some(p) = w.upgrade() else { return };
            if !guard(&p) {
                return;
            }
            let t = match g.active_name().as_deref() {
                Some("light") => Theme::Light,
                Some("sepia") => Theme::Sepia,
                Some("dark") => Theme::Dark,
                _ => Theme::Auto,
            };
            ctx().update_settings(|s| s.theme = t);
        });
        let w = Rc::downgrade(self);
        self.script.connect_active_name_notify(move |g| {
            let Some(p) = w.upgrade() else { return };
            if !guard(&p) {
                return;
            }
            let indopak = g.active_name().as_deref() == Some("indopak");
            let style = style_font(p.style.active_name().as_deref());
            ctx().update_settings(|s| {
                s.quran_font = if indopak { QuranFont::IndoPak } else { style }
            });
        });
        let w = Rc::downgrade(self);
        self.style.connect_active_name_notify(move |g| {
            let Some(p) = w.upgrade() else { return };
            if !guard(&p) {
                return;
            }
            let style = style_font(g.active_name().as_deref());
            ctx().update_settings(|s| s.quran_font = style);
        });
        let w = Rc::downgrade(self);
        self.quran_size.connect_value_notify(move |r| {
            let Some(p) = w.upgrade() else { return };
            if guard(&p) {
                let v = r.value() as u8;
                ctx().update_settings(|s| s.quran_font_scale = v);
            }
        });
        let w = Rc::downgrade(self);
        self.word_spacing.connect_value_notify(move |r| {
            let Some(p) = w.upgrade() else { return };
            if guard(&p) {
                let v = r.value() as i8;
                ctx().update_settings(|s| s.mushaf_word_spacing = v);
            }
        });
        let w = Rc::downgrade(self);
        self.tr_size.connect_value_notify(move |r| {
            let Some(p) = w.upgrade() else { return };
            if guard(&p) {
                let v = r.value() as u8;
                ctx().update_settings(|s| s.translation_font_scale = v);
            }
        });
        macro_rules! switch {
            ($field:ident) => {{
                let w = Rc::downgrade(self);
                self.$field.connect_active_notify(move |r| {
                    let Some(p) = w.upgrade() else { return };
                    if guard(&p) {
                        let v = r.is_active();
                        ctx().update_settings(|s| s.$field = v);
                    }
                });
            }};
        }
        switch!(wbw_translation);
        switch!(wbw_transliteration);
        switch!(wbw_inline);
        switch!(auto_scroll);
        switch!(word_highlight);

        let w = Rc::downgrade(self);
        self.wbw_language.connect_selected_notify(move |r| {
            let Some(p) = w.upgrade() else { return };
            if guard(&p) {
                if let Some((code, _)) = WBW_LANGUAGES.get(r.selected() as usize) {
                    ctx().update_settings(|s| s.wbw_language = code.to_string());
                }
            }
        });
        let w = Rc::downgrade(self);
        self.repeat.connect_selected_notify(move |r| {
            let Some(p) = w.upgrade() else { return };
            if guard(&p) {
                let m = match r.selected() {
                    1 => RepeatMode::Verse,
                    2 => RepeatMode::Chapter,
                    _ => RepeatMode::Off,
                };
                ctx().update_settings(|s| s.repeat = m);
            }
        });
        let w = Rc::downgrade(self);
        self.tafsir.connect_selected_notify(move |r| {
            let Some(p) = w.upgrade() else { return };
            if guard(&p) {
                if let Some(id) = p.tafsir_ids.borrow().get(r.selected() as usize).copied() {
                    ctx().update_settings(|s| s.tafsir = id);
                }
            }
        });
    }

    /// Reflect settings in the widgets without re-triggering updates.
    pub fn sync(self: &Rc<Self>, s: &Settings) {
        self.syncing.set(true);
        self.theme.set_active_name(Some(match s.theme {
            Theme::Auto => "auto",
            Theme::Light => "light",
            Theme::Sepia => "sepia",
            Theme::Dark => "dark",
        }));
        self.script
            .set_active_name(Some(if s.quran_font == QuranFont::IndoPak {
                "indopak"
            } else {
                "uthmani"
            }));
        self.style_row
            .set_visible(s.quran_font != QuranFont::IndoPak);
        self.style.set_active_name(Some(match s.quran_font {
            QuranFont::Uthmani => "hafs",
            QuranFont::QpcV1 => "v1",
            _ => "v2",
        }));
        self.quran_size.set_value(s.quran_font_scale as f64);
        self.word_spacing.set_value(s.mushaf_word_spacing as f64);
        self.tr_size.set_value(s.translation_font_scale as f64);
        self.wbw_translation.set_active(s.wbw_translation);
        self.wbw_transliteration.set_active(s.wbw_transliteration);
        self.wbw_inline.set_active(s.wbw_inline);
        if let Some(i) = WBW_LANGUAGES.iter().position(|l| l.0 == s.wbw_language) {
            self.wbw_language.set_selected(i as u32);
        }
        self.repeat.set_selected(match s.repeat {
            RepeatMode::Off => 0,
            RepeatMode::Verse => 1,
            RepeatMode::Chapter => 2,
        });
        self.auto_scroll.set_active(s.auto_scroll);
        self.word_highlight.set_active(s.word_highlight);
        self.syncing.set(false);

        // Names that need the resource lists.
        let w = Rc::downgrade(self);
        let selected = s.translations.clone();
        ctx().with_translations(move |list| {
            let Some(p) = w.upgrade() else { return };
            let names: Vec<String> = selected
                .iter()
                .filter_map(|id| list.iter().find(|t| t.id == *id).map(|t| t.name.clone()))
                .collect();
            p.translations_row.set_subtitle(&if names.is_empty() {
                "None".to_string()
            } else {
                names.join(", ")
            });
        });
        let w = Rc::downgrade(self);
        let reciter = s.reciter;
        ctx().with_reciters(move |list| {
            let Some(p) = w.upgrade() else { return };
            if let Some(r) = list.iter().find(|r| r.id == reciter) {
                p.reciter_row
                    .set_subtitle(&format!("{} · {}", r.display_name(), r.style_name()));
            }
        });
        let w = Rc::downgrade(self);
        let tafsir = s.tafsir;
        ctx().with_tafsirs(move |list| {
            let Some(p) = w.upgrade() else { return };
            if p.tafsir_ids.borrow().is_empty() {
                let names: Vec<String> = list
                    .iter()
                    .map(|t| format!("{} ({})", t.name, capitalize(&t.language_name)))
                    .collect();
                let names: Vec<&str> = names.iter().map(String::as_str).collect();
                p.syncing.set(true);
                p.tafsir.set_model(Some(&gtk::StringList::new(&names)));
                p.syncing.set(false);
                *p.tafsir_ids.borrow_mut() = list.iter().map(|t| t.id).collect();
            }
            if let Some(i) = p.tafsir_ids.borrow().iter().position(|id| *id == tafsir) {
                p.syncing.set(true);
                p.tafsir.set_selected(i as u32);
                p.syncing.set(false);
            }
        });
        self.update_cache_size();
    }

    fn update_cache_size(&self) {
        let bytes = ctx().client.store().cache_size();
        self.cache_row.set_subtitle(&format!(
            "{:.1} MB of verses, translations and timings",
            bytes as f64 / 1e6
        ));
    }
}

fn style_font(name: Option<&str>) -> QuranFont {
    match name {
        Some("hafs") => QuranFont::Uthmani,
        Some("v1") => QuranFont::QpcV1,
        _ => QuranFont::QpcV2,
    }
}

pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

fn glib_clone_panel(
    p: &Rc<SettingsPanel>,
    f: impl Fn(&Rc<SettingsPanel>) + 'static,
) -> impl Fn(&gtk::Button) + 'static {
    let w = Rc::downgrade(p);
    move |_| {
        if let Some(p) = w.upgrade() {
            f(&p);
        }
    }
}
