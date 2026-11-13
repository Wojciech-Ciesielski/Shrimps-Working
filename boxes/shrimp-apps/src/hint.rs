//! Okienka Shrimp przy plikach spoza sklepu:
//!   shrimp-apps --hint download PLIK   instalator właśnie pobrany z internetu (menu Shrimp obserwuje Pobrane)
//!   shrimp-apps --hint file PLIK       otwarty plik .exe/.msi/.deb/.rpm/.dmg/AppImage (skojarzenie typu pliku)
//! W Shrimp programy pochodzą z Flathub przez Shrimp Apps; instalator z innego systemu nie zadziała.
//! „Szukaj w Shrimp Apps” otwiera sklep z wyszukiwaniem nazwy zgadniętej z nazwy pliku.

use adw::prelude::*;
use gtk::{gio, glib};

/// Rodzaj pliku do pokazania w tekście.
pub fn kind(name: &str) -> &'static str {
    let l = name.to_lowercase();
    let ext = l.rsplit('.').next().unwrap_or("");
    match ext {
        "exe" | "msi" | "msix" | "appx" => "windows",
        "dmg" | "pkg" => "mac",
        _ => "linux",
    }
}

/// Nazwa programu do wyszukania w sklepie: „VLC-3.0.21-win64.exe” → „vlc”,
/// „ChromeSetup.exe” → „chrome”, „Steam_Installer x64.msi” → „steam”.
pub fn guess_query(file: &str) -> String {
    let base = file.rsplit(['/', '\\']).next().unwrap_or(file);
    let stem = match base.rfind('.') {
        Some(i) if i > 0 => &base[..i],
        _ => base,
    };
    // CamelCase z doklejonym Setup/Installer: ChromeSetup → Chrome Setup.
    let mut spaced = String::new();
    let mut prev_lower = false;
    for c in stem.chars() {
        if c.is_uppercase() && prev_lower {
            spaced.push(' ');
        }
        prev_lower = c.is_lowercase();
        spaced.push(c);
    }
    const NOISE: &[&str] = &[
        "setup", "installer", "install", "win", "win32", "win64", "windows", "x64", "x86", "amd64", "arm64",
        "portable", "full", "latest", "stub", "online", "offline", "release", "msi", "exe", "appimage", "linux",
    ];
    let mut words = Vec::new();
    for w in spaced.split(|c: char| !c.is_alphanumeric() && c != '+') {
        let w = w.to_lowercase();
        if w.is_empty() || NOISE.contains(&w.as_str()) {
            continue;
        }
        if w.chars().any(|c| c.is_ascii_digit()) {
            if words.is_empty() {
                continue;
            }
            break;
        }
        words.push(w);
        if words.len() == 3 {
            break;
        }
    }
    words.join(" ")
}

pub fn run(args: &[String]) -> glib::ExitCode {
    let (Some(mode), Some(path)) = (args.first(), args.get(1)) else {
        eprintln!("użycie: shrimp-apps --hint download|file PLIK");
        return glib::ExitCode::from(2);
    };
    let download = mode == "download";
    let name = std::path::Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
    let query = guess_query(&name);
    let app = adw::Application::builder()
        .application_id("org.shrimp.Apps.Hint")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        let heading = if download { crate::i18n::tr("hint.download.title") } else { crate::i18n::tr("hint.file.title") };
        let body_key = match (download, kind(&name)) {
            (_, "windows") => "hint.windows",
            (_, "mac") => "hint.mac",
            _ => "hint.linux",
        };
        let mut body = crate::i18n::trf(body_key, &[("file", &name)]);
        body.push_str("\n\n");
        body.push_str(crate::i18n::tr("hint.store"));
        let d = adw::AlertDialog::new(Some(heading), Some(&body));
        d.add_response("close", crate::i18n::tr("hint.close"));
        d.add_response("store", crate::i18n::tr("hint.search"));
        d.set_response_appearance("store", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("store"));
        d.set_close_response("close");
        // Bez okna rodzica libadwaita pokazuje dialog we własnym oknie; aplikacja czeka do odpowiedzi.
        let guard = app.hold();
        let (app2, query) = (app.clone(), query.clone());
        d.connect_response(None, move |_, r| {
            if r == "store" {
                let arg = format!("shrimp-search:{query}");
                let _ = gio::Subprocess::newv(
                    &[std::ffi::OsStr::new("shrimp-apps"), std::ffi::OsStr::new(&arg)],
                    gio::SubprocessFlags::NONE,
                );
            }
            let _ = &guard;
            app2.quit();
        });
        d.present(None::<&gtk::Widget>);
    });
    app.run_with_args::<&str>(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries() {
        assert_eq!(guess_query("VLC-3.0.21-win64.exe"), "vlc");
        assert_eq!(guess_query("ChromeSetup.exe"), "chrome");
        assert_eq!(guess_query("Steam_Installer x64.msi"), "steam");
        assert_eq!(guess_query("/home/a/Pobrane/Discord-0.0.70.deb"), "discord");
        assert_eq!(guess_query("OBS-Studio-31.0-Windows-Installer.exe"), "obs studio");
        assert_eq!(guess_query("7z2409-x64.exe"), "");
        assert_eq!(guess_query("Spotify.AppImage"), "spotify");
    }

    #[test]
    fn kinds() {
        assert_eq!(kind("a.EXE"), "windows");
        assert_eq!(kind("a.dmg"), "mac");
        assert_eq!(kind("a.rpm"), "linux");
    }
}
