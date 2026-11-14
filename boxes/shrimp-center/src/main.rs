//! Shrimp Center: systemy na komputerze (uruchom ponownie w Windows, domyślny system, ustawienia UEFI)
//! i wygląd pulpitu. Zastępuje menu GRUB-a, którego w Shrimp już nie ma (systemd-boot bez menu).
//!
//! `shrimp-center [--demo]`             okno; `--demo`: przykładowe wpisy UEFI i ustawienia w pamięci.
//! `shrimp-center --boot-next ID`       (root, pkexec z okna) system ID tylko przy następnym starcie.
//! `shrimp-center --default ID`         (root, pkexec z okna) system ID jako domyślny (BootOrder).

mod efi;
mod help;
mod i18n;
mod tweaks;
mod ui;

use adw::prelude::*;

const APP_ID: &str = "org.shrimp.Center";

fn cli() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let action: fn(&str) -> Result<(), String> = match args.first().map(String::as_str) {
        Some("--boot-next") => efi::set_boot_next,
        Some("--default") => efi::set_default,
        _ => return None,
    };
    let Some(id) = args.get(1) else {
        eprintln!("użycie: shrimp-center --boot-next|--default ID");
        return Some(2);
    };
    Some(match action(&id.to_uppercase()) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("shrimp-center: {e}");
            1
        }
    })
}

fn main() -> gtk::glib::ExitCode {
    if let Some(code) = cli() {
        return gtk::glib::ExitCode::from(code as u8);
    }
    let demo = std::env::args().any(|a| a == "--demo")
        || std::env::var("SHRIMP_CENTER_DEMO").is_ok_and(|v| v == "1");
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        ui::build(app, demo);
    });
    // Własne argumenty (--demo) nie trafiają do GApplication.
    app.run_with_args(&[std::env::args().next().unwrap_or_default()])
}
