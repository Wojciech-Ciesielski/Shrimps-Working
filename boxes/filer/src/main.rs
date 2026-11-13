//! Filer: menedżer plików Shrimp. Funkcje i układ jak w Nemo (karty, dwa panele, panel boczny,
//! widoki ikon/listy/kompaktowy, operacje z kolejką i cofaniem, kosz, miniatury, skrypty Nemo),
//! napisany od nowa w Rust na GTK4 + libadwaita, z ikonami motywu (MoreWaita w Shrimp).

mod bookmarks;
mod help;
mod i18n;
mod ops;
mod pathbar;
mod places;
mod prefs;
mod props;
mod settings;
mod tab;
mod thumbs;
mod util;
mod window;

use adw::prelude::*;
use gtk::{gio, glib};

const APP_ID: &str = "org.shrimp.Filer";

/// Źródła do testu `every_used_key_exists` (i18n).
#[cfg(test)]
pub const SOURCES: &[(&str, &str)] = &[
    ("bookmarks.rs", include_str!("bookmarks.rs")),
    ("ops.rs", include_str!("ops.rs")),
    ("pathbar.rs", include_str!("pathbar.rs")),
    ("places.rs", include_str!("places.rs")),
    ("prefs.rs", include_str!("prefs.rs")),
    ("props.rs", include_str!("props.rs")),
    ("tab.rs", include_str!("tab.rs")),
    ("util.rs", include_str!("util.rs")),
    ("window.rs", include_str!("window.rs")),
];

/// Styl Shrimp (jak Panko i Cocktail): wiersze naprzemiennie jaśniejsze i ciemniejsze, bez obrysów,
/// zaokrąglone zaznaczenie w siatce, spokojne nagłówki kolumn. Kolory tylko z nazwanych kolorów libadwaita.
const CSS: &str = "
.filer-list, .filer-grid { background: transparent; }
/* Wiersze jak w Panko i Cocktailu: niższe, co drugi jaśniejszy od tła (pasek boczny zostaje ciemniejszy). */
.filer-list > listview > row { min-height: 30px; }
.filer-list > listview > row:nth-child(even):not(:selected) { background-color: shade(@window_bg_color, 1.07); }
.filer-list > header > button { font-weight: normal; color: alpha(currentColor, 0.6); border: none; box-shadow: none; padding: 6px 12px; }
.filer-list > listview > row > cell { padding: 0 12px; }
.filer-grid > child { border-radius: 10px; margin: 4px; padding: 6px; }
.filer-grid > child:hover:not(:selected) { background-color: alpha(currentColor, 0.05); }
";

fn style() {
    let p = gtk::CssProvider::new();
    p.load_from_string(CSS);
    if let Some(d) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(&d, &p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).flags(gio::ApplicationFlags::HANDLES_OPEN).build();
    app.connect_startup(|app| {
        window::accels(app);
        style();
        let quit = gio::SimpleAction::new("quit", None);
        let a = app.clone();
        quit.connect_activate(move |_, _| a.quit());
        app.add_action(&quit);
        let nw = gio::SimpleAction::new("new-window", None);
        let a = app.clone();
        nw.connect_activate(move |_, _| {
            window::Win::new(&a, None);
        });
        app.add_action(&nw);
    });
    app.connect_activate(|app| {
        window::Win::new(app, None);
    });
    app.connect_open(|app, files, _| {
        for f in files {
            // Plik (nie folder): otwieramy folder z nim i zaznaczamy go, jak `nemo plik`.
            let is_dir = f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) != gio::FileType::Regular;
            if is_dir {
                window::Win::new(app, Some(f.clone()));
            } else if let Some(p) = f.parent() {
                let w = window::Win::new(app, Some(p));
                w.tab().select_when_ready(vec![f.clone()]);
            }
        }
    });
    app.run()
}
