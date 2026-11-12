//! Okno „Właściwości” jak w Nemo: Podstawowe, Uprawnienia, Otwórz za pomocą.
use crate::i18n::{tr, trf, trn};
use crate::{ops, util};
use adw::prelude::*;
use gtk::{gio, glib};
use std::path::PathBuf;

fn row(group: &adw::PreferencesGroup, title: &str, value: &str) -> gtk::Label {
    let l = gtk::Label::builder().label(value).selectable(true).wrap(true).xalign(1.0).css_classes(["dim-label"]).build();
    let r = adw::ActionRow::builder().title(title).build();
    r.add_suffix(&l);
    group.add(&r);
    l
}

/// Liczy rozmiar i liczbę elementów folderów w tle (lokalne ścieżki).
fn count(paths: Vec<PathBuf>, out: gtk::Label) {
    let (tx, rx) = async_channel::unbounded::<(u64, u64, bool)>();
    std::thread::spawn(move || {
        let (mut bytes, mut n) = (0u64, 0u64);
        let mut stack = paths;
        let mut last = std::time::Instant::now();
        while let Some(p) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&p) else { continue };
            for e in rd.flatten() {
                n += 1;
                if let Ok(m) = e.metadata() {
                    if m.is_dir() {
                        stack.push(e.path());
                    } else {
                        bytes += m.len();
                    }
                }
            }
            if last.elapsed().as_millis() > 200 {
                last = std::time::Instant::now();
                if tx.send_blocking((n, bytes, false)).is_err() {
                    return;
                }
            }
        }
        let _ = tx.send_blocking((n, bytes, true));
    });
    glib::spawn_future_local(async move {
        while let Ok((n, b, _)) = rx.recv().await {
            out.set_label(&trn("props.contents", n, &[("size", &util::size(b))]));
        }
    });
}

pub fn show(parent: &gtk::Window, files: Vec<gio::File>) {
    if files.is_empty() {
        return;
    }
    let infos: Vec<gio::FileInfo> = files.iter().filter_map(|f| f.query_info(util::ATTRS, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE).ok()).collect();
    if infos.len() != files.len() {
        return;
    }
    let single = files.len() == 1;
    let title = if single { trf("props.title", &[("name", &infos[0].display_name())]) } else { trn("props.title_many", files.len() as u64, &[]) };
    let stack = adw::ViewStack::new();

    // Podstawowe
    let basic = adw::PreferencesPage::new();
    let head = adw::PreferencesGroup::new();
    let icon = gtk::Image::builder().pixel_size(64).margin_bottom(6).build();
    match infos[0].icon() {
        Some(i) if single => icon.set_from_gicon(&i),
        _ => icon.set_icon_name(Some("folder-documents")),
    }
    head.add(&icon);
    basic.add(&head);
    let g = adw::PreferencesGroup::new();
    if single {
        let info = &infos[0];
        let f = files[0].clone();
        let name = adw::EntryRow::builder().title(tr("props.name")).text(info.display_name()).show_apply_button(true).build();
        name.set_editable(info.boolean("access::can-rename"));
        {
            let f = f.clone();
            let p = parent.clone();
            name.connect_apply(move |e| {
                let new = e.text().to_string();
                if !util::valid_name(&new) {
                    return;
                }
                let old = util::display_name(&f);
                match f.set_display_name(&new, gio::Cancellable::NONE) {
                    Ok(nf) => ops::push_undo(ops::Undo::Renamed(nf, old)),
                    Err(err) => {
                        let d = adw::AlertDialog::new(Some(tr("rename.failed")), Some(err.message()));
                        d.add_response("ok", tr("ok"));
                        d.present(Some(&p));
                    }
                }
            });
        }
        g.add(&name);
        row(&g, tr("props.type"), &format!("{} ({})", crate::tab::content_desc(info), info.content_type().unwrap_or_default()));
        if util::is_dir(info) {
            let l = row(&g, tr("props.contents_title"), "…");
            if let Some(p) = f.path() {
                count(vec![p], l);
            }
        } else {
            row(&g, tr("props.size"), &format!("{} ({})", util::size(info.size() as u64), trn("size.bytes", info.size() as u64, &[])));
        }
        if let Some(t) = info.symlink_target() {
            row(&g, tr("props.link_target"), &t.to_string_lossy());
        }
        if let Some(p) = f.parent() {
            row(&g, tr("props.location"), &p.parse_name());
        }
        if let Some(d) = util::access_time(info) {
            row(&g, tr("props.accessed"), &d.to_local().ok().and_then(|d| d.format("%c").ok()).unwrap_or_default());
        }
        if let Some(d) = info.modification_date_time() {
            row(&g, tr("props.modified"), &d.to_local().ok().and_then(|d| d.format("%c").ok()).unwrap_or_default());
        }
    } else {
        let files_n = infos.iter().filter(|i| !util::is_dir(i)).count() as u64;
        let bytes: u64 = infos.iter().filter(|i| !util::is_dir(i)).map(|i| i.size() as u64).sum();
        let l = row(&g, tr("props.contents_title"), &trn("props.contents", files_n, &[("size", &util::size(bytes))]));
        let dirs: Vec<PathBuf> = files.iter().zip(&infos).filter(|(_, i)| util::is_dir(i)).filter_map(|(f, _)| f.path()).collect();
        if !dirs.is_empty() {
            count(dirs, l);
        }
    }
    if let Ok(fs) = files[0].query_filesystem_info("filesystem::free,filesystem::size", gio::Cancellable::NONE) {
        if fs.has_attribute("filesystem::free") {
            row(&g, tr("props.free"), &util::size(fs.attribute_uint64("filesystem::free")));
        }
    }
    basic.add(&g);
    stack.add_titled_with_icon(&basic, Some("basic"), tr("props.basic"), "document-properties-symbolic");

    // Uprawnienia (jeden plik lokalny)
    if single && infos[0].has_attribute("unix::mode") {
        let info = &infos[0];
        let f = files[0].clone();
        let dir = util::is_dir(info);
        let mode = info.attribute_uint32("unix::mode");
        let mine = info.attribute_string("owner::user").map(|s| s.to_string()) == Some(glib::user_name().to_string_lossy().into_owned());
        let page = adw::PreferencesPage::new();
        let g = adw::PreferencesGroup::new();
        let levels: Vec<&str> = if dir {
            vec![tr("perm.none"), tr("perm.list"), tr("perm.access"), tr("perm.create")]
        } else {
            vec![tr("perm.none"), tr("perm.read"), tr("perm.read_write")]
        };
        let to_level = |bits: u32| -> u32 {
            let (r, w, x) = (bits & 4 != 0, bits & 2 != 0, bits & 1 != 0);
            if dir {
                match (r, w, x) {
                    (true, true, true) => 3,
                    (true, _, true) => 2,
                    (true, _, _) => 1,
                    _ => 0,
                }
            } else {
                match (r, w) {
                    (true, true) => 2,
                    (true, false) => 1,
                    _ => 0,
                }
            }
        };
        let owner = info.attribute_string("owner::user").map(|s| s.to_string()).unwrap_or_default();
        let group = info.attribute_string("owner::group").map(|s| s.to_string()).unwrap_or_default();
        let mut combos = vec![];
        for (title, who, shift) in [(tr("perm.owner"), owner.as_str(), 6), (tr("perm.group"), group.as_str(), 3), (tr("perm.others"), "", 0)] {
            let c = adw::ComboRow::builder().title(title).subtitle(who).model(&gtk::StringList::new(&levels)).sensitive(mine).build();
            c.set_selected(to_level((mode >> shift) & 7));
            g.add(&c);
            combos.push((c, shift));
        }
        let exec = adw::SwitchRow::builder().title(tr("perm.exec")).active(mode & 0o100 != 0).sensitive(mine).visible(!dir).build();
        g.add(&exec);
        let apply = {
            let f = f.clone();
            let combos = combos.clone();
            let exec = exec.clone();
            move || {
                let mut m = mode & !0o777;
                for (c, shift) in &combos {
                    let bits = match (dir, c.selected()) {
                        (true, 3) => 7,
                        (true, 2) => 5,
                        (true, 1) => 4,
                        (false, 2) => 6,
                        (false, 1) => 4,
                        _ => 0,
                    };
                    m |= bits << shift;
                }
                if !dir && exec.is_active() {
                    m |= (m & 0o444) >> 2;
                }
                let _ = f.set_attribute_uint32("unix::mode", m, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE);
            }
        };
        for (c, _) in &combos {
            let a = apply.clone();
            c.connect_selected_notify(move |_| a());
        }
        exec.connect_active_notify(move |_| apply());
        if !mine {
            g.set_description(Some(tr("perm.not_owner")));
        }
        page.add(&g);
        stack.add_titled_with_icon(&page, Some("perm"), tr("props.permissions"), "system-lock-screen-symbolic");
    }

    // Otwórz za pomocą
    let types: Vec<String> = infos.iter().filter_map(|i| i.content_type().map(|s| s.to_string())).collect();
    if single && !util::is_dir(&infos[0]) && !types.is_empty() {
        let ct = types[0].clone();
        let page = adw::PreferencesPage::new();
        let g = adw::PreferencesGroup::builder().title(trf("openwith.for", &[("type", &crate::tab::content_desc(&infos[0]))])).build();
        let default = gio::AppInfo::default_for_type(&ct, false);
        let mut first: Option<gtk::CheckButton> = None;
        for app in gio::AppInfo::all_for_type(&ct) {
            let radio = gtk::CheckButton::new();
            match &first {
                Some(f) => radio.set_group(Some(f)),
                None => first = Some(radio.clone()),
            }
            radio.set_active(default.as_ref().is_some_and(|d| d.id() == app.id()));
            let r = adw::ActionRow::builder().title(app.display_name()).activatable_widget(&radio).build();
            if let Some(i) = app.icon() {
                r.add_prefix(&gtk::Image::builder().gicon(&i).pixel_size(32).build());
            }
            r.add_suffix(&radio);
            let ct = ct.clone();
            radio.connect_toggled(move |b| {
                if b.is_active() {
                    let _ = app.set_as_default_for_type(&ct);
                }
            });
            g.add(&r);
        }
        let reset = gtk::Button::builder().label(tr("openwith.reset")).halign(gtk::Align::End).margin_top(12).build();
        let ct2 = ct.clone();
        reset.connect_clicked(move |_| gio::AppInfo::reset_type_associations(&ct2));
        g.set_header_suffix(Some(&reset));
        page.add(&g);
        stack.add_titled_with_icon(&page, Some("openwith"), tr("props.open_with"), "application-x-executable-symbolic");
    }

    let header = adw::HeaderBar::new();
    let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
    header.set_title_widget(Some(&switcher));
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(&stack));
    let w = adw::Window::builder().title(title).transient_for(parent).default_width(480).default_height(560).content(&tv).build();
    let esc = gtk::ShortcutController::new();
    esc.add_shortcut(gtk::Shortcut::new(gtk::ShortcutTrigger::parse_string("Escape"), Some(gtk::NamedAction::new("window.close"))));
    w.add_controller(esc);
    w.present();
}
