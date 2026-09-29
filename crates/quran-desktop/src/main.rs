mod audio;
mod ctx;
mod debug;
mod fonts;
mod mpris;
mod prof;
mod runtime;
mod theme;
mod ui;
mod window;

use adw::prelude::*;
use gtk::glib;

const APP_ID: &str = "com.quran.Desktop";

/// Record panics (which abort inside GTK callbacks) with a backtrace.
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let path = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
            .join(".local/share/quran-desktop/crash.log");
        let bt = std::backtrace::Backtrace::force_capture();
        let entry = format!(
            "--- {:?}\n{info}\n{bt}\n",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        );
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(entry.as_bytes());
        }
        default(info);
    }));
}

fn main() -> glib::ExitCode {
    install_crash_log();
    let client = match quran_core::open_default() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Couldn't open the local database: {e}");
            return glib::ExitCode::FAILURE;
        }
    };
    // `quran 2:255` or `quran --open 18` jumps straight into the reader.
    let open = std::env::args()
        .skip(1)
        .filter(|a| a != "--open")
        .find_map(|a| {
            quran_core::parse_verse_key(&a)
                .map(|(c, v)| (c, Some(v)))
                .or_else(|| a.parse::<u32>().ok().map(|c| (c, None)))
        })
        .filter(|(c, _)| (1..=114).contains(c));
    let mut flags = gtk::gio::ApplicationFlags::empty();
    if std::env::var_os("QURAN_DEBUG_DIR").is_some() {
        flags |= gtk::gio::ApplicationFlags::NON_UNIQUE;
    }
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(flags)
        .build();
    app.connect_startup(move |_| {
        ctx::init(client.clone());
    });
    app.connect_activate(move |app| {
        if let Some(w) = ctx::ctx().window.get() {
            w.present();
            return;
        }
        window::build(app);
        if let Some((c, v)) = open {
            window::open_chapter(c, v);
        }
    });
    app.run_with_args::<&str>(&[])
}
