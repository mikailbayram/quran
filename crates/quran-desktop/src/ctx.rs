//! Process-wide application state, accessible from any GTK callback via `ctx()`.

use crate::audio::Player;
use crate::fonts::Fonts;
use crate::runtime;
use adw::prelude::*;
use quran_core::{
    Chapter, Client, Reciter, Settings, TafsirResource, TranslationResource, VerseQuery,
};
use std::cell::{Cell, OnceCell, RefCell};
use std::rc::Rc;

type SettingsListener = Box<dyn Fn(&Settings, &Settings)>;

pub struct Ctx {
    pub client: Client,
    pub fonts: Fonts,
    pub player: Player,
    settings: RefCell<Settings>,
    listeners: RefCell<Vec<SettingsListener>>,
    chapters: RefCell<Rc<Vec<Chapter>>>,
    chapter_waiters: RefCell<Vec<Box<dyn FnOnce(Rc<Vec<Chapter>>)>>>,
    pub translations: RefCell<Option<Rc<Vec<TranslationResource>>>>,
    pub tafsirs: RefCell<Option<Rc<Vec<TafsirResource>>>>,
    pub reciters: RefCell<Option<Rc<Vec<Reciter>>>>,
    pub window: OnceCell<adw::ApplicationWindow>,
    pub toasts: OnceCell<adw::ToastOverlay>,
    pub nav: OnceCell<adw::NavigationView>,
    pub settings_split: OnceCell<adw::OverlaySplitView>,
    pub ui: OnceCell<Rc<crate::window::Ui>>,
    save_pending: Cell<bool>,
}

thread_local! {
    static CTX: Cell<Option<&'static Ctx>> = const { Cell::new(None) };
}

pub fn ctx() -> &'static Ctx {
    CTX.with(|c| c.get()).expect("ctx initialised")
}

pub fn init(client: Client) -> &'static Ctx {
    let settings = client.store().settings();
    let ctx: &'static Ctx = Box::leak(Box::new(Ctx {
        fonts: Fonts::new(client.clone()),
        player: Player::new(),
        client,
        settings: RefCell::new(settings),
        listeners: RefCell::default(),
        chapters: RefCell::new(Rc::new(Vec::new())),
        chapter_waiters: RefCell::default(),
        translations: RefCell::default(),
        tafsirs: RefCell::default(),
        reciters: RefCell::default(),
        window: OnceCell::new(),
        toasts: OnceCell::new(),
        nav: OnceCell::new(),
        settings_split: OnceCell::new(),
        ui: OnceCell::new(),
        save_pending: Cell::new(false),
    }));
    CTX.with(|c| c.set(Some(ctx)));
    ctx
}

impl Ctx {
    pub fn settings(&self) -> Settings {
        self.settings.borrow().clone()
    }

    pub fn with_settings<R>(&self, f: impl FnOnce(&Settings) -> R) -> R {
        f(&self.settings.borrow())
    }

    /// Mutate settings, persist (debounced) and notify listeners with (old, new).
    pub fn update_settings(&'static self, f: impl FnOnce(&mut Settings)) {
        let old = self.settings();
        let mut new = old.clone();
        f(&mut new);
        if old == new {
            return;
        }
        *self.settings.borrow_mut() = new.clone();
        // Reloading the stylesheet restyles every widget; only do it when needed.
        if old.theme != new.theme
            || old.quran_font_scale != new.quran_font_scale
            || old.translation_font_scale != new.translation_font_scale
        {
            crate::theme::apply(&new);
        }
        for l in self.listeners.borrow().iter() {
            l(&old, &new);
        }
        if !self.save_pending.replace(true) {
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
                self.save_pending.set(false);
                self.client.store().save_settings(&self.settings());
            });
        }
    }

    pub fn connect_settings(&self, f: impl Fn(&Settings, &Settings) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn verse_query(&self) -> VerseQuery {
        self.with_settings(|s| VerseQuery {
            translations: s.translations.clone(),
            wbw_language: s.wbw_language.clone(),
        })
    }

    pub fn chapters(&self) -> Rc<Vec<Chapter>> {
        self.chapters.borrow().clone()
    }

    pub fn chapter(&self, id: u32) -> Option<Chapter> {
        self.chapters
            .borrow()
            .get(id.saturating_sub(1) as usize)
            .cloned()
    }

    /// Call `f` once the chapter list is available.
    pub fn with_chapters(&'static self, f: impl FnOnce(Rc<Vec<Chapter>>) + 'static) {
        let chapters = self.chapters();
        if !chapters.is_empty() {
            f(chapters);
            return;
        }
        let first = {
            let mut w = self.chapter_waiters.borrow_mut();
            w.push(Box::new(f));
            w.len() == 1
        };
        if first {
            self.load_chapters();
        }
    }

    fn load_chapters(&'static self) {
        let client = self.client.clone();
        runtime::spawn(async move { client.chapters("en").await }, move |res| {
            match res {
                Ok(list) => {
                    let list = Rc::new(list);
                    *self.chapters.borrow_mut() = list.clone();
                    let waiters: Vec<_> = self.chapter_waiters.borrow_mut().drain(..).collect();
                    for w in waiters {
                        w(list.clone());
                    }
                }
                Err(e) => {
                    self.toast_error("Couldn't load the list of surahs", &e);
                    // try again in a few seconds so the app recovers once online
                    gtk::glib::timeout_add_local_once(
                        std::time::Duration::from_secs(5),
                        move || {
                            if !self.chapter_waiters.borrow().is_empty() {
                                self.load_chapters();
                            }
                        },
                    );
                }
            }
        });
    }

    pub fn with_translations(
        &'static self,
        f: impl FnOnce(Rc<Vec<TranslationResource>>) + 'static,
    ) {
        if let Some(t) = self.translations.borrow().clone() {
            return f(t);
        }
        let client = self.client.clone();
        runtime::spawn(
            async move { client.translations().await },
            move |res| match res {
                Ok(list) => {
                    let list = Rc::new(list);
                    *self.translations.borrow_mut() = Some(list.clone());
                    f(list);
                }
                Err(e) => self.toast_error("Couldn't load translations", &e),
            },
        );
    }

    pub fn with_tafsirs(&'static self, f: impl FnOnce(Rc<Vec<TafsirResource>>) + 'static) {
        if let Some(t) = self.tafsirs.borrow().clone() {
            return f(t);
        }
        let client = self.client.clone();
        runtime::spawn(
            async move { client.tafsirs().await },
            move |res| match res {
                Ok(list) => {
                    let list = Rc::new(list);
                    *self.tafsirs.borrow_mut() = Some(list.clone());
                    f(list);
                }
                Err(e) => self.toast_error("Couldn't load tafsirs", &e),
            },
        );
    }

    pub fn with_reciters(&'static self, f: impl FnOnce(Rc<Vec<Reciter>>) + 'static) {
        if let Some(t) = self.reciters.borrow().clone() {
            return f(t);
        }
        let client = self.client.clone();
        runtime::spawn(
            async move { client.reciters().await },
            move |res| match res {
                Ok(list) => {
                    let list = Rc::new(list);
                    *self.reciters.borrow_mut() = Some(list.clone());
                    f(list);
                }
                Err(e) => self.toast_error("Couldn't load reciters", &e),
            },
        );
    }

    pub fn reciter_name(&self, id: u32) -> String {
        self.reciters
            .borrow()
            .as_ref()
            .and_then(|r| {
                r.iter()
                    .find(|r| r.id == id)
                    .map(|r| r.display_name().to_string())
            })
            .unwrap_or_default()
    }

    pub fn toast(&self, msg: &str) {
        if let Some(t) = self.toasts.get() {
            let toast = adw::Toast::new(msg);
            toast.set_timeout(3);
            t.add_toast(toast);
        }
    }

    pub fn toast_error(&self, what: &str, err: &dyn std::fmt::Display) {
        eprintln!("{what}: {err}");
        let hint = if err.to_string().contains("network") {
            " Check your internet connection and try again."
        } else {
            " Please try again."
        };
        self.toast(&format!("{what}.{hint}"));
    }

    pub fn window(&self) -> &adw::ApplicationWindow {
        self.window.get().expect("window")
    }

    pub fn ui(&self) -> &Rc<crate::window::Ui> {
        self.ui.get().expect("ui")
    }

    pub fn copy_to_clipboard(&self, text: &str) {
        self.window().clipboard().set_text(text);
        self.toast("Copied to clipboard");
    }
}
