//! Panel boczny jak w Nemo: Mój komputer (Dom, foldery, Ostatnie, System plików, Kosz), Zakładki,
//! Urządzenia (z paskiem zajętości i wysuwaniem), Sieć. Menu kontekstowe, upuszczanie plików.
use crate::bookmarks;
use crate::i18n::tr;
use crate::util;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone)]
pub enum Place {
    File { file: gio::File, bookmark: bool },
    Mount(gio::Mount),
    Volume(gio::Volume),
}

impl Place {
    pub fn file(&self) -> Option<gio::File> {
        match self {
            Place::File { file, .. } => Some(file.clone()),
            Place::Mount(m) => Some(m.default_location()),
            Place::Volume(v) => v.get_mount().map(|m| m.default_location()),
        }
    }
}

/// Jak otworzyć: tu, w nowej karcie, w nowym oknie.
#[derive(Clone, Copy, PartialEq)]
pub enum Open {
    Here,
    Tab,
    Window,
}

pub type OnOpen = Rc<dyn Fn(gio::File, Open)>;
pub type OnDrop = Rc<dyn Fn(Vec<gio::File>, gio::File, gdk::DragAction)>;

pub struct Sidebar {
    pub root: gtk::ScrolledWindow,
    list: gtk::ListBox,
    rows: RefCell<Vec<Option<Place>>>,
    on_open: OnOpen,
    on_drop: OnDrop,
    parent: RefCell<Option<gtk::Window>>,
    _monitor: RefCell<Option<gio::FileMonitor>>,
    vm: gio::VolumeMonitor,
}

impl Sidebar {
    pub fn new(on_open: OnOpen, on_drop: OnDrop) -> Rc<Sidebar> {
        let list = gtk::ListBox::builder().css_classes(["navigation-sidebar"]).build();
        let root = gtk::ScrolledWindow::builder().child(&list).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();
        let sb = Rc::new(Sidebar { root, list, rows: RefCell::new(vec![]), on_open, on_drop, parent: RefCell::new(None), _monitor: RefCell::new(None), vm: gio::VolumeMonitor::get() });
        sb.rebuild();
        // Zmiany zakładek (także z innych programów) i urządzeń.
        let w = Rc::downgrade(&sb);
        let mon = gio::File::for_path(bookmarks::path()).monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE).ok();
        if let Some(m) = &mon {
            m.connect_changed(move |_, _, _, _| {
                if let Some(s) = w.upgrade() {
                    s.rebuild();
                }
            });
        }
        *sb._monitor.borrow_mut() = mon;
        let vm = sb.vm.clone();
        let w = Rc::downgrade(&sb);
        let again = move || {
            if let Some(s) = w.upgrade() {
                s.rebuild();
            }
        };
        let a = again.clone();
        vm.connect_mount_added(move |_, _| a());
        let a = again.clone();
        vm.connect_mount_removed(move |_, _| a());
        let a = again.clone();
        vm.connect_volume_added(move |_, _| a());
        let a = again.clone();
        vm.connect_volume_removed(move |_, _| a());
        let a = again;
        vm.connect_mount_changed(move |_, _| a());
        sb.connect();
        sb
    }

    pub fn set_parent(&self, w: &gtk::Window) {
        *self.parent.borrow_mut() = Some(w.clone());
    }

    fn header(&self, title: &str) {
        let l = gtk::Label::builder().label(title).xalign(0.0).css_classes(["heading", "dim-label"]).margin_top(8).margin_start(6).build();
        let row = gtk::ListBoxRow::builder().child(&l).activatable(false).selectable(false).build();
        self.list.append(&row);
        self.rows.borrow_mut().push(None);
    }

    fn row(&self, icon: &gio::Icon, name: &str, place: Place, usage: Option<f64>, eject: bool) {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        b.append(&gtk::Image::from_gicon(icon));
        let mid = gtk::Box::new(gtk::Orientation::Vertical, 2);
        mid.set_hexpand(true);
        mid.set_valign(gtk::Align::Center);
        mid.append(&gtk::Label::builder().label(name).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
        if let Some(u) = usage {
            let bar = gtk::LevelBar::builder().value(u).height_request(3).build();
            bar.add_offset_value("high", 0.9);
            mid.append(&bar);
        }
        b.append(&mid);
        let idx = self.rows.borrow().len() as i32;
        if eject {
            let e = gtk::Button::builder().icon_name("media-eject-symbolic").css_classes(["flat", "circular"]).tooltip_text(tr("side.eject")).valign(gtk::Align::Center).build();
            e.set_action_name(Some("side.eject"));
            e.set_action_target_value(Some(&idx.to_variant()));
            b.append(&e);
        }
        let row = gtk::ListBoxRow::builder().child(&b).tooltip_text(place.file().map(|f| f.parse_name().to_string()).unwrap_or_default()).build();
        self.list.append(&row);
        self.rows.borrow_mut().push(Some(place));
    }

    fn file_row(&self, f: gio::File, name: Option<String>, bookmark: bool) {
        let icon = gio::ThemedIcon::new(util::place_icon(&f));
        let name = name.unwrap_or_else(|| util::display_name(&f));
        self.row(icon.upcast_ref(), &name, Place::File { file: f, bookmark }, None, false);
    }

    pub fn rebuild(&self) {
        while let Some(c) = self.list.first_child() {
            self.list.remove(&c);
        }
        self.rows.borrow_mut().clear();
        self.header(tr("side.computer"));
        self.file_row(gio::File::for_path(glib::home_dir()), None, false);
        for d in [glib::UserDirectory::Desktop, glib::UserDirectory::Documents, glib::UserDirectory::Music, glib::UserDirectory::Pictures, glib::UserDirectory::Videos, glib::UserDirectory::Downloads] {
            if let Some(p) = glib::user_special_dir(d).filter(|p| *p != glib::home_dir() && p.is_dir()) {
                self.file_row(gio::File::for_path(p), None, false);
            }
        }
        self.file_row(gio::File::for_uri("recent:///"), None, false);
        self.file_row(gio::File::for_path("/"), None, false);
        let trash = gio::File::for_uri("trash:///");
        let full = trash.query_info("trash::item-count", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).map(|i| i.attribute_uint32("trash::item-count") > 0).unwrap_or(false);
        let icon = gio::ThemedIcon::new(if full { "user-trash-full-symbolic" } else { "user-trash-symbolic" });
        self.row(icon.upcast_ref(), tr("place.trash"), Place::File { file: trash, bookmark: false }, None, false);

        let bm = bookmarks::load();
        if !bm.is_empty() {
            self.header(tr("side.bookmarks"));
            for b in bm {
                self.file_row(gio::File::for_uri(&b.uri), b.name, true);
            }
        }

        let vm = &self.vm;
        let mut devices: Vec<(gio::Icon, String, Place, Option<f64>, bool)> = vec![];
        for m in vm.mounts() {
            if m.is_shadowed() || m.volume().is_none() && m.default_location().uri_scheme().as_deref() == Some("file") && m.root().path().is_some_and(|p| !p.starts_with("/run/media") && !p.starts_with("/media")) {
                continue;
            }
            let usage = m.root().query_filesystem_info("filesystem::size,filesystem::free", gio::Cancellable::NONE).ok().and_then(|i| {
                let (s, f) = (i.attribute_uint64("filesystem::size"), i.attribute_uint64("filesystem::free"));
                (s > 0).then(|| (s - f) as f64 / s as f64)
            });
            devices.push((m.symbolic_icon(), m.name().to_string(), Place::Mount(m.clone()), usage, m.can_eject() || m.can_unmount()));
        }
        for v in vm.volumes() {
            if v.get_mount().is_none() {
                devices.push((v.symbolic_icon(), v.name().to_string(), Place::Volume(v.clone()), None, v.can_eject()));
            }
        }
        if !devices.is_empty() {
            self.header(tr("side.devices"));
            for (i, n, p, u, e) in devices {
                self.row(&i, &n, p, u, e);
            }
        }
        self.header(tr("side.network"));
        self.file_row(gio::File::for_uri("network:///"), None, false);
    }

    /// Zaznacza miejsce odpowiadające bieżącemu folderowi.
    pub fn highlight(&self, f: &gio::File) {
        let rows = self.rows.borrow();
        let i = rows.iter().position(|p| p.as_ref().and_then(Place::file).is_some_and(|x| x.equal(f)));
        match i.and_then(|i| self.list.row_at_index(i as i32)) {
            Some(r) => self.list.select_row(Some(&r)),
            None => self.list.unselect_all(),
        }
    }

    fn place(&self, i: i32) -> Option<Place> {
        self.rows.borrow().get(i as usize).cloned().flatten()
    }

    /// Otwiera miejsce; niezamontowany wolumin najpierw montuje.
    fn activate(&self, i: i32, how: Open) {
        let Some(p) = self.place(i) else { return };
        if let Place::Volume(v) = &p {
            if v.get_mount().is_none() {
                let op = gtk::MountOperation::new(self.parent.borrow().as_ref());
                let open = self.on_open.clone();
                let v2 = v.clone();
                v.mount(gio::MountMountFlags::NONE, Some(&op), gio::Cancellable::NONE, move |r| {
                    if r.is_ok() {
                        if let Some(m) = v2.get_mount() {
                            open(m.default_location(), how);
                        }
                    }
                });
                return;
            }
        }
        if let Some(f) = p.file() {
            (self.on_open)(f, how);
        }
    }

    fn eject(&self, i: i32) {
        let op = gtk::MountOperation::new(self.parent.borrow().as_ref());
        let parent = self.parent.borrow().clone();
        let report = move |r: Result<(), glib::Error>| {
            if let Err(e) = r {
                if !e.matches(gio::IOErrorEnum::FailedHandled) {
                    let d = adw::AlertDialog::new(Some(tr("side.eject_failed")), Some(e.message()));
                    d.add_response("ok", tr("ok"));
                    d.present(parent.as_ref());
                }
            }
        };
        match self.place(i) {
            Some(Place::Mount(m)) if m.can_eject() => m.eject_with_operation(gio::MountUnmountFlags::NONE, Some(&op), gio::Cancellable::NONE, report),
            Some(Place::Mount(m)) => m.unmount_with_operation(gio::MountUnmountFlags::NONE, Some(&op), gio::Cancellable::NONE, report),
            Some(Place::Volume(v)) => v.eject_with_operation(gio::MountUnmountFlags::NONE, Some(&op), gio::Cancellable::NONE, report),
            _ => {}
        }
    }

    fn rename_bookmark(&self, i: i32) {
        let Some(Place::File { file, bookmark: true }) = self.place(i) else { return };
        let entry = gtk::Entry::builder().text(util::display_name(&file)).activates_default(true).build();
        let d = adw::AlertDialog::new(Some(tr("side.rename_title")), None);
        d.set_extra_child(Some(&entry));
        d.add_responses(&[("cancel", tr("cancel")), ("ok", tr("rename.button"))]);
        d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("ok"));
        d.connect_response(Some("ok"), move |_, _| bookmarks::rename(&file.uri(), &entry.text()));
        d.present(self.parent.borrow().as_ref());
    }

    fn connect(self: &Rc<Self>) {
        let w = Rc::downgrade(self);
        self.list.connect_row_activated(move |_, r| {
            if let Some(s) = w.upgrade() {
                s.activate(r.index(), Open::Here);
            }
        });
        // Akcje menu kontekstowego (parametr: numer wiersza).
        let group = gio::SimpleActionGroup::new();
        type Act = fn(&Sidebar, i32);
        let acts: [(&str, Act); 7] = [
            ("open", |s, i| s.activate(i, Open::Here)),
            ("tab", |s, i| s.activate(i, Open::Tab)),
            ("window", |s, i| s.activate(i, Open::Window)),
            ("eject", |s, i| s.eject(i)),
            ("rename", |s, i| s.rename_bookmark(i)),
            ("remove", |s, i| {
                if let Some(Place::File { file, .. }) = s.place(i) {
                    bookmarks::remove(&file.uri());
                    s.rebuild();
                }
            }),
            ("empty", |s, _| {
                if let Some(p) = s.parent.borrow().as_ref() {
                    crate::window::empty_trash(p);
                }
            }),
        ];
        for (name, f) in acts {
            let a = gio::SimpleAction::new(name, Some(glib::VariantTy::INT32));
            let w = Rc::downgrade(self);
            a.connect_activate(move |_, v| {
                if let (Some(s), Some(i)) = (w.upgrade(), v.and_then(|v| v.get::<i32>())) {
                    f(&s, i);
                }
            });
            group.add_action(&a);
        }
        self.root.insert_action_group("side", Some(&group));
        let click = gtk::GestureClick::builder().button(0).build();
        let w = Rc::downgrade(self);
        click.connect_pressed(move |g, _, x, y| {
            let Some(s) = w.upgrade() else { return };
            let Some(row) = s.list.row_at_y(y as i32) else { return };
            let i = row.index();
            let Some(place) = s.place(i) else { return };
            match g.current_button() {
                2 => s.activate(i, Open::Tab),
                3 => {
                    let m = gio::Menu::new();
                    let item = |label: &str, act: &str| {
                        let it = gio::MenuItem::new(Some(label), None);
                        it.set_action_and_target_value(Some(act), Some(&i.to_variant()));
                        it
                    };
                    m.append_item(&item(tr("menu.open"), "side.open"));
                    m.append_item(&item(tr("menu.open_tab"), "side.tab"));
                    m.append_item(&item(tr("menu.open_window"), "side.window"));
                    let extra = gio::Menu::new();
                    match &place {
                        Place::File { bookmark: true, .. } => {
                            extra.append_item(&item(tr("side.rename"), "side.rename"));
                            extra.append_item(&item(tr("side.remove"), "side.remove"));
                        }
                        Place::File { file, .. } if file.uri_scheme().as_deref() == Some("trash") => {
                            extra.append_item(&item(tr("trash.empty"), "side.empty"));
                        }
                        Place::Mount(_) | Place::Volume(_) => extra.append_item(&item(tr("side.eject"), "side.eject")),
                        _ => {}
                    }
                    m.append_section(None, &extra);
                    let pop = gtk::PopoverMenu::from_model(Some(&m));
                    pop.set_parent(&s.list);
                    pop.set_has_arrow(false);
                    pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                    pop.connect_closed(|p| {
                        let p = p.clone();
                        glib::idle_add_local_once(move || p.unparent());
                    });
                    pop.popup();
                }
                _ => {}
            }
        });
        self.list.add_controller(click);
        // Upuszczanie: na miejsce = kopiuj/przenieś; folder na nagłówek „Zakładki” = dodaj zakładkę.
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK);
        let w = Rc::downgrade(self);
        drop.connect_drop(move |d, v, _, y| {
            let (Some(s), Ok(list)) = (w.upgrade(), v.get::<gdk::FileList>()) else { return false };
            let target = s.list.row_at_y(y as i32).and_then(|r| s.place(r.index())).and_then(|p| p.file());
            match target {
                Some(t) => (s.on_drop)(list.files(), t, d.current_drop().map(|x| x.actions()).unwrap_or(gdk::DragAction::COPY)),
                None => {
                    for f in list.files() {
                        if f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) == gio::FileType::Directory {
                            bookmarks::add(&f);
                        }
                    }
                    s.rebuild();
                }
            }
            true
        });
        self.list.add_controller(drop);
    }
}
