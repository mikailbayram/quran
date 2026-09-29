use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TranslatedName {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub language_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: u32,
    pub revelation_place: String,
    pub revelation_order: u32,
    pub bismillah_pre: bool,
    pub name_simple: String,
    pub name_complex: String,
    pub name_arabic: String,
    pub verses_count: u32,
    pub pages: Vec<u32>,
    pub translated_name: TranslatedName,
}

impl Chapter {
    pub fn first_page(&self) -> u32 {
        self.pages.first().copied().unwrap_or(1)
    }
    pub fn last_page(&self) -> u32 {
        self.pages.last().copied().unwrap_or(1)
    }
    pub fn is_makki(&self) -> bool {
        self.revelation_place.eq_ignore_ascii_case("makkah")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub chapter_id: u32,
    #[serde(default)]
    pub short_text: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Juz {
    pub juz_number: u32,
    /// chapter id -> "from-to"
    pub verse_mapping: BTreeMap<String, String>,
    pub verses_count: u32,
}

impl Juz {
    /// Chapters in order with their verse range.
    pub fn ranges(&self) -> Vec<(u32, u32, u32)> {
        let mut out: Vec<(u32, u32, u32)> = self
            .verse_mapping
            .iter()
            .filter_map(|(ch, range)| {
                let ch = ch.parse().ok()?;
                let (a, b) = range.split_once('-')?;
                Some((ch, a.parse().ok()?, b.parse().ok()?))
            })
            .collect();
        out.sort_by_key(|r| r.0);
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordText {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub language_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub id: u64,
    pub position: u32,
    #[serde(default)]
    pub audio_url: Option<String>,
    pub char_type_name: String,
    #[serde(default)]
    pub text_uthmani: Option<String>,
    #[serde(default)]
    pub text_indopak: Option<String>,
    #[serde(default)]
    pub code_v2: Option<String>,
    #[serde(default)]
    pub code_v1: Option<String>,
    pub page_number: u32,
    #[serde(default)]
    pub line_number: u32,
    #[serde(default)]
    pub translation: Option<WordText>,
    #[serde(default)]
    pub transliteration: Option<WordText>,
}

impl Word {
    pub fn is_end(&self) -> bool {
        self.char_type_name == "end"
    }
    pub fn translation_text(&self) -> Option<&str> {
        self.translation.as_ref()?.text.as_deref()
    }
    pub fn transliteration_text(&self) -> Option<&str> {
        self.transliteration.as_ref()?.text.as_deref()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerseTranslation {
    pub resource_id: u32,
    pub text: String,
    #[serde(default)]
    pub resource_name: Option<String>,
    #[serde(default)]
    pub language_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verse {
    pub id: u32,
    pub verse_number: u32,
    pub verse_key: String,
    pub juz_number: u32,
    pub hizb_number: u32,
    pub page_number: u32,
    #[serde(default)]
    pub sajdah_number: Option<u32>,
    #[serde(default)]
    pub text_uthmani: Option<String>,
    #[serde(default)]
    pub words: Vec<Word>,
    #[serde(default)]
    pub translations: Vec<VerseTranslation>,
}

impl Verse {
    pub fn chapter_id(&self) -> u32 {
        self.verse_key
            .split(':')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pagination {
    pub per_page: u32,
    pub current_page: u32,
    #[serde(default)]
    pub next_page: Option<u32>,
    pub total_pages: u32,
    pub total_records: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersesPage {
    pub verses: Vec<Verse>,
    pub pagination: Pagination,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationResource {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub author_name: String,
    #[serde(default)]
    pub slug: Option<String>,
    pub language_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TafsirResource {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub author_name: String,
    pub language_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tafsir {
    #[serde(default)]
    pub resource_name: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Language {
    pub iso_code: String,
    pub name: String,
    #[serde(default)]
    pub native_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamedField {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reciter {
    pub id: u32,
    pub name: String,
    #[serde(default)]
    pub translated_name: Option<TranslatedName>,
    #[serde(default)]
    pub style: Option<NamedField>,
    #[serde(default)]
    pub qirat: Option<NamedField>,
}

impl Reciter {
    pub fn display_name(&self) -> &str {
        self.translated_name
            .as_ref()
            .map(|t| t.name.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or(&self.name)
    }
    pub fn style_name(&self) -> &str {
        self.style.as_ref().map(|s| s.name.as_str()).unwrap_or("")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerseTiming {
    pub verse_key: String,
    pub timestamp_from: u64,
    pub timestamp_to: u64,
    /// `[word_position, from_ms, to_ms]`; malformed entries have fewer items.
    #[serde(default)]
    pub segments: Vec<Vec<f64>>,
}

impl VerseTiming {
    pub fn verse_number(&self) -> u32 {
        self.verse_key
            .split(':')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }

    /// Word position (1-based) being recited at `ms`.
    pub fn word_at(&self, ms: u64) -> Option<u32> {
        let ms = ms as f64;
        self.segments
            .iter()
            .filter(|s| s.len() >= 3)
            .find(|s| ms >= s[1] && ms < s[2])
            .map(|s| s[0] as u32)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChapterAudio {
    pub chapter_id: u32,
    pub audio_url: String,
    #[serde(default)]
    pub duration: u64,
    #[serde(default)]
    pub file_size: f64,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub verse_timings: Vec<VerseTiming>,
}

impl ChapterAudio {
    /// Index into `verse_timings` for the given position.
    pub fn timing_index_at(&self, ms: u64) -> Option<usize> {
        let idx = self
            .verse_timings
            .partition_point(|t| t.timestamp_from <= ms);
        if idx == 0 {
            return if self.verse_timings.is_empty() {
                None
            } else {
                Some(0)
            };
        }
        Some(idx - 1)
    }

    pub fn timing_for_verse(&self, verse: u32) -> Option<&VerseTiming> {
        self.verse_timings
            .iter()
            .find(|t| t.verse_number() == verse)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchTranslation {
    pub text: String,
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub verse_key: String,
    pub text: String,
    #[serde(default)]
    pub translations: Vec<SearchTranslation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub query: String,
    pub total_results: u32,
    pub current_page: u32,
    pub total_pages: u32,
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Bookmark {
    pub verse_key: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LastRead {
    pub chapter_id: u32,
    pub verse_number: u32,
}

pub fn parse_verse_key(key: &str) -> Option<(u32, u32)> {
    let (a, b) = key.split_once(':')?;
    Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
}
