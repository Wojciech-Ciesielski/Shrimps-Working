//! Dane Panko niezależne od interfejsu: migawki, ich nazwy i grupy dni, zmiany plików,
//! ustawienia. Wszystko tu jest czyste i ma testy.

use std::collections::HashMap;

/// Konfiguracja migawek dla jednego woluminu (np. "root" dla /, "home" dla /home).
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub name: String,
    pub subvolume: String,
    pub values: HashMap<String, String>,
}

impl Config {
    pub fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.get(key), "yes" | "true" | "1")
    }
    /// Liczba z ustawienia. Zakres "2-10" daje górną granicę.
    pub fn number(&self, key: &str) -> u32 {
        let v = self.get(key);
        let v = v.rsplit('-').next().unwrap_or("");
        v.trim().parse().unwrap_or(0)
    }
    /// SPACE_LIMIT to ułamek (0.5); w interfejsie procenty.
    pub fn percent(&self, key: &str) -> u32 {
        self.get(key)
            .trim()
            .parse::<f64>()
            .map(|f| (f * 100.0).round() as u32)
            .unwrap_or(0)
    }
    pub fn users(&self, key: &str) -> Vec<String> {
        self.get(key)
            .split_whitespace()
            .map(str::to_owned)
            .collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapType {
    Single,
    Pre,
    Post,
}

impl SnapType {
    pub fn from_dbus(v: u16) -> Self {
        match v {
            1 => SnapType::Pre,
            2 => SnapType::Post,
            _ => SnapType::Single,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub num: u32,
    pub kind: SnapType,
    pub pre_num: u32,
    pub date: i64,
    /// Kto utworzył migawkę (na razie nieużywane w interfejsie).
    #[allow(dead_code)]
    pub uid: u32,
    pub description: String,
    pub cleanup: String,
    pub userdata: HashMap<String, String>,
}

/// Rodzaj migawki widoczny dla użytkownika: nazwa i ikona.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Category {
    AfterUpdate,
    BeforeUpdate,
    Timeline,
    BeforeRestore,
    AfterRestore,
    Install,
    Manual(String),
}

impl Category {
    pub fn title(&self) -> String {
        match self {
            Category::AfterUpdate => crate::i18n::tr("category.after-update").into(),
            Category::BeforeUpdate => crate::i18n::tr("category.before-update").into(),
            Category::Timeline => crate::i18n::tr("category.timeline").into(),
            Category::BeforeRestore => crate::i18n::tr("category.before-restore").into(),
            Category::AfterRestore => crate::i18n::tr("category.after-restore").into(),
            Category::Install => crate::i18n::tr("category.install").into(),
            Category::Manual(d) if d.trim().is_empty() => crate::i18n::tr("category.manual").into(),
            Category::Manual(d) => d.clone(),
        }
    }
    pub fn icon(&self) -> &'static str {
        match self {
            Category::AfterUpdate | Category::AfterRestore => "object-select-symbolic",
            Category::BeforeUpdate => "software-update-available-symbolic",
            Category::Timeline => "document-open-recent-symbolic",
            Category::BeforeRestore => "edit-undo-symbolic",
            Category::Install => "computer-symbolic",
            Category::Manual(_) => "camera-photo-symbolic",
        }
    }
}

impl Snapshot {
    pub fn category(&self) -> Category {
        let d = self.description.to_lowercase();
        if self.cleanup == "timeline" {
            return Category::Timeline;
        }
        if d.starts_with("rollback backup") {
            return Category::BeforeRestore;
        }
        if d.starts_with("writable copy") || d.starts_with("rollback to") {
            return Category::AfterRestore;
        }
        if d == "first root filesystem" || d == "after installation" {
            return Category::Install;
        }
        // Własne migawki Panko i innych narzędzi Shrimp niosą opis użytkownika.
        if self.userdata.contains_key("panko") {
            return Category::Manual(self.description.clone());
        }
        match self.kind {
            SnapType::Pre => Category::BeforeUpdate,
            SnapType::Post => Category::AfterUpdate,
            // transactional-update: "Snapshot Update of #N" = nowy system po aktualizacji.
            SnapType::Single if d.starts_with("snapshot update") => Category::AfterUpdate,
            SnapType::Single if d.contains("zypp") || d.contains("transactional") => {
                Category::BeforeUpdate
            }
            SnapType::Single => Category::Manual(self.description.clone()),
        }
    }

    /// Tekst wyszukiwania: nazwa widoczna dla użytkownika.
    pub fn search_text(&self) -> String {
        self.category().title().to_lowercase()
    }
}

/// Czas lokalny rozłożony na części (wypełnia go interfejs przez GLib, testy ręcznie).
#[derive(Clone, Copy, Debug)]
pub struct LocalTime {
    pub ts: i64,
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// 1 = poniedziałek … 7 = niedziela
    pub weekday: u32,
    pub hour: u32,
    pub minute: u32,
}

const WEEKDAYS: [&str; 7] = ["weekday.1", "weekday.2", "weekday.3", "weekday.4", "weekday.5", "weekday.6", "weekday.7"];
const MONTHS: [&str; 12] = [
    "month.1",
    "month.2",
    "month.3",
    "month.4",
    "month.5",
    "month.6",
    "month.7",
    "month.8",
    "month.9",
    "month.10",
    "month.11",
    "month.12",
];

/// Grupa dni na liście migawek. Kolejność wariantów = kolejność na liście.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DayGroup {
    Today,
    Yesterday,
    LastWeek,
    /// (rok, miesiąc) w odwrotnej kolejności, żeby nowsze były wyżej.
    Month(std::cmp::Reverse<(i32, u32)>),
}

impl DayGroup {
    pub fn of(t: &LocalTime, start_of_today: i64) -> Self {
        const DAY: i64 = 86_400;
        if t.ts >= start_of_today {
            DayGroup::Today
        } else if t.ts >= start_of_today - DAY {
            DayGroup::Yesterday
        } else if t.ts >= start_of_today - 7 * DAY {
            DayGroup::LastWeek
        } else {
            DayGroup::Month(std::cmp::Reverse((t.year, t.month)))
        }
    }

    pub fn title(&self) -> String {
        match self {
            DayGroup::Today => crate::i18n::tr("day.today").into(),
            DayGroup::Yesterday => crate::i18n::tr("day.yesterday").into(),
            DayGroup::LastWeek => crate::i18n::tr("day.week").into(),
            DayGroup::Month(std::cmp::Reverse((y, m))) => {
                crate::i18n::trf("day.month", &[("month", crate::i18n::tr(MONTHS[(*m as usize).clamp(1, 12) - 1])), ("year", &y.to_string())])
            }
        }
    }

    /// Godzina pokazywana w wierszu.
    pub fn time_label(&self, t: &LocalTime) -> String {
        let hm = format!("{:02}:{:02}", t.hour, t.minute);
        match self {
            DayGroup::Today | DayGroup::Yesterday => hm,
            DayGroup::LastWeek => {
                format!("{} {hm}", crate::i18n::tr(WEEKDAYS[(t.weekday as usize).clamp(1, 7) - 1]))
            }
            DayGroup::Month(_) => format!("{}.{:02} {hm}", t.day, t.month),
        }
    }

    /// Kiedy, w zdaniu: „dziś 09:21”, „wczoraj 19:42”, „czw. 08:55”.
    pub fn when(&self, t: &LocalTime) -> String {
        match self {
            DayGroup::Today => crate::i18n::trf("day.when-today", &[("time", &self.time_label(t))]),
            DayGroup::Yesterday => crate::i18n::trf("day.when-yesterday", &[("time", &self.time_label(t))]),
            _ => self.time_label(t),
        }
    }
}

/// Zmiana jednego pliku między migawką a obecnym systemem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub status: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Marker {
    Added,
    Removed,
    Changed,
}

impl Marker {
    pub fn icon(self) -> &'static str {
        match self {
            Marker::Added => "list-add-symbolic",
            Marker::Removed => "list-remove-symbolic",
            Marker::Changed => "document-edit-symbolic",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Marker::Added => crate::i18n::tr("marker.added"),
            Marker::Removed => crate::i18n::tr("marker.removed"),
            Marker::Changed => crate::i18n::tr("marker.changed"),
        }
    }
    pub fn css(self) -> &'static str {
        match self {
            Marker::Added => "mk-add",
            Marker::Removed => "mk-rem",
            Marker::Changed => "mk-chg",
        }
    }
}

const CREATED: u32 = 1;
const DELETED: u32 = 2;

/// Zob. [`FileChange::restorable`].
pub fn restorable_path(p: &str) -> bool {
    let in_usr = p == "/usr" || p.starts_with("/usr/");
    let local = p == "/usr/local" || p.starts_with("/usr/local/");
    !in_usr || local
}

impl FileChange {
    /// Porównanie (migawka → obecny system): CREATED = plik doszedł po migawce.
    pub fn marker(&self) -> Marker {
        if self.status & CREATED != 0 {
            Marker::Added
        } else if self.status & DELETED != 0 {
            Marker::Removed
        } else {
            Marker::Changed
        }
    }

    /// Czy plik da się przywrócić pojedynczo. /usr (poza /usr/local) jest na systemie niezmiennym
    /// tylko do odczytu, więc `snapper undochange` zawsze tam zawodzi; właściwe jest „Przywróć system”.
    pub fn restorable(&self) -> bool {
        restorable_path(&self.path)
    }

    /// Katalog grupy i reszta ścieżki: "/etc/zypp/a.repo" → ("/etc", "zypp/a.repo"),
    /// "/usr/bin/ninja" → ("/usr/bin", "ninja"). W /usr, /var i /opt grupą są dwa poziomy.
    pub fn split(&self) -> (String, String) {
        let p = self.path.trim_start_matches('/');
        let parts: Vec<&str> = p.split('/').collect();
        if parts.len() <= 1 {
            return ("/".into(), p.into());
        }
        let deep = matches!(parts[0], "usr" | "var" | "opt" | "srv") && parts.len() > 2;
        let n = if deep { 2 } else { 1 };
        (format!("/{}", parts[..n].join("/")), parts[n..].join("/"))
    }
}

/// Grupuje zmiany według katalogu, katalogi alfabetycznie, pliki alfabetycznie.
pub fn group_changes(changes: &[FileChange]) -> Vec<(String, Vec<(String, FileChange)>)> {
    let mut map: std::collections::BTreeMap<String, Vec<(String, FileChange)>> = Default::default();
    for c in changes {
        let (g, rest) = c.split();
        map.entry(g).or_default().push((rest, c.clone()));
    }
    let mut out: Vec<_> = map.into_iter().collect();
    for (_, v) in &mut out {
        v.sort_by_key(|a| a.0.to_lowercase());
    }
    out
}

/// Ustawienia przechowywania na stronie Zaawansowane (klucz, nazwa jako klucz tłumaczenia).
pub const LIMITS: [(&str, &str); 6] = [
    ("TIMELINE_LIMIT_HOURLY", "limit.hourly"),
    ("TIMELINE_LIMIT_DAILY", "limit.daily"),
    ("TIMELINE_LIMIT_WEEKLY", "limit.weekly"),
    ("TIMELINE_LIMIT_MONTHLY", "limit.monthly"),
    ("TIMELINE_LIMIT_YEARLY", "limit.yearly"),
    ("NUMBER_LIMIT", "limit.number"),
];

/// Nazwa woluminu dla użytkownika.
pub fn volume_title(c: &Config) -> String {
    match c.subvolume.as_str() {
        "/" => crate::i18n::tr("volume.system").into(),
        s => s.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(kind: SnapType, desc: &str, cleanup: &str) -> Snapshot {
        Snapshot {
            num: 1,
            kind,
            pre_num: 0,
            date: 0,
            uid: 0,
            description: desc.into(),
            cleanup: cleanup.into(),
            userdata: HashMap::new(),
        }
    }

    #[test]
    fn categories() {
        assert_eq!(
            snap(SnapType::Single, "timeline", "timeline").category(),
            Category::Timeline
        );
        assert_eq!(
            snap(SnapType::Pre, "zypp(zypper)", "number").category(),
            Category::BeforeUpdate
        );
        assert_eq!(
            snap(SnapType::Post, "", "number").category(),
            Category::AfterUpdate
        );
        assert_eq!(
            snap(SnapType::Single, "Snapshot Update of #5", "number").category(),
            Category::AfterUpdate
        );
        assert_eq!(
            snap(SnapType::Single, "rollback backup of #7", "number").category(),
            Category::BeforeRestore
        );
        assert_eq!(
            snap(SnapType::Single, "first root filesystem", "").category(),
            Category::Install
        );
        let mut s = snap(SnapType::Single, "Przed zmianą ustawień sieci", "number");
        s.userdata.insert("panko".into(), "yes".into());
        assert_eq!(s.category().title(), "Przed zmianą ustawień sieci");
        assert_eq!(s.category().icon(), "camera-photo-symbolic");
        assert_eq!(Category::Manual(" ".into()).title(), crate::i18n::tr("category.manual"));
    }

    fn lt(ts: i64, weekday: u32, hour: u32) -> LocalTime {
        LocalTime {
            ts,
            year: 2026,
            month: 10,
            day: 3,
            weekday,
            hour,
            minute: 5,
        }
    }

    #[test]
    fn day_groups() {
        let today = 1_000_000;
        let g = DayGroup::of(&lt(today + 10, 6, 9), today);
        assert_eq!(g, DayGroup::Today);
        assert_eq!(g.when(&lt(today + 10, 6, 9)), "dziś 09:05");
        let g = DayGroup::of(&lt(today - 10, 5, 19), today);
        assert_eq!(g.title(), "Wczoraj");
        let t = lt(today - 3 * 86_400, 4, 8);
        let g = DayGroup::of(&t, today);
        assert_eq!(g.title(), "Ostatni tydzień");
        assert_eq!(g.time_label(&t), "czw. 08:05");
        let t = lt(today - 30 * 86_400, 4, 8);
        let g = DayGroup::of(&t, today);
        assert_eq!(g.title(), "Październik 2026");
        assert_eq!(g.time_label(&t), "3.10 08:05");
        assert!(DayGroup::Today < DayGroup::LastWeek);
        assert!(
            DayGroup::Month(std::cmp::Reverse((2026, 10)))
                < DayGroup::Month(std::cmp::Reverse((2026, 9)))
        );
    }

    #[test]
    fn change_groups() {
        let c = |p: &str, s| FileChange {
            path: p.into(),
            status: s,
        };
        assert_eq!(
            c("/etc/zypp/repos.d/a.repo", 8).split(),
            ("/etc".into(), "zypp/repos.d/a.repo".into())
        );
        assert_eq!(
            c("/usr/bin/ninja", 1).split(),
            ("/usr/bin".into(), "ninja".into())
        );
        assert!(!c("/usr/bin/ninja", 1).restorable());
        assert!(!c("/usr", 1).restorable());
        assert!(c("/usr/local/bin/x", 1).restorable());
        assert!(c("/usrx/a", 1).restorable());
        assert!(c("/etc/hosts", 1).restorable());
        assert_eq!(c("/usr/x", 1).split(), ("/usr".into(), "x".into()));
        assert_eq!(c("/vmlinuz", 1).split(), ("/".into(), "vmlinuz".into()));
        assert_eq!(c("/a", 1).marker(), Marker::Added);
        assert_eq!(c("/a", 2).marker(), Marker::Removed);
        assert_eq!(c("/a", 8 | 16).marker(), Marker::Changed);
        let g = group_changes(&[c("/usr/bin/b", 1), c("/etc/hosts", 8), c("/usr/bin/A", 2)]);
        assert_eq!(g[0].0, "/etc");
        assert_eq!(g[1].1[0].0, "A");
    }

    #[test]
    fn config_values() {
        let mut c = Config::default();
        c.values.insert("NUMBER_LIMIT".into(), "2-10".into());
        c.values.insert("SPACE_LIMIT".into(), "0.5".into());
        c.values
            .insert("EMPTY_PRE_POST_CLEANUP".into(), "yes".into());
        c.values.insert("ALLOW_USERS".into(), "ala  ola".into());
        assert_eq!(c.number("NUMBER_LIMIT"), 10);
        assert_eq!(c.percent("SPACE_LIMIT"), 50);
        assert!(c.flag("EMPTY_PRE_POST_CLEANUP"));
        assert_eq!(c.users("ALLOW_USERS"), vec!["ala", "ola"]);
        assert_eq!(c.number("BRAK"), 0);
    }
}
