//! „Usuwaj migawki starsze niż N dni”. Usługa migawek nie ma limitu wieku (tylko limity liczby),
//! więc Panko trzyma własne ustawienie w /etc/panko/panko.conf, a raz dziennie czyści migawki
//! (`panko --cleanup`, jednostka systemd `panko-cleanup.timer`, jako root).

use std::collections::HashSet;

use crate::model::{SnapType, Snapshot};

pub const CONF_PATH: &str = "/etc/panko/panko.conf";
pub const MAX_DAYS: u32 = 999;
const DAY: i64 = 86_400;

/// Wiek z pliku ustawień: `MAX_AGE_DAYS=30`. Brak, 0 albo błąd = wyłączone.
pub fn parse(conf: &str) -> Option<u32> {
    conf.lines()
        .filter_map(|l| l.split_once('='))
        .find(|(k, _)| k.trim() == "MAX_AGE_DAYS")
        .and_then(|(_, v)| v.trim().parse::<u32>().ok())
        .filter(|d| (1..=MAX_DAYS).contains(d))
}

pub fn render(days: Option<u32>) -> String {
    format!(
        "# Ustawienia Panko. Zmieniaj w aplikacji (menu główne).\nMAX_AGE_DAYS={}\n",
        days.map_or(0, |d| d.clamp(1, MAX_DAYS))
    )
}

pub fn read() -> Option<u32> {
    std::fs::read_to_string(CONF_PATH)
        .ok()
        .and_then(|s| parse(&s))
}

/// Zapis jako root (wywoływany przez `pkexec panko --set-max-age`).
pub fn write(days: Option<u32>) -> std::io::Result<()> {
    std::fs::create_dir_all("/etc/panko")?;
    let tmp = format!("{CONF_PATH}.new");
    std::fs::write(&tmp, render(days))?;
    std::fs::rename(tmp, CONF_PATH)
}

/// Numery migawek do usunięcia. Zasady ostrożności:
/// - nigdy najnowsza migawka (zawsze zostaje coś do przywrócenia),
/// - migawka „przed” (pre) znika dopiero, gdy znikają wszystkie jej „po” (post),
/// - bieżącej, domyślnej i aktywnej migawki i tak nie usunie usługa migawek.
pub fn expired(snaps: &[Snapshot], now: i64, days: u32) -> Vec<u32> {
    let limit = now - i64::from(days) * DAY;
    let newest = snaps.iter().map(|s| (s.date, s.num)).max();
    let mut old: HashSet<u32> = snaps
        .iter()
        .filter(|s| s.date < limit && Some((s.date, s.num)) != newest)
        .map(|s| s.num)
        .collect();
    // Para zostaje w całości, dopóki choć jedno „po” jest młodsze od limitu.
    for s in snaps {
        if s.kind == SnapType::Post && !old.contains(&s.num) {
            old.remove(&s.pre_num);
        }
    }
    let mut v: Vec<u32> = snaps
        .iter()
        .map(|s| s.num)
        .filter(|n| old.contains(n))
        .collect();
    v.sort_unstable();
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn snap(num: u32, kind: SnapType, pre: u32, days_ago: i64) -> Snapshot {
        Snapshot {
            num,
            kind,
            pre_num: pre,
            date: 10_000_000 - days_ago * DAY,
            uid: 0,
            description: String::new(),
            cleanup: String::new(),
            userdata: HashMap::new(),
        }
    }
    const NOW: i64 = 10_000_000;

    #[test]
    fn parsing() {
        assert_eq!(parse("MAX_AGE_DAYS=30\n"), Some(30));
        assert_eq!(parse("# x\nMAX_AGE_DAYS = 7"), Some(7));
        assert_eq!(parse("MAX_AGE_DAYS=0"), None);
        assert_eq!(parse("MAX_AGE_DAYS=abc"), None);
        assert_eq!(parse("MAX_AGE_DAYS=5000"), None);
        assert_eq!(parse(""), None);
        assert_eq!(parse(&render(Some(45))), Some(45));
        assert_eq!(parse(&render(None)), None);
    }

    #[test]
    fn old_ones_go_newest_stays() {
        let s = vec![
            snap(1, SnapType::Single, 0, 100),
            snap(2, SnapType::Single, 0, 40),
            snap(3, SnapType::Single, 0, 5),
        ];
        assert_eq!(expired(&s, NOW, 30), vec![1, 2]);
        assert_eq!(expired(&s, NOW, 90), vec![1]);
        assert_eq!(expired(&s, NOW, 200), Vec::<u32>::new());
    }

    #[test]
    fn newest_is_kept_even_when_old() {
        let s = vec![
            snap(1, SnapType::Single, 0, 100),
            snap(2, SnapType::Single, 0, 50),
        ];
        assert_eq!(expired(&s, NOW, 30), vec![1]);
    }

    #[test]
    fn pair_goes_together_or_stays() {
        // pre 2 stare, post 3 młodsze od limitu: para zostaje.
        let s = vec![
            snap(1, SnapType::Single, 0, 1),
            snap(2, SnapType::Pre, 0, 31),
            snap(3, SnapType::Post, 2, 29),
        ];
        assert_eq!(expired(&s, NOW, 30), Vec::<u32>::new());
        // obie strony stare: znikają razem.
        let s = vec![
            snap(1, SnapType::Single, 0, 1),
            snap(2, SnapType::Pre, 0, 40),
            snap(3, SnapType::Post, 2, 39),
        ];
        assert_eq!(expired(&s, NOW, 30), vec![2, 3]);
    }
}
