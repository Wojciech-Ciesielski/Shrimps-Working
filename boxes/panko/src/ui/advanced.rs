//! Strona Zaawansowane: woluminy z migawkami, czyszczenie (ile migawek trzymać), dostęp.
//! Ustawienia migawek są tylko tu i w menu głównym, nie w Cocktail.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;

use super::App;
use crate::model::{volume_title, LIMITS};

/// Woluminy pokazywane zawsze (nazwa konfiguracji, ścieżka, opis dostępności).
const VOLUMES: [(&str, &str, &str); 2] = [
    ("root", "/", "volume.root"),
    ("home", "/home", "volume.home"),
];

struct Group {
    arrow: gtk::Image,
    header: gtk::Box,
    expanded: Cell<bool>,
}

struct Item {
    widget: gtk::Box,
    name: String,
    group: usize,
}

pub struct Page {
    pub root: gtk::ScrolledWindow,
    list: gtk::Box,
    groups: Vec<Group>,
    items: RefCell<Vec<Item>>,
    volumes: gtk::Box,
    limit_spins: Vec<(&'static str, gtk::SpinButton)>,
    empty_pairs: gtk::Switch,
    space: gtk::SpinButton,
    users: gtk::Entry,
}

fn node(icon: &str, name: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    b.add_css_class("panko-node");
    b.add_css_class("panko-child");
    b.append(&gtk::Image::from_icon_name(icon));
    let l = gtk::Label::new(Some(name));
    l.set_xalign(0.0);
    l.set_hexpand(true);
    b.append(&l);
    b
}

fn spin(max: f64, label: &str, unit: Option<&str>) -> (gtk::SpinButton, gtk::Box) {
    let s = gtk::SpinButton::with_range(0.0, max, 1.0);
    s.set_valign(gtk::Align::Center);
    s.update_property(&[gtk::accessible::Property::Label(label)]);
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    b.append(&s);
    if let Some(u) = unit {
        let l = gtk::Label::new(Some(u));
        l.add_css_class("dim-label");
        b.append(&l);
    }
    (s, b)
}

fn switch(label: &str) -> gtk::Switch {
    let s = gtk::Switch::new();
    s.set_valign(gtk::Align::Center);
    s.update_property(&[gtk::accessible::Property::Label(label)]);
    s
}

impl Page {
    pub fn new(app: &Rc<App>) -> Self {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.set_margin_top(12);
        list.set_margin_bottom(24);
        let mut groups = vec![];
        let mut items = vec![];

        let mut add_group = |list: &gtk::Box, icon: &str, title: &str, expanded: bool| {
            let h = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            h.add_css_class("panko-node");
            h.add_css_class("panko-group");
            let arrow = gtk::Image::from_icon_name(if expanded {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            });
            arrow.set_pixel_size(12);
            arrow.set_size_request(16, 16);
            arrow.add_css_class("dim-label");
            h.append(&arrow);
            h.append(&gtk::Image::from_icon_name(icon));
            let l = gtk::Label::new(Some(title));
            l.set_xalign(0.0);
            h.append(&l);
            list.append(&h);
            groups.push(Group {
                arrow,
                header: h,
                expanded: Cell::new(expanded),
            });
            groups.len() - 1
        };

        // Woluminy: wiersze dochodzą w refresh_advanced (zależą od konfiguracji).
        let g_vol = add_group(&list, "drive-harddisk-symbolic", crate::i18n::tr("adv.volumes"), true);
        let volumes = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.append(&volumes);

        let g_clean = add_group(&list, "user-trash-symbolic", crate::i18n::tr("adv.cleanup"), true);
        let mut limit_spins = vec![];
        for (key, name) in LIMITS {
            let icon = if key == "NUMBER_LIMIT" {
                "software-update-available-symbolic"
            } else {
                "document-open-recent-symbolic"
            };
            let name = crate::i18n::tr(name);
            let row = node(icon, name);
            let (s, b) = spin(999.0, name, None);
            row.append(&b);
            list.append(&row);
            items.push(Item {
                widget: row,
                name: name.to_lowercase(),
                group: g_clean,
            });
            let w = Rc::downgrade(app);
            s.connect_value_changed(move |s| {
                let Some(app) = w.upgrade() else { return };
                if !app.updating.get() {
                    app.queue_setting(key, &s.value_as_int().to_string());
                }
            });
            limit_spins.push((key, s));
        }
        let row = node("user-trash-symbolic", crate::i18n::tr("adv.empty-pairs"));
        let empty_pairs = switch(crate::i18n::tr("adv.empty-pairs"));
        row.append(&empty_pairs);
        list.append(&row);
        items.push(Item {
            widget: row,
            name: crate::i18n::tr("adv.empty-pairs").to_lowercase(),
            group: g_clean,
        });
        let w = Rc::downgrade(app);
        empty_pairs.connect_active_notify(move |s| {
            let Some(app) = w.upgrade() else { return };
            if !app.updating.get() {
                app.queue_setting(
                    "EMPTY_PRE_POST_CLEANUP",
                    if s.is_active() { "yes" } else { "no" },
                );
            }
        });
        let row = node("drive-harddisk-symbolic", crate::i18n::tr("adv.space"));
        let (space, b) = spin(100.0, crate::i18n::tr("adv.space"), Some("%"));
        row.append(&b);
        list.append(&row);
        items.push(Item {
            widget: row,
            name: crate::i18n::tr("adv.space").to_lowercase(),
            group: g_clean,
        });
        let w = Rc::downgrade(app);
        space.connect_value_changed(move |s| {
            let Some(app) = w.upgrade() else { return };
            if !app.updating.get() {
                app.queue_setting("SPACE_LIMIT", &format!("{:.2}", s.value() / 100.0));
            }
        });

        let g_access = add_group(&list, "security-high-symbolic", crate::i18n::tr("adv.access"), false);
        let row = node("security-high-symbolic", crate::i18n::tr("adv.users"));
        let users = gtk::Entry::new();
        users.set_placeholder_text(Some(crate::i18n::tr("adv.username")));
        users.set_width_chars(18);
        users.set_valign(gtk::Align::Center);
        users.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("adv.users"))]);
        row.append(&users);
        list.append(&row);
        items.push(Item {
            widget: row,
            name: crate::i18n::tr("adv.users").to_lowercase(),
            group: g_access,
        });
        let w = Rc::downgrade(app);
        users.connect_activate(move |e| {
            if let Some(app) = w.upgrade() {
                let v = e
                    .text()
                    .split([' ', ','])
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                app.queue_setting("ALLOW_USERS", &v);
            }
        });

        let root = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();

        for (i, g) in groups.iter().enumerate() {
            let click = gtk::GestureClick::new();
            let w = Rc::downgrade(app);
            click.connect_released(move |_, _, _, _| {
                if let Some(app) = w.upgrade() {
                    app.toggle_adv_group(i);
                }
            });
            g.header.add_controller(click);
        }
        let _ = g_vol;

        Page {
            root,
            list,
            groups,
            items: RefCell::new(items),
            volumes,
            limit_spins,
            empty_pairs,
            space,
            users,
        }
    }
}

impl App {
    fn with_adv_page<R>(&self, f: impl FnOnce(&Page) -> R) -> R {
        f(self
            .adv_page
            .borrow()
            .as_ref()
            .expect("strona Zaawansowane"))
    }

    pub fn refresh_advanced(self: &Rc<Self>) {
        let configs = self.configs.borrow().clone();
        let root = self.root_config();
        self.updating.set(true);
        self.with_adv_page(|p| {
            // Woluminy od nowa.
            while let Some(c) = p.volumes.first_child() {
                p.volumes.remove(&c);
            }
            p.items.borrow_mut().retain(|i| i.group != 0);
            let mut vols: Vec<(String, String, String)> = VOLUMES
                .iter()
                .map(|(n, s, l)| (n.to_string(), s.to_string(), crate::i18n::tr(l).to_string()))
                .collect();
            for c in &configs {
                if !vols.iter().any(|v| v.1 == c.subvolume) {
                    vols.push((
                        c.name.clone(),
                        c.subvolume.clone(),
                        crate::i18n::trf("volume.other", &[("path", &c.subvolume)]),
                    ));
                }
            }
            let mut new_items = vec![];
            for (name, subvol, label) in vols {
                let cfg = configs.iter().find(|c| c.subvolume == subvol);
                let title = cfg.map(volume_title).unwrap_or_else(|| {
                    if subvol == "/" {
                        crate::i18n::tr("volume.system").into()
                    } else {
                        subvol.clone()
                    }
                });
                let row = node("drive-harddisk-symbolic", &title);
                let s = switch(&label);
                s.set_active(cfg.is_some());
                // System zawsze ma migawki: bez nich nie da się cofnąć aktualizacji.
                s.set_sensitive(subvol != "/" || cfg.is_none());
                row.append(&s);
                p.volumes.append(&row);
                new_items.push(Item {
                    widget: row,
                    name: title.to_lowercase(),
                    group: 0,
                });
                let w = Rc::downgrade(self);
                let cfg_name = cfg.map(|c| c.name.clone()).unwrap_or(name);
                s.connect_state_set(move |sw, on| {
                    let Some(app) = w.upgrade() else {
                        return glib::Propagation::Stop;
                    };
                    if app.updating.get() {
                        return glib::Propagation::Proceed;
                    }
                    if on {
                        app.enable_volume(&cfg_name, &subvol);
                    } else {
                        let w2 = Rc::downgrade(&app);
                        let n = cfg_name.clone();
                        app.confirm(
                            crate::i18n::tr("disable.title"),
                            crate::i18n::tr("disable.body"),
                            crate::i18n::tr("button.turn-off"),
                            true,
                            None,
                            move || {
                                if let Some(app) = w2.upgrade() {
                                    app.disable_volume(&n);
                                }
                            },
                        );
                        // Przełącznik wraca, dopóki użytkownik nie potwierdzi.
                        app.updating.set(true);
                        sw.set_active(true);
                        app.updating.set(false);
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                });
            }
            {
                let mut items = p.items.borrow_mut();
                let mut all = new_items;
                all.append(&mut items);
                *items = all;
            }

            if let Some(c) = &root {
                for (key, s) in &p.limit_spins {
                    s.set_value(c.number(key) as f64);
                }
                p.empty_pairs.set_active(c.flag("EMPTY_PRE_POST_CLEANUP"));
                p.space.set_value(c.percent("SPACE_LIMIT") as f64);
                p.users.set_text(&c.users("ALLOW_USERS").join(" "));
            }
        });
        self.updating.set(false);
        self.layout_advanced("");
    }

    fn disable_volume(self: &Rc<Self>, name: &str) {
        let backend = self.backend.clone();
        let w = Rc::downgrade(self);
        let name = name.to_owned();
        super::run(
            move || backend.disable_volume(&name),
            move |r| {
                let Some(app) = w.upgrade() else { return };
                if let Err(e) = r {
                    app.fail(crate::i18n::tr("error.disable"), &e);
                }
                app.reload();
            },
        );
    }

    fn toggle_adv_group(self: &Rc<Self>, i: usize) {
        self.with_adv_page(|p| {
            let g = &p.groups[i];
            g.expanded.set(!g.expanded.get());
            g.arrow.set_icon_name(Some(if g.expanded.get() {
                "pan-down-symbolic"
            } else {
                "pan-end-symbolic"
            }));
        });
        let q = if self.current_page() == "zaawansowane" {
            self.query()
        } else {
            String::new()
        };
        self.layout_advanced(&q);
    }

    pub fn filter_advanced(self: &Rc<Self>) -> usize {
        let q = self.query();
        self.layout_advanced(&q)
    }

    /// Widoczność wierszy (zwinięcie + wyszukiwanie) i paski. Zwraca liczbę trafień.
    fn layout_advanced(&self, q: &str) -> usize {
        self.with_adv_page(|p| {
            let items = p.items.borrow();
            let mut hits = 0;
            let mut group_hit = vec![false; p.groups.len()];
            for it in items.iter() {
                let hit = q.is_empty() || it.name.contains(q);
                if hit {
                    hits += 1;
                    group_hit[it.group] = true;
                }
                let open = !q.is_empty() || p.groups[it.group].expanded.get();
                it.widget.set_visible(hit && open);
            }
            for (i, g) in p.groups.iter().enumerate() {
                g.header.set_visible(q.is_empty() || group_hit[i]);
            }
            // Paski po kolei dla widocznych wierszy, w kolejności na ekranie.
            let mut n = 0;
            let mut stripe = |w: &gtk::Widget| {
                if !w.is_visible() {
                    return;
                }
                w.remove_css_class("st-a");
                w.remove_css_class("st-b");
                w.add_css_class(if n % 2 == 0 { "st-a" } else { "st-b" });
                n += 1;
            };
            let mut c = p.list.first_child();
            while let Some(w) = c {
                if w == *p.volumes.upcast_ref::<gtk::Widget>() {
                    let mut v = p.volumes.first_child();
                    while let Some(x) = v {
                        stripe(&x);
                        v = x.next_sibling();
                    }
                } else {
                    stripe(&w);
                }
                c = w.next_sibling();
            }
            hits
        })
    }
}

use gtk::glib;
