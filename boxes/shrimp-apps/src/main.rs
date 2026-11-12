//! Shrimp Apps: sklep z aplikacjami Flathub dla Shrimp (GTK4 + libadwaita + libflatpak).

mod backend;
mod catalog;
mod categories;
mod details;
mod headless;
mod i18n;
mod net;
mod selftest;
mod store;
mod widgets;
mod window;

use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use backend::{Backend, Cmd};
use store::Store;

const APP_ID: &str = "org.shrimp.Apps";
const STYLE: &str = include_str!("style.css");
const HELP_JSON: &str = include_str!("../../help-links/help.json");

fn help_url() -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(HELP_JSON).ok()?;
    let base = v.get("base")?.as_str()?.trim_end_matches('/');
    if !base.starts_with("https://") {
        return None;
    }
    let topic = v.get("topics")?.get("apps")?.as_str()?;
    let lang = glib::language_names().first().map(|l| l.split(['_', '.']).next().unwrap_or("").to_string()).unwrap_or_default();
    Some(format!("{base}/{topic}?lang={lang}&from=apps"))
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    // Sprawdzenie bez okna: shrimp-apps --self-test [ID_APLIKACJI]
    if args.get(1).map(String::as_str) == Some("--self-test") {
        return selftest::run(args.get(2).cloned());
    }

    // Instalacja bez okna (obraz, onboarding): shrimp-apps --install [--base] [ID...]
    if args.get(1).map(String::as_str) == Some("--install") {
        return headless::run(&args[2..]);
    }

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(|_| {
        let css = gtk::CssProvider::new();
        css.load_from_string(STYLE);
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("display"),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    app.connect_activate(|app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        build(app, None);
    });
    // appstream://org.gimp.GIMP lub sam identyfikator: otwiera stronę aplikacji.
    app.connect_open(|app, files, _| {
        let id = files.first().map(|f| {
            let uri = f.uri().to_string();
            uri.trim_start_matches("appstream://").trim_start_matches("appstream:").trim_end_matches('/').to_string()
        });
        build(app, id);
    });
    app.run()
}

fn build(app: &adw::Application, open_id: Option<String>) {
    let store = Rc::new(Store::default());
    let shell = window::Shell::new(app, store.clone());

    let (tx, rx) = async_channel::unbounded();
    let backend = Rc::new(Backend::start(tx));
    store.set_backend(backend);
    store.send(Cmd::Refresh { force: false });

    {
        let shell = shell.clone();
        glib::spawn_future_local(async move {
            let mut pending = open_id;
            while let Ok(ev) = rx.recv().await {
                let catalog = matches!(ev, backend::Event::Catalog(_));
                shell.handle(ev);
                if catalog {
                    if let Some(id) = pending.take() {
                        shell.open_app(&id);
                    }
                }
            }
        });
    }
    {
        let shell = shell.clone();
        glib::spawn_future_local(async move {
            if let Ok(ids) = gio::spawn_blocking(net::popular).await {
                shell.popular_loaded(ids);
            }
        });
    }

    let actions = [
        ("refresh", vec!["<Control>r"]),
        ("help", vec!["F1"]),
        ("about", vec![]),
        ("search", vec!["<Control>f"]),
        ("quit", vec!["<Control>q"]),
    ];
    for (name, accels) in actions {
        let a = gio::SimpleAction::new(name, None);
        let s = shell.clone();
        a.connect_activate(move |_, _| match name {
            "refresh" => s.store.send(Cmd::Refresh { force: true }),
            "help" => {
                if let Some(u) = help_url() {
                    gtk::UriLauncher::new(&u).launch(Some(&s.window), gio::Cancellable::NONE, |_| {});
                }
            }
            "about" => about(&s.window),
            "search" => s.focus_search(),
            "quit" => s.window.close(),
            _ => {}
        });
        if name == "help" {
            a.set_enabled(help_url().is_some());
        }
        app.add_action(&a);
        if !accels.is_empty() {
            app.set_accels_for_action(&format!("app.{name}"), &accels);
        }
    }
    let launch = gio::SimpleAction::new("launch", Some(glib::VariantTy::STRING));
    {
        let s = shell.clone();
        launch.connect_activate(move |_, p| {
            if let Some(id) = p.and_then(|v| v.get::<String>()) {
                s.store.launch(&id);
            }
        });
    }
    app.add_action(&launch);

    shell.window.present();
}

fn about(window: &adw::ApplicationWindow) {
    let d = adw::AboutDialog::builder()
        .application_name("Shrimp Apps")
        .application_icon(APP_ID)
        .developer_name("Shrimp")
        .version(env!("CARGO_PKG_VERSION"))
        .license_type(gtk::License::Gpl30)
        .comments(crate::i18n::tr("about.comments"))
        .build();
    d.present(Some(window));
}
