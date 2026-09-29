//! MPRIS integration: media keys, lock screen and desktop shell controls.

use crate::audio::Snapshot;
use crate::ctx::ctx;
use gtk::glib;
use gtk::prelude::GtkWindowExt;
use mpris_server::{Metadata, PlaybackStatus, Player, Time};
use std::cell::RefCell;
use std::rc::Rc;

pub fn start() {
    glib::spawn_future_local(async {
        let player = match Player::builder("QuranDesktop")
            .identity("Quran")
            .desktop_entry(crate::APP_ID)
            .can_play(true)
            .can_pause(true)
            .can_go_next(true)
            .can_go_previous(true)
            .can_seek(true)
            .can_control(true)
            .build()
            .await
        {
            Ok(p) => Rc::new(p),
            Err(e) => {
                eprintln!("MPRIS unavailable: {e}");
                return;
            }
        };
        player.connect_play_pause(|_| ctx().player.toggle());
        player.connect_play(|_| ctx().player.resume());
        player.connect_pause(|_| ctx().player.pause());
        player.connect_stop(|_| ctx().player.stop());
        player.connect_next(|_| ctx().player.next_verse());
        player.connect_previous(|_| ctx().player.prev_verse());
        player.connect_raise(|_| ctx().window().present());
        player.connect_seek(|_, offset| {
            let pos = ctx().player.snapshot().position_ms as i64 + offset.as_millis();
            ctx().player.seek_ms(pos.max(0) as u64);
        });
        player.connect_set_position(|_, _, pos| {
            ctx().player.seek_ms(pos.as_millis().max(0) as u64);
        });
        glib::spawn_future_local(player.run());

        let last: Rc<RefCell<Option<Snapshot>>> = Rc::default();
        let p = player.clone();
        ctx().player.connect(move |snap| {
            p.set_position(Time::from_millis(snap.position_ms as i64));
            let prev = last.replace(Some(snap.clone()));
            let changed = prev
                .as_ref()
                .map(|s| {
                    s.active != snap.active
                        || s.playing != snap.playing
                        || s.chapter != snap.chapter
                        || s.verse != snap.verse
                        || s.reciter != snap.reciter
                        || s.duration_ms != snap.duration_ms
                })
                .unwrap_or(true);
            if !changed {
                return;
            }
            let p = p.clone();
            let snap = snap.clone();
            glib::spawn_future_local(async move {
                let status = if !snap.active {
                    PlaybackStatus::Stopped
                } else if snap.playing {
                    PlaybackStatus::Playing
                } else {
                    PlaybackStatus::Paused
                };
                let _ = p.set_playback_status(status).await;
                if snap.active {
                    let name = ctx()
                        .chapter(snap.chapter)
                        .map(|c| c.name_simple)
                        .unwrap_or_default();
                    let meta = Metadata::builder()
                        .title(format!("{name} · {}:{}", snap.chapter, snap.verse))
                        .artist([ctx().reciter_name(snap.reciter)])
                        .album("The Noble Quran")
                        .length(Time::from_millis(snap.duration_ms as i64))
                        .build();
                    let _ = p.set_metadata(meta).await;
                }
            });
        });
    });
}
