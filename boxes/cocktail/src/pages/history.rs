//! Historia: zmiany pakietów z dziennika zyppera, pogrupowane według dnia.

use gtk::glib;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::app::{self, App};
use crate::history;
use crate::tree::Tree;

const LIMIT: usize = 500;
const LOG: &str = "/var/log/zypp/history";

pub struct History {
    pub widget: gtk::ScrolledWindow,
    tree: Rc<Tree>,
    none: gtk::Label,
    status: gtk::Label,
    loaded: Cell<bool>,
}

impl History {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let tree = Tree::new();
        let none = super::no_results();
        let status = super::note(crate::i18n::tr("state.loading"));
        content.append(&status);
        content.append(&tree.list);
        content.append(&none);
        let page = Rc::new(History { widget: super::scrolled(&content), tree, none, status, loaded: Cell::new(false) });
        let weak = Rc::downgrade(&page);
        app.on_change.borrow_mut().push(Box::new(move || {
            if let Some(p) = weak.upgrade() {
                if p.loaded.get() {
                    p.loaded.set(false);
                    p.load();
                }
            }
        }));
        page
    }

    pub fn load(self: &Rc<Self>) {
        if self.loaded.replace(true) {
            return;
        }
        let p = self.clone();
        glib::spawn_future_local(async move {
            // Dziennik jest zwykle tylko dla roota; wtedy czyta go cocktail-query (bez hasła).
            let text = match std::fs::read_to_string(LOG) {
                Ok(t) => Some(t),
                Err(_) => {
                    let out = app::helper("cocktail-query", &["history"]).await;
                    out.ok().then_some(out.stdout)
                }
            };
            p.render(text.as_deref());
        });
    }

    fn render(&self, text: Option<&str>) {
        self.tree.clear();
        let Some(text) = text else {
            self.status.set_text(crate::i18n::tr("history.error"));
            self.status.set_visible(true);
            return;
        };
        let entries = history::parse(text);
        self.status.set_text(crate::i18n::tr("history.empty"));
        self.status.set_visible(entries.is_empty());
        let now = glib::DateTime::now_local().ok();
        let today = now.map(|d| history::days(d.year() as i64, d.month() as i64, d.day_of_month() as i64)).unwrap_or(0);
        let mut group: Option<(&str, usize)> = None;
        for e in entries.iter().take(LIMIT) {
            let g = history::group(&e.date, today);
            let idx = match group {
                Some((name, idx)) if name == g => idx,
                _ => {
                    let idx = self.tree.group(None, "document-open-recent-symbolic", crate::i18n::tr(g), g != "history.earlier");
                    group = Some((g, idx));
                    idx
                }
            };
            let when = if g == "history.today" || g == "history.yesterday" { e.time.clone() } else { format!("{} {}", e.date, e.time) };
            self.tree.leaf(Some(idx), None, &e.title(), Some(&when), None);
        }
        self.tree.refresh();
    }

    pub fn search(&self, q: &str) {
        let n = self.tree.set_filter(q);
        self.none.set_visible(!q.trim().is_empty() && n == 0);
    }
}
