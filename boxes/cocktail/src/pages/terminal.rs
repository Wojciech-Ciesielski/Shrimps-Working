//! Terminal: bardzo prosty wiersz poleceń ze stałym początkiem `transactional-update pkg install`.
//! Użytkownik wpisuje tylko nazwy pakietów; polecenie idzie przez pomocnika (pkexec), a wyjście
//! pojawia się na bieżąco nad wierszem.

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
    pub widget: gtk::Box,
    app: Rc<App>,
    out: gtk::TextView,
    entry: gtk::Entry,
}

impl Terminal {
    pub fn new(app: &Rc<App>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
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
        widget.append(&sw);
        widget.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        widget.append(&line);

        let t = Rc::new(Terminal { widget, app: app.clone(), out, entry });
        let weak = Rc::downgrade(&t);
        t.entry.connect_activate(move |e| {
            let Some(t) = weak.upgrade() else { return };
            let text = e.text().to_string();
            glib::spawn_future_local(async move { t.run(&text).await });
        });
        t
    }

    pub fn focus(&self) {
        self.entry.grab_focus();
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
                self.print(&format!("\n{}\n", crate::i18n::trf("terminal.bad-name", &[("name", &bad)])));
                return;
            }
        };
        if self.app.busy.get() {
            self.app.toast(crate::i18n::tr("toast.busy"));
            return;
        }
        self.app.busy.set(true);
        self.entry.set_sensitive(false);
        self.entry.set_text("");
        self.print(&format!("\n$ {PREFIX} {}\n", names.join(" ")));
        self.stream(&names).await;
        self.app.busy.set(false);
        self.entry.set_sensitive(true);
        self.entry.grab_focus();
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
            126 | 127 => {}
            c => self.print(&format!("{}\n", crate::i18n::trf("terminal.failed", &[("code", &c.to_string())]))),
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
