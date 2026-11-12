//! Przykładowe dane z makiety (`panko --demo` albo PANKO_DEMO=1): do podglądu wyglądu,
//! zrzutów ekranu i testów bez usługi migawek. Zmiany żyją tylko w pamięci.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::backend::{Backend, Error, Result};
use crate::model::{Config, FileChange, SnapType, Snapshot};

pub struct Demo {
    state: Mutex<State>,
}

struct State {
    configs: Vec<Config>,
    snaps: Vec<Snapshot>,
    next: u32,
    max_age: Option<u32>,
}

fn config(name: &str, subvolume: &str, extra: &[(&str, &str)]) -> Config {
    let mut values: HashMap<String, String> = [
        ("TIMELINE_CREATE", "yes"),
        ("TIMELINE_LIMIT_HOURLY", "10"),
        ("TIMELINE_LIMIT_DAILY", "7"),
        ("TIMELINE_LIMIT_WEEKLY", "4"),
        ("TIMELINE_LIMIT_MONTHLY", "6"),
        ("TIMELINE_LIMIT_YEARLY", "0"),
        ("NUMBER_LIMIT", "10"),
        ("EMPTY_PRE_POST_CLEANUP", "yes"),
        ("SPACE_LIMIT", "0.5"),
        ("ALLOW_USERS", ""),
    ]
    .iter()
    .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
    .collect();
    for (k, v) in extra {
        values.insert((*k).to_owned(), (*v).to_owned());
    }
    Config {
        name: name.into(),
        subvolume: subvolume.into(),
        values,
    }
}

impl Demo {
    pub fn new() -> Self {
        let now = gtk::glib::DateTime::now_local().expect("czas lokalny");
        let today =
            gtk::glib::DateTime::from_local(now.year(), now.month(), now.day_of_month(), 0, 0, 0.0)
                .expect("północ")
                .to_unix();
        let at = |days: i64, h: i64, m: i64| today - days * 86_400 + h * 3600 + m * 60;
        let mk = |num, kind, date, desc: &str, cleanup: &str, panko: bool| {
            let mut userdata = HashMap::new();
            if panko {
                userdata.insert("panko".to_owned(), "yes".to_owned());
            }
            Snapshot {
                num,
                kind,
                pre_num: 0,
                date,
                uid: 0,
                description: desc.into(),
                cleanup: cleanup.into(),
                userdata,
            }
        };
        let snaps = vec![
            mk(
                1,
                SnapType::Single,
                at(40, 10, 0),
                "first root filesystem",
                "",
                false,
            ),
            mk(
                2,
                SnapType::Single,
                at(6, 12, 0),
                "timeline",
                "timeline",
                false,
            ),
            mk(
                3,
                SnapType::Single,
                at(3, 8, 55),
                "rollback backup of #1",
                "number",
                false,
            ),
            mk(
                4,
                SnapType::Pre,
                at(1, 10, 3),
                "zypp(transactional-update)",
                "number",
                false,
            ),
            mk(
                5,
                SnapType::Single,
                at(1, 18, 0),
                "timeline",
                "timeline",
                false,
            ),
            mk(
                6,
                SnapType::Single,
                at(1, 19, 42),
                "Przed zmianą ustawień sieci",
                "number",
                true,
            ),
            mk(
                7,
                SnapType::Single,
                at(0, 8, 0),
                "timeline",
                "timeline",
                false,
            ),
            mk(
                8,
                SnapType::Pre,
                at(0, 9, 14),
                "zypp(transactional-update)",
                "number",
                false,
            ),
            mk(9, SnapType::Post, at(0, 9, 21), "", "number", false),
        ];
        Demo {
            state: Mutex::new(State {
                // /home bez migawek, jak w makiecie (przełącznik wyłączony).
                configs: vec![config("root", "/", &[])],
                snaps,
                next: 10,
                max_age: None,
            }),
        }
    }
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Backend for Demo {
    fn configs(&self) -> Result<Vec<Config>> {
        Ok(self.state.lock().unwrap().configs.clone())
    }

    fn snapshots(&self, config: &str) -> Result<Vec<Snapshot>> {
        if config != "root" {
            return Ok(vec![]);
        }
        Ok(self.state.lock().unwrap().snaps.clone())
    }

    fn create(&self, _config: &str, description: &str) -> Result<u32> {
        let mut s = self.state.lock().unwrap();
        let num = s.next;
        s.next += 1;
        let mut userdata = HashMap::new();
        userdata.insert("panko".to_owned(), "yes".to_owned());
        s.snaps.push(Snapshot {
            num,
            kind: SnapType::Single,
            pre_num: 0,
            date: gtk::glib::DateTime::now_local()
                .map(|d| d.to_unix())
                .unwrap_or(0),
            uid: 1000,
            description: description.into(),
            cleanup: "number".into(),
            userdata,
        });
        Ok(num)
    }

    fn delete(&self, _config: &str, nums: &[u32]) -> Result<()> {
        self.state
            .lock()
            .unwrap()
            .snaps
            .retain(|s| !nums.contains(&s.num));
        Ok(())
    }

    fn changes(&self, _config: &str, num: u32) -> Result<Vec<FileChange>> {
        let c = |p: &str, s| FileChange {
            path: p.into(),
            status: s,
        };
        // Starsze migawki mają więcej zmian; najnowsza nie ma żadnych.
        let all = vec![
            c("/etc/fstab", 8),
            c("/etc/hosts", 8),
            c("/etc/zypp/repos.d/shrimp.repo", 1),
            c("/usr/bin/ninja", 1),
            c("/usr/bin/ffmpeg", 2),
            c("/var/lib/NetworkManager/NetworkManager.state", 8),
        ];
        let max = self
            .state
            .lock()
            .unwrap()
            .snaps
            .iter()
            .map(|s| s.num)
            .max()
            .unwrap_or(0);
        let n = ((max.saturating_sub(num)) as usize * 2).min(all.len());
        Ok(all.into_iter().take(n).collect())
    }

    fn restore_files(&self, _config: &Config, _num: u32, _paths: &[String]) -> Result<()> {
        Ok(())
    }

    fn set_config(&self, config: &str, values: &[(String, String)]) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        let c = s
            .configs
            .iter_mut()
            .find(|c| c.name == config)
            .ok_or(Error::Failed("brak".into()))?;
        for (k, v) in values {
            c.values.insert(k.clone(), v.clone());
        }
        Ok(())
    }

    fn rollback(&self, num: u32) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        let n = s.next;
        s.next += 1;
        s.snaps.push(Snapshot {
            num: n,
            kind: SnapType::Single,
            pre_num: 0,
            date: gtk::glib::DateTime::now_local()
                .map(|d| d.to_unix())
                .unwrap_or(0),
            uid: 0,
            description: format!("rollback backup of #{num}"),
            cleanup: "number".into(),
            userdata: HashMap::new(),
        });
        Ok(())
    }

    fn enable_volume(&self, name: &str, subvolume: &str) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        if !s.configs.iter().any(|c| c.name == name) {
            s.configs.push(config(name, subvolume, &[]));
        }
        Ok(())
    }

    fn disable_volume(&self, name: &str) -> Result<()> {
        self.state
            .lock()
            .unwrap()
            .configs
            .retain(|c| c.name != name);
        Ok(())
    }

    fn grant_access(&self, _user: &str, _configs: &[Config]) -> Result<()> {
        Ok(())
    }

    fn reboot(&self) -> Result<()> {
        Ok(())
    }

    fn max_age(&self) -> Option<u32> {
        self.state.lock().unwrap().max_age
    }

    fn set_max_age(&self, days: Option<u32>) -> Result<()> {
        self.state.lock().unwrap().max_age = days;
        Ok(())
    }

    fn is_demo(&self) -> bool {
        true
    }
}
