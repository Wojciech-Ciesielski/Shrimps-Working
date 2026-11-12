//! Operacje na plikach jak w Nemo: kopiowanie, przenoszenie, dowiązania, kosz, usuwanie, przywracanie z kosza.
//! Praca idzie w osobnym wątku (GIO synchronicznie); okno „Operacje na plikach” pokazuje postęp
//! (po 0,5 s), pytania o konflikty („Zastąp / Scal / Pomiń / Zmień nazwę”, „dla wszystkich”) i błędy.
//! Każda udana operacja zostawia wpis do cofnięcia (Ctrl+Z).
use crate::i18n::{tr, trf, trn};
use crate::util;
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Copy,
    Move,
    Link,
    Trash,
    Delete,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Answer {
    Replace,
    Merge,
    Skip,
    Rename,
    Cancel,
}

/// Wpis do cofnięcia.
#[derive(Debug, Clone)]
pub enum Undo {
    /// Nowe pliki (kopie, dowiązania, nowy folder): cofnięcie = do kosza.
    Created(Vec<gio::File>),
    /// (gdzie jest teraz, skąd przyszedł)
    Moved(Vec<(gio::File, gio::File)>),
    /// (plik po zmianie, poprzednia nazwa)
    Renamed(gio::File, String),
    /// Pierwotne miejsca plików w koszu.
    Trashed(Vec<gio::File>),
}

enum Msg {
    Total(u64, u64),
    Progress { bytes: u64, files: u64, name: String },
    Ask { name: String, dir: bool, reply: mpsc::Sender<(Answer, bool)> },
    Error { text: String, reply: mpsc::Sender<(bool, bool)> },
    Done { undo: Option<Undo>, created: Vec<gio::File> },
}

thread_local! {
    static UNDO: RefCell<Vec<Undo>> = const { RefCell::new(Vec::new()) };
    static JOBS: RefCell<Option<Jobs>> = const { RefCell::new(None) };
}

pub fn push_undo(u: Undo) {
    UNDO.with(|s| s.borrow_mut().push(u));
}

// ---------- praca w wątku ----------

struct Worker {
    tx: async_channel::Sender<Msg>,
    cancel: gio::Cancellable,
    total: u64,
    bytes: u64,
    files: u64,
    last: Instant,
    all_conflicts: Option<Answer>,
    all_errors: Option<bool>,
}

#[derive(Debug)]
struct Stop;

const NOFOLLOW: gio::FileQueryInfoFlags = gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS;

impl Worker {
    fn progress(&mut self, name: &str, force: bool) {
        if force || self.last.elapsed() > Duration::from_millis(100) {
            self.last = Instant::now();
            let _ = self.tx.send_blocking(Msg::Progress { bytes: self.bytes, files: self.files, name: name.into() });
        }
    }

    fn check(&self) -> Result<(), Stop> {
        if self.cancel.is_cancelled() { Err(Stop) } else { Ok(()) }
    }

    fn ask(&mut self, f: &gio::File, dir: bool) -> Result<Answer, Stop> {
        if let Some(a) = self.all_conflicts {
            // „Scal” dla wszystkich dotyczy folderów; pliki wtedy zastępujemy (jak w Nemo).
            return Ok(match (a, dir) {
                (Answer::Merge, false) => Answer::Replace,
                (Answer::Replace, true) => Answer::Merge,
                (a, _) => a,
            });
        }
        let (tx, rx) = mpsc::channel();
        let name = f.basename().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
        let _ = self.tx.send_blocking(Msg::Ask { name, dir, reply: tx });
        let (a, all) = rx.recv().unwrap_or((Answer::Cancel, false));
        if a == Answer::Cancel {
            self.cancel.cancel();
            return Err(Stop);
        }
        if all {
            self.all_conflicts = Some(a);
        }
        Ok(a)
    }

    /// Błąd jednego pliku: pominąć (Ok) czy przerwać (Err).
    fn error(&mut self, e: &glib::Error, f: &gio::File) -> Result<(), Stop> {
        if e.matches(gio::IOErrorEnum::Cancelled) {
            return Err(Stop);
        }
        if let Some(skip) = self.all_errors {
            return if skip { Ok(()) } else { Err(Stop) };
        }
        let (tx, rx) = mpsc::channel();
        let text = trf("ops.error", &[("name", &f.parse_name()), ("error", e.message())]);
        let _ = self.tx.send_blocking(Msg::Error { text, reply: tx });
        let (skip, all) = rx.recv().unwrap_or((false, false));
        if all {
            self.all_errors = Some(skip);
        }
        if skip { Ok(()) } else { self.cancel.cancel(); Err(Stop) }
    }

    fn count(&self, f: &gio::File) -> (u64, u64) {
        let Ok(i) = f.query_info("standard::type,standard::size", NOFOLLOW, Some(&self.cancel)) else { return (0, 0) };
        if i.file_type() != gio::FileType::Directory {
            return (i.size() as u64, 1);
        }
        let mut t = (0, 1);
        if let Ok(en) = f.enumerate_children("standard::name", NOFOLLOW, Some(&self.cancel)) {
            while let Ok(Some(c)) = en.next_file(Some(&self.cancel)) {
                let (b, n) = self.count(&f.child(c.name()));
                t.0 += b;
                t.1 += n;
            }
        }
        t
    }

    fn delete_tree(&mut self, f: &gio::File) -> Result<(), Stop> {
        self.check()?;
        if let Ok(i) = f.query_info("standard::type", NOFOLLOW, Some(&self.cancel)) {
            if i.file_type() == gio::FileType::Directory {
                if let Ok(en) = f.enumerate_children("standard::name", NOFOLLOW, Some(&self.cancel)) {
                    while let Ok(Some(c)) = en.next_file(Some(&self.cancel)) {
                        self.delete_tree(&f.child(c.name()))?;
                    }
                }
            }
        }
        if let Err(e) = f.delete(Some(&self.cancel)) {
            self.error(&e, f)?;
        }
        self.files += 1;
        self.progress(&f.parse_name(), false);
        Ok(())
    }

    /// Kopiuje drzewo; zwraca miejsce docelowe (może mieć inną nazwę) albo None, gdy pominięto.
    fn copy_tree(&mut self, src: &gio::File, dst: &gio::File) -> Result<Option<gio::File>, Stop> {
        self.check()?;
        let info = match src.query_info("standard::type,standard::size,standard::name,standard::display-name", NOFOLLOW, Some(&self.cancel)) {
            Ok(i) => i,
            Err(e) => return self.error(&e, src).map(|_| None),
        };
        let dir = info.file_type() == gio::FileType::Directory;
        let mut dst = dst.clone();
        let mut overwrite = false;
        let mut merge = false;
        if dst.query_exists(Some(&self.cancel)) {
            match self.ask(&dst, dir)? {
                Answer::Skip => {
                    let (b, n) = self.count(src);
                    self.bytes += b;
                    self.files += n;
                    return Ok(None);
                }
                Answer::Rename => {
                    let parent = dst.parent().unwrap();
                    dst = parent.child(util::free_name(&parent, &dst.basename().unwrap().to_string_lossy(), true));
                }
                Answer::Merge if dir => merge = true,
                Answer::Replace | Answer::Merge => {
                    if dir {
                        self.delete_tree(&dst)?;
                    } else {
                        overwrite = true;
                    }
                }
                Answer::Cancel => return Err(Stop),
            }
        }
        let name = info.display_name().to_string();
        if dir {
            if !merge {
                if let Err(e) = dst.make_directory(Some(&self.cancel)) {
                    return self.error(&e, &dst).map(|_| None);
                }
            }
            self.files += 1;
            if let Ok(en) = src.enumerate_children("standard::name", NOFOLLOW, Some(&self.cancel)) {
                while let Ok(Some(c)) = en.next_file(Some(&self.cancel)) {
                    self.copy_tree(&src.child(c.name()), &dst.child(c.name()))?;
                }
            }
            // Uprawnienia i daty folderu jak w źródle.
            let _ = src.copy_attributes(&dst, gio::FileCopyFlags::ALL_METADATA | gio::FileCopyFlags::NOFOLLOW_SYMLINKS, Some(&self.cancel));
        } else {
            let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::ALL_METADATA;
            if overwrite {
                flags |= gio::FileCopyFlags::OVERWRITE;
            }
            let base = self.bytes;
            let tx = self.tx.clone();
            let mut last = Instant::now();
            let files = self.files;
            let r = src.copy(&dst, flags, Some(&self.cancel), Some(&mut |cur: i64, _total: i64| {
                if last.elapsed() > Duration::from_millis(100) {
                    last = Instant::now();
                    let _ = tx.send_blocking(Msg::Progress { bytes: base + cur as u64, files, name: name.clone() });
                }
            }));
            self.bytes = base + info.size() as u64;
            self.files += 1;
            if let Err(e) = r {
                return self.error(&e, src).map(|_| None);
            }
            self.progress(&name, false);
        }
        Ok(Some(dst))
    }

    fn move_one(&mut self, src: &gio::File, dst: &gio::File) -> Result<Option<gio::File>, Stop> {
        self.check()?;
        if dst.equal(src) {
            return Ok(None);
        }
        if dst.has_prefix(src) {
            let e = glib::Error::new(gio::IOErrorEnum::WouldRecurse, tr("ops.into_itself"));
            return self.error(&e, src).map(|_| None);
        }
        let dir = src.query_file_type(NOFOLLOW, Some(&self.cancel)) == gio::FileType::Directory;
        let mut dst = dst.clone();
        let mut flags = gio::FileCopyFlags::NOFOLLOW_SYMLINKS | gio::FileCopyFlags::NO_FALLBACK_FOR_MOVE | gio::FileCopyFlags::ALL_METADATA;
        if dst.query_exists(Some(&self.cancel)) {
            match self.ask(&dst, dir)? {
                Answer::Skip => return Ok(None),
                Answer::Rename => {
                    let parent = dst.parent().unwrap();
                    dst = parent.child(util::free_name(&parent, &dst.basename().unwrap().to_string_lossy(), true));
                }
                Answer::Merge if dir => {
                    // Scalanie folderów: kopia do istniejącego, potem usunięcie źródła.
                    self.all_conflicts.get_or_insert(Answer::Merge);
                    let r = self.copy_tree(src, &dst)?;
                    self.delete_tree(src)?;
                    return Ok(r);
                }
                Answer::Replace | Answer::Merge => {
                    if dir {
                        self.delete_tree(&dst)?;
                    } else {
                        flags |= gio::FileCopyFlags::OVERWRITE;
                    }
                }
                Answer::Cancel => return Err(Stop),
            }
        }
        match src.move_(&dst, flags, Some(&self.cancel), None) {
            Ok(()) => {
                self.files += 1;
                self.progress(&dst.parse_name(), false);
                Ok(Some(dst))
            }
            // Inny system plików: kopia i usunięcie źródła.
            Err(e) if e.matches(gio::IOErrorEnum::NotSupported) || e.matches(gio::IOErrorEnum::WouldRecurse) || e.matches(gio::IOErrorEnum::WouldMerge) => {
                let r = self.copy_tree(src, &dst)?;
                if r.is_some() {
                    self.delete_tree(src)?;
                }
                Ok(r)
            }
            Err(e) => self.error(&e, src).map(|_| None),
        }
    }

    fn run(&mut self, kind: Kind, sources: &[gio::File], dest: Option<&gio::File>) -> (Option<Undo>, Vec<gio::File>) {
        let mut created = Vec::new();
        let mut moved = Vec::new();
        let mut trashed = Vec::new();
        if matches!(kind, Kind::Copy | Kind::Move | Kind::Delete) {
            let (mut b, mut n) = (0, 0);
            for s in sources {
                let (x, y) = self.count(s);
                b += x;
                n += y;
            }
            self.total = b;
            let _ = self.tx.send_blocking(Msg::Total(b, n));
        } else {
            let _ = self.tx.send_blocking(Msg::Total(0, sources.len() as u64));
        }
        for s in sources {
            if self.cancel.is_cancelled() {
                break;
            }
            let name = s.basename().map(|b| b.to_string_lossy().into_owned()).unwrap_or_default();
            let r: Result<(), Stop> = (|| {
                match kind {
                    Kind::Copy => {
                        let dir = dest.unwrap();
                        let same = s.parent().is_some_and(|p| p.equal(dir));
                        // Wklejenie do tego samego folderu: „plik (kopia)”, bez pytania.
                        let target = if same { dir.child(util::free_name(dir, &name, true)) } else { dir.child(&name) };
                        if let Some(f) = self.copy_tree(s, &target)? {
                            created.push(f);
                        }
                    }
                    Kind::Move => {
                        if let Some(f) = self.move_one(s, &dest.unwrap().child(&name))? {
                            moved.push((f.clone(), s.clone()));
                            created.push(f);
                        }
                    }
                    Kind::Link => {
                        let dir = dest.unwrap();
                        let target = dir.child(util::free_name(dir, &trf("ops.link_name", &[("name", &name)]), false));
                        match target.make_symbolic_link(s.path().unwrap_or_default(), Some(&self.cancel)) {
                            Ok(()) => created.push(target),
                            Err(e) => self.error(&e, s)?,
                        }
                    }
                    Kind::Trash => match s.trash(Some(&self.cancel)) {
                        Ok(()) => trashed.push(s.clone()),
                        Err(e) => self.error(&e, s)?,
                    },
                    Kind::Delete => self.delete_tree(s)?,
                    Kind::Restore => {
                        let orig = s
                            .query_info("trash::orig-path", gio::FileQueryInfoFlags::NONE, Some(&self.cancel))
                            .ok()
                            .and_then(|i| i.attribute_byte_string("trash::orig-path").map(|p| gio::File::for_path(p.as_str())));
                        if let Some(o) = orig {
                            if let Some(p) = o.parent() {
                                let _ = p.make_directory_with_parents(Some(&self.cancel));
                            }
                            if let Some(f) = self.move_one(s, &o)? {
                                created.push(f);
                            }
                        }
                    }
                }
                if !matches!(kind, Kind::Copy | Kind::Move | Kind::Delete) {
                    self.files += 1;
                }
                self.progress(&name, true);
                Ok(())
            })();
            if r.is_err() {
                break;
            }
        }
        let undo = match kind {
            Kind::Copy | Kind::Link if !created.is_empty() => Some(Undo::Created(created.clone())),
            Kind::Move if !moved.is_empty() => Some(Undo::Moved(moved)),
            Kind::Trash if !trashed.is_empty() => Some(Undo::Trashed(trashed)),
            _ => None,
        };
        (undo, created)
    }
}

// ---------- okno „Operacje na plikach” ----------

struct Jobs {
    window: adw::Window,
    list: gtk::ListBox,
    running: Rc<RefCell<usize>>,
}

fn jobs(parent: &gtk::Window) -> (adw::Window, gtk::ListBox, Rc<RefCell<usize>>) {
    JOBS.with(|j| {
        let mut j = j.borrow_mut();
        let jb = j.get_or_insert_with(|| {
            let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
            content.append(&adw::HeaderBar::new());
            let clamp = gtk::ScrolledWindow::builder().vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).build();
            let inner = gtk::Box::builder().orientation(gtk::Orientation::Vertical).margin_top(12).margin_bottom(12).margin_start(12).margin_end(12).build();
            inner.append(&list);
            clamp.set_child(Some(&inner));
            content.append(&clamp);
            let window = adw::Window::builder().title(tr("ops.title")).default_width(460).default_height(220).hide_on_close(true).content(&content).build();
            Jobs { window, list, running: Rc::new(RefCell::new(0)) }
        });
        jb.window.set_transient_for(Some(parent));
        (jb.window.clone(), jb.list.clone(), jb.running.clone())
    })
}

fn verb(kind: Kind) -> &'static str {
    match kind {
        Kind::Copy => "ops.copying",
        Kind::Move => "ops.moving",
        Kind::Link => "ops.linking",
        Kind::Trash => "ops.trashing",
        Kind::Delete => "ops.deleting",
        Kind::Restore => "ops.restoring",
    }
}

/// Pytanie o konflikt nazw (w wątku interfejsu).
fn ask_conflict(parent: &gtk::Window, name: &str, dir: bool, reply: mpsc::Sender<(Answer, bool)>) {
    let d = adw::AlertDialog::new(Some(if dir { tr("conflict.folder_title") } else { tr("conflict.file_title") }), Some(&trf(if dir { "conflict.folder_body" } else { "conflict.file_body" }, &[("name", name)])));
    let all = gtk::CheckButton::with_label(tr("conflict.all"));
    d.set_extra_child(Some(&all));
    d.add_responses(&[("cancel", tr("cancel")), ("skip", tr("conflict.skip")), ("rename", tr("conflict.rename"))]);
    if dir {
        d.add_response("merge", tr("conflict.merge"));
        d.set_response_appearance("merge", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("merge"));
    } else {
        d.add_response("replace", tr("conflict.replace"));
        d.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        d.set_default_response(Some("skip"));
    }
    d.set_close_response("cancel");
    d.connect_response(None, move |_, r| {
        let a = match r {
            "replace" => Answer::Replace,
            "merge" => Answer::Merge,
            "skip" => Answer::Skip,
            "rename" => Answer::Rename,
            _ => Answer::Cancel,
        };
        let _ = reply.send((a, all.is_active()));
    });
    d.present(Some(parent));
}

fn ask_error(parent: &gtk::Window, text: &str, reply: mpsc::Sender<(bool, bool)>) {
    let d = adw::AlertDialog::new(Some(tr("ops.error_title")), Some(text));
    let all = gtk::CheckButton::with_label(tr("conflict.all"));
    d.set_extra_child(Some(&all));
    d.add_responses(&[("cancel", tr("cancel")), ("skip", tr("conflict.skip"))]);
    d.set_default_response(Some("skip"));
    d.set_close_response("cancel");
    d.connect_response(None, move |_, r| {
        let _ = reply.send((r == "skip", all.is_active()));
    });
    d.present(Some(parent));
}

/// Uruchamia operację. `done` dostaje utworzone pliki (do zaznaczenia w widoku).
pub fn run(parent: &gtk::Window, kind: Kind, sources: Vec<gio::File>, dest: Option<gio::File>, done: impl Fn(Vec<gio::File>) + 'static) {
    start(parent, kind, sources, dest, true, done);
}

/// `record`: czy wynik trafia na stos cofania (cofnięcie samo nie trafia).
fn start(parent: &gtk::Window, kind: Kind, sources: Vec<gio::File>, dest: Option<gio::File>, record: bool, done: impl Fn(Vec<gio::File>) + 'static) {
    if sources.is_empty() {
        return;
    }
    let (tx, rx) = async_channel::unbounded::<Msg>();
    let cancel = gio::Cancellable::new();
    {
        let cancel = cancel.clone();
        std::thread::spawn(move || {
            let mut w = Worker { tx: tx.clone(), cancel, total: 0, bytes: 0, files: 0, last: Instant::now(), all_conflicts: None, all_errors: None };
            let (undo, created) = w.run(kind, &sources, dest.as_ref());
            let _ = tx.send_blocking(Msg::Done { undo, created });
        });
    }
    let (window, list, running) = jobs(parent);
    *running.borrow_mut() += 1;
    let row = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(6).margin_top(10).margin_bottom(10).margin_start(12).margin_end(12).build();
    let title = gtk::Label::builder().xalign(0.0).css_classes(["heading"]).label(tr(verb(kind))).build();
    let bar = gtk::ProgressBar::new();
    let detail = gtk::Label::builder().xalign(0.0).css_classes(["dim-label", "caption"]).ellipsize(gtk::pango::EllipsizeMode::Middle).build();
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.set_hexpand(true);
    bar.set_valign(gtk::Align::Center);
    let stop = gtk::Button::builder().icon_name("process-stop-symbolic").tooltip_text(tr("cancel")).css_classes(["flat", "circular"]).build();
    line.append(&bar);
    line.append(&stop);
    row.append(&title);
    row.append(&line);
    row.append(&detail);
    list.append(&row);
    {
        let cancel = cancel.clone();
        stop.connect_clicked(move |_| cancel.cancel());
    }
    let started = Instant::now();
    let parent = parent.clone();
    glib::spawn_future_local(async move {
        let (mut total, mut nfiles) = (0u64, 0u64);
        let mut shown = false;
        while let Ok(m) = rx.recv().await {
            if !shown && started.elapsed() > Duration::from_millis(500) {
                shown = true;
                window.present();
            }
            match m {
                Msg::Total(b, n) => (total, nfiles) = (b, n),
                Msg::Progress { bytes, files, name } => {
                    if total > 0 {
                        bar.set_fraction((bytes as f64 / total as f64).min(1.0));
                        detail.set_label(&format!("{}  ·  {} / {}", name, util::size(bytes), util::size(total)));
                    } else if nfiles > 0 {
                        bar.set_fraction((files as f64 / nfiles as f64).min(1.0));
                        detail.set_label(&trn("ops.files_left", nfiles.saturating_sub(files), &[("name", &name)]));
                    } else {
                        bar.pulse();
                    }
                }
                Msg::Ask { name, dir, reply } => {
                    window.present();
                    shown = true;
                    ask_conflict(&parent, &name, dir, reply);
                }
                Msg::Error { text, reply } => ask_error(&parent, &text, reply),
                Msg::Done { undo, created } => {
                    if let Some(u) = undo.filter(|_| record) {
                        push_undo(u);
                    }
                    list.remove(&row);
                    let left = {
                        let mut r = running.borrow_mut();
                        *r -= 1;
                        *r
                    };
                    if left == 0 {
                        window.set_visible(false);
                    }
                    done(created);
                    break;
                }
            }
        }
    });
}

/// Cofa ostatnią operację (Ctrl+Z).
pub fn undo(parent: &gtk::Window, done: impl Fn() + 'static) {
    let Some(u) = UNDO.with(|s| s.borrow_mut().pop()) else { return };
    match u {
        Undo::Created(files) => start(parent, Kind::Trash, files, None, false, move |_| done()),
        Undo::Moved(pairs) => {
            // Każdy plik wraca do swojego folderu (zwykle wszystkie do jednego).
            let mut by_dir: Vec<(gio::File, Vec<gio::File>)> = Vec::new();
            for (now, before) in pairs {
                let Some(dir) = before.parent() else { continue };
                match by_dir.iter_mut().find(|(d, _)| d.equal(&dir)) {
                    Some((_, v)) => v.push(now),
                    None => by_dir.push((dir, vec![now])),
                }
            }
            let done = Rc::new(done);
            for (dir, files) in by_dir {
                let done = done.clone();
                start(parent, Kind::Move, files, Some(dir), false, move |_| done());
            }
        }
        Undo::Renamed(f, old) => {
            let _ = f.set_display_name(&old, gio::Cancellable::NONE);
            done();
        }
        Undo::Trashed(orig) => {
            // Szukamy w koszu najnowszych wpisów o tych pierwotnych ścieżkach.
            let trash = gio::File::for_uri("trash:///");
            let mut found = Vec::new();
            if let Ok(en) = trash.enumerate_children("standard::name,trash::orig-path,trash::deletion-date", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) {
                let mut items: Vec<(String, String, gio::File)> = Vec::new();
                while let Ok(Some(i)) = en.next_file(gio::Cancellable::NONE) {
                    let p = i.attribute_byte_string("trash::orig-path").map(|s| s.to_string()).unwrap_or_default();
                    let d = i.attribute_string("trash::deletion-date").map(|s| s.to_string()).unwrap_or_default();
                    items.push((p, d, trash.child(i.name())));
                }
                for o in &orig {
                    let op = o.path().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
                    if let Some((_, _, f)) = items.iter().filter(|(p, _, _)| *p == op).max_by(|a, b| a.1.cmp(&b.1)) {
                        found.push(f.clone());
                    }
                }
            }
            start(parent, Kind::Restore, found, None, false, move |_| done());
        }
    }
}

/// Usuwanie na zawsze, z pytaniem (Shift+Delete, opróżnianie kosza).
pub fn confirm_delete(parent: &gtk::Window, files: Vec<gio::File>, done: impl Fn() + 'static) {
    if files.is_empty() {
        return;
    }
    let body = if files.len() == 1 {
        trf("delete.one", &[("name", &util::display_name(&files[0]))])
    } else {
        trn("delete.many", files.len() as u64, &[])
    };
    if !crate::settings::get().confirm_delete {
        run(parent, Kind::Delete, files, None, move |_| done());
        return;
    }
    let d = adw::AlertDialog::new(Some(tr("delete.title")), Some(&body));
    d.add_responses(&[("cancel", tr("cancel")), ("delete", tr("delete.button"))]);
    d.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    d.set_default_response(Some("cancel"));
    d.set_close_response("cancel");
    let p = parent.clone();
    let done = Rc::new(done);
    d.connect_response(None, move |_, r| {
        if r == "delete" {
            let done = done.clone();
            run(&p, Kind::Delete, files.clone(), None, move |_| done());
        }
    });
    d.present(Some(parent));
}

/// Do kosza (z pytaniem, gdy ustawione). Kosz niedostępny (np. pendrive bez .Trash): pytanie o usunięcie.
pub fn trash(parent: &gtk::Window, files: Vec<gio::File>, done: impl Fn() + 'static) {
    if files.is_empty() {
        return;
    }
    let done = Rc::new(done);
    let go = {
        let p = parent.clone();
        let done = done.clone();
        move |files: Vec<gio::File>| {
            let can = files.iter().all(|f| {
                f.query_info("access::can-trash", gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS, gio::Cancellable::NONE)
                    .map(|i| !i.has_attribute("access::can-trash") || i.boolean("access::can-trash"))
                    .unwrap_or(true)
            });
            let done = done.clone();
            if can {
                run(&p, Kind::Trash, files, None, move |_| done());
            } else {
                let d = adw::AlertDialog::new(Some(tr("trash.unavailable_title")), Some(tr("trash.unavailable_body")));
                d.add_responses(&[("cancel", tr("cancel")), ("delete", tr("delete.button"))]);
                d.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                d.set_close_response("cancel");
                let p2 = p.clone();
                d.connect_response(None, move |_, r| {
                    if r == "delete" {
                        let done = done.clone();
                        run(&p2, Kind::Delete, files.clone(), None, move |_| done());
                    }
                });
                d.present(Some(&p));
            }
        }
    };
    if crate::settings::get().confirm_trash {
        let body = if files.len() == 1 { trf("trash.confirm_one", &[("name", &util::display_name(&files[0]))]) } else { trn("trash.confirm_many", files.len() as u64, &[]) };
        let d = adw::AlertDialog::new(Some(tr("trash.confirm_title")), Some(&body));
        d.add_responses(&[("cancel", tr("cancel")), ("trash", tr("action.trash"))]);
        d.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
        d.set_close_response("cancel");
        d.connect_response(None, move |_, r| {
            if r == "trash" {
                go(files.clone());
            }
        });
        d.present(Some(parent));
    } else {
        go(files);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> gio::File {
        let p = std::env::temp_dir().join(format!("filer-ops-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        gio::File::for_path(p)
    }

    fn worker() -> (Worker, async_channel::Receiver<Msg>) {
        let (tx, rx) = async_channel::unbounded();
        (Worker { tx, cancel: gio::Cancellable::new(), total: 0, bytes: 0, files: 0, last: Instant::now(), all_conflicts: Some(Answer::Rename), all_errors: Some(true) }, rx)
    }

    #[test]
    fn copy_move_delete() {
        let t = tmp("cmd");
        let p = t.path().unwrap();
        std::fs::create_dir_all(p.join("a/b")).unwrap();
        std::fs::write(p.join("a/b/x.txt"), "abc").unwrap();
        std::fs::create_dir_all(p.join("dest")).unwrap();
        let (mut w, _rx) = worker();
        let (undo, created) = w.run(Kind::Copy, &[t.child("a")], Some(&t.child("dest")));
        assert_eq!(created.len(), 1);
        assert!(matches!(undo, Some(Undo::Created(_))));
        assert_eq!(std::fs::read_to_string(p.join("dest/a/b/x.txt")).unwrap(), "abc");
        // Ten sam folder: kopia z „(kopia)” w nazwie.
        let (_, c2) = w.run(Kind::Copy, &[t.child("dest/a/b/x.txt")], Some(&t.child("dest/a/b")));
        assert_eq!(c2[0].basename().unwrap().to_string_lossy(), "x (kopia).txt");
        // Konflikt przy przenoszeniu: „zmień nazwę”.
        std::fs::write(p.join("x.txt"), "inny").unwrap();
        let (undo, moved) = w.run(Kind::Move, &[t.child("x.txt")], Some(&t.child("dest/a/b")));
        assert!(matches!(undo, Some(Undo::Moved(_))));
        assert_eq!(moved[0].basename().unwrap().to_string_lossy(), "x (kopia 2).txt");
        assert!(!p.join("x.txt").exists());
        // Folder do samego siebie: błąd pominięty.
        let (_, none) = w.run(Kind::Move, &[t.child("dest")], Some(&t.child("dest/a")));
        assert!(none.is_empty());
        w.run(Kind::Delete, &[t.child("dest")], None);
        assert!(!p.join("dest").exists());
        let _ = std::fs::remove_dir_all(p);
    }

    #[test]
    fn merge_folders() {
        let t = tmp("merge");
        let p = t.path().unwrap();
        std::fs::create_dir_all(p.join("src/F")).unwrap();
        std::fs::create_dir_all(p.join("dst/F")).unwrap();
        std::fs::write(p.join("src/F/nowy.txt"), "1").unwrap();
        std::fs::write(p.join("dst/F/stary.txt"), "2").unwrap();
        let (mut w, _rx) = worker();
        w.all_conflicts = Some(Answer::Merge);
        w.run(Kind::Copy, &[t.child("src/F")], Some(&t.child("dst")));
        assert!(p.join("dst/F/nowy.txt").exists() && p.join("dst/F/stary.txt").exists());
        let _ = std::fs::remove_dir_all(p);
    }
}
