# Quran

A fast, native Quran app for the Linux desktop, modelled on [quran.com](https://quran.com).
Written in Rust with GTK4, libadwaita and GStreamer.

## Features

- **Reading view** laid out like the printed Madani Mushaf: 15 justified lines per
  page, surah banners and Bismillah, using the King Fahd Complex page fonts
  (V1 — 1405 AH print, or V2 — 1421 AH print). Uthmanic Hafs and IndoPak scripts too.
- **Translation view**, verse by verse, with any number of translations and
  word-by-word translation/transliteration (on hover or inline).
- **Recitations** from many reciters with verse and word highlighting that follows
  the audio, repeat (verse/surah), playback speed, offline download, and media
  keys / desktop controls via MPRIS.
- **Tafsir**, surah info, full-text search, bookmarks and "continue reading".
- **Themes**: light, sepia, dark, or follow the system.
- **Offline cache**: every surah, translation and timing you open is stored locally
  and loads instantly afterwards.

## Build and run

Requirements (Arch Linux package names):

```sh
sudo pacman -S --needed rust gtk4 libadwaita gstreamer gst-plugins-base gst-plugins-good
```

Then:

```sh
cargo build --release
./target/release/quran            # open the home screen
./target/release/quran 2:255      # jump straight to a verse
./target/release/quran 18         # or a surah
```

The first time a surah is opened it needs an internet connection; after that it
works offline. Recitations stream unless downloaded from the player bar.

### Keyboard shortcuts

| Keys | Action |
| --- | --- |
| `Ctrl+K` / `Ctrl+F` | Search |
| `Ctrl+,` | Settings |
| `Ctrl+R` | Switch Translation / Reading |
| `Space` | Play / pause |
| `Alt+←` / `Alt+→` | Previous / next verse |
| `Ctrl++` / `Ctrl+-` | Bigger / smaller text |
| `Alt+Home` | Back to home |

## Project layout

```
crates/
  quran-core/     platform-independent core: API client, SQLite cache,
                  models, settings, bookmarks (reusable for mobile front ends)
  quran-desktop/  GTK4/libadwaita front end
    src/ui/       home, reader (translation + Mushaf views), player bar,
                  settings drawer, search, dialogs
    resources/    bundled fonts and artwork
```

Data and settings live in `~/.local/share/quran-desktop/` (SQLite database,
downloaded page fonts and recitations).

## Data sources

- Quran text, translations, tafsir, word-by-word data:
  [quran.com API](https://api.quran.com/api/v4) (Quran Foundation)
- Recitations and verse/word timings: quran.com audio API
  (files served by [QuranicAudio](https://quranicaudio.com))
- Fonts and Bismillah artwork: King Fahd Glorious Quran Printing Complex (QPC)
  fonts, IndoPak and surah-name fonts as distributed by
  [quran.com-frontend-next](https://github.com/quran/quran.com-frontend-next);
  UI font [Figtree](https://github.com/erikdkennedy/figtree) (OFL)

The app shows text as served by the API; it does not yet verify it against a
second source.

## Development

Opt-in debugging hooks (not used in normal runs):

- `QURAN_DEBUG_DIR=/tmp/qd` — accepts commands written to `/tmp/qd/cmd`
  (`open 2:255`, `mode reading`, `font v1`, `theme sepia`, `snap name`,
  `t open 2` to time an interaction, …). See `src/debug.rs`.
- `QURAN_PROFILE=1` — collects timing spans, printed by the `prof` debug command.
- `QURAN_LAYOUT_DEBUG=1` — logs Mushaf line widths and word gaps.

```sh
cargo test -p quran-core
```
