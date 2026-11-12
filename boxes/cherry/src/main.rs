//! Cherry: terminal Shrimp (zsh), który tłumaczy komendy prostym językiem.
//! Panel „Jak to działa?”, notatki, skróty i historia obok terminala VTE.

mod explain;
mod help;
mod i18n;
mod store;
mod ui;

use adw::prelude::*;

const APP_ID: &str = "org.shrimp.Cherry";

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(ui::build);
    app.run()
}
