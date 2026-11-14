//! Okno Cherry: pionowy pasek po lewej (pomoc u góry, lista kart, na dole nowa karta i historia), a obok
//! terminal (VTE, zsh) z paskiem tytułu, w którym po prawej są ustawienia i przyciski okna, jak w innych
//! aplikacjach Shrimp. Bez kart poziomych.
//! Skróty jak w Windows: Ctrl+C kopiuje zaznaczenie (bez zaznaczenia przerywa), Ctrl+V wkleja, menu pod
//! prawym przyciskiem; edycję wpisywanej komendy (Shift+strzałki, Ctrl+A, Ctrl+Z…) dokłada .zshrc Cherry.
//! Historię komend zapisuje sam zsh (dokładnie to, co wykonał), Cherry ją tylko pokazuje.

use adw::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use vte::prelude::*;

use crate::i18n::{tr, trf};
use crate::store::{self, Data};

const CSS: &str = r#"
.cherry-side { background: @sidebar_bg_color; }
.cherry-side headerbar { background: transparent; box-shadow: none; }
.cherry-side .navigation-sidebar row { padding-top: 4px; padding-bottom: 4px; }
.cherry-foot { padding: 6px; }
.cherry-term { border-radius: 0; padding: 6px 10px; }
.cherry-tok { font-family: monospace; font-weight: bold; }
.cherry-pop .title-4 { margin: 6px 6px 4px 6px; }
"#;

/// Szerokość paska z kartami.
const SIDE_WIDTH: f64 = 260.0;

/// Stan jednej karty: terminal, podpis w pasku kart i wpisywana komenda.
struct Tab {
    term: vte::Terminal,
    /// Folder i gałąź gita pod tytułem karty.
    subtitle: RefCell<String>,
    dir: RefCell<PathBuf>,
}

pub struct App {
    data: RefCell<Data>,
    window: adw::ApplicationWindow,
    view: adw::TabView,
    side_tabs: gtk::ListBox,
    hist_btn: gtk::MenuButton,
    hist_list: gtk::ListBox,
    hist_search: gtk::SearchEntry,
    tabs: RefCell<HashMap<String, Rc<Tab>>>,
    next_id: Cell<u32>,
    zdotdir: PathBuf,
    me: RefCell<Weak<App>>,
}

thread_local! {
    /// Otwarte okna Cherry (nowa karta z wiersza poleceń trafia do pierwszego z nich).
    static WINDOWS: RefCell<Vec<Weak<App>>> = const { RefCell::new(Vec::new()) };
}

/// Folder z argumentów: `--working-directory DIR`, `--working-directory=DIR` albo jeden argument-folder.
/// Ścieżka względna liczy się od `cwd` procesu, który uruchomił Cherry.
pub fn dir_from_args(args: &[String], cwd: Option<&std::path::Path>) -> Option<PathBuf> {
    let mut it = args.iter();
    let mut raw = None;
    while let Some(a) = it.next() {
        if a == "--working-directory" || a == "-w" {
            raw = it.next().cloned();
        } else if let Some(d) = a.strip_prefix("--working-directory=") {
            raw = Some(d.to_owned());
        } else if !a.starts_with('-') {
            raw = Some(a.clone());
        }
    }
    let p = PathBuf::from(raw?);
    let p = match (p.is_absolute(), cwd) {
        (false, Some(c)) => c.join(p),
        _ => p,
    };
    p.is_dir().then_some(p)
}

/// Nowa karta w `dir` w otwartym oknie albo nowe okno. Bez folderu: pokazuje otwarte okno.
pub fn open(app: &adw::Application, dir: Option<PathBuf>) {
    let win = WINDOWS.with(|w| w.borrow().iter().find_map(Weak::upgrade));
    match win {
        Some(a) => {
            if let Some(d) = dir {
                a.add_tab(d.to_str());
            }
            a.window.present();
        }
        None => build(app, dir),
    }
}

fn now() -> i64 {
    gtk::glib::DateTime::now_utc()
        .map(|d| d.to_unix())
        .unwrap_or(0)
}

fn build(app: &adw::Application, dir: Option<PathBuf>) {
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
    let zdotdir = store::write_zsh();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .default_width(1100)
        .default_height(680)
        .title("Cherry")
        .build();
    let view = adw::TabView::new();
    view.set_hexpand(true);
    view.set_vexpand(true);

    // Pasek boczny: u góry tylko pomoc (przyciski okna są w prawym górnym rogu, nad terminalem).
    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    let help = gtk::Button::builder()
        .icon_name("dialog-question-symbolic")
        .tooltip_text(tr("help"))
        .build();
    help.set_sensitive(crate::help::url().is_some());
    header.pack_start(&help);

    let side_tabs = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .build();
    side_tabs.add_css_class("navigation-sidebar");

    // Na dole paska: nowa karta po lewej, historia po prawej (okienko nad przyciskiem).
    let new_tab = icon_button("list-add-symbolic", tr("tab.new"));
    let hist_list = list();
    let hist_clear = icon_button("user-trash-symbolic", tr("hist.clear"));
    let hist_search = gtk::SearchEntry::builder()
        .placeholder_text(tr("hist.search"))
        .build();
    let hb = gtk::Box::new(gtk::Orientation::Vertical, 6);
    hb.append(&heading_with(tr("tool.history"), &hist_clear));
    hb.append(&hist_search);
    hb.append(&scroll(&hist_list));
    let hist_btn = pop_button("document-open-recent-symbolic", tr("tool.history"), &hb);

    let foot = gtk::CenterBox::new();
    foot.add_css_class("cherry-foot");
    foot.set_start_widget(Some(&new_tab));
    foot.set_end_widget(Some(&hist_btn));

    let side = adw::ToolbarView::new();
    side.add_css_class("cherry-side");
    side.add_top_bar(&header);
    side.set_content(Some(&scroll(&side_tabs)));
    side.add_bottom_bar(&foot);

    // Terminal z własnym paskiem tytułu: ustawienia i przyciski okna w prawym górnym rogu okna.
    let top = adw::HeaderBar::new();
    top.set_show_title(false);
    let menu_btn = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(tr("settings"))
        .build();
    top.pack_end(&menu_btn);
    let main = adw::ToolbarView::new();
    main.add_top_bar(&top);
    main.set_content(Some(&view));

    // Pasek boczny o stałej szerokości, terminal obok.
    let body = adw::OverlaySplitView::builder()
        .sidebar(&side)
        .content(&main)
        .min_sidebar_width(SIDE_WIDTH)
        .max_sidebar_width(SIDE_WIDTH)
        .collapsed(false)
        .show_sidebar(true)
        .build();
    window.set_content(Some(&body));

    let me = Rc::new(App {
        data: RefCell::new(data),
        window: window.clone(),
        view: view.clone(),
        side_tabs: side_tabs.clone(),
        hist_btn,
        hist_list,
        hist_search: hist_search.clone(),
        tabs: RefCell::new(HashMap::new()),
        next_id: Cell::new(1),
        zdotdir,
        me: RefCell::new(Weak::new()),
    });
    *me.me.borrow_mut() = Rc::downgrade(&me);

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
            a.new_tab_here()
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
    // Kolory terminala za motywem (jasny/ciemny).
    let w = Rc::downgrade(&me);
    adw::StyleManager::default().connect_dark_notify(move |_| {
        if let Some(a) = w.upgrade() {
            for t in a.tabs.borrow().values() {
                apply_colors(&t.term);
            }
        }
    });

    let w = Rc::downgrade(&me);
    if let Some(pop) = me.hist_btn.popover() {
        pop.connect_show(move |_| {
            if let Some(a) = w.upgrade() {
                a.ingest_history();
            }
        });
    }

    me.ingest_history();
    me.add_tab(dir.as_deref().and_then(|d| d.to_str()));
    WINDOWS.with(|w| {
        let mut w = w.borrow_mut();
        w.retain(|a| a.upgrade().is_some());
        w.push(Rc::downgrade(&me));
    });
    // Sygnały trzymają słabe odwołania; jedyne mocne żyje tak długo jak okno.
    let keep = RefCell::new(Some(me));
    window.connect_close_request(move |_| {
        keep.borrow_mut().take();
        gtk::glib::Propagation::Proceed
    });
    window.present();
}

fn heading_with(text: &str, btn: &gtk::Button) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let h = gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .hexpand(true)
        .build();
    h.add_css_class("title-4");
    b.append(&h);
    btn.set_valign(gtk::Align::Center);
    b.append(btn);
    b
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
    b.update_property(&[gtk::accessible::Property::Label(tip)]);
    b
}

/// Przycisk z ikoną otwierający okienko (historia) nad sobą.
fn pop_button(icon: &str, tip: &str, content: &gtk::Box) -> gtk::MenuButton {
    content.add_css_class("cherry-pop");
    content.set_size_request(340, 420);
    let pop = gtk::Popover::builder()
        .child(content)
        .position(gtk::PositionType::Top)
        .build();
    let b = gtk::MenuButton::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .popover(&pop)
        .build();
    b.add_css_class("flat");
    b.update_property(&[gtk::accessible::Property::Label(tip)]);
    b
}

/// Wiersz listy: komenda (czcionka stała) i opis pod nią.
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

/// Ustawienia: jedna pionowa lista (dziś: jak długo trzymać historię).
fn settings_popover(app: &Rc<App>) -> gtk::Popover {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 6);
    b.set_margin_top(10);
    b.set_margin_bottom(10);
    b.set_margin_start(10);
    b.set_margin_end(10);
    let title = gtk::Label::builder()
        .label(tr("set.history"))
        .xalign(0.0)
        .build();
    title.add_css_class("heading");
    b.append(&title);
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
    dd.update_property(&[gtk::accessible::Property::Label(tr("set.history"))]);
    let cur = app.data.borrow().settings.history_days;
    dd.set_selected(
        store::HISTORY_CHOICES
            .iter()
            .position(|c| *c == cur)
            .unwrap_or(0) as u32,
    );
    b.append(&dd);
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

/// Terminal jest najciemniejszym elementem okna, w obu motywach.
fn apply_colors(term: &vte::Terminal) {
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

/// Klawisz okna Cherry (nie trafia do programu w terminalu).
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum KeyAction {
    /// Ctrl+C przy zaznaczeniu, Ctrl+Shift+C.
    Copy,
    /// Ctrl+V, Ctrl+Shift+V, Shift+Insert.
    Paste,
    /// Ctrl+Shift+A.
    SelectAll,
    /// Ctrl+Shift+T.
    NewTab,
    /// Ctrl+Shift+W.
    CloseTab,
    /// Ctrl+Tab, Ctrl+PageDown.
    NextTab,
    /// Ctrl+Shift+Tab, Ctrl+PageUp.
    PrevTab,
    ZoomIn,
    ZoomOut,
    ZoomReset,
}

/// Skróty okna jak w Windows Terminal. `selection`: czy w terminalu jest zaznaczony tekst; bez niego
/// Ctrl+C idzie do programu (przerwanie albo kopiowanie zaznaczenia w wierszu komendy, zob. .zshrc).
pub fn key_action(
    key: gtk::gdk::Key,
    mods: gtk::gdk::ModifierType,
    selection: bool,
) -> Option<KeyAction> {
    use gtk::gdk::{Key, ModifierType as M};
    let m = mods & (M::CONTROL_MASK | M::SHIFT_MASK | M::ALT_MASK | M::SUPER_MASK);
    let ctrl = m == M::CONTROL_MASK;
    let ctrl_shift = m == M::CONTROL_MASK | M::SHIFT_MASK;
    let k = key.to_lower();
    Some(match k {
        Key::c if ctrl && selection => KeyAction::Copy,
        Key::c if ctrl_shift => KeyAction::Copy,
        Key::v if ctrl || ctrl_shift => KeyAction::Paste,
        Key::Insert if m == M::SHIFT_MASK => KeyAction::Paste,
        Key::a if ctrl_shift => KeyAction::SelectAll,
        Key::t if ctrl_shift => KeyAction::NewTab,
        Key::w if ctrl_shift => KeyAction::CloseTab,
        Key::Tab | Key::Page_Down if ctrl => KeyAction::NextTab,
        Key::ISO_Left_Tab | Key::Tab if ctrl_shift => KeyAction::PrevTab,
        Key::Page_Up if ctrl => KeyAction::PrevTab,
        Key::plus | Key::equal | Key::KP_Add if ctrl || ctrl_shift => KeyAction::ZoomIn,
        Key::minus | Key::KP_Subtract if ctrl => KeyAction::ZoomOut,
        Key::_0 | Key::KP_0 if ctrl => KeyAction::ZoomReset,
        _ => return None,
    })
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

    /// Nowa karta w folderze bieżącej karty.
    fn new_tab_here(&self) {
        let dir = self.active().map(|t| t.dir.borrow().clone());
        self.add_tab(dir.as_deref().and_then(|d| d.to_str()));
    }

    /// Nowa karta z terminalem w podanym folderze (domyślnie folder domowy).
    fn add_tab(&self, dir: Option<&str>) {
        let id = format!("tab{}", self.next_id.get());
        self.next_id.set(self.next_id.get() + 1);

        let term = vte::Terminal::new();
        term.set_widget_name(&id);
        term.add_css_class("cherry-term");
        term.set_scrollback_lines(10_000);
        term.set_vexpand(true);
        term.set_hexpand(true);
        term.set_font(Some(&gtk::pango::FontDescription::from_string(
            "Monospace 11",
        )));
        apply_colors(&term);
        self.add_context_menu(&term);
        self.add_keys(&term);

        let home = gtk::glib::home_dir();
        let wd = dir.map(PathBuf::from).unwrap_or_else(|| home.clone());
        let tab = Rc::new(Tab {
            term: term.clone(),
            subtitle: RefCell::new(String::new()),
            dir: RefCell::new(wd.clone()),
        });
        self.tabs.borrow_mut().insert(id.clone(), tab.clone());
        let page = self.view.append(&term);
        page.set_title(tr("tab.home"));
        self.update_dir(&tab, &page, &wd);
        self.spawn(&term, &wd);

        // Bieżący folder (OSC 7 z .zshrc Cherry): tytuł karty, folder i gałąź gita pod nim.
        let w = Rc::downgrade(&self.me());
        let (t2, p2) = (Rc::downgrade(&tab), page.clone());
        term.connect_current_directory_uri_changed(move |t| {
            let (Some(a), Some(tab)) = (w.upgrade(), t2.upgrade()) else {
                return;
            };
            if let Some(uri) = t.current_directory_uri() {
                if let Ok((path, _)) = gtk::glib::filename_from_uri(&uri) {
                    a.update_dir(&tab, &p2, &path);
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

    /// Menu pod prawym przyciskiem: Kopiuj, Wklej, Zaznacz wszystko.
    fn add_context_menu(&self, term: &vte::Terminal) {
        let group = gtk::gio::SimpleActionGroup::new();
        let copy = gtk::gio::SimpleAction::new("copy", None);
        let t = term.downgrade();
        copy.connect_activate(move |_, _| {
            if let Some(t) = t.upgrade() {
                t.copy_clipboard_format(vte::Format::Text);
            }
        });
        let t = term.downgrade();
        let c2 = copy.clone();
        term.connect_selection_changed(move |_| {
            if let Some(t) = t.upgrade() {
                c2.set_enabled(t.has_selection());
            }
        });
        copy.set_enabled(false);
        group.add_action(&copy);
        let paste = gtk::gio::SimpleAction::new("paste", None);
        let t = term.downgrade();
        paste.connect_activate(move |_, _| {
            if let Some(t) = t.upgrade() {
                t.paste_clipboard();
            }
        });
        group.add_action(&paste);
        let all = gtk::gio::SimpleAction::new("select-all", None);
        let t = term.downgrade();
        all.connect_activate(move |_, _| {
            if let Some(t) = t.upgrade() {
                t.select_all();
            }
        });
        group.add_action(&all);
        term.insert_action_group("term", Some(&group));
        let menu = gtk::gio::Menu::new();
        menu.append(Some(tr("menu.copy")), Some("term.copy"));
        menu.append(Some(tr("menu.paste")), Some("term.paste"));
        menu.append(Some(tr("menu.select-all")), Some("term.select-all"));
        term.set_context_menu_model(Some(&menu));
    }

    /// Skróty okna na terminalu (przed VTE, żeby Ctrl+V nie trafił do programu jako znak).
    fn add_keys(&self, term: &vte::Terminal) {
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        let w = Rc::downgrade(&self.me());
        let t = term.downgrade();
        keys.connect_key_pressed(move |_, key, _, mods| {
            let (Some(a), Some(t)) = (w.upgrade(), t.upgrade()) else {
                return gtk::glib::Propagation::Proceed;
            };
            let Some(act) = key_action(key, mods, t.has_selection()) else {
                return gtk::glib::Propagation::Proceed;
            };
            match act {
                KeyAction::Copy => {
                    if t.has_selection() {
                        t.copy_clipboard_format(vte::Format::Text);
                        t.unselect_all();
                    }
                }
                KeyAction::Paste => t.paste_clipboard(),
                KeyAction::SelectAll => t.select_all(),
                KeyAction::NewTab => a.new_tab_here(),
                KeyAction::CloseTab => {
                    if let Some(p) = a.view.selected_page() {
                        a.view.close_page(&p);
                    }
                }
                KeyAction::NextTab => {
                    if !a.view.select_next_page() && a.view.n_pages() > 1 {
                        a.view.set_selected_page(&a.view.nth_page(0));
                    }
                }
                KeyAction::PrevTab => {
                    if !a.view.select_previous_page() && a.view.n_pages() > 1 {
                        a.view
                            .set_selected_page(&a.view.nth_page(a.view.n_pages() - 1));
                    }
                }
                KeyAction::ZoomIn => t.set_font_scale((t.font_scale() + 0.1).min(3.0)),
                KeyAction::ZoomOut => t.set_font_scale((t.font_scale() - 0.1).max(0.5)),
                KeyAction::ZoomReset => t.set_font_scale(1.0),
            }
            gtk::glib::Propagation::Stop
        });
        term.add_controller(keys);
    }

    fn spawn(&self, term: &vte::Terminal, wd: &std::path::Path) {
        let zsh = gtk::glib::find_program_in_path("zsh");
        let shell = zsh
            .clone()
            .or_else(|| std::env::var_os("SHELL").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/bin/bash"));
        let shell = shell.to_string_lossy().into_owned();
        let zdot = format!("ZDOTDIR={}", self.zdotdir.display());
        let hist = format!("CHERRY_HISTORY={}", store::commands_file().display());
        let mut env = vec!["TERM_PROGRAM=Cherry", "COLORTERM=truecolor"];
        if zsh.is_some() {
            env.push(&zdot);
            env.push(&hist);
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

    fn update_dir(&self, tab: &Tab, page: &adw::TabPage, path: &std::path::Path) {
        let home = gtk::glib::home_dir();
        let mut sub = short_path(path, &home);
        if let Some(b) = store::git_branch(path) {
            sub = format!("{sub} · {}", trf("status.git", &[("branch", &b)]));
        }
        *tab.subtitle.borrow_mut() = sub;
        *tab.dir.borrow_mut() = path.to_path_buf();
        let title = if path == home {
            tr("tab.home").to_owned()
        } else {
            path.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "/".into())
        };
        page.set_title(&title);
        page.set_tooltip(&path.display().to_string());
        self.refresh_side_tabs();
    }

    /// Komendy, które zsh zapisał od ostatniego razu (zshaddhistory w .zshrc Cherry), trafiają do historii.
    fn ingest_history(&self) {
        let text = store::take_commands();
        if !text.is_empty() {
            self.data.borrow_mut().ingest(&text, now());
            self.save();
        }
        self.refresh_history();
    }

    /// Wpisuje tekst do aktywnego terminala (bez Entera) i oddaje mu fokus.
    fn insert(&self, cmd: &str) {
        if let Some(t) = self.active() {
            t.term.feed_child(cmd.as_bytes());
            t.term.grab_focus();
        }
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
                .wrap(true)
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
                a.hist_btn.popdown();
                a.insert(c)
            }
        });
    }

    /// Pionowa lista kart: tytuł (folder), pod nim ścieżka i gałąź gita, krzyżyk.
    fn refresh_side_tabs(&self) {
        clear(&self.side_tabs);
        for i in 0..self.view.n_pages() {
            let page = self.view.nth_page(i);
            let sub = self
                .tabs
                .borrow()
                .get(page.child().widget_name().as_str())
                .map(|t| t.subtitle.borrow().clone())
                .unwrap_or_default();
            let hb = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            hb.append(&gtk::Image::from_icon_name("utilities-terminal-symbolic"));
            let vb = gtk::Box::new(gtk::Orientation::Vertical, 0);
            vb.set_hexpand(true);
            vb.set_valign(gtk::Align::Center);
            let l = gtk::Label::builder()
                .label(page.title())
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            vb.append(&l);
            if !sub.is_empty() {
                let s = gtk::Label::builder()
                    .label(&sub)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::Start)
                    .build();
                s.add_css_class("dim-label");
                s.add_css_class("caption");
                vb.append(&s);
            }
            hb.append(&vb);
            let close = icon_button("window-close-symbolic", tr("tab.close"));
            close.set_valign(gtk::Align::Center);
            let v = self.view.clone();
            let p2 = page.clone();
            close.connect_clicked(move |_| v.close_page(&p2));
            hb.append(&close);
            let row = gtk::ListBoxRow::builder().child(&hb).build();
            row.set_tooltip_text(page.tooltip().as_deref());
            self.side_tabs.append(&row);
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

/// `~`, `~/Dokumenty` albo pełna ścieżka poza folderem domowym.
pub fn short_path(path: &std::path::Path, home: &std::path::Path) -> String {
    match path.strip_prefix(home) {
        Ok(r) if r.as_os_str().is_empty() => "~".to_owned(),
        Ok(r) => format!("~/{}", r.display()),
        Err(_) => path.display().to_string(),
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

#[cfg(test)]
mod tests {
    use super::{key_action, short_path, KeyAction};
    use gtk::gdk::{Key, ModifierType as M};
    use std::path::Path;

    #[test]
    fn windows_keys() {
        let c = M::CONTROL_MASK;
        let cs = M::CONTROL_MASK | M::SHIFT_MASK;
        // Ctrl+C kopiuje tylko zaznaczenie; bez niego idzie do programu (przerwanie).
        assert_eq!(key_action(Key::c, c, true), Some(KeyAction::Copy));
        assert_eq!(key_action(Key::c, c, false), None);
        assert_eq!(key_action(Key::C, cs, false), Some(KeyAction::Copy));
        assert_eq!(key_action(Key::v, c, false), Some(KeyAction::Paste));
        assert_eq!(key_action(Key::V, cs, false), Some(KeyAction::Paste));
        assert_eq!(key_action(Key::Insert, M::SHIFT_MASK, false), Some(KeyAction::Paste));
        // Ctrl+A, Ctrl+Z, Ctrl+X zostają dla wiersza komendy (zsh) i programów.
        assert_eq!(key_action(Key::a, c, false), None);
        assert_eq!(key_action(Key::z, c, false), None);
        assert_eq!(key_action(Key::x, c, true), None);
        assert_eq!(key_action(Key::T, cs, false), Some(KeyAction::NewTab));
        assert_eq!(key_action(Key::ISO_Left_Tab, cs, false), Some(KeyAction::PrevTab));
        assert_eq!(key_action(Key::Tab, c, false), Some(KeyAction::NextTab));
        // Caps Lock i NumLock nie przeszkadzają.
        assert_eq!(key_action(Key::v, c | M::LOCK_MASK, false), Some(KeyAction::Paste));
        assert_eq!(key_action(Key::v, M::ALT_MASK | c, false), None);
    }

    #[test]
    fn folder_from_args() {
        let tmp = std::env::temp_dir();
        let t = tmp.to_string_lossy().into_owned();
        let d = |a: &[&str]| {
            let v: Vec<String> = a.iter().map(|s| s.to_string()).collect();
            super::dir_from_args(&v, Some(Path::new("/")))
        };
        assert_eq!(d(&["--working-directory", &t]), Some(tmp.clone()));
        assert_eq!(d(&[&format!("--working-directory={t}")]), Some(tmp.clone()));
        assert_eq!(d(&[&t]), Some(tmp.clone()));
        assert_eq!(d(&[]), None);
        assert_eq!(d(&["/nie/ma/takiego/folderu"]), None);
        assert_eq!(d(&["etc"]), Some(Path::new("/etc").to_path_buf()));
    }

    #[test]
    fn paths() {
        let home = Path::new("/home/ola");
        assert_eq!(short_path(home, home), "~");
        assert_eq!(short_path(Path::new("/home/ola/Dokumenty"), home), "~/Dokumenty");
        assert_eq!(short_path(Path::new("/etc"), home), "/etc");
    }
}
