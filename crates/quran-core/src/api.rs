//! Quran.com / Quran Foundation API client with a cache-first SQLite layer.
//!
//! Quran text, translations and timings never change, so every successful
//! response is stored forever and served locally afterwards.

use crate::models::*;
use crate::store::Store;
use futures::StreamExt;
use serde::de::DeserializeOwned;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

pub const API_V4: &str = "https://api.quran.com/api/v4";
pub const API_QDC: &str = "https://api.qurancdn.com/api/qdc";
pub const WBW_AUDIO: &str = "https://audio.qurancdn.com/";
const FONT_BASE: &str =
    "https://raw.githubusercontent.com/quran/quran.com-frontend-next/master/public/fonts";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("could not read response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error("the server answered {0}")]
    Status(u16),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Options that shape which verse fields are fetched.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VerseQuery {
    pub translations: Vec<u32>,
    pub wbw_language: String,
}

impl VerseQuery {
    fn params(&self) -> String {
        let tr = self
            .translations
            .iter()
            .map(|t| t.to_string())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "words=true&translations={tr}&translation_fields=resource_name,language_name\
             &word_fields=text_uthmani,text_indopak,code_v2,code_v1\
             &fields=text_uthmani&language={}",
            self.wbw_language
        )
    }
}

#[derive(Clone)]
pub struct Client {
    http: reqwest::Client,
    store: Arc<Store>,
    data_dir: PathBuf,
}

#[derive(serde::Deserialize)]
struct ChaptersResp {
    chapters: Vec<Chapter>,
}
#[derive(serde::Deserialize)]
struct ChapterInfoResp {
    chapter_info: ChapterInfo,
}
#[derive(serde::Deserialize)]
struct JuzsResp {
    juzs: Vec<Juz>,
}
#[derive(serde::Deserialize)]
struct TranslationsResp {
    translations: Vec<TranslationResource>,
}
#[derive(serde::Deserialize)]
struct TafsirsResp {
    tafsirs: Vec<TafsirResource>,
}
#[derive(serde::Deserialize)]
struct TafsirResp {
    tafsir: Tafsir,
}
#[derive(serde::Deserialize)]
struct RecitersResp {
    reciters: Vec<Reciter>,
}
#[derive(serde::Deserialize)]
struct AudioFilesResp {
    audio_files: Vec<ChapterAudio>,
}
#[derive(serde::Deserialize)]
struct LanguagesResp {
    languages: Vec<Language>,
}
#[derive(serde::Deserialize)]
struct SearchResp {
    search: SearchResponse,
}

impl Client {
    pub fn new(store: Arc<Store>, data_dir: PathBuf) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("quran-desktop/", env!("CARGO_PKG_VERSION")))
            .pool_max_idle_per_host(8)
            .build()
            .expect("http client");
        Self {
            http,
            store,
            data_dir,
        }
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    async fn get_bytes(&self, url: &str) -> Result<Vec<u8>> {
        if let Some(b) = self.store.cache_get(url) {
            return Ok(b);
        }
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let body = resp.bytes().await?.to_vec();
        self.store.cache_put(url, &body);
        Ok(body)
    }

    async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
        let bytes = self.get_bytes(url).await?;
        match serde_json::from_slice(&bytes) {
            Ok(v) => Ok(v),
            Err(e) => {
                // A bad cached body should not poison the cache forever.
                self.store.cache_delete(url);
                Err(e.into())
            }
        }
    }

    pub async fn chapters(&self, lang: &str) -> Result<Vec<Chapter>> {
        let r: ChaptersResp = self
            .get_json(&format!("{API_V4}/chapters?language={lang}"))
            .await?;
        Ok(r.chapters)
    }

    pub async fn chapter_info(&self, id: u32, lang: &str) -> Result<ChapterInfo> {
        let r: ChapterInfoResp = self
            .get_json(&format!("{API_V4}/chapters/{id}/info?language={lang}"))
            .await?;
        Ok(r.chapter_info)
    }

    pub async fn juzs(&self) -> Result<Vec<Juz>> {
        let r: JuzsResp = self.get_json(&format!("{API_V4}/juzs")).await?;
        let mut seen = std::collections::BTreeMap::new();
        for j in r.juzs {
            seen.entry(j.juz_number).or_insert(j);
        }
        Ok(seen.into_values().collect())
    }

    /// Parse a response straight from the local cache, without touching the
    /// network or the async runtime. Lets the UI paint cached text in the same
    /// frame it was asked for.
    fn cached_json<T: DeserializeOwned>(&self, url: &str) -> Option<T> {
        let bytes = self.store.cache_get(url)?;
        serde_json::from_slice(&bytes).ok()
    }

    fn verses_by_chapter_url(chapter: u32, page: u32, q: &VerseQuery) -> String {
        format!(
            "{API_V4}/verses/by_chapter/{chapter}?{}&per_page=50&page={page}",
            q.params()
        )
    }

    fn verses_by_page_url(mushaf_page: u32, q: &VerseQuery) -> String {
        format!(
            "{API_V4}/verses/by_page/{mushaf_page}?{}&per_page=50",
            q.params()
        )
    }

    pub async fn verses_by_chapter(
        &self,
        chapter: u32,
        page: u32,
        q: &VerseQuery,
    ) -> Result<VersesPage> {
        self.get_json(&Self::verses_by_chapter_url(chapter, page, q))
            .await
    }

    pub fn cached_verses_by_chapter(
        &self,
        chapter: u32,
        page: u32,
        q: &VerseQuery,
    ) -> Option<VersesPage> {
        self.cached_json(&Self::verses_by_chapter_url(chapter, page, q))
    }

    pub fn cached_verses_by_page(&self, mushaf_page: u32, q: &VerseQuery) -> Option<Vec<Verse>> {
        self.cached_json::<VersesPage>(&Self::verses_by_page_url(mushaf_page, q))
            .map(|r| r.verses)
    }

    /// All verses of a chapter, fetching the remaining 50-verse pages concurrently.
    pub async fn all_verses_by_chapter(&self, chapter: u32, q: &VerseQuery) -> Result<Vec<Verse>> {
        let first = self.verses_by_chapter(chapter, 1, q).await?;
        let total = first.pagination.total_pages;
        let mut verses = first.verses;
        let rest = futures::stream::iter(2..=total)
            .map(|p| {
                let q = q.clone();
                async move { self.verses_by_chapter(chapter, p, &q).await }
            })
            .buffered(6)
            .collect::<Vec<_>>()
            .await;
        for page in rest {
            verses.extend(page?.verses);
        }
        Ok(verses)
    }

    pub async fn verses_by_page(&self, mushaf_page: u32, q: &VerseQuery) -> Result<Vec<Verse>> {
        let r: VersesPage = self
            .get_json(&Self::verses_by_page_url(mushaf_page, q))
            .await?;
        Ok(r.verses)
    }

    pub async fn verse(&self, key: &str, q: &VerseQuery) -> Result<Verse> {
        #[derive(serde::Deserialize)]
        struct R {
            verse: Verse,
        }
        let r: R = self
            .get_json(&format!("{API_V4}/verses/by_key/{key}?{}", q.params()))
            .await?;
        Ok(r.verse)
    }

    pub async fn translations(&self) -> Result<Vec<TranslationResource>> {
        let r: TranslationsResp = self
            .get_json(&format!("{API_V4}/resources/translations"))
            .await?;
        Ok(r.translations)
    }

    pub async fn tafsirs(&self) -> Result<Vec<TafsirResource>> {
        let r: TafsirsResp = self
            .get_json(&format!("{API_V4}/resources/tafsirs"))
            .await?;
        Ok(r.tafsirs)
    }

    pub async fn tafsir(&self, tafsir: u32, key: &str) -> Result<Tafsir> {
        let r: TafsirResp = self
            .get_json(&format!("{API_V4}/tafsirs/{tafsir}/by_ayah/{key}"))
            .await?;
        Ok(r.tafsir)
    }

    pub async fn languages(&self) -> Result<Vec<Language>> {
        let r: LanguagesResp = self
            .get_json(&format!("{API_V4}/resources/languages"))
            .await?;
        Ok(r.languages)
    }

    pub async fn reciters(&self) -> Result<Vec<Reciter>> {
        let r: RecitersResp = self
            .get_json(&format!("{API_QDC}/audio/reciters?locale=en"))
            .await?;
        Ok(r.reciters)
    }

    pub async fn chapter_audio(&self, reciter: u32, chapter: u32) -> Result<ChapterAudio> {
        let r: AudioFilesResp = self
            .get_json(&format!(
                "{API_QDC}/audio/reciters/{reciter}/audio_files?chapter={chapter}&segments=true"
            ))
            .await?;
        r.audio_files.into_iter().next().ok_or(Error::Status(404))
    }

    /// Search is never cached as final results may change and queries are unbounded.
    pub async fn search(&self, query: &str, page: u32, lang: &str) -> Result<SearchResponse> {
        let url = reqwest::Url::parse_with_params(
            &format!("{API_V4}/search"),
            &[
                ("q", query),
                ("size", "20"),
                ("page", &page.to_string()),
                ("language", lang),
            ],
        )
        .expect("valid url");
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let r: SearchResp = resp.json().await?;
        Ok(r.search)
    }

    // ---------- files (fonts, audio) ----------

    pub fn fonts_dir(&self) -> PathBuf {
        self.data_dir.join("fonts")
    }

    pub fn audio_dir(&self) -> PathBuf {
        self.data_dir.join("audio")
    }

    /// Local path of a chapter recitation if it has been downloaded.
    pub fn local_audio_path(&self, reciter: u32, chapter: u32) -> PathBuf {
        self.audio_dir()
            .join(reciter.to_string())
            .join(format!("{chapter:03}.mp3"))
    }

    /// Download `url` to `dest` atomically; returns immediately if it already exists.
    pub async fn download_to(&self, url: &str, dest: &Path) -> Result<PathBuf> {
        if dest.exists() {
            return Ok(dest.to_path_buf());
        }
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let resp = self.http.get(url).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let tmp = dest.with_extension("part");
        let mut file = tokio::fs::File::create(&tmp).await?;
        let mut stream = resp.bytes_stream();
        while let Some(chunk) = stream.next().await {
            file.write_all(&chunk?).await?;
        }
        file.flush().await?;
        drop(file);
        tokio::fs::rename(&tmp, dest).await?;
        Ok(dest.to_path_buf())
    }

    pub async fn download_chapter_audio(&self, reciter: u32, chapter: u32) -> Result<PathBuf> {
        let audio = self.chapter_audio(reciter, chapter).await?;
        self.download_to(&audio.audio_url, &self.local_audio_path(reciter, chapter))
            .await
    }

    /// Fetch (once) a Quran font file and return its local path.
    pub async fn font(&self, font: FontFile) -> Result<PathBuf> {
        let dest = self.fonts_dir().join(font.file_name());
        if !font.url().ends_with(".woff2") {
            return self.download_to(&font.url(), &dest).await;
        }
        if dest.exists() {
            return Ok(dest);
        }
        // WOFF2 is ~15x smaller over the wire; Pango only loads plain TrueType, so unpack it here.
        let resp = self.http.get(font.url()).send().await?;
        if !resp.status().is_success() {
            return Err(Error::Status(resp.status().as_u16()));
        }
        let packed = resp.bytes().await?;
        let ttf = wuff::decompress_woff2(&packed)
            .map_err(|e| Error::Io(std::io::Error::other(format!("{e:?}"))))?;
        tokio::fs::create_dir_all(self.fonts_dir()).await?;
        let tmp = dest.with_extension("part");
        tokio::fs::write(&tmp, ttf).await?;
        tokio::fs::rename(&tmp, &dest).await?;
        Ok(dest)
    }
}

/// Font files used by the reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontFile {
    /// QPC V2 glyph font for a Mushaf page (1..=604).
    QpcV2Page(u32),
    /// QPC V1 (1405 AH print) glyph font for a Mushaf page.
    QpcV1Page(u32),
    UthmanicHafs,
    IndoPak,
    SurahNames,
}

impl FontFile {
    pub fn file_name(&self) -> String {
        match self {
            FontFile::QpcV2Page(p) => format!("qpc-v2-p{p}.ttf"),
            FontFile::QpcV1Page(p) => format!("qpc-v1-p{p}.ttf"),
            FontFile::UthmanicHafs => "UthmanicHafs1Ver18.ttf".into(),
            FontFile::IndoPak => "indopak-nastaleeq-waqf-lazim-v4.2.1.ttf".into(),
            FontFile::SurahNames => "sura_names.ttf".into(),
        }
    }

    pub fn url(&self) -> String {
        match self {
            FontFile::QpcV2Page(p) => format!("{FONT_BASE}/quran/hafs/v2/woff2/p{p}.woff2"),
            FontFile::QpcV1Page(p) => format!("{FONT_BASE}/quran/hafs/v1/woff2/p{p}.woff2"),
            FontFile::UthmanicHafs => {
                format!("{FONT_BASE}/quran/hafs/uthmanic_hafs/UthmanicHafs1Ver18.ttf")
            }
            FontFile::IndoPak => format!(
                "{FONT_BASE}/quran/hafs/nastaleeq/indopak/indopak-nastaleeq-waqf-lazim-v4.2.1.ttf"
            ),
            FontFile::SurahNames => format!("{FONT_BASE}/quran/surah-names/v1/sura_names.ttf"),
        }
    }

    /// Font family name embedded in the file.
    pub fn family(&self) -> String {
        match self {
            FontFile::QpcV2Page(p) => format!("QCF2{p:03}"),
            FontFile::QpcV1Page(p) => format!("QCF_P{p:03}"),
            FontFile::UthmanicHafs => "KFGQPC HAFS Uthmanic Script".into(),
            FontFile::IndoPak => "AlQuran IndoPak by QuranWBW".into(),
            FontFile::SurahNames => "sura_names".into(),
        }
    }
}

pub fn wbw_audio_url(path: &str) -> String {
    if path.starts_with("http") {
        path.to_string()
    } else {
        format!("{WBW_AUDIO}{path}")
    }
}
