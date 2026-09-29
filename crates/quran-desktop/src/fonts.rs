//! Quran font management. Core fonts ship inside the binary; the 604 QPC V2
//! page fonts (~40 KB each as WOFF2) are fetched lazily and cached on disk.

use crate::runtime;
use pango::prelude::*;
use quran_core::{Client, FontFile};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

const BUNDLED: &[(&str, &[u8])] = &[
    (
        "UthmanicHafs1Ver18.ttf",
        include_bytes!("../resources/fonts/UthmanicHafs1Ver18.ttf"),
    ),
    (
        "indopak-nastaleeq-waqf-lazim-v4.2.1.ttf",
        include_bytes!("../resources/fonts/indopak-nastaleeq-waqf-lazim-v4.2.1.ttf"),
    ),
    (
        "sura_names.ttf",
        include_bytes!("../resources/fonts/sura_names.ttf"),
    ),
    (
        "Figtree.ttf",
        include_bytes!("../resources/fonts/Figtree.ttf"),
    ),
];

type Waiter = Box<dyn FnOnce()>;

pub struct Fonts {
    map: pango::FontMap,
    client: Client,
    loaded: RefCell<HashSet<FontFile>>,
    pending: RefCell<HashMap<FontFile, Vec<Waiter>>>,
}

impl Fonts {
    pub fn new(client: Client) -> Self {
        // GTK widgets use the default font map; custom maps aren't inherited by
        // every descendant (list rows), so fonts are registered globally.
        let map = pangocairo::FontMap::default();
        let dir = client.fonts_dir().join("bundled");
        let _ = std::fs::create_dir_all(&dir);
        for (name, bytes) in BUNDLED {
            let path = dir.join(name);
            let fresh = std::fs::metadata(&path)
                .map(|m| m.len() == bytes.len() as u64)
                .unwrap_or(false);
            if !fresh {
                let _ = std::fs::write(&path, bytes);
            }
            if let Err(e) = map.add_font_file(&path) {
                eprintln!("font {name}: {e}");
            }
        }
        let loaded = [
            FontFile::UthmanicHafs,
            FontFile::IndoPak,
            FontFile::SurahNames,
        ]
        .into_iter()
        .collect();
        Self {
            map,
            client,
            loaded: RefCell::new(loaded),
            pending: RefCell::new(HashMap::new()),
        }
    }

    pub fn is_loaded(&self, f: FontFile) -> bool {
        self.loaded.borrow().contains(&f)
    }

    fn add(&self, f: FontFile, path: &Path) {
        let _p = crate::prof::span("font.add");
        if let Err(e) = self.map.add_font_file(path) {
            eprintln!("font {}: {e}", path.display());
            // corrupt download; remove so the next attempt refetches
            let _ = std::fs::remove_file(path);
            return;
        }
        self.loaded.borrow_mut().insert(f);
    }

    /// Make sure `f` is usable, then run `ready` on the main thread.
    /// If already loaded, `ready` is not called (callers render immediately instead).
    pub fn ensure(&'static self, f: FontFile, ready: impl FnOnce() + 'static) {
        if self.is_loaded(f) {
            return;
        }
        let first = {
            let mut pending = self.pending.borrow_mut();
            let waiters = pending.entry(f).or_default();
            waiters.push(Box::new(ready));
            waiters.len() == 1
        };
        if !first {
            return;
        }
        // A previous run may have already downloaded it.
        let local = self.client.fonts_dir().join(f.file_name());
        if local.exists() {
            self.add(f, &local);
            self.flush(f);
            return;
        }
        let client = self.client.clone();
        runtime::spawn(async move { client.font(f).await }, move |res| {
            match res {
                Ok(path) => self.add(f, &path),
                Err(e) => eprintln!("font download {f:?}: {e}"),
            }
            self.flush(f);
        });
    }

    fn flush(&self, f: FontFile) {
        let waiters = self.pending.borrow_mut().remove(&f).unwrap_or_default();
        if self.is_loaded(f) {
            for w in waiters {
                w();
            }
        }
    }

    /// Warm up page fonts in the background (e.g. the pages of the chapter being opened).
    pub fn prefetch_pages(
        &'static self,
        font: quran_core::QuranFont,
        pages: impl IntoIterator<Item = u32>,
    ) {
        for p in pages {
            if let Some(f) = font.page_font(p) {
                self.ensure(f, || {});
            }
        }
    }
}
