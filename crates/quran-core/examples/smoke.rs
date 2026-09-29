use quran_core::*;
use std::sync::Arc;

#[tokio::main]
async fn main() {
    let dir = std::env::temp_dir().join("quran-smoke");
    std::fs::create_dir_all(&dir).unwrap();
    let c = Client::new(Arc::new(Store::open(&dir.join("db.sqlite3")).unwrap()), dir);
    let q = VerseQuery {
        translations: vec![131, 20],
        wbw_language: "en".into(),
    };
    let t = std::time::Instant::now();
    let ch = c.chapters("en").await.unwrap();
    println!("chapters {} in {:?}", ch.len(), t.elapsed());
    let t = std::time::Instant::now();
    let v = c.all_verses_by_chapter(2, &q).await.unwrap();
    println!(
        "baqarah {} verses in {:?}; tr={:?}",
        v.len(),
        t.elapsed(),
        v[0].translations
            .iter()
            .map(|t| (t.resource_id, t.resource_name.clone()))
            .collect::<Vec<_>>()
    );
    println!(
        "{}",
        text::html_to_pango(&v[0].translations[0].text, Default::default())
    );
    let p = c.verses_by_page(3, &q).await.unwrap();
    println!("page 3: {} verses", p.len());
    let r = c.reciters().await.unwrap();
    println!("reciters {}", r.len());
    let a = c.chapter_audio(7, 1).await.unwrap();
    println!(
        "audio {} timings={} word@7000={:?}",
        a.audio_url,
        a.verse_timings.len(),
        a.verse_timings[a.timing_index_at(7000).unwrap()].word_at(7000)
    );
    println!(
        "translations {} tafsirs {} juzs {}",
        c.translations().await.unwrap().len(),
        c.tafsirs().await.unwrap().len(),
        c.juzs().await.unwrap().len()
    );
    let s = c.search("patience", 1, "en").await.unwrap();
    println!(
        "search {} {:?}",
        s.total_results,
        s.results.first().map(|r| &r.verse_key)
    );
    println!("tafsir {}", &c.tafsir(169, "1:1").await.unwrap().text[..80]);
    println!(
        "info {}",
        &c.chapter_info(1, "en").await.unwrap().short_text
    );
    println!("langs {}", c.languages().await.unwrap().len());
}
