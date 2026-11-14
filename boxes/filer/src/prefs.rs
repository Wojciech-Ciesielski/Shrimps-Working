//! Preferencje jak w Nemo: Widoki, Zachowanie, Wyświetlanie, Podgląd.
use crate::i18n::tr;
use crate::settings::{self, Settings};
use adw::prelude::*;

fn switch(g: &adw::PreferencesGroup, title: &str, get: fn(&Settings) -> bool, set: fn(&mut Settings, bool), changed: &std::rc::Rc<dyn Fn()>) {
    let r = adw::SwitchRow::builder().title(title).active(get(&settings::get())).build();
    let c = changed.clone();
    r.connect_active_notify(move |r| {
        let v = r.is_active();
        settings::update(|s| set(s, v));
        c();
    });
    g.add(&r);
}

pub fn show(parent: &gtk::Window, changed: std::rc::Rc<dyn Fn()>) {
    let d = adw::PreferencesDialog::new();
    d.set_title(tr("prefs.title"));
    let s = settings::get();

    let views = adw::PreferencesPage::builder().title(tr("prefs.views")).icon_name("view-grid-symbolic").build();
    let g = adw::PreferencesGroup::new();
    let kinds = ["icons", "list", "compact"];
    let view = adw::ComboRow::builder()
        .title(tr("prefs.default_view"))
        .model(&gtk::StringList::new(&[tr("view.icons"), tr("view.list"), tr("view.compact")]))
        .selected(kinds.iter().position(|k| *k == s.view).unwrap_or(0) as u32)
        .build();
    {
        let c = changed.clone();
        view.connect_selected_notify(move |r| {
            let v = kinds[r.selected() as usize].to_string();
            settings::update(|s| s.view = v);
            c();
        });
    }
    g.add(&view);
    let zoom = adw::SpinRow::with_range(0.0, 6.0, 1.0);
    zoom.set_title(tr("prefs.default_zoom"));
    zoom.set_value(s.zoom as f64);
    {
        let c = changed.clone();
        zoom.connect_value_notify(move |r| {
            let v = r.value() as i32;
            settings::update(|s| s.zoom = v);
            c();
        });
    }
    g.add(&zoom);
    switch(&g, tr("prefs.folders_first"), |s| s.folders_first, |s, v| s.folders_first = v, &changed);
    switch(&g, tr("prefs.show_hidden"), |s| s.show_hidden, |s, v| s.show_hidden = v, &changed);
    views.add(&g);
    d.add(&views);

    let behavior = adw::PreferencesPage::builder().title(tr("prefs.behavior")).icon_name("input-mouse-symbolic").build();
    let g = adw::PreferencesGroup::new();
    switch(&g, tr("prefs.single_click"), |s| s.single_click, |s, v| s.single_click = v, &changed);
    switch(&g, tr("prefs.confirm_trash"), |s| s.confirm_trash, |s, v| s.confirm_trash = v, &changed);
    switch(&g, tr("prefs.confirm_delete"), |s| s.confirm_delete, |s, v| s.confirm_delete = v, &changed);
    switch(&g, tr("prefs.show_delete"), |s| s.show_delete_item, |s, v| s.show_delete_item = v, &changed);
    behavior.add(&g);
    d.add(&behavior);

    let display = adw::PreferencesPage::builder().title(tr("prefs.display")).icon_name("preferences-desktop-display-symbolic").build();
    let g = adw::PreferencesGroup::new();
    switch(&g, tr("prefs.full_path"), |s| s.full_path_title, |s, v| s.full_path_title = v, &changed);
    switch(&g, tr("prefs.sidebar"), |s| s.show_sidebar, |s, v| s.show_sidebar = v, &changed);
    display.add(&g);
    d.add(&display);

    let preview = adw::PreferencesPage::builder().title(tr("prefs.preview")).icon_name("image-x-generic-symbolic").build();
    let g = adw::PreferencesGroup::new();
    switch(&g, tr("prefs.thumbnails"), |s| s.thumbnails, |s, v| s.thumbnails = v, &changed);
    let limit = adw::SpinRow::with_range(1.0, 4000.0, 10.0);
    limit.set_title(tr("prefs.thumb_limit"));
    limit.set_value(s.thumbnail_limit_mb as f64);
    {
        let c = changed.clone();
        limit.connect_value_notify(move |r| {
            let v = r.value() as u64;
            settings::update(|s| s.thumbnail_limit_mb = v);
            c();
        });
    }
    g.add(&limit);
    preview.add(&g);
    d.add(&preview);
    d.present(Some(parent));
}
