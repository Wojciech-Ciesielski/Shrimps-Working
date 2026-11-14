//! Strona „Z Ubuntu”: popularne aplikacje (ponad milion instalacji według statystyk Flathub), których
//! Flathub nie ma jako sprawdzonych. Lista: data/ubuntu-apps.json, sprawdzana na żywo w CI
//! (tools/check-ubuntu-apps.py). Wszystkie działają w jednym wspólnym kontenerze Ubuntu 24.04
//! (distrobox `shrimp-ubuntu`): apt w kontenerze, skrót w menu przez distrobox-export.
//! Przed aktualizacją kontenera powstaje kopia (podman commit), którą można przywrócić.
use adw::prelude::*;
use gtk::{gio, glib};
use serde::Deserialize;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::rc::Rc;
use std::sync::OnceLock;

use crate::i18n::{tr, trf};

#[derive(Debug, Clone, Deserialize)]
pub struct UbuntuApp {
    pub package: String,
    pub exec: String,
    pub name: String,
    pub flathub: String,
    pub category: String,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    pub container: String,
    pub image: String,
    pub backup: String,
    pub min_downloads: u64,
    pub apps: Vec<UbuntuApp>,
}

const DATA: &str = include_str!("../data/ubuntu-apps.json");

pub fn data() -> &'static Data {
    static D: OnceLock<Data> = OnceLock::new();
    D.get_or_init(|| serde_json::from_str(DATA).expect("data/ubuntu-apps.json"))
}

type Cmd = Vec<String>;

fn cmd(parts: &[&str]) -> Cmd {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Polecenie w kontenerze.
fn enter(d: &Data, args: &[&str]) -> Cmd {
    let mut c = cmd(&["distrobox", "enter", &d.container, "--"]);
    c.extend(args.iter().map(|s| s.to_string()));
    c
}

fn apt(d: &Data, args: &[&str]) -> Cmd {
    let mut a = vec!["sudo", "env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "-y"];
    a.extend(args);
    enter(d, &a)
}

pub fn create_plan(d: &Data) -> Vec<Cmd> {
    vec![cmd(&["distrobox", "create", "--yes", "--name", &d.container, "--image", &d.image])]
}

pub fn install_plan(d: &Data, app: &UbuntuApp, exists: bool) -> Vec<Cmd> {
    let mut p = if exists { vec![] } else { create_plan(d) };
    p.push(apt(d, &["update"]));
    p.push(apt(d, &["install", &app.package]));
    p.push(enter(d, &["distrobox-export", "--app", &app.exec]));
    p
}

pub fn remove_plan(d: &Data, app: &UbuntuApp) -> Vec<Cmd> {
    vec![enter(d, &["distrobox-export", "--app", &app.exec, "--delete"]), apt(d, &["remove", &app.package])]
}

/// Aktualizacja wszystkiego w kontenerze; najpierw kopia (jedna, nadpisywana).
pub fn update_plan(d: &Data) -> Vec<Cmd> {
    vec![cmd(&["podman", "container", "commit", &d.container, &d.backup]), apt(d, &["update"]), apt(d, &["full-upgrade"])]
}

/// Powrót do kopii sprzed ostatniej aktualizacji (skróty w menu wskazują nazwę kontenera, więc działają dalej).
pub fn restore_plan(d: &Data) -> Vec<Cmd> {
    vec![cmd(&["distrobox", "rm", "--force", &d.container]), cmd(&["distrobox", "create", "--yes", "--name", &d.container, "--image", &d.backup])]
}

fn ok(c: &[&str]) -> bool {
    Command::new(c[0]).args(&c[1..]).output().is_ok_and(|o| o.status.success())
}

pub fn tools_present() -> bool {
    ok(&["distrobox", "version"]) && ok(&["podman", "--version"])
}

pub fn container_exists(d: &Data) -> bool {
    ok(&["podman", "container", "exists", &d.container])
}

pub fn backup_exists(d: &Data) -> bool {
    ok(&["podman", "image", "exists", &d.backup])
}

/// Wykonuje kolejne polecenia; przy błędzie zwraca ostatnią linię komunikatu.
fn run(plan: &[Cmd]) -> Result<(), String> {
    for c in plan {
        let o = Command::new(&c[0]).args(&c[1..]).output().map_err(|e| format!("{}: {e}", c[0]))?;
        if !o.status.success() {
            let err = String::from_utf8_lossy(&o.stderr);
            let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("").trim().to_string();
            return Err(if last.is_empty() { format!("{}: {}", c.join(" "), o.status) } else { last });
        }
    }
    Ok(())
}

fn applications_dir() -> PathBuf {
    glib::user_data_dir().join("applications")
}

/// Skrót wyeksportowany przez distrobox-export dla tej aplikacji (plik `<kontener>-*.desktop`,
/// którego Exec uruchamia `exec` w kontenerze).
pub fn exported_in(dir: &Path, d: &Data, app: &UbuntuApp) -> Option<PathBuf> {
    let prefix = format!("{}-", d.container);
    let rd = std::fs::read_dir(dir).ok()?;
    rd.flatten().map(|e| e.path()).find(|p| {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if !name.starts_with(&prefix) || !name.ends_with(".desktop") {
            return false;
        }
        let Ok(txt) = std::fs::read_to_string(p) else { return false };
        txt.lines().filter_map(|l| l.strip_prefix("Exec=")).any(|e| {
            e.split_once(" -- ").is_some_and(|(_, cmd)| cmd.split_whitespace().next().is_some_and(|c| c == app.exec || c.ends_with(&format!("/{}", app.exec))))
        })
    })
}

// ---------- strona ----------

struct Ctx {
    list: gtk::ListBox,
    toasts: adw::ToastOverlay,
    busy: Cell<bool>,
    update: gtk::Button,
    restore: gtk::Button,
}

/// Strona do wstawienia w Shrimp Apps (treść AdwToolbarView).
pub fn page() -> gtk::Widget {
    let d = data();
    let toasts = adw::ToastOverlay::new();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(12);
    content.set_margin_end(12);

    let intro = crate::widgets::wrap_label(&trf("ubuntu.intro", &[("n", &format_million(d.min_downloads))]), &["dim-label"]);
    content.append(&intro);

    if !tools_present() {
        let st = adw::StatusPage::builder()
            .icon_name("dialog-warning-symbolic")
            .title(tr("ubuntu.no-tools"))
            .description(tr("ubuntu.no-tools-body"))
            .build();
        content.append(&st);
        toasts.set_child(Some(&scrolled(&content)));
        return toasts.upcast();
    }

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    content.append(&list);

    let update = gtk::Button::builder().label(tr("ubuntu.update")).build();
    update.add_css_class("pill");
    let restore = gtk::Button::builder().label(tr("ubuntu.restore")).build();
    restore.add_css_class("pill");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.set_halign(gtk::Align::Center);
    bar.append(&update);
    bar.append(&restore);
    content.append(&bar);
    content.append(&crate::widgets::wrap_label(tr("ubuntu.backup-note"), &["dim-label", "caption"]));

    let ctx = Rc::new(Ctx { list: list.clone(), toasts: toasts.clone(), busy: Cell::new(false), update: update.clone(), restore: restore.clone() });
    fill(&ctx);
    {
        let c = ctx.clone();
        update.connect_clicked(move |_| {
            let d = data();
            run_op(&c, update_plan(d), tr("ubuntu.updated").to_string());
        });
    }
    {
        let c = ctx.clone();
        restore.connect_clicked(move |b| {
            let c = c.clone();
            let dlg = adw::AlertDialog::new(Some(tr("ubuntu.restore-q")), Some(tr("ubuntu.restore-body")));
            dlg.add_responses(&[("cancel", tr("ubuntu.cancel")), ("restore", tr("ubuntu.restore"))]);
            dlg.set_response_appearance("restore", adw::ResponseAppearance::Destructive);
            dlg.set_close_response("cancel");
            dlg.connect_response(None, move |_, r| {
                if r == "restore" {
                    run_op(&c, restore_plan(data()), tr("ubuntu.restored").to_string());
                }
            });
            dlg.present(Some(b));
        });
    }
    toasts.set_child(Some(&scrolled(&content)));
    toasts.upcast()
}

fn format_million(n: u64) -> String {
    // 1000000 → „1 000 000” (separator tysięcy jak w języku interfejsu: spacja wąska działa w każdym).
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push('\u{202F}');
        }
        out.push(ch);
    }
    out
}

fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::builder().maximum_size(820).child(child).build();
    gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build()
}

/// Lista od nowa (po każdej operacji).
fn fill(ctx: &Rc<Ctx>) {
    while let Some(r) = ctx.list.row_at_index(0) {
        ctx.list.remove(&r);
    }
    let d = data();
    let dir = applications_dir();
    let mut apps: Vec<&UbuntuApp> = d.apps.iter().collect();
    apps.sort_by_key(|a| crate::catalog::fold(&a.name));
    for app in apps {
        let exported = exported_in(&dir, d, app);
        let cat = crate::categories::Category::ALL.into_iter().find(|c| c.key() == app.category).map(|c| c.title().to_string()).unwrap_or_default();
        let row = adw::ActionRow::builder().title(&app.name).subtitle(format!("{cat} · {}", app.package)).build();
        // Źródło liczby instalacji (zasada miliona, sprawdzana w CI).
        row.set_tooltip_text(Some(&format!("https://flathub.org/apps/{}", app.flathub)));
        row.add_prefix(&gtk::Image::builder().icon_name("application-x-executable-symbolic").pixel_size(32).build());
        let btns = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        btns.set_valign(gtk::Align::Center);
        if let Some(desktop) = exported {
            let open = gtk::Button::builder().label(tr("ubuntu.open")).build();
            open.connect_clicked(move |_| {
                if let Some(info) = gio::DesktopAppInfo::from_filename(&desktop) {
                    let _ = info.launch(&[], gio::AppLaunchContext::NONE);
                }
            });
            let rm = gtk::Button::builder().icon_name("user-trash-symbolic").tooltip_text(tr("ubuntu.remove")).build();
            rm.add_css_class("flat");
            let (c, a) = (ctx.clone(), app.clone());
            rm.connect_clicked(move |_| run_op(&c, remove_plan(data(), &a), trf("ubuntu.removed", &[("name", &a.name)])));
            btns.append(&rm);
            btns.append(&open);
        } else {
            let get = gtk::Button::builder().label(tr("ubuntu.install")).build();
            get.add_css_class("suggested-action");
            let (c, a) = (ctx.clone(), app.clone());
            get.connect_clicked(move |b| {
                b.set_label(tr("ubuntu.installing"));
                let exists = container_exists(data());
                run_op(&c, install_plan(data(), &a, exists), trf("ubuntu.installed", &[("name", &a.name)]));
            });
            btns.append(&get);
        }
        row.add_suffix(&btns);
        ctx.list.append(&row);
    }
    let exists = container_exists(d);
    ctx.update.set_sensitive(!ctx.busy.get() && exists);
    ctx.restore.set_sensitive(!ctx.busy.get() && exists && backup_exists(d));
    ctx.list.set_sensitive(!ctx.busy.get());
}

/// Jedna operacja naraz, w wątku; po niej lista od nowa i komunikat.
fn run_op(ctx: &Rc<Ctx>, plan: Vec<Cmd>, done: String) {
    if ctx.busy.replace(true) {
        return;
    }
    ctx.list.set_sensitive(false);
    ctx.update.set_sensitive(false);
    ctx.restore.set_sensitive(false);
    let ctx = ctx.clone();
    glib::spawn_future_local(async move {
        let r = gio::spawn_blocking(move || run(&plan)).await.unwrap_or_else(|_| Err("panic".into()));
        ctx.busy.set(false);
        let msg = match r {
            Ok(()) => done,
            Err(e) => trf("ubuntu.failed", &[("error", &e)]),
        };
        ctx.toasts.add_toast(adw::Toast::new(&msg));
        fill(&ctx);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_is_valid() {
        let d = data();
        assert_eq!(d.min_downloads, 1_000_000);
        assert!(d.image.contains("ubuntu:24.04"));
        assert!(!d.apps.is_empty());
        for a in &d.apps {
            assert!(crate::categories::Category::ALL.iter().any(|c| c.key() == a.category), "{}: kategoria {}", a.package, a.category);
            assert!(a.package.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "+-.".contains(c)), "{}", a.package);
            assert!(!a.exec.contains(char::is_whitespace));
        }
    }

    #[test]
    fn plans() {
        let d = data();
        let vlc = d.apps.iter().find(|a| a.package == "vlc").unwrap();
        let p = install_plan(d, vlc, false);
        assert_eq!(p[0][..3], ["distrobox", "create", "--yes"]);
        assert_eq!(p.last().unwrap()[4..], ["distrobox-export", "--app", "vlc"]);
        assert!(p[2].ends_with(&["install".into(), "vlc".into()]));
        assert_eq!(install_plan(d, vlc, true).len(), 3);
        let u = update_plan(d);
        assert_eq!(u[0], ["podman", "container", "commit", "shrimp-ubuntu", "localhost/shrimp-ubuntu-backup:latest"]);
        assert_eq!(restore_plan(d)[1].last().unwrap(), &d.backup);
        assert!(remove_plan(d, vlc)[0].contains(&"--delete".to_string()));
    }

    #[test]
    fn finds_exported_desktop() {
        let d = data();
        let t = std::env::temp_dir().join(format!("shrimp-ubuntu-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&t);
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("shrimp-ubuntu-vlc.desktop"), "[Desktop Entry]\nExec=/usr/bin/distrobox-enter  -n shrimp-ubuntu  --   /usr/bin/vlc --started-from-file %U\n").unwrap();
        std::fs::write(t.join("other-gimp.desktop"), "[Desktop Entry]\nExec=distrobox-enter -n other -- gimp\n").unwrap();
        let vlc = d.apps.iter().find(|a| a.package == "vlc").unwrap();
        let gimp = d.apps.iter().find(|a| a.package == "gimp").unwrap();
        assert_eq!(exported_in(&t, d, vlc), Some(t.join("shrimp-ubuntu-vlc.desktop")));
        assert_eq!(exported_in(&t, d, gimp), None);
        assert_eq!(format_million(1_000_000), "1\u{202F}000\u{202F}000");
        let _ = std::fs::remove_dir_all(&t);
    }
}
