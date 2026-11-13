//! `shrimp-apps --install [--base | --manifest PLIK] [ID...]`: instalacja bez okna.
//!
//! Tędy przechodzą aplikacje z obrazu i z onboardingu: to sklep Shrimp (jego backend
//! libflatpak i jego repozytorium) pobiera podstawowy zestaw, a nie osobne wywołania `flatpak`.
//! Na stdout idzie po jednym obiekcie JSON na linię:
//!   {"id":"org.gnome.Loupe","percent":42}
//!   {"done":true,"id":"org.gnome.Loupe"}   albo z "error" / "skipped" (klucze alfabetycznie)
//! Kod wyjścia: 0, gdy wszystkie się udały lub były już zainstalowane; 1, gdy któraś zawiodła;
//! 2 przy błędnym wywołaniu; 3, gdy nie ma sieci (nic nie pobrano).

use gtk::{gio, glib};

use crate::backend::{Backend, Cmd, Event, OpKind};

/// Zestaw podstawowy instalowany razem z systemem; plik trafia do `/usr/share/shrimp/base-apps.json`.
pub const DEFAULT_MANIFEST: &str = "/usr/share/shrimp/base-apps.json";

fn json_line(v: serde_json::Value) {
    println!("{v}");
}

fn read_manifest(path: &str) -> Result<Vec<String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("{path}: {e}"))?;
    let list = v.get("apps").and_then(|a| a.as_array()).ok_or(format!("{path}: brak tablicy \"apps\""))?;
    Ok(list
        .iter()
        .filter_map(|a| a.get("id").and_then(|i| i.as_str()).map(str::to_string))
        .collect())
}

/// Identyfikator aplikacji we Flathubie (bez ukośników i spacji), żeby nie zbudować dziwnego refa.
fn valid_id(id: &str) -> bool {
    id.contains('.') && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-')
}

pub fn run(args: &[String]) -> glib::ExitCode {
    let mut ids: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--base" => match read_manifest(DEFAULT_MANIFEST) {
                Ok(l) => ids.extend(l),
                Err(e) => {
                    eprintln!("shrimp-apps: {e}");
                    return glib::ExitCode::from(2);
                }
            },
            "--manifest" => {
                i += 1;
                match args.get(i).map(|p| read_manifest(p)) {
                    Some(Ok(l)) => ids.extend(l),
                    Some(Err(e)) => {
                        eprintln!("shrimp-apps: {e}");
                        return glib::ExitCode::from(2);
                    }
                    None => {
                        eprintln!("shrimp-apps: --manifest wymaga ścieżki");
                        return glib::ExitCode::from(2);
                    }
                }
            }
            id => ids.push(id.to_string()),
        }
        i += 1;
    }
    // Bez powtórzeń, w kolejności podania (dedup() usuwał tylko sąsiednie).
    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    // Zestaw podstawowy: aplikacja, którą system ma już z pakietu RPM (np. Monitor systemu, Dziennik),
    // nie wchodzi drugi raz jako Flatpak (dwie takie same ikony w siatce).
    ids.retain(|id| {
        let host = host_desktop(id);
        if let Some(f) = &host {
            json_line(serde_json::json!({ "id": id, "done": true, "skipped": true, "host": f }));
        }
        host.is_none()
    });
    if ids.is_empty() {
        return glib::ExitCode::SUCCESS;
    }
    if ids.is_empty() || ids.iter().any(|id| !valid_id(id)) {
        eprintln!("użycie: shrimp-apps --install [--base | --manifest PLIK] [ID_APLIKACJI...]");
        return glib::ExitCode::from(2);
    }

    let (tx, rx) = async_channel::unbounded();
    let backend = Backend::start(tx);
    backend.send(Cmd::ListInstalled);
    let installed = loop {
        match rx.recv_blocking() {
            Ok(Event::Installed(l)) => break l,
            Ok(Event::CatalogFailed(e)) => {
                eprintln!("shrimp-apps: {e}");
                return glib::ExitCode::FAILURE;
            }
            Ok(_) => {}
            Err(_) => return glib::ExitCode::FAILURE,
        }
    };

    let arch = std::env::consts::ARCH;
    let mut failed = 0;
    let mut offline = 0;
    for id in &ids {
        if installed.iter().any(|a| &a.id == id) {
            json_line(serde_json::json!({ "id": id, "done": true, "skipped": true }));
            extras(&backend, &rx, id);
            continue;
        }
        backend.send(Cmd::Run {
            kind: OpKind::Install,
            app_id: id.clone(),
            flatpak_ref: format!("app/{id}/{arch}/stable"),
            cancel: gio::Cancellable::new(),
        });
        let mut last = -1;
        loop {
            match rx.recv_blocking() {
                Ok(Event::Progress { app_id, fraction }) if &app_id == id => {
                    let p = (fraction * 100.0) as i32;
                    if p != last {
                        last = p;
                        json_line(serde_json::json!({ "id": id, "percent": p }));
                    }
                }
                Ok(Event::Done { app_id, error, .. }) if &app_id == id => {
                    match error {
                        None => {
                            json_line(serde_json::json!({ "id": id, "done": true }));
                            extras(&backend, &rx, id);
                        }
                        Some(e) => {
                            failed += 1;
                            if is_network_error(&e) {
                                offline += 1;
                            }
                            json_line(serde_json::json!({ "id": id, "done": true, "error": e }));
                        }
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => return glib::ExitCode::FAILURE,
            }
        }
    }
    if failed == 0 {
        glib::ExitCode::SUCCESS
    } else if offline == failed && offline == ids.len() {
        glib::ExitCode::from(3)
    } else {
        glib::ExitCode::FAILURE
    }
}

/// Plik .desktop aplikacji z pakietu systemowego o tym samym identyfikatorze (albo jej starej nazwie).
fn host_desktop(id: &str) -> Option<String> {
    let alias = match id {
        "org.gnome.SystemMonitor" => Some("gnome-system-monitor.desktop"),
        "org.gnome.DiskUtility" => Some("gnome-disks.desktop"),
        _ => None,
    };
    let dir = std::env::var("SHRIMP_APPS_HOST_APPS").unwrap_or_else(|_| "/usr/share/applications".into());
    [format!("{id}.desktop")].into_iter().chain(alias.map(str::to_string)).map(|f| format!("{dir}/{f}")).find(|p| std::path::Path::new(p).exists())
}

/// Dodatki, bez których Flatpak aplikacji nie ruszy albo nie odtwarza wideo. Steam: 32-bitowe biblioteki
/// (GL32, Compat.i386) i kodeki (ffmpeg-full w obu architekturach) dla gałęzi jego środowiska (np. 24.08).
pub fn extra_refs(id: &str, branch: &str, arch: &str) -> Vec<String> {
    match id {
        "com.valvesoftware.Steam" => [
            "org.freedesktop.Platform.GL32.default",
            "org.freedesktop.Platform.Compat.i386",
            "org.freedesktop.Platform.ffmpeg-full",
            "org.freedesktop.Platform.ffmpeg_full.i386",
        ]
        .iter()
        .map(|n| format!("runtime/{n}/{arch}/{branch}"))
        .collect(),
        _ => Vec::new(),
    }
}

/// Gałąź środowiska zainstalowanej aplikacji (`runtime=org.freedesktop.Platform/x86_64/24.08` → 24.08).
fn runtime_branch(id: &str) -> Option<String> {
    use libflatpak::prelude::*;
    let inst = if std::env::var("SHRIMP_APPS_INSTALLATION").as_deref() == Ok("user") {
        libflatpak::Installation::new_user(gio::Cancellable::NONE)
    } else {
        libflatpak::Installation::new_system(gio::Cancellable::NONE)
    }
    .ok()?;
    let r = inst.installed_ref(libflatpak::RefKind::App, id, None, None, gio::Cancellable::NONE).ok()?;
    let meta = r.load_metadata(gio::Cancellable::NONE).ok()?;
    let text = String::from_utf8_lossy(&meta).to_string();
    text.lines().find_map(|l| l.strip_prefix("runtime=")).and_then(|r| r.rsplit('/').next()).map(str::to_string)
}

/// Doinstalowuje dodatki aplikacji; błąd dodatku nie jest błędem aplikacji (tylko wpis w wyjściu).
fn extras(backend: &Backend, rx: &async_channel::Receiver<Event>, id: &str) {
    let arch = libflatpak::functions::default_arch().map(|a| a.to_string()).unwrap_or_else(|| "x86_64".into());
    let Some(branch) = (if extra_refs(id, "x", &arch).is_empty() { None } else { runtime_branch(id) }) else { return };
    for r in extra_refs(id, &branch, &arch) {
        let key = format!("{id}+{r}");
        backend.send(Cmd::Run { kind: OpKind::Install, app_id: key.clone(), flatpak_ref: r.clone(), cancel: gio::Cancellable::new() });
        loop {
            match rx.recv_blocking() {
                Ok(Event::Done { app_id, error, .. }) if app_id == key => {
                    let already = error.as_deref().is_some_and(|e| e.contains("already installed"));
                    match error.filter(|_| !already) {
                        None => json_line(serde_json::json!({ "id": id, "extra": r, "done": true })),
                        Some(e) => json_line(serde_json::json!({ "id": id, "extra": r, "error": e })),
                    }
                    break;
                }
                Ok(_) => {}
                Err(_) => return,
            }
        }
    }
}

fn is_network_error(e: &str) -> bool {
    let e = e.to_lowercase();
    ["resolve", "network", "connection", "unreachable", "timed out", "could not connect"].iter().any(|k| e.contains(k))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids() {
        assert!(valid_id("org.gnome.Loupe"));
        assert!(!valid_id("Loupe"));
        assert!(!valid_id("org/gnome/Loupe"));
        assert!(!valid_id("org.gnome.Loupe --system"));
    }

    #[test]
    fn steam_extras() {
        let r = extra_refs("com.valvesoftware.Steam", "24.08", "x86_64");
        assert!(r.contains(&"runtime/org.freedesktop.Platform.GL32.default/x86_64/24.08".to_string()));
        assert!(r.iter().any(|x| x.contains("Compat.i386")));
        assert!(extra_refs("org.gnome.Loupe", "48", "x86_64").is_empty());
    }

    #[test]
    fn host_apps_are_skipped() {
        let d = std::env::temp_dir().join(format!("shrimp-apps-host-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("gnome-system-monitor.desktop"), "").unwrap();
        std::env::set_var("SHRIMP_APPS_HOST_APPS", &d);
        assert!(host_desktop("org.gnome.SystemMonitor").is_some());
        assert!(host_desktop("org.gnome.Loupe").is_none());
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn network() {
        assert!(is_network_error("Unable to connect: Could not resolve host"));
        assert!(!is_network_error("No such ref"));
    }
}
