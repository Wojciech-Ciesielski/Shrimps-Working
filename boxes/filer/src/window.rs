//! Okno Filera jak w Nemo: pasek menu, pasek narzędzi ze ścieżką, panel boczny, karty,
//! podział na dwa panele (F3), pasek stanu z powiększeniem. Wszystkie polecenia to akcje „win.*”.
use crate::i18n::{tr, trf};
use crate::ops::{self, Kind};
use crate::pathbar::PathBar;
use crate::places::{Open, Sidebar};
use crate::tab::{self, Tab};
use crate::{bookmarks, prefs, props, settings, util};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};

struct Pane {
    view: adw::TabView,
    root: gtk::Box,
    tabs: RefCell<Vec<(adw::TabPage, Rc<Tab>)>>,
}

pub struct Win {
    pub window: adw::ApplicationWindow,
    panes: RefCell<Vec<Rc<Pane>>>,
    active: Cell<usize>,
    split: gtk::Paned,
    side_paned: gtk::Paned,
    sidebar: Rc<Sidebar>,
    pathbar: Rc<PathBar>,
    menubar: gtk::PopoverMenuBar,
    statusbar: gtk::Box,
    status: gtk::Label,
    zoom: gtk::Scale,
    search_bar: gtk::SearchBar,
    search: gtk::SearchEntry,
    back: gtk::Button,
    forward: gtk::Button,
    up: gtk::Button,
    me: RefCell<Weak<Win>>,
    refreshing: Cell<bool>,
}

type Refill = Box<dyn Fn()>;
type StrAction = Box<dyn Fn(&Win, String)>;

thread_local! {
    /// Otwarte okna (silne odwołania: okno żyje do zamknięcia).
    static WINDOWS: RefCell<Vec<Rc<Win>>> = const { RefCell::new(Vec::new()) };
}

fn windows() -> Vec<Rc<Win>> {
    WINDOWS.with(|w| w.borrow().clone())
}

/// Akcje, które w polu tekstowym muszą zostać przy polu (Ctrl+C, Delete, Backspace…).
const TEXT_KEYS: [&str; 8] = ["copy", "cut", "paste", "select-all", "trash", "delete", "back", "undo"];

fn button(icon: &str, tip: &str, action: &str) -> gtk::Button {
    gtk::Button::builder().icon_name(icon).tooltip_text(tip).action_name(action).build()
}

pub fn home() -> gio::File {
    gio::File::for_path(glib::home_dir())
}

/// Opróżnianie kosza z pytaniem (jak w Nemo).
pub fn empty_trash(parent: &gtk::Window) {
    let d = adw::AlertDialog::new(Some(tr("trash.empty_title")), Some(tr("trash.empty_body")));
    d.add_responses(&[("cancel", tr("cancel")), ("empty", tr("trash.empty"))]);
    d.set_response_appearance("empty", adw::ResponseAppearance::Destructive);
    d.set_close_response("cancel");
    let p = parent.clone();
    d.connect_response(Some("empty"), move |_, _| {
        let trash = gio::File::for_uri("trash:///");
        let mut items = vec![];
        if let Ok(en) = trash.enumerate_children("standard::name", gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, gio::Cancellable::NONE) {
            while let Ok(Some(i)) = en.next_file(gio::Cancellable::NONE) {
                items.push(trash.child(i.name()));
            }
        }
        ops::run(&p, Kind::Delete, items, None, |_| {});
    });
    d.present(Some(parent));
}

fn terminal(dir: &std::path::Path) {
    for t in ["cherry", "kgx", "gnome-terminal", "xdg-terminal-exec", "xterm"] {
        if std::process::Command::new(t).current_dir(dir).spawn().is_ok() {
            return;
        }
    }
}

fn scripts_dir() -> PathBuf {
    glib::user_data_dir().join("nemo").join("scripts")
}

fn list_dir(p: &std::path::Path, exec_only: bool) -> Vec<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut v: Vec<PathBuf> = std::fs::read_dir(p)
        .map(|rd| {
            rd.flatten()
                .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
                .filter(|e| e.metadata().is_ok_and(|m| m.is_file() && (!exec_only || m.permissions().mode() & 0o111 != 0)))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

impl Win {
    pub fn new(app: &adw::Application, location: Option<gio::File>) -> Rc<Win> {
        let s = settings::get();
        let window = adw::ApplicationWindow::builder().application(app).default_width(s.window.0).default_height(s.window.1).icon_name("org.shrimp.Filer").build();

        let back = button("go-previous-symbolic", tr("go.back"), "win.back");
        let forward = button("go-next-symbolic", tr("go.forward"), "win.forward");
        let up = button("go-up-symbolic", tr("go.up"), "win.up");
        let reload = button("view-refresh-symbolic", tr("go.reload"), "win.reload");
        let home_b = button("go-home-symbolic", tr("go.home"), "win.home");

        let me: Rc<RefCell<Weak<Win>>> = Rc::new(RefCell::new(Weak::new()));
        let go_me = me.clone();
        let drop_me = me.clone();
        let pathbar = PathBar::new(
            Rc::new(move |f, new_tab| {
                if let Some(w) = go_me.borrow().upgrade() {
                    if new_tab { w.open_in_tab(&f, false) } else { w.tab().navigate(&f) }
                }
            }),
            Rc::new(move |files, dest, act| {
                if let Some(w) = drop_me.borrow().upgrade() {
                    w.drop_files(files, &dest, act);
                }
            }),
        );
        let open_me = me.clone();
        let drop_me = me.clone();
        let sidebar = Sidebar::new(
            Rc::new(move |f, how| {
                if let Some(w) = open_me.borrow().upgrade() {
                    match how {
                        Open::Here => w.tab().navigate(&f),
                        Open::Tab => w.open_in_tab(&f, false),
                        Open::Window => w.open_in_window(&f),
                    }
                }
            }),
            Rc::new(move |files, dest, act| {
                if let Some(w) = drop_me.borrow().upgrade() {
                    w.drop_files(files, &dest, act);
                }
            }),
        );
        sidebar.set_parent(window.upcast_ref());

        let header = adw::HeaderBar::new();
        let nav = gtk::Box::builder().css_classes(["linked"]).build();
        nav.append(&back);
        nav.append(&forward);
        header.pack_start(&nav);
        header.pack_start(&up);
        header.pack_start(&reload);
        header.pack_start(&home_b);
        header.set_title_widget(Some(&pathbar.root));
        let views = gtk::Box::builder().css_classes(["linked"]).build();
        for (v, icon, tip) in [("icons", "view-grid-symbolic", tr("view.icons")), ("list", "view-list-symbolic", tr("view.list")), ("compact", "view-list-bullet-symbolic", tr("view.compact"))] {
            let b = gtk::ToggleButton::builder().icon_name(icon).tooltip_text(tip).action_name("win.view").action_target(&v.to_variant()).build();
            views.append(&b);
        }
        header.pack_end(&views);
        let search_btn = gtk::ToggleButton::builder().icon_name("system-search-symbolic").tooltip_text(tr("go.search")).build();
        header.pack_end(&search_btn);

        let menubar = gtk::PopoverMenuBar::from_model(Some(&Self::menu_model()));
        let search = gtk::SearchEntry::builder().placeholder_text(tr("search.placeholder")).hexpand(true).build();
        let search_bar = gtk::SearchBar::builder().child(&gtk::Box::builder().hexpand(true).build()).show_close_button(true).build();
        let sb_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let clamp = adw::Clamp::builder().maximum_size(600).child(&search).hexpand(true).build();
        sb_box.append(&clamp);
        search_bar.set_child(Some(&sb_box));
        search_bar.connect_entry(&search);
        search_btn.bind_property("active", &search_bar, "search-mode-enabled").bidirectional().build();

        let split = gtk::Paned::builder().orientation(gtk::Orientation::Horizontal).wide_handle(true).shrink_start_child(false).shrink_end_child(false).build();
        let side_paned = gtk::Paned::builder().orientation(gtk::Orientation::Horizontal).start_child(&sidebar.root).end_child(&split).position(s.sidebar_width).shrink_start_child(false).resize_start_child(false).build();

        let status = gtk::Label::builder().hexpand(true).ellipsize(gtk::pango::EllipsizeMode::Middle).css_classes(["caption"]).build();
        let zoom = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 6.0, 1.0);
        zoom.set_draw_value(false);
        zoom.set_width_request(120);
        zoom.set_tooltip_text(Some(tr("view.zoom")));
        let side_toggle = gtk::Button::builder().icon_name("sidebar-show-symbolic").tooltip_text(tr("view.sidebar")).action_name("win.show-sidebar").css_classes(["flat"]).build();
        let statusbar = gtk::Box::builder().spacing(6).margin_start(6).margin_end(6).margin_top(2).margin_bottom(2).build();
        statusbar.append(&side_toggle);
        statusbar.append(&status);
        statusbar.append(&zoom);

        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&header);
        tv.add_top_bar(&menubar);
        tv.add_top_bar(&search_bar);
        tv.set_content(Some(&side_paned));
        tv.add_bottom_bar(&statusbar);
        window.set_content(Some(&tv));

        let w = Rc::new(Win {
            window,
            panes: RefCell::new(vec![]),
            active: Cell::new(0),
            split,
            side_paned,
            sidebar,
            pathbar,
            menubar,
            statusbar,
            status,
            zoom,
            search_bar,
            search,
            back,
            forward,
            up,
            me: RefCell::new(Weak::new()),
            refreshing: Cell::new(false),
        });
        *me.borrow_mut() = Rc::downgrade(&w);
        *w.me.borrow_mut() = Rc::downgrade(&w);
        WINDOWS.with(|x| x.borrow_mut().push(w.clone()));
        let gone = Rc::downgrade(&w);
        w.window.connect_destroy(move |_| {
            if let Some(g) = gone.upgrade() {
                WINDOWS.with(|x| x.borrow_mut().retain(|o| !Rc::ptr_eq(o, &g)));
            }
        });
        w.actions();
        w.connect();
        w.add_pane();
        w.add_tab(0, &location.unwrap_or_else(home), true);
        w.apply_settings();
        w.window.present();
        w.tab().focus_view();
        w
    }

    fn weak(&self) -> Weak<Win> {
        self.me.borrow().clone()
    }

    // ---------- panele i karty ----------

    fn add_pane(&self) {
        let view = adw::TabView::new();
        let bar = adw::TabBar::builder().view(&view).autohide(true).build();
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&bar);
        root.append(&view);
        view.set_vexpand(true);
        let pane = Rc::new(Pane { view: view.clone(), root: root.clone(), tabs: RefCell::new(vec![]) });
        let idx = self.panes.borrow().len();
        if idx == 0 {
            self.split.set_start_child(Some(&root));
        } else {
            self.split.set_end_child(Some(&root));
            let w = self.split.width();
            if w > 0 {
                self.split.set_position(w / 2);
            }
        }
        let me = self.weak();
        let p = Rc::downgrade(&pane);
        view.connect_selected_page_notify(move |_| {
            if let (Some(w), Some(p)) = (me.upgrade(), p.upgrade()) {
                if let Some(i) = w.panes.borrow().iter().position(|x| Rc::ptr_eq(x, &p)) {
                    w.active.set(i);
                }
                w.refresh();
            }
        });
        let me = self.weak();
        let p = Rc::downgrade(&pane);
        view.connect_page_detached(move |v, page, _| {
            let (Some(w), Some(p)) = (me.upgrade(), p.upgrade()) else { return };
            p.tabs.borrow_mut().retain(|(pg, _)| pg != page);
            if v.n_pages() == 0 {
                w.pane_empty(&p);
            }
        });
        self.panes.borrow_mut().push(pane);
    }

    fn pane_empty(&self, p: &Rc<Pane>) {
        let n = self.panes.borrow().len();
        if n == 1 {
            self.window.close();
            return;
        }
        let idx = self.panes.borrow().iter().position(|x| Rc::ptr_eq(x, p)).unwrap_or(1);
        self.panes.borrow_mut().remove(idx);
        self.split.set_end_child(None::<&gtk::Widget>);
        if idx == 0 {
            let other = self.panes.borrow()[0].root.clone();
            self.split.set_start_child(None::<&gtk::Widget>);
            self.split.set_start_child(Some(&other));
        }
        self.active.set(0);
        self.refresh();
    }

    fn add_tab(&self, pane: usize, f: &gio::File, select: bool) -> Rc<Tab> {
        let me = self.me.borrow().upgrade().unwrap();
        let t = Tab::new(&me, f);
        let p = self.panes.borrow()[pane].clone();
        let page = match p.view.selected_page() {
            Some(cur) => p.view.add_page(&t.root, Some(&cur)),
            None => p.view.append(&t.root),
        };
        *t.page.borrow_mut() = Some(page.clone());
        p.tabs.borrow_mut().push((page.clone(), t.clone()));
        if select {
            p.view.set_selected_page(&page);
            self.active.set(pane);
        }
        self.tab_changed(&t);
        t
    }

    fn pane(&self) -> Rc<Pane> {
        let ps = self.panes.borrow();
        ps.get(self.active.get()).unwrap_or(&ps[0]).clone()
    }

    /// Aktywna karta aktywnego panelu.
    pub fn tab(&self) -> Rc<Tab> {
        let p = self.pane();
        let sel = p.view.selected_page();
        let tabs = p.tabs.borrow();
        sel.and_then(|s| tabs.iter().find(|(pg, _)| *pg == s).map(|(_, t)| t.clone()))
            .or_else(|| tabs.first().map(|(_, t)| t.clone()))
            .unwrap_or_else(|| self.all_tabs()[0].clone())
    }

    /// Karta w drugim panelu (do „Kopiuj do → drugi panel”).
    fn other_tab(&self) -> Option<Rc<Tab>> {
        let ps = self.panes.borrow();
        if ps.len() < 2 {
            return None;
        }
        let p = &ps[1 - self.active.get().min(1)];
        let sel = p.view.selected_page()?;
        let tabs = p.tabs.borrow();
        tabs.iter().find(|(pg, _)| *pg == sel).map(|(_, t)| t.clone())
    }

    fn all_tabs(&self) -> Vec<Rc<Tab>> {
        self.panes.borrow().iter().flat_map(|p| p.tabs.borrow().iter().map(|(_, t)| t.clone()).collect::<Vec<_>>()).collect()
    }

    fn is_active(&self, t: &Tab) -> bool {
        let p = self.pane();
        let sel = p.view.selected_page();
        let tabs = p.tabs.borrow();
        sel.is_some_and(|s| tabs.iter().any(|(pg, x)| *pg == s && std::ptr::eq(x.as_ref(), t)))
    }

    pub fn focus_tab(&self, t: &Tab) {
        let i = self.panes.borrow().iter().position(|p| p.tabs.borrow().iter().any(|(_, x)| std::ptr::eq(x.as_ref(), t)));
        if let Some(i) = i {
            if self.active.get() != i {
                self.active.set(i);
                self.refresh();
            }
        }
    }

    pub fn tab_changed(&self, t: &Tab) {
        if let Some(page) = t.page.borrow().as_ref() {
            let title = match t.searching() {
                Some(q) => trf("search.tab", &[("query", &q)]),
                None => util::display_name(&t.location()),
            };
            page.set_title(&title);
            page.set_tooltip(&t.location().parse_name());
            page.set_icon(Some(&gio::ThemedIcon::new(util::place_icon(&t.location()))));
            page.set_loading(t.is_loading());
        }
        if self.is_active(t) {
            self.refresh();
        }
    }

    /// Odświeża okno według aktywnej karty.
    fn refresh(&self) {
        if self.pane().tabs.borrow().is_empty() {
            return;
        }
        if self.refreshing.replace(true) {
            return;
        }
        let t = self.tab();
        let loc = t.location();
        let s = settings::get();
        self.window.set_title(Some(&if s.full_path_title { loc.parse_name().to_string() } else { util::display_name(&loc) }));
        self.pathbar.set(&loc);
        self.sidebar.highlight(&loc);
        self.back.set_sensitive(t.can_back());
        self.forward.set_sensitive(t.can_forward());
        self.up.set_sensitive(loc.parent().is_some());
        self.status.set_label(&t.status());
        self.zoom.set_value(t.zoom() as f64);
        if let Some(a) = self.window.lookup_action("view") {
            a.change_state(&t.view().to_variant());
        }
        if t.searching().is_none() && self.search_bar.is_search_mode() && !self.search.has_focus() {
            self.search.set_text("");
        }
        self.refreshing.set(false);
    }

    pub fn open_in_tab(&self, f: &gio::File, select: bool) {
        self.add_tab(self.active.get(), f, select);
    }

    pub fn open_in_window(&self, f: &gio::File) {
        if let Some(app) = self.window.application().and_downcast::<adw::Application>() {
            Win::new(&app, Some(f.clone()));
        }
    }

    // ---------- otwieranie ----------

    pub fn open_info(&self, t: &Tab, info: &gio::FileInfo) {
        let target = info.attribute_string("standard::target-uri").map(|u| gio::File::for_uri(&u)).or_else(|| util::file_of(info));
        let Some(f) = target else { return };
        if t.is_trash() {
            // W koszu nie otwieramy plików (jak w Nemo): pokazujemy właściwości.
            if !util::is_dir(info) {
                props::show(self.window.upcast_ref(), vec![f]);
                return;
            }
        }
        if util::is_dir(info) {
            t.navigate(&f);
        } else {
            gtk::FileLauncher::new(Some(&f)).launch(Some(&self.window), gio::Cancellable::NONE, |_| {});
        }
    }

    pub fn open_selection(&self, t: &Tab, new_tab: bool) {
        let infos = t.infos();
        let many = infos.len() > 1;
        for i in infos {
            if util::is_dir(&i) && (new_tab || many) {
                if let Some(f) = util::file_of(&i) {
                    self.open_in_tab(&f, false);
                }
            } else {
                self.open_info(t, &i);
            }
        }
    }

    // ---------- schowek ----------

    fn set_clipboard(&self, files: Vec<gio::File>, cut: bool) {
        if files.is_empty() {
            return;
        }
        let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
        let gnome = format!("{}\n{}", if cut { "cut" } else { "copy" }, uris.join("\n"));
        let list = uris.iter().map(|u| format!("{u}\r\n")).collect::<String>();
        let plain = files.iter().map(|f| f.parse_name().to_string()).collect::<Vec<_>>().join("\n");
        let p = gdk::ContentProvider::new_union(&[
            gdk::ContentProvider::for_bytes("x-special/gnome-copied-files", &glib::Bytes::from_owned(gnome.into_bytes())),
            gdk::ContentProvider::for_bytes("text/uri-list", &glib::Bytes::from_owned(list.into_bytes())),
            gdk::ContentProvider::for_bytes("text/plain;charset=utf-8", &glib::Bytes::from_owned(plain.into_bytes())),
        ]);
        let _ = self.window.clipboard().set_content(Some(&p));
        tab::CUT.with(|c| *c.borrow_mut() = if cut { uris } else { vec![] });
        for w in windows() {
            for t in w.all_tabs() {
                t.redraw();
            }
        }
    }

    fn paste(&self, dest: gio::File) {
        let cb = self.window.clipboard();
        let me = self.weak();
        glib::spawn_future_local(async move {
            let Ok((stream, mime)) = cb.read_future(&["x-special/gnome-copied-files", "text/uri-list"], glib::Priority::DEFAULT).await else { return };
            let out = gio::MemoryOutputStream::new_resizable();
            if out.splice_future(&stream, gio::OutputStreamSpliceFlags::CLOSE_SOURCE | gio::OutputStreamSpliceFlags::CLOSE_TARGET, glib::Priority::DEFAULT).await.is_err() {
                return;
            }
            let bytes = out.steal_as_bytes();
            let text = String::from_utf8_lossy(&bytes).to_string();
            let mut lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
            let mut cut = false;
            if mime == "x-special/gnome-copied-files" && !lines.is_empty() {
                cut = lines.remove(0) == "cut";
            }
            let files: Vec<gio::File> = lines.iter().map(|u| gio::File::for_uri(u)).collect();
            let Some(w) = me.upgrade() else { return };
            let t = w.tab();
            let tw = t.weak();
            ops::run(w.window.upcast_ref(), if cut { Kind::Move } else { Kind::Copy }, files, Some(dest), move |created| {
                if let Some(t) = tw.upgrade() {
                    t.select_when_ready(created);
                }
            });
            if cut {
                tab::CUT.with(|c| c.borrow_mut().clear());
                cb.set_content(None::<&gdk::ContentProvider>).ok();
            }
        });
    }

    /// Upuszczenie plików: jednoznaczna akcja wg źródła, inaczej przeniesienie na tym samym dysku, kopia na inny.
    pub fn drop_files(&self, files: Vec<gio::File>, dest: &gio::File, actions: gdk::DragAction) {
        if files.is_empty() || files.iter().any(|f| f.equal(dest) || f.parent().is_some_and(|p| p.equal(dest)) && actions != gdk::DragAction::COPY) {
            return;
        }
        let kind = if actions == gdk::DragAction::LINK {
            Kind::Link
        } else if actions == gdk::DragAction::COPY {
            Kind::Copy
        } else if actions == gdk::DragAction::MOVE {
            Kind::Move
        } else {
            let fs = |f: &gio::File| f.query_info("id::filesystem", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok().and_then(|i| i.attribute_string("id::filesystem")).map(|s| s.to_string());
            if fs(&files[0]).is_some() && fs(&files[0]) == fs(dest) { Kind::Move } else { Kind::Copy }
        };
        let tw = self.tab().weak();
        ops::run(self.window.upcast_ref(), kind, files, Some(dest.clone()), move |created| {
            if let Some(t) = tw.upgrade() {
                t.select_when_ready(created);
            }
        });
    }

    pub fn empty_trash(&self) {
        empty_trash(self.window.upcast_ref());
    }

    // ---------- zmiana nazwy, nowe pliki ----------

    fn rename(&self, f: &gio::File) {
        let t = self.tab();
        let current = util::display_name(f);
        let entry = gtk::Entry::builder().text(&current).activates_default(true).build();
        let dot = if f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) == gio::FileType::Directory { None } else { current.rfind('.').filter(|i| *i > 0) };
        let select = move |e: &gtk::Entry| {
            let end = dot.map(|i| current[..i].chars().count() as i32).unwrap_or(-1);
            e.select_region(0, end);
        };
        let tw = t.weak();
        let p = self.window.clone();
        let file = f.clone();
        let commit = Rc::new(move |name: String| {
            if !util::valid_name(&name) || name == util::display_name(&file) {
                return;
            }
            let old = util::display_name(&file);
            match file.set_display_name(&name, gio::Cancellable::NONE) {
                Ok(nf) => {
                    ops::push_undo(ops::Undo::Renamed(nf.clone(), old));
                    if let Some(t) = tw.upgrade() {
                        t.select_when_ready(vec![nf]);
                    }
                }
                Err(e) => {
                    let d = adw::AlertDialog::new(Some(tr("rename.failed")), Some(e.message()));
                    d.add_response("ok", tr("ok"));
                    d.present(Some(&p));
                }
            }
        });
        // W miejscu: dymek nad plikiem; gdy plik niewidoczny, okno dialogowe.
        if let Some(w) = t.widget_for(f) {
            let pop = gtk::Popover::builder().child(&entry).build();
            pop.set_parent(&w);
            let c = commit.clone();
            let pw = pop.downgrade();
            entry.connect_activate(move |e| {
                c(e.text().to_string());
                if let Some(p) = pw.upgrade() {
                    p.popdown();
                }
            });
            pop.connect_closed(|p| {
                let p = p.clone();
                glib::idle_add_local_once(move || p.unparent());
            });
            pop.popup();
            entry.grab_focus();
            select(&entry);
        } else {
            let d = adw::AlertDialog::new(Some(tr("rename.title")), None);
            d.set_extra_child(Some(&entry));
            d.add_responses(&[("cancel", tr("cancel")), ("ok", tr("rename.button"))]);
            d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
            d.set_default_response(Some("ok"));
            let e2 = entry.clone();
            d.connect_response(Some("ok"), move |_, _| commit(e2.text().to_string()));
            d.present(Some(&self.window));
            let e = entry.clone();
            glib::idle_add_local_once(move || {
                e.grab_focus();
                select(&e);
            });
        }
    }

    fn rename_later(&self, f: gio::File) {
        let me = self.weak();
        let mut tries = 0;
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            tries += 1;
            let Some(w) = me.upgrade() else { return glib::ControlFlow::Break };
            if w.tab().widget_for(&f).is_some() || tries > 15 {
                w.rename(&f);
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    fn new_folder(&self) {
        let t = self.tab();
        let dir = t.location();
        let f = dir.child(util::free_name(&dir, tr("new.folder"), false));
        match f.make_directory(gio::Cancellable::NONE) {
            Ok(()) => {
                ops::push_undo(ops::Undo::Created(vec![f.clone()]));
                t.select_when_ready(vec![f.clone()]);
                self.rename_later(f);
            }
            Err(e) => self.error(tr("new.failed"), e.message()),
        }
    }

    fn new_document(&self, template: &str) {
        let t = self.tab();
        let dir = t.location();
        let r = if template.is_empty() {
            let f = dir.child(util::free_name(&dir, tr("new.document"), false));
            f.create(gio::FileCreateFlags::NONE, gio::Cancellable::NONE).map(|_| f)
        } else {
            let src = gio::File::for_path(template);
            let base = src.basename().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
            let f = dir.child(util::free_name(&dir, &base, false));
            src.copy(&f, gio::FileCopyFlags::NONE, gio::Cancellable::NONE, None).map(|_| f)
        };
        match r {
            Ok(f) => {
                ops::push_undo(ops::Undo::Created(vec![f.clone()]));
                t.select_when_ready(vec![f.clone()]);
                self.rename_later(f);
            }
            Err(e) => self.error(tr("new.failed"), e.message()),
        }
    }

    fn error(&self, title: &str, body: &str) {
        let d = adw::AlertDialog::new(Some(title), Some(body));
        d.add_response("ok", tr("ok"));
        d.present(Some(&self.window));
    }

    // ---------- menu ----------

    fn menu_model() -> gio::Menu {
        let item = |m: &gio::Menu, label: &str, action: &str| m.append(Some(label), Some(action));
        let bar = gio::Menu::new();

        let file = gio::Menu::new();
        let s = gio::Menu::new();
        item(&s, tr("menu.new_tab"), "win.new-tab");
        item(&s, tr("menu.new_window"), "win.new-window");
        item(&s, tr("menu.new_folder"), "win.new-folder");
        file.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("menu.connect"), "win.connect");
        item(&s, tr("menu.properties"), "win.properties");
        file.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("menu.close_tab"), "win.close-tab");
        item(&s, tr("menu.close_window"), "window.close");
        item(&s, tr("menu.quit"), "app.quit");
        file.append_section(None, &s);
        bar.append_submenu(Some(tr("menu.file")), &file);

        let edit = gio::Menu::new();
        let s = gio::Menu::new();
        item(&s, tr("menu.undo"), "win.undo");
        edit.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("menu.cut"), "win.cut");
        item(&s, tr("menu.copy"), "win.copy");
        item(&s, tr("menu.paste"), "win.paste");
        edit.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("menu.select_all"), "win.select-all");
        item(&s, tr("menu.select_pattern"), "win.select-pattern");
        item(&s, tr("menu.invert"), "win.invert");
        edit.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("menu.rename"), "win.rename");
        item(&s, tr("action.trash"), "win.trash");
        item(&s, tr("menu.delete"), "win.delete");
        edit.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("menu.preferences"), "win.preferences");
        edit.append_section(None, &s);
        bar.append_submenu(Some(tr("menu.edit")), &edit);

        let view = gio::Menu::new();
        let s = gio::Menu::new();
        item(&s, tr("go.reload"), "win.reload");
        view.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("view.hidden"), "win.show-hidden");
        item(&s, tr("view.menubar"), "win.show-menubar");
        item(&s, tr("view.sidebar"), "win.show-sidebar");
        item(&s, tr("view.statusbar"), "win.show-statusbar");
        item(&s, tr("view.split"), "win.split");
        item(&s, tr("view.location"), "win.location");
        view.append_section(None, &s);
        view.append_section(None, &Self::view_section());
        let s = gio::Menu::new();
        item(&s, tr("view.zoom_in"), "win.zoom-in");
        item(&s, tr("view.zoom_out"), "win.zoom-out");
        item(&s, tr("view.zoom_normal"), "win.zoom-normal");
        view.append_section(None, &s);
        view.append_submenu(Some(tr("sort.title")), &Self::sort_menu());
        bar.append_submenu(Some(tr("menu.view")), &view);

        let go = gio::Menu::new();
        let s = gio::Menu::new();
        item(&s, tr("go.back"), "win.back");
        item(&s, tr("go.forward"), "win.forward");
        item(&s, tr("go.up"), "win.up");
        go.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("go.home"), "win.home");
        item(&s, tr("place.computer"), "win.go::computer:///");
        item(&s, tr("place.recent"), "win.go::recent:///");
        item(&s, tr("place.trash"), "win.go::trash:///");
        item(&s, tr("place.network"), "win.go::network:///");
        go.append_section(None, &s);
        let s = gio::Menu::new();
        item(&s, tr("view.location"), "win.location");
        item(&s, tr("go.search"), "win.search");
        go.append_section(None, &s);
        bar.append_submenu(Some(tr("menu.go")), &go);

        let bm = gio::Menu::new();
        let s = gio::Menu::new();
        item(&s, tr("bookmarks.add"), "win.bookmark");
        item(&s, tr("bookmarks.edit"), "win.edit-bookmarks");
        bm.append_section(None, &s);
        bar.append_submenu(Some(tr("menu.bookmarks")), &bm);

        let help = gio::Menu::new();
        item(&help, tr("menu.about"), "win.about");
        bar.append_submenu(Some(tr("menu.help")), &help);
        bar
    }

    fn view_section() -> gio::Menu {
        let s = gio::Menu::new();
        s.append(Some(tr("view.icons")), Some("win.view::icons"));
        s.append(Some(tr("view.list")), Some("win.view::list"));
        s.append(Some(tr("view.compact")), Some("win.view::compact"));
        s
    }

    fn sort_menu() -> gio::Menu {
        let m = gio::Menu::new();
        let s = gio::Menu::new();
        for (k, l) in [("name", tr("sort.name")), ("size", tr("sort.size")), ("type", tr("sort.type")), ("modified", tr("sort.modified"))] {
            s.append(Some(l), Some(&format!("win.sort::{k}")));
        }
        m.append_section(None, &s);
        let s = gio::Menu::new();
        s.append(Some(tr("sort.reverse")), Some("win.reverse"));
        s.append(Some(tr("sort.folders_first")), Some("win.folders-first"));
        m.append_section(None, &s);
        m
    }

    /// Menu kontekstowe pliku albo tła.
    pub fn context_menu(&self, t: &Tab, w: &gtk::Widget, x: f64, y: f64, on_item: bool) {
        let m = gio::Menu::new();
        let sec = || gio::Menu::new();
        let infos = t.infos();
        let s = settings::get();
        if on_item && t.is_trash() {
            let a = sec();
            a.append(Some(tr("trash.restore")), Some("win.restore"));
            a.append(Some(tr("menu.delete")), Some("win.delete"));
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("menu.properties")), Some("win.properties"));
            m.append_section(None, &a);
        } else if on_item {
            let all_dirs = infos.iter().all(util::is_dir);
            let a = sec();
            a.append(Some(tr("menu.open")), Some("win.open"));
            if all_dirs {
                a.append(Some(tr("menu.open_tab")), Some("win.open-tab"));
                a.append(Some(tr("menu.open_window")), Some("win.open-window"));
            }
            if infos.len() == 1 && !all_dirs {
                let ow = sec();
                if let Some(ct) = infos[0].content_type() {
                    for app in gio::AppInfo::all_for_type(&ct).iter().take(12) {
                        if let Some(id) = app.id() {
                            let it = gio::MenuItem::new(Some(&app.display_name()), None);
                            it.set_action_and_target_value(Some("win.open-with"), Some(&id.to_variant()));
                            if let Some(i) = app.icon() {
                                it.set_icon(&i);
                            }
                            ow.append_item(&it);
                        }
                    }
                }
                let other = gio::MenuItem::new(Some(tr("menu.open_with_other")), None);
                other.set_action_and_target_value(Some("win.open-with"), Some(&"".to_variant()));
                let o = sec();
                o.append_item(&other);
                let sub = gio::Menu::new();
                sub.append_section(None, &ow);
                sub.append_section(None, &o);
                a.append_submenu(Some(tr("menu.open_with")), &sub);
            }
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("menu.cut")), Some("win.cut"));
            a.append(Some(tr("menu.copy")), Some("win.copy"));
            if infos.len() == 1 && all_dirs {
                a.append(Some(tr("menu.paste_into")), Some("win.paste-into"));
            }
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("menu.make_link")), Some("win.make-link"));
            a.append(Some(tr("menu.rename")), Some("win.rename"));
            let dests = |act: &str| {
                let d = gio::Menu::new();
                d.append(Some(tr("place.home")), Some(&format!("win.{act}::home")));
                d.append(Some(tr("dest.desktop")), Some(&format!("win.{act}::desktop")));
                if self.panes.borrow().len() > 1 {
                    d.append(Some(tr("dest.other_pane")), Some(&format!("win.{act}::pane")));
                }
                d.append(Some(tr("dest.other")), Some(&format!("win.{act}::other")));
                d
            };
            a.append_submenu(Some(tr("menu.copy_to")), &dests("copy-to"));
            a.append_submenu(Some(tr("menu.move_to")), &dests("move-to"));
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("action.trash")), Some("win.trash"));
            if s.show_delete_item {
                a.append(Some(tr("menu.delete")), Some("win.delete"));
            }
            m.append_section(None, &a);
            let a = sec();
            Self::scripts(&a);
            if infos.len() == 1 && all_dirs {
                a.append(Some(tr("menu.terminal")), Some("win.terminal-selected"));
            }
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("menu.properties")), Some("win.properties"));
            m.append_section(None, &a);
        } else {
            let a = sec();
            if !t.is_trash() {
                a.append(Some(tr("menu.new_folder")), Some("win.new-folder"));
                let docs = gio::Menu::new();
                if let Some(td) = glib::user_special_dir(glib::UserDirectory::Templates) {
                    for p in list_dir(&td, false) {
                        let it = gio::MenuItem::new(Some(&p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()), None);
                        it.set_action_and_target_value(Some("win.new-doc"), Some(&p.to_string_lossy().to_variant()));
                        docs.append_item(&it);
                    }
                }
                let empty = gio::MenuItem::new(Some(tr("new.empty")), None);
                empty.set_action_and_target_value(Some("win.new-doc"), Some(&"".to_variant()));
                let e = gio::Menu::new();
                e.append_item(&empty);
                docs.append_section(None, &e);
                a.append_submenu(Some(tr("menu.new_document")), &docs);
            }
            m.append_section(None, &a);
            let a = sec();
            Self::scripts(&a);
            if t.location().path().is_some() {
                a.append(Some(tr("menu.terminal")), Some("win.terminal"));
            }
            m.append_section(None, &a);
            let a = sec();
            if t.is_trash() {
                a.append(Some(tr("trash.empty")), Some("win.empty-trash"));
            } else {
                a.append(Some(tr("menu.paste")), Some("win.paste"));
            }
            a.append(Some(tr("menu.select_all")), Some("win.select-all"));
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("view.hidden")), Some("win.show-hidden"));
            a.append_submenu(Some(tr("sort.title")), &Self::sort_menu());
            a.append_submenu(Some(tr("menu.view")), &Self::view_section());
            m.append_section(None, &a);
            let a = sec();
            a.append(Some(tr("menu.properties")), Some("win.properties-folder"));
            m.append_section(None, &a);
        }
        let pop = gtk::PopoverMenu::from_model(Some(&m));
        pop.set_parent(w);
        pop.set_has_arrow(false);
        pop.set_halign(gtk::Align::Start);
        pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
        pop.connect_closed(|p| {
            let p = p.clone();
            glib::idle_add_local_once(move || p.unparent());
        });
        pop.popup();
    }

    /// Podmenu „Skrypty” z ~/.local/share/nemo/scripts (zgodne z Nemo).
    fn scripts(m: &gio::Menu) {
        let list = list_dir(&scripts_dir(), true);
        if list.is_empty() {
            return;
        }
        let sub = gio::Menu::new();
        for p in list {
            let it = gio::MenuItem::new(Some(&p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()), None);
            it.set_action_and_target_value(Some("win.script"), Some(&p.to_string_lossy().to_variant()));
            sub.append_item(&it);
        }
        let open = gio::Menu::new();
        open.append(Some(tr("menu.scripts_folder")), Some("win.scripts-folder"));
        sub.append_section(None, &open);
        m.append_submenu(Some(tr("menu.scripts")), &sub);
    }

    // ---------- ustawienia ----------

    pub fn settings_changed(&self) {
        for w in windows() {
            w.apply_settings();
            for t in w.all_tabs() {
                t.refilter();
                t.resort();
                t.redraw();
            }
        }
    }

    fn apply_settings(&self) {
        let s = settings::get();
        self.menubar.set_visible(s.show_menubar);
        self.statusbar.set_visible(s.show_statusbar);
        self.sidebar.root.set_visible(s.show_sidebar);
        for (name, v) in [
            ("show-hidden", s.show_hidden),
            ("show-menubar", s.show_menubar),
            ("show-sidebar", s.show_sidebar),
            ("show-statusbar", s.show_statusbar),
            ("reverse", s.sort_reverse),
            ("folders-first", s.folders_first),
        ] {
            if let Some(a) = self.window.lookup_action(name).and_downcast::<gio::SimpleAction>() {
                a.set_state(&v.to_variant());
            }
        }
        if let Some(a) = self.window.lookup_action("sort").and_downcast::<gio::SimpleAction>() {
            a.set_state(&s.sort.to_variant());
        }
        for t in self.all_tabs() {
            t.set_single_click(s.single_click);
        }
        self.refresh();
    }

    fn edit_bookmarks(&self) {
        let list = gtk::ListBox::builder().css_classes(["boxed-list"]).selection_mode(gtk::SelectionMode::None).build();
        let fill: Rc<RefCell<Option<Refill>>> = Rc::new(RefCell::new(None));
        let l2 = list.clone();
        let f2 = fill.clone();
        *fill.borrow_mut() = Some(Box::new(move || {
            while let Some(c) = l2.first_child() {
                l2.remove(&c);
            }
            for b in bookmarks::load() {
                let r = adw::EntryRow::builder().title(gio::File::for_uri(&b.uri).parse_name()).text(b.name.clone().unwrap_or_default()).show_apply_button(true).build();
                let uri = b.uri.clone();
                r.connect_apply(move |e| bookmarks::rename(&uri, &e.text()));
                let del = gtk::Button::builder().icon_name("user-trash-symbolic").valign(gtk::Align::Center).css_classes(["flat"]).tooltip_text(tr("side.remove")).build();
                let uri = b.uri.clone();
                let f3 = f2.clone();
                del.connect_clicked(move |_| {
                    bookmarks::remove(&uri);
                    if let Some(f) = f3.borrow().as_ref() {
                        f();
                    }
                });
                r.add_suffix(&del);
                l2.append(&r);
            }
        }));
        if let Some(f) = fill.borrow().as_ref() {
            f();
        }
        let page = adw::PreferencesPage::new();
        let g = adw::PreferencesGroup::builder().description(tr("bookmarks.edit_hint")).build();
        g.add(&list);
        page.add(&g);
        let d = adw::Dialog::builder().title(tr("bookmarks.edit")).content_width(520).content_height(420).build();
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&adw::HeaderBar::new());
        tv.set_content(Some(&page));
        d.set_child(Some(&tv));
        d.present(Some(&self.window));
    }

    // ---------- akcje ----------

    fn actions(&self) {
        let win = &self.window;
        let add = |name: &str, f: Box<dyn Fn(&Win)>| {
            let a = gio::SimpleAction::new(name, None);
            let me = self.weak();
            a.connect_activate(move |_, _| {
                if let Some(w) = me.upgrade() {
                    f(&w);
                }
            });
            win.add_action(&a);
        };
        let add_s = |name: &str, f: StrAction| {
            let a = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
            let me = self.weak();
            a.connect_activate(move |_, v| {
                if let (Some(w), Some(s)) = (me.upgrade(), v.and_then(|v| v.get::<String>())) {
                    f(&w, s);
                }
            });
            win.add_action(&a);
        };
        let toggle = |name: &str, f: fn(&mut settings::Settings, bool), initial: bool| {
            let a = gio::SimpleAction::new_stateful(name, None, &initial.to_variant());
            let me = self.weak();
            a.connect_activate(move |a, _| {
                let v = !a.state().and_then(|s| s.get::<bool>()).unwrap_or(false);
                a.set_state(&v.to_variant());
                settings::update(|s| f(s, v));
                if let Some(w) = me.upgrade() {
                    w.settings_changed();
                }
            });
            win.add_action(&a);
        };
        let s = settings::get();
        let sel = |w: &Win| w.tab().selected();
        let par = |w: &Win| -> gtk::Window { w.window.clone().upcast() };

        add("back", Box::new(|w| w.tab().go_back()));
        add("forward", Box::new(|w| w.tab().go_forward()));
        add("up", Box::new(|w| w.tab().go_up()));
        add("reload", Box::new(|w| w.tab().reload()));
        add("home", Box::new(|w| w.tab().navigate(&home())));
        add_s("go", Box::new(|w, u| w.tab().navigate(&gio::File::for_uri(&u))));
        add("location", Box::new(|w| w.pathbar.show_entry(true)));
        add("search", Box::new(|w| {
            let on = !w.search_bar.is_search_mode();
            w.search_bar.set_search_mode(on);
            if on {
                w.search.grab_focus();
            }
        }));
        add("new-tab", Box::new(|w| {
            let loc = w.tab().location();
            w.open_in_tab(&loc, true);
        }));
        add("new-window", Box::new(|w| {
            let loc = w.tab().location();
            w.open_in_window(&loc);
        }));
        add("close-tab", Box::new(|w| {
            let p = w.pane();
            if let Some(pg) = p.view.selected_page() {
                p.view.close_page(&pg);
            }
        }));
        add("split", Box::new(|w| {
            if w.panes.borrow().len() > 1 {
                let p = w.panes.borrow()[1].clone();
                let pages: Vec<adw::TabPage> = p.tabs.borrow().iter().map(|(pg, _)| pg.clone()).collect();
                for pg in pages {
                    p.view.close_page(&pg);
                }
            } else {
                let loc = w.tab().location();
                w.add_pane();
                w.add_tab(1, &loc, true);
                w.tab().focus_view();
            }
        }));
        add("open", Box::new(|w| w.open_selection(&w.tab(), false)));
        add("open-tab", Box::new(|w| w.open_selection(&w.tab(), true)));
        add("open-window", Box::new(move |w| {
            for f in sel(w) {
                w.open_in_window(&f);
            }
        }));
        add_s("open-with", Box::new(move |w, id| {
            let files = sel(w);
            if id.is_empty() {
                if let Some(f) = files.first() {
                    let l = gtk::FileLauncher::new(Some(f));
                    l.set_always_ask(true);
                    l.launch(Some(&w.window), gio::Cancellable::NONE, |_| {});
                }
            } else if let Some(app) = gio::AppInfo::all().into_iter().find(|a| a.id().as_deref() == Some(id.as_str())) {
                let ctx = WidgetExt::display(&w.window).app_launch_context();
                let _ = app.launch(&files, Some(&ctx));
            }
        }));
        add("cut", Box::new(move |w| w.set_clipboard(sel(w), true)));
        add("copy", Box::new(move |w| w.set_clipboard(sel(w), false)));
        add("paste", Box::new(|w| {
            let loc = w.tab().location();
            w.paste(loc);
        }));
        add("paste-into", Box::new(move |w| {
            if let Some(f) = sel(w).into_iter().next() {
                w.paste(f);
            }
        }));
        add("make-link", Box::new(move |w| {
            let t = w.tab();
            let tw = t.weak();
            ops::run(&par(w), Kind::Link, sel(w), Some(t.location()), move |c| {
                if let Some(t) = tw.upgrade() {
                    t.select_when_ready(c);
                }
            });
        }));
        add("rename", Box::new(move |w| {
            if let Some(f) = sel(w).into_iter().next() {
                w.rename(&f);
            }
        }));
        for (name, kind) in [("copy-to", Kind::Copy), ("move-to", Kind::Move)] {
            add_s(name, Box::new(move |w, dest| {
                let files = sel(w);
                let run = {
                    let p = par(w);
                    move |d: gio::File| ops::run(&p, kind, files.clone(), Some(d), |_| {})
                };
                match dest.as_str() {
                    "home" => run(home()),
                    "desktop" => run(gio::File::for_path(glib::user_special_dir(glib::UserDirectory::Desktop).unwrap_or_else(glib::home_dir))),
                    "pane" => {
                        if let Some(o) = w.other_tab() {
                            run(o.location());
                        }
                    }
                    _ => {
                        let d = gtk::FileDialog::builder().title(tr("dest.choose")).modal(true).build();
                        d.set_initial_folder(Some(&w.tab().location()));
                        d.select_folder(Some(&w.window), gio::Cancellable::NONE, move |r| {
                            if let Ok(f) = r {
                                run(f);
                            }
                        });
                    }
                }
            }));
        }
        add("trash", Box::new(move |w| {
            if w.tab().is_trash() {
                ops::confirm_delete(&par(w), sel(w), || {});
            } else {
                ops::trash(&par(w), sel(w), || {});
            }
        }));
        add("delete", Box::new(move |w| ops::confirm_delete(&par(w), sel(w), || {})));
        add("restore", Box::new(move |w| ops::run(&par(w), Kind::Restore, sel(w), None, |_| {})));
        add("empty-trash", Box::new(|w| w.empty_trash()));
        add("undo", Box::new(move |w| ops::undo(&par(w), || {})));
        add("select-all", Box::new(|w| w.tab().select_all()));
        add("invert", Box::new(|w| w.tab().invert()));
        add("select-pattern", Box::new(|w| {
            let entry = gtk::Entry::builder().placeholder_text("*.jpg").activates_default(true).build();
            let d = adw::AlertDialog::new(Some(tr("pattern.title")), Some(tr("pattern.body")));
            d.set_extra_child(Some(&entry));
            d.add_responses(&[("cancel", tr("cancel")), ("ok", tr("pattern.select"))]);
            d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
            d.set_default_response(Some("ok"));
            let t = w.tab().weak();
            d.connect_response(Some("ok"), move |_, _| {
                if let Some(t) = t.upgrade() {
                    t.select_pattern(&entry.text());
                }
            });
            d.present(Some(&w.window));
        }));
        add("new-folder", Box::new(|w| w.new_folder()));
        add_s("new-doc", Box::new(|w, p| w.new_document(&p)));
        add("properties", Box::new(move |w| {
            let f = sel(w);
            props::show(&par(w), if f.is_empty() { vec![w.tab().location()] } else { f });
        }));
        add("properties-folder", Box::new(move |w| props::show(&par(w), vec![w.tab().location()])));
        add("terminal", Box::new(|w| {
            if let Some(p) = w.tab().location().path() {
                terminal(&p);
            }
        }));
        add("terminal-selected", Box::new(move |w| {
            if let Some(p) = sel(w).first().and_then(|f| f.path()) {
                terminal(&p);
            }
        }));
        add_s("script", Box::new(move |w, script| {
            let files = sel(w);
            let paths: Vec<String> = files.iter().filter_map(|f| f.path()).map(|p| p.to_string_lossy().into_owned()).collect();
            let uris: Vec<String> = files.iter().map(|f| f.uri().to_string()).collect();
            let loc = w.tab().location();
            let mut c = std::process::Command::new(&script);
            c.args(&paths)
                .env("NEMO_SCRIPT_SELECTED_FILE_PATHS", paths.iter().map(|p| format!("{p}\n")).collect::<String>())
                .env("NEMO_SCRIPT_SELECTED_URIS", uris.iter().map(|p| format!("{p}\n")).collect::<String>())
                .env("NEMO_SCRIPT_CURRENT_URI", loc.uri().as_str());
            if let Some(p) = loc.path() {
                c.current_dir(p);
            }
            if let Err(e) = c.spawn() {
                w.error(tr("menu.scripts"), &e.to_string());
            }
        }));
        add("scripts-folder", Box::new(|w| {
            let d = scripts_dir();
            let _ = std::fs::create_dir_all(&d);
            w.open_in_tab(&gio::File::for_path(d), true);
        }));
        add("connect", Box::new(|w| {
            let entry = gtk::Entry::builder().placeholder_text("smb://serwer/udział, sftp://użytkownik@serwer").activates_default(true).build();
            let d = adw::AlertDialog::new(Some(tr("connect.title")), Some(tr("connect.body")));
            d.set_extra_child(Some(&entry));
            d.add_responses(&[("cancel", tr("cancel")), ("ok", tr("connect.button"))]);
            d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
            d.set_default_response(Some("ok"));
            let me = w.weak();
            d.connect_response(Some("ok"), move |_, _| {
                if let Some(w) = me.upgrade() {
                    let f = util::parse_location(&entry.text());
                    w.tab().navigate(&f);
                }
            });
            d.present(Some(&w.window));
        }));
        add("bookmark", Box::new(|w| {
            bookmarks::add(&w.tab().location());
            w.sidebar.rebuild();
        }));
        add("edit-bookmarks", Box::new(|w| w.edit_bookmarks()));
        add("preferences", Box::new(move |w| {
            let me = w.weak();
            prefs::show(&par(w), Rc::new(move || {
                if let Some(w) = me.upgrade() {
                    w.settings_changed();
                }
            }));
        }));
        add("about", Box::new(|w| {
            let a = adw::AboutDialog::builder()
                .application_name("Filer")
                .application_icon("org.shrimp.Filer")
                .version(env!("CARGO_PKG_VERSION"))
                .developer_name("Shrimp")
                .license_type(gtk::License::Gpl30)
                .comments(tr("about.comments"))
                .build();
            a.present(Some(&w.window));
        }));
        add("zoom-in", Box::new(|w| {
            let t = w.tab();
            t.set_zoom(t.zoom() + 1);
        }));
        add("zoom-out", Box::new(|w| {
            let t = w.tab();
            t.set_zoom(t.zoom() - 1);
        }));
        add("zoom-normal", Box::new(|w| w.tab().set_zoom(settings::get().zoom)));
        let view = gio::SimpleAction::new_stateful("view", Some(glib::VariantTy::STRING), &s.view.to_variant());
        let me = self.weak();
        view.connect_activate(move |a, v| {
            let (Some(w), Some(v)) = (me.upgrade(), v.and_then(|v| v.get::<String>())) else { return };
            a.set_state(&v.to_variant());
            if !w.refreshing.get() {
                w.tab().set_view(&v);
            }
        });
        view.connect_change_state(|a, v| {
            if let Some(v) = v {
                a.set_state(v);
            }
        });
        win.add_action(&view);
        let sort = gio::SimpleAction::new_stateful("sort", Some(glib::VariantTy::STRING), &s.sort.to_variant());
        let me = self.weak();
        sort.connect_activate(move |a, v| {
            let (Some(w), Some(v)) = (me.upgrade(), v.and_then(|v| v.get::<String>())) else { return };
            a.set_state(&v.to_variant());
            settings::update(|s| s.sort = v);
            w.settings_changed();
        });
        win.add_action(&sort);
        add_s("column", Box::new(|w, id| {
            settings::update(|s| {
                if let Some(i) = s.columns.iter().position(|c| *c == id) {
                    s.columns.remove(i);
                } else {
                    s.columns.push(id);
                }
            });
            w.settings_changed();
        }));
        toggle("show-hidden", |s, v| s.show_hidden = v, s.show_hidden);
        toggle("show-menubar", |s, v| s.show_menubar = v, s.show_menubar);
        toggle("show-sidebar", |s, v| s.show_sidebar = v, s.show_sidebar);
        toggle("show-statusbar", |s, v| s.show_statusbar = v, s.show_statusbar);
        toggle("reverse", |s, v| s.sort_reverse = v, s.sort_reverse);
        toggle("folders-first", |s, v| s.folders_first = v, s.folders_first);
    }

    fn connect(&self) {
        // Akcje plików w polach tekstowych zostają przy polu.
        let me = self.weak();
        self.window.connect_focus_widget_notify(move |win| {
            let Some(w) = me.upgrade() else { return };
            let editing = gtk::prelude::GtkWindowExt::focus(win).is_some_and(|f| f.is::<gtk::Text>() || f.is::<gtk::Editable>());
            for a in TEXT_KEYS {
                if let Some(a) = w.window.lookup_action(a).and_downcast::<gio::SimpleAction>() {
                    a.set_enabled(!editing);
                }
            }
        });
        let me = self.weak();
        self.search.connect_search_changed(move |e| {
            if let Some(w) = me.upgrade() {
                w.tab().start_search(&e.text());
            }
        });
        let me = self.weak();
        self.search_bar.connect_search_mode_enabled_notify(move |b| {
            if let Some(w) = me.upgrade() {
                if !b.is_search_mode() {
                    w.tab().stop_search();
                    w.tab_changed(&w.tab());
                }
            }
        });
        let me = self.weak();
        self.zoom.connect_value_changed(move |z| {
            if let Some(w) = me.upgrade() {
                if !w.refreshing.get() {
                    w.tab().set_zoom(z.value() as i32);
                }
            }
        });
        // Ctrl + kółko myszy: powiększenie.
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = self.weak();
        scroll.connect_scroll(move |c, _, dy| {
            if !c.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
                return glib::Propagation::Proceed;
            }
            if let Some(w) = me.upgrade() {
                let t = w.tab();
                t.set_zoom(t.zoom() + if dy < 0.0 { 1 } else { -1 });
            }
            glib::Propagation::Stop
        });
        self.split.add_controller(scroll);
        // Boczne przyciski myszy: wstecz / dalej.
        let mouse = gtk::GestureClick::builder().button(0).build();
        mouse.set_propagation_phase(gtk::PropagationPhase::Capture);
        let me = self.weak();
        mouse.connect_pressed(move |g, _, _, _| {
            let Some(w) = me.upgrade() else { return };
            match g.current_button() {
                8 => w.tab().go_back(),
                9 => w.tab().go_forward(),
                _ => {}
            }
        });
        self.window.add_controller(mouse);
        let me = self.weak();
        self.window.connect_close_request(move |win| {
            let (wd, ht) = (win.width(), win.height());
            let side = me.upgrade().map(|w| w.side_paned.position());
            settings::update(|s| {
                if wd > 0 && ht > 0 && !win.is_maximized() {
                    s.window = (wd, ht);
                }
                if let Some(p) = side.filter(|p| *p > 0) {
                    s.sidebar_width = p;
                }
            });
            glib::Propagation::Proceed
        });
    }
}

/// Skróty klawiszowe jak w Nemo.
pub fn accels(app: &adw::Application) {
    let keys: &[(&str, &[&str])] = &[
        ("win.back", &["<Alt>Left", "BackSpace"]),
        ("win.forward", &["<Alt>Right"]),
        ("win.up", &["<Alt>Up"]),
        ("win.home", &["<Alt>Home"]),
        ("win.reload", &["F5", "<Ctrl>r"]),
        ("win.location", &["<Ctrl>l"]),
        ("win.search", &["<Ctrl>f"]),
        ("win.new-tab", &["<Ctrl>t"]),
        ("win.new-window", &["<Ctrl>n"]),
        ("win.close-tab", &["<Ctrl>w"]),
        ("win.split", &["F3"]),
        ("win.new-folder", &["<Ctrl><Shift>n"]),
        ("win.rename", &["F2"]),
        ("win.cut", &["<Ctrl>x"]),
        ("win.copy", &["<Ctrl>c"]),
        ("win.paste", &["<Ctrl>v"]),
        ("win.trash", &["Delete"]),
        ("win.delete", &["<Shift>Delete"]),
        ("win.undo", &["<Ctrl>z"]),
        ("win.select-all", &["<Ctrl>a"]),
        ("win.invert", &["<Ctrl><Shift>i"]),
        ("win.select-pattern", &["<Ctrl>s"]),
        ("win.make-link", &["<Ctrl>m"]),
        ("win.properties", &["<Alt>Return"]),
        ("win.show-hidden", &["<Ctrl>h"]),
        ("win.show-sidebar", &["F9"]),
        ("win.view::icons", &["<Ctrl>1"]),
        ("win.view::list", &["<Ctrl>2"]),
        ("win.view::compact", &["<Ctrl>3"]),
        ("win.zoom-in", &["<Ctrl>plus", "<Ctrl>equal", "<Ctrl>KP_Add"]),
        ("win.zoom-out", &["<Ctrl>minus", "<Ctrl>KP_Subtract"]),
        ("win.zoom-normal", &["<Ctrl>0", "<Ctrl>KP_0"]),
        ("win.bookmark", &["<Ctrl>d"]),
        ("win.edit-bookmarks", &["<Ctrl>b"]),
        ("win.terminal", &["F4"]),
        ("win.open-tab", &["<Ctrl>Return"]),
        ("window.close", &["<Ctrl><Shift>w"]),
        ("app.quit", &["<Ctrl>q"]),
    ];
    for (a, k) in keys {
        app.set_accels_for_action(a, k);
    }
}
