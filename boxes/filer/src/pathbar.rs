//! Pasek ścieżki jak w Nemo: przyciski folderów (od Domu albo od „/”) albo pole adresu (Ctrl+L).
use crate::i18n::tr;
use crate::util;
use gtk::{gdk, gio, glib, prelude::*};
use std::rc::Rc;

pub struct PathBar {
    pub root: gtk::Box,
    stack: gtk::Stack,
    crumbs: gtk::Box,
    pub entry: gtk::Entry,
    toggle: gtk::ToggleButton,
    go: OnGo,
    drop: OnDrop,
}

pub type OnGo = Rc<dyn Fn(gio::File, bool)>;
pub type OnDrop = Rc<dyn Fn(Vec<gio::File>, gio::File, gdk::DragAction)>;

impl PathBar {
    pub fn new(on_go: OnGo, on_drop: OnDrop) -> Rc<PathBar> {
        let stack = gtk::Stack::builder().hexpand(true).transition_type(gtk::StackTransitionType::Crossfade).build();
        let crumbs = gtk::Box::builder().css_classes(["linked"]).build();
        let scroller = gtk::ScrolledWindow::builder().child(&crumbs).vscrollbar_policy(gtk::PolicyType::Never).hscrollbar_policy(gtk::PolicyType::External).hexpand(true).build();
        let entry = gtk::Entry::builder().hexpand(true).placeholder_text(tr("pathbar.placeholder")).build();
        stack.add_named(&scroller, Some("crumbs"));
        stack.add_named(&entry, Some("entry"));
        let toggle = gtk::ToggleButton::builder().icon_name("document-edit-symbolic").tooltip_text(tr("pathbar.toggle")).css_classes(["flat"]).build();
        let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.set_hexpand(true);
        root.append(&toggle);
        root.append(&stack);
        let pb = Rc::new(PathBar { root, stack, crumbs, entry, toggle, go: on_go.clone(), drop: on_drop });
        {
            let p = pb.clone();
            pb.toggle.connect_toggled(move |t| p.show_entry(t.is_active()));
        }
        {
            let p = pb.clone();
            let go = on_go.clone();
            pb.entry.connect_activate(move |e| {
                let f = util::parse_location(&e.text());
                p.toggle.set_active(false);
                go(f, false);
            });
        }
        {
            let keys = gtk::EventControllerKey::new();
            let p = pb.clone();
            keys.connect_key_pressed(move |_, k, _, _| {
                if k == gdk::Key::Escape {
                    p.toggle.set_active(false);
                    return glib::Propagation::Stop;
                }
                glib::Propagation::Proceed
            });
            pb.entry.add_controller(keys);
        }
        pb
    }

    pub fn show_entry(&self, on: bool) {
        if self.toggle.is_active() != on {
            self.toggle.set_active(on);
            return;
        }
        self.stack.set_visible_child_name(if on { "entry" } else { "crumbs" });
        if on {
            self.entry.grab_focus();
            self.entry.select_region(0, -1);
        }
    }

    pub fn set(&self, f: &gio::File) {
        self.entry.set_text(&f.parse_name());
        let (go, drop) = (self.go.clone(), self.drop.clone());
        while let Some(c) = self.crumbs.first_child() {
            self.crumbs.remove(&c);
        }
        // Od bieżącego w górę; w domu zaczynamy od „Dom”.
        let mut chain = vec![f.clone()];
        let home = gio::File::for_path(glib::home_dir());
        let mut cur = f.clone();
        while !cur.equal(&home) {
            match cur.parent() {
                Some(p) => {
                    chain.push(p.clone());
                    cur = p;
                }
                None => break,
            }
        }
        chain.reverse();
        let last = chain.len() - 1;
        for (i, d) in chain.into_iter().enumerate() {
            let b = gtk::Button::new();
            let inner = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            if i == 0 {
                inner.append(&gtk::Image::from_icon_name(util::place_icon(&d)));
            }
            if i > 0 || d.path().is_none_or(|p| p.as_os_str() != "/") {
                inner.append(&gtk::Label::new(Some(&util::display_name(&d))));
            }
            b.set_child(Some(&inner));
            if i == last {
                b.add_css_class("suggested-action");
            }
            {
                let go = go.clone();
                let d = d.clone();
                b.connect_clicked(move |_| go(d.clone(), false));
            }
            let middle = gtk::GestureClick::builder().button(2).build();
            {
                let go = go.clone();
                let d = d.clone();
                middle.connect_pressed(move |_, _, _, _| go(d.clone(), true));
            }
            b.add_controller(middle);
            let target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE);
            {
                let drop = drop.clone();
                target.connect_drop(move |t, v, _, _| {
                    let Ok(l) = v.get::<gdk::FileList>() else { return false };
                    drop(l.files(), d.clone(), t.current_drop().map(|x| x.actions()).unwrap_or(gdk::DragAction::COPY));
                    true
                });
            }
            b.add_controller(target);
            self.crumbs.append(&b);
        }
    }
}
