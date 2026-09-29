//! quran.com colour themes (light / sepia / dark) mapped onto GTK + libadwaita.

use quran_core::{Settings, Theme};
use std::cell::OnceCell;

struct Palette {
    bg: &'static str,
    elevated: &'static str,
    alt: &'static str,
    alt_medium: &'static str,
    text: &'static str,
    faded: &'static str,
    accent: &'static str,
    accent_deep: &'static str,
    accent_faded: &'static str,
    accent_faint: &'static str,
    hairline: &'static str,
    search_bg: &'static str,
    word_hover: &'static str,
    /// Mushaf page: paper, ink and frame ornament colours.
    paper: &'static str,
    ink: &'static str,
    frame: &'static str,
    dark: bool,
}

// Values from quran.com-frontend-next/src/styles/themes/*.scss
const LIGHT: Palette = Palette {
    bg: "#ffffff",
    elevated: "#ffffff",
    alt: "#f4f5f6",
    alt_medium: "#e9ecef",
    text: "#272727",
    faded: "#666666",
    accent: "#2ca4ab",
    accent_deep: "#258c91",
    accent_faded: "#ebf9fa",
    accent_faint: "#c2edef",
    hairline: "rgb(235, 238, 240)",
    search_bg: "#e9ecef",
    word_hover: "#2ca4ab",
    paper: "#fdfbf5",
    ink: "#1c1a17",
    frame: "#b08a64",
    dark: false,
};

const SEPIA: Palette = Palette {
    bg: "#f8ebd5",
    elevated: "#fff7ea",
    alt: "#f0e2cc",
    alt_medium: "#efe2cd",
    text: "#010101",
    faded: "#666666",
    accent: "#72603f",
    accent_deep: "#3f2d0c",
    accent_faded: "#f2e0bf",
    accent_faint: "#d8c6a5",
    hairline: "#dbccb3",
    search_bg: "#efe2cd",
    word_hover: "#72603f",
    paper: "#fbf1dc",
    ink: "#1a120b",
    frame: "#9a7447",
    dark: false,
};

const DARK: Palette = Palette {
    bg: "#1f2125",
    elevated: "#25282c",
    alt: "#343a40",
    alt_medium: "#3a4047",
    text: "#e7e9ea",
    faded: "#9ba1a6",
    accent: "#2ca4ab",
    accent_deep: "#6fd3d9",
    accent_faded: "#272f33",
    accent_faint: "#3d474d",
    hairline: "rgb(70, 75, 80)",
    search_bg: "#2b2f33",
    word_hover: "#2ca4ab",
    paper: "#26282c",
    ink: "#efede8",
    frame: "#7d6750",
    dark: true,
};

thread_local! {
    static VARS: OnceCell<gtk::CssProvider> = const { OnceCell::new() };
}

pub fn init(display: &gtk::gdk::Display) {
    let base = gtk::CssProvider::new();
    let mut css = String::from(include_str!("style.css"));
    // One class per Quran font; page fonts resolve once downloaded.
    css.push_str(".qf-hafs { font-family: \"KFGQPC HAFS Uthmanic Script\"; }\n");
    css.push_str(".qf-indopak { font-family: \"AlQuran IndoPak by QuranWBW\"; }\n");
    for p in 1..=604 {
        css.push_str(&format!(".qf-p{p} {{ font-family: QCF2{p:03}; }}\n"));
        css.push_str(&format!(".qf-v1-p{p} {{ font-family: QCF_P{p:03}; }}\n"));
    }
    base.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        display,
        &base,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    VARS.with(|v| {
        let p = v.get_or_init(gtk::CssProvider::new);
        gtk::style_context_add_provider_for_display(
            display,
            p,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    });
}

/// (accent, accent-faint) for highlighted words.
pub fn accent_colors(s: &Settings) -> (&'static str, &'static str) {
    let p = palette(s);
    (p.accent, p.accent_faint)
}

/// Main text colour of the active theme (for recolouring SVG artwork).
pub fn text_color(s: &Settings) -> &'static str {
    palette(s).text
}

fn palette(s: &Settings) -> &'static Palette {
    match s.theme {
        Theme::Sepia => &SEPIA,
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
        Theme::Auto => {
            if adw::StyleManager::default().is_dark() {
                &DARK
            } else {
                &LIGHT
            }
        }
    }
}

pub fn apply(s: &Settings) {
    let sm = adw::StyleManager::default();
    sm.set_color_scheme(match s.theme {
        Theme::Auto => adw::ColorScheme::Default,
        Theme::Dark => adw::ColorScheme::ForceDark,
        Theme::Light | Theme::Sepia => adw::ColorScheme::ForceLight,
    });
    let p = match s.theme {
        Theme::Sepia => &SEPIA,
        Theme::Dark => &DARK,
        Theme::Light => &LIGHT,
        Theme::Auto => {
            if sm.is_dark() {
                &DARK
            } else {
                &LIGHT
            }
        }
    };
    let quran_px = s.quran_font_px();
    let tr_px = s.translation_font_px();
    let wbw_px = (tr_px * 0.8).max(11.0);
    let shade = if p.dark {
        "rgba(0,0,0,0.36)"
    } else {
        "rgba(0,0,0,0.07)"
    };
    let css = format!(
        ":root {{
  --q-bg: {bg}; --q-elevated: {elevated}; --q-alt: {alt}; --q-alt-medium: {alt_medium};
  --q-text: {text}; --q-faded: {faded};
  --q-accent: {accent}; --q-accent-deep: {accent_deep}; --q-accent-faded: {accent_faded}; --q-accent-faint: {accent_faint};
  --q-hairline: {hairline}; --q-search-bg: {search_bg}; --q-word-hover: {word_hover};
  --q-paper: {paper}; --q-ink: {ink}; --q-frame: {frame};
  --q-quran-size: {quran_px}px; --q-translation-size: {tr_px}px; --q-wbw-size: {wbw_px}px;

  --window-bg-color: {bg}; --window-fg-color: {text};
  --view-bg-color: {bg}; --view-fg-color: {text};
  --headerbar-bg-color: {bg}; --headerbar-fg-color: {text}; --headerbar-backdrop-color: {bg};
  --headerbar-shade-color: {shade}; --headerbar-border-color: {hairline};
  --sidebar-bg-color: {elevated}; --sidebar-fg-color: {text}; --sidebar-backdrop-color: {elevated};
  --sidebar-shade-color: {shade}; --sidebar-border-color: {hairline};
  --secondary-sidebar-bg-color: {elevated}; --secondary-sidebar-fg-color: {text};
  --secondary-sidebar-backdrop-color: {elevated};
  --card-bg-color: {elevated}; --card-fg-color: {text}; --card-shade-color: {shade};
  --dialog-bg-color: {elevated}; --dialog-fg-color: {text};
  --popover-bg-color: {elevated}; --popover-fg-color: {text}; --popover-shade-color: {shade};
  --thumbnail-bg-color: {elevated}; --thumbnail-fg-color: {text};
  --accent-bg-color: {accent}; --accent-fg-color: #ffffff; --accent-color: {accent_deep};
  --border-color: {hairline};
}}
.quran-text, .arabic-word {{ font-size: {quran_px}px; }}
.translation-text {{ font-size: {tr_px}px; }}
.wbw {{ font-size: {wbw_px}px; }}
",
        bg = p.bg,
        elevated = p.elevated,
        alt = p.alt,
        alt_medium = p.alt_medium,
        text = p.text,
        faded = p.faded,
        accent = p.accent,
        accent_deep = p.accent_deep,
        accent_faded = p.accent_faded,
        accent_faint = p.accent_faint,
        hairline = p.hairline,
        search_bg = p.search_bg,
        word_hover = p.word_hover,
        paper = p.paper,
        ink = p.ink,
        frame = p.frame,
    );
    VARS.with(|v| {
        if let Some(provider) = v.get() {
            provider.load_from_string(&css);
        }
    });
}
