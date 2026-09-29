//! Bottom audio player, mirroring quran.com's player: progress on top,
//! transport in the middle, reciter / verse on the side.

use super::common::*;
use crate::audio::Snapshot;
use crate::ctx::ctx;
use adw::prelude::*;
use quran_core::RepeatMode;
use std::cell::Cell;
use std::rc::Rc;

pub struct PlayerBar {
    pub root: gtk::Revealer,
    scale: gtk::Scale,
    elapsed: gtk::Label,
    total: gtk::Label,
    play: gtk::Button,
    spinner: adw::Spinner,
    title: gtk::Label,
    subtitle: gtk::Label,
    repeat: gtk::Button,
    speed: gtk::MenuButton,
    download: gtk::Button,
    dragging: Cell<bool>,
    updating: Cell<bool>,
}

const SPEEDS: [f64; 7] = [0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0];

impl PlayerBar {
    pub fn new() -> Rc<Self> {
        let root = gtk::Revealer::new();
        root.set_transition_type(gtk::RevealerTransitionType::SlideUp);
        root.set_transition_duration(180);

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 2);
        outer.add_css_class("player-bar");

        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        outer.append(&scale);

        let row = gtk::CenterBox::new();
        // start: reciter & position
        let info = gtk::Box::new(gtk::Orientation::Vertical, 0);
        info.set_valign(gtk::Align::Center);
        let title = label("", &["player-title"]);
        let subtitle = label("", &["player-subtitle"]);
        title.set_ellipsize(pango::EllipsizeMode::End);
        subtitle.set_ellipsize(pango::EllipsizeMode::End);
        title.set_max_width_chars(28);
        info.append(&title);
        info.append(&subtitle);
        let reciter_btn = gtk::Button::new();
        reciter_btn.set_child(Some(&info));
        reciter_btn.add_css_class("flat");
        reciter_btn.set_tooltip_text(Some("Change reciter"));
        reciter_btn.connect_clicked(|_| super::pickers::show_reciters());
        row.set_start_widget(Some(&reciter_btn));

        // center: transport
        let center = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let repeat = icon_button("media-playlist-repeat-symbolic", "Repeat: off");
        let prev = icon_button("media-skip-backward-symbolic", "Previous verse");
        let play = gtk::Button::from_icon_name("media-playback-start-symbolic");
        play.add_css_class("play-button");
        play.set_tooltip_text(Some("Play / pause (Space)"));
        let spinner = adw::Spinner::new();
        spinner.set_visible(false);
        let next = icon_button("media-skip-forward-symbolic", "Next verse");
        let speed = gtk::MenuButton::new();
        speed.set_label("1x");
        speed.add_css_class("flat");
        speed.set_tooltip_text(Some("Playback speed"));
        speed.set_valign(gtk::Align::Center);
        let speed_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let speed_pop = gtk::Popover::new();
        speed_pop.set_child(Some(&speed_box));
        install_pointer_cursor(&speed_pop);
        speed.set_popover(Some(&speed_pop));
        for s in SPEEDS {
            let b = gtk::Button::with_label(&speed_label(s));
            b.add_css_class("flat");
            let pop = speed_pop.clone();
            b.connect_clicked(move |_| {
                pop.popdown();
                ctx().update_settings(|st| st.playback_rate = s);
                ctx().player.set_rate(s);
            });
            speed_box.append(&b);
        }
        center.append(&repeat);
        center.append(&prev);
        center.append(&play);
        center.append(&spinner);
        center.append(&next);
        center.append(&speed);
        row.set_center_widget(Some(&center));

        // end: time, download, close
        let end = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let elapsed = label("0:00", &["player-time"]);
        let slash = label("/", &["player-time"]);
        let total = label("0:00", &["player-time"]);
        let download = icon_button("folder-download-symbolic", "Download for offline listening");
        let close = icon_button("window-close-symbolic", "Close player");
        end.append(&elapsed);
        end.append(&slash);
        end.append(&total);
        end.append(&download);
        end.append(&close);
        row.set_end_widget(Some(&end));
        outer.append(&row);
        root.set_child(Some(&outer));

        let bar = Rc::new(Self {
            root,
            scale,
            elapsed,
            total,
            play,
            spinner,
            title,
            subtitle,
            repeat,
            speed,
            download,
            dragging: Cell::new(false),
            updating: Cell::new(false),
        });

        bar.play.connect_clicked(|_| ctx().player.toggle());
        prev.connect_clicked(|_| ctx().player.prev_verse());
        next.connect_clicked(|_| ctx().player.next_verse());
        close.connect_clicked(|_| ctx().player.stop());
        bar.download
            .connect_clicked(|_| ctx().player.download_current());
        bar.repeat.connect_clicked(|_| {
            ctx().update_settings(|s| {
                s.repeat = match s.repeat {
                    RepeatMode::Off => RepeatMode::Verse,
                    RepeatMode::Verse => RepeatMode::Chapter,
                    RepeatMode::Chapter => RepeatMode::Off,
                }
            })
        });

        // Seeking: apply on release so dragging doesn't spam the pipeline.
        let w = Rc::downgrade(&bar);
        bar.scale.connect_change_value(move |_, _, value| {
            if let Some(b) = w.upgrade() {
                if !b.updating.get() {
                    b.dragging.set(true);
                    b.elapsed.set_text(&format_ms(value.max(0.0) as u64));
                }
            }
            gtk::glib::Propagation::Proceed
        });
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let w = Rc::downgrade(&bar);
        click.connect_released(move |_, _, _, _| {
            if let Some(b) = w.upgrade() {
                if b.dragging.replace(false) {
                    ctx().player.seek_ms(b.scale.value().max(0.0) as u64);
                }
            }
        });
        bar.scale.add_controller(click);

        let w = Rc::downgrade(&bar);
        ctx().player.connect(move |snap| {
            if let Some(b) = w.upgrade() {
                b.update(snap);
            }
        });
        let w = Rc::downgrade(&bar);
        ctx().connect_settings(move |_, new| {
            if let Some(b) = w.upgrade() {
                b.update_settings(new.repeat, new.playback_rate);
                b.update(&ctx().player.snapshot());
            }
        });
        let s = ctx().settings();
        bar.update_settings(s.repeat, s.playback_rate);
        bar
    }

    fn update_settings(&self, repeat: RepeatMode, rate: f64) {
        let (icon, tip, on) = match repeat {
            RepeatMode::Off => ("media-playlist-repeat-symbolic", "Repeat: off", false),
            RepeatMode::Verse => ("media-playlist-repeat-song-symbolic", "Repeat: verse", true),
            RepeatMode::Chapter => ("media-playlist-repeat-symbolic", "Repeat: surah", true),
        };
        self.repeat.set_icon_name(icon);
        self.repeat.set_tooltip_text(Some(tip));
        if on {
            self.repeat.add_css_class("toggle-on");
        } else {
            self.repeat.remove_css_class("toggle-on");
        }
        self.speed.set_label(&speed_label(rate));
    }

    fn update(&self, snap: &Snapshot) {
        self.root.set_reveal_child(snap.active);
        if !snap.active {
            return;
        }
        self.play.set_icon_name(if snap.playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        });
        self.spinner.set_visible(snap.loading);
        let chapter = ctx().chapter(snap.chapter);
        let name = chapter
            .as_ref()
            .map(|c| c.name_simple.clone())
            .unwrap_or_default();
        self.title
            .set_text(&format!("{name} · {}:{}", snap.chapter, snap.verse));
        let reciter = ctx().reciter_name(snap.reciter);
        if reciter.is_empty() {
            // load names once, then refresh
            ctx().with_reciters(|_| {});
        }
        self.subtitle.set_text(&reciter);
        self.download
            .set_visible(!ctx().player.is_downloaded(snap.reciter, snap.chapter));
        if !self.dragging.get() {
            self.updating.set(true);
            let dur = snap.duration_ms.max(1) as f64;
            if (self.scale.adjustment().upper() - dur).abs() > 0.5 {
                self.scale.set_range(0.0, dur);
            }
            self.scale.set_value(snap.position_ms as f64);
            self.updating.set(false);
            self.elapsed.set_text(&format_ms(snap.position_ms));
        }
        self.total.set_text(&format_ms(snap.duration_ms));
    }
}

fn speed_label(s: f64) -> String {
    if (s - s.round()).abs() < f64::EPSILON {
        format!("{}x", s as u32)
    } else {
        format!("{s}x")
    }
}
