//! Pakiety: wszystkie zainstalowane pakiety systemu alfabetycznie, wiersz po wierszu, z nowszą
//! wersją, gdy jest; wyszukiwanie filtruje listę i po chwili dokłada pakiety z repozytoriów.
//! Lista ma kilka tysięcy wierszy, więc to GtkListView (tworzy tylko widoczne wiersze).

use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::app::{self, App, Op};
use crate::zypper;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pkg {
    pub name: String,
    pub version: String,
    pub summary: String,
    /// Nowsza wersja z repozytoriów, gdy jest.
    pub update: Option<String>,
    pub installed: bool,
}

/// Wynik `rpm -qa --qf '%{NAME}\t%{VERSION}-%{RELEASE}\t%{SUMMARY}\n'` z aktualizacjami,
/// alfabetycznie bez względu na wielkość liter, bez kluczy gpg-pubkey i powtórzeń (kilka architektur).
pub fn parse_installed(rpm: &str, updates: &[zypper::Update]) -> Vec<Pkg> {
    let up: HashMap<&str, &str> = updates.iter().map(|u| (u.name.as_str(), u.new.as_str())).collect();
    let mut out: Vec<Pkg> = rpm
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '\t');
            let name = it.next()?.trim();
            if name.is_empty() || name == "gpg-pubkey" {
                return None;
            }
            Some(Pkg {
                name: name.to_string(),
                version: it.next().unwrap_or("").trim().to_string(),
                summary: it.next().unwrap_or("").trim().to_string(),
                update: up.get(name).map(|s| s.to_string()),
                installed: true,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.name.cmp(&b.name)));
    out.dedup_by(|a, b| a.name == b.name);
    out
}

/// Wiersze pasujące do zapytania (nazwa albo opis).
pub fn filter<'a>(all: &'a [Pkg], q: &str) -> Vec<&'a Pkg> {
    let q = q.trim().to_lowercase();
    if q.is_empty() {
        return all.iter().collect();
    }
    all.iter().filter(|p| p.name.to_lowercase().contains(&q) || p.summary.to_lowercase().contains(&q)).collect()
}

pub struct Packages {
    pub widget: gtk::Box,
    app: Rc<App>,
    view: gtk::ListView,
    status: gtk::Label,
    none: gtk::Label,
    all: RefCell<Vec<Pkg>>,
    found: RefCell<Vec<Pkg>>,
    query: RefCell<String>,
    /// Numer wyszukiwania, żeby spóźnione wyniki nie nadpisały nowszych.
    generation: Cell<u32>,
    loaded: Cell<bool>,
}

impl Packages {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let status = super::note(crate::i18n::tr("state.loading"));
        let none = super::no_results();
        let factory = gtk::SignalListItemFactory::new();
        let view = gtk::ListView::new(None::<gtk::NoSelection>, Some(factory.clone()));
        view.add_css_class("cocktail-list");
        let sw = gtk::ScrolledWindow::new();
        sw.set_hscrollbar_policy(gtk::PolicyType::Never);
        sw.set_vexpand(true);
        sw.set_child(Some(&view));
        widget.append(&status);
        widget.append(&none);
        widget.append(&sw);
        let page = Rc::new(Packages {
            widget,
            app: app.clone(),
            view,
            status,
            none,
            all: RefCell::new(Vec::new()),
            found: RefCell::new(Vec::new()),
            query: RefCell::new(String::new()),
            generation: Cell::new(0),
            loaded: Cell::new(false),
        });
        let weak = Rc::downgrade(&page);
        factory.connect_bind(move |_, item| {
            let Some(li) = item.downcast_ref::<gtk::ListItem>() else { return };
            let Some(p) = weak.upgrade() else { return };
            let Some(obj) = li.item().and_downcast::<glib::BoxedAnyObject>() else { return };
            let pkg = obj.borrow::<Pkg>().clone();
            let row = p.row(&pkg);
            row.add_css_class(if li.position() % 2 == 1 { "st-b" } else { "st-a" });
            li.set_child(Some(&row));
            li.set_activatable(false);
        });
        let weak = Rc::downgrade(&page);
        app.on_change.borrow_mut().push(Box::new(move || {
            if let Some(p) = weak.upgrade() {
                if p.loaded.get() {
                    glib::spawn_future_local(async move { p.reload().await });
                }
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
        let rpm = app::run(&["rpm", "-qa", "--qf", "%{NAME}\\t%{VERSION}-%{RELEASE}\\t%{SUMMARY}\\n"]).await;
        let up = app::run(&["zypper", "--xmlout", "--non-interactive", "--no-refresh", "list-updates"]).await;
        let updates = if up.ok() { zypper::parse_updates(&up.stdout).unwrap_or_default() } else { Vec::new() };
        let all = parse_installed(&rpm.stdout, &updates);
        self.status.set_text(crate::i18n::tr("devices.error"));
        self.status.set_visible(all.is_empty());
        *self.all.borrow_mut() = all;
        // Pakiety z wyszukiwania, które właśnie się zainstalowały, wracają do listy głównej.
        let names: Vec<String> = self.all.borrow().iter().map(|p| p.name.clone()).collect();
        self.found.borrow_mut().retain(|f| !names.contains(&f.name));
        self.render();
    }

    fn render(&self) {
        let all = self.all.borrow();
        let q = self.query.borrow();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        let mut n = 0;
        for p in filter(&all, &q) {
            store.append(&glib::BoxedAnyObject::new(p.clone()));
            n += 1;
        }
        for p in self.found.borrow().iter().take(100) {
            store.append(&glib::BoxedAnyObject::new(p.clone()));
            n += 1;
        }
        self.view.set_model(Some(&gtk::NoSelection::new(Some(store))));
        self.none.set_visible(!q.trim().is_empty() && n == 0);
    }

    /// Wiersz: nazwa, wersja, opis, nowsza wersja i przycisk tekstowy (Zaktualizuj / Zainstaluj / Usuń).
    fn row(self: &Rc<Self>, pkg: &Pkg) -> gtk::Box {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        b.add_css_class("cocktail-pkg");
        let name = gtk::Label::new(Some(&pkg.name));
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        // Stałe szerokości kolumn: wiersze ListView nie dzielą SizeGroup.
        name.set_size_request(170, -1);
        b.append(&name);
        let ver = gtk::Label::new(Some(if pkg.installed { &pkg.version } else { crate::i18n::tr("state.available") }));
        ver.add_css_class("dim-label");
        ver.add_css_class("numeric");
        ver.set_xalign(0.0);
        ver.set_size_request(110, -1);
        ver.set_ellipsize(gtk::pango::EllipsizeMode::End);
        b.append(&ver);
        let sum = gtk::Label::new(Some(&pkg.summary));
        sum.add_css_class("dim-label");
        sum.set_xalign(0.0);
        sum.set_hexpand(true);
        sum.set_ellipsize(gtk::pango::EllipsizeMode::End);
        sum.set_width_chars(1);
        b.append(&sum);

        if let Some(op) = self.app.pending.borrow().get(&pkg.name).copied() {
            let l = gtk::Label::new(Some(match op {
                Op::Remove => crate::i18n::tr("pending.removed"),
                _ => crate::i18n::tr("pending.ready"),
            }));
            l.add_css_class("dim-label");
            b.append(&l);
            return b;
        }
        if let Some(new) = &pkg.update {
            let l = gtk::Label::new(Some(&format!("→ {new}")));
            l.set_tooltip_text(Some(&crate::i18n::trf("packages.update-to", &[("version", new)])));
            l.add_css_class("cocktail-new");
            l.add_css_class("numeric");
            b.append(&l);
        }
        let (op, label) = match (pkg.installed, pkg.update.is_some()) {
            (false, _) => (Op::Install, crate::i18n::tr("button.install")),
            (true, true) => (Op::Update, crate::i18n::tr("button.update")),
            (true, false) => (Op::Remove, crate::i18n::tr("button.remove")),
        };
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("flat");
        btn.set_valign(gtk::Align::Center);
        btn.set_visible(!(op == Op::Remove && zypper::is_protected(&pkg.name)));
        let (p, name) = (self.clone(), pkg.name.clone());
        btn.connect_clicked(move |btn| {
            let (p, name, btn) = (p.clone(), name.clone(), btn.clone());
            glib::spawn_future_local(async move {
                btn.set_sensitive(false);
                if op != Op::Remove {
                    btn.set_label(crate::i18n::tr("state.installing"));
                }
                if !p.app.change(op, vec![name.clone()], &name).await {
                    btn.set_sensitive(true);
                    btn.set_label(label);
                }
            });
        });
        b.append(&btn);
        b
    }

    /// Pole wyszukiwania z paska bocznego: filtr listy od razu, repozytoria po chwili.
    pub fn search(self: &Rc<Self>, q: &str) {
        *self.query.borrow_mut() = q.to_string();
        let gen = self.generation.get() + 1;
        self.generation.set(gen);
        let q = q.trim().to_string();
        if q.chars().count() < 2 {
            self.found.borrow_mut().clear();
            self.render();
            return;
        }
        self.render();
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
                let installed: Vec<String> = p.all.borrow().iter().map(|x| x.name.clone()).collect();
                *p.found.borrow_mut() = found
                    .into_iter()
                    .filter(|f| !f.installed && !installed.contains(&f.name))
                    .map(|f| Pkg { name: f.name, version: String::new(), summary: f.summary, update: None, installed: false })
                    .collect();
                p.render();
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_sorted_with_updates() {
        let rpm = "zypper\t1.14-1\tPackage manager\ngpg-pubkey\t1-1\tkey\nMesa\t25.2.1-1.1\tGraphics\nbash\t5.2-1\tShell\nMesa\t25.2.1-1.1\tGraphics\n";
        let up = vec![zypper::Update { name: "Mesa".into(), old: "25.2.1-1.1".into(), new: "25.2.2-1.1".into() }];
        let all = parse_installed(rpm, &up);
        let names: Vec<&str> = all.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["bash", "Mesa", "zypper"]);
        assert_eq!(all[1].update.as_deref(), Some("25.2.2-1.1"));
        assert_eq!(all[0].update, None);
        assert_eq!(filter(&all, "shell").len(), 1);
        assert_eq!(filter(&all, "  ").len(), 3);
    }
}
