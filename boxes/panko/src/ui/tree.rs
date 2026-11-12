//! Płaska lista z grupami do zwijania, jak w makiecie: pełna szerokość, wiersze
//! naprzemiennie jaśniejsze i ciemniejsze. Strona buduje `Vec<Row>`, a lista tylko je pokazuje.
//! GtkListView utrzyma też dziesiątki tysięcy plików z porównania.

use std::cell::Cell;
use std::rc::Rc;

use gtk::prelude::*;
use gtk::{gio, glib};

use crate::model::Marker;

#[derive(Clone)]
pub enum Row {
    Group {
        key: String,
        title: String,
        icon: Option<&'static str>,
        expanded: bool,
    },
    Snap {
        num: u32,
        title: String,
        icon: &'static str,
        time: String,
        selected: bool,
    },
    File {
        name: String,
        marker: Marker,
        checked: Rc<Cell<bool>>,
        /// false: pole wyboru nieaktywne (plik z /usr, tylko do odczytu).
        restorable: bool,
    },
}

pub struct Callbacks {
    pub toggle_group: Box<dyn Fn(&str)>,
    pub click_snap: Box<dyn Fn(u32)>,
    pub check_changed: Box<dyn Fn()>,
}

pub struct Tree {
    pub scroller: gtk::ScrolledWindow,
    view: gtk::ListView,
}

struct RowWidgets {
    root: gtk::Box,
    arrow: gtk::Image,
    spacer: gtk::Box,
    check: gtk::CheckButton,
    icon: gtk::Image,
    name: gtk::Label,
    time: gtk::Label,
    marker: gtk::Image,
}

impl RowWidgets {
    /// Widżety wiersza w kolejności z `setup`.
    fn of(root: &gtk::Box) -> Self {
        let mut it = std::iter::successors(root.first_child(), |w| w.next_sibling());
        let mut next = || it.next().expect("wiersz");
        RowWidgets {
            root: root.clone(),
            arrow: next().downcast().expect("strzałka"),
            spacer: next().downcast().expect("odstęp"),
            check: next().downcast().expect("pole wyboru"),
            icon: next().downcast().expect("ikona"),
            name: next().downcast().expect("nazwa"),
            time: next().downcast().expect("godzina"),
            marker: next().downcast().expect("znacznik"),
        }
    }
}

fn row_of(item: &glib::Object) -> Row {
    item.downcast_ref::<glib::BoxedAnyObject>()
        .expect("wiersz")
        .borrow::<Row>()
        .clone()
}

impl Tree {
    pub fn new(cb: Callbacks) -> Self {
        let cb = Rc::new(cb);
        let factory = gtk::SignalListItemFactory::new();

        factory.connect_setup({
            let cb = cb.clone();
            move |_, obj| {
                let li = obj.downcast_ref::<gtk::ListItem>().expect("ListItem");
                let root = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                root.add_css_class("panko-node");
                let arrow = gtk::Image::from_icon_name("pan-down-symbolic");
                arrow.add_css_class("dim-label");
                arrow.set_pixel_size(12);
                arrow.set_size_request(16, 16);
                let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                spacer.set_size_request(16, -1);
                let check = gtk::CheckButton::new();
                let icon = gtk::Image::new();
                let name = gtk::Label::new(None);
                name.set_xalign(0.0);
                name.set_hexpand(true);
                name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                let time = gtk::Label::new(None);
                time.add_css_class("panko-time");
                time.add_css_class("numeric");
                let marker = gtk::Image::new();
                marker.set_pixel_size(14);
                for w in [
                    arrow.upcast_ref::<gtk::Widget>(),
                    spacer.upcast_ref(),
                    check.upcast_ref(),
                    icon.upcast_ref(),
                    name.upcast_ref(),
                    time.upcast_ref(),
                    marker.upcast_ref(),
                ] {
                    root.append(w);
                }

                // Klik w wiersz: grupa się zwija/rozwija, migawka zaznacza/odznacza.
                let click = gtk::GestureClick::new();
                let weak = li.downgrade();
                let cb2 = cb.clone();
                click.connect_released(move |g, _, _, _| {
                    let Some(li) = weak.upgrade() else { return };
                    let Some(item) = li.item() else { return };
                    match row_of(&item) {
                        Row::Group { key, .. } => (cb2.toggle_group)(&key),
                        Row::Snap { num, .. } => (cb2.click_snap)(num),
                        Row::File { .. } => return,
                    }
                    g.set_state(gtk::EventSequenceState::Claimed);
                });
                root.add_controller(click);

                let weak = li.downgrade();
                let cb3 = cb.clone();
                check.connect_toggled(move |c| {
                    let Some(li) = weak.upgrade() else { return };
                    let Some(item) = li.item() else { return };
                    if let Row::File { checked, .. } = row_of(&item) {
                        if checked.get() != c.is_active() {
                            checked.set(c.is_active());
                            (cb3.check_changed)();
                        }
                    }
                });

                li.set_child(Some(&root));
                li.set_activatable(false);
            }
        });

        factory.connect_bind(|_, obj| {
            let li = obj.downcast_ref::<gtk::ListItem>().expect("ListItem");
            let Some(item) = li.item() else { return };
            let row = row_of(&item);
            let Some(w) = li
                .child()
                .and_downcast::<gtk::Box>()
                .map(|b| RowWidgets::of(&b))
            else {
                return;
            };
            let w = &w;
            let show = |a: bool, s: bool, c: bool, i: bool, t: bool, m: bool| {
                w.arrow.set_visible(a);
                w.spacer.set_visible(s);
                w.check.set_visible(c);
                w.icon.set_visible(i);
                w.time.set_visible(t);
                w.marker.set_visible(m);
            };
            w.root.remove_css_class("panko-group");
            w.root.remove_css_class("panko-child");
            for c in ["mk-add", "mk-rem", "mk-chg"] {
                w.marker.remove_css_class(c);
            }
            let mut selected = false;
            match &row {
                Row::Group {
                    title,
                    icon,
                    expanded,
                    ..
                } => {
                    show(true, false, false, icon.is_some(), false, false);
                    w.root.add_css_class("panko-group");
                    w.arrow.set_icon_name(Some(if *expanded {
                        "pan-down-symbolic"
                    } else {
                        "pan-end-symbolic"
                    }));
                    if let Some(i) = icon {
                        w.icon.set_icon_name(Some(i));
                    }
                    w.name.set_label(title);
                    w.root
                        .update_property(&[gtk::accessible::Property::Label(&crate::i18n::trf(
                            if *expanded { "tree.collapse" } else { "tree.expand" },
                            &[("name", title)],
                        ))]);
                }
                Row::Snap {
                    title,
                    icon,
                    time,
                    selected: sel,
                    ..
                } => {
                    show(false, true, false, true, true, false);
                    w.root.add_css_class("panko-child");
                    w.icon.set_icon_name(Some(icon));
                    w.name.set_label(title);
                    w.time.set_label(time);
                    selected = *sel;
                }
                Row::File {
                    name,
                    marker,
                    checked,
                    restorable,
                } => {
                    show(false, false, true, true, false, true);
                    w.root.add_css_class("panko-child");
                    w.check.set_active(checked.get() && *restorable);
                    w.check.set_sensitive(*restorable);
                    w.check.set_tooltip_text(if *restorable {
                        None
                    } else {
                        Some(crate::i18n::tr("changes.usr-readonly"))
                    });
                    w.check
                        .update_property(&[gtk::accessible::Property::Label(&crate::i18n::trf("tree.select", &[("name", name)]))]);
                    w.icon.set_icon_name(Some("text-x-generic-symbolic"));
                    w.name.set_label(name);
                    w.marker.set_icon_name(Some(marker.icon()));
                    w.marker.add_css_class(marker.css());
                    w.marker.set_tooltip_text(Some(marker.label()));
                    w.marker
                        .update_property(&[gtk::accessible::Property::Label(marker.label())]);
                }
            }
            {
                let r = &w.root;
                if selected {
                    r.add_css_class("sel");
                } else {
                    r.remove_css_class("sel");
                }
            }
            // Paski liczone od pozycji. set_rows wymienia całą listę, więc każdy wiersz
            // przechodzi tu po zwinięciu grupy. (Sygnał notify::position w setup psuje GtkListView 4.14.)
            stripe(li);
        });

        let view = gtk::ListView::new(None::<gtk::NoSelection>, Some(factory));
        view.add_css_class("panko-tree");
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&view)
            .build();
        Tree { scroller, view }
    }

    /// Każde odświeżenie to nowy model: wymiana wszystkich elementów przez splice
    /// kończyła się w GTK 4.14 błędem przy ponownym użyciu wierszy.
    pub fn set_rows(&self, rows: Vec<Row>) {
        let items: Vec<glib::BoxedAnyObject> =
            rows.into_iter().map(glib::BoxedAnyObject::new).collect();
        let store = gio::ListStore::new::<glib::BoxedAnyObject>();
        store.extend_from_slice(&items);
        // Pozycja przewijania zostaje.
        let adj = self.scroller.vadjustment();
        let y = adj.value();
        self.view
            .set_model(Some(&gtk::NoSelection::new(Some(store))));
        adj.set_value(y);
    }
}

/// Paski i zaznaczenie są na własnym widżecie wiersza: zmiana klas na wierszu GtkListView
/// w czasie bind psuje GTK 4.14 (segfault przy następnej zmianie modelu).
fn stripe(li: &gtk::ListItem) {
    let Some(row) = li.child() else { return };
    let even = li.position().is_multiple_of(2);
    row.remove_css_class(if even { "st-b" } else { "st-a" });
    row.add_css_class(if even { "st-a" } else { "st-b" });
}
