//! Stan aplikacji współdzielony przez wszystkie widoki: katalog, zainstalowane,
//! trwające operacje oraz obserwatorzy, którzy odświeżają się przy zmianie.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk::{gio, glib, prelude::*};

use crate::backend::{Backend, Cmd, Installed, OpKind};
use crate::catalog::App;
use crate::categories::Category;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Available,
    Installed,
    UpdateAvailable,
    Queued(OpKind),
    Busy(OpKind, f64),
}

impl Status {
    pub fn busy(self) -> bool {
        matches!(self, Status::Queued(_) | Status::Busy(..))
    }
}

type Watcher = (String, glib::WeakRef<glib::Object>, Rc<dyn Fn()>);

pub const ANY: &str = "*";

#[derive(Default)]
pub struct Store {
    pub apps: RefCell<Vec<Rc<App>>>,
    pub by_id: RefCell<HashMap<String, Rc<App>>>,
    rank: RefCell<HashMap<String, usize>>,
    pub installed: RefCell<HashMap<String, Installed>>,
    ops: RefCell<HashMap<String, Status>>,
    cancels: RefCell<HashMap<String, gio::Cancellable>>,
    watchers: RefCell<Vec<Watcher>>,
    pub sizes: RefCell<HashMap<String, (u64, u64)>>,
    backend: RefCell<Option<Rc<Backend>>>,
}

impl Store {
    pub fn set_backend(&self, b: Rc<Backend>) {
        self.backend.replace(Some(b));
    }

    pub fn send(&self, cmd: Cmd) {
        if let Some(b) = self.backend.borrow().as_ref() {
            b.send(cmd);
        }
    }

    pub fn set_catalog(&self, apps: Vec<App>) {
        let apps: Vec<Rc<App>> = apps.into_iter().map(Rc::new).collect();
        let mut by_id = HashMap::new();
        for a in &apps {
            by_id.insert(a.id.clone(), a.clone());
        }
        self.apps.replace(apps);
        self.by_id.replace(by_id);
        self.resort();
    }

    pub fn set_popular(&self, ids: Vec<String>) {
        let rank = ids.into_iter().enumerate().map(|(i, id)| (id, i)).collect();
        self.rank.replace(rank);
        self.resort();
    }

    fn resort(&self) {
        let rank = self.rank.borrow();
        self.apps
            .borrow_mut()
            .sort_by_key(|a| (rank.get(&a.id).copied().unwrap_or(usize::MAX), crate::catalog::fold(&a.name)));
    }

    pub fn rank(&self, id: &str) -> usize {
        self.rank.borrow().get(id).copied().unwrap_or(usize::MAX)
    }

    pub fn app(&self, id: &str) -> Option<Rc<App>> {
        self.by_id.borrow().get(id).cloned()
    }

    pub fn in_category(&self, c: Category) -> Vec<Rc<App>> {
        self.apps.borrow().iter().filter(|a| a.main_category() == Some(c)).cloned().collect()
    }

    pub fn status(&self, id: &str) -> Status {
        if let Some(s) = self.ops.borrow().get(id) {
            return *s;
        }
        match self.installed.borrow().get(id) {
            Some(i) if i.has_update => Status::UpdateAvailable,
            Some(_) => Status::Installed,
            None => Status::Available,
        }
    }

    pub fn set_installed(&self, list: Vec<Installed>) {
        self.installed.replace(list.into_iter().map(|i| (i.id.clone(), i)).collect());
        self.notify(ANY);
    }

    pub fn updates(&self) -> Vec<String> {
        let mut v: Vec<String> = self.installed.borrow().values().filter(|i| i.has_update).map(|i| i.id.clone()).collect();
        v.sort();
        v
    }

    /// Obserwator zmian stanu aplikacji `id` (albo dowolnej, gdy `ANY`), żyje tak długo jak `owner`.
    pub fn watch(&self, id: &str, owner: &impl IsA<glib::Object>, f: impl Fn() + 'static) {
        self.watchers
            .borrow_mut()
            .push((id.to_string(), owner.upcast_ref::<glib::Object>().downgrade(), Rc::new(f)));
    }

    pub fn notify(&self, id: &str) {
        let list: Vec<Rc<dyn Fn()>> = {
            let mut w = self.watchers.borrow_mut();
            w.retain(|(_, o, _)| o.upgrade().is_some());
            w.iter()
                .filter(|(wid, _, _)| id == ANY || wid == id || wid == ANY)
                .map(|(_, _, f)| f.clone())
                .collect()
        };
        for f in list {
            f();
        }
    }

    pub fn set_op(&self, id: &str, s: Option<Status>) {
        match s {
            Some(s) => {
                self.ops.borrow_mut().insert(id.to_string(), s);
            }
            None => {
                self.ops.borrow_mut().remove(id);
                self.cancels.borrow_mut().remove(id);
            }
        }
        self.notify(id);
    }

    pub fn flatpak_ref(&self, id: &str) -> Option<String> {
        if let Some(i) = self.installed.borrow().get(id) {
            return Some(i.flatpak_ref.clone());
        }
        self.app(id).map(|a| a.flatpak_ref.clone())
    }

    pub fn start(&self, kind: OpKind, id: &str) {
        if self.status(id).busy() {
            return;
        }
        let Some(flatpak_ref) = self.flatpak_ref(id) else { return };
        let cancel = gio::Cancellable::new();
        self.cancels.borrow_mut().insert(id.to_string(), cancel.clone());
        self.set_op(id, Some(Status::Queued(kind)));
        self.send(Cmd::Run { kind, app_id: id.to_string(), flatpak_ref, cancel });
    }

    pub fn update_all(&self) {
        let ids: Vec<String> = self.updates().into_iter().filter(|id| !self.status(id).busy()).collect();
        if ids.is_empty() {
            return;
        }
        let cancel = gio::Cancellable::new();
        for id in &ids {
            self.cancels.borrow_mut().insert(id.clone(), cancel.clone());
            self.ops.borrow_mut().insert(id.clone(), Status::Queued(OpKind::Update));
        }
        self.notify(ANY);
        self.send(Cmd::UpdateAll { app_ids: ids, cancel });
    }

    pub fn cancel(&self, id: &str) {
        if let Some(c) = self.cancels.borrow().get(id) {
            c.cancel();
        }
    }

    pub fn launch(&self, id: &str) {
        self.send(Cmd::Launch { app_id: id.to_string() });
    }

    /// Główna akcja przycisku przy aplikacji.
    pub fn primary(&self, id: &str) {
        match self.status(id) {
            Status::Available => self.start(OpKind::Install, id),
            Status::Installed => self.launch(id),
            Status::UpdateAvailable => self.start(OpKind::Update, id),
            _ => {}
        }
    }

    pub fn display_name(&self, id: &str) -> String {
        if let Some(a) = self.app(id) {
            return a.name.clone();
        }
        self.installed.borrow().get(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string())
    }
}

pub fn label_for(s: Status) -> String {
    match s {
        Status::Available => crate::i18n::tr("status.get").into(),
        Status::Installed => crate::i18n::tr("button.open").into(),
        Status::UpdateAvailable => crate::i18n::tr("status.update").into(),
        Status::Queued(_) => crate::i18n::tr("status.queued").into(),
        Status::Busy(OpKind::Uninstall, _) => crate::i18n::tr("status.removing").into(),
        Status::Busy(_, f) => format!("{} %", (f * 100.0).round() as i32),
    }
}
