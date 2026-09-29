//! Main window: navigation (home → reader), settings drawer, player bar, shortcuts.

use crate::ctx::ctx;
use crate::theme;
use crate::ui::{home::Home, player_bar::PlayerBar, reader::Reader, settings::SettingsPanel};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::rc::Rc;

/// Top-level views; owned here so they live as long as the window.
#[allow(dead_code)]
pub struct Ui {
    pub home: Rc<Home>,
    pub reader: Rc<Reader>,
    pub player_bar: Rc<PlayerBar>,
    pub settings: Rc<SettingsPanel>,
}

pub fn build(app: &adw::Application) {
    let c = ctx();
    let win = adw::ApplicationWindow::new(app);
    win.set_title(Some("Quran"));
    win.set_default_size(1280, 880);
    win.set_size_request(380, 480);
    let _ = c.window.set(win.clone());

    let display = gdk::Display::default().expect("display");
    // Every interaction should be instant: no slide/crossfade transitions.
    gtk::Settings::for_display(&display).set_gtk_enable_animations(false);
    theme::init(&display);
    theme::apply(&c.settings());
    adw::StyleManager::default().connect_dark_notify(|_| {
        if ctx().with_settings(|s| s.theme == quran_core::Theme::Auto) {
            theme::apply(&ctx().settings());
        }
    });

    let home = Home::new();
    let reader = Reader::new();
    let player_bar = PlayerBar::new();
    let settings = SettingsPanel::new();

    let nav = adw::NavigationView::new();
    nav.add(&home.page);
    let _ = c.nav.set(nav.clone());
    {
        let home = home.clone();
        nav.connect_popped(move |_, _| home.refresh());
    }

    let main = gtk::Box::new(gtk::Orientation::Vertical, 0);
    nav.set_vexpand(true);
    main.append(&nav);
    main.append(&player_bar.root);

    let split = adw::OverlaySplitView::new();
    split.set_sidebar_position(gtk::PackType::End);
    split.set_sidebar(Some(&settings.root));
    split.set_content(Some(&main));
    split.set_collapsed(true);
    split.set_show_sidebar(false);
    split.set_max_sidebar_width(400.0);
    split.set_min_sidebar_width(340.0);
    let _ = c.settings_split.set(split.clone());

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&split));
    let _ = c.toasts.set(toasts.clone());
    win.set_content(Some(&toasts));

    let _ = c.ui.set(Rc::new(Ui {
        home,
        reader,
        player_bar,
        settings,
    }));

    crate::ui::common::install_pointer_cursor(&win);
    install_actions(app, &win);
    crate::mpris::start();
    crate::debug::start();
    win.present();
}

fn install_actions(app: &adw::Application, win: &adw::ApplicationWindow) {
    let add = |name: &str, accels: &[&str], f: fn()| {
        let a = gio::SimpleAction::new(name, None);
        a.connect_activate(move |_, _| f());
        app.add_action(&a);
        if !accels.is_empty() {
            app.set_accels_for_action(&format!("app.{name}"), accels);
        }
    };
    add("search", &["<Control>k", "<Control>f"], || {
        crate::ui::search::show(None)
    });
    add("settings", &["<Control>comma"], || {
        let s = ctx().settings_split.get().unwrap();
        s.set_show_sidebar(!s.shows_sidebar());
    });
    add("next-verse", &["<Alt>Right"], || ctx().player.next_verse());
    add("prev-verse", &["<Alt>Left"], || ctx().player.prev_verse());
    add("home", &["<Alt>Home"], || {
        ctx().nav.get().unwrap().pop_to_tag("home");
    });
    add("quit", &["<Control>q"], || {
        ctx().window().close();
    });
    add("font-bigger", &["<Control>plus", "<Control>equal"], || {
        ctx().update_settings(|s| {
            s.quran_font_scale = (s.quran_font_scale + 1).min(10);
            s.translation_font_scale = (s.translation_font_scale + 1).min(10);
        })
    });
    add("font-smaller", &["<Control>minus"], || {
        ctx().update_settings(|s| {
            s.quran_font_scale = s.quran_font_scale.saturating_sub(1).max(1);
            s.translation_font_scale = s.translation_font_scale.saturating_sub(1).max(1);
        })
    });
    add("toggle-mode", &["<Control>r"], || {
        ctx().update_settings(|s| {
            s.reading_mode = match s.reading_mode {
                quran_core::ReadingMode::Translation => quran_core::ReadingMode::Reading,
                quran_core::ReadingMode::Reading => quran_core::ReadingMode::Translation,
            }
        })
    });

    // Space toggles playback unless typing in a text field.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let w = win.clone();
    keys.connect_key_pressed(move |_, key, _, mods| {
        if key == gdk::Key::space && mods.is_empty() {
            let typing = gtk::prelude::GtkWindowExt::focus(&w)
                .map(|f| f.is::<gtk::Text>() || f.is::<gtk::TextView>() || f.is::<gtk::Editable>())
                .unwrap_or(false);
            if !typing && ctx().player.snapshot().active {
                ctx().player.toggle();
                return glib::Propagation::Stop;
            }
        }
        if key == gdk::Key::Escape {
            let s = ctx().settings_split.get().unwrap();
            if s.shows_sidebar() {
                s.set_show_sidebar(false);
                return glib::Propagation::Stop;
            }
        }
        glib::Propagation::Proceed
    });
    win.add_controller(keys);
}

/// Open a chapter in the reader (pushing it if needed).
pub fn open_chapter(chapter: u32, verse: Option<u32>) {
    ctx().with_chapters(move |_| {
        let c = ctx();
        let ui = c.ui();
        ui.reader.open(chapter, verse);
        let nav = c.nav.get().unwrap();
        let on_reader = nav
            .visible_page()
            .and_then(|p| p.tag())
            .map(|t| t == "reader")
            .unwrap_or(false);
        if !on_reader {
            nav.push(&ui.reader.page);
        }
    });
}
