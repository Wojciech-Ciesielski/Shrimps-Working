//! Wspólne elementy: ikony i obrazy z sieci, wiersz aplikacji, przycisk akcji.

use std::path::PathBuf;
use std::rc::Rc;

use gtk::{gdk, gio, glib, prelude::*};

use crate::catalog::App;
use crate::net;
use crate::store::{label_for, Status, Store};

pub const FALLBACK_ICON: &str = "application-x-executable-symbolic";

/// Pierwsza ikona z listy, którą ma motyw (ikony z GNOME Icon Library mają zapas w Adwaita).
pub fn pick_icon(names: &[&'static str]) -> &'static str {
    let theme = gtk::IconTheme::for_display(&gdk::Display::default().expect("display"));
    names.iter().copied().find(|n| theme.has_icon(n)).unwrap_or(names[names.len() - 1])
}

pub const ICON_DISCOVER: &[&str] = &["compass2-symbolic", "explore-symbolic", "starred-symbolic"];
pub const ICON_INSTALLED: &[&str] = &["library-symbolic", "system-software-install-symbolic"];
pub const ICON_WEB: &[&str] = &["globe-symbolic", "web-browser-symbolic"];
// Ta sama ikona pomocy co w Panko i Cocktailu (znak zapytania), nie koło ratunkowe help-browser.
pub const ICON_HELP: &[&str] = &["dialog-question-symbolic", "help-about-symbolic"];
pub const ICON_VERIFIED: &[&str] = &["verified-checkmark-symbolic", "emblem-ok-symbolic"];

/// Wczytuje obraz z dysku poza wątkiem GTK.
async fn texture_from_path(path: PathBuf) -> Option<gdk::Texture> {
    gio::spawn_blocking(move || gdk::Texture::from_filename(&path).ok())
        .await
        .ok()
        .flatten()
}

/// Pobiera obraz (z pamięcią podręczną) i wczytuje go poza wątkiem GTK.
pub async fn texture_from_url(url: String) -> Option<gdk::Texture> {
    gio::spawn_blocking(move || {
        let path = net::cached_image(&url).ok()?;
        gdk::Texture::from_filename(&path).ok()
    })
    .await
    .ok()
    .flatten()
}

/// Ikona aplikacji: z katalogu (plik), z sieci albo z motywu (zainstalowane spoza katalogu).
pub fn app_icon(app: Option<&App>, id: &str, size: i32) -> gtk::Image {
    let img = gtk::Image::builder().pixel_size(size).icon_name(FALLBACK_ICON).build();
    img.add_css_class("app-icon");
    let theme = gtk::IconTheme::for_display(&gdk::Display::default().expect("display"));
    match app {
        Some(a) if a.icon_cached.is_some() || a.icon_remote.is_some() => {
            let cached = a.icon_cached.clone();
            let remote = a.icon_remote.clone();
            let weak = img.downgrade();
            glib::spawn_future_local(async move {
                let mut tex = None;
                if let Some(p) = cached {
                    tex = texture_from_path(p).await;
                }
                if tex.is_none() {
                    if let Some(u) = remote {
                        tex = texture_from_url(u).await;
                    }
                }
                if let (Some(t), Some(img)) = (tex, weak.upgrade()) {
                    img.set_paintable(Some(&t));
                }
            });
        }
        _ => {
            if theme.has_icon(id) {
                img.set_icon_name(Some(id));
            }
        }
    }
    img
}

fn apply_status(btn: &gtk::Button, s: Status) {
    btn.set_label(&label_for(s));
    btn.set_sensitive(!s.busy());
}

/// Tekstowy przycisk akcji bez tła (Pobierz / Otwórz / Zaktualizuj / postęp).
pub fn action_button(store: &Rc<Store>, id: &str) -> gtk::Button {
    let btn = gtk::Button::builder().valign(gtk::Align::Center).build();
    btn.add_css_class("flat");
    btn.add_css_class("get-button");
    apply_status(&btn, store.status(id));
    {
        let store = store.clone();
        let id = id.to_string();
        btn.connect_clicked(move |_| store.primary(&id));
    }
    {
        let store2 = store.clone();
        let id2 = id.to_string();
        let weak = btn.downgrade();
        store.watch(id, &btn, move || {
            if let Some(b) = weak.upgrade() {
                apply_status(&b, store2.status(&id2));
            }
        });
    }
    btn
}

/// Wiersz aplikacji z makiety: ikona, nazwa, kategoria, przycisk akcji.
/// `on_open` otwiera stronę aplikacji.
pub fn app_row(store: &Rc<Store>, app: &Rc<App>, subtitle: Option<String>, on_open: Rc<dyn Fn(&str)>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.add_css_class("app-row");

    let open = gtk::Button::builder().hexpand(true).build();
    open.add_css_class("flat");
    open.add_css_class("app-tile");
    open.set_tooltip_text(Some(&app.summary).filter(|s| !s.is_empty()).map(|s| s.as_str()));
    let inner = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    inner.append(&app_icon(Some(app), &app.id, 48));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
    texts.set_valign(gtk::Align::Center);
    let name = gtk::Label::builder()
        .label(&app.name)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    name.add_css_class("heading");
    let sub = subtitle.unwrap_or_else(|| app.main_category().map(|c| c.title().to_string()).unwrap_or_default());
    let cat = gtk::Label::builder()
        .label(&sub)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    cat.add_css_class("dim-label");
    cat.add_css_class("caption");
    texts.append(&name);
    texts.append(&cat);
    inner.append(&texts);
    open.set_child(Some(&inner));
    let id = app.id.clone();
    open.connect_clicked(move |_| on_open(&id));
    open.update_property(&[gtk::accessible::Property::Label(&app.name)]);

    row.append(&open);
    row.append(&action_button(store, &app.id));
    row
}

/// Etykieta do wrapowania tekstu na stronach.
pub fn wrap_label(text: &str, classes: &[&str]) -> gtk::Label {
    let l = gtk::Label::builder()
        .label(text)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .xalign(0.0)
        .build();
    for c in classes {
        l.add_css_class(c);
    }
    l
}

pub fn format_size(bytes: u64) -> String {
    glib::format_size(bytes).to_string()
}
