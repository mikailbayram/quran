//! Development hook, enabled with `QURAN_DEBUG_DIR=/some/dir`.
//!
//! Write commands into `$QURAN_DEBUG_DIR/cmd` (one per line) and the app runs
//! them: `snap NAME` renders the window to `NAME.png`, `open 2:255`,
//! `mode reading|translation`, `theme light|sepia|dark|auto`, `font v2|hafs|indopak`,
//! `settings`, `play`, `pause`, `search TEXT`, `tafsir 2:255`, `back`, `scroll N`.

use crate::ctx::ctx;
use adw::prelude::*;
use gtk::glib;
use std::path::PathBuf;
use std::time::Duration;

pub fn start() {
    let Some(dir) = std::env::var_os("QURAN_DEBUG_DIR").map(PathBuf::from) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    glib::timeout_add_local(Duration::from_millis(300), move || {
        let cmd_file = dir.join("cmd");
        if let Ok(text) = std::fs::read_to_string(&cmd_file) {
            let _ = std::fs::remove_file(&cmd_file);
            for line in text.lines() {
                run(&dir, line.trim());
            }
        }
        glib::ControlFlow::Continue
    });
}

fn run(dir: &std::path::Path, line: &str) {
    let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
    match cmd {
        "snap" => snap(&dir.join(format!("{arg}.png"))),
        "open" => {
            if let Some((c, v)) = quran_core::parse_verse_key(arg) {
                crate::window::open_chapter(c, Some(v));
            } else if let Ok(c) = arg.parse() {
                crate::window::open_chapter(c, None);
            }
        }
        "mode" => ctx().update_settings(|s| {
            s.reading_mode = if arg == "reading" {
                quran_core::ReadingMode::Reading
            } else {
                quran_core::ReadingMode::Translation
            }
        }),
        "theme" => ctx().update_settings(|s| {
            s.theme = match arg {
                "light" => quran_core::Theme::Light,
                "sepia" => quran_core::Theme::Sepia,
                "dark" => quran_core::Theme::Dark,
                _ => quran_core::Theme::Auto,
            }
        }),
        "font" => ctx().update_settings(|s| {
            s.quran_font = match arg {
                "hafs" => quran_core::QuranFont::Uthmani,
                "indopak" => quran_core::QuranFont::IndoPak,
                "v1" => quran_core::QuranFont::QpcV1,
                _ => quran_core::QuranFont::QpcV2,
            }
        }),
        "wbw" => ctx().update_settings(|s| s.wbw_inline = arg == "inline"),
        "settings" => {
            let s = ctx().settings_split.get().unwrap();
            s.set_show_sidebar(!s.shows_sidebar());
        }
        "play" => {
            let ch = ctx().ui().reader.chapter();
            let v = arg.parse().ok();
            ctx().player.play_chapter(ch, v);
        }
        "pause" => ctx().player.pause(),
        "search" => crate::ui::search::show(Some(arg)),
        "tafsir" => crate::ui::dialogs::show_tafsir(arg),
        "info" => crate::ui::dialogs::show_chapter_info(arg.parse().unwrap_or(1)),
        "reciters" => crate::ui::pickers::show_reciters(),
        "translations" => crate::ui::pickers::show_translations(),
        "back" => {
            ctx().nav.get().unwrap().pop();
        }
        "size" => {
            if let Some((w, h)) = arg.split_once('x') {
                ctx()
                    .window()
                    .set_default_size(w.parse().unwrap_or(1280), h.parse().unwrap_or(880));
            }
        }
        "scroll" => ctx().ui().reader.scroll_to_verse(arg.parse().unwrap_or(1)),
        "close-dialogs" => {
            if let Some(d) = ctx().window().visible_dialog() {
                d.close();
            }
        }
        "bench" => bench(arg),
        "t" => {
            // time an interaction until two frames after it have been painted
            let (what, rest) = arg.split_once(' ').unwrap_or((arg, ""));
            let rest = rest.to_string();
            let label = arg.to_string();
            timed(label, move || match what {
                "open" => run(std::path::Path::new("/"), &format!("open {rest}")),
                "back" => {
                    ctx().nav.get().unwrap().pop();
                }
                "switch" => ctx().update_settings(|s| {
                    s.reading_mode = match s.reading_mode {
                        quran_core::ReadingMode::Translation => quran_core::ReadingMode::Reading,
                        quran_core::ReadingMode::Reading => quran_core::ReadingMode::Translation,
                    }
                }),
                "settings" => {
                    let s = ctx().settings_split.get().unwrap();
                    s.set_show_sidebar(!s.shows_sidebar());
                }
                "theme" => run(std::path::Path::new("/"), &format!("theme {rest}")),
                "search" => crate::ui::search::show(None),
                "close" => {
                    if let Some(d) = ctx().window().visible_dialog() {
                        d.close();
                    }
                }
                _ => eprintln!("t: unknown {what}"),
            });
        }
        "prof" => eprintln!(
            "prof on={:?}\n{}",
            std::env::var_os("QURAN_PROFILE"),
            crate::prof::report()
        ),
        "probe" => probe(ctx().window().upcast_ref(), &mut 0),
        _ => eprintln!("debug: unknown command {line}"),
    }
}

/// Render the whole window (including dialogs) to a PNG.
fn snap(path: &std::path::Path) {
    let win = ctx().window();
    let paintable = gtk::WidgetPaintable::new(Some(win));
    let (w, h) = (win.width() as f64, win.height() as f64);
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w, h);
    let Some(node) = snapshot.to_node() else {
        return;
    };
    let Some(renderer) = win.renderer() else {
        return;
    };
    let texture = renderer.render_texture(&node, None);
    if let Err(e) = texture.save_to_png(path) {
        eprintln!("debug snap: {e}");
    }
}

fn probe(w: &gtk::Widget, n: &mut u32) {
    use pango::prelude::*;
    if *n >= 4 {
        return;
    }
    if let Some(l) = w.downcast_ref::<gtk::Label>() {
        if l.has_css_class("arabic-word") && l.is_mapped() {
            *n += 1;
            let desc = l.pango_context().font_description().map(|d| d.to_string());
            let runs: Vec<String> = l
                .layout()
                .line(0)
                .map(|li| {
                    li.runs()
                        .iter()
                        .map(|r| r.item().analysis().font().describe().to_string())
                        .collect()
                })
                .unwrap_or_default();
            eprintln!(
                "probe classes={:?} ctxdesc={desc:?} runs={runs:?} text={:?}",
                l.css_classes(),
                l.text()
            );
        }
    }
    let mut c = w.first_child();
    while let Some(ch) = c {
        probe(&ch, n);
        c = ch.next_sibling();
    }
}

/// Simulated smooth scrolling through the current view while recording frame times.
fn bench(arg: &str) {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;
    crate::prof::reset();
    let steps: u32 = arg.parse().unwrap_or(400);
    let win = ctx().window();
    let Some(clock) = win.frame_clock() else {
        return;
    };
    let frames: Rc<RefCell<Vec<f64>>> = Rc::default();
    let last = Rc::new(Cell::new(None::<std::time::Instant>));
    let (f2, l2) = (frames.clone(), last.clone());
    let handler = Rc::new(Cell::new(Some(clock.connect_after_paint(move |_| {
        let now = std::time::Instant::now();
        if let Some(prev) = l2.replace(Some(now)) {
            f2.borrow_mut().push((now - prev).as_secs_f64() * 1000.0);
        }
    }))));
    let scroller = ctx().ui().reader.active_scroller();
    let adj = scroller.vadjustment();
    let n = Rc::new(Cell::new(0u32));
    let t0 = std::time::Instant::now();
    glib::timeout_add_local(Duration::from_millis(8), move || {
        n.set(n.get() + 1);
        adj.set_value(adj.value() + 45.0);
        if n.get() % 100 == 0 {
            eprintln!(
                "bench step {} value={:.0} upper={:.0}",
                n.get(),
                adj.value(),
                adj.upper()
            );
        }
        if n.get() < steps {
            return glib::ControlFlow::Continue;
        }
        if let Some(h) = handler.take() {
            clock.disconnect(h);
        }
        let mut f = frames.borrow().clone();
        f.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let pct = |p: f64| {
            f.get(((f.len() as f64 * p) as usize).min(f.len().saturating_sub(1)))
                .copied()
                .unwrap_or(0.0)
        };
        let janky = f.iter().filter(|x| **x > 20.0).count();
        eprintln!(
            "bench: {:.1}s frames={} p50={:.1}ms p90={:.1}ms p99={:.1}ms max={:.1}ms >20ms={}\n{}",
            t0.elapsed().as_secs_f64(),
            f.len(),
            pct(0.5),
            pct(0.9),
            pct(0.99),
            f.last().copied().unwrap_or(0.0),
            janky,
            crate::prof::report()
        );
        glib::ControlFlow::Break
    });
}

fn timed(label: String, f: impl FnOnce()) {
    use std::cell::Cell;
    use std::rc::Rc;
    crate::prof::reset();
    let t = std::time::Instant::now();
    f();
    let sync = t.elapsed();
    let clock = ctx().window().frame_clock().unwrap();
    let id = Rc::new(Cell::new(None));
    let id2 = id.clone();
    let frames = Rc::new(Cell::new(0));
    let h = clock.connect_after_paint(move |c| {
        frames.set(frames.get() + 1);
        if frames.get() == 2 {
            eprintln!(
                "[{label}] sync={:.1}ms painted={:.1}ms\n{}",
                sync.as_secs_f64() * 1e3,
                t.elapsed().as_secs_f64() * 1e3,
                crate::prof::report()
            );
            if let Some(h) = id2.take() {
                c.disconnect(h);
            }
        }
    });
    id.set(Some(h));
}
