//! Strona Zmiany: pliki zmienione od wybranej migawki, wybór plików do przywrócenia.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use adw::prelude::*;

use super::snapshots::snap_label;
use super::tree::{Callbacks, Row, Tree};
use super::{run, App};
use crate::model::{group_changes, FileChange};

pub struct Page {
    pub root: gtk::Box,
    tree: Tree,
    choice: gtk::DropDown,
    restore: gtk::Button,
    empty: gtk::Label,
    spinner: gtk::Spinner,
    state: RefCell<State>,
}

#[derive(Default)]
struct State {
    /// Migawki w kolejności listy wyboru (najnowsza pierwsza).
    nums: Vec<u32>,
    current: Option<u32>,
    changes: Option<Vec<FileChange>>,
    loading: Option<u32>,
    checked: HashMap<String, Rc<Cell<bool>>>,
    collapsed: HashSet<String>,
}

impl Page {
    pub fn new(app: &Rc<App>) -> Self {
        let w1 = Rc::downgrade(app);
        let w2 = Rc::downgrade(app);
        let tree = Tree::new(Callbacks {
            toggle_group: Box::new(move |key| {
                let Some(app) = w1.upgrade() else { return };
                app.with_chg_page(|p| {
                    let mut st = p.state.borrow_mut();
                    if !st.collapsed.remove(key) {
                        st.collapsed.insert(key.to_owned());
                    }
                });
                app.refresh_changes();
            }),
            click_snap: Box::new(|_| {}),
            check_changed: Box::new(move || {
                if let Some(app) = w2.upgrade() {
                    app.update_restore_button();
                }
            }),
        });

        let label = gtk::Label::new(Some(crate::i18n::tr("changes.compare")));
        label.add_css_class("panko-sel-name");
        let choice = gtk::DropDown::from_strings(&[]);
        choice.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("changes.compare-a11y"))]);
        let left = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        left.append(&label);
        left.append(&choice);
        let restore = gtk::Button::with_label(crate::i18n::tr("changes.restore"));
        restore.add_css_class("suggested-action");
        restore.add_css_class("panko-main");
        restore.set_sensitive(false);
        restore.set_halign(gtk::Align::End);
        restore.set_hexpand(true);
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        bar.add_css_class("panko-toolbar");
        bar.append(&left);
        bar.append(&restore);

        let empty = gtk::Label::new(Some(crate::i18n::tr("changes.none")));
        empty.add_css_class("panko-empty");
        empty.set_xalign(0.0);
        empty.set_visible(false);
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(24, 24);
        spinner.set_margin_top(24);
        spinner.set_visible(false);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&bar);
        root.append(&empty);
        root.append(&spinner);
        root.append(&tree.scroller);

        let w = Rc::downgrade(app);
        choice.connect_selected_notify(move |d| {
            let Some(app) = w.upgrade() else { return };
            if app.updating.get() {
                return;
            }
            let num =
                app.with_chg_page(|p| p.state.borrow().nums.get(d.selected() as usize).copied());
            if let Some(num) = num {
                app.load_changes(num);
            }
        });
        let w = Rc::downgrade(app);
        restore.connect_clicked(move |_| {
            if let Some(app) = w.upgrade() {
                app.restore_selected_files();
            }
        });

        Page {
            root,
            tree,
            choice,
            restore,
            empty,
            spinner,
            state: RefCell::new(State::default()),
        }
    }
}

impl App {
    fn with_chg_page<R>(&self, f: impl FnOnce(&Page) -> R) -> R {
        f(self.chg_page.borrow().as_ref().expect("strona Zmiany"))
    }

    /// Lista „Porównaj z” po każdym wczytaniu migawek.
    pub fn refresh_changes_choices(self: &Rc<Self>) {
        let mut snaps = self.snaps.borrow().clone();
        snaps.sort_by_key(|s| std::cmp::Reverse((s.date, s.num)));
        let labels: Vec<String> = snaps.iter().map(snap_label).collect();
        let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
        self.updating.set(true);
        self.with_chg_page(|p| {
            let mut st = p.state.borrow_mut();
            st.nums = snaps.iter().map(|s| s.num).collect();
            p.choice.set_model(Some(&gtk::StringList::new(&labels)));
            if st.current.is_some_and(|n| !st.nums.contains(&n)) {
                st.current = None;
                st.changes = None;
            }
            if st.current.is_none() {
                st.current = st.nums.first().copied();
            }
            if let Some(i) = st
                .current
                .and_then(|n| st.nums.iter().position(|&x| x == n))
            {
                p.choice.set_selected(i as u32);
            }
            // Po zmianach w systemie (np. przywróceniu plików) porównanie liczy się od nowa.
            st.changes = None;
        });
        self.updating.set(false);
        if self.current_page() == "zmiany" {
            self.changes_shown();
        }
    }

    /// Wejście na stronę Zmiany: porównanie liczy się dopiero wtedy (bywa kosztowne).
    pub fn changes_shown(self: &Rc<Self>) {
        let (cur, have) = self.with_chg_page(|p| {
            let st = p.state.borrow();
            (st.current, st.changes.is_some() || st.loading.is_some())
        });
        match cur {
            Some(n) if !have => self.load_changes(n),
            _ => self.refresh_changes(),
        }
    }

    /// „Pokaż zmiany” na stronie Migawki.
    pub fn show_changes_for(self: &Rc<Self>, num: u32) {
        let i = self.with_chg_page(|p| p.state.borrow().nums.iter().position(|&x| x == num));
        self.updating.set(true);
        self.with_chg_page(|p| {
            if let Some(i) = i {
                p.choice.set_selected(i as u32);
            }
        });
        self.updating.set(false);
        self.load_changes(num);
        self.show_page("zmiany");
    }

    fn load_changes(self: &Rc<Self>, num: u32) {
        let Some(cfg) = self.root_config() else {
            return;
        };
        self.with_chg_page(|p| {
            let mut st = p.state.borrow_mut();
            st.current = Some(num);
            st.changes = None;
            st.loading = Some(num);
            st.checked.clear();
            st.collapsed.clear();
        });
        self.refresh_changes();
        let backend = self.backend.clone();
        let w = Rc::downgrade(self);
        run(
            move || backend.changes(&cfg.name, num),
            move |r| {
                let Some(app) = w.upgrade() else { return };
                let stale = app.with_chg_page(|p| p.state.borrow().loading != Some(num));
                if stale {
                    return; // w międzyczasie wybrano inną migawkę
                }
                let changes = match r {
                    Ok(c) => c,
                    Err(e) => {
                        app.fail(crate::i18n::tr("error.compare"), &e);
                        vec![]
                    }
                };
                app.with_chg_page(|p| {
                    let mut st = p.state.borrow_mut();
                    st.loading = None;
                    // Przy dużych porównaniach grupy startują zwinięte.
                    if changes.len() > 200 {
                        st.collapsed = group_changes(&changes)
                            .into_iter()
                            .map(|(g, _)| g)
                            .collect();
                    }
                    st.changes = Some(changes);
                });
                app.apply_search();
            },
        );
    }

    pub fn refresh_changes(self: &Rc<Self>) {
        let q = if self.current_page() == "zmiany" {
            self.query()
        } else {
            String::new()
        };
        self.with_chg_page(|p| {
            let mut st = p.state.borrow_mut();
            p.spinner.set_visible(st.loading.is_some());
            p.spinner.set_spinning(st.loading.is_some());
            let Some(changes) = st.changes.clone() else {
                p.tree.set_rows(vec![]);
                p.empty.set_visible(false);
                p.restore.set_sensitive(false);
                return;
            };
            let mut rows = vec![];
            for (group, files) in group_changes(&changes) {
                let files: Vec<_> = files
                    .into_iter()
                    .filter(|(name, c)| {
                        q.is_empty()
                            || name.to_lowercase().contains(&q)
                            || c.path.to_lowercase().contains(&q)
                    })
                    .collect();
                if files.is_empty() {
                    continue;
                }
                let expanded = !q.is_empty() || !st.collapsed.contains(&group);
                rows.push(Row::Group {
                    key: group.clone(),
                    title: group.clone(),
                    icon: Some("folder-symbolic"),
                    expanded,
                });
                if expanded {
                    for (name, c) in files {
                        let checked = st.checked.entry(c.path.clone()).or_default().clone();
                        rows.push(Row::File {
                            name,
                            marker: c.marker(),
                            checked,
                            restorable: c.restorable(),
                        });
                    }
                }
            }
            p.tree.set_rows(rows);
            p.empty.set_visible(changes.is_empty());
        });
        self.update_restore_button();
    }

    pub fn filter_changes(self: &Rc<Self>) -> usize {
        self.refresh_changes();
        let q = self.query();
        self.with_chg_page(|p| {
            let st = p.state.borrow();
            st.changes.as_ref().map_or(0, |c| {
                c.iter()
                    .filter(|c| c.path.to_lowercase().contains(&q))
                    .count()
            })
        })
    }

    fn checked_paths(&self) -> Vec<String> {
        self.with_chg_page(|p| {
            let st = p.state.borrow();
            let mut v: Vec<String> = st
                .checked
                .iter()
                .filter(|(p, c)| c.get() && crate::model::restorable_path(p))
                .map(|(p, _)| p.clone())
                .collect();
            v.sort();
            v
        })
    }

    fn update_restore_button(&self) {
        let any = !self.checked_paths().is_empty();
        self.with_chg_page(|p| p.restore.set_sensitive(any));
    }

    fn restore_selected_files(self: &Rc<Self>) {
        let paths = self.checked_paths();
        let num = self.with_chg_page(|p| p.state.borrow().current);
        let (Some(num), Some(cfg)) = (num, self.root_config()) else {
            return;
        };
        if paths.is_empty() {
            return;
        }
        let w = Rc::downgrade(self);
        self.confirm(
            crate::i18n::tr("restore-files.title"),
            crate::i18n::tr("restore-files.body"),
            crate::i18n::tr("button.restore"),
            true,
            None,
            move || {
                let Some(app) = w.upgrade() else { return };
                let backend = app.backend.clone();
                let (cfg, paths) = (cfg.clone(), paths.clone());
                let w2 = Rc::downgrade(&app);
                run(
                    move || backend.restore_files(&cfg, num, &paths).map(|_| paths),
                    move |r| {
                        let Some(app) = w2.upgrade() else { return };
                        match r {
                            Ok(paths) => {
                                // Przywrócone pliki znikają z listy (jak w makiecie), a porównanie liczy się od nowa.
                                if app.backend.is_demo() {
                                    app.with_chg_page(|p| {
                                        let mut st = p.state.borrow_mut();
                                        if let Some(c) = st.changes.as_mut() {
                                            c.retain(|c| !paths.contains(&c.path));
                                        }
                                        st.checked.clear();
                                    });
                                    app.refresh_changes();
                                } else {
                                    app.load_changes(num);
                                }
                            }
                            Err(e) => app.fail(crate::i18n::tr("error.restore-files"), &e),
                        }
                    },
                );
            },
        );
    }
}
