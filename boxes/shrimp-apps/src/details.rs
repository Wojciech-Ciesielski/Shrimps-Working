//! Strona aplikacji: nagłówek z akcjami, zrzuty ekranu, opis, krótkie informacje na dole.

use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use crate::backend::{Cmd, OpKind};
use crate::catalog::App;
use crate::store::{label_for, Status, Store};
use crate::widgets::{self, wrap_label};

pub fn page(store: &Rc<Store>, app: &Rc<App>, window: &gtk::Window) -> adw::NavigationPage {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 24);
    col.add_css_class("details");

    // Nagłówek: ikona, nazwa, twórca, akcje.
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 18);
    head.append(&widgets::app_icon(Some(app), &app.id, 128));
    let texts = gtk::Box::new(gtk::Orientation::Vertical, 4);
    texts.set_valign(gtk::Align::Center);
    texts.set_hexpand(true);
    let name = wrap_label(&app.name, &["title-1"]);
    texts.append(&name);
    if let Some(dev) = &app.developer {
        let dev_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        dev_box.append(&wrap_label(dev, &["dim-label"]));
        if app.verified {
            let v = gtk::Image::from_icon_name(widgets::pick_icon(widgets::ICON_VERIFIED));
            v.add_css_class("accent");
            v.set_tooltip_text(Some(crate::i18n::tr("details.verified")));
            dev_box.append(&v);
        }
        texts.append(&dev_box);
    }
    if !app.summary.is_empty() {
        texts.append(&wrap_label(&app.summary, &[]));
    }
    head.append(&texts);

    let actions = gtk::Box::new(gtk::Orientation::Vertical, 6);
    actions.set_valign(gtk::Align::Center);
    let main_btn = gtk::Button::builder().width_request(140).build();
    main_btn.add_css_class("pill");
    main_btn.add_css_class("suggested-action");
    let remove_btn = gtk::Button::builder().label(crate::i18n::tr("button.remove")).width_request(140).build();
    remove_btn.add_css_class("pill");
    let progress = gtk::ProgressBar::builder().width_request(140).build();
    let cancel_btn = gtk::Button::builder().label(crate::i18n::tr("button.cancel")).width_request(140).build();
    cancel_btn.add_css_class("pill");
    actions.append(&main_btn);
    actions.append(&progress);
    actions.append(&remove_btn);
    actions.append(&cancel_btn);
    head.append(&actions);
    col.append(&head);

    let update = {
        let store = store.clone();
        let id = app.id.clone();
        let (main_btn, remove_btn, progress, cancel_btn) =
            (main_btn.clone(), remove_btn.clone(), progress.clone(), cancel_btn.clone());
        move || {
            let s = store.status(&id);
            main_btn.set_label(&label_for(s));
            main_btn.set_sensitive(!s.busy());
            main_btn.set_visible(!s.busy());
            progress.set_visible(s.busy());
            cancel_btn.set_visible(matches!(s, Status::Busy(k, _) | Status::Queued(k) if k != OpKind::Uninstall));
            match s {
                Status::Busy(_, f) => {
                    progress.set_fraction(f);
                    progress.set_text(Some(&label_for(s)));
                    progress.set_show_text(true);
                }
                _ => {
                    progress.set_fraction(0.0);
                    progress.set_text(Some(&label_for(s)));
                    progress.set_show_text(true);
                }
            }
            remove_btn.set_visible(matches!(s, Status::Installed | Status::UpdateAvailable));
        }
    };
    update();
    {
        let store2 = store.clone();
        let id = app.id.clone();
        main_btn.connect_clicked(move |_| store2.primary(&id));
    }
    {
        let store2 = store.clone();
        let id = app.id.clone();
        cancel_btn.connect_clicked(move |_| store2.cancel(&id));
    }
    {
        let store2 = store.clone();
        let app2 = app.clone();
        let window = window.clone();
        remove_btn.connect_clicked(move |_| confirm_remove(&store2, &app2, &window));
    }

    // Zrzuty ekranu
    if !app.screenshots.is_empty() {
        col.append(&screenshots(app));
    }

    // Opis
    if !app.description.is_empty() {
        let d = wrap_label(&app.description, &["body", "description"]);
        d.set_selectable(true);
        d.set_can_focus(false);
        col.append(&d);
    }

    // Krótkie informacje na dole
    let info = gtk::Grid::builder().column_spacing(48).row_spacing(12).build();
    info.add_css_class("app-info");
    let size_value = gtk::Label::builder().label("…").xalign(0.0).build();
    let mut cells: Vec<(&str, gtk::Widget)> = Vec::new();
    if let Some(v) = app.version() {
        cells.push((crate::i18n::tr("details.version"), value_label(v).upcast()));
    }
    cells.push((crate::i18n::tr("details.size"), size_value.clone().upcast()));
    if let Some(c) = app.main_category() {
        cells.push((crate::i18n::tr("details.category"), value_label(c.title()).upcast()));
    }
    if let Some(l) = &app.license {
        cells.push((crate::i18n::tr("details.license"), value_label(&license_text(l)).upcast()));
    }
    if let Some(d) = &app.developer {
        cells.push((crate::i18n::tr("details.developer"), value_label(d).upcast()));
    }
    if let Some(h) = &app.homepage {
        let b = gtk::LinkButton::builder().uri(h).label(crate::i18n::tr("button.open")).halign(gtk::Align::Start).build();
        b.add_css_class("info-link");
        cells.push((crate::i18n::tr("details.website"), b.upcast()));
    }
    for (i, (k, v)) in cells.into_iter().enumerate() {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let kl = gtk::Label::builder().label(k).xalign(0.0).build();
        kl.add_css_class("caption");
        kl.add_css_class("dim-label");
        cell.append(&kl);
        cell.append(&v);
        info.attach(&cell, (i % 3) as i32, (i / 3) as i32, 1, 1);
    }
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("row-separator");
    col.append(&sep);
    col.append(&info);

    {
        let store2 = store.clone();
        let id = app.id.clone();
        let size_value = size_value.clone();
        let update = update.clone();
        let set_size = move || {
            let installed = store2.installed.borrow().get(&id).map(|i| i.installed_size).filter(|s| *s > 0);
            let remote = store2.sizes.borrow().get(&id).copied();
            let text = match (installed, remote) {
                (Some(i), _) => widgets::format_size(i),
                (None, Some((d, _))) if d > 0 => crate::i18n::trf("details.download-size", &[("size", &widgets::format_size(d))]),
                (None, Some((_, i))) => widgets::format_size(i),
                _ => "—".into(),
            };
            size_value.set_label(&text);
        };
        set_size();
        store.watch(&app.id, &col, move || {
            update();
            set_size();
        });
    }
    if !store.sizes.borrow().contains_key(&app.id) {
        store.send(Cmd::Size { app_id: app.id.clone(), flatpak_ref: app.flatpak_ref.clone() });
    }

    let clamp = adw::Clamp::builder().maximum_size(960).child(&col).build();
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build();
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::builder().show_title(false).build());
    tv.set_content(Some(&scroll));
    adw::NavigationPage::builder().title(&app.name).tag(format!("app:{}", app.id)).child(&tv).build()
}

fn value_label(t: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(t)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(28)
        .build()
}

fn license_text(spdx: &str) -> String {
    if spdx.starts_with("LicenseRef-proprietary") {
        crate::i18n::tr("license.proprietary").into()
    } else {
        spdx.replace(" AND ", &format!(" {} ", crate::i18n::tr("license.and"))).replace(" OR ", &format!(" {} ", crate::i18n::tr("license.or")))
    }
}

fn screenshots(app: &App) -> gtk::Widget {
    let carousel = adw::Carousel::builder().spacing(12).allow_scroll_wheel(false).height_request(380).build();
    carousel.add_css_class("screenshots");
    for s in &app.screenshots {
        let pic = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .can_shrink(true)
            .hexpand(true)
            .vexpand(true)
            .build();
        if let Some(c) = &s.caption {
            pic.set_alternative_text(Some(c));
            pic.set_tooltip_text(Some(c));
        }
        let url = if s.medium.is_empty() { s.full.clone() } else { s.medium.clone() };
        let weak = pic.downgrade();
        glib::spawn_future_local(async move {
            if let (Some(t), Some(p)) = (widgets::texture_from_url(url).await, weak.upgrade()) {
                p.set_paintable(Some(&t));
            }
        });
        carousel.append(&pic);
    }
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&carousel));
    if app.screenshots.len() > 1 {
        let prev = gtk::Button::from_icon_name("go-previous-symbolic");
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        prev.set_tooltip_text(Some(crate::i18n::tr("details.previous")));
        next.set_tooltip_text(Some(crate::i18n::tr("details.next")));
        for (b, align, d) in [(&prev, gtk::Align::Start, -1i32), (&next, gtk::Align::End, 1)] {
            b.add_css_class("circular");
            b.add_css_class("osd");
            b.set_halign(align);
            b.set_valign(gtk::Align::Center);
            b.set_margin_start(12);
            b.set_margin_end(12);
            let c = carousel.clone();
            b.connect_clicked(move |_| {
                let n = c.n_pages() as i32;
                let p = (c.position().round() as i32 + d).rem_euclid(n.max(1));
                c.scroll_to(&c.nth_page(p as u32), true);
            });
            overlay.add_overlay(b);
        }
    }
    let v = gtk::Box::new(gtk::Orientation::Vertical, 6);
    v.append(&overlay);
    if app.screenshots.len() > 1 {
        v.append(&adw::CarouselIndicatorDots::builder().carousel(&carousel).build());
    }
    v.upcast()
}

fn confirm_remove(store: &Rc<Store>, app: &Rc<App>, window: &gtk::Window) {
    let d = adw::AlertDialog::new(
        Some(&crate::i18n::trf("remove.title", &[("name", &app.name)])),
        Some(crate::i18n::tr("remove.body")),
    );
    d.add_response("cancel", crate::i18n::tr("button.cancel"));
    d.add_response("remove", crate::i18n::tr("button.remove"));
    d.set_default_response(Some("cancel"));
    d.set_close_response("cancel");
    let store = store.clone();
    let id = app.id.clone();
    d.connect_response(None, move |_, r| {
        if r == "remove" {
            store.start(OpKind::Uninstall, &id);
        }
    });
    d.present(Some(window));
}
