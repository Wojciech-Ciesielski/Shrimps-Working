//! Ustawienia wyglądu (GSettings): motyw i kolor akcentu, dok, skalowanie tekstu, animacje, przyciski
//! okna i czcionki. Brakujący schemat albo klucz (np. starszy GNOME bez koloru akcentu) chowa swój wiersz.

use gtk::gio;
use gtk::prelude::*;

pub const INTERFACE: &str = "org.gnome.desktop.interface";
pub const WM: &str = "org.gnome.desktop.wm.preferences";
pub const DOCK: &str = "org.gnome.shell.extensions.dash-to-dock";

/// Kolory akcentu GNOME 47+ (klucz accent-color) i ich odcienie z libadwaita.
pub const ACCENTS: [(&str, &str); 9] = [
    ("blue", "#3584e4"),
    ("teal", "#2190a4"),
    ("green", "#3a944a"),
    ("yellow", "#c88800"),
    ("orange", "#ed5b00"),
    ("red", "#e62d42"),
    ("pink", "#d56199"),
    ("purple", "#9141ac"),
    ("slate", "#6f8396"),
];
pub const TEXT_SCALES: [f64; 4] = [1.0, 1.1, 1.25, 1.5];
/// Tylko zamknij / wszystkie po prawej (domyślnie w Shrimp) / wszystkie po lewej.
pub const BUTTONS: [&str; 3] = [
    "appmenu:close",
    "appmenu:minimize,maximize,close",
    "close,minimize,maximize:appmenu",
];
pub const DOCK_POSITIONS: [&str; 3] = ["BOTTOM", "LEFT", "RIGHT"];
pub const DOCK_SIZES: [i32; 3] = [32, 48, 64];
pub const FONTS: [(&str, &str); 3] = [
    ("font-name", "font.interface"),
    ("document-font-name", "font.document"),
    ("monospace-font-name", "font.mono"),
];

/// Klucze, które przywraca „Przywróć domyślne” (do wartości domyślnych Shrimp z 99_shrimp.gschema.override).
const RESET: [(&str, &[&str]); 3] = [
    (
        INTERFACE,
        &[
            "color-scheme",
            "accent-color",
            "text-scaling-factor",
            "enable-animations",
            "font-name",
            "document-font-name",
            "monospace-font-name",
        ],
    ),
    (WM, &["button-layout"]),
    (
        DOCK,
        &[
            "dock-position",
            "dash-max-icon-size",
            "dock-fixed",
            "intellihide",
            "show-trash",
        ],
    ),
];

/// Katalogi schematów rozszerzeń zainstalowanych poza /usr/share/glib-2.0 (np. dok z extensions.gnome.org).
const EXT_SCHEMAS: [&str; 2] = [
    "/usr/share/gnome-shell/extensions/dash-to-dock@micxgx.gmail.com/schemas",
    ".local/share/gnome-shell/extensions/dash-to-dock@micxgx.gmail.com/schemas",
];

pub struct Store {
    pub interface: Option<gio::Settings>,
    pub wm: Option<gio::Settings>,
    pub dock: Option<gio::Settings>,
}

impl Store {
    /// `demo`: ustawienia w pamięci, nic nie trafia do systemu.
    pub fn new(demo: bool) -> Self {
        Self {
            interface: open(INTERFACE, demo),
            wm: open(WM, demo),
            dock: open(DOCK, demo),
        }
    }

    pub fn reset(&self) {
        for (id, keys) in RESET {
            let s = match id {
                INTERFACE => &self.interface,
                WM => &self.wm,
                _ => &self.dock,
            };
            if let Some(s) = s {
                for k in keys {
                    if has(s, k) {
                        s.reset(k);
                    }
                }
            }
        }
    }
}

fn lookup(id: &str) -> Option<gio::SettingsSchema> {
    if let Some(s) = gio::SettingsSchemaSource::default().and_then(|src| src.lookup(id, true)) {
        return Some(s);
    }
    let home = gtk::glib::home_dir();
    EXT_SCHEMAS.iter().find_map(|dir| {
        let dir = home.join(dir);
        if !dir.join("gschemas.compiled").is_file() {
            return None;
        }
        gio::SettingsSchemaSource::from_directory(&dir, gio::SettingsSchemaSource::default().as_ref(), false)
            .ok()?
            .lookup(id, false)
    })
}

fn open(id: &str, demo: bool) -> Option<gio::Settings> {
    let schema = lookup(id)?;
    let backend = demo.then(gio::memory_settings_backend_new);
    Some(gio::Settings::new_full(&schema, backend.as_ref(), None))
}

pub fn has(s: &gio::Settings, key: &str) -> bool {
    s.settings_schema().is_some_and(|sc| sc.has_key(key))
}

/// Indeks najbliższej wartości (np. 1.15 → 110 %, 40 px → 32 px przy równej odległości bierze mniejszą).
pub fn nearest<T: Copy + Into<f64>>(values: &[T], v: f64) -> u32 {
    let mut best = 0;
    for (i, x) in values.iter().enumerate() {
        if (Into::<f64>::into(*x) - v).abs() < (Into::<f64>::into(values[best]) - v).abs() {
            best = i;
        }
    }
    best as u32
}

/// Indeks na liście albo `fallback`, gdy wartość jest spoza niej (np. własny układ przycisków).
pub fn index_of(values: &[&str], v: &str, fallback: u32) -> u32 {
    values
        .iter()
        .position(|x| x.eq_ignore_ascii_case(v))
        .map_or(fallback, |i| i as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_values() {
        assert_eq!(nearest(&TEXT_SCALES, 1.0), 0);
        assert_eq!(nearest(&TEXT_SCALES, 1.15), 1);
        assert_eq!(nearest(&TEXT_SCALES, 1.3), 2);
        assert_eq!(nearest(&TEXT_SCALES, 2.0), 3);
        assert_eq!(nearest(&DOCK_SIZES, 40.0), 0);
        assert_eq!(nearest(&DOCK_SIZES, 48.0), 1);
    }

    #[test]
    fn index_values() {
        assert_eq!(index_of(&BUTTONS, "appmenu:minimize,maximize,close", 0), 1);
        assert_eq!(index_of(&BUTTONS, ":close", 1), 1);
        assert_eq!(index_of(&DOCK_POSITIONS, "LEFT", 0), 1);
    }

    #[test]
    fn accents_have_translations() {
        for (name, _) in ACCENTS {
            let key: &'static str = Box::leak(format!("accent.{name}").into_boxed_str());
            assert_ne!(crate::i18n::tr(key), key);
        }
    }
}
