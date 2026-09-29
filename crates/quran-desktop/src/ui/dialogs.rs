//! Tafsir and Surah Info dialogs.

use super::common::*;
use crate::ctx::ctx;
use crate::runtime;
use adw::prelude::*;
use quran_core::text::{HtmlOptions, html_to_pango};
use std::rc::Rc;

fn reading_dialog(title: &str) -> (adw::Dialog, adw::HeaderBar, gtk::Box) {
    let dialog = adw::Dialog::new();
    dialog.set_title(title);
    dialog.set_content_width(760);
    dialog.set_content_height(760);
    let tv = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    tv.add_top_bar(&header);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 16);
    body.set_margin_start(24);
    body.set_margin_end(24);
    body.set_margin_top(12);
    body.set_margin_bottom(24);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(720);
    clamp.set_child(Some(&body));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&clamp));
    scroller.set_vexpand(true);
    tv.set_content(Some(&scroller));
    dialog.set_child(Some(&tv));
    (dialog, header, body)
}

fn loading() -> adw::Spinner {
    let s = adw::Spinner::new();
    s.set_size_request(32, 32);
    s.set_margin_top(80);
    s
}

pub fn show_tafsir(verse_key: &str) {
    let (dialog, header, body) = reading_dialog(&format!("Tafsir · {verse_key}"));
    let picker = gtk::DropDown::from_strings(&[]);
    picker.set_enable_search(true);
    header.pack_start(&picker);

    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.append(&text_box);
    let key = verse_key.to_string();
    let load: Rc<dyn Fn(u32)> = {
        let text_box = text_box.clone();
        Rc::new(move |id: u32| {
            clear_box(&text_box);
            text_box.append(&loading());
            let client = ctx().client.clone();
            let key = key.clone();
            let text_box = text_box.clone();
            runtime::spawn(async move { client.tafsir(id, &key).await }, move |res| {
                clear_box(&text_box);
                match res {
                    Ok(t) => {
                        let rtl = ctx()
                            .tafsirs
                            .borrow()
                            .as_ref()
                            .and_then(|l| {
                                l.iter()
                                    .find(|x| x.id == id)
                                    .map(|x| is_rtl_language(&x.language_name))
                            })
                            .unwrap_or(false);
                        let l = gtk::Label::new(None);
                        let markup = html_to_pango(
                            &t.text,
                            HtmlOptions {
                                blocks: true,
                                keep_footnotes: true,
                            },
                        );
                        if markup.trim().is_empty() {
                            l.set_text("This tafsir has no commentary for this verse. It may be covered in a nearby verse.");
                            l.add_css_class("faded");
                        } else {
                            l.set_markup(&markup);
                        }
                        l.set_wrap(true);
                        l.set_selectable(true);
                        l.set_xalign(if rtl { 1.0 } else { 0.0 });
                        l.add_css_class("tafsir-text");
                        if rtl {
                            l.add_css_class("rtl");
                            l.set_direction(gtk::TextDirection::Rtl);
                        }
                        text_box.append(&l);
                    }
                    Err(e) => {
                        text_box.append(&label(
                            "Couldn't load this tafsir. Check your internet connection and try again.",
                            &["faded"],
                        ));
                        eprintln!("tafsir: {e}");
                    }
                }
            });
        })
    };

    let current = ctx().with_settings(|s| s.tafsir);
    let lp = load.clone();
    let p = picker.clone();
    ctx().with_tafsirs(move |list| {
        let names: Vec<String> = list
            .iter()
            .map(|t| {
                format!(
                    "{} ({})",
                    t.name,
                    super::settings::capitalize(&t.language_name)
                )
            })
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        p.set_model(Some(&gtk::StringList::new(&names)));
        let idx = list.iter().position(|t| t.id == current).unwrap_or(0);
        p.set_selected(idx as u32);
        let ids: Vec<u32> = list.iter().map(|t| t.id).collect();
        lp(ids.get(idx).copied().unwrap_or(current));
        p.connect_selected_notify(move |d| {
            if let Some(id) = ids.get(d.selected() as usize) {
                lp(*id);
            }
        });
    });
    dialog.present(Some(ctx().window()));
}

pub fn show_chapter_info(chapter: u32) {
    let Some(ch) = ctx().chapter(chapter) else {
        return;
    };
    let (dialog, _header, body) = reading_dialog(&format!("Surah {}", ch.name_simple));
    let head = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let glyph = gtk::Label::new(Some(&surah_glyph(ch.id)));
    glyph.add_css_class("chapter-glyph");
    head.append(&glyph);
    let facts = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    facts.set_halign(gtk::Align::Center);
    for (k, v) in [
        ("Ayahs", ch.verses_count.to_string()),
        (
            "Revelation",
            if ch.is_makki() {
                "Mecca".into()
            } else {
                "Medina".into()
            },
        ),
        ("Order", ch.revelation_order.to_string()),
        ("Pages", format!("{}–{}", ch.first_page(), ch.last_page())),
    ] {
        let b = gtk::Box::new(gtk::Orientation::Vertical, 2);
        b.append(&label(k, &["card-subtitle"]));
        b.append(&label(&v, &["card-title"]));
        facts.append(&b);
    }
    head.append(&facts);
    body.append(&head);
    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text_box.append(&loading());
    body.append(&text_box);
    let client = ctx().client.clone();
    runtime::spawn(
        async move { client.chapter_info(chapter, "en").await },
        move |res| {
            clear_box(&text_box);
            match res {
                Ok(info) => {
                    let l = gtk::Label::new(None);
                    l.set_markup(&html_to_pango(
                        &info.text,
                        HtmlOptions {
                            blocks: true,
                            keep_footnotes: true,
                        },
                    ));
                    l.set_wrap(true);
                    l.set_selectable(true);
                    l.set_xalign(0.0);
                    l.add_css_class("tafsir-text");
                    text_box.append(&l);
                    if !info.source.is_empty() {
                        let src = label(&format!("Source: {}", info.source), &["translator"]);
                        src.set_margin_top(16);
                        src.set_wrap(true);
                        text_box.append(&src);
                    }
                }
                Err(e) => {
                    text_box.append(&label(
                    "Couldn't load the surah info. Check your internet connection and try again.",
                    &["faded"],
                ));
                    eprintln!("chapter info: {e}");
                }
            }
        },
    );
    dialog.present(Some(ctx().window()));
}
