//! Strona Migawki: grupy dni, zaznaczenie migawki, tworzenie, usuwanie, przywracanie systemu.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use adw::prelude::*;
use gtk::glib;

use super::tree::{Callbacks, Row, Tree};
use super::{run, App};
use crate::model::{DayGroup, LocalTime, Snapshot};

pub struct Page {
    pub root: gtk::Box,
    tree: Tree,
    sel_box: gtk::Box,
    sel_name: gtk::Label,
    main: gtk::Button,
    empty: gtk::Label,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    selected: Option<u32>,
    /// Ręcznie rozwinięte/zwinięte grupy (klucz = tytuł grupy).
    expanded: HashMap<String, bool>,
}

pub fn local(ts: i64) -> LocalTime {
    let d = glib::DateTime::from_unix_local(ts).expect("data");
    LocalTime {
        ts,
        year: d.year(),
        month: d.month() as u32,
        day: d.day_of_month() as u32,
        weekday: d.day_of_week() as u32,
        hour: d.hour() as u32,
        minute: d.minute() as u32,
    }
}

pub fn start_of_today() -> i64 {
    let n = glib::DateTime::now_local().expect("czas");
    glib::DateTime::from_local(n.year(), n.month(), n.day_of_month(), 0, 0, 0.0)
        .map(|d| d.to_unix())
        .unwrap_or(0)
}

/// „Po aktualizacji, dziś 09:21” – nazwa migawki w oknach i na liście porównania.
pub fn snap_label(s: &Snapshot) -> String {
    let t = local(s.date);
    let g = DayGroup::of(&t, start_of_today());
    format!("{}, {}", s.category().title(), g.when(&t))
}

impl Page {
    pub fn new(app: &Rc<App>) -> Self {
        let w1 = Rc::downgrade(app);
        let w2 = Rc::downgrade(app);
        let tree = Tree::new(Callbacks {
            toggle_group: Box::new(move |key| {
                if let Some(app) = w1.upgrade() {
                    app.toggle_snap_group(key);
                }
            }),
            click_snap: Box::new(move |num| {
                if let Some(app) = w2.upgrade() {
                    app.click_snap(num);
                }
            }),
            check_changed: Box::new(|| {}),
        });

        let sel_name = gtk::Label::new(None);
        sel_name.add_css_class("panko-sel-name");
        sel_name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        sel_name.set_max_width_chars(28);
        let show = gtk::Button::with_label(crate::i18n::tr("snap.show-changes"));
        let del = gtk::Button::with_label(crate::i18n::tr("button.delete"));
        let sel_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        sel_box.append(&sel_name);
        sel_box.append(&show);
        sel_box.append(&del);
        sel_box.set_visible(false);
        let main = gtk::Button::with_label(crate::i18n::tr("snap.create"));
        main.add_css_class("suggested-action");
        main.add_css_class("panko-main");
        main.set_halign(gtk::Align::End);
        main.set_hexpand(true);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        bar.add_css_class("panko-toolbar");
        bar.append(&sel_box);
        bar.append(&main);

        let empty = gtk::Label::new(Some(crate::i18n::tr("snap.none")));
        empty.add_css_class("panko-empty");
        empty.set_xalign(0.0);
        empty.set_valign(gtk::Align::Start);
        empty.set_visible(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&bar);
        root.append(&empty);
        root.append(&tree.scroller);

        let w = Rc::downgrade(app);
        main.connect_clicked(move |_| {
            if let Some(app) = w.upgrade() {
                app.main_snap_action();
            }
        });
        let w = Rc::downgrade(app);
        show.connect_clicked(move |_| {
            let Some(app) = w.upgrade() else { return };
            let sel = app
                .snap_page
                .borrow()
                .as_ref()
                .and_then(|p| p.state.borrow().selected);
            if let Some(num) = sel {
                app.show_changes_for(num);
            }
        });
        let w = Rc::downgrade(app);
        del.connect_clicked(move |_| {
            if let Some(app) = w.upgrade() {
                app.delete_selected();
            }
        });

        Page {
            root,
            tree,
            sel_box,
            sel_name,
            main,
            empty,
            state: RefCell::new(State::default()),
        }
    }
}

impl App {
    fn with_snap_page<R>(&self, f: impl FnOnce(&Page) -> R) -> R {
        f(self.snap_page.borrow().as_ref().expect("strona Migawki"))
    }

    pub fn refresh_snapshots(self: &Rc<Self>) {
        let snaps = self.snaps.borrow().clone();
        let q = self.query();
        let filtering = !q.is_empty() && self.current_page() == "migawki";
        self.with_snap_page(|p| {
            let mut st = p.state.borrow_mut();
            if st
                .selected
                .is_some_and(|n| !snaps.iter().any(|s| s.num == n))
            {
                st.selected = None;
            }
            let today = start_of_today();
            let mut groups: Vec<(DayGroup, Vec<(LocalTime, &Snapshot)>)> = vec![];
            let mut sorted: Vec<&Snapshot> = snaps.iter().collect();
            sorted.sort_by_key(|s| std::cmp::Reverse((s.date, s.num)));
            for s in sorted {
                if filtering && !s.search_text().contains(&q) {
                    continue;
                }
                let t = local(s.date);
                let g = DayGroup::of(&t, today);
                match groups.iter_mut().find(|(k, _)| *k == g) {
                    Some((_, v)) => v.push((t, s)),
                    None => groups.push((g, vec![(t, s)])),
                }
            }
            groups.sort_by(|a, b| a.0.cmp(&b.0));
            let mut rows = vec![];
            for (i, (g, items)) in groups.iter().enumerate() {
                let title = g.title();
                // Domyślnie otwarte: Dziś, Wczoraj i pierwsza grupa (gdy nic nie ma z ostatnich dni).
                let default = matches!(g, DayGroup::Today | DayGroup::Yesterday) || i == 0;
                let expanded = filtering || *st.expanded.get(&title).unwrap_or(&default);
                rows.push(Row::Group {
                    key: title.clone(),
                    title,
                    icon: None,
                    expanded,
                });
                if expanded {
                    for (t, s) in items {
                        let c = s.category();
                        rows.push(Row::Snap {
                            num: s.num,
                            title: c.title(),
                            icon: c.icon(),
                            time: g.time_label(t),
                            selected: st.selected == Some(s.num),
                        });
                    }
                }
            }
            p.tree.set_rows(rows);
            p.empty.set_visible(snaps.is_empty());
            let sel = st.selected.and_then(|n| snaps.iter().find(|s| s.num == n));
            p.sel_box.set_visible(sel.is_some());
            if let Some(s) = sel {
                p.sel_name.set_label(&snap_label(s));
            }
            p.main.set_label(if sel.is_some() {
                crate::i18n::tr("snap.restore-system")
            } else {
                crate::i18n::tr("snap.create")
            });
        });
    }

    pub fn filter_snapshots(self: &Rc<Self>) -> usize {
        self.refresh_snapshots();
        let q = self.query();
        self.snaps
            .borrow()
            .iter()
            .filter(|s| s.search_text().contains(&q))
            .count()
    }

    fn toggle_snap_group(self: &Rc<Self>, key: &str) {
        let today = start_of_today();
        // Stan domyślny grupy liczony jak w refresh_snapshots.
        let first = self
            .snaps
            .borrow()
            .iter()
            .max_by_key(|s| s.date)
            .map(|s| DayGroup::of(&local(s.date), today).title());
        self.with_snap_page(|p| {
            let mut st = p.state.borrow_mut();
            let default = key == crate::i18n::tr("day.today") || key == crate::i18n::tr("day.yesterday") || first.as_deref() == Some(key);
            let cur = *st.expanded.get(key).unwrap_or(&default);
            st.expanded.insert(key.to_owned(), !cur);
        });
        self.refresh_snapshots();
    }

    fn click_snap(self: &Rc<Self>, num: u32) {
        self.with_snap_page(|p| {
            let mut st = p.state.borrow_mut();
            st.selected = if st.selected == Some(num) {
                None
            } else {
                Some(num)
            };
        });
        self.refresh_snapshots();
    }

    pub fn select_snap(self: &Rc<Self>, num: u32) {
        self.with_snap_page(|p| p.state.borrow_mut().selected = Some(num));
        self.refresh_snapshots();
    }

    fn selected_snap(&self) -> Option<Snapshot> {
        let n = self.with_snap_page(|p| p.state.borrow().selected)?;
        self.snaps.borrow().iter().find(|s| s.num == n).cloned()
    }

    fn main_snap_action(self: &Rc<Self>) {
        match self.selected_snap() {
            None => {
                let entry = gtk::Entry::new();
                entry.set_placeholder_text(Some(crate::i18n::tr("snap.description")));
                entry.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("snap.description-a11y"))]);
                let e2 = entry.clone();
                let w = Rc::downgrade(self);
                self.confirm(
                    crate::i18n::tr("snap.create"),
                    "",
                    crate::i18n::tr("button.create"),
                    false,
                    Some(&entry),
                    move || {
                        let desc = e2.text().trim().to_owned();
                        let desc = if desc.is_empty() {
                            crate::i18n::tr("category.manual").to_owned()
                        } else {
                            desc
                        };
                        if let Some(app) = w.upgrade() {
                            app.create_snapshot(desc);
                        }
                    },
                );
            }
            Some(s) => {
                let text = crate::i18n::trf("restore-system.body", &[("name", &snap_label(&s))]);
                let w = Rc::downgrade(self);
                self.confirm(
                    crate::i18n::tr("restore-system.title"),
                    &text,
                    crate::i18n::tr("button.restore"),
                    true,
                    None,
                    move || {
                        if let Some(app) = w.upgrade() {
                            app.rollback(s.num);
                        }
                    },
                );
            }
        }
    }

    fn create_snapshot(self: &Rc<Self>, desc: String) {
        let Some(cfg) = self.root_config() else {
            return;
        };
        let backend = self.backend.clone();
        let w = Rc::downgrade(self);
        run(
            move || backend.create(&cfg.name, &desc),
            move |r| {
                let Some(app) = w.upgrade() else { return };
                if let Err(e) = r {
                    app.fail(crate::i18n::tr("error.create"), &e);
                }
                app.with_snap_page(|p| {
                    p.state.borrow_mut().expanded.insert(crate::i18n::tr("day.today").into(), true);
                });
                app.reload();
            },
        );
    }

    fn delete_selected(self: &Rc<Self>) {
        let Some(s) = self.selected_snap() else {
            return;
        };
        let Some(cfg) = self.root_config() else {
            return;
        };
        let w = Rc::downgrade(self);
        self.confirm(
            crate::i18n::tr("delete.title"),
            crate::i18n::tr("delete.body"),
            crate::i18n::tr("button.delete"),
            true,
            None,
            move || {
                let Some(app) = w.upgrade() else { return };
                let backend = app.backend.clone();
                let cfg = cfg.name.clone();
                // Para przed/po aktualizacji znika razem, jak w innych narzędziach openSUSE.
                let mut nums = vec![s.num];
                for o in app.snaps.borrow().iter() {
                    if o.pre_num == s.num && o.num != s.num {
                        nums.push(o.num);
                    }
                }
                let w2 = Rc::downgrade(&app);
                run(
                    move || backend.delete(&cfg, &nums),
                    move |r| {
                        let Some(app) = w2.upgrade() else { return };
                        if let Err(e) = r {
                            app.fail(crate::i18n::tr("error.delete"), &e);
                        }
                        app.with_snap_page(|p| p.state.borrow_mut().selected = None);
                        app.reload();
                    },
                );
            },
        );
    }

    fn rollback(self: &Rc<Self>, num: u32) {
        let backend = self.backend.clone();
        let w = Rc::downgrade(self);
        run(
            move || {
                backend.rollback(num)?;
                // Po udanym przywróceniu restart; jeśli się nie da, użytkownik zrobi go sam.
                Ok::<bool, crate::backend::Error>(backend.reboot().is_ok())
            },
            move |r| {
                let Some(app) = w.upgrade() else { return };
                match r {
                    Ok(true) if !app.backend.is_demo() => {}
                    Ok(_) => app.toast(crate::i18n::tr("toast.restart")),
                    Err(e) => app.fail(crate::i18n::tr("error.restore-system"), &e),
                }
                app.with_snap_page(|p| p.state.borrow_mut().selected = None);
                app.reload();
            },
        );
    }
}
