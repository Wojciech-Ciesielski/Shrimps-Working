//! Instalator programów dla Windows (.exe, .msi) przez Wine we wspólnym kontenerze Ubuntu
//! (ten sam co strona „Z Ubuntu”, src/ubuntu.rs). Przed instalacją okienko pokazuje werdykt
//! Wine AppDB pobrany na żywo: ocenę z najnowszego testu (Platinum…Garbage) i wersję Wine.
//! Skróty, które tworzy instalator (~/.local/share/applications/wine/), dostają na początku
//! `distrobox enter <kontener> --`, bo Wine jest tylko w kontenerze.
use std::path::{Path, PathBuf};

use crate::ubuntu::{self, Data};

const APPDB: &str = "https://appdb.winehq.org/objectManager.php";

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub app: String,
    pub rating: String,
    pub wine: String,
    pub url: String,
}

/// Encje HTML w nazwach z AppDB (&amp;, &#1234;, &#x4E2D;).
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';').filter(|e| *e < 10) else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let c = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some(' '),
            _ => ent
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match c {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Wyniki wyszukiwania: (identyfikator aplikacji, nazwa).
pub fn parse_search(html: &str) -> Vec<(u32, String)> {
    const PAT: &str = "<tr><td><a href=\"https://appdb.winehq.org/objectManager.php?sClass=application&amp;iId=";
    html.split(PAT)
        .skip(1)
        .filter_map(|part| {
            let (id, rest) = part.split_once("\">")?;
            let name = rest.split("</a>").next()?;
            Some((id.parse().ok()?, unescape(name)))
        })
        .collect()
}

/// Wiersze wersji ze strony aplikacji: (ocena, wersja Wine z ostatniego testu).
pub fn parse_versions(html: &str) -> Vec<(String, String)> {
    html.split("<tr  data-donav=")
        .skip(1)
        .filter_map(|row| {
            let row = row.split("</tr>").next()?;
            let (_, after) = row.split_once("<td class=\"")?;
            let (rating, after) = after.split_once('"')?;
            let (_, after) = after.split_once("</td><td align=\"center\">")?;
            let wine = after.split("</td>").next()?.trim();
            matches!(rating, "Platinum" | "Gold" | "Silver" | "Bronze" | "Garbage").then(|| (rating.to_string(), unescape(wine)))
        })
        .collect()
}

/// Porównanie wersji Wine („11.13” > „9.22” > „7.20-staging” > „1.3.37”).
fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).take(3).map(|p| p.parse().unwrap_or(0)).collect()
}

/// Werdykt: ocena z najnowszego testu (najwyższa wersja Wine).
pub fn verdict_from(app: &str, id: u32, versions: &[(String, String)]) -> Option<Verdict> {
    let (rating, wine) = versions.iter().max_by_key(|(_, w)| version_key(w))?;
    Some(Verdict { app: app.to_string(), rating: rating.clone(), wine: wine.clone(), url: format!("{APPDB}?sClass=application&iId={id}") })
}

/// Najlepsze dopasowanie nazwy: dokładne (bez wielkości liter), potem pierwsze zaczynające się od nazwy.
fn best_match(name: &str, hits: &[(u32, String)]) -> Option<(u32, String)> {
    let n = name.to_lowercase();
    hits.iter()
        .find(|(_, h)| h.to_lowercase() == n)
        .or_else(|| hits.iter().find(|(_, h)| h.to_lowercase().starts_with(&n)))
        .or_else(|| (hits.len() == 1).then(|| &hits[0]))
        .cloned()
}

/// Wyszukanie w Wine AppDB (sieć; wołać poza wątkiem GTK). `Ok(None)`: brak wpisu.
pub fn lookup(name: &str) -> Result<Option<Verdict>, String> {
    let url = format!("{APPDB}?bIsQueue=false&bIsRejected=false&sClass=application&sTitle=Browse+Applications&iItemsPerPage=25&iPage=1&sOrderBy=appName&bAscending=true");
    let form = [
        ("iappFamily-appNameOp", "2"),
        ("sappFamily-appNameData", name),
        ("iappVersion-ratingOp", "5"),
        ("iappCategoryOp", "11"),
        ("iappVersion-licenseOp", "5"),
        ("ionlyDownloadableOp", "10"),
        ("sFilterSubmit", ""),
    ];
    let html = crate::net::post_form(&url, &form)?;
    let Some((id, app)) = best_match(name, &parse_search(&html)) else { return Ok(None) };
    let page = crate::net::get_bytes(&format!("{APPDB}?sClass=application&iId={id}"))?;
    Ok(verdict_from(&app, id, &parse_versions(&String::from_utf8_lossy(&page))))
}

/// Prefiks Wine Shrimp (w katalogu domowym, wspólnym z kontenerem).
pub fn prefix() -> PathBuf {
    gtk::glib::user_data_dir().join("shrimp/wine")
}

/// Wine w kontenerze (raz): 32-bitowa architektura i pakiety wine, wine32.
pub fn setup_plan(d: &Data, exists: bool) -> Vec<Vec<String>> {
    let mut p = if exists { vec![] } else { ubuntu::create_plan(d) };
    let enter = |args: &[&str]| -> Vec<String> {
        ["distrobox", "enter", d.container.as_str(), "--"].iter().chain(args).map(|s| s.to_string()).collect()
    };
    p.push(enter(&["sudo", "dpkg", "--add-architecture", "i386"]));
    p.push(enter(&["sudo", "env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-y", "update"]));
    p.push(enter(&["sudo", "env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-y", "install", "--no-install-recommends", "wine", "wine64", "wine32:i386"]));
    p
}

/// Uruchomienie instalatora (.msi przez msiexec).
pub fn run_plan(d: &Data, prefix: &Path, file: &Path) -> Vec<String> {
    let mut c: Vec<String> = ["distrobox", "enter", d.container.as_str(), "--", "env"].iter().map(|s| s.to_string()).collect();
    c.push(format!("WINEPREFIX={}", prefix.display()));
    c.push("wine".into());
    if file.extension().is_some_and(|e| e.eq_ignore_ascii_case("msi")) {
        c.extend(["msiexec".into(), "/i".into()]);
    }
    c.push(file.display().to_string());
    c
}

/// Linia Exec= ze skrótu Wine z przedrostkiem kontenera (None: już owinięta albo nie Wine).
pub fn wrap_exec(line: &str, container: &str) -> Option<String> {
    let cmd = line.strip_prefix("Exec=")?;
    if cmd.contains("distrobox") || !cmd.split_whitespace().any(|w| w == "wine" || w.ends_with("/wine")) {
        return None;
    }
    Some(format!("Exec=distrobox enter {container} -- {cmd}"))
}

/// Przepisuje skróty Wine w `dir` (rekurencyjnie); zwraca liczbę zmienionych plików.
pub fn wrap_shortcuts(dir: &Path, container: &str) -> usize {
    let mut n = 0;
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            n += wrap_shortcuts(&p, container);
            continue;
        }
        if p.extension().is_none_or(|x| x != "desktop") {
            continue;
        }
        let Ok(txt) = std::fs::read_to_string(&p) else { continue };
        let mut changed = false;
        let out: Vec<String> = txt
            .lines()
            .map(|l| match wrap_exec(l, container) {
                Some(w) => {
                    changed = true;
                    w
                }
                None => l.to_string(),
            })
            .collect();
        if changed && std::fs::write(&p, out.join("\n") + "\n").is_ok() {
            n += 1;
        }
    }
    n
}

/// Całość w wątku: Wine w kontenerze (jeśli trzeba), instalator, skróty. Zwraca liczbę skrótów.
pub fn install(file: &Path) -> Result<usize, String> {
    let d = ubuntu::data();
    let exists = ubuntu::container_exists(d);
    let has_wine = exists
        && std::process::Command::new("distrobox")
            .args(["enter", &d.container, "--", "sh", "-c", "command -v wine"])
            .output()
            .is_ok_and(|o| o.status.success());
    if !has_wine {
        ubuntu::run(&setup_plan(d, exists))?;
    }
    let pfx = prefix();
    let _ = std::fs::create_dir_all(&pfx);
    ubuntu::run(&[run_plan(d, &pfx, file)])?;
    Ok(wrap_shortcuts(&gtk::glib::user_data_dir().join("applications/wine"), &d.container))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fragmenty prawdziwych stron AppDB (2026-11, wyszukiwanie POST i strona aplikacji).
    const SEARCH: &str = r#"<p>Showing entry <b>1</b> to <b>1</b> of <b>1</b></p><table class="whq-table whq-table-full"><thead><tr><td>x</td></tr></thead><tr><td><a href="https://appdb.winehq.org/objectManager.php?sClass=application&amp;iId=2983">Notepad++</a></td><td>2983</td><td>Editor</td></tr><tr><td><a href="https://appdb.winehq.org/objectManager.php?sClass=application&amp;iId=7455">&amp;ML</a></td><td>7455</td><td>x</td></tr></table>"#;
    const APP: &str = r#"<table class="whq-table"><thead><tr><td width="80">Version</td></tr></thead><tr  data-donav="https://appdb.winehq.org/objectManager.php?sClass=version&iId=3218"><td><a href="https://appdb.winehq.org/objectManager.php?sClass=version&amp;iId=3218">3.x</a></td><td>old</td><td class="Platinum" align="center">Platinum</td><td align="center">1.3.37</td><td align="center">4</td><td align="center">0</td></tr><tr  data-donav="https://appdb.winehq.org/objectManager.php?sClass=version&iId=7834"><td><a href="https://appdb.winehq.org/objectManager.php?sClass=version&amp;iId=7834">4.x</a></td><td>new</td><td class="Garbage" align="center">Garbage</td><td align="center">11.13</td><td align="center">1</td><td align="center">0</td></tr><tr  data-donav="x"><td><a>5.x</a></td><td>d</td><td class="Silver" align="center">Silver</td><td align="center">9.22-staging</td></tr></table>"#;

    #[test]
    fn parses_appdb() {
        let hits = parse_search(SEARCH);
        assert_eq!(hits, [(2983, "Notepad++".to_string()), (7455, "&ML".to_string())]);
        assert_eq!(best_match("notepad++", &hits), Some((2983, "Notepad++".into())));
        assert_eq!(best_match("Nope", &hits), None);
        let v = parse_versions(APP);
        assert_eq!(v.len(), 3);
        let verdict = verdict_from("IrfanView", 163, &v).unwrap();
        assert_eq!(verdict.rating, "Garbage");
        assert_eq!(verdict.wine, "11.13");
        assert!(verdict.url.ends_with("iId=163"));
        assert_eq!(unescape("&#1044;&#x4E2D;&amp;&bogus x"), "Д中&&bogus x");
        assert!(version_key("11.13") > version_key("9.22-staging"));
        assert!(version_key("7.20-staging") > version_key("1.3.37"));
    }

    #[test]
    fn shortcuts_go_through_container() {
        let l = r#"Exec=env WINEPREFIX="/home/u/.local/share/shrimp/wine" wine C:\\\\users\\\\Public\\\\Desktop\\\\App.lnk"#;
        assert_eq!(wrap_exec(l, "shrimp-ubuntu").unwrap(), format!("Exec=distrobox enter shrimp-ubuntu -- {}", &l[5..]));
        assert_eq!(wrap_exec("Exec=distrobox enter shrimp-ubuntu -- env wine x", "shrimp-ubuntu"), None);
        assert_eq!(wrap_exec("Exec=gedit %U", "shrimp-ubuntu"), None);
        let t = std::env::temp_dir().join(format!("shrimp-wine-{}", std::process::id()));
        std::fs::create_dir_all(t.join("Programs/App")).unwrap();
        std::fs::write(t.join("Programs/App/App.desktop"), format!("[Desktop Entry]\nName=App\n{l}\n")).unwrap();
        assert_eq!(wrap_shortcuts(&t, "shrimp-ubuntu"), 1);
        assert_eq!(wrap_shortcuts(&t, "shrimp-ubuntu"), 0);
        let _ = std::fs::remove_dir_all(&t);
    }

    #[test]
    fn plans() {
        let d = ubuntu::data();
        let p = setup_plan(d, true);
        assert!(p.last().unwrap().contains(&"wine32:i386".to_string()));
        let r = run_plan(d, Path::new("/h/pfx"), Path::new("/h/Pobrane/App Setup.msi"));
        assert_eq!(r[4..], ["env", "WINEPREFIX=/h/pfx", "wine", "msiexec", "/i", "/h/Pobrane/App Setup.msi"]);
        assert_eq!(run_plan(d, Path::new("/p"), Path::new("/x.exe")).last().unwrap(), "/x.exe");
    }
}
