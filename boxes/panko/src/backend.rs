//! Dostęp do migawek. Odczyt i tworzenie/usuwanie przez usługę systemową
//! `org.opensuse.Snapper` (D-Bus). Operacje, które usługa zostawia administratorowi
//! (ustawienia, przywracanie plików i systemu, nowe woluminy), idą przez pkexec
//! i program `snapper`, z regułą polkit `org.shrimp.panko.*` (data/org.shrimp.Panko.policy).

use std::collections::HashMap;
use std::process::Command;

use crate::escape::{escape, unescape};
use crate::model::{Config, FileChange, SnapType, Snapshot};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Konto nie ma dostępu do migawek (ALLOW_USERS / ALLOW_GROUPS).
    NoPermission,
    /// Usługa migawek nie działa albo nie jest zainstalowana.
    Unavailable(String),
    /// Użytkownik zamknął okno hasła.
    Cancelled,
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoPermission => write!(f, "no permissions"),
            Error::Unavailable(s) => write!(f, "unavailable: {s}"),
            Error::Cancelled => write!(f, "cancelled"),
            Error::Failed(s) => write!(f, "{s}"),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Backend: Send + Sync {
    fn configs(&self) -> Result<Vec<Config>>;
    fn snapshots(&self, config: &str) -> Result<Vec<Snapshot>>;
    fn create(&self, config: &str, description: &str) -> Result<u32>;
    fn delete(&self, config: &str, nums: &[u32]) -> Result<()>;
    /// Pliki zmienione między migawką `num` a obecnym systemem.
    fn changes(&self, config: &str, num: u32) -> Result<Vec<FileChange>>;
    /// Przywraca wybrane pliki do stanu z migawki.
    fn restore_files(&self, config: &Config, num: u32, paths: &[String]) -> Result<()>;
    fn set_config(&self, config: &str, values: &[(String, String)]) -> Result<()>;
    /// Ustawia system do uruchomienia z migawki (przy następnym starcie).
    fn rollback(&self, num: u32) -> Result<()>;
    fn enable_volume(&self, name: &str, subvolume: &str) -> Result<()>;
    fn disable_volume(&self, name: &str) -> Result<()>;
    /// Daje bieżącemu kontu dostęp do wszystkich konfiguracji.
    fn grant_access(&self, user: &str, configs: &[Config]) -> Result<()>;
    fn reboot(&self) -> Result<()>;
    /// Po ilu dniach usuwać migawki (None = nie usuwaj).
    fn max_age(&self) -> Option<u32>;
    fn set_max_age(&self, days: Option<u32>) -> Result<()>;
    fn is_demo(&self) -> bool {
        false
    }
}

const DEST: &str = "org.opensuse.Snapper";
const PATH: &str = "/org/opensuse/Snapper";
const SNAPPER: &str = "/usr/bin/snapper";
const TRANSACTIONAL_UPDATE: &str = "/usr/sbin/transactional-update";

type RawConfig = (String, String, HashMap<String, String>);
type RawSnapshot = (
    u32,
    u16,
    u32,
    i64,
    u32,
    String,
    String,
    HashMap<String, String>,
);

pub struct Dbus {
    conn: zbus::blocking::Connection,
}

impl Dbus {
    pub fn connect() -> Result<Self> {
        let conn =
            zbus::blocking::Connection::system().map_err(|e| Error::Unavailable(e.to_string()))?;
        Ok(Dbus { conn })
    }

    fn proxy(&self) -> Result<zbus::blocking::Proxy<'_>> {
        zbus::blocking::Proxy::new(&self.conn, DEST, PATH, DEST).map_err(map_err)
    }

    fn call<A, R>(&self, method: &str, args: &A) -> Result<R>
    where
        A: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
    {
        self.proxy()?.call(method, args).map_err(map_err)
    }

    /// Wpis „bez wyjścia” z pkexec na wypadek braku uprawnień w usłudze.
    fn fallback_or<T>(r: Result<T>, args: &[String], ok: impl FnOnce() -> Result<T>) -> Result<T> {
        match r {
            Err(Error::NoPermission) => {
                pkexec(SNAPPER, args)?;
                ok()
            }
            other => other,
        }
    }
}

fn map_err(e: zbus::Error) -> Error {
    match &e {
        zbus::Error::MethodError(name, msg, _) => {
            let n = name.as_str();
            if n == "error.no_permissions" {
                Error::NoPermission
            } else if n.starts_with("org.freedesktop.DBus.Error.ServiceUnknown")
                || n.starts_with("org.freedesktop.DBus.Error.NameHasNoOwner")
                || n.starts_with("org.freedesktop.DBus.Error.Spawn")
            {
                Error::Unavailable(n.to_owned())
            } else {
                Error::Failed(format!("{n}: {}", msg.clone().unwrap_or_default()))
            }
        }
        zbus::Error::FDO(fdo) => match **fdo {
            zbus::fdo::Error::ServiceUnknown(_) | zbus::fdo::Error::NameHasNoOwner(_) => {
                Error::Unavailable(e.to_string())
            }
            _ => Error::Failed(e.to_string()),
        },
        _ => Error::Failed(e.to_string()),
    }
}

fn unescape_map(m: HashMap<String, String>) -> HashMap<String, String> {
    m.into_iter()
        .map(|(k, v)| (unescape(&k), unescape(&v)))
        .collect()
}

impl Backend for Dbus {
    fn configs(&self) -> Result<Vec<Config>> {
        let raw: Vec<RawConfig> = self.call("ListConfigs", &())?;
        let mut out: Vec<Config> = raw
            .into_iter()
            .map(|(name, subvolume, values)| Config {
                name: unescape(&name),
                subvolume: unescape(&subvolume),
                values: unescape_map(values),
            })
            .collect();
        // System pierwszy.
        out.sort_by_key(|c| (c.subvolume != "/", c.subvolume.clone()));
        Ok(out)
    }

    fn snapshots(&self, config: &str) -> Result<Vec<Snapshot>> {
        let raw: Vec<RawSnapshot> = self.call("ListSnapshots", &(escape(config),))?;
        Ok(raw
            .into_iter()
            .filter(|s| s.0 != 0) // 0 = obecny system
            .map(
                |(num, kind, pre_num, date, uid, desc, cleanup, userdata)| Snapshot {
                    num,
                    kind: SnapType::from_dbus(kind),
                    pre_num,
                    date,
                    uid,
                    description: unescape(&desc),
                    cleanup: unescape(&cleanup),
                    userdata: unescape_map(userdata),
                },
            )
            .collect())
    }

    fn create(&self, config: &str, description: &str) -> Result<u32> {
        let mut ud = HashMap::new();
        ud.insert("panko".to_owned(), "yes".to_owned());
        let r = self.call(
            "CreateSingleSnapshot",
            &(escape(config), escape(description), "number", ud),
        );
        let args = vec![
            "-c".into(),
            config.into(),
            "create".into(),
            "--description".into(),
            description.into(),
            "--cleanup-algorithm".into(),
            "number".into(),
            "--userdata".into(),
            "panko=yes".into(),
            "--print-number".into(),
        ];
        Self::fallback_or(r, &args, || Ok(0))
    }

    fn delete(&self, config: &str, nums: &[u32]) -> Result<()> {
        let r: Result<()> = self.call("DeleteSnapshots", &(escape(config), nums.to_vec()));
        let mut args: Vec<String> = vec!["-c".into(), config.into(), "delete".into()];
        args.extend(nums.iter().map(u32::to_string));
        Self::fallback_or(r, &args, || Ok(()))
    }

    fn changes(&self, config: &str, num: u32) -> Result<Vec<FileChange>> {
        let cfg = escape(config);
        let _: u32 = self.call("CreateComparison", &(cfg.as_str(), num, 0u32))?;
        let files: Result<Vec<(String, u32)>> = self.call("GetFiles", &(cfg.as_str(), num, 0u32));
        let _: Result<()> = self.call("DeleteComparison", &(cfg.as_str(), num, 0u32));
        Ok(files?
            .into_iter()
            .map(|(path, status)| FileChange {
                path: unescape(&path),
                status,
            })
            .collect())
    }

    fn restore_files(&self, config: &Config, num: u32, paths: &[String]) -> Result<()> {
        let mut args: Vec<String> = vec![
            "-c".into(),
            config.name.clone(),
            "undochange".into(),
            format!("{num}..0"),
        ];
        let base = config.subvolume.trim_end_matches('/');
        args.extend(paths.iter().map(|p| format!("{base}{p}")));
        pkexec(SNAPPER, &args)
    }

    fn set_config(&self, config: &str, values: &[(String, String)]) -> Result<()> {
        let mut args: Vec<String> = vec!["-c".into(), config.into(), "set-config".into()];
        args.extend(values.iter().map(|(k, v)| format!("{k}={v}")));
        pkexec(SNAPPER, &args)
    }

    fn rollback(&self, num: u32) -> Result<()> {
        // Na systemie niezmiennym przywracanie zna transactional-update (ustawia też /etc).
        if std::path::Path::new(TRANSACTIONAL_UPDATE).exists() {
            pkexec(TRANSACTIONAL_UPDATE, &["rollback".into(), num.to_string()])
        } else {
            pkexec(SNAPPER, &["rollback".into(), num.to_string()])
        }
    }

    fn enable_volume(&self, name: &str, subvolume: &str) -> Result<()> {
        pkexec(
            SNAPPER,
            &[
                "-c".into(),
                name.into(),
                "create-config".into(),
                subvolume.into(),
            ],
        )
    }

    fn disable_volume(&self, name: &str) -> Result<()> {
        pkexec(SNAPPER, &["-c".into(), name.into(), "delete-config".into()])
    }

    fn grant_access(&self, user: &str, configs: &[Config]) -> Result<()> {
        // Jedno okno hasła na konfigurację; reguła polkit trzyma zgodę przez kilka minut.
        for c in configs {
            let mut users = c.users("ALLOW_USERS");
            if !users.iter().any(|u| u == user) {
                users.push(user.to_owned());
            }
            pkexec(
                SNAPPER,
                &[
                    "-c".into(),
                    c.name.clone(),
                    "set-config".into(),
                    format!("ALLOW_USERS={}", users.join(" ")),
                    "SYNC_ACL=yes".into(),
                ],
            )?;
        }
        Ok(())
    }

    fn max_age(&self) -> Option<u32> {
        crate::retention::read()
    }

    fn set_max_age(&self, days: Option<u32>) -> Result<()> {
        let arg = days.map_or("off".to_owned(), |d| d.to_string());
        let me = std::env::current_exe()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "/usr/bin/panko".into());
        pkexec(&me, &["--set-max-age".into(), arg])
    }

    fn reboot(&self) -> Result<()> {
        let p = zbus::blocking::Proxy::new(
            &self.conn,
            "org.freedesktop.login1",
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
        )
        .map_err(map_err)?;
        p.call::<_, _, ()>("Reboot", &(true,)).map_err(map_err)
    }
}

/// Uruchamia program jako administrator. 126/127 = okno hasła zamknięte lub odmowa.
pub fn pkexec(program: &str, args: &[String]) -> Result<()> {
    let out = Command::new("pkexec")
        .arg(program)
        .args(args)
        .output()
        .map_err(|e| Error::Failed(format!("pkexec: {e}")))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126) | Some(127) => Err(Error::Cancelled),
        _ => Err(Error::Failed(
            String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        )),
    }
}

/// Nazwa bieżącego konta (do nadania dostępu).
pub fn current_user() -> String {
    std::env::var("USER")
        .ok()
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| gtk::glib::user_name().to_string_lossy().into_owned())
}

/// Brak połączenia z magistralą systemową: każda operacja zwraca „niedostępne”.
pub struct Offline;

impl Backend for Offline {
    fn configs(&self) -> Result<Vec<Config>> {
        Err(Error::Unavailable("no system bus".into()))
    }
    fn snapshots(&self, _: &str) -> Result<Vec<Snapshot>> {
        self.configs().map(|_| vec![])
    }
    fn create(&self, _: &str, _: &str) -> Result<u32> {
        self.configs().map(|_| 0)
    }
    fn delete(&self, _: &str, _: &[u32]) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn changes(&self, _: &str, _: u32) -> Result<Vec<FileChange>> {
        self.configs().map(|_| vec![])
    }
    fn restore_files(&self, _: &Config, _: u32, _: &[String]) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn set_config(&self, _: &str, _: &[(String, String)]) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn rollback(&self, _: u32) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn enable_volume(&self, _: &str, _: &str) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn disable_volume(&self, _: &str) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn grant_access(&self, _: &str, _: &[Config]) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn reboot(&self) -> Result<()> {
        self.configs().map(|_| ())
    }
    fn max_age(&self) -> Option<u32> {
        None
    }
    fn set_max_age(&self, _: Option<u32>) -> Result<()> {
        self.configs().map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Prawdziwa usługa migawek (uruchamiać ręcznie, jako root albo użytkownik z dostępem):
    /// PANKO_TEST_CONFIG=<konfiguracja> cargo test -- --ignored dbus
    /// Konfiguracja musi mieć migawkę 1 z plikiem /a.txt, zmienionym potem, i bez /b.txt.
    #[test]
    #[ignore]
    fn dbus_roundtrip() {
        let cfg = std::env::var("PANKO_TEST_CONFIG").expect("PANKO_TEST_CONFIG");
        let b = Dbus::connect().expect("magistrala systemowa");
        let configs = b.configs().expect("ListConfigs");
        assert!(configs.iter().any(|c| c.name == cfg));
        let num = b
            .create(&cfg, "Test Panko: zażółć gęślą jaźń")
            .expect("create");
        assert!(num > 0);
        let snaps = b.snapshots(&cfg).expect("ListSnapshots");
        let mine = snaps.iter().find(|s| s.num == num).expect("nowa migawka");
        assert_eq!(mine.description, "Test Panko: zażółć gęślą jaźń");
        assert_eq!(mine.category().title(), "Test Panko: zażółć gęślą jaźń");
        let ch = b.changes(&cfg, 1).expect("porównanie");
        let a = ch.iter().find(|c| c.path == "/a.txt").expect("/a.txt");
        assert_eq!(a.marker(), crate::model::Marker::Changed);
        let bb = ch.iter().find(|c| c.path == "/b.txt").expect("/b.txt");
        assert_eq!(bb.marker(), crate::model::Marker::Added);
        b.delete(&cfg, &[num]).expect("delete");
        assert!(!b.snapshots(&cfg).unwrap().iter().any(|s| s.num == num));
    }
}
