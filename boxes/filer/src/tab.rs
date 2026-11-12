//! Karta: jeden folder (albo wyniki wyszukiwania) w widoku ikon, listy lub kompaktowym, jak w Nemo.
//! Model: DirectoryList (albo lista wyników) → filtr ukrytych → sortowanie → MultiSelection.
use crate::i18n::{tr, trf, trn};
use crate::window::Win;
use crate::{settings, thumbs, util};
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering as AO};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Rozmiary ikon dla poziomów powiększenia 0…6 (widok ikon); w liście i widoku kompaktowym mniejsze.
const ICON: [i32; 7] = [32, 40, 48, 64, 96, 128, 192];
const SMALL: [i32; 7] = [16, 16, 24, 24, 32, 48, 64];

pub struct Tab {
    pub root: gtk::Box,
    pub page: RefCell<Option<adw::TabPage>>,
    stack: gtk::Stack,
    grid: gtk::GridView,
    list: gtk::ColumnView,
    banner: adw::Banner,
    error: adw::StatusPage,
    dir: gtk::DirectoryList,
    results: gio::ListStore,
    filter: gtk::FilterListModel,
    hidden: gtk::CustomFilter,
    sorter: gtk::CustomSorter,
    pub sel: gtk::MultiSelection,
    location: RefCell<gio::File>,
    back: RefCell<Vec<gio::File>>,
    forward: RefCell<Vec<gio::File>>,
    search: RefCell<Option<(String, Arc<AtomicBool>)>>,
    view: RefCell<String>,
    zoom: Cell<i32>,
    typeahead: RefCell<(String, Instant)>,
    pending: RefCell<Vec<gio::File>>,
    syncing: Cell<bool>,
    me: RefCell<Weak<Tab>>,
    win: Weak<Win>,
}

thread_local! {
    static KEYS: RefCell<HashMap<String, Rc<glib::FilenameCollationKey>>> = RefCell::new(HashMap::new());
    /// Pliki w schowku po „Wytnij” (półprzezroczyste, jak w Nemo).
    pub static CUT: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn key(name: &str) -> Rc<glib::FilenameCollationKey> {
    KEYS.with(|k| {
        let mut k = k.borrow_mut();
        if k.len() > 100_000 {
            k.clear();
        }
        k.entry(name.to_string()).or_insert_with(|| Rc::new(glib::FilenameCollationKey::from(name))).clone()
    })
}

/// Porównanie jak w Nemo: foldery najpierw (opcja), potem wybrana kolumna, przy remisie nazwa.
pub fn compare(a: &gio::FileInfo, b: &gio::FileInfo, s: &settings::Settings) -> Ordering {
    if s.folders_first {
        match (util::is_dir(a), util::is_dir(b)) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
    }
    let name = || key(&a.display_name()).cmp(&key(&b.display_name()));
    let o = match s.sort.as_str() {
        "size" => a.size().cmp(&b.size()).then_with(name),
        "type" => content_desc(a).cmp(&content_desc(b)).then_with(name),
        "modified" => mtime(a).cmp(&mtime(b)).then_with(name),
        _ => name(),
    };
    if s.sort_reverse { o.reverse() } else { o }
}

fn mtime(i: &gio::FileInfo) -> i64 {
    i.attribute_uint64("time::modified") as i64
}

pub fn content_desc(i: &gio::FileInfo) -> String {
    i.content_type().map(|c| gio::content_type_get_description(&c).to_string()).unwrap_or_default()
}

/// Ikona albo miniatura pliku.
fn set_icon(image: &gtk::Image, info: &gio::FileInfo, px: i32) {
    image.set_pixel_size(px);
    let s = settings::get();
    if s.thumbnails && px >= 32 {
        if let Some(p) = thumbs::existing(info) {
            image.set_from_file(Some(&p));
            return;
        }
    }
    match info.icon() {
        Some(i) => image.set_from_gicon(&i),
        None => image.set_icon_name(Some("text-x-generic")),
    }
    if s.thumbnails && px >= 32 && thumbs::wanted(info, s.thumbnail_limit_mb) {
        if let Some(f) = util::file_of(info) {
            let img = image.downgrade();
            let uri = f.uri().to_string();
            image.set_widget_name(&uri);
            thumbs::request(&f, info, move |p| {
                if let (Some(img), Some(p)) = (img.upgrade(), p) {
                    if img.widget_name() == uri {
                        img.set_from_file(Some(&p));
                    }
                }
            });
        }
    }
}

fn is_cut(info: &gio::FileInfo) -> bool {
    let Some(f) = util::file_of(info) else { return false };
    let u = f.uri();
    CUT.with(|c| c.borrow().iter().any(|x| *x == u.as_str()))
}

impl Tab {
    pub fn new(win: &Rc<Win>, location: &gio::File) -> Rc<Tab> {
        let s = settings::get();
        let dir = gtk::DirectoryList::new(Some(util::ATTRS), None::<&gio::File>);
        dir.set_monitored(true);
        let results = gio::ListStore::new::<gio::FileInfo>();
        let hidden = gtk::CustomFilter::new(|o| {
            let i = o.downcast_ref::<gio::FileInfo>().unwrap();
            settings::get().show_hidden || !(i.is_hidden() || i.is_backup())
        });
        let filter = gtk::FilterListModel::new(Some(dir.clone()), Some(hidden.clone()));
        let sorter = gtk::CustomSorter::new(|a, b| {
            compare(a.downcast_ref().unwrap(), b.downcast_ref().unwrap(), &settings::get()).into()
        });
        let sorted = gtk::SortListModel::new(Some(filter.clone()), Some(sorter.clone()));
        let sel = gtk::MultiSelection::new(Some(sorted));
        let grid = gtk::GridView::builder().model(&sel).enable_rubberband(true).max_columns(64).build();
        grid.add_css_class("filer-grid");
        let list = gtk::ColumnView::builder().model(&sel).enable_rubberband(true).show_column_separators(false).reorderable(false).build();
        list.add_css_class("filer-list");
        let stack = gtk::Stack::new();
        let scroll = |w: &gtk::Widget| gtk::ScrolledWindow::builder().child(w).vexpand(true).hexpand(true).build();
        stack.add_named(&scroll(grid.upcast_ref()), Some("grid"));
        stack.add_named(&scroll(list.upcast_ref()), Some("list"));
        let error = adw::StatusPage::builder().icon_name("dialog-warning-symbolic").build();
        stack.add_named(&error, Some("error"));
        let banner = adw::Banner::new("");
        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&banner);
        root.append(&stack);
        let tab = Rc::new(Tab {
            root,
            page: RefCell::new(None),
            stack,
            grid,
            list,
            banner,
            error,
            dir,
            results,
            filter,
            hidden,
            sorter,
            sel,
            location: RefCell::new(location.clone()),
            back: RefCell::new(vec![]),
            forward: RefCell::new(vec![]),
            search: RefCell::new(None),
            view: RefCell::new(s.view.clone()),
            zoom: Cell::new(s.zoom),
            typeahead: RefCell::new((String::new(), Instant::now())),
            pending: RefCell::new(vec![]),
            syncing: Cell::new(false),
            me: RefCell::new(Weak::new()),
            win: Rc::downgrade(win),
        });
        *tab.me.borrow_mut() = Rc::downgrade(&tab);
        tab.build_columns();
        tab.connect();
        tab.apply_view();
        tab.load(location);
        tab
    }

    pub fn location(&self) -> gio::File {
        self.location.borrow().clone()
    }

    pub fn is_trash(&self) -> bool {
        self.location().uri_scheme().as_deref() == Some("trash")
    }

    pub fn searching(&self) -> Option<String> {
        self.search.borrow().as_ref().map(|(q, _)| q.clone())
    }

    pub fn can_back(&self) -> bool {
        !self.back.borrow().is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.borrow().is_empty()
    }

    pub fn view(&self) -> String {
        self.view.borrow().clone()
    }

    pub fn zoom(&self) -> i32 {
        self.zoom.get()
    }

    pub fn is_loading(&self) -> bool {
        self.search.borrow().is_none() && self.dir.is_loading()
    }

    fn notify(&self) {
        if let Some(w) = self.win.upgrade() {
            w.tab_changed(self);
        }
    }

    // ---------- nawigacja ----------

    pub fn navigate(&self, f: &gio::File) {
        if f.equal(&self.location()) && self.search.borrow().is_none() {
            return;
        }
        self.back.borrow_mut().push(self.location());
        self.forward.borrow_mut().clear();
        self.load(f);
    }

    pub fn go_back(&self) {
        let Some(f) = self.back.borrow_mut().pop() else { return };
        self.forward.borrow_mut().push(self.location());
        self.load(&f);
    }

    pub fn go_forward(&self) {
        let Some(f) = self.forward.borrow_mut().pop() else { return };
        self.back.borrow_mut().push(self.location());
        self.load(&f);
    }

    pub fn go_up(&self) {
        let loc = self.location();
        if let Some(p) = loc.parent() {
            self.navigate(&p);
            // Folder, z którego wyszliśmy, zostaje zaznaczony (Nemo).
            self.pending.borrow_mut().push(loc);
        }
    }

    pub fn reload(&self) {
        let f = self.location();
        self.load(&f);
    }

    fn load(&self, f: &gio::File) {
        self.stop_search();
        *self.location.borrow_mut() = f.clone();
        self.filter.set_model(Some(&self.dir));
        self.dir.set_file(None::<&gio::File>);
        self.dir.set_file(Some(f));
        self.show_view();
        if self.is_trash() {
            self.banner.set_title(tr("trash.banner"));
            self.banner.set_button_label(Some(tr("trash.empty")));
            self.banner.set_revealed(true);
        } else {
            self.banner.set_revealed(false);
        }
        self.notify();
    }

    fn show_view(&self) {
        self.stack.set_visible_child_name(if self.view.borrow().as_str() == "list" { "list" } else { "grid" });
    }

    fn on_error(&self, e: &glib::Error) {
        let Some(w) = self.win.upgrade() else { return };
        if e.matches(gio::IOErrorEnum::NotMounted) {
            // Udział sieciowy, pendrive bez montowania: najpierw montujemy (z pytaniem o hasło).
            let op = gtk::MountOperation::new(Some(&w.window));
            let f = self.location();
            let me = self.weak();
            f.mount_enclosing_volume(gio::MountMountFlags::NONE, Some(&op), gio::Cancellable::NONE, move |r| {
                if let Some(t) = me.upgrade() {
                    match r {
                        Ok(()) => t.reload(),
                        Err(e) => t.show_error(&e),
                    }
                }
            });
            return;
        }
        self.show_error(e);
    }

    fn show_error(&self, e: &glib::Error) {
        self.error.set_title(&trf("view.error_title", &[("name", &util::display_name(&self.location()))]));
        self.error.set_description(Some(e.message()));
        self.stack.set_visible_child_name("error");
    }

    // ---------- wyszukiwanie ----------

    /// Szuka w bieżącym folderze i podfolderach (nazwa zawiera tekst, bez wielkości liter).
    pub fn start_search(&self, query: &str) {
        self.stop_search();
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            self.filter.set_model(Some(&self.dir));
            self.notify();
            return;
        }
        let Some(root) = self.location().path() else { return };
        let stop = Arc::new(AtomicBool::new(false));
        *self.search.borrow_mut() = Some((query.to_string(), stop.clone()));
        self.results.remove_all();
        self.filter.set_model(Some(&self.results));
        self.show_view();
        let (tx, rx) = async_channel::unbounded::<Vec<std::path::PathBuf>>();
        let hidden = settings::get().show_hidden;
        std::thread::spawn(move || {
            let mut batch = vec![];
            let mut last = Instant::now();
            let mut stack = vec![root];
            while let Some(d) = stack.pop() {
                if stop.load(AO::Relaxed) {
                    return;
                }
                let Ok(rd) = std::fs::read_dir(&d) else { continue };
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_lowercase();
                    if !hidden && name.starts_with('.') {
                        continue;
                    }
                    let p = e.path();
                    if e.file_type().is_ok_and(|t| t.is_dir()) {
                        stack.push(p.clone());
                    }
                    if name.contains(&q) {
                        batch.push(p);
                    }
                }
                if !batch.is_empty() && last.elapsed() > Duration::from_millis(150) {
                    last = Instant::now();
                    if tx.send_blocking(std::mem::take(&mut batch)).is_err() {
                        return;
                    }
                }
            }
            let _ = tx.send_blocking(batch);
        });
        let results = self.results.clone();
        glib::spawn_future_local(async move {
            while let Ok(v) = rx.recv().await {
                let infos: Vec<gio::FileInfo> = v
                    .iter()
                    .filter_map(|p| {
                        let f = gio::File::for_path(p);
                        let i = f.query_info(util::ATTRS, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok()?;
                        i.set_attribute_object("standard::file", &f);
                        Some(i)
                    })
                    .collect();
                results.extend_from_slice(&infos);
            }
        });
        self.notify();
    }

    pub fn stop_search(&self) {
        if let Some((_, stop)) = self.search.borrow_mut().take() {
            stop.store(true, AO::Relaxed);
            self.filter.set_model(Some(&self.dir));
        }
    }

    // ---------- zaznaczenie ----------

    pub fn infos(&self) -> Vec<gio::FileInfo> {
        let set = self.sel.selection();
        (0..set.size()).filter_map(|i| self.sel.item(set.nth(i as u32))).filter_map(|o| o.downcast::<gio::FileInfo>().ok()).collect()
    }

    pub fn selected(&self) -> Vec<gio::File> {
        self.infos().iter().filter_map(util::file_of).collect()
    }

    pub fn n_items(&self) -> u32 {
        self.sel.n_items()
    }

    pub fn all_infos(&self) -> Vec<gio::FileInfo> {
        (0..self.sel.n_items()).filter_map(|i| self.sel.item(i)).filter_map(|o| o.downcast().ok()).collect()
    }

    pub fn select_all(&self) {
        self.sel.select_all();
    }

    pub fn invert(&self) {
        let set = self.sel.selection();
        let all = gtk::Bitset::new_range(0, self.sel.n_items());
        all.subtract(&set);
        self.sel.set_selection(&all, &gtk::Bitset::new_range(0, self.sel.n_items()));
    }

    /// Zaznacza pliki pasujące do wzorca („*.jpg”, „raport?.txt”).
    pub fn select_pattern(&self, pattern: &str) {
        let set = gtk::Bitset::new_empty();
        for (i, info) in self.all_infos().iter().enumerate() {
            if glob(pattern, &info.display_name()) {
                set.add(i as u32);
            }
        }
        self.sel.set_selection(&set, &gtk::Bitset::new_range(0, self.sel.n_items()));
    }

    /// Zaznacza te pliki, gdy pojawią się w widoku (po kopiowaniu, nowym folderze itd.).
    pub fn select_when_ready(&self, files: Vec<gio::File>) {
        *self.pending.borrow_mut() = files;
        self.apply_pending();
    }

    fn apply_pending(&self) {
        if self.pending.borrow().is_empty() {
            return;
        }
        let pending = self.pending.borrow().clone();
        let set = gtk::Bitset::new_empty();
        let mut first = None;
        for (i, info) in self.all_infos().iter().enumerate() {
            if let Some(f) = util::file_of(info) {
                if pending.iter().any(|p| p.equal(&f)) {
                    set.add(i as u32);
                    first.get_or_insert(i as u32);
                }
            }
        }
        if set.size() as usize == pending.len() {
            self.pending.borrow_mut().clear();
        }
        if let Some(pos) = first {
            self.sel.set_selection(&set, &gtk::Bitset::new_range(0, self.sel.n_items()));
            self.scroll_to(pos);
        }
    }

    fn scroll_to(&self, pos: u32) {
        if self.view.borrow().as_str() == "list" {
            self.list.scroll_to(pos, None, gtk::ListScrollFlags::FOCUS, None);
        } else {
            self.grid.scroll_to(pos, gtk::ListScrollFlags::FOCUS, None);
        }
    }

    /// Widżet pliku w widoku (do zmiany nazwy w miejscu).
    pub fn widget_for(&self, f: &gio::File) -> Option<gtk::Widget> {
        let root: gtk::Widget = if self.view.borrow().as_str() == "list" { self.list.clone().upcast() } else { self.grid.clone().upcast() };
        let uri = f.uri();
        let mut stack = vec![root];
        while let Some(w) = stack.pop() {
            if w.widget_name() == uri.as_str() && w.is_mapped() && w.is::<gtk::Box>() {
                return Some(w);
            }
            let mut c = w.first_child();
            while let Some(x) = c {
                c = x.next_sibling();
                stack.push(x);
            }
        }
        None
    }

    pub fn focus_view(&self) {
        if self.view.borrow().as_str() == "list" {
            self.list.grab_focus();
        } else {
            self.grid.grab_focus();
        }
    }

    // ---------- wygląd ----------

    pub fn set_view(&self, v: &str) {
        *self.view.borrow_mut() = v.to_string();
        self.apply_view();
        self.show_view();
        self.notify();
    }

    pub fn set_zoom(&self, z: i32) {
        self.zoom.set(z.clamp(0, 6));
        self.apply_view();
        self.notify();
    }

    pub fn set_single_click(&self, on: bool) {
        self.grid.set_single_click_activate(on);
        self.list.set_single_click_activate(on);
    }

    pub fn refilter(&self) {
        self.hidden.changed(gtk::FilterChange::Different);
    }

    pub fn resort(&self) {
        self.sorter.changed(gtk::SorterChange::Different);
        self.sync_column_sort();
    }

    /// Odświeża wiersze (np. po „Wytnij”, zmianie miniatur).
    pub fn redraw(&self) {
        let v = self.view();
        *self.view.borrow_mut() = String::new();
        self.set_view(&v);
    }

    fn apply_view(&self) {
        let v = self.view.borrow().clone();
        let z = self.zoom.get() as usize;
        match v.as_str() {
            "list" => {}
            "compact" => {
                self.grid.set_factory(Some(&self.grid_factory(true, SMALL[z])));
                self.grid.set_min_columns(1);
            }
            _ => {
                self.grid.set_factory(Some(&self.grid_factory(false, ICON[z])));
                self.grid.set_min_columns(1);
            }
        }
        if v == "list" {
            self.build_columns();
        }
    }

    // ---------- fabryki widoków ----------

    fn grid_factory(&self, compact: bool, px: i32) -> gtk::SignalListItemFactory {
        let f = gtk::SignalListItemFactory::new();
        let me = self.weak();
        f.connect_setup(move |_, li| {
            let li = li.downcast_ref::<gtk::ListItem>().unwrap();
            let image = gtk::Image::new();
            let emblem = gtk::Image::builder().icon_name("emblem-symbolic-link").pixel_size((px / 3).max(12)).halign(gtk::Align::End).valign(gtk::Align::End).visible(false).build();
            let overlay = gtk::Overlay::builder().child(&image).build();
            overlay.add_overlay(&emblem);
            let label = gtk::Label::builder().ellipsize(gtk::pango::EllipsizeMode::End).build();
            let b = if compact {
                label.set_xalign(0.0);
                label.set_width_chars(18);
                label.set_max_width_chars(28);
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                b.set_margin_start(4);
                b.set_margin_end(4);
                b
            } else {
                label.set_wrap(true);
                label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                label.set_lines(3);
                label.set_justify(gtk::Justification::Center);
                label.set_max_width_chars(((px + 40) / 8).max(10));
                label.set_width_chars(((px + 40) / 8).max(10));
                let b = gtk::Box::new(gtk::Orientation::Vertical, 4);
                b.set_margin_top(4);
                b.set_margin_bottom(4);
                b
            };
            b.append(&overlay);
            b.append(&label);
            li.set_child(Some(&b));
            if let Some(t) = me.upgrade() {
                t.item_controllers(b.upcast_ref(), li);
            }
        });
        f.connect_bind(move |_, li| {
            let li = li.downcast_ref::<gtk::ListItem>().unwrap();
            let info = li.item().and_downcast::<gio::FileInfo>().unwrap();
            let b = li.child().unwrap();
            let overlay = b.first_child().and_downcast::<gtk::Overlay>().unwrap();
            let image = overlay.child().and_downcast::<gtk::Image>().unwrap();
            let emblem = overlay.last_child().and_downcast::<gtk::Image>().unwrap();
            let label = b.last_child().and_downcast::<gtk::Label>().unwrap();
            set_icon(&image, &info, px);
            emblem.set_visible(info.is_symlink());
            label.set_label(&info.display_name());
            let name = info.display_name();
            b.set_tooltip_text(if compact { Some(name.as_str()) } else { None });
            b.set_opacity(if is_cut(&info) || info.is_hidden() { 0.55 } else { 1.0 });
            if let Some(f) = util::file_of(&info) {
                b.set_widget_name(&f.uri());
            }
        });
        f
    }

    fn build_columns(&self) {
        while let Some(c) = self.list.columns().item(0).and_downcast::<gtk::ColumnViewColumn>() {
            self.list.remove_column(&c);
        }
        let s = settings::get();
        let px = SMALL[self.zoom.get() as usize];
        let me = self.weak();
        let text = |id: &str, title: &str, f: fn(&gio::FileInfo) -> String, expand: bool, right: bool| {
            let fac = gtk::SignalListItemFactory::new();
            fac.connect_setup(move |_, li| {
                let l = gtk::Label::builder().xalign(if right { 1.0 } else { 0.0 }).ellipsize(gtk::pango::EllipsizeMode::End).css_classes(["dim-label", "numeric"]).build();
                li.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&l));
            });
            fac.connect_bind(move |_, li| {
                let li = li.downcast_ref::<gtk::ListItem>().unwrap();
                let info = li.item().and_downcast::<gio::FileInfo>().unwrap();
                li.child().and_downcast::<gtk::Label>().unwrap().set_label(&f(&info));
            });
            let c = gtk::ColumnViewColumn::builder().title(title).factory(&fac).expand(expand).resizable(true).id(id).build();
            c.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
            c
        };
        // Nazwa
        let fac = gtk::SignalListItemFactory::new();
        {
            let me = me.clone();
            fac.connect_setup(move |_, li| {
                let li = li.downcast_ref::<gtk::ListItem>().unwrap();
                let b = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let image = gtk::Image::new();
                let emblem = gtk::Image::builder().icon_name("emblem-symbolic-link").pixel_size(10).halign(gtk::Align::End).valign(gtk::Align::End).visible(false).build();
                let overlay = gtk::Overlay::builder().child(&image).build();
                overlay.add_overlay(&emblem);
                b.append(&overlay);
                b.append(&gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
                li.set_child(Some(&b));
                if let Some(t) = me.upgrade() {
                    t.item_controllers(b.upcast_ref(), li);
                }
            });
        }
        {
            fac.connect_bind(move |_, li| {
                let li = li.downcast_ref::<gtk::ListItem>().unwrap();
                let info = li.item().and_downcast::<gio::FileInfo>().unwrap();
                let b = li.child().unwrap();
                let overlay = b.first_child().and_downcast::<gtk::Overlay>().unwrap();
                set_icon(&overlay.child().and_downcast::<gtk::Image>().unwrap(), &info, px);
                overlay.last_child().unwrap().set_visible(info.is_symlink());
                b.last_child().and_downcast::<gtk::Label>().unwrap().set_label(&info.display_name());
                b.set_opacity(if is_cut(&info) || info.is_hidden() { 0.55 } else { 1.0 });
                if let Some(f) = util::file_of(&info) {
                    b.set_widget_name(&f.uri());
                }
            });
        }
        let name = gtk::ColumnViewColumn::builder().title(tr("col.name")).factory(&fac).expand(true).resizable(true).id("name").build();
        name.set_sorter(Some(&gtk::CustomSorter::new(|_, _| gtk::Ordering::Equal)));
        self.list.append_column(&name);
        let all: [Column; 7] = [
            ("size", tr("col.size"), |i| if util::is_dir(i) { String::new() } else { util::size(i.size() as u64) }, true),
            ("type", tr("col.type"), content_desc, false),
            ("modified", tr("col.modified"), |i| i.modification_date_time().map(|d| util::date(&d)).unwrap_or_default(), false),
            ("accessed", tr("col.accessed"), |i| util::access_time(i).map(|d| util::date(&d)).unwrap_or_default(), false),
            ("owner", tr("col.owner"), |i| i.attribute_string("owner::user").map(|s| s.to_string()).unwrap_or_default(), false),
            ("permissions", tr("col.permissions"), |i| util::mode_string(i.attribute_uint32("unix::mode")), false),
            ("location", tr("col.location"), |i| util::file_of(i).and_then(|f| f.parent()).map(|p| p.parse_name().to_string()).unwrap_or_default(), false),
        ];
        let menu = gio::Menu::new();
        for (id, title, f, right) in all {
            menu.append(Some(title), Some(&format!("win.column::{id}")));
            if s.columns.iter().any(|c| c == id) {
                let c = text(id, title, f, id == "location", right);
                self.list.append_column(&c);
            }
        }
        for i in 0..self.list.columns().n_items() {
            if let Some(c) = self.list.columns().item(i).and_downcast::<gtk::ColumnViewColumn>() {
                c.set_header_menu(Some(&menu));
            }
        }
        self.sync_column_sort();
    }

    fn sync_column_sort(&self) {
        let s = settings::get();
        let cols = self.list.columns();
        let col = (0..cols.n_items()).filter_map(|i| cols.item(i).and_downcast::<gtk::ColumnViewColumn>()).find(|c| c.id().as_deref() == Some(s.sort.as_str()));
        self.syncing.set(true);
        self.list.sort_by_column(col.as_ref(), if s.sort_reverse { gtk::SortType::Descending } else { gtk::SortType::Ascending });
        self.syncing.set(false);
    }

    pub fn weak(&self) -> Weak<Tab> {
        self.me.borrow().clone()
    }

    /// Prawy przycisk, przeciąganie i upuszczanie na element widoku.
    fn item_controllers(&self, w: &gtk::Widget, li: &gtk::ListItem) {
        let me = self.weak();
        let click = gtk::GestureClick::builder().button(0).build();
        {
            let li = li.downgrade();
            click.connect_pressed(move |g, n, x, y| {
                let (Some(t), Some(li)) = (me.upgrade(), li.upgrade()) else { return };
                let pos = li.position();
                match g.current_button() {
                    3 => {
                        g.set_state(gtk::EventSequenceState::Claimed);
                        if !t.sel.is_selected(pos) {
                            t.sel.select_item(pos, true);
                        }
                        if let (Some(win), Some(w)) = (t.win.upgrade(), g.widget()) {
                            win.context_menu(&t, &w, x, y, true);
                        }
                    }
                    2 if n == 1 => {
                        g.set_state(gtk::EventSequenceState::Claimed);
                        let info = li.item().and_downcast::<gio::FileInfo>();
                        if let (Some(win), Some(info)) = (t.win.upgrade(), info) {
                            if util::is_dir(&info) {
                                if let Some(f) = util::file_of(&info) {
                                    win.open_in_tab(&f, false);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            });
        }
        w.add_controller(click);
        // Przeciąganie zaznaczonych plików.
        let drag = gtk::DragSource::builder().actions(gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK).build();
        {
            let me = self.weak();
            let li = li.downgrade();
            drag.connect_prepare(move |d, _, _| {
                let (t, li) = (me.upgrade()?, li.upgrade()?);
                if !t.sel.is_selected(li.position()) {
                    t.sel.select_item(li.position(), true);
                }
                let files = t.selected();
                if let Some(info) = li.item().and_downcast::<gio::FileInfo>() {
                    if let Some(icon) = info.icon() {
                        let theme = gtk::IconTheme::for_display(&d.widget()?.display());
                        let p = theme.lookup_by_gicon(&icon, 48, 1, gtk::TextDirection::None, gtk::IconLookupFlags::empty());
                        d.set_icon(Some(&p), 0, 0);
                    }
                }
                Some(gdk::ContentProvider::for_value(&gdk::FileList::from_array(&files).to_value()))
            });
        }
        w.add_controller(drag);
        // Upuszczanie na folder.
        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK);
        {
            let li = li.downgrade();
            drop.connect_accept(move |_, _| {
                li.upgrade().and_then(|l| l.item().and_downcast::<gio::FileInfo>()).is_some_and(|i| util::is_dir(&i))
            });
        }
        {
            let me = self.weak();
            let li = li.downgrade();
            drop.connect_drop(move |d, v, _, _| {
                let (Some(t), Some(li)) = (me.upgrade(), li.upgrade()) else { return false };
                let Some(dest) = li.item().and_downcast::<gio::FileInfo>().filter(util::is_dir).and_then(|i| util::file_of(&i)) else { return false };
                let Ok(list) = v.get::<gdk::FileList>() else { return false };
                if let Some(w) = t.win.upgrade() {
                    w.drop_files(list.files(), &dest, d.current_drop().map(|x| x.actions()).unwrap_or(gdk::DragAction::COPY));
                }
                true
            });
        }
        w.add_controller(drop);
    }

    fn connect(self: &Rc<Self>) {
        let me = Rc::downgrade(self);
        self.dir.connect_loading_notify(move |d| {
            if let Some(t) = me.upgrade() {
                if !d.is_loading() {
                    if let Some(e) = d.error() {
                        t.on_error(&e);
                    }
                }
                t.notify();
            }
        });
        let me = Rc::downgrade(self);
        self.dir.connect_error_notify(move |d| {
            if let (Some(t), Some(e)) = (me.upgrade(), d.error()) {
                t.on_error(&e);
            }
        });
        let me = Rc::downgrade(self);
        self.sel.connect_items_changed(move |_, _, _, _| {
            if let Some(t) = me.upgrade() {
                t.apply_pending();
                t.notify();
            }
        });
        let me = Rc::downgrade(self);
        self.sel.connect_selection_changed(move |_, _, _| {
            if let Some(t) = me.upgrade() {
                t.notify();
            }
        });
        for v in [self.grid.upcast_ref::<gtk::Widget>(), self.list.upcast_ref()] {
            // Otwieranie (dwuklik / Enter / pojedyncze kliknięcie wg ustawień)
            let me = Rc::downgrade(self);
            let activate = move |pos: u32| {
                let Some(t) = me.upgrade() else { return };
                let Some(win) = t.win.upgrade() else { return };
                let sel = t.infos();
                if sel.len() > 1 {
                    win.open_selection(&t, false);
                } else if let Some(info) = t.sel.item(pos).and_downcast::<gio::FileInfo>() {
                    win.open_info(&t, &info);
                }
            };
            if let Some(g) = v.downcast_ref::<gtk::GridView>() {
                g.connect_activate(move |_, p| activate(p));
            } else if let Some(l) = v.downcast_ref::<gtk::ColumnView>() {
                l.connect_activate(move |_, p| activate(p));
            }
            // Prawy przycisk na tle
            let click = gtk::GestureClick::builder().button(3).build();
            let me = Rc::downgrade(self);
            click.connect_pressed(move |g, _, x, y| {
                let Some(t) = me.upgrade() else { return };
                t.sel.unselect_all();
                if let (Some(win), Some(w)) = (t.win.upgrade(), g.widget()) {
                    win.context_menu(&t, &w, x, y, false);
                }
            });
            v.add_controller(click);
            // Kliknięcie lewym w puste tło odznacza.
            let bg = gtk::GestureClick::builder().button(1).build();
            let me = Rc::downgrade(self);
            bg.connect_released(move |g, _, x, y| {
                let Some(t) = me.upgrade() else { return };
                if let Some(w) = g.widget() {
                    let hit = w.pick(x, y, gtk::PickFlags::DEFAULT);
                    let on_bg = hit.as_ref().is_none_or(|h| h == &w || h.css_name() == "gridview" || h.css_name() == "listview");
                    if on_bg {
                        t.sel.unselect_all();
                    }
                }
            });
            v.add_controller(bg);
            // Upuszczanie na tło: do bieżącego folderu.
            let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY | gdk::DragAction::MOVE | gdk::DragAction::LINK);
            let me = Rc::downgrade(self);
            drop.connect_drop(move |d, v, _, _| {
                let (Some(t), Ok(list)) = (me.upgrade(), v.get::<gdk::FileList>()) else { return false };
                if let Some(w) = t.win.upgrade() {
                    w.drop_files(list.files(), &t.location(), d.current_drop().map(|x| x.actions()).unwrap_or(gdk::DragAction::COPY));
                }
                true
            });
            v.add_controller(drop);
            // Szukanie przez pisanie: zaznacza pierwszy plik zaczynający się od wpisanych liter.
            let keys = gtk::EventControllerKey::new();
            let me = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, k, _, m| {
                let Some(t) = me.upgrade() else { return glib::Propagation::Proceed };
                if m.intersects(gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK) {
                    return glib::Propagation::Proceed;
                }
                let Some(c) = k.to_unicode().filter(|c| !c.is_control() && (*c != ' ' || !t.typeahead.borrow().0.is_empty())) else {
                    return glib::Propagation::Proceed;
                };
                t.type_ahead(c);
                glib::Propagation::Stop
            });
            v.add_controller(keys);
            let focus = gtk::EventControllerFocus::new();
            let me = Rc::downgrade(self);
            focus.connect_enter(move |_| {
                if let (Some(t), Some(w)) = (me.upgrade(), me.upgrade().and_then(|t| t.win.upgrade())) {
                    w.focus_tab(&t);
                }
            });
            v.add_controller(focus);
        }
        // Sortowanie klikiem w nagłówek kolumny.
        let me = Rc::downgrade(self);
        if let Some(s) = self.list.sorter() {
            s.connect_changed(move |s, _| {
                let Some(t) = me.upgrade() else { return };
                if t.syncing.get() {
                    return;
                }
                let Some(cs) = s.downcast_ref::<gtk::ColumnViewSorter>() else { return };
                if let Some(id) = cs.primary_sort_column().and_then(|c| c.id()) {
                    let rev = cs.primary_sort_order() == gtk::SortType::Descending;
                    let id = if ["name", "size", "type", "modified"].contains(&id.as_str()) { id.to_string() } else { "name".into() };
                    settings::update(|s| {
                        s.sort = id;
                        s.sort_reverse = rev;
                    });
                    if let Some(w) = t.win.upgrade() {
                        w.settings_changed();
                    }
                }
            });
        }
        let me = Rc::downgrade(self);
        self.banner.connect_button_clicked(move |_| {
            if let Some(w) = me.upgrade().and_then(|t| t.win.upgrade()) {
                w.empty_trash();
            }
        });
    }

    fn type_ahead(&self, c: char) {
        {
            let mut ta = self.typeahead.borrow_mut();
            if ta.1.elapsed() > Duration::from_millis(1200) {
                ta.0.clear();
            }
            ta.0.push(c);
            ta.1 = Instant::now();
        }
        let prefix = self.typeahead.borrow().0.to_lowercase();
        if let Some(pos) = self.all_infos().iter().position(|i| i.display_name().to_lowercase().starts_with(&prefix)) {
            self.sel.select_item(pos as u32, true);
            self.scroll_to(pos as u32);
        }
    }

    /// Tekst paska stanu jak w Nemo.
    pub fn status(&self) -> String {
        let sel = self.infos();
        let free = self
            .location()
            .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
            .ok()
            .filter(|i| i.has_attribute("filesystem::free"))
            .map(|i| util::size(i.attribute_uint64("filesystem::free")));
        match sel.len() {
            0 => {
                let n = trn("status.items", self.n_items() as u64, &[]);
                match free {
                    Some(f) => trf("status.free", &[("items", &n), ("free", &f)]),
                    None => n,
                }
            }
            1 => {
                let i = &sel[0];
                if util::is_dir(i) {
                    trf("status.one_folder", &[("name", &i.display_name())])
                } else {
                    trf("status.one", &[("name", &i.display_name()), ("size", &util::size(i.size() as u64))])
                }
            }
            n => {
                let (dirs, files): (Vec<_>, Vec<_>) = sel.iter().partition(|i| util::is_dir(i));
                let size: u64 = files.iter().map(|i| i.size() as u64).sum();
                let mut parts = vec![];
                if !dirs.is_empty() {
                    parts.push(trn("status.folders", dirs.len() as u64, &[]));
                }
                if !files.is_empty() {
                    parts.push(trn("status.files", files.len() as u64, &[("size", &util::size(size))]));
                }
                let _ = n;
                parts.join(", ")
            }
        }
    }
}

/// Kolumna listy: id, tytuł, tekst komórki, wyrównanie do prawej.
type Column = (&'static str, &'static str, fn(&gio::FileInfo) -> String, bool);

/// Dopasowanie wzorca z „*” i „?” (bez wielkości liter), jak „Zaznacz według wzorca” w Nemo.
pub fn glob(pattern: &str, name: &str) -> bool {
    fn m(p: &[char], n: &[char]) -> bool {
        match (p.first(), n.first()) {
            (None, None) => true,
            (Some('*'), _) => m(&p[1..], n) || (!n.is_empty() && m(p, &n[1..])),
            (Some('?'), Some(_)) => m(&p[1..], &n[1..]),
            (Some(a), Some(b)) => a == b && m(&p[1..], &n[1..]),
            _ => false,
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    m(&p, &n)
}

#[cfg(test)]
mod tests {
    use super::glob;
    #[test]
    fn patterns() {
        assert!(glob("*.JPG", "wakacje.jpg"));
        assert!(glob("raport?.txt", "raport2.txt"));
        assert!(!glob("raport?.txt", "raport12.txt"));
        assert!(glob("*", ""));
    }
}
