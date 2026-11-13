//! Okno Panko: pasek tytułu, pasek boczny (Migawki, Zmiany, Zaawansowane, wyszukiwanie),
//! strony i stany (wczytywanie, brak dostępu, migawki niedostępne).

mod advanced;
mod changes;
mod snapshots;
mod tree;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::backend::{Backend, Error};
use crate::model::{Config, Snapshot};

pub const PAGES: [(&str, &str, &str); 3] = [
    ("migawki", "page.snapshots", "document-open-recent-symbolic"),
    ("zmiany", "page.changes", "document-edit-symbolic"),
    (
        "zaawansowane",
        "page.advanced",
        "applications-system-symbolic",
    ),
];

type StatusAction = Box<dyn Fn(&Rc<App>)>;
type Pending = (Vec<(String, String)>, Option<glib::SourceId>);

pub struct App {
    pub backend: Arc<dyn Backend>,
    pub window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    /// "main" = strony, "status" = stan zamiast stron, "loading".
    body: gtk::Stack,
    status: adw::StatusPage,
    status_button: gtk::Button,
    status_action: RefCell<Option<StatusAction>>,
    pages: gtk::Stack,
    sidebar: gtk::ListBox,
    search: gtk::SearchEntry,
    no_results: gtk::Label,
    pub configs: RefCell<Vec<Config>>,
    pub snaps: RefCell<Vec<Snapshot>>,
    /// Blokada sygnałów przy wypełnianiu kontrolek z danych.
    pub updating: Cell<bool>,
    pub snap_page: RefCell<Option<snapshots::Page>>,
    pub chg_page: RefCell<Option<changes::Page>>,
    pub adv_page: RefCell<Option<advanced::Page>>,
    menu_timeline: gtk::Switch,
    menu_days: gtk::SpinButton,
    menu_age_on: gtk::Switch,
    max_age: Cell<Option<u32>>,
}

/// Uruchamia `work` w wątku i oddaje wynik do `done` w wątku interfejsu.
pub fn run<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    done: impl FnOnce(T) + 'static,
) {
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(work).await {
            Ok(v) => done(v),
            Err(_) => eprintln!("panko: wątek roboczy przerwał pracę"),
        }
    });
}

const CSS: &str = r#"
.panko-side { background-color: @sidebar_bg_color; }
.panko-side > headerbar, .panko-content > headerbar { background: transparent; box-shadow: none; }
.panko-side-body { padding: 0 8px 12px 8px; }
.panko-side .navigation-sidebar { background: transparent; }
.panko-side .navigation-sidebar > row { min-height: 36px; font-weight: 600; }
.panko-side .navigation-sidebar > row:selected { font-weight: 700; }
.panko-content { background-color: @window_bg_color; }
.panko-toolbar { padding: 0 12px 8px 24px; min-height: 34px; }
.panko-main { min-width: 184px; }
.panko-tree, .panko-tree > row { background: transparent; }
.panko-tree > row { padding: 0; border-radius: 0; }
.panko-node.st-a { background-color: shade(@window_bg_color, 1.07); }
.panko-node.sel { background-color: @accent_bg_color; color: @accent_fg_color; }
.panko-node.sel .dim-label, .panko-node.sel .panko-time { color: @accent_fg_color; opacity: 1; }
.panko-node { min-height: 26px; padding: 1px 24px; }
.panko-node.panko-child { padding-left: 44px; }
.panko-time { opacity: 0.7; font-size: 0.93em; padding-left: 12px; }
.mk-add { color: @success_color; }
.mk-rem { color: @warning_color; }
.mk-chg { color: @accent_color; }
.panko-sel-name { font-weight: 600; opacity: 0.7; }
.panko-empty { opacity: 0.7; padding: 8px 24px; }
.panko-sample { font-size: 0.85em; opacity: 0.7; padding: 0 24px 24px; }
.panko-menu-row { padding: 6px 8px; }
"#;

pub fn build(app: &adw::Application, backend: Arc<dyn Backend>) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("ekran"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Panko")
        .default_width(880)
        .default_height(640)
        .width_request(360)
        .height_request(400)
        .build();

    // Bez wspólnego paska tytułu: pasek boczny sięga od góry do dołu okna (pomoc w jego rogu),
    // treść ma własny przezroczysty pasek z menu i przyciskami okna.
    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    header.set_show_end_title_buttons(false);
    let content_header = adw::HeaderBar::new();
    content_header.set_show_title(false);
    content_header.set_show_start_title_buttons(false);
    let help = gtk::Button::from_icon_name("dialog-question-symbolic");
    help.set_tooltip_text(Some(crate::i18n::tr("help")));
    match crate::help::url() {
        Some(url) => {
            help.connect_clicked(move |b| {
                let root = b.root().and_downcast::<gtk::Window>();
                gtk::UriLauncher::new(&url).launch(root.as_ref(), gio::Cancellable::NONE, |_| {});
            });
        }
        // Strony pomocy jeszcze nie ma (boxes/help-links/help.json › base): sama podpowiedź.
        None => help.set_sensitive(false),
    }
    header.pack_start(&help);

    let (menu_button, menu_timeline, menu_days, menu_age_on, menu_about) = build_menu();
    content_header.pack_end(&menu_button);

    // Pasek boczny.
    let sidebar = gtk::ListBox::new();
    sidebar.add_css_class("navigation-sidebar");
    for (_, title, icon) in PAGES {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        b.append(&gtk::Image::from_icon_name(icon));
        b.append(&gtk::Label::new(Some(crate::i18n::tr(title))));
        sidebar.append(&b);
    }
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some(crate::i18n::tr("search.placeholder")));
    search.set_valign(gtk::Align::End);
    search.set_vexpand(true);
    let side_body = gtk::Box::new(gtk::Orientation::Vertical, 2);
    side_body.add_css_class("panko-side-body");
    side_body.set_vexpand(true);
    side_body.append(&sidebar);
    side_body.append(&search);
    let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
    side.add_css_class("panko-side");
    side.set_size_request(200, -1);
    side.append(&header);
    side.append(&side_body);

    let pages = gtk::Stack::new();
    pages.set_transition_type(gtk::StackTransitionType::Crossfade);
    pages.set_vexpand(true);
    let no_results = gtk::Label::new(Some(crate::i18n::tr("search.none")));
    no_results.add_css_class("panko-empty");
    no_results.set_xalign(0.0);
    no_results.set_visible(false);
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.append(&pages);
    content.append(&no_results);
    if backend.is_demo() {
        let sample = gtk::Label::new(Some(crate::i18n::tr("demo.note")));
        sample.add_css_class("panko-sample");
        sample.set_xalign(0.0);
        content.append(&sample);
    }

    let main = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    main.append(&content);

    let status = adw::StatusPage::new();
    let status_button = gtk::Button::new();
    status_button.add_css_class("suggested-action");
    status_button.add_css_class("pill");
    status_button.set_halign(gtk::Align::Center);
    status.set_child(Some(&status_button));
    let loading = gtk::Spinner::new();
    loading.set_spinning(true);
    loading.set_size_request(32, 32);
    loading.set_halign(gtk::Align::Center);
    loading.set_valign(gtk::Align::Center);

    let body = gtk::Stack::new();
    body.add_named(&main, Some("main"));
    body.add_named(&status, Some("status"));
    body.add_named(&loading, Some("loading"));
    body.set_visible_child_name("loading");
    // Pasek boczny jest stale widoczny; bez stron (wczytywanie, brak dostępu) jest nieaktywny.
    side_body.set_sensitive(false);
    {
        let side_body = side_body.clone();
        body.connect_visible_child_name_notify(move |b| {
            side_body.set_sensitive(b.visible_child_name().as_deref() == Some("main"));
        });
    }

    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&body));
    let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
    right.add_css_class("panko-content");
    right.set_hexpand(true);
    right.append(&content_header);
    right.append(&toasts);
    toasts.set_vexpand(true);
    let window_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    window_box.append(&side);
    window_box.append(&right);
    window.set_content(Some(&window_box));

    let this = Rc::new(App {
        backend,
        window: window.clone(),
        toasts,
        body,
        status,
        status_button: status_button.clone(),
        status_action: RefCell::new(None),
        pages: pages.clone(),
        sidebar: sidebar.clone(),
        search: search.clone(),
        no_results,
        configs: RefCell::new(vec![]),
        snaps: RefCell::new(vec![]),
        updating: Cell::new(false),
        snap_page: RefCell::new(None),
        chg_page: RefCell::new(None),
        adv_page: RefCell::new(None),
        menu_timeline: menu_timeline.clone(),
        menu_days: menu_days.clone(),
        menu_age_on: menu_age_on.clone(),
        max_age: Cell::new(None),
    });

    let sp = snapshots::Page::new(&this);
    pages.add_named(&sp.root, Some("migawki"));
    *this.snap_page.borrow_mut() = Some(sp);
    let cp = changes::Page::new(&this);
    pages.add_named(&cp.root, Some("zmiany"));
    *this.chg_page.borrow_mut() = Some(cp);
    let ap = advanced::Page::new(&this);
    pages.add_named(&ap.root, Some("zaawansowane"));
    *this.adv_page.borrow_mut() = Some(ap);

    let weak = Rc::downgrade(&this);
    sidebar.connect_row_selected(move |_, row| {
        let (Some(app), Some(row)) = (weak.upgrade(), row) else {
            return;
        };
        let name = PAGES[row.index().max(0) as usize].0;
        app.pages.set_visible_child_name(name);
        app.apply_search();
        if name == "zmiany" {
            app.changes_shown();
        }
    });
    sidebar.select_row(sidebar.row_at_index(0).as_ref());

    let weak = Rc::downgrade(&this);
    search.connect_search_changed(move |_| {
        if let Some(app) = weak.upgrade() {
            app.apply_search();
        }
    });

    let weak = Rc::downgrade(&this);
    status_button.connect_clicked(move |_| {
        let Some(app) = weak.upgrade() else { return };
        let action = app.status_action.borrow_mut().take();
        if let Some(f) = action {
            f(&app);
        }
    });

    // Menu główne: te same ustawienia co w Zaawansowane, skrót do najczęstszych.
    let weak = Rc::downgrade(&this);
    menu_timeline.connect_active_notify(move |s| {
        let Some(app) = weak.upgrade() else { return };
        if !app.updating.get() {
            app.queue_setting("TIMELINE_CREATE", if s.is_active() { "yes" } else { "no" });
        }
    });
    // „Usuwaj migawki starsze niż N dni”: przełącznik + liczba, zapis jednym pkexec po chwili.
    let weak = Rc::downgrade(&this);
    menu_age_on.connect_active_notify(move |s| {
        let Some(app) = weak.upgrade() else { return };
        app.menu_days.set_sensitive(s.is_active());
        if !app.updating.get() {
            app.queue_max_age();
        }
    });
    let weak = Rc::downgrade(&this);
    menu_days.connect_value_changed(move |_| {
        let Some(app) = weak.upgrade() else { return };
        if !app.updating.get() {
            app.queue_max_age();
        }
    });
    let win = window.clone();
    menu_about.connect_clicked(move |b| {
        if let Some(p) = b
            .ancestor(gtk::Popover::static_type())
            .and_downcast::<gtk::Popover>()
        {
            p.popdown();
        }
        let about = adw::AboutDialog::builder()
            .application_name("Panko")
            .developer_name("Shrimp")
            .version(env!("CARGO_PKG_VERSION"))
            .comments(crate::i18n::tr("about.comments"))
            .license_type(gtk::License::Gpl30)
            .build();
        about.present(Some(&win));
    });

    // Okno trzyma stan aplikacji; sygnały mają tylko słabe odwołania.
    unsafe {
        window.set_data("panko-app", this.clone());
    }
    window.present();
    this.reload();

    // Do zrzutów ekranu i testów: PANKO_PAGE=zmiany|zaawansowane, PANKO_SELECT=<numer migawki>.
    if let Ok(page) = std::env::var("PANKO_PAGE") {
        this.show_page(&page);
    }
    if let Some(num) = std::env::var("PANKO_SELECT")
        .ok()
        .and_then(|v| v.parse().ok())
    {
        let weak = Rc::downgrade(&this);
        glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
            if let Some(app) = weak.upgrade() {
                if app.current_page() == "zmiany" {
                    app.show_changes_for(num);
                } else {
                    app.select_snap(num);
                }
            }
        });
    }
}

fn build_menu() -> (
    gtk::MenuButton,
    gtk::Switch,
    gtk::SpinButton,
    gtk::Switch,
    gtk::Button,
) {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    b.set_size_request(290, -1);
    let row = |label: &str, w: &gtk::Widget| {
        let r = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        r.add_css_class("panko-menu-row");
        let l = gtk::Label::new(Some(label));
        l.set_xalign(0.0);
        l.set_hexpand(true);
        r.append(&l);
        r.append(w);
        r
    };
    // Aktualizacje systemu zawsze robią migawkę (transactional-update), tu tylko informacja.
    let before_update = gtk::Switch::new();
    before_update.set_active(true);
    before_update.set_sensitive(false);
    before_update.set_valign(gtk::Align::Center);
    before_update.update_property(&[gtk::accessible::Property::Label(
        crate::i18n::tr("menu.before-update"),
    )]);
    b.append(&row(
        crate::i18n::tr("menu.before-update"),
        before_update.upcast_ref(),
    ));
    let timeline = gtk::Switch::new();
    timeline.set_valign(gtk::Align::Center);
    timeline.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("menu.timeline"))]);
    b.append(&row(crate::i18n::tr("menu.timeline"), timeline.upcast_ref()));
    // Usługa migawek nie ma limitu wieku: Panko czyści sam (retention.rs, panko-cleanup.timer).
    let age_on = gtk::Switch::new();
    age_on.set_valign(gtk::Align::Center);
    age_on.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("menu.max-age"))]);
    b.append(&row(crate::i18n::tr("menu.max-age"), age_on.upcast_ref()));
    let days = gtk::SpinButton::with_range(1.0, f64::from(crate::retention::MAX_DAYS), 1.0);
    days.set_valign(gtk::Align::Center);
    days.set_value(30.0);
    days.set_sensitive(false);
    days.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("menu.days-a11y"))]);
    let num = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    num.append(&days);
    let d = gtk::Label::new(Some(crate::i18n::tr("menu.days")));
    d.add_css_class("dim-label");
    num.append(&d);
    b.append(&row(crate::i18n::tr("menu.older-than"), num.upcast_ref()));
    let about = gtk::Button::with_label(crate::i18n::tr("menu.about"));
    about.add_css_class("flat");
    if let Some(l) = about.child().and_downcast::<gtk::Label>() {
        l.set_xalign(0.0);
    }
    b.append(&about);
    let pop = gtk::Popover::new();
    pop.set_child(Some(&b));
    let mb = gtk::MenuButton::new();
    mb.set_icon_name("open-menu-symbolic");
    mb.set_tooltip_text(Some(crate::i18n::tr("menu.main")));
    mb.set_popover(Some(&pop));
    (mb, timeline, days, age_on, about)
}

impl App {
    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    /// Komunikat o błędzie. Zamknięte okno hasła nie jest błędem.
    pub fn fail(&self, text: &str, e: &Error) {
        eprintln!("panko: {text} ({e})");
        if *e != Error::Cancelled {
            self.toast(text);
        }
    }

    pub fn root_config(&self) -> Option<Config> {
        self.configs
            .borrow()
            .iter()
            .find(|c| c.subvolume == "/")
            .cloned()
            .or_else(|| self.configs.borrow().first().cloned())
    }

    fn show_status(
        self: &Rc<Self>,
        icon: &str,
        title: &str,
        text: &str,
        button: Option<&str>,
        action: impl Fn(&Rc<App>) + 'static,
    ) {
        self.status.set_icon_name(Some(icon));
        self.status.set_title(title);
        self.status.set_description(Some(text));
        self.status_button.set_visible(button.is_some());
        self.status_button.set_label(button.unwrap_or(""));
        *self.status_action.borrow_mut() = Some(Box::new(action));
        self.body.set_visible_child_name("status");
    }

    /// Wczytuje konfiguracje i migawki systemu od nowa.
    pub fn reload(self: &Rc<Self>) {
        let backend = self.backend.clone();
        let weak = Rc::downgrade(self);
        run(
            move || {
                let configs = backend.configs()?;
                let root = configs
                    .iter()
                    .find(|c| c.subvolume == "/")
                    .or(configs.first())
                    .cloned();
                let snaps = match &root {
                    Some(c) => backend.snapshots(&c.name)?,
                    None => vec![],
                };
                Ok::<_, Error>((configs, snaps, backend.max_age()))
            },
            move |r| {
                let Some(app) = weak.upgrade() else { return };
                match r {
                    Ok((configs, _, _)) if configs.is_empty() => app.show_status(
                        "document-open-recent-symbolic",
                        crate::i18n::tr("status.off"),
                        crate::i18n::tr("status.off-body"),
                        Some(crate::i18n::tr("button.turn-on")),
                        |app| app.enable_volume("root", "/"),
                    ),
                    Ok((configs, snaps, max_age)) => {
                        app.max_age.set(max_age);
                        *app.configs.borrow_mut() = configs;
                        *app.snaps.borrow_mut() = snaps;
                        app.body.set_visible_child_name("main");
                        app.refresh_all();
                    }
                    Err(Error::NoPermission) => app.show_status(
                        "security-high-symbolic",
                        crate::i18n::tr("status.no-access"),
                        crate::i18n::tr("status.no-access-body"),
                        Some(crate::i18n::tr("button.allow")),
                        |app| app.grant_access(),
                    ),
                    Err(e) => {
                        eprintln!("panko: {e}");
                        app.show_status(
                            "dialog-warning-symbolic",
                            crate::i18n::tr("status.unavailable"),
                            crate::i18n::tr("status.unavailable-body"),
                            Some(crate::i18n::tr("button.retry")),
                            |app| app.reload(),
                        )
                    }
                }
            },
        );
    }

    fn grant_access(self: &Rc<Self>) {
        let backend = self.backend.clone();
        let weak = Rc::downgrade(self);
        let user = crate::backend::current_user();
        run(
            move || {
                let configs = backend.configs()?;
                backend.grant_access(&user, &configs)
            },
            move |r| {
                let Some(app) = weak.upgrade() else { return };
                match r {
                    Ok(()) => app.reload(),
                    Err(e) => {
                        app.fail(crate::i18n::tr("error.allow"), &e);
                        // Przycisk znowu działa.
                        *app.status_action.borrow_mut() =
                            Some(Box::new(|app: &Rc<App>| app.grant_access()));
                    }
                }
            },
        );
    }

    pub fn enable_volume(self: &Rc<Self>, name: &str, subvolume: &str) {
        let backend = self.backend.clone();
        let weak = Rc::downgrade(self);
        let (name, subvolume) = (name.to_owned(), subvolume.to_owned());
        run(
            move || backend.enable_volume(&name, &subvolume),
            move |r| {
                let Some(app) = weak.upgrade() else { return };
                if let Err(e) = r {
                    app.fail(crate::i18n::tr("error.enable"), &e);
                }
                app.reload();
            },
        );
    }

    pub fn refresh_all(self: &Rc<Self>) {
        self.updating.set(true);
        if let Some(c) = self.root_config() {
            self.menu_timeline.set_active(c.flag("TIMELINE_CREATE"));
        }
        let age = self.max_age.get();
        self.menu_age_on.set_active(age.is_some());
        self.menu_days.set_sensitive(age.is_some());
        if let Some(d) = age {
            self.menu_days.set_value(f64::from(d));
        }
        self.updating.set(false);
        self.refresh_snapshots();
        self.refresh_changes_choices();
        self.refresh_advanced();
        self.apply_search();
    }

    pub fn current_page(&self) -> String {
        self.pages
            .visible_child_name()
            .map(|s| s.to_string())
            .unwrap_or_default()
    }

    pub fn show_page(&self, name: &str) {
        if let Some(i) = PAGES.iter().position(|p| p.0 == name) {
            self.sidebar
                .select_row(self.sidebar.row_at_index(i as i32).as_ref());
        }
    }

    pub fn query(&self) -> String {
        self.search.text().trim().to_lowercase()
    }

    /// Jedno pole wyszukiwania filtruje listę otwartej strony.
    pub fn apply_search(self: &Rc<Self>) {
        let shown = match self.current_page().as_str() {
            "migawki" => self.filter_snapshots(),
            "zmiany" => self.filter_changes(),
            _ => self.filter_advanced(),
        };
        self.no_results
            .set_visible(!self.query().is_empty() && shown == 0);
    }

    /// Okno potwierdzenia jak w makiecie: Anuluj + przycisk akcji.
    pub fn confirm(
        &self,
        title: &str,
        text: &str,
        ok: &str,
        destructive: bool,
        entry: Option<&gtk::Entry>,
        on_ok: impl Fn() + 'static,
    ) {
        let d = adw::AlertDialog::new(Some(title), if text.is_empty() { None } else { Some(text) });
        d.add_response("cancel", crate::i18n::tr("button.cancel"));
        d.add_response("ok", ok);
        d.set_response_appearance(
            "ok",
            if destructive {
                adw::ResponseAppearance::Destructive
            } else {
                adw::ResponseAppearance::Suggested
            },
        );
        d.set_default_response(Some("ok"));
        d.set_close_response("cancel");
        if let Some(e) = entry {
            d.set_extra_child(Some(e));
            let d2 = d.clone();
            e.connect_activate(move |_| {
                d2.emit_by_name::<()>("response", &[&"ok"]);
                d2.close();
            });
        }
        d.connect_response(None, move |_, r| {
            if r == "ok" {
                on_ok();
            }
        });
        d.present(Some(&self.window));
        if let Some(e) = entry {
            e.grab_focus();
        }
    }

    /// Zmiana ustawienia czeka chwilę, żeby kilka kliknięć dało jedno okno hasła.
    pub fn queue_setting(self: &Rc<Self>, key: &str, value: &str) {
        thread_local! {
            static PENDING: RefCell<Pending> = const { RefCell::new((vec![], None)) };
        }
        PENDING.with(|p| {
            let mut p = p.borrow_mut();
            p.0.retain(|(k, _)| k != key);
            p.0.push((key.to_owned(), value.to_owned()));
            if let Some(id) = p.1.take() {
                id.remove();
            }
            let weak = Rc::downgrade(self);
            p.1 = Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(900),
                move || {
                    let values = PENDING.with(|p| {
                        let mut p = p.borrow_mut();
                        p.1 = None;
                        std::mem::take(&mut p.0)
                    });
                    if let Some(app) = weak.upgrade() {
                        app.save_settings(values);
                    }
                },
            ));
        });
    }

    /// Wiek migawek: kilka kliknięć daje jedno okno hasła, jak w queue_setting.
    pub fn queue_max_age(self: &Rc<Self>) {
        thread_local! {
            static PENDING: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
        }
        PENDING.with(|p| {
            if let Some(id) = p.borrow_mut().take() {
                id.remove();
            }
            let weak = Rc::downgrade(self);
            *p.borrow_mut() = Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(900),
                move || {
                    PENDING.with(|p| p.borrow_mut().take());
                    let Some(app) = weak.upgrade() else { return };
                    let days = app
                        .menu_age_on
                        .is_active()
                        .then(|| app.menu_days.value_as_int() as u32);
                    let backend = app.backend.clone();
                    let weak = Rc::downgrade(&app);
                    run(
                        move || backend.set_max_age(days),
                        move |r| {
                            let Some(app) = weak.upgrade() else { return };
                            if let Err(e) = r {
                                app.fail(crate::i18n::tr("error.settings"), &e);
                            }
                            app.reload();
                        },
                    );
                },
            ));
        });
    }

    fn save_settings(self: &Rc<Self>, values: Vec<(String, String)>) {
        let Some(cfg) = self.root_config() else {
            return;
        };
        let backend = self.backend.clone();
        let weak = Rc::downgrade(self);
        run(
            move || backend.set_config(&cfg.name, &values),
            move |r| {
                let Some(app) = weak.upgrade() else { return };
                if let Err(e) = r {
                    app.fail(crate::i18n::tr("error.settings"), &e);
                }
                // Po zapisie albo odmowie kontrolki pokazują prawdziwy stan.
                app.reload();
            },
        );
    }
}
