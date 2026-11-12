//! `shrimp-apps --self-test [ID]`: sprawdza backend bez okna (CI, ISO).
//! Wczytuje katalog i szuka w nim; gdy podano ID: instaluje, sprawdza listę
//! zainstalowanych, aktualizuje, uruchamia nic i usuwa.

use gtk::{gio, glib};

use crate::backend::{Backend, Cmd, Event, Installed, OpKind};
use crate::catalog::{self, App};

struct T {
    rx: async_channel::Receiver<Event>,
    backend: Backend,
}

impl T {
    /// Czeka na pierwsze zdarzenie, dla którego `f` zwraca Some.
    fn wait<R>(&self, mut f: impl FnMut(Event) -> Option<R>) -> Result<R, String> {
        loop {
            let ev = self.rx.recv_blocking().map_err(|e| e.to_string())?;
            match &ev {
                Event::Status(s) => println!("  … {s}"),
                Event::Progress { app_id, fraction } => {
                    let p = (fraction * 100.0) as i32;
                    if p % 20 == 0 {
                        println!("  {app_id}: {p} %");
                    }
                }
                _ => {}
            }
            if let Some(r) = f(ev) {
                return Ok(r);
            }
        }
    }

    fn installed(&self) -> Result<Vec<Installed>, String> {
        self.backend.send(Cmd::ListInstalled);
        self.wait(|e| if let Event::Installed(l) = e { Some(l) } else { None })
    }

    fn op(&self, kind: OpKind, id: &str, r: &str) -> Result<(), String> {
        println!("{kind:?} {id}");
        self.backend.send(Cmd::Run {
            kind,
            app_id: id.into(),
            flatpak_ref: r.into(),
            cancel: gio::Cancellable::new(),
        });
        let err = self.wait(|e| match e {
            Event::Done { app_id, error, .. } if app_id == id => Some(error),
            _ => None,
        })?;
        match err {
            Some(e) => Err(format!("{kind:?} {id}: {e}")),
            None => Ok(()),
        }
    }
}

pub fn run(app_id: Option<String>) -> glib::ExitCode {
    match run_inner(app_id) {
        Ok(()) => {
            println!("OK");
            glib::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("BŁĄD: {e}");
            glib::ExitCode::FAILURE
        }
    }
}

fn run_inner(app_id: Option<String>) -> Result<(), String> {
    let (tx, rx) = async_channel::unbounded();
    let t = T { rx, backend: Backend::start(tx) };

    println!("Katalog ({})", crate::backend::remote_name());
    t.backend.send(Cmd::Refresh { force: true });
    let apps: Vec<App> = t.wait(|e| match e {
        Event::Catalog(a) => Some(Ok(a)),
        Event::CatalogFailed(e) => Some(Err(e)),
        Event::CatalogStale(e) => {
            println!("  uwaga: katalog z pamięci ({e})");
            None
        }
        _ => None,
    })??;
    println!("  aplikacji: {}", apps.len());
    let mut per_cat = std::collections::BTreeMap::new();
    for a in &apps {
        *per_cat.entry(a.main_category().map(|c| c.title()).unwrap_or("(bez kategorii)")).or_insert(0) += 1;
    }
    for (k, v) in &per_cat {
        println!("  {k}: {v}");
    }
    let with_icons = apps.iter().filter(|a| a.icon_cached.is_some() || a.icon_remote.is_some()).count();
    println!("  z ikoną: {with_icons}, ze zrzutami: {}", apps.iter().filter(|a| !a.screenshots.is_empty()).count());

    let pop = crate::net::popular();
    println!("Popularne z Flathub: {} (pierwsze: {:?})", pop.len(), pop.iter().take(5).collect::<Vec<_>>());

    let query = app_id.as_deref().map(|id| id.rsplit('.').next().unwrap_or(id).to_string()).unwrap_or_else(|| "text".into());
    let terms = catalog::search_terms(&query);
    let mut hits: Vec<&App> = apps.iter().filter(|a| a.search_score(&terms) > 0).collect();
    hits.sort_by_key(|a| std::cmp::Reverse(a.search_score(&terms)));
    println!("Szukaj „{query}”: {} wyników: {:?}", hits.len(), hits.iter().take(5).map(|a| &a.name).collect::<Vec<_>>());
    if hits.is_empty() {
        return Err("wyszukiwanie nic nie znalazło".into());
    }

    let installed = t.installed()?;
    println!("Zainstalowane: {} (z aktualizacją: {})", installed.len(), installed.iter().filter(|i| i.has_update).count());

    let Some(id) = app_id else { return Ok(()) };
    let app = apps.iter().find(|a| a.id == id).ok_or(format!("{id} nie ma w katalogu"))?;
    println!("Aplikacja: {} ({}), {}", app.name, app.flatpak_ref, app.summary);

    t.backend.send(Cmd::Size { app_id: id.clone(), flatpak_ref: app.flatpak_ref.clone() });
    let (d, i) = t.wait(|e| match e {
        Event::Size { download, installed, .. } => Some((download, installed)),
        _ => None,
    })?;
    println!("  rozmiar: {} do pobrania, {} po instalacji", glib::format_size(d), glib::format_size(i));

    let was_installed = installed.iter().any(|i| i.id == id);
    if !was_installed {
        t.op(OpKind::Install, &id, &app.flatpak_ref)?;
    }
    let after = t.installed()?;
    let inst = after.iter().find(|i| i.id == id).ok_or("po instalacji brak na liście")?;
    println!("  zainstalowana: {} {:?}, aktualizacja: {}", inst.name, inst.version, inst.has_update);

    t.op(OpKind::Update, &id, &inst.flatpak_ref)?;
    let after = t.installed()?;
    let inst = after.iter().find(|i| i.id == id).ok_or("po aktualizacji brak na liście")?;
    if inst.has_update {
        return Err("po aktualizacji nadal jest aktualizacja".into());
    }
    println!("  po aktualizacji: {:?}", inst.version);

    if !was_installed {
        t.op(OpKind::Uninstall, &id, &inst.flatpak_ref)?;
        if t.installed()?.iter().any(|i| i.id == id) {
            return Err("po usunięciu nadal na liście".into());
        }
        println!("  usunięta");
    }
    Ok(())
}
