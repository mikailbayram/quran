use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Auto,
    Light,
    Sepia,
    Dark,
}

/// Script used for the Arabic text, mirroring quran.com's "Quran font" setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuranFont {
    /// King Fahd Complex (QPC) V2 per-page glyph fonts; pixel-identical to the printed Madani Mushaf.
    #[default]
    QpcV2,
    /// King Fahd Complex V1 glyphs: the older 1405 AH print, same page layout.
    QpcV1,
    /// KFGQPC Uthmanic Hafs Unicode text.
    Uthmani,
    /// IndoPak Nastaleeq.
    IndoPak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReadingMode {
    /// Verse by verse with translations.
    #[default]
    Translation,
    /// Mushaf page layout.
    Reading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepeatMode {
    #[default]
    Off,
    Verse,
    Chapter,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub quran_font: QuranFont,
    pub reading_mode: ReadingMode,
    /// 1..=10 like quran.com.
    pub quran_font_scale: u8,
    pub translation_font_scale: u8,
    pub translations: Vec<u32>,
    pub tafsir: u32,
    pub reciter: u32,
    pub wbw_translation: bool,
    pub wbw_transliteration: bool,
    /// Show word-by-word inline under each word instead of on hover.
    pub wbw_inline: bool,
    pub wbw_language: String,
    pub playback_rate: f64,
    pub repeat: RepeatMode,
    pub auto_scroll: bool,
    pub word_highlight: bool,
    /// Mushaf reading view word gap adjustment, -10..=10 (0.02em per step).
    pub mushaf_word_spacing: i8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::Auto,
            quran_font: QuranFont::QpcV2,
            reading_mode: ReadingMode::Translation,
            quran_font_scale: 3,
            translation_font_scale: 3,
            // Saheeh International
            translations: vec![20],
            tafsir: 169,
            // Mishari Rashid al-Afasy
            reciter: 7,
            wbw_translation: true,
            wbw_transliteration: false,
            wbw_inline: false,
            wbw_language: "en".into(),
            playback_rate: 1.0,
            repeat: RepeatMode::Off,
            auto_scroll: true,
            word_highlight: true,
            mushaf_word_spacing: -2,
        }
    }
}

impl QuranFont {
    /// The per-page glyph font needed for `page`, for the QPC scripts.
    pub fn page_font(self, page: u32) -> Option<crate::FontFile> {
        match self {
            QuranFont::QpcV2 => Some(crate::FontFile::QpcV2Page(page)),
            QuranFont::QpcV1 => Some(crate::FontFile::QpcV1Page(page)),
            _ => None,
        }
    }
}

impl Settings {
    pub fn quran_font_px(&self) -> f64 {
        // quran.com scale 1..10 maps to roughly 24px..64px
        20.0 + self.quran_font_scale.clamp(1, 10) as f64 * 4.4
    }
    pub fn translation_font_px(&self) -> f64 {
        12.0 + self.translation_font_scale.clamp(1, 10) as f64 * 1.6
    }
}
