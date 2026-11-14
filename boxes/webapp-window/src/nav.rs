//! Reguły okna bez GTK (testowane): identyfikator okna, które linki zostają w aplikacji, nazwy pobranych plików.
use std::path::{Path, PathBuf};

/// Host z adresu, bez portu i „www.”.
pub fn host(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    let h = h.rsplit_once('@').map_or(h, |(_, h)| h);
    let h = h.split(':').next().unwrap_or("");
    h.trim_start_matches("www.").to_ascii_lowercase()
}

/// Identyfikator z małych liter, cyfr i „-” (nazwa katalogu danych).
pub fn safe_id(s: &str) -> String {
    let v: String = s.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let v = v.trim_matches('-').to_string();
    if v.is_empty() { "app".into() } else { v }
}

/// Identyfikator aplikacji GTK/D-Bus: `org.shrimp.WebApp.<id>` (ostatni człon: litery, cyfry, „_”, nie od cyfry).
pub fn app_id(id: &str) -> String {
    let mut seg: String = safe_id(id).replace('-', "_");
    if seg.starts_with(|c: char| c.is_ascii_digit()) {
        seg.insert(0, '_');
    }
    format!("org.shrimp.WebApp.{seg}")
}

/// Domena główna: „mail.google.com” → „google.com”, „x.bbc.co.uk” → „bbc.co.uk”.
pub fn base_domain(host: &str) -> String {
    let parts: Vec<&str> = host.split('.').filter(|p| !p.is_empty()).collect();
    let n = parts.len();
    if n <= 2 {
        return host.to_string();
    }
    let second = parts[n - 2];
    let take = if parts[n - 1].len() == 2 && ["co", "com", "org", "net", "gov", "edu", "ac", "gob"].contains(&second) { 3 } else { 2 };
    parts[n - take..].join(".")
}

/// Logowanie u dużych dostawców zostaje w oknie (inaczej sesja trafiłaby do przeglądarki).
const LOGIN: [&str; 7] = [
    "accounts.google.com",
    "login.microsoftonline.com",
    "login.live.com",
    "appleid.apple.com",
    "github.com",
    "facebook.com",
    "id.atlassian.com",
];

/// Kliknięty link poza aplikacją idzie do przeglądarki; przekierowania (bez kliknięcia) zostają.
/// Adresy inne niż http(s) (mailto:, tel:, zoommtg:) zawsze idą do systemu.
pub fn to_browser(app_url: &str, uri: &str, user_click: bool) -> bool {
    if !(uri.starts_with("https://") || uri.starts_with("http://")) {
        return !uri.starts_with("about:") && !uri.starts_with("blob:") && !uri.starts_with("data:");
    }
    if !user_click {
        return false;
    }
    let h = host(uri);
    if base_domain(&h) == base_domain(&host(app_url)) || LOGIN.contains(&h.as_str()) {
        return false;
    }
    true
}

/// Nazwa pliku bez katalogów i znaków sterujących.
pub fn safe_file_name(s: &str) -> String {
    let n: String = s.rsplit(['/', '\\']).next().unwrap_or("").chars().filter(|c| !c.is_control()).collect();
    let n = n.trim().trim_start_matches('.').to_string();
    if n.is_empty() { "plik".into() } else { n }
}

/// Wolna ścieżka: „a.pdf”, potem „a (2).pdf”…
pub fn free_path(dir: &Path, name: &str) -> PathBuf {
    let p = dir.join(name);
    if !p.exists() {
        return p;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) if !s.is_empty() => (s.to_string(), format!(".{e}")),
        _ => (name.to_string(), String::new()),
    };
    (2..).map(|i| dir.join(format!("{stem} ({i}){ext}"))).find(|p| !p.exists()).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert_eq!(app_id("whatsapp"), "org.shrimp.WebApp.whatsapp");
        assert_eq!(app_id("google-docs"), "org.shrimp.WebApp.google_docs");
        assert_eq!(app_id("9gag"), "org.shrimp.WebApp._9gag");
        assert_eq!(safe_id("../Moja App!"), "moja-app");
    }

    #[test]
    fn hosts() {
        assert_eq!(host("https://www.YouTube.com/watch?v=1"), "youtube.com");
        assert_eq!(host("https://user@mail.google.com:443/x"), "mail.google.com");
        assert_eq!(base_domain("mail.google.com"), "google.com");
        assert_eq!(base_domain("news.bbc.co.uk"), "bbc.co.uk");
        assert_eq!(base_domain("allegro.com.pl"), "allegro.com.pl");
    }

    #[test]
    fn links() {
        let app = "https://web.whatsapp.com/";
        assert!(!to_browser(app, "https://static.whatsapp.com/x", true));
        assert!(to_browser(app, "https://example.org/", true));
        assert!(!to_browser(app, "https://example.org/", false));
        assert!(!to_browser(app, "https://accounts.google.com/o/oauth2", true));
        assert!(to_browser(app, "mailto:a@b.c", false));
        assert!(!to_browser(app, "about:blank", true));
    }

    #[test]
    fn files() {
        assert_eq!(safe_file_name("../../etc/passwd"), "passwd");
        assert_eq!(safe_file_name(".hidden"), "hidden");
        let d = std::env::temp_dir().join(format!("shrimp-dl-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("a.pdf"), b"").unwrap();
        assert_eq!(free_path(&d, "a.pdf"), d.join("a (2).pdf"));
        assert_eq!(free_path(&d, "b.pdf"), d.join("b.pdf"));
        let _ = std::fs::remove_dir_all(&d);
    }
}
