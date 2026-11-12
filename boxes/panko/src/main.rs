//! Panko: cofanie zmian w systemie Shrimp. Nakładka GTK4 + libadwaita na migawki
//! (usługa `org.opensuse.Snapper`). `panko --demo` pokazuje przykładowe dane z makiety.

mod backend;
mod demo;
mod escape;
mod help;
mod i18n;
mod model;
mod retention;
mod ui;

use std::sync::Arc;

use adw::prelude::*;

const APP_ID: &str = "org.shrimp.Panko";

/// `panko --cleanup`: raz dziennie jako root (panko-cleanup.timer). `panko --set-max-age N|off`:
/// zapis ustawienia jako root (pkexec z okna). Oba bez okna i bez GTK.
fn cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--cleanup") => Some(cleanup()),
        Some("--set-max-age") => {
            let days = match args.get(1).map(String::as_str) {
                Some("off") => None,
                Some(v) => match v.parse::<u32>() {
                    Ok(d) if (1..=retention::MAX_DAYS).contains(&d) => Some(d),
                    _ => {
                        eprintln!(
                            "panko: liczba dni od 1 do {} albo \"off\"",
                            retention::MAX_DAYS
                        );
                        return Some(2);
                    }
                },
                None => return Some(2),
            };
            Some(match retention::write(days) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("panko: nie udało się zapisać {}: {e}", retention::CONF_PATH);
                    1
                }
            })
        }
        _ => None,
    }
}

fn cleanup() -> i32 {
    use backend::Backend;
    let Some(days) = retention::read() else {
        println!("panko: usuwanie starych migawek jest wyłączone");
        return 0;
    };
    let b = match backend::Dbus::connect() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("panko: {e}");
            return 1;
        }
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let mut code = 0;
    let configs = match b.configs() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("panko: {e}");
            return 1;
        }
    };
    for c in configs {
        let result = b.snapshots(&c.name).and_then(|snaps| {
            let old = retention::expired(&snaps, now, days);
            if !old.is_empty() {
                b.delete(&c.name, &old)?;
            }
            Ok(old)
        });
        match result {
            Ok(old) => println!(
                "panko: {}: usunięto {} migawek starszych niż {days} dni {old:?}",
                c.name,
                old.len()
            ),
            Err(e) => {
                eprintln!("panko: {}: {e}", c.name);
                code = 1;
            }
        }
    }
    code
}

fn main() -> gtk::glib::ExitCode {
    if let Some(code) = cli() {
        return gtk::glib::ExitCode::from(code as u8);
    }
    let demo = std::env::args().any(|a| a == "--demo")
        || std::env::var("PANKO_DEMO").is_ok_and(|v| v == "1");
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
        .build();
    app.connect_activate(move |app| {
        let backend: Arc<dyn backend::Backend> = if demo {
            Arc::new(demo::Demo::new())
        } else {
            match backend::Dbus::connect() {
                Ok(b) => Arc::new(b),
                Err(e) => {
                    // Bez magistrali systemowej okno pokaże stan „niedostępne”.
                    eprintln!("panko: {e}");
                    Arc::new(backend::Offline)
                }
            }
        };
        ui::build(app, backend);
    });
    // Własne argumenty (--demo) nie trafiają do GApplication.
    app.run_with_args(&[std::env::args().next().unwrap_or_default()])
}
