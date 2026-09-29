//! Recitation playback on GStreamer with verse- and word-level sync.
//!
//! Chapter recitations come as a single file plus millisecond timings for every
//! verse and word (quran.com QDC audio). Position updates map to the current
//! verse/word, which the reader highlights.

use crate::ctx::ctx;
use crate::runtime;
use gst::prelude::*;
use quran_core::{ChapterAudio, RepeatMode};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// A chapter is loaded (the bar is visible).
    pub active: bool,
    pub chapter: u32,
    pub reciter: u32,
    pub verse: u32,
    pub word: Option<u32>,
    pub playing: bool,
    pub loading: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
}

#[derive(Default)]
struct State {
    snap: Snapshot,
    audio: Option<Rc<ChapterAudio>>,
    token: u64,
    /// Seek to apply once the stream is ready.
    pending_seek: Option<u64>,
    /// Stop at the end of this verse (single-verse playback).
    stop_after: Option<u32>,
}

type Listener = Box<dyn Fn(&Snapshot)>;

pub struct Player {
    play: gst_play::Play,
    _adapter: gst_play::PlaySignalAdapter,
    word: gst_play::Play,
    st: RefCell<State>,
    listeners: RefCell<Vec<Listener>>,
}

impl Player {
    pub fn new() -> Self {
        gst::init().expect("GStreamer");
        let play = gst_play::Play::new(None::<gst_play::PlayVideoRenderer>);
        let mut config = play.config();
        config.set_position_update_interval(50);
        config.set_seek_accurate(true);
        let _ = play.set_config(config);
        disable_video(&play);

        let adapter = gst_play::PlaySignalAdapter::new(&play);
        adapter.connect_position_updated(|_, pos| {
            if let Some(pos) = pos {
                ctx().player.on_position(pos.mseconds());
            }
        });
        adapter.connect_duration_changed(|_, dur| {
            if let Some(d) = dur {
                ctx().player.mutate(|s| s.snap.duration_ms = d.mseconds());
            }
        });
        adapter.connect_state_changed(|_, state| {
            ctx().player.on_state(state);
        });
        adapter.connect_end_of_stream(|_| ctx().player.on_eos());
        adapter.connect_error(|_, err, _| {
            let p = &ctx().player;
            p.mutate(|s| {
                s.snap.playing = false;
                s.snap.loading = false;
            });
            ctx().toast_error("Playback failed", err);
        });

        let word = gst_play::Play::new(None::<gst_play::PlayVideoRenderer>);
        disable_video(&word);

        Self {
            play,
            _adapter: adapter,
            word,
            st: RefCell::default(),
            listeners: RefCell::default(),
        }
    }

    pub fn connect(&self, f: impl Fn(&Snapshot) + 'static) {
        self.listeners.borrow_mut().push(Box::new(f));
    }

    pub fn snapshot(&self) -> Snapshot {
        self.st.borrow().snap.clone()
    }

    pub fn audio(&self) -> Option<Rc<ChapterAudio>> {
        self.st.borrow().audio.clone()
    }

    fn mutate(&self, f: impl FnOnce(&mut State)) {
        let (before, after) = {
            let mut st = self.st.borrow_mut();
            let before = st.snap.clone();
            f(&mut st);
            (before, st.snap.clone())
        };
        if before != after {
            for l in self.listeners.borrow().iter() {
                l(&after);
            }
        }
    }

    /// Start (or restart) a chapter, optionally from a verse.
    pub fn play_chapter(&'static self, chapter: u32, from_verse: Option<u32>) {
        self.play_range(chapter, from_verse, None)
    }

    /// Play a single verse and stop.
    pub fn play_verse_only(&'static self, chapter: u32, verse: u32) {
        self.play_range(chapter, Some(verse), Some(verse))
    }

    fn play_range(&'static self, chapter: u32, from_verse: Option<u32>, stop_after: Option<u32>) {
        let reciter = ctx().with_settings(|s| s.reciter);
        // Same chapter & reciter already loaded: just seek.
        let reuse = {
            let st = self.st.borrow();
            st.audio.is_some() && st.snap.chapter == chapter && st.snap.reciter == reciter
        };
        if reuse {
            self.mutate(|s| s.stop_after = stop_after);
            self.seek_verse(from_verse.unwrap_or(1));
            self.play.play();
            return;
        }

        self.play.stop();
        let token = {
            let mut st = self.st.borrow_mut();
            st.token += 1;
            st.token
        };
        self.mutate(|s| {
            s.audio = None;
            s.stop_after = stop_after;
            s.pending_seek = None;
            s.snap = Snapshot {
                active: true,
                chapter,
                reciter,
                verse: from_verse.unwrap_or(1),
                word: None,
                playing: false,
                loading: true,
                position_ms: 0,
                duration_ms: 0,
            };
        });

        let client = ctx().client.clone();
        runtime::spawn(
            async move { client.chapter_audio(reciter, chapter).await },
            move |res| {
                if self.st.borrow().token != token {
                    return;
                }
                let audio = match res {
                    Ok(a) => Rc::new(a),
                    Err(e) => {
                        self.mutate(|s| s.snap.loading = false);
                        ctx().toast_error("Couldn't load this recitation", &e);
                        return;
                    }
                };
                let local = ctx().client.local_audio_path(reciter, chapter);
                let uri = if local.exists() {
                    gtk::glib::filename_to_uri(&local, None)
                        .map(|u| u.to_string())
                        .unwrap_or_else(|_| audio.audio_url.clone())
                } else {
                    audio.audio_url.clone()
                };
                let seek = from_verse
                    .filter(|v| *v > 1)
                    .and_then(|v| audio.timing_for_verse(v))
                    .map(|t| t.timestamp_from);
                self.mutate(|s| {
                    s.audio = Some(audio.clone());
                    s.pending_seek = seek;
                    s.snap.duration_ms = audio.duration;
                });
                self.play.set_uri(Some(&uri));
                self.play.set_rate(ctx().with_settings(|s| s.playback_rate));
                self.play.play();
            },
        );
    }

    pub fn toggle(&'static self) {
        let snap = self.snapshot();
        if !snap.active {
            return;
        }
        if snap.playing {
            self.play.pause();
        } else if self.st.borrow().audio.is_some() {
            self.play.play();
        }
    }

    pub fn pause(&self) {
        self.play.pause();
    }

    pub fn resume(&self) {
        if self.st.borrow().audio.is_some() {
            self.play.play();
        }
    }

    pub fn stop(&self) {
        self.play.stop();
        self.mutate(|s| {
            s.token += 1;
            s.audio = None;
            s.snap = Snapshot::default();
        });
    }

    pub fn seek_ms(&self, ms: u64) {
        self.play.seek(gst::ClockTime::from_mseconds(ms));
        self.mutate(|s| s.snap.position_ms = ms);
    }

    pub fn seek_verse(&self, verse: u32) {
        let Some(audio) = self.audio() else { return };
        if let Some(t) = audio.timing_for_verse(verse) {
            self.seek_ms(t.timestamp_from);
            self.mutate(|s| {
                s.snap.verse = verse;
                s.snap.word = None;
            });
        }
    }

    pub fn next_verse(&self) {
        let snap = self.snapshot();
        let Some(audio) = self.audio() else { return };
        if snap.verse < audio.verse_timings.len() as u32 {
            self.seek_verse(snap.verse + 1);
        }
    }

    pub fn prev_verse(&self) {
        let snap = self.snapshot();
        if snap.verse > 1 {
            self.seek_verse(snap.verse - 1);
        }
    }

    pub fn set_rate(&self, rate: f64) {
        self.play.set_rate(rate);
    }

    /// Reload the current chapter with the reciter from settings, keeping the verse.
    pub fn reciter_changed(&'static self) {
        let snap = self.snapshot();
        if snap.active {
            let resume = snap.playing || snap.loading;
            self.play_chapter(snap.chapter, Some(snap.verse));
            if !resume {
                self.play.pause();
            }
        }
    }

    pub fn play_word(&self, url: &str) {
        self.word.stop();
        self.word
            .set_uri(Some(&quran_core::api::wbw_audio_url(url)));
        self.word.play();
    }

    fn on_state(&self, state: gst_play::PlayState) {
        use gst_play::PlayState::*;
        let pending = if matches!(state, Playing | Paused) {
            self.st.borrow_mut().pending_seek.take()
        } else {
            None
        };
        if let Some(ms) = pending {
            self.seek_ms(ms);
        }
        self.mutate(|s| match state {
            Playing => {
                s.snap.playing = true;
                s.snap.loading = false;
            }
            Paused => {
                s.snap.playing = false;
                s.snap.loading = false;
            }
            Buffering => s.snap.loading = true,
            Stopped => s.snap.playing = false,
            _ => {}
        });
    }

    fn on_position(&self, ms: u64) {
        let Some(audio) = self.audio() else { return };
        let (repeat, word_highlight) = ctx().with_settings(|s| (s.repeat, s.word_highlight));
        let idx = audio.timing_index_at(ms);
        let timing = idx.and_then(|i| audio.verse_timings.get(i));
        let verse = timing.map(|t| t.verse_number()).unwrap_or(1);
        let snap = self.snapshot();
        let stop_after = self.st.borrow().stop_after;

        // Leaving the verse being repeated / played alone.
        if verse != snap.verse && snap.verse > 0 && ms > 0 {
            if repeat == RepeatMode::Verse {
                if let Some(t) = audio.timing_for_verse(snap.verse) {
                    if ms >= t.timestamp_to {
                        self.seek_ms(t.timestamp_from);
                        return;
                    }
                }
            } else if stop_after == Some(snap.verse) {
                self.play.pause();
                self.mutate(|s| s.stop_after = None);
                if let Some(t) = audio.timing_for_verse(snap.verse) {
                    self.seek_ms(t.timestamp_from);
                }
                return;
            }
        }

        let word = if word_highlight {
            timing.and_then(|t| t.word_at(ms))
        } else {
            None
        };
        self.mutate(|s| {
            s.snap.position_ms = ms;
            s.snap.verse = verse;
            s.snap.word = word;
        });
    }

    fn on_eos(&self) {
        let repeat = ctx().with_settings(|s| s.repeat);
        match repeat {
            RepeatMode::Chapter => {
                self.seek_ms(0);
                self.play.play();
            }
            RepeatMode::Verse => {
                let v = self.snapshot().verse;
                self.seek_verse(v);
                self.play.play();
            }
            RepeatMode::Off => {
                self.play.pause();
                self.mutate(|s| {
                    s.snap.playing = false;
                    s.snap.word = None;
                });
            }
        }
    }

    /// Save the current chapter's recitation for offline playback.
    pub fn download_current(&'static self) {
        let snap = self.snapshot();
        if !snap.active {
            return;
        }
        let client = ctx().client.clone();
        ctx().toast("Downloading recitation…");
        runtime::spawn(
            async move {
                client
                    .download_chapter_audio(snap.reciter, snap.chapter)
                    .await
            },
            |res| match res {
                Ok(_) => ctx().toast("Recitation saved for offline listening"),
                Err(e) => ctx().toast_error("Download failed", &e),
            },
        );
    }

    pub fn is_downloaded(&self, reciter: u32, chapter: u32) -> bool {
        ctx().client.local_audio_path(reciter, chapter).exists()
    }
}

fn disable_video(play: &gst_play::Play) {
    let pipeline = play.pipeline();
    // playbin flags: audio (0x2) + soft-volume (0x10); no video, no subtitles
    if pipeline.has_property("flags") {
        let flags = pipeline.property_value("flags");
        if let Ok(class) = glib_flags_class(&flags) {
            if let Some(v) = class
                .builder_with_value(flags.clone())
                .and_then(|b| b.unset_by_nick("video").unset_by_nick("text").build())
            {
                pipeline.set_property_from_value("flags", &v);
            }
        }
    }
}

fn glib_flags_class(v: &gtk::glib::Value) -> Result<gtk::glib::FlagsClass, ()> {
    gtk::glib::FlagsClass::with_type(v.type_()).ok_or(())
}
