//! Pakiety: wybrane pakiety systemu w kategoriach (drzewo z makiety) i wyszukiwanie w repozytoriach.

use gtk::glib;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use crate::app::{self, App, Op};
use crate::catalog;
use crate::tree::Tree;
use crate::zypper;

pub struct Packages {
    pub widget: gtk::ScrolledWindow,
    app: Rc<App>,
    tree: Rc<Tree>,
    results: Rc<Tree>,
    results_box: gtk::Box,
    none: gtk::Label,
    installed: RefCell<HashSet<String>>,
    query: RefCell<String>,
    found: RefCell<Vec<zypper::Found>>,
    /// Numer wyszukiwania, żeby spóźnione wyniki nie nadpisały nowszych.
    generation: Cell<u32>,
    loaded: Cell<bool>,
}

impl Packages {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 18);
        let results = Tree::new();
        let results_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let rl = gtk::Label::new(Some(crate::i18n::tr("search.results")));
        rl.add_css_class("heading");
        rl.set_xalign(0.0);
        rl.set_margin_start(10);
        results_box.append(&rl);
        results_box.append(&results.list);
        results_box.set_visible(false);
        let tree = Tree::new();
        let none = super::no_results();
        content.append(&results_box);
        content.append(&tree.list);
        content.append(&none);
        let page = Rc::new(Packages {
            widget: super::scrolled(&content),
            app: app.clone(),
            tree,
            results,
            results_box,
            none,
            installed: RefCell::new(HashSet::new()),
            query: RefCell::new(String::new()),
            found: RefCell::new(Vec::new()),
            generation: Cell::new(0),
            loaded: Cell::new(false),
        });
        let weak = Rc::downgrade(&page);
        app.on_change.borrow_mut().push(Box::new(move || {
            if let Some(p) = weak.upgrade() {
                glib::spawn_future_local(async move { p.reload().await });
            }
        }));
        page
    }

    /// Pierwsze wejście w zakładkę.
    pub fn load(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let p = self.clone();
        glib::spawn_future_local(async move { p.reload().await });
    }

    async fn reload(self: Rc<Self>) {
        let mut names: Vec<String> = catalog::all_packages().into_iter().map(str::to_string).collect();
        names.extend(self.found.borrow().iter().map(|f| f.name.clone()));
        let mut argv = vec!["rpm", "-q", "--qf", "%{NAME}\\n"];
        argv.extend(names.iter().map(String::as_str));
        let out = app::run(&argv).await;
        *self.installed.borrow_mut() = zypper::parse_rpm_installed(&out.stdout).into_iter().collect();
        self.render_catalog();
        self.render_results();
    }

    fn render_catalog(self: &Rc<Self>) {
        self.tree.clear();
        let root = self.tree.group(None, "package-x-generic-symbolic", crate::i18n::tr("packages.root"), true);
        for s in catalog::SECTIONS {
            let g = self.tree.group(Some(root), s.icon, crate::i18n::tr(s.title), s.title == "cat.multimedia");
            for item in s.items {
                let pkgs: Vec<String> = item.packages.iter().map(|p| p.to_string()).collect();
                let title = crate::i18n::tr(item.title);
                let w = self.action(pkgs, title);
                self.tree.leaf(Some(g), Some("package-x-generic-symbolic"), title, None, Some(&w));
            }
        }
        self.apply_filter();
    }

    fn render_results(self: &Rc<Self>) {
        self.results.clear();
        let found = self.found.borrow().clone();
        for f in found.iter().take(100) {
            let w = self.action(vec![f.name.clone()], &f.name);
            let summary = (!f.summary.is_empty()).then_some(f.summary.as_str());
            self.results.leaf(None, Some("package-x-generic-symbolic"), &f.name, summary, Some(&w));
        }
        self.results.refresh();
        self.results_box.set_visible(!found.is_empty());
        self.apply_filter();
    }

    /// Znacznik stanu i przycisk tekstowy bez tła (Zainstaluj / Usuń).
    fn action(self: &Rc<Self>, pkgs: Vec<String>, label: &str) -> gtk::Widget {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let installed = {
            let set = self.installed.borrow();
            pkgs.iter().all(|p| set.contains(p))
        };
        let pending = pkgs.iter().find_map(|p| self.app.pending.borrow().get(p).copied());
        if let Some(op) = pending {
            let l = gtk::Label::new(Some(match op {
                Op::Remove => crate::i18n::tr("pending.removed"),
                _ => crate::i18n::tr("pending.ready"),
            }));
            l.add_css_class("dim-label");
            b.append(&l);
            return b.upcast();
        }
        let mark = gtk::Image::from_icon_name(if installed { "object-select-symbolic" } else { "folder-download-symbolic" });
        mark.set_tooltip_text(Some(crate::i18n::tr(if installed { "state.installed" } else { "state.available" })));
        mark.add_css_class("dim-label");
        let protected = pkgs.iter().any(|p| zypper::is_protected(p));
        let btn = gtk::Button::with_label(crate::i18n::tr(if installed { "button.remove" } else { "button.install" }));
        btn.add_css_class("flat");
        btn.set_visible(!(installed && protected));
        let (p, label) = (self.clone(), label.to_string());
        btn.connect_clicked(move |btn| {
            let (p, pkgs, label, btn) = (p.clone(), pkgs.clone(), label.clone(), btn.clone());
            glib::spawn_future_local(async move {
                let op = if installed { Op::Remove } else { Op::Install };
                btn.set_sensitive(false);
                if !installed {
                    btn.set_label(crate::i18n::tr("state.installing"));
                }
                if !p.app.change(op, pkgs, &label).await {
                    btn.set_sensitive(true);
                    btn.set_label(crate::i18n::tr(if installed { "button.remove" } else { "button.install" }));
                }
            });
        });
        b.append(&mark);
        b.append(&btn);
        b.upcast()
    }

    fn apply_filter(&self) {
        let q = self.query.borrow();
        let shown = self.tree.set_filter(&q) + self.found.borrow().len();
        self.none.set_visible(!q.trim().is_empty() && shown == 0);
    }

    /// Pole wyszukiwania z paska bocznego: filtr kategorii od razu, repozytoria po chwili.
    pub fn search(self: &Rc<Self>, q: &str) {
        *self.query.borrow_mut() = q.to_string();
        let gen = self.generation.get() + 1;
        self.generation.set(gen);
        self.apply_filter();
        let q = q.trim().to_string();
        if q.chars().count() < 2 {
            self.found.borrow_mut().clear();
            self.render_results();
            return;
        }
        let p = self.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
            if p.generation.get() != gen {
                return;
            }
            glib::spawn_future_local(async move {
                let out = app::run(&["zypper", "--xmlout", "--non-interactive", "--no-refresh", "search", "--type", "package", &q]).await;
                if p.generation.get() != gen {
                    return;
                }
                // 104 = nic nie znaleziono.
                let found = if out.ok() || out.code == 104 { zypper::parse_search(&out.stdout).unwrap_or_default() } else { Vec::new() };
                *p.found.borrow_mut() = found;
                p.reload().await;
            });
        });
    }
}
