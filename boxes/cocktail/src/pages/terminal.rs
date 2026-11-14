//! Instalowanie: w trybie prostym pytanie „Co chcesz zainstalować?” i pole na środku, bez wyjścia
//! polecenia. W trybie zaawansowanym (menu ☰) na górze wiersz ze stałym początkiem
//! `transactional-update pkg install`, a pod nim wyjście na bieżąco. Polecenie idzie przez pomocnika
//! (pkexec); użytkownik wpisuje tylko nazwy pakietów.

use adw::prelude::*;
use gtk::{gio, glib};
use std::ffi::OsStr;
use std::rc::Rc;

use crate::app::App;
use crate::zypper;

pub const PREFIX: &str = "transactional-update pkg install";

/// Nazwy pakietów z wiersza; Err(nazwa) dla pierwszej niedozwolonej.
pub fn parse_names(line: &str) -> Result<Vec<String>, String> {
    let names: Vec<String> = line.split_whitespace().map(str::to_string).collect();
    match names.iter().find(|n| !zypper::valid_name(n)) {
        Some(bad) => Err(bad.clone()),
        None => Ok(names),
    }
}

pub struct Terminal {
    pub widget: gtk::Stack,
    app: Rc<App>,
    out: gtk::TextView,
    entry: gtk::Entry,
    simple_entry: gtk::Entry,
    simple_button: gtk::Button,
    simple_status: gtk::Label,
    spinner: gtk::Spinner,
}

impl Terminal {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        // Tryb zaawansowany: wiersz polecenia na górze, wyjście pod nim.
        let advanced = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let out = gtk::TextView::new();
        out.set_editable(false);
        out.set_cursor_visible(false);
        out.set_monospace(true);
        out.set_wrap_mode(gtk::WrapMode::WordChar);
        out.add_css_class("cocktail-term");
        out.buffer().set_text(crate::i18n::tr("terminal.hint"));
        let sw = gtk::ScrolledWindow::new();
        sw.set_vexpand(true);
        sw.set_hscrollbar_policy(gtk::PolicyType::Never);
        sw.set_child(Some(&out));

        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        line.add_css_class("cocktail-term-line");
        let prompt = gtk::Label::new(Some(&format!("$ {PREFIX}")));
        prompt.add_css_class("cocktail-prefix");
        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_placeholder_text(Some(crate::i18n::tr("terminal.placeholder")));
        line.append(&prompt);
        line.append(&entry);
        advanced.append(&line);
        advanced.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        advanced.append(&sw);

        // Tryb prosty: krótkie pytanie i pole na środku, wynik jednym zdaniem pod spodem.
        let simple = gtk::Box::new(gtk::Orientation::Vertical, 18);
        simple.set_valign(gtk::Align::Center);
        simple.set_halign(gtk::Align::Center);
        simple.set_margin_start(24);
        simple.set_margin_end(24);
        let question = gtk::Label::new(Some(crate::i18n::tr("install.question")));
        question.add_css_class("title-2");
        question.set_wrap(true);
        question.set_justify(gtk::Justification::Center);
        let simple_entry = gtk::Entry::new();
        simple_entry.set_width_request(360);
        simple_entry.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("install.question"))]);
        let simple_button = gtk::Button::with_label(crate::i18n::tr("install.button"));
        simple_button.add_css_class("suggested-action");
        simple_button.add_css_class("pill");
        simple_button.add_css_class("cocktail-main");
        simple_button.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.set_visible(false);
        let simple_status = gtk::Label::new(None);
        simple_status.add_css_class("dim-label");
        simple_status.set_wrap(true);
        simple_status.set_justify(gtk::Justification::Center);
        simple_status.set_max_width_chars(48);
        simple.append(&question);
        simple.append(&simple_entry);
        simple.append(&simple_button);
        simple.append(&spinner);
        simple.append(&simple_status);

        let widget = gtk::Stack::new();
        widget.add_named(&simple, Some("simple"));
        widget.add_named(&advanced, Some("advanced"));
        widget.set_visible_child_name(if crate::app::advanced() { "advanced" } else { "simple" });

        let t = Rc::new(Terminal { widget, app: app.clone(), out, entry, simple_entry, simple_button, simple_status, spinner });
        let weak = Rc::downgrade(&t);
        let go = move |text: String| {
            let Some(t) = weak.upgrade() else { return };
            glib::spawn_future_local(async move { t.run(&text).await });
        };
        let g = go.clone();
        t.entry.connect_activate(move |e| g(e.text().to_string()));
        let g = go.clone();
        t.simple_entry.connect_activate(move |e| g(e.text().to_string()));
        let e = t.simple_entry.clone();
        t.simple_button.connect_clicked(move |_| go(e.text().to_string()));
        t
    }

    /// Przełącza tryb prosty i zaawansowany (wyjście polecenia zostaje, widać je po przełączeniu).
    pub fn set_advanced(&self, on: bool) {
        self.widget.set_visible_child_name(if on { "advanced" } else { "simple" });
    }

    pub fn focus(&self) {
        if self.widget.visible_child_name().as_deref() == Some("advanced") {
            self.entry.grab_focus();
        } else {
            self.simple_entry.grab_focus();
        }
    }

    fn busy(&self, on: bool) {
        self.entry.set_sensitive(!on);
        self.simple_entry.set_sensitive(!on);
        self.simple_button.set_sensitive(!on);
        self.spinner.set_visible(on);
        self.spinner.set_spinning(on);
    }

    fn print(&self, text: &str) {
        let buf = self.out.buffer();
        let mut end = buf.end_iter();
        buf.insert(&mut end, text);
        let mark = buf.create_mark(None, &buf.end_iter(), false);
        self.out.scroll_mark_onscreen(&mark);
        buf.delete_mark(&mark);
    }

    async fn run(self: Rc<Self>, line: &str) {
        let names = match parse_names(line) {
            Ok(n) if n.is_empty() => return,
            Ok(n) => n,
            Err(bad) => {
                let msg = crate::i18n::trf("terminal.bad-name", &[("name", &bad)]);
                self.print(&format!("\n{msg}\n"));
                self.simple_status.set_label(&msg);
                return;
            }
        };
        if self.app.busy.get() {
            self.app.toast(crate::i18n::tr("toast.busy"));
            return;
        }
        self.app.busy.set(true);
        self.busy(true);
        self.entry.set_text("");
        self.simple_status.set_label(crate::i18n::tr("install.working"));
        self.print(&format!("\n$ {PREFIX} {}\n", names.join(" ")));
        let code = self.stream(&names).await;
        if code == 0 {
            self.simple_entry.set_text("");
        }
        self.app.busy.set(false);
        self.busy(false);
        self.focus();
    }

    /// Uruchamia pomocnika i dopisuje jego wyjście wiersz po wierszu. Zwraca kod wyjścia.
    async fn stream(&self, names: &[String]) -> i32 {
        let dir = std::env::var("COCKTAIL_LIBEXEC").unwrap_or_else(|_| "/usr/libexec".into());
        let helper = format!("{dir}/cocktail-helper");
        let mut argv: Vec<&str> = Vec::new();
        if std::env::var_os("COCKTAIL_NO_PKEXEC").is_none() {
            argv.push("pkexec");
        }
        argv.push(&helper);
        argv.push("install");
        argv.extend(names.iter().map(String::as_str));
        let args: Vec<&OsStr> = argv.iter().map(OsStr::new).collect();
        let launcher = gio::SubprocessLauncher::new(gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_MERGE);
        let proc = match launcher.spawn(&args) {
            Ok(p) => p,
            Err(e) => {
                self.print(&format!("{}\n", e.message()));
                self.simple_status.set_label(e.message());
                return -1;
            }
        };
        let mut reboot = false;
        if let Some(pipe) = proc.stdout_pipe() {
            let data = gio::DataInputStream::new(&pipe);
            while let Ok(Some(l)) = data.read_line_utf8_future(glib::Priority::DEFAULT).await {
                if l.trim() == "reboot" {
                    reboot = true;
                    continue;
                }
                self.print(&format!("{l}\n"));
            }
        }
        let _ = proc.wait_future().await;
        let code = if proc.has_exited() { proc.exit_status() } else { -1 };
        match code {
            0 => {
                self.print(&format!("{}\n", crate::i18n::tr("terminal.done")));
                self.simple_status.set_label(crate::i18n::tr(if reboot { "terminal.reboot" } else { "terminal.done" }));
                if reboot {
                    self.print(&format!("{}\n", crate::i18n::tr("terminal.reboot")));
                    let mut pending = self.app.pending.borrow_mut();
                    for n in names {
                        pending.insert(n.clone(), crate::app::Op::Install);
                    }
                    drop(pending);
                    self.app.needs_reboot();
                }
                self.app.changed();
            }
            // Okno hasła zamknięte.
            126 | 127 => self.simple_status.set_label(""),
            c => {
                let msg = crate::i18n::trf("terminal.failed", &[("code", &c.to_string())]);
                self.print(&format!("{msg}\n"));
                self.simple_status.set_label(&msg);
            }
        }
        code
    }
}

#[cfg(test)]
mod tests {
    use super::parse_names;

    #[test]
    fn names() {
        assert_eq!(parse_names("  vlc   git ").unwrap(), vec!["vlc", "git"]);
        assert!(parse_names("").unwrap().is_empty());
        assert_eq!(parse_names("vlc ;rm").unwrap_err(), ";rm");
        assert_eq!(parse_names("--from x").unwrap_err(), "--from");
    }
}
