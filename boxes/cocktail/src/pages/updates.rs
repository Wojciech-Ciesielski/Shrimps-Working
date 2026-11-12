//! Aktualizacje: tylko stan (makieta): „System jest aktualny / Shrimp 26 / Sprawdź aktualizacje”.

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
    pub widget: gtk::Box,
    icon: gtk::Image,
    title: gtk::Label,
    sub: gtk::Label,
    button: gtk::Button,
    state: RefCell<State>,
    os: String,
}

impl Updates {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.set_valign(gtk::Align::Center);
        widget.set_halign(gtk::Align::Center);
        widget.set_vexpand(true);
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
        widget.append(&icon);
        widget.append(&title);
        widget.append(&sub);
        widget.append(&button);

        let os = std::fs::read_to_string("/etc/os-release").map(|s| zypper::os_name(&s)).unwrap_or_else(|_| "Linux".into());
        let page = Rc::new(Updates { widget, icon, title, sub, button, state: RefCell::new(State::Ok), os });
        page.show(State::Ok);

        let (p, a) = (page.clone(), app.clone());
        page.button.connect_clicked(move |_| {
            let (p, a) = (p.clone(), a.clone());
            glib::spawn_future_local(async move { p.clicked(&a).await });
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

async fn list_updates() -> Result<Vec<zypper::Update>, String> {
    let out = app::run(&["zypper", "--xmlout", "--non-interactive", "--no-refresh", "list-updates"]).await;
    if !out.ok() {
        return Err(app::tail(&out));
    }
    zypper::parse_updates(&out.stdout)
}
