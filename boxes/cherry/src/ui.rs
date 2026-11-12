//! Okno Cherry: karty z terminalem (VTE, zsh), po lewej pionowy pasek narzędzi bez podpisów
//! (Jak to działa?, Notatki, Skróty, Historia) i panel wybranego narzędzia, pod terminalem pasek stanu.
//! Zgodnie z Adwaita: ikony bez dodatkowych kształtów, bez linii oddzielających (panele rozdziela odcień tła),
//! terminal najciemniejszy i bez zaokrągleń, dochodzi do krawędzi okna.

use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use vte::prelude::*;

use crate::explain::{self, Kind};
use crate::i18n::{tr, trf};
use crate::store::{self, Abbrev, Data, KeyShortcut, Note, TabsPos};

const CSS: &str = r#"
.cherry-tools { background: @headerbar_bg_color; padding: 6px 4px; }
.cherry-panel { background: @sidebar_bg_color; }
.cherry-panel .title-4 { margin: 12px 12px 4px 12px; }
.cherry-cmd { font-family: monospace; font-size: 1.15em; font-weight: bold; }
.cherry-tok { font-family: monospace; font-weight: bold; }
.cherry-status { background: @headerbar_bg_color; padding: 4px 10px; font-size: 0.9em; }
.cherry-status label { font-family: monospace; }
.cherry-term { border-radius: 0; }
.cherry-sidetabs { background: @sidebar_bg_color; }
row.cherry-warn .cherry-tok, row.cherry-warn .cherry-desc { color: @warning_color; }
row.cherry-hint .cherry-tok { color: @accent_color; }
"#;

/// Narzędzia lewego paska: identyfikator strony panelu i ikona Adwaita.
const TOOLS: [(&str, &str, &str); 4] = [
    ("explain", "dialog-information-symbolic", "tool.explain"),
    ("notes", "document-edit-symbolic", "tool.notes"),
    (
        "keys",
        "preferences-desktop-keyboard-shortcuts-symbolic",
        "tool.keys",
    ),
    ("history", "document-open-recent-symbolic", "tool.history"),
];

/// Stan jednej karty: terminal, pasek stanu i ostatnio wykonana komenda.
struct Tab {
    term: vte::Terminal,
    status_path: gtk::Label,
    status_git: gtk::Label,
    git_box: gtk::Box,
    current: RefCell<String>,
    last: RefCell<String>,
}

pub struct App {
    data: RefCell<Data>,
    window: adw::ApplicationWindow,
    view: adw::TabView,
    bar: adw::TabBar,
    side_tabs: gtk::ListBox,
    side_rev: gtk::Revealer,
    panel_rev: gtk::Revealer,
    panel: gtk::Stack,
    tool_btns: Vec<(&'static str, gtk::ToggleButton)>,
    explain_cmd: gtk::Label,
    explain_list: gtk::ListBox,
    explain_empty: gtk::Label,
    notes_list: gtk::ListBox,
    keys_list: gtk::ListBox,
    abbr_list: gtk::ListBox,
    hist_list: gtk::ListBox,
    hist_search: gtk::SearchEntry,
    shortcuts: gtk::ShortcutController,
    tabs: RefCell<HashMap<String, Rc<Tab>>>,
    next_id: Cell<u32>,
    zdotdir: PathBuf,
    me: RefCell<Weak<App>>,
}

fn now() -> i64 {
    gtk::glib::DateTime::now_utc()
        .map(|d| d.to_unix())
        .unwrap_or(0)
}

pub fn build(app: &adw::Application) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let mut data = store::load();
    data.prune(now());
    let zdotdir = store::write_zsh(&data);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(680)
        .title("Terminal")
        .build();
    let view = adw::TabView::new();
    let bar = adw::TabBar::builder().view(&view).autohide(false).build();

    // Pasek tytułu: pomoc (znak zapytania) po lewej, nowa karta i menu ustawień po prawej.
    let header = adw::HeaderBar::new();
    let help = gtk::Button::builder()
        .icon_name("help-about-symbolic")
        .tooltip_text(tr("help"))
        .build();
    help.set_sensitive(crate::help::url().is_some());
    header.pack_start(&help);
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(tr("settings"))
        .build();
    header.pack_end(&menu_btn);
    let new_tab = gtk::Button::builder()
        .icon_name("tab-new-symbolic")
        .tooltip_text(tr("tab.new"))
        .build();
    header.pack_end(&new_tab);

    // Lewy pasek narzędzi (same ikony) i panel narzędzia.
    let tools = gtk::Box::new(gtk::Orientation::Vertical, 4);
    tools.add_css_class("cherry-tools");
    let mut tool_btns = Vec::new();
    for (id, icon, tip) in TOOLS {
        let b = gtk::ToggleButton::builder()
            .icon_name(icon)
            .tooltip_text(tr(tip))
            .build();
        b.add_css_class("flat");
        b.update_property(&[gtk::accessible::Property::Label(tr(tip))]);
        tools.append(&b);
        tool_btns.push((id, b));
    }
    let panel = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .build();
    panel.set_size_request(300, -1);
    panel.add_css_class("cherry-panel");
    let panel_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideRight)
        .child(&panel)
        .build();

    // Jak to działa?
    let explain_cmd = gtk::Label::builder()
        .xalign(0.0)
        .wrap(true)
        .selectable(true)
        .margin_start(12)
        .margin_end(12)
        .build();
    explain_cmd.add_css_class("cherry-cmd");
    let explain_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    explain_list.add_css_class("navigation-sidebar");
    let explain_empty = gtk::Label::builder()
        .label(tr("explain.empty"))
        .wrap(true)
        .xalign(0.0)
        .margin_start(12)
        .margin_end(12)
        .build();
    explain_empty.add_css_class("dim-label");
    let parts_title = heading(tr("explain.parts"));
    let ex_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    ex_box.append(&heading(tr("tool.explain")));
    ex_box.append(&explain_cmd);
    ex_box.append(&parts_title);
    ex_box.append(&explain_list);
    ex_box.append(&explain_empty);
    panel.add_named(&scroll(&ex_box), Some("explain"));

    // Notatki: lista, plus w nagłówku otwiera okno nowej notatki.
    let notes_list = list();
    let notes_add = icon_button("list-add-symbolic", tr("note.new"));
    let nb = gtk::Box::new(gtk::Orientation::Vertical, 6);
    nb.append(&heading_with(tr("tool.notes"), &notes_add));
    nb.append(&notes_list);
    panel.add_named(&scroll(&nb), Some("notes"));

    // Skróty: klawisze i skróty w zapisie (aliasy zsh).
    let keys_list = list();
    let abbr_list = list();
    let keys_add = icon_button("list-add-symbolic", tr("short.new"));
    let kb = gtk::Box::new(gtk::Orientation::Vertical, 6);
    kb.append(&heading_with(tr("tool.keys"), &keys_add));
    kb.append(&subheading(tr("short.keys")));
    kb.append(&keys_list);
    kb.append(&subheading(tr("short.abbr")));
    kb.append(&abbr_list);
    panel.add_named(&scroll(&kb), Some("keys"));

    // Historia: kosz w nagłówku czyści całą, wyszukiwarka na dole.
    let hist_list = list();
    let hist_clear = icon_button("user-trash-symbolic", tr("hist.clear"));
    let hist_search = gtk::SearchEntry::builder()
        .placeholder_text(tr("hist.search"))
        .margin_start(8)
        .margin_end(8)
        .margin_bottom(8)
        .build();
    let hb = gtk::Box::new(gtk::Orientation::Vertical, 6);
    hb.append(&heading_with(tr("tool.history"), &hist_clear));
    let hs = scroll(&hist_list);
    hs.set_vexpand(true);
    hb.append(&hs);
    hb.append(&hist_search);
    panel.add_named(&hb, Some("history"));

    // Karty pionowo po prawej (opcja w ustawieniach).
    let side_tabs = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    side_tabs.add_css_class("navigation-sidebar");
    let side_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    side_box.add_css_class("cherry-sidetabs");
    side_box.set_size_request(200, -1);
    side_box.append(&scroll(&side_tabs));
    let side_new = icon_button("list-add-symbolic", tr("tab.new"));
    side_new.set_halign(gtk::Align::Start);
    side_new.set_margin_start(6);
    side_new.set_margin_bottom(6);
    side_box.append(&side_new);
    let side_rev = gtk::Revealer::builder()
        .transition_type(gtk::RevealerTransitionType::SlideLeft)
        .child(&side_box)
        .build();

    let center = gtk::Box::new(gtk::Orientation::Vertical, 0);
    center.append(&bar);
    center.append(&view);
    view.set_vexpand(true);
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.append(&tools);
    body.append(&panel_rev);
    center.set_hexpand(true);
    body.append(&center);
    body.append(&side_rev);

    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&header);
    tv.set_content(Some(&body));
    window.set_content(Some(&tv));

    let shortcuts = gtk::ShortcutController::new();
    shortcuts.set_scope(gtk::ShortcutScope::Global);
    window.add_controller(shortcuts.clone());

    let me = Rc::new(App {
        data: RefCell::new(data),
        window: window.clone(),
        view: view.clone(),
        bar,
        side_tabs: side_tabs.clone(),
        side_rev,
        panel_rev,
        panel,
        tool_btns,
        explain_cmd,
        explain_list,
        explain_empty,
        notes_list,
        keys_list,
        abbr_list,
        hist_list,
        hist_search: hist_search.clone(),
        shortcuts,
        tabs: RefCell::new(HashMap::new()),
        next_id: Cell::new(1),
        zdotdir,
        me: RefCell::new(Weak::new()),
    });
    *me.me.borrow_mut() = Rc::downgrade(&me);

    // Sygnały
    help.connect_clicked(move |b| {
        if let Some(u) = crate::help::url() {
            let root = b.root().and_downcast::<gtk::Window>();
            gtk::UriLauncher::new(&u).launch(root.as_ref(), gtk::gio::Cancellable::NONE, |_| {});
        }
    });
    menu_btn.set_popover(Some(&settings_popover(&me)));
    let w = Rc::downgrade(&me);
    new_tab.connect_clicked(move |_| {
        if let Some(a) = w.upgrade() {
            a.add_tab(None)
        }
    });
    let w = Rc::downgrade(&me);
    side_new.connect_clicked(move |_| {
        if let Some(a) = w.upgrade() {
            a.add_tab(None)
        }
    });
    for (id, b) in &me.tool_btns {
        let w = Rc::downgrade(&me);
        let id = *id;
        b.connect_toggled(move |b| {
            if let Some(a) = w.upgrade() {
                a.tool_toggled(id, b.is_active())
            }
        });
    }
    let w = Rc::downgrade(&me);
    notes_add.connect_clicked(move |_| {
        if let Some(a) = w.upgrade() {
            a.note_dialog()
        }
    });
    let w = Rc::downgrade(&me);
    keys_add.connect_clicked(move |_| {
        if let Some(a) = w.upgrade() {
            a.shortcut_dialog()
        }
    });
    let w = Rc::downgrade(&me);
    hist_clear.connect_clicked(move |_| {
        if let Some(a) = w.upgrade() {
            a.data.borrow_mut().history.clear();
            a.save();
            a.refresh_history();
        }
    });
    let w = Rc::downgrade(&me);
    hist_search.connect_search_changed(move |_| {
        if let Some(a) = w.upgrade() {
            a.refresh_history()
        }
    });
    let w = Rc::downgrade(&me);
    view.connect_selected_page_notify(move |_| {
        if let Some(a) = w.upgrade() {
            a.refresh_explain();
            a.sync_side_selection();
            if let Some(t) = a.active() {
                t.term.grab_focus();
            }
        }
    });
    let w = Rc::downgrade(&me);
    view.connect_close_page(move |v, p| {
        if let Some(a) = w.upgrade() {
            a.tabs.borrow_mut().remove(p.child().widget_name().as_str());
            if v.n_pages() <= 1 {
                // Ostatnia karta: zamiast pustego okna nowa karta w folderze domowym.
                a.add_tab(None);
            }
        }
        v.close_page_finish(p, true);
        gtk::glib::Propagation::Stop
    });
    let w = Rc::downgrade(&me);
    view.connect_n_pages_notify(move |_| {
        if let Some(a) = w.upgrade() {
            a.refresh_side_tabs()
        }
    });
    let w = Rc::downgrade(&me);
    side_tabs.connect_row_activated(move |_, row| {
        if let Some(a) = w.upgrade() {
            let i = row.index();
            if i >= 0 && i < a.view.n_pages() {
                a.view.set_selected_page(&a.view.nth_page(i));
            }
        }
    });

    me.apply_tabs_pos();
    me.refresh_notes();
    me.refresh_shortcuts();
    me.refresh_history();
    me.add_tab(None);
    if let Some((_, b)) = me.tool_btns.first() {
        b.set_active(true);
    }
    // Sygnały trzymają słabe odwołania; jedyne mocne żyje tak długo jak okno.
    let keep = RefCell::new(Some(me));
    window.connect_close_request(move |_| {
        keep.borrow_mut().take();
        gtk::glib::Propagation::Proceed
    });
    window.present();
}

fn heading(text: &str) -> gtk::Label {
    let l = gtk::Label::builder().label(text).xalign(0.0).build();
    l.add_css_class("title-4");
    l
}

fn heading_with(text: &str, btn: &gtk::Button) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let h = heading(text);
    h.set_hexpand(true);
    b.append(&h);
    btn.set_valign(gtk::Align::Center);
    btn.set_margin_end(8);
    b.append(btn);
    b
}

fn subheading(text: &str) -> gtk::Label {
    let l = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .margin_start(12)
        .margin_top(6)
        .build();
    l.add_css_class("heading");
    l.add_css_class("dim-label");
    l
}

fn list() -> gtk::ListBox {
    let l = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .build();
    l.add_css_class("navigation-sidebar");
    l
}

fn scroll(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(child)
        .build()
}

fn icon_button(icon: &str, tip: &str) -> gtk::Button {
    let b = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .build();
    b.add_css_class("flat");
    b
}

/// Wiersz listy: komenda (czcionka stała) i opis pod nią, opcjonalnie przycisk usuwania.
fn two_line_row(cmd: &str, desc: &str) -> (gtk::ListBoxRow, gtk::Box) {
    let row = gtk::ListBoxRow::new();
    let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let vb = gtk::Box::new(gtk::Orientation::Vertical, 2);
    vb.set_hexpand(true);
    let c = gtk::Label::builder()
        .label(cmd)
        .xalign(0.0)
        .wrap(true)
        .wrap_mode(gtk::pango::WrapMode::WordChar)
        .build();
    c.add_css_class("cherry-tok");
    vb.append(&c);
    if !desc.is_empty() {
        let d = gtk::Label::builder()
            .label(desc)
            .xalign(0.0)
            .wrap(true)
            .build();
        d.add_css_class("dim-label");
        d.add_css_class("cherry-desc");
        vb.append(&d);
    }
    hb.append(&vb);
    row.set_child(Some(&hb));
    (row, hb)
}

fn clear(l: &gtk::ListBox) {
    while let Some(c) = l.first_child() {
        l.remove(&c);
    }
}

fn settings_popover(app: &Rc<App>) -> gtk::Popover {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 10);
    b.set_margin_top(10);
    b.set_margin_bottom(10);
    b.set_margin_start(10);
    b.set_margin_end(10);

    // Położenie kart: same ikony, podpowiedź w dymku.
    let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    tabs.add_css_class("linked");
    let top = gtk::ToggleButton::builder()
        .icon_name("go-top-symbolic")
        .tooltip_text(tr("set.tabs.top"))
        .build();
    let right = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-right-symbolic")
        .tooltip_text(tr("set.tabs.right"))
        .group(&top)
        .build();
    top.update_property(&[gtk::accessible::Property::Label(tr("set.tabs.top"))]);
    right.update_property(&[gtk::accessible::Property::Label(tr("set.tabs.right"))]);
    let pos = app.data.borrow().settings.tabs;
    top.set_active(pos == TabsPos::Top);
    right.set_active(pos == TabsPos::Right);
    tabs.append(&top);
    tabs.append(&right);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let ti = gtk::Image::from_icon_name("view-paged-symbolic");
    ti.set_tooltip_text(Some(tr("set.tabs")));
    ti.set_hexpand(true);
    ti.set_halign(gtk::Align::Start);
    row.append(&ti);
    row.append(&tabs);
    b.append(&row);
    let w = Rc::downgrade(app);
    top.connect_toggled(move |t| {
        if let (true, Some(a)) = (t.is_active(), w.upgrade()) {
            a.set_tabs_pos(TabsPos::Top)
        }
    });
    let w = Rc::downgrade(app);
    right.connect_toggled(move |t| {
        if let (true, Some(a)) = (t.is_active(), w.upgrade()) {
            a.set_tabs_pos(TabsPos::Right)
        }
    });

    // Usuwanie historii: nigdy, 1, 7, 30 dni.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let hi = gtk::Image::from_icon_name("document-open-recent-symbolic");
    hi.set_tooltip_text(Some(tr("set.history")));
    row.append(&hi);
    let labels: Vec<String> = store::HISTORY_CHOICES
        .iter()
        .map(|c| match c {
            None => tr("set.history.never").to_owned(),
            Some(d) => {
                crate::i18n::trn("set.history.days", u64::from(*d), &[("n", &d.to_string())])
            }
        })
        .collect();
    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let dd = gtk::DropDown::from_strings(&refs);
    dd.set_tooltip_text(Some(tr("set.history")));
    dd.update_property(&[gtk::accessible::Property::Label(tr("set.history"))]);
    let cur = app.data.borrow().settings.history_days;
    dd.set_selected(
        store::HISTORY_CHOICES
            .iter()
            .position(|c| *c == cur)
            .unwrap_or(0) as u32,
    );
    dd.set_hexpand(true);
    row.append(&dd);
    b.append(&row);
    let w = Rc::downgrade(app);
    dd.connect_selected_notify(move |d| {
        if let Some(a) = w.upgrade() {
            let i = d.selected() as usize;
            a.data.borrow_mut().settings.history_days =
                store::HISTORY_CHOICES.get(i).copied().flatten();
            a.data.borrow_mut().prune(now());
            a.save();
            a.refresh_history();
        }
    });

    gtk::Popover::builder().child(&b).build()
}

impl App {
    fn me(&self) -> Rc<App> {
        self.me.borrow().upgrade().expect("App żyje razem z oknem")
    }

    fn save(&self) {
        store::save(&self.data.borrow());
    }

    fn active(&self) -> Option<Rc<Tab>> {
        let p = self.view.selected_page()?;
        self.tabs
            .borrow()
            .get(p.child().widget_name().as_str())
            .cloned()
    }

    /// Nowa karta z terminalem w podanym folderze (domyślnie folder domowy).
    fn add_tab(&self, dir: Option<&str>) {
        let id = format!("tab{}", self.next_id.get());
        self.next_id.set(self.next_id.get() + 1);

        let term = vte::Terminal::new();
        term.add_css_class("cherry-term");
        term.set_scrollback_lines(10_000);
        term.set_vexpand(true);
        term.set_hexpand(true);
        term.set_font(Some(&gtk::pango::FontDescription::from_string(
            "Monospace 11",
        )));
        self.apply_colors(&term);

        let status = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        status.add_css_class("cherry-status");
        let p_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        p_box.append(&gtk::Image::from_icon_name("folder-symbolic"));
        let status_path = gtk::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk::pango::EllipsizeMode::Start)
            .build();
        p_box.append(&status_path);
        p_box.set_hexpand(true);
        status.append(&p_box);
        let git_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        git_box.append(&gtk::Image::from_icon_name(
            "media-playlist-shuffle-symbolic",
        ));
        let status_git = gtk::Label::new(None);
        git_box.append(&status_git);
        git_box.set_visible(false);
        status.append(&git_box);
        let u_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        u_box.append(&gtk::Image::from_icon_name("avatar-default-symbolic"));
        u_box.append(&gtk::Label::new(Some(&format!(
            "{}@{}",
            gtk::glib::user_name().to_string_lossy(),
            gtk::glib::host_name()
        ))));
        status.append(&u_box);

        let page_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        page_box.set_widget_name(&id);
        page_box.append(&term);
        page_box.append(&status);

        let tab = Rc::new(Tab {
            term: term.clone(),
            status_path,
            status_git,
            git_box,
            current: RefCell::new(String::new()),
            last: RefCell::new(String::new()),
        });
        self.tabs.borrow_mut().insert(id.clone(), tab.clone());
        let page = self.view.append(&page_box);
        page.set_title(tr("tab.home"));
        page.set_icon(Some(&gtk::gio::ThemedIcon::new(
            "utilities-terminal-symbolic",
        )));

        let home = gtk::glib::home_dir();
        let wd = dir.map(PathBuf::from).unwrap_or_else(|| home.clone());
        self.update_status(&tab, &page, &wd);
        self.spawn(&term, &wd);

        // Bieżący folder (OSC 7 z .zshrc Cherry): pasek stanu, gałąź gita i tytuł karty.
        let w = Rc::downgrade(&self.me());
        let (t2, p2) = (Rc::downgrade(&tab), page.clone());
        term.connect_current_directory_uri_changed(move |t| {
            let (Some(a), Some(tab)) = (w.upgrade(), t2.upgrade()) else {
                return;
            };
            if let Some(uri) = t.current_directory_uri() {
                if let Ok((path, _)) = gtk::glib::filename_from_uri(&uri) {
                    a.update_status(&tab, &p2, &path);
                }
            }
        });
        // Wpisywana komenda: wiersz kursora po znaku zachęty „❯ ”.
        let w = Rc::downgrade(&self.me());
        let t2 = Rc::downgrade(&tab);
        term.connect_cursor_moved(move |t| {
            let (Some(a), Some(tab)) = (w.upgrade(), t2.upgrade()) else {
                return;
            };
            let line = typed_line(t);
            if *tab.current.borrow() != line {
                *tab.current.borrow_mut() = line;
                a.refresh_explain_for(&tab);
            }
        });
        // Enter: zapis do historii i ostatniej komendy.
        let w = Rc::downgrade(&self.me());
        let t2 = Rc::downgrade(&tab);
        term.connect_commit(move |_, text, _| {
            let (Some(a), Some(tab)) = (w.upgrade(), t2.upgrade()) else {
                return;
            };
            if text.contains('\r') {
                let cmd = tab.current.borrow().trim().to_owned();
                if !cmd.is_empty() {
                    *tab.last.borrow_mut() = cmd.clone();
                    a.data.borrow_mut().record(&cmd, now());
                    a.save();
                    a.refresh_history();
                }
            }
        });
        let w = Rc::downgrade(&self.me());
        let p2 = page.clone();
        term.connect_child_exited(move |_, _| {
            if let Some(a) = w.upgrade() {
                a.view.close_page(&p2)
            }
        });

        self.view.set_selected_page(&page);
        term.grab_focus();
        self.refresh_side_tabs();
    }

    fn spawn(&self, term: &vte::Terminal, wd: &std::path::Path) {
        let zsh = gtk::glib::find_program_in_path("zsh");
        let shell = zsh
            .clone()
            .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/bin/bash"));
        let shell = shell.to_string_lossy().into_owned();
        let zdot = format!("ZDOTDIR={}", self.zdotdir.display());
        let mut env = vec!["TERM_PROGRAM=Cherry", "COLORTERM=truecolor"];
        if zsh.is_some() {
            env.push(&zdot);
        }
        let wd = wd.to_string_lossy().into_owned();
        term.spawn_async(
            vte::PtyFlags::DEFAULT,
            Some(&wd),
            &[&shell, "-l"],
            &env,
            gtk::glib::SpawnFlags::DEFAULT,
            || {},
            -1,
            gtk::gio::Cancellable::NONE,
            |r| {
                if let Err(e) = r {
                    eprintln!("cherry: nie udało się uruchomić powłoki: {e}");
                }
            },
        );
    }

    fn apply_colors(&self, term: &vte::Terminal) {
        // Terminal jest najciemniejszym elementem okna, w obu motywach.
        let dark = adw::StyleManager::default().is_dark();
        let (fg, bg) = if dark {
            ("#ffffff", "#1d1d20")
        } else {
            ("#f6f5f4", "#241f31")
        };
        let rgba = |s: &str| gtk::gdk::RGBA::parse(s).unwrap();
        let pal: Vec<gtk::gdk::RGBA> = [
            "#241f31", "#c01c28", "#2ec27e", "#f5c211", "#1e78e4", "#9841bb", "#0ab9dc", "#c0bfbc",
            "#5e5c64", "#ed333b", "#57e389", "#f8e45c", "#51a1ff", "#c061cb", "#4fd2fd", "#f6f5f4",
        ]
        .iter()
        .map(|s| rgba(s))
        .collect();
        let refs: Vec<&gtk::gdk::RGBA> = pal.iter().collect();
        term.set_colors(Some(&rgba(fg)), Some(&rgba(bg)), &refs);
    }

    fn update_status(&self, tab: &Tab, page: &adw::TabPage, path: &std::path::Path) {
        let home = gtk::glib::home_dir();
        let short = match path.strip_prefix(&home) {
            Ok(r) if r.as_os_str().is_empty() => "~".to_owned(),
            Ok(r) => format!("~/{}", r.display()),
            Err(_) => path.display().to_string(),
        };
        tab.status_path.set_label(&short);
        tab.status_path
            .set_tooltip_text(Some(&path.display().to_string()));
        match store::git_branch(path) {
            Some(b) => {
                tab.status_git
                    .set_label(&trf("status.git", &[("branch", &b)]));
                tab.git_box.set_visible(true);
            }
            None => tab.git_box.set_visible(false),
        }
        let title = if path == home {
            tr("tab.home").to_owned()
        } else {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into())
        };
        page.set_title(&title);
        self.refresh_side_tabs();
    }

    fn tool_toggled(&self, id: &str, on: bool) {
        if on {
            for (other, b) in &self.tool_btns {
                if *other != id && b.is_active() {
                    b.set_active(false);
                }
            }
            self.panel.set_visible_child_name(id);
            self.panel_rev.set_reveal_child(true);
            if id == "explain" {
                self.refresh_explain();
            }
        } else if !self.tool_btns.iter().any(|(_, b)| b.is_active()) {
            self.panel_rev.set_reveal_child(false);
        }
    }

    fn refresh_explain(&self) {
        if let Some(t) = self.active() {
            self.refresh_explain_for(&t);
        }
    }

    /// Panel pokazuje wpisywaną komendę, a przy pustym znaku zachęty ostatnio wykonaną.
    fn refresh_explain_for(&self, tab: &Tab) {
        if self.active().is_none_or(|a| !std::ptr::eq(a.as_ref(), tab)) {
            return;
        }
        let cur = tab.current.borrow().trim().to_owned();
        let src = if cur.is_empty() {
            tab.last.borrow().clone()
        } else {
            cur
        };
        clear(&self.explain_list);
        let parts = explain::analyze(&src);
        self.explain_cmd.set_label(&src);
        self.explain_cmd.set_visible(!src.is_empty());
        self.explain_empty.set_visible(parts.is_empty());
        if let Some(t) = self.explain_list.prev_sibling() {
            t.set_visible(!parts.is_empty());
        }
        for p in parts {
            let (row, _) = two_line_row(&p.tok, tr_key(p.key));
            row.set_activatable(false);
            match p.kind {
                Kind::Warn => row.add_css_class("cherry-warn"),
                Kind::Hint => row.add_css_class("cherry-hint"),
                Kind::Normal => {}
            }
            self.explain_list.append(&row);
        }
    }

    /// Wpisuje tekst do aktywnego terminala (bez Entera) i oddaje mu fokus.
    fn insert(&self, cmd: &str) {
        if let Some(t) = self.active() {
            t.term.feed_child(cmd.as_bytes());
            t.term.grab_focus();
        }
    }

    fn refresh_notes(&self) {
        clear(&self.notes_list);
        let notes = self.data.borrow().notes.clone();
        for (i, n) in notes.iter().enumerate() {
            let (row, hb) = two_line_row(&n.cmd, &n.text);
            let del = icon_button("user-trash-symbolic", tr("delete"));
            del.set_valign(gtk::Align::Center);
            hb.append(&del);
            let w = Rc::downgrade(&self.me());
            del.connect_clicked(move |_| {
                if let Some(a) = w.upgrade() {
                    if i < a.data.borrow().notes.len() {
                        a.data.borrow_mut().notes.remove(i);
                    }
                    a.save();
                    a.refresh_notes();
                }
            });
            self.notes_list.append(&row);
        }
        let w = Rc::downgrade(&self.me());
        reconnect_activate(&self.notes_list, move |i| {
            if let Some(a) = w.upgrade() {
                let c = a.data.borrow().notes.get(i).map(|n| n.cmd.clone());
                if let Some(c) = c {
                    a.insert(&c)
                }
            }
        });
    }

    fn refresh_shortcuts(&self) {
        clear(&self.keys_list);
        clear(&self.abbr_list);
        let d = self.data.borrow().clone();
        // Skróty klawiszowe: kontroler okna wpisuje komendę do aktywnej karty.
        while let Some(s) = gtk::gio::prelude::ListModelExt::item(&self.shortcuts, 0)
            .and_downcast::<gtk::Shortcut>()
        {
            self.shortcuts.remove_shortcut(&s);
        }
        for (i, k) in d.keys.iter().enumerate() {
            let label = gtk::accelerator_parse(&k.accel)
                .map(|(key, m)| gtk::accelerator_get_label(key, m).to_string())
                .unwrap_or_else(|| k.accel.clone());
            let (row, hb) = two_line_row(&label, &k.cmd);
            self.add_delete(&hb, move |d| {
                if i < d.keys.len() {
                    d.keys.remove(i);
                }
            });
            self.keys_list.append(&row);
            if let Some(trigger) = gtk::ShortcutTrigger::parse_string(&k.accel) {
                let w = Rc::downgrade(&self.me());
                let cmd = k.cmd.clone();
                let action = gtk::CallbackAction::new(move |_, _| {
                    if let Some(a) = w.upgrade() {
                        a.insert(&cmd)
                    }
                    gtk::glib::Propagation::Stop
                });
                self.shortcuts
                    .add_shortcut(gtk::Shortcut::new(Some(trigger), Some(action)));
            }
        }
        for (i, ab) in d.abbrevs.iter().enumerate() {
            let (row, hb) = two_line_row(&ab.abbr, &ab.cmd);
            self.add_delete(&hb, move |d| {
                if i < d.abbrevs.len() {
                    d.abbrevs.remove(i);
                }
            });
            self.abbr_list.append(&row);
        }
        let w = Rc::downgrade(&self.me());
        reconnect_activate(&self.keys_list, move |i| {
            if let Some(a) = w.upgrade() {
                let c = a.data.borrow().keys.get(i).map(|k| k.cmd.clone());
                if let Some(c) = c {
                    a.insert(&c)
                }
            }
        });
        let w = Rc::downgrade(&self.me());
        reconnect_activate(&self.abbr_list, move |i| {
            if let Some(a) = w.upgrade() {
                let c = a.data.borrow().abbrevs.get(i).map(|k| k.abbr.clone());
                if let Some(c) = c {
                    a.insert(&c)
                }
            }
        });
    }

    fn add_delete(&self, hb: &gtk::Box, f: impl Fn(&mut Data) + 'static) {
        let del = icon_button("user-trash-symbolic", tr("delete"));
        del.set_valign(gtk::Align::Center);
        hb.append(&del);
        let w = Rc::downgrade(&self.me());
        del.connect_clicked(move |_| {
            if let Some(a) = w.upgrade() {
                f(&mut a.data.borrow_mut());
                a.save();
                store::write_zsh(&a.data.borrow());
                a.refresh_shortcuts();
            }
        });
    }

    fn refresh_history(&self) {
        clear(&self.hist_list);
        let q = self.hist_search.text().to_lowercase();
        let items: Vec<(String, i64)> = self
            .data
            .borrow()
            .history
            .iter()
            .filter(|h| q.is_empty() || h.cmd.to_lowercase().contains(&q))
            .map(|h| (h.cmd.clone(), h.at))
            .collect();
        if items.is_empty() {
            let l = gtk::Label::builder()
                .label(if q.is_empty() {
                    tr("hist.empty")
                } else {
                    tr("hist.none")
                })
                .margin_top(12)
                .build();
            l.add_css_class("dim-label");
            let row = gtk::ListBoxRow::builder()
                .child(&l)
                .activatable(false)
                .selectable(false)
                .build();
            self.hist_list.append(&row);
        }
        for (cmd, at) in &items {
            let when = gtk::glib::DateTime::from_unix_local(*at)
                .ok()
                .and_then(|d| d.format("%x %H:%M").ok())
                .map(|s| s.to_string())
                .unwrap_or_default();
            let (row, _) = two_line_row(cmd, &when);
            self.hist_list.append(&row);
        }
        let w = Rc::downgrade(&self.me());
        reconnect_activate(&self.hist_list, move |i| {
            if let (Some(a), Some((c, _))) = (w.upgrade(), items.get(i)) {
                a.insert(c)
            }
        });
    }

    fn note_dialog(&self) {
        let cmd = gtk::Entry::builder()
            .placeholder_text(tr("note.cmd"))
            .build();
        cmd.add_css_class("cherry-tok");
        let text = gtk::Entry::builder()
            .placeholder_text(tr("note.text"))
            .build();
        let sugg = list();
        let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
        b.append(&cmd);
        b.append(&sugg);
        b.append(&text);
        // Podpowiedzi: znane komendy i historia zawierające wpisany tekst.
        let history: Vec<String> = self
            .data
            .borrow()
            .history
            .iter()
            .map(|h| h.cmd.clone())
            .collect();
        let c2 = cmd.clone();
        let s2 = sugg.clone();
        cmd.connect_changed(move |e| {
            clear(&s2);
            let q = e.text().trim().to_lowercase();
            if q.is_empty() {
                return;
            }
            let mut seen = Vec::new();
            for s in explain::names()
                .map(str::to_owned)
                .chain(history.iter().cloned())
            {
                if s.to_lowercase().contains(&q) && s != e.text().trim() && !seen.contains(&s) {
                    seen.push(s);
                }
                if seen.len() == 4 {
                    break;
                }
            }
            for s in seen {
                let l = gtk::Label::builder().label(&s).xalign(0.0).build();
                l.add_css_class("cherry-tok");
                let row = gtk::ListBoxRow::builder().child(&l).build();
                let c3 = c2.clone();
                let s3 = s.clone();
                let click = gtk::GestureClick::new();
                click.connect_released(move |_, _, _, _| {
                    c3.set_text(&s3);
                    c3.set_position(-1);
                });
                row.add_controller(click);
                s2.append(&row);
            }
        });
        let d = adw::AlertDialog::builder()
            .heading(tr("note.new"))
            .extra_child(&b)
            .build();
        d.add_responses(&[("cancel", tr("cancel")), ("save", tr("save"))]);
        d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("save"));
        let w = Rc::downgrade(&self.me());
        d.connect_response(None, move |_, r| {
            let Some(a) = w.upgrade() else { return };
            let c = cmd.text().trim().to_owned();
            if r != "save" || c.is_empty() {
                return;
            }
            let t = text.text().trim().to_owned();
            a.data
                .borrow_mut()
                .notes
                .insert(0, Note { cmd: c, text: t });
            a.save();
            a.refresh_notes();
        });
        d.present(Some(&self.window));
    }

    fn shortcut_dialog(&self) {
        let kinds = gtk::DropDown::from_strings(&[tr("short.keys"), tr("short.abbr")]);
        let key = gtk::Entry::builder()
            .placeholder_text(tr("short.key.hint"))
            .build();
        let cmd = gtk::Entry::builder()
            .placeholder_text(tr("note.cmd"))
            .build();
        cmd.add_css_class("cherry-tok");
        let b = gtk::Box::new(gtk::Orientation::Vertical, 8);
        b.append(&kinds);
        b.append(&key);
        b.append(&cmd);
        let k2 = key.clone();
        kinds.connect_selected_notify(move |d| {
            k2.set_placeholder_text(Some(if d.selected() == 0 {
                tr("short.key.hint")
            } else {
                tr("short.abbr.hint")
            }));
        });
        let d = adw::AlertDialog::builder()
            .heading(tr("short.new"))
            .extra_child(&b)
            .build();
        d.add_responses(&[("cancel", tr("cancel")), ("save", tr("save"))]);
        d.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        let w = Rc::downgrade(&self.me());
        d.connect_response(None, move |_, r| {
            let Some(a) = w.upgrade() else { return };
            let (k, c) = (key.text().trim().to_owned(), cmd.text().trim().to_owned());
            if r != "save" || k.is_empty() || c.is_empty() {
                return;
            }
            if kinds.selected() == 0 {
                let Some(accel) = parse_accel(&k) else { return };
                a.data.borrow_mut().keys.push(KeyShortcut { accel, cmd: c });
            } else {
                let ab = Abbrev { abbr: k, cmd: c };
                if store::alias_line(&ab).is_none() {
                    return;
                }
                a.data.borrow_mut().abbrevs.push(ab);
                store::write_zsh(&a.data.borrow());
            }
            a.save();
            a.refresh_shortcuts();
        });
        d.present(Some(&self.window));
    }

    fn set_tabs_pos(&self, p: TabsPos) {
        self.data.borrow_mut().settings.tabs = p;
        self.save();
        self.apply_tabs_pos();
    }

    fn apply_tabs_pos(&self) {
        let right = self.data.borrow().settings.tabs == TabsPos::Right;
        self.bar.set_visible(!right);
        self.side_rev.set_reveal_child(right);
        self.refresh_side_tabs();
    }

    /// Pionowa lista kart po prawej: tytuł i krzyżyk, jak karty u góry.
    fn refresh_side_tabs(&self) {
        clear(&self.side_tabs);
        for i in 0..self.view.n_pages() {
            let page = self.view.nth_page(i);
            let hb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            hb.append(&gtk::Image::from_icon_name("utilities-terminal-symbolic"));
            let l = gtk::Label::builder()
                .label(page.title())
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            hb.append(&l);
            let close = icon_button("window-close-symbolic", tr("tab.close"));
            let v = self.view.clone();
            let p2 = page.clone();
            close.connect_clicked(move |_| v.close_page(&p2));
            hb.append(&close);
            self.side_tabs
                .append(&gtk::ListBoxRow::builder().child(&hb).build());
        }
        self.sync_side_selection();
    }

    fn sync_side_selection(&self) {
        if let Some(p) = self.view.selected_page() {
            let i = self.view.page_position(&p);
            if let Some(r) = self.side_tabs.row_at_index(i) {
                self.side_tabs.select_row(Some(&r));
            }
        }
    }
}

/// Jedno połączenie „row-activated” na listę (lista jest przebudowywana przy każdej zmianie).
fn reconnect_activate(l: &gtk::ListBox, f: impl Fn(usize) + 'static) {
    let id: Option<gtk::glib::SignalHandlerId> = unsafe { l.steal_data("cherry-activate") };
    if let Some(id) = id {
        l.disconnect(id);
    }
    let id = l.connect_row_activated(move |_, r| {
        if r.index() >= 0 {
            f(r.index() as usize)
        }
    });
    unsafe { l.set_data("cherry-activate", id) };
}

/// Tekst w wierszu kursora po znaku zachęty Cherry („❯ ”). Bez znaku zachęty: pusty.
fn typed_line(t: &vte::Terminal) -> String {
    let (_, row) = t.cursor_position();
    let (text, _) = t.text_range_format(vte::Format::Text, row, 0, row, t.column_count());
    let text = text.map(|s| s.to_string()).unwrap_or_default();
    after_prompt(&text)
}

pub fn after_prompt(line: &str) -> String {
    match line.rfind("❯ ") {
        Some(i) => line[i + "❯ ".len()..].trim_end().to_owned(),
        None => String::new(),
    }
}

/// „Alt+G”, „Ctrl+Shift+K” albo zapis GTK („<Alt>g”) → akcelerator GTK.
pub fn parse_accel(s: &str) -> Option<String> {
    let s = s.trim();
    let candidate = if s.starts_with('<') {
        s.to_owned()
    } else {
        let parts: Vec<&str> = s
            .split('+')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .collect();
        let (key, mods) = parts.split_last()?;
        let mut out = String::new();
        for m in mods {
            out.push_str(match m.to_lowercase().as_str() {
                "ctrl" | "control" | "strg" => "<Control>",
                "alt" => "<Alt>",
                "shift" => "<Shift>",
                "super" | "win" | "meta" => "<Super>",
                _ => return None,
            });
        }
        let k = if key.chars().count() == 1 {
            key.to_lowercase()
        } else {
            (*key).to_owned()
        };
        out.push_str(&k);
        out
    };
    gtk::accelerator_parse(&candidate).map(|_| candidate)
}

/// Klucz opisu z rozbioru (zawsze &'static, z tablic `explain`).
fn tr_key(k: &'static str) -> &'static str {
    tr(k)
}

#[cfg(test)]
mod tests {
    #[test]
    fn prompt_extraction() {
        assert_eq!(super::after_prompt("~/Dokumenty ❯ ls -la   "), "ls -la");
        assert_eq!(super::after_prompt("bez znaku"), "");
        assert_eq!(super::after_prompt("~ ❯ echo ❯ x"), "x");
    }
}
