//! Dziennik zmian pakietów `/var/log/zypp/history`.
//!
//! Format wiersza: `2026-11-06 10:00:00|install|vlc|3.0.21-1.1|x86_64|root@host|repo|sha|`
//! albo `…|remove |vlc|3.0.21-1.1|x86_64|root@host|`. Aktualizacja to w dzienniku zwykłe
//! „install” nowej wersji, więc rozpoznajemy ją po tym, że pakiet był już zainstalowany.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    Installed,
    Updated { old: String },
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `YYYY-MM-DD`
    pub date: String,
    /// `HH:MM`
    pub time: String,
    pub name: String,
    pub version: String,
    pub kind: Kind,
}

use crate::i18n::trf;

impl Entry {
    pub fn title(&self) -> String {
        match &self.kind {
            Kind::Installed => trf("history.installed", &[("name", &self.name), ("version", short(&self.version))]),
            Kind::Updated { old } => trf(
                "history.updated",
                &[("name", &self.name), ("old", short(old)), ("version", short(&self.version))],
            ),
            Kind::Removed => trf("history.removed", &[("name", &self.name), ("version", short(&self.version))]),
        }
    }
}

/// Wersja bez numeru wydania pakietu: „3.0.21-1.1” → „3.0.21”.
fn short(v: &str) -> &str {
    v.rsplit_once('-').map(|(a, _)| a).unwrap_or(v)
}

/// Wpisy od najnowszego.
pub fn parse(text: &str) -> Vec<Entry> {
    let mut installed: HashMap<String, (String, String)> = HashMap::new(); // nazwa → (wersja, chwila)
    let mut out: Vec<Entry> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('|').collect();
        if f.len() < 4 {
            continue;
        }
        let (stamp, action, name, version) = (f[0].trim(), f[1].trim(), f[2].trim(), f[3].trim());
        let Some((date, time)) = stamp.split_once(' ') else { continue };
        let time = time.get(..5).unwrap_or(time).to_string();
        let kind = match action {
            "install" => match installed.insert(name.to_string(), (version.to_string(), stamp.to_string())) {
                Some((old, _)) if old != version => Kind::Updated { old },
                Some(_) => continue, // ponowna instalacja tej samej wersji
                None => Kind::Installed,
            },
            "remove" => {
                match installed.get(name) {
                    // Usunięcie starej wersji w tej samej chwili co instalacja nowej = część aktualizacji.
                    Some((cur, when)) if cur != version && when == stamp => continue,
                    _ => {}
                }
                installed.remove(name);
                Kind::Removed
            }
            _ => continue,
        };
        out.push(Entry { date: date.to_string(), time, name: name.to_string(), version: version.to_string(), kind });
    }
    out.reverse();
    out
}

/// Dni od 1970-01-01 dla daty kalendarzowej (algorytm „days from civil”).
pub fn days(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.split('-').map(|p| p.parse::<i64>().ok());
    Some(days(it.next()??, it.next()??, it.next()??))
}

/// Grupa dnia jak w makiecie (klucz tłumaczenia): Dziś, Wczoraj, Ostatni tydzień, Wcześniej.
pub fn group(date: &str, today: i64) -> &'static str {
    match parse_date(date).map(|d| today - d) {
        Some(0) => "history.today",
        Some(1) => "history.yesterday",
        Some(n) if (2..=7).contains(&n) => "history.week",
        _ => "history.earlier",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: &str = "\
# 2026-11-01 09:00:00 vlc.rpm installed ok
2026-11-01 09:00:00|command|root@pc|'zypper' 'in' 'vlc'|
2026-11-01 09:00:00|install|vlc|3.0.20-1.1|x86_64|root@pc|oss|abc|
2026-11-01 09:00:00|install|ninja|1.12.1-1.1|x86_64|root@pc|oss|abc|
2026-11-05 10:12:30|install|vlc|3.0.21-1.1|x86_64|root@pc|oss|abc|
2026-11-05 10:12:30|remove |vlc|3.0.20-1.1|x86_64|root@pc|
2026-11-06 11:00:00|remove |ninja|1.12.1-1.1|x86_64|root@pc|
";

    #[test]
    fn entries() {
        let e = parse(LOG);
        assert_eq!(e.len(), 4);
        assert_eq!(e[0].title(), "Usunięto: ninja 1.12.1");
        assert_eq!(e[1].title(), "Zaktualizowano: vlc 3.0.20 → 3.0.21");
        assert_eq!(e[1].time, "10:12");
        assert_eq!(e[3].title(), "Zainstalowano: vlc 3.0.20");
    }

    #[test]
    fn groups() {
        let today = parse_date("2026-11-07").unwrap();
        assert_eq!(group("2026-11-07", today), "history.today");
        assert_eq!(group("2026-11-06", today), "history.yesterday");
        assert_eq!(group("2026-11-01", today), "history.week");
        assert_eq!(group("2026-10-01", today), "history.earlier");
        assert_eq!(crate::i18n::tr(group("2026-11-07", today)), "Dziś");
        assert_eq!(days(1970, 1, 1), 0);
        assert_eq!(parse_date("2026-03-01").unwrap() - parse_date("2026-02-28").unwrap(), 1);
    }
}
