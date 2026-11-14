//! shrimp-webapp: aplikacja internetowa we własnym oknie, bez przeglądarki (WebKitGTK).
//!
//! `shrimp-webapp --id ID --name NAZWA --url ADRES`
//!
//! Każda aplikacja ma własny identyfikator okna (`org.shrimp.WebApp.<id>`, ten sam co StartupWMClass
//! w pliku .desktop z boxa web-apps, więc dock łączy okno z ikoną), własne dane i ciasteczka
//! (`~/.local/share/shrimp/webapps/<id>/`). Linki poza aplikacją otwiera domyślna przeglądarka.

mod i18n;
mod nav;

use std::path::PathBuf;

use adw::prelude::*;
use gtk::{gio, glib};
use i18n::{tr, trf};
use webkit::prelude::*;

struct Args {
    id: String,
    name: String,
    url: String,
}

fn args() -> Option<Args> {
    let a: Vec<String> = std::env::args().collect();
    let get = |k: &str| a.iter().position(|x| x == k).and_then(|i| a.get(i + 1)).cloned();
    let url = get("--url")?;
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return None;
    }
    let id = get("--id").map(|i| nav::safe_id(&i)).unwrap_or_else(|| nav::safe_id(&nav::host(&url)));
    let name = get("--name").unwrap_or_else(|| nav::host(&url));
    Some(Args { id, name, url })
}

fn main() -> glib::ExitCode {
    let Some(args) = args() else {
        eprintln!("użycie: shrimp-webapp --id ID --name NAZWA --url https://…");
        return glib::ExitCode::from(2);
    };
    let app = adw::Application::builder().application_id(nav::app_id(&args.id)).build();
    let args = std::rc::Rc::new(args);
    app.connect_activate(move |app| {
        if let Some(w) = app.active_window() {
            w.present();
            return;
        }
        build(app, &args);
    });
    app.run_with_args(&[std::env::args().next().unwrap_or_default()])
}

fn data_dir(id: &str) -> PathBuf {
    glib::user_data_dir().join("shrimp/webapps").join(id)
}

fn open_external(win: &adw::ApplicationWindow, uri: &str) {
    gtk::UriLauncher::new(uri).launch(Some(win), gio::Cancellable::NONE, |_| {});
}

fn build(app: &adw::Application, args: &Args) {
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(&args.name)
        .default_width(1100)
        .default_height(760)
        .build();

    // Dane tej aplikacji osobno: logowanie w jednej nie wpływa na inne.
    let dir = data_dir(&args.id);
    let cache = glib::user_cache_dir().join("shrimp/webapps").join(&args.id);
    let session = webkit::NetworkSession::new(dir.join("data").to_str(), cache.to_str());
    if let Some(cookies) = session.cookie_manager() {
        cookies.set_persistent_storage(&dir.join("cookies.sqlite").to_string_lossy(), webkit::CookiePersistentStorage::Sqlite);
    }
    let view = webkit::WebView::builder().network_session(&session).build();
    if let Some(s) = webkit::prelude::WebViewExt::settings(&view) {
        s.set_enable_developer_extras(false);
        s.set_javascript_can_open_windows_automatically(false);
        s.set_enable_back_forward_navigation_gestures(true);
    }

    let back = gtk::Button::builder().icon_name("go-previous-symbolic").tooltip_text(tr("nav.back")).sensitive(false).build();
    let forward = gtk::Button::builder().icon_name("go-next-symbolic").tooltip_text(tr("nav.forward")).sensitive(false).build();
    let reload = gtk::Button::builder().icon_name("view-refresh-symbolic").tooltip_text(tr("nav.reload")).build();
    let title = adw::WindowTitle::new(&args.name, &nav::host(&args.url));
    let header = adw::HeaderBar::new();
    header.pack_start(&back);
    header.pack_start(&forward);
    header.pack_start(&reload);
    header.set_title_widget(Some(&title));
    let menu = gio::Menu::new();
    menu.append(Some(tr("menu.zoom-in")), Some("win.zoom-in"));
    menu.append(Some(tr("menu.zoom-out")), Some("win.zoom-out"));
    menu.append(Some(tr("menu.copy")), Some("win.copy"));
    menu.append(Some(tr("menu.browser")), Some("win.browser"));
    header.pack_end(&gtk::MenuButton::builder().icon_name("open-menu-symbolic").tooltip_text(tr("menu.main")).menu_model(&menu).build());

    let progress = gtk::ProgressBar::new();
    progress.add_css_class("osd");
    progress.set_valign(gtk::Align::Start);
    progress.set_visible(false);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&view));
    overlay.add_overlay(&progress);
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&overlay));
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(&toasts));
    window.set_content(Some(&tv));

    // Nawigacja i tytuł.
    {
        let v = view.clone();
        back.connect_clicked(move |_| v.go_back());
        let v = view.clone();
        forward.connect_clicked(move |_| v.go_forward());
        let v = view.clone();
        reload.connect_clicked(move |_| v.reload());
    }
    {
        let (back, forward) = (back.clone(), forward.clone());
        view.connect_load_changed(move |v, _| {
            back.set_sensitive(v.can_go_back());
            forward.set_sensitive(v.can_go_forward());
        });
    }
    {
        let (t1, name) = (title.clone(), args.name.clone());
        view.connect_title_notify(move |v| {
            let t = v.title().map(|t| t.to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| name.clone());
            t1.set_title(&t);
        });
        let t2 = title.clone();
        view.connect_uri_notify(move |v| {
            if let Some(u) = v.uri() {
                t2.set_subtitle(&nav::host(&u));
            }
        });
        let progress = progress.clone();
        view.connect_estimated_load_progress_notify(move |v| {
            let p = v.estimated_load_progress();
            progress.set_fraction(p);
            progress.set_visible(p > 0.0 && p < 1.0);
        });
    }

    // Linki poza aplikacją (i nowe okna) otwiera domyślna przeglądarka.
    {
        let base = args.url.clone();
        let win = window.clone();
        view.connect_decide_policy(move |_, decision, kind| {
            if !matches!(kind, webkit::PolicyDecisionType::NavigationAction | webkit::PolicyDecisionType::NewWindowAction) {
                return false;
            }
            let Some(d) = decision.downcast_ref::<webkit::NavigationPolicyDecision>() else { return false };
            let Some(uri) = d.navigation_action().and_then(|a| a.request()).and_then(|r| r.uri()) else { return false };
            let user = d.navigation_action().is_some_and(|a| a.is_user_gesture());
            if nav::to_browser(&base, &uri, user) {
                open_external(&win, &uri);
                decision.ignore();
                return true;
            }
            false
        });
        // Nowe okno z tej samej aplikacji: otwórz w tym samym widoku.
        view.connect_create(|v, action| {
            if let Some(uri) = action.clone().request().and_then(|r| r.uri()) {
                v.load_uri(&uri);
            }
            None
        });
    }

    // Powiadomienia: tak (jak w aplikacji). Kamera i mikrofon: pytanie.
    {
        let win = window.clone();
        let name = args.name.clone();
        view.connect_permission_request(move |_, req| {
            if req.is::<webkit::NotificationPermissionRequest>() {
                req.allow();
                return true;
            }
            if req.is::<webkit::UserMediaPermissionRequest>() {
                let d = adw::AlertDialog::new(Some(&trf("perm.title", &[("name", &name)])), None);
                d.add_responses(&[("deny", tr("perm.deny")), ("allow", tr("perm.allow"))]);
                d.set_response_appearance("allow", adw::ResponseAppearance::Suggested);
                d.set_close_response("deny");
                let req = req.clone();
                d.connect_response(None, move |_, r| if r == "allow" { req.allow() } else { req.deny() });
                d.present(Some(&win));
                return true;
            }
            false
        });
    }

    // Pobieranie do folderu Pobrane.
    {
        let toasts = toasts.clone();
        session.connect_download_started(move |_, dl| {
            let toasts = toasts.clone();
            dl.connect_decide_destination(|dl, suggested| {
                let dir = glib::user_special_dir(glib::UserDirectory::Downloads).unwrap_or_else(glib::home_dir);
                let name = nav::safe_file_name(suggested);
                dl.set_destination(&nav::free_path(&dir, &name).to_string_lossy());
                true
            });
            dl.connect_finished(move |dl| {
                if let Some(dest) = dl.destination() {
                    let file = std::path::Path::new(dest.as_str()).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
                    toasts.add_toast(adw::Toast::new(&trf("download.done", &[("file", &file)])));
                }
            });
        });
    }

    // Menu.
    let actions = [
        ("zoom-in", 1.1f64),
        ("zoom-out", 1.0 / 1.1),
    ];
    for (name, k) in actions {
        let a = gio::SimpleAction::new(name, None);
        let v = view.clone();
        a.connect_activate(move |_, _| v.set_zoom_level((v.zoom_level() * k).clamp(0.5, 3.0)));
        window.add_action(&a);
    }
    let copy = gio::SimpleAction::new("copy", None);
    {
        let (v, toasts, win) = (view.clone(), toasts.clone(), window.clone());
        copy.connect_activate(move |_, _| {
            if let Some(u) = v.uri() {
                win.clipboard().set_text(&u);
                toasts.add_toast(adw::Toast::new(tr("copied")));
            }
        });
    }
    window.add_action(&copy);
    let browser = gio::SimpleAction::new("browser", None);
    {
        let (v, win) = (view.clone(), window.clone());
        browser.connect_activate(move |_, _| {
            if let Some(u) = v.uri() {
                open_external(&win, &u);
            }
        });
    }
    window.add_action(&browser);
    app.set_accels_for_action("win.zoom-in", &["<Ctrl>plus", "<Ctrl>equal"]);
    app.set_accels_for_action("win.zoom-out", &["<Ctrl>minus"]);
    let reload_a = gio::SimpleAction::new("reload", None);
    {
        let v = view.clone();
        reload_a.connect_activate(move |_, _| v.reload());
    }
    window.add_action(&reload_a);
    app.set_accels_for_action("win.reload", &["F5", "<Ctrl>r"]);

    view.load_uri(&args.url);
    window.present();
}
