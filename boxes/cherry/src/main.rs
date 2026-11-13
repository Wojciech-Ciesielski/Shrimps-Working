//! Cherry: terminal Shrimp (zsh) z kartami w pionowym pasku, notatkami, historią i skrótami jak w Windows.

mod help;
mod i18n;
mod store;
mod ui;

use adw::prelude::*;

const APP_ID: &str = "org.shrimp.Cherry";

fn main() -> gtk::glib::ExitCode {
    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gtk::gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();
    // `cherry [FOLDER]` albo `cherry --working-directory FOLDER` (np. „Otwórz w terminalu” w Filerze):
    // nowa karta w tym folderze, także gdy Cherry już działa (argumenty trafiają do pierwszej instancji).
    app.connect_command_line(|app, cl| {
        let args: Vec<String> = cl
            .arguments()
            .iter()
            .skip(1)
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let dir = ui::dir_from_args(&args, cl.cwd().as_deref());
        ui::open(app, dir);
        gtk::glib::ExitCode::SUCCESS
    });
    app.run()
}
