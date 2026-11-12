//! Rozmowa z libflatpak w osobnym wątku.
//!
//! Obiekty libflatpak nie mogą przechodzić między wątkami, więc cała praca z
//! instalacją (katalog, instalacja, aktualizacja, usuwanie, uruchamianie) dzieje
//! się w jednym wątku roboczym. Okno wysyła mu polecenia (`Cmd`) i dostaje
//! zdarzenia (`Event`) przez kanał async, odbierany w głównej pętli GTK.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, SystemTime};

use libflatpak::gio;
use libflatpak::glib;
use libflatpak::prelude::*;

use crate::catalog::{self, App, Langs};

pub const FLATHUB_REPO_FILE: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

/// Katalog odświeżamy z sieci, gdy jest starszy niż tyle.
const APPSTREAM_MAX_AGE: Duration = Duration::from_secs(6 * 3600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Install,
    Update,
    Uninstall,
}

pub enum Cmd {
    /// Upewnij się, że jest repozytorium, wczytaj katalog (z sieci, gdy `force` lub stary)
    /// i listę zainstalowanych.
    Refresh { force: bool },
    /// Tylko lista zainstalowanych i aktualizacji.
    ListInstalled,
    Run { kind: OpKind, app_id: String, flatpak_ref: String, cancel: gio::Cancellable },
    /// Wszystkie dostępne aktualizacje jedną transakcją.
    UpdateAll { app_ids: Vec<String>, cancel: gio::Cancellable },
    Launch { app_id: String },
    Size { app_id: String, flatpak_ref: String },
}

#[derive(Debug, Clone)]
pub struct Installed {
    pub id: String,
    pub flatpak_ref: String,
    pub name: String,
    pub summary: String,
    pub version: Option<String>,
    pub installed_size: u64,
    pub has_update: bool,
}

#[derive(Debug)]
pub enum Event {
    Status(String),
    Catalog(Vec<App>),
    /// Nie udało się wczytać katalogu (ani z sieci, ani z pamięci podręcznej).
    CatalogFailed(String),
    /// Odświeżenie z sieci nie wyszło, ale pokazujemy zapisany katalog.
    CatalogStale(String),
    Installed(Vec<Installed>),
    Progress { app_id: String, fraction: f64 },
    Done { app_id: String, kind: OpKind, error: Option<String>, cancelled: bool },
    Size { app_id: String, download: u64, installed: u64 },
    LaunchFailed { app_id: String, error: String },
}

pub struct Backend {
    tx: mpsc::Sender<Cmd>,
}

impl Backend {
    pub fn start(events: async_channel::Sender<Event>) -> Backend {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("flatpak".into())
            .spawn(move || {
                let ctx = glib::MainContext::new();
                let _ = ctx.with_thread_default(|| worker(rx, events));
            })
            .expect("wątek flatpak");
        Backend { tx }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

pub fn remote_name() -> String {
    std::env::var("SHRIMP_APPS_REMOTE").unwrap_or_else(|_| "flathub".into())
}

fn open_installation() -> Result<libflatpak::Installation, glib::Error> {
    // SHRIMP_APPS_INSTALLATION=user tylko do testów; Shrimp instaluje aplikacje dla całego systemu.
    if std::env::var("SHRIMP_APPS_INSTALLATION").as_deref() == Ok("user") {
        libflatpak::Installation::new_user(gio::Cancellable::NONE)
    } else {
        libflatpak::Installation::new_system(gio::Cancellable::NONE)
    }
}

struct Worker {
    inst: libflatpak::Installation,
    events: async_channel::Sender<Event>,
    remote: String,
    langs: Langs,
}

fn worker(rx: mpsc::Receiver<Cmd>, events: async_channel::Sender<Event>) {
    let inst = match open_installation() {
        Ok(i) => i,
        Err(e) => {
            let _ = events.send_blocking(Event::CatalogFailed(e.to_string()));
            return;
        }
    };
    let langs = Langs::new(glib::language_names().iter().map(|s| s.as_str()));
    let w = Worker { inst, events, remote: remote_name(), langs };
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Refresh { force } => w.refresh(force),
            Cmd::ListInstalled => w.send_installed(),
            Cmd::Run { kind, app_id, flatpak_ref, cancel } => {
                w.run(kind, &app_id, &[flatpak_ref], &cancel);
                w.send_installed();
            }
            Cmd::UpdateAll { app_ids, cancel } => w.update_all(app_ids, &cancel),
            Cmd::Launch { app_id } => {
                if let Err(e) = w.inst.launch(&app_id, None, None, None, gio::Cancellable::NONE) {
                    w.emit(Event::LaunchFailed { app_id, error: e.to_string() });
                }
            }
            Cmd::Size { app_id, flatpak_ref } => {
                if let Ok(r) = w.inst.fetch_remote_ref_sync(
                    &w.remote,
                    libflatpak::RefKind::App,
                    ref_part(&flatpak_ref, 1),
                    Some(ref_part(&flatpak_ref, 2)),
                    Some(ref_part(&flatpak_ref, 3)),
                    gio::Cancellable::NONE,
                ) {
                    w.emit(Event::Size { app_id, download: r.download_size(), installed: r.installed_size() });
                }
            }
        }
    }
}

fn ref_part(r: &str, i: usize) -> &str {
    r.split('/').nth(i).unwrap_or("")
}

impl Worker {
    fn emit(&self, e: Event) {
        let _ = self.events.send_blocking(e);
    }

    /// Dodaje Flathub, jeśli go nie ma (na Shrimp jest dodany już w obrazie).
    fn ensure_remote(&self) -> Result<libflatpak::Remote, String> {
        if let Ok(r) = self.inst.remote_by_name(&self.remote, gio::Cancellable::NONE) {
            if r.is_disabled() {
                r.set_disabled(false);
                let _ = self.inst.modify_remote(&r, gio::Cancellable::NONE);
            }
            return Ok(r);
        }
        if self.remote != "flathub" {
            return Err(crate::i18n::trf("error.no-remote", &[("remote", &self.remote)]));
        }
        self.emit(Event::Status(crate::i18n::tr("status.adding-flathub").into()));
        let body = crate::net::get_bytes(FLATHUB_REPO_FILE).map_err(|e| e.to_string())?;
        let remote = libflatpak::Remote::from_file("flathub", &glib::Bytes::from_owned(body))
            .map_err(|e| e.to_string())?;
        self.inst
            .add_remote(&remote, true, gio::Cancellable::NONE)
            .map_err(|e| e.to_string())?;
        self.inst
            .remote_by_name("flathub", gio::Cancellable::NONE)
            .map_err(|e| e.to_string())
    }

    fn appstream_dir(&self, remote: &libflatpak::Remote) -> Option<PathBuf> {
        remote.appstream_dir(None).and_then(|f| f.path())
    }

    fn appstream_age(&self, remote: &libflatpak::Remote) -> Option<Duration> {
        let ts = remote.appstream_timestamp(None)?.path()?;
        let m = std::fs::metadata(ts).ok()?.modified().ok()?;
        SystemTime::now().duration_since(m).ok()
    }

    fn refresh(&self, force: bool) {
        let remote = match self.ensure_remote() {
            Ok(r) => r,
            Err(e) => {
                self.emit(Event::CatalogFailed(e));
                self.send_installed();
                return;
            }
        };
        let stale = force || self.appstream_age(&remote).map_or(true, |a| a > APPSTREAM_MAX_AGE);
        let mut net_error = None;
        if stale {
            self.emit(Event::Status(crate::i18n::tr("status.downloading-catalog").into()));
            if let Err(e) = self.inst.update_appstream_full_sync(&self.remote, None, None, gio::Cancellable::NONE) {
                net_error = Some(e.to_string());
            }
        }
        match self.appstream_dir(&remote).ok_or_else(|| crate::i18n::tr("error.no-catalog").to_string()).and_then(|d| catalog::load_dir(&d, &self.langs)) {
            Ok(apps) if !apps.is_empty() => {
                self.emit(Event::Catalog(apps));
                if let Some(e) = net_error {
                    self.emit(Event::CatalogStale(e));
                }
            }
            Ok(_) => self.emit(Event::CatalogFailed(net_error.unwrap_or_else(|| crate::i18n::tr("error.empty-catalog").into()))),
            Err(e) => self.emit(Event::CatalogFailed(net_error.unwrap_or(e))),
        }
        self.send_installed();
    }

    fn installed(&self) -> Vec<Installed> {
        // Aktualizacje sprawdza libflatpak w sieci; bez sieci po prostu ich nie ma.
        let updates: HashSet<String> = self
            .inst
            .list_installed_refs_for_update(gio::Cancellable::NONE)
            .unwrap_or_default()
            .iter()
            .filter(|r| r.kind() == libflatpak::RefKind::App)
            .filter_map(|r| r.name().map(|n| n.to_string()))
            .collect();
        let refs = self
            .inst
            .list_installed_refs_by_kind(libflatpak::RefKind::App, gio::Cancellable::NONE)
            .unwrap_or_default();
        let mut out: Vec<Installed> = refs
            .iter()
            .filter(|r| r.is_current() || refs.iter().filter(|o| o.name() == r.name()).count() == 1)
            .filter_map(|r| {
                let id = r.name()?.to_string();
                Some(Installed {
                    flatpak_ref: r.format_ref()?.to_string(),
                    name: r.appdata_name().map(|s| s.to_string()).unwrap_or_else(|| id.clone()),
                    summary: r.appdata_summary().map(|s| s.to_string()).unwrap_or_default(),
                    version: r.appdata_version().map(|s| s.to_string()),
                    installed_size: r.installed_size(),
                    has_update: updates.contains(&id),
                    id,
                })
            })
            .collect();
        out.sort_by_key(|i| catalog::fold(&i.name));
        out
    }

    fn send_installed(&self) {
        let list = self.installed();
        self.emit(Event::Installed(list));
    }

    fn update_all(&self, app_ids: Vec<String>, cancel: &gio::Cancellable) {
        let list = self.installed();
        let refs: Vec<String> = list
            .iter()
            .filter(|i| i.has_update && app_ids.contains(&i.id))
            .map(|i| i.flatpak_ref.clone())
            .collect();
        // Jedna transakcja = jedno pytanie o hasło; postęp pokazujemy przy każdej aplikacji.
        for id in &app_ids {
            self.emit(Event::Progress { app_id: id.clone(), fraction: 0.0 });
        }
        let (events, ids) = (self.events.clone(), app_ids.clone());
        let ok = self.transaction(OpKind::Update, &refs, cancel, move |f| {
            for id in &ids {
                let _ = events.send_blocking(Event::Progress { app_id: id.clone(), fraction: f });
            }
        });
        for id in &app_ids {
            self.emit(Event::Done {
                app_id: id.clone(),
                kind: OpKind::Update,
                error: ok.clone().err(),
                cancelled: cancel.is_cancelled(),
            });
        }
        self.send_installed();
    }

    fn run(&self, kind: OpKind, app_id: &str, refs: &[String], cancel: &gio::Cancellable) {
        self.emit(Event::Progress { app_id: app_id.into(), fraction: 0.0 });
        let (events, id) = (self.events.clone(), app_id.to_string());
        let res = self.transaction(kind, refs, cancel, move |f| {
            let _ = events.send_blocking(Event::Progress { app_id: id.clone(), fraction: f });
        });
        self.emit(Event::Done {
            app_id: app_id.into(),
            kind,
            error: res.err(),
            cancelled: cancel.is_cancelled(),
        });
    }

    fn transaction(
        &self,
        kind: OpKind,
        refs: &[String],
        cancel: &gio::Cancellable,
        progress: impl Fn(f64) + 'static,
    ) -> Result<(), String> {
        let t = libflatpak::Transaction::for_installation(&self.inst, gio::Cancellable::NONE)
            .map_err(|e| e.to_string())?;
        for r in refs {
            let added = match kind {
                OpKind::Install => t.add_install(&self.remote, r, &[]),
                OpKind::Update => t.add_update(r, &[], None),
                OpKind::Uninstall => t.add_uninstall(r),
            };
            added.map_err(|e| e.to_string())?;
        }
        // Nie dopisujemy nowych repozytoriów z zależności (tylko Flathub).
        t.connect_add_new_remote(|_, _, _, _, _| false);

        let progress = std::rc::Rc::new(progress);
        let done_ops = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let first_error: std::rc::Rc<std::cell::RefCell<Option<String>>> = Default::default();
        {
            let progress = progress.clone();
            let done_ops = done_ops.clone();
            // Nowa operacja zaczyna się, gdy poprzednia skończona (operation-done ma w
            // bindingu niebezpieczny argument commit, który przy usuwaniu bywa NULL).
            let started = std::rc::Rc::new(std::cell::Cell::new(0usize));
            t.connect_new_operation(move |t, _op, p| {
                done_ops.set(started.get());
                started.set(started.get() + 1);
                p.set_update_frequency(200);
                let total = t.operations().len().max(1);
                let progress = progress.clone();
                let done_ops = done_ops.clone();
                p.connect_changed(move |p| {
                    let part = p.progress().clamp(0, 100) as f64 / 100.0;
                    let f = (done_ops.get() as f64 + part) / total as f64;
                    progress(f.clamp(0.0, 1.0));
                });
            });
        }
        {
            let first_error = first_error.clone();
            t.connect_operation_error(move |_, op, err, details| {
                // Błąd zależności nieobowiązkowej (np. tłumaczenia) nie przerywa całości.
                // W libflatpak 0.7 stała FLATPAK_TRANSACTION_ERROR_DETAILS_NON_FATAL nazywa się FATAL.
                let non_fatal = details.contains(libflatpak::TransactionErrorDetails::FATAL);
                if !non_fatal {
                    let what = op.get_ref().map(|r| r.to_string()).unwrap_or_default();
                    first_error.borrow_mut().get_or_insert_with(|| format!("{what}: {err}"));
                }
                non_fatal
            });
        }
        let res = t.run(Some(cancel));
        progress(1.0);
        match res {
            Ok(()) => Ok(()),
            Err(e) => Err(first_error.borrow().clone().unwrap_or_else(|| e.to_string())),
        }
    }
}
