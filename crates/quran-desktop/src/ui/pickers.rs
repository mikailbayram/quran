//! Selection dialogs: translations (multi-select, grouped by language) and reciters.

use super::common::*;
use super::settings::capitalize;
use crate::ctx::ctx;
use adw::prelude::*;
use std::rc::Rc;

fn dialog_shell(title: &str, placeholder: &str) -> (adw::Dialog, gtk::SearchEntry, gtk::ListBox) {
    let dialog = adw::Dialog::new();
    dialog.set_title(title);
    dialog.set_content_width(520);
    dialog.set_content_height(640);
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    let entry = gtk::SearchEntry::new();
    entry.set_placeholder_text(Some(placeholder));
    entry.set_margin_start(12);
    entry.set_margin_end(12);
    entry.set_margin_bottom(6);
    tv.add_top_bar(&entry);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_margin_start(12);
    list.set_margin_end(12);
    list.set_margin_top(6);
    list.set_margin_bottom(12);
    list.set_valign(gtk::Align::Start);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&list));
    scroller.set_vexpand(true);
    tv.set_content(Some(&scroller));
    dialog.set_child(Some(&tv));

    let l = list.clone();
    let e = entry.clone();
    list.set_filter_func(move |row| {
        let q = e.text().to_lowercase();
        q.is_empty() || row.widget_name().to_lowercase().contains(&q)
    });
    entry.connect_search_changed(move |_| l.invalidate_filter());
    (dialog, entry, list)
}

pub fn show_translations() {
    let (dialog, entry, list) = dialog_shell("Translations", "Search translations or languages");
    ctx().with_translations(move |all| {
        let selected = ctx().with_settings(|s| s.translations.clone());
        let mut sorted: Vec<_> = all.iter().cloned().collect();
        // selected first, then English, then alphabetical by language
        sorted.sort_by_key(|t| {
            (
                !selected.contains(&t.id),
                t.language_name != "english",
                t.language_name.clone(),
                t.name.clone(),
            )
        });
        let languages: Rc<Vec<String>> =
            Rc::new(sorted.iter().map(|t| t.language_name.clone()).collect());
        let selected_count = selected.len();
        for t in &sorted {
            let row = adw::ActionRow::new();
            row.set_title(&escape_markup(&t.name));
            row.set_subtitle(&escape_markup(&t.author_name));
            let check = gtk::CheckButton::new();
            check.set_active(selected.contains(&t.id));
            check.set_valign(gtk::Align::Center);
            row.add_prefix(&check);
            row.set_activatable_widget(Some(&check));
            row.set_widget_name(&format!("{} {} {}", t.name, t.author_name, t.language_name));
            let id = t.id;
            check.connect_toggled(move |c| {
                let on = c.is_active();
                ctx().update_settings(|s| {
                    s.translations.retain(|x| *x != id);
                    if on {
                        s.translations.push(id);
                    }
                });
            });
            list.append(&row);
        }
        let langs = languages.clone();
        list.set_header_func(move |row, before| {
            let i = row.index() as usize;
            let header_text = if i < selected_count {
                (i == 0).then(|| "Selected".to_string())
            } else {
                let lang = &langs[i];
                let prev = before.map(|b| b.index() as usize);
                let changed = match prev {
                    Some(p) if p >= selected_count => langs[p] != *lang,
                    _ => true,
                };
                changed.then(|| capitalize(lang))
            };
            // only show group headers when not searching
            row.set_header(
                header_text
                    .as_ref()
                    .map(|t| {
                        let l = label(t, &["heading"]);
                        l.set_margin_top(14);
                        l.set_margin_bottom(6);
                        l.set_margin_start(6);
                        l
                    })
                    .as_ref(),
            );
        });
        let _ = &entry;
    });
    dialog.present(Some(ctx().window()));
}

pub fn show_reciters() {
    let (dialog, _entry, list) = dialog_shell("Reciters", "Search reciters");
    let d = dialog.clone();
    ctx().with_reciters(move |all| {
        let current = ctx().with_settings(|s| s.reciter);
        let mut group: Option<gtk::CheckButton> = None;
        for r in all.iter() {
            let row = adw::ActionRow::new();
            row.set_title(&escape_markup(r.display_name()));
            let qirat = r.qirat.as_ref().map(|q| q.name.clone()).unwrap_or_default();
            row.set_subtitle(&escape_markup(
                &[r.style_name(), &qirat]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" · "),
            ));
            let radio = gtk::CheckButton::new();
            if let Some(g) = &group {
                radio.set_group(Some(g));
            } else {
                group = Some(radio.clone());
            }
            radio.set_active(r.id == current);
            radio.set_valign(gtk::Align::Center);
            row.add_prefix(&radio);
            row.set_activatable_widget(Some(&radio));
            row.set_widget_name(&format!("{} {}", r.display_name(), r.style_name()));
            let id = r.id;
            let d = d.clone();
            radio.connect_toggled(move |c| {
                if c.is_active() && ctx().with_settings(|s| s.reciter) != id {
                    ctx().update_settings(|s| s.reciter = id);
                    ctx().player.reciter_changed();
                    d.close();
                }
            });
            list.append(&row);
        }
    });
    dialog.present(Some(ctx().window()));
}

fn escape_markup(s: &str) -> String {
    quran_core::text::escape(s)
}
