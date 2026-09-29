//! Search dialog: instant navigation (surah names, 2:255, juz/page) plus
//! full-text search across the Quran and translations.

use super::common::*;
use crate::ctx::ctx;
use crate::runtime;
use adw::prelude::*;
use gtk::glib;
use quran_core::parse_verse_key;
use quran_core::text::{HtmlOptions, html_to_pango};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

struct State {
    token: Cell<u64>,
    debounce: RefCell<Option<glib::SourceId>>,
    page: Cell<u32>,
    total_pages: Cell<u32>,
    query: RefCell<String>,
}

pub fn show(initial: Option<&str>) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Search");
    dialog.set_content_width(720);
    dialog.set_content_height(720);
    let tv = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some(
        "Search the Quran — e.g. mercy, patience, 2:255, Yaseen",
    ));
    entry.set_hexpand(true);
    header.set_title_widget(Some(&entry));
    tv.add_top_bar(&header);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_start(14);
    content.set_margin_end(14);
    content.set_margin_top(10);
    content.set_margin_bottom(14);
    let quick = gtk::ListBox::new();
    quick.add_css_class("boxed-list");
    quick.set_selection_mode(gtk::SelectionMode::None);
    let status = label("", &["faded"]);
    let results = gtk::ListBox::new();
    results.add_css_class("boxed-list");
    results.set_selection_mode(gtk::SelectionMode::None);
    let more = gtk::Button::with_label("Load more results");
    more.add_css_class("pill-button");
    more.set_halign(gtk::Align::Center);
    more.set_visible(false);
    let spinner = adw::Spinner::new();
    spinner.set_visible(false);
    spinner.set_size_request(24, 24);
    content.append(&quick);
    content.append(&status);
    content.append(&spinner);
    content.append(&results);
    content.append(&more);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(760);
    clamp.set_child(Some(&content));
    scroller.set_child(Some(&clamp));
    scroller.set_vexpand(true);
    tv.set_content(Some(&scroller));
    dialog.set_child(Some(&tv));

    let st = Rc::new(State {
        token: Cell::new(0),
        debounce: RefCell::new(None),
        page: Cell::new(1),
        total_pages: Cell::new(0),
        query: RefCell::new(String::new()),
    });

    let run = {
        let (results, status, spinner, more, st, dialog) = (
            results.clone(),
            status.clone(),
            spinner.clone(),
            more.clone(),
            st.clone(),
            dialog.clone(),
        );
        Rc::new(move |append: bool| {
            let q = st.query.borrow().clone();
            if !append {
                results.remove_all();
                st.page.set(1);
            }
            more.set_visible(false);
            if q.chars().count() < 2 {
                status.set_text("");
                spinner.set_visible(false);
                return;
            }
            let token = st.token.get() + 1;
            st.token.set(token);
            spinner.set_visible(true);
            let page = st.page.get();
            let client = ctx().client.clone();
            let (results, status, spinner, more, st, dialog) = (
                results.clone(),
                status.clone(),
                spinner.clone(),
                more.clone(),
                st.clone(),
                dialog.clone(),
            );
            runtime::spawn(
                async move { client.search(&q, page, "en").await },
                move |res| {
                    if st.token.get() != token {
                        return;
                    }
                    spinner.set_visible(false);
                    match res {
                        Ok(r) => {
                            st.total_pages.set(r.total_pages);
                            status.set_text(&match r.total_results {
                                0 => "No results. Try a different word or spelling.".to_string(),
                                1 => "1 result".to_string(),
                                n => format!("{n} results"),
                            });
                            for item in r.results {
                                results.append(&result_row(&item, &dialog));
                            }
                            more.set_visible(page < r.total_pages);
                        }
                        Err(e) => {
                            status.set_text(
                                "Search needs an internet connection. Check your connection and try again.",
                            );
                            eprintln!("search: {e}");
                        }
                    }
                },
            );
        })
    };

    {
        let (st, run) = (st.clone(), run.clone());
        more.connect_clicked(move |_| {
            st.page.set(st.page.get() + 1);
            run(true);
        });
    }

    {
        let (st, run, quick, dialog) = (st.clone(), run.clone(), quick.clone(), dialog.clone());
        entry.connect_search_changed(move |e| {
            let q = e.text().trim().to_string();
            fill_quick(&quick, &q, &dialog);
            *st.query.borrow_mut() = q;
            if let Some(id) = st.debounce.take() {
                id.remove();
            }
            let (st2, run) = (st.clone(), run.clone());
            let id = glib::timeout_add_local_once(Duration::from_millis(350), move || {
                st2.debounce.replace(None);
                run(false);
            });
            st.debounce.replace(Some(id));
        });
    }
    {
        let (quick, dialog) = (quick.clone(), dialog.clone());
        entry.connect_activate(move |_| {
            // Enter opens the first quick match
            if let Some(row) = quick.row_at_index(0) {
                row.activate();
                dialog.close();
            }
        });
    }
    if let Some(q) = initial {
        entry.set_text(q);
    }
    quick.set_visible(false);
    dialog.present(Some(ctx().window()));
    entry.grab_focus();
}

fn fill_quick(list: &gtk::ListBox, q: &str, dialog: &adw::Dialog) {
    list.remove_all();
    let mut items: Vec<(String, String, u32, Option<u32>)> = vec![];
    let lower = q.to_lowercase();
    if let Some((c, v)) = parse_verse_key(q) {
        if let Some(ch) = ctx().chapter(c) {
            if v >= 1 && v <= ch.verses_count {
                items.push((format!("Go to {c}:{v}"), ch.name_simple.clone(), c, Some(v)));
            }
        }
    } else if let Some(n) = lower
        .strip_prefix("juz")
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        if (1..=30).contains(&n) {
            // first surah of the juz is resolved lazily when opened
            items.push((
                format!("Juz {n}"),
                "Open the first verse of this juz".into(),
                0,
                Some(n),
            ));
        }
    } else if let Some(n) = lower
        .strip_prefix("page")
        .and_then(|s| s.trim().parse::<u32>().ok())
    {
        if let Some(ch) = ctx().chapters().iter().rev().find(|c| c.first_page() <= n) {
            items.push((format!("Page {n}"), ch.name_simple.clone(), ch.id, None));
        }
    } else if !q.is_empty() {
        let norm = |s: &str| {
            s.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        };
        let nq = norm(q);
        for ch in ctx().chapters().iter() {
            if ch.id.to_string() == q
                || norm(&ch.name_simple).contains(&nq)
                || ch.translated_name.name.to_lowercase().contains(&lower)
                || ch.name_arabic.contains(q)
            {
                items.push((
                    format!("{}. {}", ch.id, ch.name_simple),
                    format!("{} · {} Ayahs", ch.translated_name.name, ch.verses_count),
                    ch.id,
                    None,
                ));
            }
            if items.len() >= 5 {
                break;
            }
        }
    }
    for (title, subtitle, chapter, verse) in items {
        let row = adw::ActionRow::new();
        row.set_title(&quran_core::text::escape(&title));
        row.set_subtitle(&quran_core::text::escape(&subtitle));
        row.set_activatable(true);
        row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let d = dialog.clone();
        let is_juz = chapter == 0;
        row.connect_activated(move |_| {
            d.close();
            if is_juz {
                open_juz(verse.unwrap_or(1));
            } else {
                crate::window::open_chapter(chapter, verse);
            }
        });
        list.append(&row);
    }
    list.set_visible(list.first_child().is_some());
}

fn open_juz(n: u32) {
    let client = ctx().client.clone();
    runtime::spawn(async move { client.juzs().await }, move |res| match res {
        Ok(juzs) => {
            if let Some(j) = juzs.iter().find(|j| j.juz_number == n) {
                if let Some((c, v, _)) = j.ranges().first().copied() {
                    crate::window::open_chapter(c, Some(v));
                }
            }
        }
        Err(e) => ctx().toast_error("Couldn't open this juz", &e),
    });
}

fn result_row(item: &quran_core::SearchResult, dialog: &adw::Dialog) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_activatable(true);
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    b.add_css_class("search-result");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let (c, v) = parse_verse_key(&item.verse_key).unwrap_or((1, 1));
    let name = ctx().chapter(c).map(|c| c.name_simple).unwrap_or_default();
    top.append(&label(
        &format!("{name} {}", item.verse_key),
        &["verse-key", "accent-text"],
    ));
    b.append(&top);
    let ar = gtk::Label::new(Some(&item.text));
    ar.add_css_class("quran-snippet");
    ar.set_wrap(true);
    ar.set_xalign(1.0);
    ar.set_direction(gtk::TextDirection::Rtl);
    b.append(&ar);
    for t in &item.translations {
        let l = gtk::Label::new(None);
        // quran.com marks matches with <em>; show them bold
        let html = t.text.replace("<em>", "<b>").replace("</em>", "</b>");
        l.set_markup(&html_to_pango(&html, HtmlOptions::default()));
        l.set_wrap(true);
        l.set_xalign(0.0);
        b.append(&l);
        if !t.name.is_empty() {
            b.append(&label(&format!("— {}", t.name), &["translator"]));
        }
    }
    row.set_child(Some(&b));
    let click = gtk::GestureClick::new();
    let d = dialog.clone();
    click.connect_released(move |_, _, _, _| {
        d.close();
        crate::window::open_chapter(c, Some(v));
    });
    row.add_controller(click);
    row
}
