//! Katalog aplikacji internetowych z `data/webapps.json`.
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Kategorie w kolejności paska bocznego Shrimp Apps.
pub const CATEGORIES: [&str; 8] =
    ["internet", "biuro", "wideo", "muzyka", "grafika", "gry", "programowanie", "narzedzia"];

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WebApp {
    pub id: String,
    pub name: String,
    pub url: String,
    pub category: String,
    /// Nazwy w innych językach, np. `{"pl": "Dokumenty Google"}`.
    #[serde(default)]
    pub names: HashMap<String, String>,
    /// Ścieżka ikony względem `data/` (wypełnia `tools/verify.py`).
    #[serde(default)]
    pub icon: Option<String>,
}

#[derive(Deserialize)]
struct Doc {
    apps: Vec<WebApp>,
}

mod icons {
    include!(concat!(env!("OUT_DIR"), "/icons.rs"));
}

static CATALOG: OnceLock<Vec<WebApp>> = OnceLock::new();

/// Wszystkie aplikacje z katalogu, w kolejności z pliku (najpopularniejsze w kategorii najpierw).
pub fn catalog() -> &'static [WebApp] {
    CATALOG.get_or_init(|| {
        let doc: Doc = serde_json::from_str(include_str!("../data/webapps.json"))
            .expect("data/webapps.json: niepoprawny JSON");
        doc.apps
    })
}

pub fn find(id: &str) -> Option<&'static WebApp> {
    catalog().iter().find(|a| a.id == id)
}

impl WebApp {
    /// Aplikacja spoza katalogu, utworzona przez użytkownika.
    pub fn custom(name: &str, url: &str) -> WebApp {
        WebApp {
            id: format!("custom-{}", slug(&host_and_path(url))),
            name: name.trim().to_string(),
            url: url.to_string(),
            category: "custom".into(),
            names: HashMap::new(),
            icon: None,
        }
    }

    /// Nazwa w języku interfejsu (np. „Mapy Google” po polsku).
    pub fn display_name(&self) -> &str {
        let lang = crate::i18n::lang();
        self.names.get(lang.as_str()).map(String::as_str).unwrap_or(&self.name)
    }

    /// Domena bez „www.”, do podpisu w wierszu.
    pub fn host(&self) -> String {
        host_of(&self.url)
    }

    /// Ikona PNG 128×128 wbudowana w bibliotekę, jeśli CI ją pobrało.
    pub fn icon_png(&self) -> Option<&'static [u8]> {
        icons::ICONS.iter().find(|(id, _)| *id == self.id).map(|(_, b)| *b)
    }

    /// Ikona zastępcza: pierwsza litera na kolorowym tle (jak w makiecie Shrimp Apps).
    pub fn letter_svg(&self) -> String {
        letter_svg(self.display_name())
    }
}

pub fn letter_svg(name: &str) -> String {
    const TINTS: [&str; 8] =
        ["#3584e4", "#26a269", "#e66100", "#813d9c", "#c64600", "#1c71d8", "#a51d2d", "#865e3c"];
    let ch = name.trim().chars().next().unwrap_or('?').to_uppercase().next().unwrap_or('?');
    let ch = match ch {
        '<' | '>' | '&' | '"' => '?',
        c => c,
    };
    let tint = TINTS[(ch as usize + name.chars().count()) % TINTS.len()];
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 64 64"><rect width="64" height="64" rx="14" fill="{tint}"/><text x="32" y="44" font-family="Cantarell, sans-serif" font-size="34" font-weight="700" text-anchor="middle" fill="#ffffff">{ch}</text></svg>"##
    )
}

fn strip_scheme(url: &str) -> &str {
    url.split_once("://").map(|(_, r)| r).unwrap_or(url)
}

pub fn host_of(url: &str) -> String {
    let rest = strip_scheme(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host = host.rsplit('@').next().unwrap_or(host);
    host.trim_start_matches("www.").to_lowercase()
}

fn host_and_path(url: &str) -> String {
    let rest = strip_scheme(url);
    rest.split(['?', '#']).next().unwrap_or("").trim_start_matches("www.").trim_end_matches('/').to_lowercase()
}

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Zamienia to, co wpisał użytkownik, na adres https (albo http, jeśli tak podał).
/// `None`, gdy to nie wygląda na adres strony.
pub fn normalize_url(input: &str) -> Option<String> {
    let s = input.trim();
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return None;
    }
    let url = if let Some((scheme, _)) = s.split_once("://") {
        if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") {
            return None;
        }
        s.to_string()
    } else {
        format!("https://{s}")
    };
    let host = host_of(&url);
    let host = host.split(':').next().unwrap_or("");
    let ok = host.len() > 3
        && host.contains('.')
        && !host.starts_with('.')
        && !host.ends_with('.')
        && host.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-');
    ok.then_some(url)
}

/// Nazwa zaproponowana z adresu: „notion.so” → „Notion”.
pub fn name_from_url(url: &str) -> String {
    let host = host_of(url);
    let parts: Vec<&str> = host.split('.').collect();
    let base = if parts.len() > 2 { parts[parts.len() - 2] } else { parts[0] };
    let mut c = base.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn catalog_has_150_unique_apps() {
        let c = catalog();
        assert_eq!(c.len(), 150);
        let ids: HashSet<_> = c.iter().map(|a| &a.id).collect();
        let urls: HashSet<_> = c.iter().map(|a| &a.url).collect();
        assert_eq!(ids.len(), 150, "powtórzone id");
        assert_eq!(urls.len(), 150, "powtórzone adresy");
        for a in c {
            assert!(CATEGORIES.contains(&a.category.as_str()), "{}: kategoria {}", a.id, a.category);
            assert!(a.url.starts_with("https://"), "{}: tylko https", a.id);
            assert!(normalize_url(&a.url).is_some(), "{}: adres", a.id);
            assert!(!a.name.is_empty());
            assert_eq!(slug(&a.id), a.id, "{}: id ma być slugiem", a.id);
        }
    }

    #[test]
    fn urls_normalize() {
        assert_eq!(normalize_url("notion.so").as_deref(), Some("https://notion.so"));
        assert_eq!(normalize_url("http://a.example/x").as_deref(), Some("http://a.example/x"));
        assert_eq!(normalize_url("ftp://a.example"), None);
        assert_eq!(normalize_url("not a url"), None);
        assert_eq!(normalize_url("localhost"), None);
        assert_eq!(host_of("https://www.Example.com:8080/x?y"), "example.com:8080");
        assert_eq!(name_from_url("https://app.todoist.com/"), "Todoist");
        assert_eq!(WebApp::custom("X", "https://www.a.b/c/").id, "custom-a-b-c");
    }

    #[test]
    fn letter_icon_is_safe_svg() {
        let svg = letter_svg("<b>");
        assert!(svg.contains(">?</text>"));
        assert!(letter_svg("ąb").contains(">Ą</text>"));
    }
}
