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
    ids.dedup();
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
                        None => json_line(serde_json::json!({ "id": id, "done": true })),
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
    fn network() {
        assert!(is_network_error("Unable to connect: Could not resolve host"));
        assert!(!is_network_error("No such ref"));
    }
}
