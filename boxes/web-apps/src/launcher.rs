//! Tworzenie aplikacji internetowej jednym kliknięciem.
//!
//! Aplikacja = plik `~/.local/share/applications/shrimp-webapp-<id>.desktop` + ikona w
//! `~/.local/share/shrimp/webapps/`. GNOME Shell widzi ją od razu w siatce aplikacji.
//! Okno otwiera `shrimp-webapp` (box webapp-window: własne okno WebKitGTK, osobne dane każdej
//! aplikacji). Serwisy z DRM (Netflix, Spotify…), których WebKitGTK nie odtworzy, i systemy bez
//! `shrimp-webapp` dostają przeglądarkę z rodziny Chromium w trybie aplikacji (`--app=URL`).
//! Gdy i takiej nie ma, adres otwiera domyślna przeglądarka (`xdg-open`).
//!
//! Shrimp Apps działa poza piaskownicą (pakiet systemowy), więc pisze pliki wprost i nie
//! potrzebuje okna potwierdzenia z portalu `org.freedesktop.portal.DynamicLauncher`.
//! Z piaskownicy tworzenie zwraca [`Error::Sandboxed`].
use crate::catalog::{self, WebApp};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const PREFIX: &str = "shrimp-webapp-";

/// Przeglądarki z trybem aplikacji, w kolejności wyboru: (identyfikator Flathub, polecenia w systemie).
const CHROMIUM_FAMILY: &[(&str, &[&str])] = &[
    ("org.chromium.Chromium", &["chromium", "chromium-browser"]),
    ("com.google.Chrome", &["google-chrome-stable", "google-chrome"]),
    ("com.brave.Browser", &["brave-browser", "brave"]),
    ("com.microsoft.Edge", &["microsoft-edge-stable", "microsoft-edge"]),
    ("com.vivaldi.Vivaldi", &["vivaldi-stable", "vivaldi"]),
    ("io.github.ungoogled_software.ungoogled_chromium", &[]),
];

/// Okno aplikacji internetowej Shrimp (box webapp-window).
const SHRIMP_WEBAPP: &str = "shrimp-webapp";

/// Serwisy z DRM (Widevine): WebKitGTK ich nie odtworzy, więc zostają przy Chromium.
const DRM_HOSTS: &[&str] = &[
    "netflix.com",
    "open.spotify.com",
    "disneyplus.com",
    "primevideo.com",
    "music.apple.com",
    "tv.apple.com",
    "max.com",
    "hbomax.com",
    "deezer.com",
    "tidal.com",
    "player.pl",
    "canalplusonline.com",
    "skyshowtime.com",
    "crunchyroll.com",
    "music.amazon.com",
];

#[derive(Debug)]
pub enum Error {
    Sandboxed,
    InvalidUrl,
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Sandboxed => f.write_str(&crate::i18n::tr("error.sandbox")),
            Error::InvalidUrl => f.write_str(&crate::i18n::tr("error.url")),
            Error::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Browser {
    /// Własne okno Shrimp (`shrimp-webapp --id … --name … --url …`, pełna ścieżka).
    Shrimp(PathBuf),
    /// Przeglądarka z Flathuba (`flatpak run <id> --app=URL`).
    Flatpak(String),
    /// Przeglądarka z systemu (pełna ścieżka).
    Host(PathBuf),
    /// Brak trybu aplikacji: adres otworzy domyślna przeglądarka.
    Default,
}

/// Utworzona aplikacja internetowa (odczytana z pliku `.desktop`).
#[derive(Debug, Clone, PartialEq)]
pub struct Created {
    pub id: String,
    pub name: String,
    pub url: String,
    pub desktop_file: PathBuf,
    pub icon_file: PathBuf,
}

/// Katalogi, w których powstają pliki (w testach tymczasowe).
#[derive(Debug, Clone)]
pub struct Dirs {
    pub data_home: PathBuf,
    /// Gdzie szukać przeglądarek z Flathuba (instalacje: systemowa i użytkownika).
    pub flatpak_roots: Vec<PathBuf>,
    pub path: Vec<PathBuf>,
}

impl Dirs {
    pub fn from_env() -> Dirs {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| "/".into());
        let data_home = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| home.join(".local/share"));
        let path = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        Dirs {
            flatpak_roots: vec![PathBuf::from("/var/lib/flatpak"), data_home.join("flatpak")],
            data_home,
            path,
        }
    }

    fn applications(&self) -> PathBuf {
        self.data_home.join("applications")
    }

    fn icons(&self) -> PathBuf {
        self.data_home.join("shrimp/webapps")
    }

    pub fn desktop_file(&self, id: &str) -> PathBuf {
        self.applications().join(format!("{PREFIX}{id}.desktop"))
    }
}

pub fn in_sandbox() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn in_path(dirs: &Dirs, cmd: &str) -> Option<PathBuf> {
    dirs.path.iter().map(|d| d.join(cmd)).find(|p| p.is_file())
}

/// Serwis z DRM (adres na liście albo w jej poddomenie).
pub fn needs_drm(url: &str) -> bool {
    let host = catalog::host_of(url).to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    DRM_HOSTS.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
}

/// Czym otworzy się aplikacja pod adresem `url`: okno Shrimp, a dla DRM przeglądarka z Chromium.
pub fn choose(dirs: &Dirs, url: &str) -> Browser {
    if !needs_drm(url) {
        if let Some(p) = in_path(dirs, SHRIMP_WEBAPP) {
            return Browser::Shrimp(p);
        }
    }
    detect_browser(dirs)
}

/// Klasa okna `shrimp-webapp` (identyfikator aplikacji GTK, jak `nav::app_id` w webapp-window).
pub fn shrimp_wm_class(id: &str) -> String {
    let v: String = id.to_ascii_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let v = v.trim_matches('-');
    let mut seg = if v.is_empty() { "app".to_string() } else { v.replace('-', "_") };
    if seg.starts_with(|c: char| c.is_ascii_digit()) {
        seg.insert(0, '_');
    }
    format!("org.shrimp.WebApp.{seg}")
}

/// Przeglądarka z rodziny Chromium (tryb aplikacji) albo domyślna.
pub fn detect_browser(dirs: &Dirs) -> Browser {
    for (flatpak, _) in CHROMIUM_FAMILY {
        if dirs.flatpak_roots.iter().any(|r| r.join("app").join(flatpak).join("current").exists()) {
            return Browser::Flatpak(flatpak.to_string());
        }
    }
    for (_, cmds) in CHROMIUM_FAMILY {
        for cmd in *cmds {
            if let Some(p) = in_path(dirs, cmd) {
                return Browser::Host(p);
            }
        }
    }
    Browser::Default
}

/// Klasa okna, którą Chromium nadaje oknu `--app=URL` (żeby GNOME przypiął okno do ikony).
pub fn chromium_wm_class(url: &str) -> String {
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let rest = rest.split(['?', '#']).next().unwrap_or("");
    let (host, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    format!("chrome-{}_{}-Default", host, path.replace('/', "_"))
}

/// Cytowanie argumentu w linii Exec= (specyfikacja Desktop Entry).
fn exec_arg(s: &str) -> String {
    let needs = s.is_empty() || s.chars().any(|c| " \t\n\"'\\><~|&;$*?#()`%=".contains(c));
    let s = s.replace('%', "%%");
    if !needs {
        return s;
    }
    let mut out = String::from("\"");
    for c in s.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out.push('"');
    // W pliku .desktop odwrotny ukośnik w wartości też trzeba podwoić.
    out.replace('\\', "\\\\")
}

/// Wartość klucza w pliku .desktop (bez nowych linii).
fn value(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', " ").replace('\r', "")
}

fn exec_line(browser: &Browser, app: &WebApp) -> String {
    let url = app.url.as_str();
    let arg = format!("--app={url}");
    let app_arg = &arg;
    match browser {
        Browser::Shrimp(p) => format!(
            "{} --id {} --name {} --url {}",
            exec_arg(&p.to_string_lossy()),
            exec_arg(&app.id),
            exec_arg(&app.name),
            exec_arg(url)
        ),
        Browser::Flatpak(id) => format!("flatpak run {} {}", exec_arg(id), exec_arg(app_arg)),
        Browser::Host(p) => format!("{} {}", exec_arg(&p.to_string_lossy()), exec_arg(app_arg)),
        Browser::Default => format!("xdg-open {}", exec_arg(url)),
    }
}

fn desktop_categories(cat: &str) -> &'static str {
    match cat {
        "grafika" => "Graphics;",
        "wideo" => "AudioVideo;Video;",
        "muzyka" => "AudioVideo;Audio;",
        "gry" => "Game;",
        "biuro" => "Office;",
        "programowanie" => "Development;",
        "narzedzia" => "Utility;",
        _ => "Network;",
    }
}

/// Tworzy aplikację (albo nadpisuje istniejącą). Zwraca ścieżkę pliku `.desktop`.
pub fn create(app: &WebApp) -> Result<PathBuf, Error> {
    if in_sandbox() {
        return Err(Error::Sandboxed);
    }
    let dirs = Dirs::from_env();
    create_in(&dirs, app, &choose(&dirs, &app.url))
}

pub fn create_in(dirs: &Dirs, app: &WebApp, browser: &Browser) -> Result<PathBuf, Error> {
    if catalog::normalize_url(&app.url).is_none() || !app.url.contains("://") {
        return Err(Error::InvalidUrl);
    }
    fs::create_dir_all(dirs.applications())?;
    fs::create_dir_all(dirs.icons())?;
    // Ikona: wbudowana z katalogu albo litera; stara wersja o innym rozszerzeniu znika.
    let (icon_file, bytes) = match app.icon_png() {
        Some(png) => (dirs.icons().join(format!("{}.png", app.id)), png.to_vec()),
        None => (dirs.icons().join(format!("{}.svg", app.id)), app.letter_svg().into_bytes()),
    };
    for ext in ["png", "svg"] {
        let other = dirs.icons().join(format!("{}.{ext}", app.id));
        if other != icon_file {
            let _ = fs::remove_file(other);
        }
    }
    write_atomic(&icon_file, &bytes)?;

    let mut d = String::from("[Desktop Entry]\nType=Application\nVersion=1.0\n");
    d += &format!("Name={}\n", value(&app.name));
    let mut names: Vec<_> = app.names.iter().collect();
    names.sort();
    for (lang, n) in names {
        d += &format!("Name[{lang}]={}\n", value(n));
    }
    d += &format!("Comment={}\n", value(&catalog::host_of(&app.url)));
    d += &format!("Exec={}\n", exec_line(browser, app));
    d += &format!("Icon={}\n", value(&icon_file.to_string_lossy()));
    d += "Terminal=false\nStartupNotify=true\n";
    match browser {
        Browser::Shrimp(_) => d += &format!("StartupWMClass={}\n", value(&shrimp_wm_class(&app.id))),
        Browser::Flatpak(_) | Browser::Host(_) => d += &format!("StartupWMClass={}\n", value(&chromium_wm_class(&app.url))),
        Browser::Default => {}
    }
    d += &format!("Categories={}\n", desktop_categories(&app.category));
    d += &format!("X-Shrimp-WebApp={}\n", value(&app.id));
    d += &format!("X-Shrimp-WebApp-Url={}\n", value(&app.url));
    let path = dirs.desktop_file(&app.id);
    write_atomic(&path, d.as_bytes())?;
    Ok(path)
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

pub fn is_created(id: &str) -> bool {
    Dirs::from_env().desktop_file(id).exists()
}

pub fn remove(id: &str) -> Result<(), Error> {
    remove_in(&Dirs::from_env(), id)
}

pub fn remove_in(dirs: &Dirs, id: &str) -> Result<(), Error> {
    match fs::remove_file(dirs.desktop_file(id)) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    for ext in ["png", "svg"] {
        let _ = fs::remove_file(dirs.icons().join(format!("{id}.{ext}")));
    }
    // Dane okna shrimp-webapp (logowanie, ciasteczka); identyfikator bez ukośników i kropek na początku.
    if !id.is_empty() && !id.contains(['/', '\\']) && !id.starts_with('.') {
        let _ = fs::remove_dir_all(dirs.icons().join(id));
    }
    Ok(())
}

/// Wszystkie aplikacje internetowe utworzone przez Shrimp Apps (do sekcji „Zainstalowane”).
pub fn created() -> Vec<Created> {
    created_in(&Dirs::from_env())
}

pub fn created_in(dirs: &Dirs) -> Vec<Created> {
    let Ok(rd) = fs::read_dir(dirs.applications()) else { return vec![] };
    let mut out: Vec<Created> = rd
        .filter_map(|e| e.ok())
        .filter(|e| {
            let n = e.file_name();
            let n = n.to_string_lossy();
            n.starts_with(PREFIX) && n.ends_with(".desktop")
        })
        .filter_map(|e| parse_desktop(&e.path()))
        .collect();
    out.sort_by_key(|a| a.name.to_lowercase());
    out
}

fn parse_desktop(path: &Path) -> Option<Created> {
    let txt = fs::read_to_string(path).ok()?;
    let get = |k: &str| {
        txt.lines()
            .find_map(|l| l.strip_prefix(k).and_then(|r| r.strip_prefix('=')))
            .map(|v| v.replace("\\\\", "\\"))
    };
    let lang = crate::i18n::lang();
    Some(Created {
        id: get("X-Shrimp-WebApp")?,
        name: get(&format!("Name[{lang}]")).or_else(|| get("Name"))?,
        url: get("X-Shrimp-WebApp-Url")?,
        icon_file: PathBuf::from(get("Icon")?),
        desktop_file: path.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dirs(t: &tempfile::TempDir) -> Dirs {
        Dirs { data_home: t.path().join("share"), flatpak_roots: vec![t.path().join("fp")], path: vec![t.path().join("bin")] }
    }

    #[test]
    fn wm_class_matches_chromium() {
        assert_eq!(chromium_wm_class("https://web.whatsapp.com/"), "chrome-web.whatsapp.com__-Default");
        assert_eq!(chromium_wm_class("https://discord.com/app"), "chrome-discord.com__app-Default");
        assert_eq!(chromium_wm_class("https://x.com"), "chrome-x.com__-Default");
    }

    #[test]
    fn exec_quoting() {
        assert_eq!(exec_arg("--app=https://a.b/"), "\"--app=https://a.b/\"");
        assert_eq!(exec_arg("chromium"), "chromium");
        assert_eq!(exec_arg("https://a.b/?q=100%"), "\"https://a.b/?q=100%%\"");
        assert_eq!(exec_arg("a$b"), "\"a\\\\$b\"");
    }

    #[test]
    fn browser_detection_prefers_flatpak_chromium() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(&t);
        assert_eq!(detect_browser(&d), Browser::Default);
        fs::create_dir_all(t.path().join("bin")).unwrap();
        fs::write(t.path().join("bin/google-chrome"), "").unwrap();
        assert_eq!(detect_browser(&d), Browser::Host(t.path().join("bin/google-chrome")));
        fs::create_dir_all(t.path().join("fp/app/com.brave.Browser/current")).unwrap();
        assert_eq!(detect_browser(&d), Browser::Flatpak("com.brave.Browser".into()));
    }

    #[test]
    fn shrimp_window_except_drm() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(&t);
        fs::create_dir_all(t.path().join("bin")).unwrap();
        fs::write(t.path().join("bin/chromium"), "").unwrap();
        assert_eq!(choose(&d, "https://web.whatsapp.com/"), Browser::Host(t.path().join("bin/chromium")));
        fs::write(t.path().join("bin/shrimp-webapp"), "").unwrap();
        assert_eq!(choose(&d, "https://web.whatsapp.com/"), Browser::Shrimp(t.path().join("bin/shrimp-webapp")));
        assert_eq!(choose(&d, "https://www.netflix.com/"), Browser::Host(t.path().join("bin/chromium")));
        assert_eq!(choose(&d, "https://open.spotify.com/"), Browser::Host(t.path().join("bin/chromium")));
        assert!(!needs_drm("https://spotify.com.evil.example/"));
        assert!(!needs_drm("https://notnetflix.com/"));
        assert_eq!(shrimp_wm_class("google-maps"), "org.shrimp.WebApp.google_maps");
        assert_eq!(shrimp_wm_class("9gag"), "org.shrimp.WebApp._9gag");
        let app = catalog::catalog().iter().find(|a| a.id == "google-maps").unwrap();
        let b = Browser::Shrimp(t.path().join("bin/shrimp-webapp"));
        let p = create_in(&d, app, &b).unwrap();
        let txt = fs::read_to_string(&p).unwrap();
        assert!(txt.contains(" --id google-maps --name "), "{txt}");
        assert!(txt.contains(" --url https://www.google.com/maps\n"), "{txt}");
        assert!(txt.contains("StartupWMClass=org.shrimp.WebApp.google_maps\n"), "{txt}");
        fs::create_dir_all(d.icons().join("google-maps/data")).unwrap();
        remove_in(&d, "google-maps").unwrap();
        assert!(!d.icons().join("google-maps").exists());
    }

    #[test]
    fn create_list_remove_every_catalog_app() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(&t);
        let b = Browser::Flatpak("org.chromium.Chromium".into());
        for app in catalog::catalog() {
            let p = create_in(&d, app, &b).unwrap();
            let txt = fs::read_to_string(&p).unwrap();
            assert!(txt.contains(&format!("Exec=flatpak run org.chromium.Chromium \"--app={}\"", app.url.replace('%', "%%"))), "{txt}");
            assert!(txt.contains(&format!("X-Shrimp-WebApp={}\n", app.id)));
        }
        let list = created_in(&d);
        assert_eq!(list.len(), 150);
        let gmaps = list.iter().find(|c| c.id == "google-maps").unwrap();
        assert_eq!(gmaps.url, "https://www.google.com/maps");
        assert!(gmaps.icon_file.exists());
        for app in catalog::catalog() {
            remove_in(&d, &app.id).unwrap();
        }
        assert!(created_in(&d).is_empty());
        assert_eq!(fs::read_dir(d.icons()).unwrap().count(), 0);
    }

    #[test]
    fn custom_app_and_default_browser() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(&t);
        let app = WebApp::custom("Mój \"serwis\"", &catalog::normalize_url("example.org/a b").unwrap_or("https://example.org/ab".into()));
        let p = create_in(&d, &app, &Browser::Default).unwrap();
        let txt = fs::read_to_string(p).unwrap();
        assert!(txt.contains("Exec=xdg-open https://example.org/ab\n"), "{txt}");
        assert!(!txt.contains("StartupWMClass"));
        assert!(txt.contains("Name=Mój \"serwis\"\n"));
        assert!(txt.contains(".svg\n"), "litera jako ikona: {txt}");
        let bad = WebApp::custom("x", "javascript:alert(1)");
        assert!(matches!(create_in(&d, &bad, &Browser::Default), Err(Error::InvalidUrl)));
    }
}
