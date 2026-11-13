//! Aktualizacje: stan (makieta): „System jest aktualny / Shrimp 26 / Sprawdź aktualizacje”, a pod nim
//! przełącznik aktualizacji w tle (domyślnie włączony, zmiany po restarcie) i sterowniki.

use adw::prelude::*;
use gtk::glib;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::{self, App, Op};
use crate::zypper;

#[derive(Clone, PartialEq)]
enum State {
    Ok,
    Available(usize),
    Done,
}

pub struct Updates {
    pub widget: gtk::ScrolledWindow,
    auto: adw::SwitchRow,
    drivers: adw::ActionRow,
    drivers_button: gtk::Button,
    /// Blokada sygnału przełącznika przy ustawianiu stanu z systemu.
    syncing: std::cell::Cell<bool>,
    icon: gtk::Image,
    title: gtk::Label,
    sub: gtk::Label,
    button: gtk::Button,
    state: RefCell<State>,
    os: String,
}

impl Updates {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let top = gtk::Box::new(gtk::Orientation::Vertical, 0);
        top.set_halign(gtk::Align::Center);
        top.set_margin_top(48);
        let icon = gtk::Image::new();
        icon.set_pixel_size(64);
        icon.set_margin_bottom(24);
        let title = gtk::Label::new(None);
        title.add_css_class("title-2");
        title.set_wrap(true);
        title.set_justify(gtk::Justification::Center);
        let sub = gtk::Label::new(None);
        sub.add_css_class("dim-label");
        sub.set_margin_top(6);
        let button = gtk::Button::new();
        button.add_css_class("suggested-action");
        button.add_css_class("cocktail-main");
        button.set_halign(gtk::Align::Center);
        button.set_margin_top(30);
        top.append(&icon);
        top.append(&title);
        top.append(&sub);
        top.append(&button);

        let auto = adw::SwitchRow::new();
        auto.set_title(crate::i18n::tr("auto.title"));
        auto.set_subtitle(crate::i18n::tr("auto.sub"));
        let drivers = adw::ActionRow::new();
        drivers.set_title(crate::i18n::tr("drivers.title"));
        drivers.set_subtitle(crate::i18n::tr("state.checking"));
        let drivers_button = gtk::Button::with_label(crate::i18n::tr("button.update"));
        drivers_button.set_valign(gtk::Align::Center);
        drivers_button.set_visible(false);
        drivers.add_suffix(&drivers_button);
        let group = adw::PreferencesGroup::new();
        group.add(&auto);
        group.add(&drivers);
        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(560);
        clamp.set_margin_top(36);
        clamp.set_margin_start(24);
        clamp.set_margin_end(24);
        clamp.set_margin_bottom(24);
        clamp.set_child(Some(&group));
        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.append(&top);
        body.append(&clamp);
        let widget = gtk::ScrolledWindow::new();
        widget.set_hscrollbar_policy(gtk::PolicyType::Never);
        widget.set_vexpand(true);
        widget.set_child(Some(&body));

        let os = std::fs::read_to_string("/etc/os-release").map(|s| zypper::os_name(&s)).unwrap_or_else(|_| "Linux".into());
        let page = Rc::new(Updates {
            widget,
            auto,
            drivers,
            drivers_button,
            syncing: std::cell::Cell::new(false),
            icon,
            title,
            sub,
            button,
            state: RefCell::new(State::Ok),
            os,
        });
        page.show(State::Ok);

        let (p, a) = (page.clone(), app.clone());
        page.button.connect_clicked(move |_| {
            let (p, a) = (p.clone(), a.clone());
            glib::spawn_future_local(async move { p.clicked(&a).await });
        });

        let (p, a) = (page.clone(), app.clone());
        page.auto.connect_active_notify(move |_| {
            if p.syncing.get() {
                return;
            }
            let (p, a) = (p.clone(), a.clone());
            glib::spawn_future_local(async move { p.set_auto(&a).await });
        });
        let (p, a) = (page.clone(), app.clone());
        page.drivers_button.connect_clicked(move |_| {
            let (p, a) = (p.clone(), a.clone());
            glib::spawn_future_local(async move { p.update_drivers(&a).await });
        });
        let p = page.clone();
        glib::spawn_future_local(async move {
            let on = app::run(&["systemctl", "is-enabled", "transactional-update.timer"]).await;
            p.syncing.set(true);
            p.auto.set_active(on.stdout.trim() == "enabled");
            p.syncing.set(false);
            p.check_drivers().await;
        });

        // Na start bez odświeżania repozytoriów (bez uprawnień, z pamięci podręcznej zyppera).
        let p = page.clone();
        glib::spawn_future_local(async move {
            if let Some(n) = list_updates().await.ok().map(|u| u.len()).filter(|n| *n > 0) {
                if *p.state.borrow() == State::Ok {
                    p.show(State::Available(n));
                }
            }
        });
        page
    }

    fn show(&self, s: State) {
        self.button.set_sensitive(true);
        self.icon.remove_css_class("success");
        match &s {
            State::Ok => {
                self.icon.set_icon_name(Some("object-select-symbolic"));
                self.icon.add_css_class("success");
                self.title.set_text(crate::i18n::tr("updates.uptodate"));
                self.sub.set_text(&self.os);
                self.button.set_label(crate::i18n::tr("updates.check"));
            }
            State::Available(n) => {
                self.icon.set_icon_name(Some("software-update-available-symbolic"));
                self.title.set_text(crate::i18n::tr("updates.available"));
                self.sub.set_text(&crate::i18n::trn("updates.packages", *n as u64, &[]));
                self.button.set_label(crate::i18n::tr("updates.install"));
            }
            State::Done => {
                self.icon.set_icon_name(Some("object-select-symbolic"));
                self.icon.add_css_class("success");
                self.title.set_text(crate::i18n::tr("updates.installed"));
                self.sub.set_text(crate::i18n::tr("updates.restart-to-finish"));
                self.button.set_label(crate::i18n::tr("button.restart"));
            }
        }
        *self.state.borrow_mut() = s;
    }

    async fn clicked(self: Rc<Self>, app: &Rc<App>) {
        let state = self.state.borrow().clone();
        self.button.set_sensitive(false);
        match state {
            State::Ok => {
                self.button.set_label(crate::i18n::tr("state.checking"));
                let r = app::helper("cocktail-query", &["refresh"]).await;
                if r.cancelled() {
                    self.show(State::Ok);
                    return;
                }
                if !r.ok() {
                    self.show(State::Ok);
                    app.alert(crate::i18n::tr("updates.check-failed"), &app::tail(&r)).await;
                    return;
                }
                match list_updates().await {
                    Ok(u) if !u.is_empty() => self.show(State::Available(u.len())),
                    Ok(_) => {
                        self.show(State::Ok);
                        app.toast(crate::i18n::tr("toast.no-updates"));
                    }
                    Err(e) => {
                        self.show(State::Ok);
                        app.alert(crate::i18n::tr("updates.check-failed"), &e).await;
                    }
                }
            }
            State::Available(n) => {
                self.button.set_label(crate::i18n::tr("state.installing"));
                if app.change(Op::Update, Vec::new(), "system").await {
                    if app.banner.is_revealed() {
                        self.show(State::Done);
                    } else {
                        self.show(State::Ok);
                    }
                } else {
                    self.show(State::Available(n));
                }
            }
            State::Done => {
                self.button.set_label(crate::i18n::tr("state.restarting"));
                app::reboot();
            }
        }
    }
}

impl Updates {
    /// Przełącznik aktualizacji w tle: pomocnik przez pkexec; przy odmowie wraca do poprzedniego stanu.
    async fn set_auto(self: Rc<Self>, app: &Rc<App>) {
        let on = self.auto.is_active();
        self.auto.set_sensitive(false);
        let out = app::helper("cocktail-helper", &["auto-updates", if on { "on" } else { "off" }]).await;
        self.auto.set_sensitive(true);
        if !out.ok() {
            self.syncing.set(true);
            self.auto.set_active(!on);
            self.syncing.set(false);
            if !out.cancelled() {
                app.alert(crate::i18n::tr("error.auto"), &app::tail(&out)).await;
            }
        }
    }

    /// Sterowniki do aktualizacji: pakiety sterowników z listy aktualizacji i oprogramowanie układowe (fwupd).
    async fn check_drivers(&self) {
        let pk = list_updates().await.unwrap_or_default().into_iter().filter(|u| zypper::is_driver(&u.name)).count();
        let fw = app::run(&["fwupdmgr", "get-updates", "--json"]).await;
        let n = pk + zypper::fwupd_updates(&fw.stdout);
        if n == 0 {
            self.drivers.set_subtitle(crate::i18n::tr("drivers.uptodate"));
        } else {
            self.drivers.set_subtitle(&crate::i18n::trn("drivers.available", n as u64, &[]));
        }
        self.drivers_button.set_visible(n > 0);
    }

    async fn update_drivers(self: Rc<Self>, app: &Rc<App>) {
        if app.busy.get() {
            app.toast(crate::i18n::tr("toast.busy"));
            return;
        }
        app.busy.set(true);
        self.drivers_button.set_sensitive(false);
        self.drivers.set_subtitle(crate::i18n::tr("state.installing"));
        let out = app::helper("cocktail-helper", &["drivers"]).await;
        app.busy.set(false);
        self.drivers_button.set_sensitive(true);
        if out.ok() {
            if out.stdout.lines().any(|l| l.trim() == "reboot") {
                app.needs_reboot();
            }
            self.drivers_button.set_visible(false);
            self.drivers.set_subtitle(crate::i18n::tr(if app.banner.is_revealed() { "updates.restart-to-finish" } else { "drivers.uptodate" }));
        } else {
            self.check_drivers().await;
            if !out.cancelled() {
                app.alert(crate::i18n::tr("error.drivers"), &app::tail(&out)).await;
            }
        }
    }
}

async fn list_updates() -> Result<Vec<zypper::Update>, String> {
    let out = app::run(&["zypper", "--xmlout", "--non-interactive", "--no-refresh", "list-updates"]).await;
    if !out.ok() {
        return Err(app::tail(&out));
    }
    zypper::parse_updates(&out.stdout)
}
