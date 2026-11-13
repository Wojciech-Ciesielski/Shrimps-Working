//! Stan aplikacji, uruchamianie poleceń (gio::Subprocess, bez blokowania okna) i zmiany pakietów.

use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::rc::Rc;

use crate::zypper;

pub struct Out {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn ok(&self) -> bool {
        self.code == 0
    }
    /// pkexec: 126 = okno hasła zamknięte, 127 = brak uprawnień.
    pub fn cancelled(&self) -> bool {
        self.code == 126 || self.code == 127
    }
}

pub async fn run(argv: &[&str]) -> Out {
    let fail = |e: glib::Error| Out { code: -1, stdout: String::new(), stderr: e.message().to_string() };
    let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE);
    let args: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
    let proc = match launcher.spawn(&args) {
        Ok(p) => p,
        Err(e) => return fail(e),
    };
    let (o, e) = match proc.communicate_utf8_future(None).await {
        Ok(v) => v,
        Err(e) => return fail(e),
    };
    let code = if proc.has_exited() { proc.exit_status() } else { -1 };
    Out { code, stdout: o.map(|s| s.to_string()).unwrap_or_default(), stderr: e.map(|s| s.to_string()).unwrap_or_default() }
}

/// Pomocnik z /usr/libexec przez pkexec. `COCKTAIL_LIBEXEC` podmienia katalog (praca nad kodem),
/// `COCKTAIL_NO_PKEXEC=1` uruchamia pomocnika wprost (testy z atrapami zyppera).
pub async fn helper(name: &str, args: &[&str]) -> Out {
    let dir = std::env::var("COCKTAIL_LIBEXEC").unwrap_or_else(|_| "/usr/libexec".into());
    let path = format!("{dir}/{name}");
    let mut argv: Vec<&str> = Vec::new();
    if std::env::var_os("COCKTAIL_NO_PKEXEC").is_none() {
        argv.push("pkexec");
    }
    argv.push(&path);
    argv.extend_from_slice(args);
    run(&argv).await
}

/// Ostatnie sensowne linie wyjścia polecenia do pokazania w oknie błędu.
pub fn tail(out: &Out) -> String {
    let mut msgs = zypper::errors(&out.stdout);
    if msgs.is_empty() {
        let text = if out.stderr.trim().is_empty() { &out.stdout } else { &out.stderr };
        msgs = text.lines().map(str::trim).filter(|l| !l.is_empty()).map(str::to_string).collect();
        let n = msgs.len();
        msgs = msgs.split_off(n.saturating_sub(4));
    }
    if msgs.is_empty() {
        crate::i18n::trf("error.exit-code", &[("code", &out.code.to_string())])
    } else {
        msgs.join("\n")
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    Install,
    Remove,
    Update,
}

pub struct App {
    pub window: adw::ApplicationWindow,
    pub banner: adw::Banner,
    pub toasts: adw::ToastOverlay,
    /// Zmiany czekające na restart (niezmienny system): pakiet → operacja.
    pub pending: RefCell<HashMap<String, Op>>,
    pub busy: Cell<bool>,
    /// Wywoływane po każdej udanej zmianie (odświeżenie list).
    pub on_change: RefCell<Vec<Box<dyn Fn()>>>,
}

impl App {
    pub fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    pub async fn alert(&self, heading: &str, body: &str) {
        let d = adw::AlertDialog::new(Some(heading), Some(body));
        d.add_response("ok", crate::i18n::tr("button.close"));
        d.choose_future(Some(&self.window)).await;
    }

    /// Potwierdzenie. Przycisk w kolorze akcentu, nie czerwony: czerwony jest tylko dla błędów.
    pub async fn confirm(&self, heading: &str, body: &str, action: &str) -> bool {
        let d = adw::AlertDialog::new(Some(heading), Some(body));
        d.add_response("cancel", crate::i18n::tr("button.cancel"));
        d.add_response("go", action);
        d.set_response_appearance("go", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("cancel"));
        d.set_close_response("cancel");
        d.choose_future(Some(&self.window)).await == "go"
    }

    pub fn needs_reboot(&self) {
        self.banner.set_revealed(true);
    }

    pub fn changed(&self) {
        for f in self.on_change.borrow().iter() {
            f();
        }
    }

    /// Instalacja, usunięcie albo aktualizacja. `label` to nazwa dla człowieka („VLC”).
    /// Zwraca true, gdy zmiana się udała.
    pub async fn change(self: &Rc<Self>, op: Op, packages: Vec<String>, label: &str) -> bool {
        if self.busy.get() {
            self.toast(crate::i18n::tr("toast.busy"));
            return false;
        }
        if packages.iter().any(|p| !zypper::valid_name(p)) {
            return false;
        }
        self.busy.set(true);
        let ok = self.change_inner(op, &packages, label).await;
        self.busy.set(false);
        if ok {
            self.changed();
        }
        ok
    }

    async fn change_inner(self: &Rc<Self>, op: Op, packages: &[String], label: &str) -> bool {
        let args: Vec<&str> = packages.iter().map(String::as_str).collect();
        if op == Op::Remove {
            let mut q = vec!["remove-preview"];
            q.extend_from_slice(&args);
            let out = helper("cocktail-query", &q).await;
            if out.cancelled() {
                return false;
            }
            let all = match zypper::parse_remove_preview(&out.stdout) {
                Ok(v) if out.ok() => v,
                _ => {
                    self.alert(&crate::i18n::trf("remove.cannot", &[("name", label)]), &tail(&out)).await;
                    return false;
                }
            };
            if let Some(p) = all.iter().find(|p| zypper::is_protected(p)) {
                self.alert(
                    &crate::i18n::trf("remove.cannot", &[("name", label)]),
                    &crate::i18n::trf("remove.protected", &[("package", p)]),
                )
                .await;
                return false;
            }
            let extra: Vec<&String> = all.iter().filter(|p| !packages.contains(p)).collect();
            let body = if extra.is_empty() {
                String::new()
            } else {
                let shown: Vec<&str> = extra.iter().take(12).map(|s| s.as_str()).collect();
                let more = extra.len().saturating_sub(shown.len());
                let list = shown.join(", ");
                if more > 0 {
                    crate::i18n::trn("remove.also-more", more as u64, &[("list", &list)])
                } else {
                    crate::i18n::trf("remove.also", &[("list", &list)])
                }
            };
            if !self.confirm(&crate::i18n::trf("remove.confirm", &[("name", label)]), &body, crate::i18n::tr("button.remove")).await {
                return false;
            }
        }

        let mut a: Vec<&str> = vec![match op {
            Op::Install => "install",
            Op::Remove => "remove",
            Op::Update => "update",
        }];
        if op == Op::Update && !args.is_empty() {
            a[0] = "update-pkg";
        }
        a.extend_from_slice(&args);
        let out = helper("cocktail-helper", &a).await;
        if out.cancelled() {
            return false;
        }
        if !out.ok() {
            let heading = match op {
                Op::Install => crate::i18n::trf("error.install", &[("name", label)]),
                Op::Remove => crate::i18n::trf("error.remove", &[("name", label)]),
                Op::Update => crate::i18n::tr("error.update").to_string(),
            };
            self.alert(&heading, &tail(&out)).await;
            return false;
        }
        if out.stdout.lines().any(|l| l.trim() == "reboot") {
            let mut pending = self.pending.borrow_mut();
            for p in packages {
                pending.insert(p.clone(), op);
            }
            drop(pending);
            self.needs_reboot();
        }
        true
    }
}

pub fn reboot() {
    glib::spawn_future_local(async {
        let _ = run(&["systemctl", "reboot"]).await;
    });
}
