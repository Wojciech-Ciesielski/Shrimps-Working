//! Zakładki wspólne z GTK, Nautilusem i Nemo: ~/.config/gtk-3.0/bookmarks („adres [nazwa]” w wierszu).
use gtk::{gio, glib};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    pub uri: String,
    pub name: Option<String>,
}

pub fn path() -> PathBuf {
    glib::user_config_dir().join("gtk-3.0").join("bookmarks")
}

pub fn parse(s: &str) -> Vec<Bookmark> {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| match l.split_once(' ') {
            Some((u, n)) => Bookmark { uri: u.into(), name: Some(n.trim().to_string()).filter(|n| !n.is_empty()) },
            None => Bookmark { uri: l.into(), name: None },
        })
        .collect()
}

pub fn format(v: &[Bookmark]) -> String {
    v.iter().map(|b| match &b.name { Some(n) => format!("{} {n}\n", b.uri), None => format!("{}\n", b.uri) }).collect()
}

pub fn load() -> Vec<Bookmark> {
    std::fs::read_to_string(path()).map(|s| parse(&s)).unwrap_or_default()
}

pub fn save(v: &[Bookmark]) {
    let p = path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, format(v));
}

pub fn add(f: &gio::File) {
    use gtk::prelude::FileExt;
    let mut v = load();
    let uri = f.uri().to_string();
    if !v.iter().any(|b| b.uri == uri) {
        v.push(Bookmark { uri, name: None });
        save(&v);
    }
}

pub fn remove(uri: &str) {
    let v: Vec<Bookmark> = load().into_iter().filter(|b| b.uri != uri).collect();
    save(&v);
}

pub fn rename(uri: &str, name: &str) {
    let v: Vec<Bookmark> = load()
        .into_iter()
        .map(|b| if b.uri == uri { Bookmark { name: Some(name.to_string()).filter(|n| !n.trim().is_empty()), ..b } } else { b })
        .collect();
    save(&v);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let s = "file:///home/jan/Projekty Projekty firmy\nsftp://serwer/dom\n\n";
        let v = parse(s);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].name.as_deref(), Some("Projekty firmy"));
        assert_eq!(format(&v), "file:///home/jan/Projekty Projekty firmy\nsftp://serwer/dom\n");
    }
}
