//! Katalog aplikacji z metadanych AppStream zdalnego repozytorium (Flathub).
//!
//! libflatpak pobiera `appstream.xml.gz` do katalogu repozytorium
//! (`Remote::appstream_dir`), obok niego leżą ikony `icons/128x128/<id>.png`.
//! Tu nie ma nic z GTK, więc moduł da się testować bez ekranu.

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::categories::Category;

#[derive(Debug, Clone, Default)]
pub struct Screenshot {
    /// Największy obraz (do powiększenia).
    pub full: String,
    /// Obraz około 624–752 px szerokości (do karuzeli na stronie aplikacji).
    pub medium: String,
    pub caption: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Release {
    pub version: String,
    pub timestamp: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct App {
    pub id: String,
    /// Pełny ref Flatpaka, np. `app/org.gimp.GIMP/x86_64/stable`.
    pub flatpak_ref: String,
    pub name: String,
    pub summary: String,
    /// Opis jako zwykły tekst: akapity rozdzielone pustą linią, punkty z „•”.
    pub description: String,
    pub developer: Option<String>,
    pub license: Option<String>,
    pub homepage: Option<String>,
    pub icon_cached: Option<PathBuf>,
    pub icon_remote: Option<String>,
    pub categories: Vec<String>,
    pub keywords: Vec<String>,
    pub screenshots: Vec<Screenshot>,
    pub releases: Vec<Release>,
    pub brand_light: Option<String>,
    pub brand_dark: Option<String>,
    pub verified: bool,
}

impl App {
    pub fn main_category(&self) -> Option<Category> {
        Category::for_freedesktop(&self.categories)
    }

    pub fn version(&self) -> Option<&str> {
        self.releases.first().map(|r| r.version.as_str())
    }

    /// Ocena dopasowania do zapytania wyszukiwania (0 = brak).
    pub fn search_score(&self, terms: &[String]) -> u32 {
        let name = fold(&self.name);
        let id = self.id.to_lowercase();
        let summary = fold(&self.summary);
        let mut total = 0;
        for t in terms {
            let mut s = 0;
            if name == *t {
                s = 100;
            } else if name.starts_with(t.as_str()) {
                s = 80;
            } else if name.split_whitespace().any(|w| w.starts_with(t.as_str())) {
                s = 60;
            } else if name.contains(t.as_str()) {
                s = 50;
            } else if self.keywords.iter().any(|k| fold(k).starts_with(t.as_str())) {
                s = 30;
            } else if id.contains(t.as_str()) {
                s = 20;
            } else if summary.contains(t.as_str()) {
                s = 10;
            }
            if s == 0 {
                return 0; // każde słowo musi pasować
            }
            total += s;
        }
        total
    }
}

/// Małe litery i bez polskich znaków diakrytycznych, żeby „zolw” znalazło „Żółw”.
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            'á' | 'à' | 'â' | 'ä' | 'ã' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ö' | 'ô' | 'ò' | 'õ' => 'o',
            'ü' | 'ú' | 'ù' | 'û' => 'u',
            other => other,
        })
        .collect()
}

pub fn search_terms(query: &str) -> Vec<String> {
    fold(query).split_whitespace().map(str::to_string).collect()
}

// ---------------------------------------------------------------- mini DOM

#[derive(Debug, Default)]
struct Element {
    name: String,
    attrs: Vec<(String, String)>,
    children: Vec<Node>,
}

#[derive(Debug)]
enum Node {
    Elem(Element),
    Text(String),
}

impl Element {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
    }
    fn lang(&self) -> Option<&str> {
        self.attr("xml:lang")
    }
    fn elems(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Elem(e) => Some(e),
            _ => None,
        })
    }
    fn all<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.elems().filter(move |e| e.name == name)
    }
    fn first(&self, name: &str) -> Option<&Element> {
        self.elems().find(|e| e.name == name)
    }
    fn text(&self) -> String {
        let mut out = String::new();
        for n in &self.children {
            match n {
                Node::Text(t) => out.push_str(t),
                Node::Elem(e) => out.push_str(&e.text()),
            }
        }
        collapse_ws(&out)
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Kolejność języków: np. `["pl_PL", "pl", ""]` (pusty = bez `xml:lang`).
#[derive(Debug, Clone)]
pub struct Langs(Vec<String>);

impl Langs {
    pub fn new<I: IntoIterator<Item = S>, S: AsRef<str>>(names: I) -> Self {
        let mut v: Vec<String> = Vec::new();
        for n in names {
            let n = n.as_ref();
            // „pl_PL.UTF-8@euro” → „pl_PL”
            let n = n.split(['.', '@']).next().unwrap_or("");
            let n = if n == "C" || n == "POSIX" { "" } else { n };
            if !v.iter().any(|x| x == n) {
                v.push(n.to_string());
            }
        }
        if !v.iter().any(|x| x.is_empty()) {
            v.push(String::new());
        }
        Langs(v)
    }

    fn rank(&self, lang: Option<&str>) -> Option<usize> {
        let l = lang.unwrap_or("");
        // „en” i „en_US” traktujemy jak brak tłumaczenia (tekst źródłowy).
        let l = if l == "en" || l == "en_US" { "" } else { l };
        self.0.iter().position(|x| x == l)
    }

    /// Wybiera tekst w najlepszym dostępnym języku spośród elementów o tej samej nazwie.
    fn pick<'a>(&self, elems: impl Iterator<Item = &'a Element>) -> Option<&'a Element> {
        let mut best: Option<(usize, &Element)> = None;
        for e in elems {
            if let Some(r) = self.rank(e.lang()) {
                if best.map_or(true, |(br, _)| r < br) {
                    best = Some((r, e));
                }
            }
        }
        best.map(|(_, e)| e)
    }
}

/// Opis AppStream → zwykły tekst. Obsługuje oba formaty tłumaczeń:
/// `<description xml:lang>` (nowy) i `<p xml:lang>` w jednym opisie (stary, appstream-glib).
fn description_text(descs: &[&Element], langs: &Langs) -> String {
    // Nowy format: kilka <description>, każdy w innym języku.
    let chosen: &Element = match langs.pick(descs.iter().copied()) {
        Some(d) => d,
        None => match descs.first() {
            Some(d) => d,
            None => return String::new(),
        },
    };
    // Stary format: w jednym <description> akapity w wielu językach.
    let best_rank = chosen
        .elems()
        .filter_map(|e| langs.rank(e.lang()))
        .min()
        .unwrap_or(langs.0.len() - 1);
    let mut parts: Vec<String> = Vec::new();
    for e in chosen.elems() {
        let is_list = e.name == "ul" || e.name == "ol";
        if !(is_list && e.lang().is_none()) && langs.rank(e.lang()) != Some(best_rank) {
            continue;
        }
        match e.name.as_str() {
            "p" => parts.push(e.text()),
            "ul" | "ol" => {
                let items: Vec<String> = e
                    .all("li")
                    .filter(|li| langs.rank(li.lang()) == Some(best_rank))
                    .map(|li| format!("• {}", li.text()))
                    .collect();
                if !items.is_empty() {
                    parts.push(items.join("\n"));
                }
            }
            _ => {}
        }
    }
    parts.retain(|p| !p.is_empty());
    parts.join("\n\n")
}

fn component_to_app(c: &Element, langs: &Langs, icon_root: Option<&Path>) -> Option<App> {
    let kind = c.attr("type").unwrap_or("");
    if kind != "desktop-application" && kind != "desktop" {
        return None;
    }
    let id = c.first("id")?.text();
    let id = id.strip_suffix(".desktop").unwrap_or(&id).to_string();
    let bundle = c
        .all("bundle")
        .find(|b| b.attr("type") == Some("flatpak"))
        .map(|b| b.text())?;
    if !bundle.starts_with("app/") {
        return None;
    }
    let name = langs.pick(c.all("name")).map(|e| e.text()).unwrap_or_else(|| id.clone());
    let summary = langs.pick(c.all("summary")).map(|e| e.text()).unwrap_or_default();
    let descs: Vec<&Element> = c.all("description").collect();
    let description = description_text(&descs, langs);

    let developer = c
        .first("developer")
        .and_then(|d| langs.pick(d.all("name")))
        .map(|e| e.text())
        .or_else(|| langs.pick(c.all("developer_name")).map(|e| e.text()))
        .filter(|s| !s.is_empty());

    let mut icon_cached = None;
    let mut icon_remote = None;
    let mut best_cached = 0u32;
    for i in c.all("icon") {
        let h: u32 = i.attr("height").and_then(|v| v.parse().ok()).unwrap_or(0);
        match i.attr("type") {
            Some("cached") => {
                if let Some(root) = icon_root {
                    // Największa ikona do 128 px (64 px gdy nie ma większej).
                    if h <= 128 && h >= best_cached {
                        let dir = if h == 0 { "64x64".to_string() } else { format!("{h}x{h}") };
                        let scale = i.attr("scale").filter(|s| *s != "1");
                        let dir = match scale {
                            Some(s) => format!("{dir}@{s}"),
                            None => dir,
                        };
                        icon_cached = Some(root.join("icons").join(dir).join(i.text()));
                        best_cached = h;
                    }
                }
            }
            Some("remote") => {
                if icon_remote.is_none() || h == 128 {
                    icon_remote = Some(i.text());
                }
            }
            _ => {}
        }
    }

    let categories = c
        .first("categories")
        .map(|cs| cs.all("category").map(|e| e.text()).collect())
        .unwrap_or_default();
    let keywords: Vec<String> = c
        .all("keywords")
        .flat_map(|k| k.all("keyword"))
        .filter(|k| langs.rank(k.lang()).is_some())
        .map(|k| k.text())
        .collect();

    let mut homepage = None;
    for u in c.all("url") {
        match u.attr("type") {
            Some("homepage") => homepage = Some(u.text()),
            _ => {}
        }
    }

    let mut screenshots = Vec::new();
    if let Some(ss) = c.first("screenshots") {
        let mut list: Vec<&Element> = ss.all("screenshot").collect();
        // Domyślny zrzut na początek.
        list.sort_by_key(|s| if s.attr("type") == Some("default") { 0 } else { 1 });
        for s in list {
            let mut images: Vec<(u32, String, bool)> = s
                .all("image")
                .map(|i| {
                    let w = i.attr("width").and_then(|v| v.parse().ok()).unwrap_or(0);
                    (w, i.text(), i.attr("type") == Some("source"))
                })
                .filter(|(_, u, _)| !u.is_empty())
                .collect();
            if images.is_empty() {
                continue;
            }
            images.sort_by_key(|(w, _, _)| *w);
            let full = images
                .iter()
                .find(|(_, _, src)| *src)
                .or(images.last())
                .map(|(_, u, _)| u.clone())
                .unwrap_or_default();
            let medium = images
                .iter()
                .find(|(w, _, _)| *w >= 600)
                .or(images.last())
                .map(|(_, u, _)| u.clone())
                .unwrap_or_default();
            let caption = langs.pick(s.all("caption")).map(|e| e.text()).filter(|c| !c.is_empty());
            screenshots.push(Screenshot { full, medium, caption });
        }
    }

    let mut releases: Vec<Release> = c
        .first("releases")
        .map(|r| {
            r.all("release")
                .map(|e| Release {
                    version: e.attr("version").unwrap_or("").to_string(),
                    timestamp: e.attr("timestamp").and_then(|t| t.parse().ok()),
                })
                .filter(|r| !r.version.is_empty())
                .collect()
        })
        .unwrap_or_default();
    releases.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

    let mut brand_light = None;
    let mut brand_dark = None;
    if let Some(b) = c.first("branding") {
        for col in b.all("color").filter(|e| e.attr("type") == Some("primary")) {
            match col.attr("scheme_preference") {
                Some("dark") => brand_dark = Some(col.text()),
                _ => brand_light = Some(col.text()),
            }
        }
    }

    let verified = c
        .first("custom")
        .map(|cu| {
            cu.all("value")
                .any(|v| v.attr("key") == Some("flathub::verification::verified") && v.text() == "true")
        })
        .unwrap_or(false);

    Some(App {
        id,
        flatpak_ref: bundle,
        name,
        summary,
        description,
        developer,
        license: c.first("project_license").map(|e| e.text()),
        homepage,
        icon_cached: icon_cached.filter(|p| p.exists()),
        icon_remote,
        categories,
        keywords,
        screenshots,
        releases,
        brand_light,
        brand_dark,
        verified,
    })
}

/// Czyta strumień XML AppStream i zwraca aplikacje (bez środowisk i dodatków).
pub fn parse<R: BufRead>(input: R, langs: &Langs, icon_root: Option<&Path>) -> Result<Vec<App>, String> {
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(false);
    let mut buf = Vec::new();
    let mut stack: Vec<Element> = Vec::new();
    let mut apps = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();

    fn start(e: &quick_xml::events::BytesStart) -> Element {
        let name = e.name().as_ref().to_owned();
        let attrs = e
            .attributes()
            .flatten()
            .map(|a| {
                (
                    a.key.as_ref().to_owned(),
                    a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map(|v| v.into_owned()).unwrap_or_default(),
                )
            })
            .collect();
        Element { name, attrs, children: Vec::new() }
    }

    loop {
        let ev = reader.read_event_into(&mut buf).map_err(|e| e.to_string())?;
        match ev {
            Event::Start(e) => {
                let el = start(&e);
                if !stack.is_empty() || el.name == "component" {
                    stack.push(el);
                }
            }
            Event::Empty(e) => {
                if let Some(top) = stack.last_mut() {
                    top.children.push(Node::Elem(start(&e)));
                }
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    let s = t.as_ref().to_owned();
                    top.children.push(Node::Text(s));
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    let name = r.as_ref().to_owned();
                    let s = match name.as_str() {
                        "amp" => "&".to_string(),
                        "lt" => "<".to_string(),
                        "gt" => ">".to_string(),
                        "quot" => "\"".to_string(),
                        "apos" => "'".to_string(),
                        n if n.starts_with("#x") => u32::from_str_radix(&n[2..], 16)
                            .ok()
                            .and_then(char::from_u32)
                            .map(String::from)
                            .unwrap_or_default(),
                        n if n.starts_with('#') => n[1..]
                            .parse::<u32>()
                            .ok()
                            .and_then(char::from_u32)
                            .map(String::from)
                            .unwrap_or_default(),
                        _ => String::new(),
                    };
                    top.children.push(Node::Text(s));
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.children.push(Node::Text(t.as_ref().to_owned()));
                }
            }
            Event::End(_) => {
                if let Some(el) = stack.pop() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(Node::Elem(el));
                    } else if let Some(app) = component_to_app(&el, langs, icon_root) {
                        // Ten sam identyfikator dwa razy: zostaje ostatni.
                        if let Some(&i) = seen.get(&app.id) {
                            apps[i] = app;
                        } else {
                            seen.insert(app.id.clone(), apps.len());
                            apps.push(app);
                        }
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    Ok(apps)
}

/// Wczytuje katalog z katalogu AppStream repozytorium (`appstream.xml.gz` lub `appstream.xml`).
pub fn load_dir(dir: &Path, langs: &Langs) -> Result<Vec<App>, String> {
    let gz = dir.join("appstream.xml.gz");
    let plain = dir.join("appstream.xml");
    if gz.exists() {
        let f = std::fs::File::open(&gz).map_err(|e| e.to_string())?;
        let d = flate2::read::GzDecoder::new(f);
        parse(BufReader::new(d), langs, Some(dir))
    } else if plain.exists() {
        let f = std::fs::File::open(&plain).map_err(|e| e.to_string())?;
        parse(BufReader::new(f), langs, Some(dir))
    } else {
        Err(format!("brak pliku AppStream w {}", dir.display()))
    }
}

#[cfg(test)]
fn parse_str(xml: &str, langs: &Langs) -> Vec<App> {
    parse(BufReader::new(xml.as_bytes()), langs, None).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<components version="0.8" origin="flathub">
  <component type="runtime"><id>org.gnome.Platform</id><bundle type="flatpak">runtime/org.gnome.Platform/x86_64/47</bundle></component>
  <component type="desktop-application">
    <id>org.kde.krita</id>
    <name>Krita</name>
    <name xml:lang="pl">Krita PL</name>
    <summary>Digital Painting</summary>
    <summary xml:lang="pl">Malowanie cyfrowe</summary>
    <description>
      <p>Krita is a painting program &amp; more.</p>
      <p xml:lang="pl">Krita to program do malowania.</p>
      <ul><li>Brushes</li><li xml:lang="pl">Pędzle</li></ul>
    </description>
    <developer id="org.kde"><name>KDE</name></developer>
    <project_license>GPL-3.0+</project_license>
    <icon type="cached" height="64" width="64">org.kde.krita.png</icon>
    <icon type="cached" height="128" width="128">org.kde.krita.png</icon>
    <icon type="remote" height="128" width="128">https://dl.flathub.org/media/krita.png</icon>
    <categories><category>Graphics</category><category>2DGraphics</category></categories>
    <keywords><keyword>paint</keyword><keyword xml:lang="pl">rysowanie</keyword></keywords>
    <url type="homepage">https://krita.org</url>
    <screenshots>
      <screenshot>
        <image type="thumbnail" width="224" height="126">https://x/224.png</image>
        <image type="thumbnail" width="752" height="423">https://x/752.png</image>
        <image type="source" width="1920" height="1080">https://x/src.png</image>
      </screenshot>
      <screenshot type="default"><image type="source">https://x/default.png</image></screenshot>
    </screenshots>
    <releases><release version="5.2.6" timestamp="1700000000"/><release version="5.2.9" timestamp="1720000000"/></releases>
    <branding><color type="primary" scheme_preference="light">#71b5ff</color><color type="primary" scheme_preference="dark">#204a87</color></branding>
    <custom><value key="flathub::verification::verified">true</value></custom>
    <bundle type="flatpak" runtime="org.kde.Platform/x86_64/6.7">app/org.kde.krita/x86_64/stable</bundle>
  </component>
  <component type="desktop-application">
    <id>com.example.New.desktop</id>
    <name>New</name>
    <summary>New format</summary>
    <description><p>English</p></description>
    <description xml:lang="pl"><p>Polski</p><ol><li>Jeden</li></ol></description>
    <developer_name>Ex</developer_name>
    <bundle type="flatpak">app/com.example.New/x86_64/stable</bundle>
  </component>
</components>"#;

    fn pl() -> Langs {
        Langs::new(["pl_PL.UTF-8", "pl_PL", "pl", "C"])
    }

    #[test]
    fn parses_apps_and_skips_runtimes() {
        let apps = parse_str(XML, &pl());
        assert_eq!(apps.len(), 2);
        let k = &apps[0];
        assert_eq!(k.id, "org.kde.krita");
        assert_eq!(k.flatpak_ref, "app/org.kde.krita/x86_64/stable");
        assert_eq!(k.name, "Krita PL");
        assert_eq!(k.summary, "Malowanie cyfrowe");
        assert_eq!(k.description, "Krita to program do malowania.\n\n• Pędzle");
        assert_eq!(k.developer.as_deref(), Some("KDE"));
        assert_eq!(k.version(), Some("5.2.9"));
        assert_eq!(k.screenshots.len(), 2);
        assert_eq!(k.screenshots[0].full, "https://x/default.png");
        assert_eq!(k.screenshots[1].medium, "https://x/752.png");
        assert_eq!(k.screenshots[1].full, "https://x/src.png");
        assert_eq!(k.brand_dark.as_deref(), Some("#204a87"));
        assert!(k.verified);
        assert_eq!(k.main_category(), Some(Category::Graphics));
        assert!(k.keywords.contains(&"rysowanie".to_string()));
        assert_eq!(k.icon_remote.as_deref(), Some("https://dl.flathub.org/media/krita.png"));
    }

    #[test]
    fn new_description_format_and_desktop_suffix() {
        let apps = parse_str(XML, &pl());
        let n = &apps[1];
        assert_eq!(n.id, "com.example.New");
        assert_eq!(n.description, "Polski\n\n• Jeden");
        assert_eq!(n.developer.as_deref(), Some("Ex"));
    }

    #[test]
    fn english_fallback() {
        let apps = parse_str(XML, &Langs::new(["de_DE", "de", "C"]));
        assert_eq!(apps[0].name, "Krita");
        assert_eq!(apps[0].description, "Krita is a painting program & more.\n\n• Brushes");
        assert_eq!(apps[1].description, "English");
    }

    #[test]
    fn search() {
        let apps = parse_str(XML, &pl());
        let k = &apps[0];
        assert!(k.search_score(&search_terms("krita")) > 0);
        assert!(k.search_score(&search_terms("RYSOW")) > 0);
        assert!(k.search_score(&search_terms("malowanie")) > 0);
        assert_eq!(k.search_score(&search_terms("krita xyz")), 0);
        assert_eq!(fold("Żółć"), "zolc");
    }
}
