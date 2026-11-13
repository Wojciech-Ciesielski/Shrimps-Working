//! Lista-drzewo z makiety: zwijane kategorie, wiersze naprzemiennie jaśniejsze/ciemniejsze,
//! bez kart i obrysów. Wiersze są dopisywane w kolejności, więc indeks wiersza = indeks węzła.

use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

struct Node {
    row: gtk::ListBoxRow,
    parent: Option<usize>,
    group: bool,
    expanded: Cell<bool>,
    key: String,
    tri: Option<gtk::Image>,
}

pub struct Tree {
    pub list: gtk::ListBox,
    nodes: RefCell<Vec<Node>>,
    filter: RefCell<String>,
}

const INDENT: i32 = 22;

impl Tree {
    pub fn new() -> Rc<Self> {
        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("cocktail-tree");
        let tree = Rc::new(Tree { list, nodes: RefCell::new(Vec::new()), filter: RefCell::new(String::new()) });
        let weak: Weak<Tree> = Rc::downgrade(&tree);
        tree.list.connect_row_activated(move |_, row| {
            if let Some(t) = weak.upgrade() {
                t.toggle(row.index() as usize);
            }
        });
        tree
    }

    pub fn clear(&self) {
        self.list.remove_all();
        self.nodes.borrow_mut().clear();
    }

    fn depth(&self, parent: Option<usize>) -> i32 {
        let nodes = self.nodes.borrow();
        let mut d = 0;
        let mut p = parent;
        while let Some(i) = p {
            d += 1;
            p = nodes[i].parent;
        }
        d
    }

    fn push(&self, parent: Option<usize>, group: bool, expanded: bool, key: &str, content: gtk::Box, tri: Option<gtk::Image>) -> usize {
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&content));
        row.set_activatable(group);
        self.list.append(&row);
        let mut nodes = self.nodes.borrow_mut();
        nodes.push(Node { row, parent, group, expanded: Cell::new(expanded), key: key.to_lowercase(), tri });
        nodes.len() - 1
    }

    fn row_box(&self, parent: Option<usize>) -> gtk::Box {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        b.set_margin_start(24 + INDENT * self.depth(parent));
        b.set_margin_end(24);
        b.set_margin_top(1);
        b.set_margin_bottom(1);
        // Ta sama wysokość wiersza z przyciskiem i bez (jak w Panko).
        b.set_size_request(-1, 28);
        b
    }

    /// Zwijana kategoria.
    pub fn group(&self, parent: Option<usize>, icon: &str, title: &str, expanded: bool) -> usize {
        let b = self.row_box(parent);
        let tri = gtk::Image::from_icon_name(if expanded { "pan-down-symbolic" } else { "pan-end-symbolic" });
        tri.add_css_class("dim-label");
        b.append(&tri);
        b.append(&gtk::Image::from_icon_name(icon));
        let name = gtk::Label::new(Some(title));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.add_css_class("heading");
        b.append(&name);
        self.push(parent, true, expanded, title, b, Some(tri))
    }

    /// Zwykły wiersz: ikona, nazwa, opcjonalnie dopisek `dim` i widżet na końcu.
    pub fn leaf(&self, parent: Option<usize>, icon: Option<&str>, title: &str, extra: Option<&str>, suffix: Option<&gtk::Widget>) -> usize {
        let b = self.row_box(parent);
        // Odstęp w miejscu trójkąta kategorii, żeby ikony były w jednej linii.
        let sp = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sp.set_size_request(16, -1);
        b.append(&sp);
        if let Some(icon) = icon {
            b.append(&gtk::Image::from_icon_name(icon));
        }
        let name = gtk::Label::new(Some(title));
        name.set_xalign(0.0);
        b.append(&name);
        let mut key = title.to_string();
        if let Some(extra) = extra {
            let e = gtk::Label::new(Some(extra));
            e.add_css_class("dim-label");
            e.set_xalign(0.0);
            e.set_ellipsize(gtk::pango::EllipsizeMode::End);
            e.set_hexpand(true);
            b.append(&e);
            key.push(' ');
            key.push_str(extra);
        } else {
            name.set_hexpand(true);
            name.set_wrap(true);
        }
        if let Some(s) = suffix {
            s.set_valign(gtk::Align::Center);
            b.append(s);
        }
        self.push(parent, false, false, &key, b, None)
    }

    fn toggle(&self, idx: usize) {
        {
            let nodes = self.nodes.borrow();
            let Some(n) = nodes.get(idx) else { return };
            if !n.group || !self.filter.borrow().is_empty() {
                return;
            }
            let open = !n.expanded.get();
            n.expanded.set(open);
            if let Some(t) = &n.tri {
                t.set_icon_name(Some(if open { "pan-down-symbolic" } else { "pan-end-symbolic" }));
            }
        }
        self.refresh();
    }

    /// Filtruje wiersze; zwraca liczbę widocznych wierszy (bez kategorii).
    pub fn set_filter(&self, q: &str) -> usize {
        *self.filter.borrow_mut() = q.trim().to_lowercase();
        self.refresh()
    }

    pub fn refresh(&self) -> usize {
        let nodes = self.nodes.borrow();
        let q = self.filter.borrow();
        let n = nodes.len();
        let mut visible = vec![false; n];
        let mut leaves = 0;
        if q.is_empty() {
            for i in 0..n {
                let mut v = true;
                let mut p = nodes[i].parent;
                while let Some(j) = p {
                    v &= nodes[j].expanded.get();
                    p = nodes[j].parent;
                }
                visible[i] = v;
                if v && !nodes[i].group {
                    leaves += 1;
                }
            }
        } else {
            for i in 0..n {
                if nodes[i].group {
                    continue;
                }
                let mut hit = nodes[i].key.contains(q.as_str());
                let mut p = nodes[i].parent;
                while let Some(j) = p {
                    hit |= nodes[j].key.contains(q.as_str());
                    p = nodes[j].parent;
                }
                if hit {
                    leaves += 1;
                    visible[i] = true;
                    let mut p = nodes[i].parent;
                    while let Some(j) = p {
                        visible[j] = true;
                        p = nodes[j].parent;
                    }
                }
            }
        }
        let mut k = 0;
        for (i, node) in nodes.iter().enumerate() {
            node.row.set_visible(visible[i]);
            node.row.remove_css_class("st-b");
            if visible[i] {
                if k % 2 == 1 {
                    node.row.add_css_class("st-b");
                }
                k += 1;
            }
        }
        leaves
    }
}
