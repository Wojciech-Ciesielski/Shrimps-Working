//! Okno Shrimp Center: pasek boczny (Systemy, Wygląd) i strony ustawień w stylu Cocktail i Panko.

use std::cell::{Cell, RefCell};
use std::process::Command;
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::efi::{self, Kind, State};
use crate::i18n::{tr, trf};
use crate::tweaks::{self, Store};

const PAGES: [(&str, &str, &str); 2] = [
    ("systemy", "page.systems", "computer-symbolic"),
    ("wyglad", "page.appearance", "applications-graphics-symbolic"),
];

const CSS: &str = r#"
.center-side { background-color: @sidebar_bg_color; }
.center-side > headerbar, .center-content > headerbar { background: transparent; box-shadow: none; }
.center-side .navigation-sidebar { background: transparent; padding: 0 8px 12px 8px; }
.center-side .navigation-sidebar > row { min-height: 36px; font-weight: 600; }
.center-content { background-color: @window_bg_color; }
.center-sample { font-size: 0.85em; opacity: 0.7; padding: 0 24px 12px; }
.accent-dot { min-width: 24px; min-height: 24px; padding: 0; border-radius: 999px; }
.accent-dot:checked { outline: 2px solid @window_fg_color; outline-offset: 2px; }
"#;

struct App {
    demo: bool,
    window: adw::ApplicationWindow,
    toasts: adw::ToastOverlay,
    systems: adw::PreferencesPage,
    groups: RefCell<Vec<adw::PreferencesGroup>>,
    efi: RefCell<State>,
}

/// Uruchamia `work` w wątku i oddaje wynik do `done` w wątku interfejsu.
fn run<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static, done: impl FnOnce(T) + 'static) {
    glib::spawn_future_local(async move {
        if let Ok(v) = gio::spawn_blocking(work).await {
            done(v);
        }
    });
}

/// Polecenie z wynikiem; anulowane okno hasła (pkexec 126/127) to pusty błąd, bez komunikatu.
fn command(prog: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(prog).args(args).output().map_err(|e| format!("{prog}: {e}"))?;
    match out.status.code() {
        Some(0) => Ok(()),
        Some(126 | 127) if prog == "pkexec" => Err(String::new()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().to_owned()),
    }
}

fn self_exe() -> String {
    // Reguła polkit dotyczy /usr/bin/shrimp-center; przy `cargo run` ścieżka jest inna (i okno hasła ogólne).
    std::env::current_exe()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "/usr/bin/shrimp-center".into())
}

pub fn build(app: &adw::Application, demo: bool) {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);
    let mut css = String::new();
    for (name, hex) in tweaks::ACCENTS {
        css += &format!(".accent-{name}, .accent-{name}:hover, .accent-{name}:checked {{ background: {hex}; }}\n");
    }
    let colors = gtk::CssProvider::new();
    colors.load_from_string(&css);
    let display = gtk::gdk::Display::default().expect("ekran");
    for p in [&provider, &colors] {
        gtk::style_context_add_provider_for_display(&display, p, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    }

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Shrimp Center")
        .default_width(860)
        .default_height(640)
        .width_request(360)
        .height_request(400)
        .build();

    // Pasek boczny od góry do dołu okna, treść z własnym paskiem (przycisk panelu, menu, przyciski okna).
    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    header.set_show_end_title_buttons(false);
    let content_header = adw::HeaderBar::new();
    content_header.set_show_title(false);
    content_header.set_show_start_title_buttons(false);
    let side_btn = gtk::ToggleButton::builder()
        .icon_name("sidebar-show-symbolic")
        .tooltip_text(tr("view.sidebar"))
        .css_classes(["flat"])
        .build();
    content_header.pack_start(&side_btn);
    let menu = gio::Menu::new();
    menu.append(Some(tr("menu.about")), Some("win.about"));
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text(tr("menu.main"))
        .menu_model(&menu)
        .build();
    content_header.pack_end(&menu_button);

    let sidebar = gtk::ListBox::new();
    sidebar.add_css_class("navigation-sidebar");
    sidebar.set_vexpand(true);
    for (_, title, icon) in PAGES {
        let b = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        b.append(&gtk::Image::from_icon_name(icon));
        b.append(&gtk::Label::new(Some(tr(title))));
        sidebar.append(&b);
    }
    let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
    side.add_css_class("center-side");
    side.append(&header);
    side.append(&sidebar);

    let systems = adw::PreferencesPage::new();
    let store = Rc::new(Store::new(demo));
    let appearance = adw::PreferencesPage::new();

    let pages = gtk::Stack::new();
    pages.set_transition_type(gtk::StackTransitionType::Crossfade);
    pages.set_vexpand(true);
    pages.add_named(&systems, Some(PAGES[0].0));
    pages.add_named(&appearance, Some(PAGES[1].0));
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&pages));
    toasts.set_vexpand(true);

    let right = gtk::Box::new(gtk::Orientation::Vertical, 0);
    right.add_css_class("center-content");
    right.set_hexpand(true);
    right.append(&content_header);
    right.append(&toasts);
    if demo {
        let sample = gtk::Label::new(Some(tr("demo.note")));
        sample.add_css_class("center-sample");
        sample.set_xalign(0.0);
        right.append(&sample);
    }

    let split = adw::OverlaySplitView::new();
    split.set_sidebar(Some(&side));
    split.set_content(Some(&right));
    split.set_min_sidebar_width(200.0);
    split.set_max_sidebar_width(240.0);
    split
        .bind_property("show-sidebar", &side_btn, "active")
        .bidirectional()
        .sync_create()
        .build();
    // Wąskie okno: panel boczny nad treścią.
    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        560.0,
        adw::LengthUnit::Sp,
    ));
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);
    window.set_content(Some(&split));

    let this = Rc::new(App {
        demo,
        window: window.clone(),
        toasts,
        systems,
        groups: RefCell::new(vec![]),
        efi: RefCell::new(State::default()),
    });

    {
        let pages = pages.clone();
        let split = split.clone();
        sidebar.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                pages.set_visible_child_name(PAGES[row.index().max(0) as usize].0);
            }
            if split.is_collapsed() {
                split.set_show_sidebar(false);
            }
        });
    }
    // SHRIMP_CENTER_PAGE=wyglad: od razu druga strona (podgląd, testy).
    let start = std::env::var("SHRIMP_CENTER_PAGE")
        .ok()
        .and_then(|p| PAGES.iter().position(|(id, _, _)| *id == p))
        .unwrap_or(0);
    sidebar.select_row(sidebar.row_at_index(start as i32).as_ref());

    // „O Shrimp Center” otwiera stronę programu na stronie Shrimp.
    let about = gio::SimpleAction::new("about", None);
    {
        let win = window.clone();
        about.connect_activate(move |_, _| {
            if let Some(url) = crate::help::url() {
                gtk::UriLauncher::new(&url).launch(Some(&win), gio::Cancellable::NONE, |_| {});
            }
        });
    }
    about.set_enabled(crate::help::url().is_some());
    window.add_action(&about);

    build_appearance(&this, &appearance, &store);
    // Okno trzyma stan aplikacji; sygnały mają tylko słabe odwołania.
    unsafe {
        window.set_data("shrimp-center-app", this.clone());
    }
    this.refresh_systems();
    window.present();
}

impl App {
    fn toast(&self, text: &str) {
        self.toasts.add_toast(adw::Toast::new(text));
    }

    fn error(&self, e: &str) {
        if !e.is_empty() {
            self.toast(&trf("err.action", &[("error", e)]));
        }
    }

    fn refresh_systems(self: &Rc<Self>) {
        let demo = self.demo;
        let weak = Rc::downgrade(self);
        run(
            move || {
                if demo {
                    Ok(efi::demo())
                } else if !efi::available() {
                    Err(tr("err.efi").to_owned())
                } else {
                    efi::read()
                }
            },
            move |r| {
                if let Some(app) = weak.upgrade() {
                    app.fill_systems(r);
                }
            },
        );
    }

    /// Nazwa dla człowieka: „Shrimp”, „Windows”, przy dwóch takich samych z nazwą wpisu.
    fn label(st: &State, e: &efi::Entry) -> String {
        let base = match e.kind {
            Kind::Shrimp => "Shrimp",
            Kind::Windows => "Windows",
            Kind::Other => return e.name.clone(),
        };
        if st.entries.iter().filter(|x| x.kind == e.kind).count() > 1 {
            format!("{base} ({})", e.name)
        } else {
            base.to_owned()
        }
    }

    fn fill_systems(self: &Rc<Self>, r: Result<State, String>) {
        for g in self.groups.borrow_mut().drain(..) {
            self.systems.remove(&g);
        }
        let mut groups = vec![];
        let restart = adw::PreferencesGroup::builder().title(tr("restart.title")).build();
        groups.push(restart.clone());
        match r {
            Err(e) => restart.set_description(Some(&e)),
            Ok(st) => {
                let default_name = st.default_entry().map(|e| Self::label(&st, e)).unwrap_or_else(|| "Shrimp".into());
                let others: Vec<&efi::Entry> = st
                    .entries
                    .iter()
                    .filter(|e| Some(&e.id) != st.current.as_ref() && !(st.current.is_none() && e.kind == Kind::Shrimp))
                    .collect();
                if others.is_empty() {
                    restart.set_description(Some(tr("only.shrimp")));
                }
                for e in others {
                    let name = Self::label(&st, e);
                    let row = adw::ActionRow::builder()
                        .title(&name)
                        .subtitle(trf("restart.once.sub", &[("name", &default_name)]))
                        .build();
                    let icon = if e.kind == Kind::Windows { "computer-symbolic" } else { "drive-harddisk-symbolic" };
                    row.add_prefix(&gtk::Image::from_icon_name(icon));
                    let b = gtk::Button::builder().label(tr("restart.button")).valign(gtk::Align::Center).build();
                    let weak = Rc::downgrade(self);
                    let id = e.id.clone();
                    b.connect_clicked(move |_| {
                        if let Some(app) = weak.upgrade() {
                            let heading = trf("confirm.heading", &[("name", &name)]);
                            let id = id.clone();
                            app.confirm(&heading, move |app| app.reboot_into(id.clone()));
                        }
                    });
                    row.add_suffix(&b);
                    row.set_activatable_widget(Some(&b));
                    restart.add(&row);
                }
                restart.add(&self.firmware_row());

                if st.entries.len() >= 2 {
                    let def = adw::PreferencesGroup::new();
                    let labels: Vec<String> = st.entries.iter().map(|e| Self::label(&st, e)).collect();
                    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                    let combo = adw::ComboRow::builder()
                        .title(tr("default.title"))
                        .subtitle(tr("default.sub"))
                        .model(&gtk::StringList::new(&refs))
                        .build();
                    let cur = st
                        .default_entry()
                        .and_then(|d| st.entries.iter().position(|e| e.id == d.id))
                        .unwrap_or(0);
                    combo.set_selected(cur as u32);
                    let weak = Rc::downgrade(self);
                    combo.connect_selected_notify(move |c| {
                        let Some(app) = weak.upgrade() else { return };
                        let id = app.efi.borrow().entries.get(c.selected() as usize).map(|e| e.id.clone());
                        if let Some(id) = id {
                            app.set_default(id);
                        }
                    });
                    def.add(&combo);
                    groups.push(def);
                }
                *self.efi.borrow_mut() = st;
            }
        }
        for g in &groups {
            self.systems.add(g);
        }
        *self.groups.borrow_mut() = groups;
    }

    fn firmware_row(self: &Rc<Self>) -> adw::ActionRow {
        let row = adw::ActionRow::builder()
            .title(tr("restart.firmware"))
            .subtitle(tr("restart.firmware.sub"))
            .build();
        row.add_prefix(&gtk::Image::from_icon_name("emblem-system-symbolic"));
        let b = gtk::Button::builder().label(tr("restart.button")).valign(gtk::Align::Center).build();
        let weak = Rc::downgrade(self);
        b.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.confirm(tr("confirm.firmware"), |app| {
                    if app.demo {
                        app.toast(tr("demo.note"));
                        return;
                    }
                    let weak = Rc::downgrade(app);
                    run(
                        || command("systemctl", &["reboot", "--firmware-setup"]),
                        move |r| {
                            if let (Some(app), Err(e)) = (weak.upgrade(), r) {
                                app.error(&e);
                            }
                        },
                    );
                });
            }
        });
        row.add_suffix(&b);
        row.set_activatable_widget(Some(&b));
        row
    }

    fn confirm(self: &Rc<Self>, heading: &str, yes: impl Fn(&Rc<App>) + 'static) {
        let d = adw::AlertDialog::new(Some(heading), Some(tr("confirm.body")));
        d.add_responses(&[("cancel", tr("cancel")), ("ok", tr("restart.button"))]);
        d.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
        d.set_default_response(Some("ok"));
        d.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        d.connect_response(None, move |_, r| {
            if let (Some(app), "ok") = (weak.upgrade(), r) {
                yes(&app);
            }
        });
        d.present(Some(&self.window));
    }

    /// BootNext na wybrany system (pkexec) i restart; po nim komputer wraca do domyślnego systemu.
    fn reboot_into(self: &Rc<Self>, id: String) {
        if self.demo {
            self.toast(tr("demo.note"));
            return;
        }
        let exe = self_exe();
        let weak = Rc::downgrade(self);
        run(
            move || {
                command("pkexec", &[&exe, "--boot-next", &id])?;
                command("systemctl", &["reboot"])
            },
            move |r| {
                if let (Some(app), Err(e)) = (weak.upgrade(), r) {
                    app.error(&e);
                }
            },
        );
    }

    fn set_default(self: &Rc<Self>, id: String) {
        if self.demo {
            let mut st = self.efi.borrow_mut();
            st.order = st.order_with_first(&id);
            return;
        }
        let exe = self_exe();
        let weak = Rc::downgrade(self);
        run(
            move || command("pkexec", &[&exe, "--default", &id]),
            move |r| {
                let Some(app) = weak.upgrade() else { return };
                if let Err(e) = r {
                    app.error(&e);
                }
                // Zawsze od nowa z UEFI: po błędzie lista wraca do prawdziwego stanu.
                app.refresh_systems();
            },
        );
    }
}

/// Wiersz z listą, połączony z kluczem GSettings w obie strony (zmiana gdzie indziej też się pokazuje).
fn combo(
    title: &str,
    labels: &[&str],
    s: &gio::Settings,
    key: &str,
    read: impl Fn(&gio::Settings) -> u32 + 'static,
    write: impl Fn(&gio::Settings, u32) + 'static,
) -> adw::ComboRow {
    let row = adw::ComboRow::builder()
        .title(title)
        .model(&gtk::StringList::new(labels))
        .build();
    let busy = Rc::new(Cell::new(true));
    row.set_selected(read(s));
    busy.set(false);
    {
        let s = s.clone();
        let busy = busy.clone();
        row.connect_selected_notify(move |r| {
            if !busy.get() {
                write(&s, r.selected());
            }
        });
    }
    let weak = row.downgrade();
    s.connect_changed(Some(key), move |s, _| {
        if let Some(row) = weak.upgrade() {
            busy.set(true);
            row.set_selected(read(s));
            busy.set(false);
        }
    });
    row
}

fn build_appearance(app: &Rc<App>, page: &adw::PreferencesPage, store: &Rc<Store>) {
    // Pulpit: styl, kolor akcentu, animacje.
    if let Some(s) = &store.interface {
        let g = adw::PreferencesGroup::builder().title(tr("look.group.desktop")).build();
        if tweaks::has(s, "color-scheme") {
            g.add(&combo(
                tr("look.style"),
                &[tr("look.light"), tr("look.dark")],
                s,
                "color-scheme",
                |s| u32::from(s.string("color-scheme") == "prefer-dark"),
                |s, i| {
                    let _ = s.set_string("color-scheme", if i == 1 { "prefer-dark" } else { "default" });
                },
            ));
        }
        if tweaks::has(s, "accent-color") {
            g.add(&accent_row(s));
        }
        if tweaks::has(s, "enable-animations") {
            let row = adw::SwitchRow::builder().title(tr("look.animations")).build();
            s.bind("enable-animations", &row, "active").build();
            g.add(&row);
        }
        page.add(&g);

        // Skalowanie: wielkość tekstu tutaj, skala ekranu w Ustawieniach GNOME (zależy od monitora).
        let g = adw::PreferencesGroup::builder().title(tr("look.group.scale")).build();
        if tweaks::has(s, "text-scaling-factor") {
            let labels: Vec<String> = tweaks::TEXT_SCALES.iter().map(|f| format!("{} %", (f * 100.0).round())).collect();
            let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
            g.add(&combo(
                tr("look.text"),
                &refs,
                s,
                "text-scaling-factor",
                |s| tweaks::nearest(&tweaks::TEXT_SCALES, s.double("text-scaling-factor")),
                |s, i| {
                    let _ = s.set_double("text-scaling-factor", tweaks::TEXT_SCALES[i as usize]);
                },
            ));
        }
        let display = adw::ActionRow::builder()
            .title(tr("look.display"))
            .subtitle(tr("look.display.sub"))
            .activatable(true)
            .build();
        display.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
        let weak = Rc::downgrade(app);
        display.connect_activated(move |_| {
            if let Err(e) = Command::new("gnome-control-center").arg("display").spawn() {
                if let Some(app) = weak.upgrade() {
                    app.error(&e.to_string());
                }
            }
        });
        g.add(&display);
        page.add(&g);
    }

    // Dok (Dash to Dock).
    if let Some(s) = &store.dock {
        let g = adw::PreferencesGroup::builder().title(tr("dock.group")).build();
        if tweaks::has(s, "dock-position") {
            g.add(&combo(
                tr("dock.position"),
                &[tr("dock.bottom"), tr("dock.left"), tr("dock.right")],
                s,
                "dock-position",
                |s| tweaks::index_of(&tweaks::DOCK_POSITIONS, &s.string("dock-position"), 0),
                |s, i| {
                    let _ = s.set_string("dock-position", tweaks::DOCK_POSITIONS[i as usize]);
                },
            ));
        }
        if tweaks::has(s, "dash-max-icon-size") {
            g.add(&combo(
                tr("dock.size"),
                &[tr("size.small"), tr("size.medium"), tr("size.large")],
                s,
                "dash-max-icon-size",
                |s| tweaks::nearest(&tweaks::DOCK_SIZES, f64::from(s.int("dash-max-icon-size"))),
                |s, i| {
                    let _ = s.set_int("dash-max-icon-size", tweaks::DOCK_SIZES[i as usize]);
                },
            ));
        }
        if tweaks::has(s, "dock-fixed") && tweaks::has(s, "intellihide") {
            let row = adw::SwitchRow::builder()
                .title(tr("dock.autohide"))
                .subtitle(tr("dock.autohide.sub"))
                .build();
            s.bind("dock-fixed", &row, "active").invert_boolean().build();
            let st = s.clone();
            row.connect_active_notify(move |r| {
                if r.is_active() && !st.boolean("intellihide") {
                    let _ = st.set_boolean("intellihide", true);
                }
            });
            g.add(&row);
        }
        if tweaks::has(s, "show-trash") {
            let row = adw::SwitchRow::builder().title(tr("dock.trash")).build();
            s.bind("show-trash", &row, "active").build();
            g.add(&row);
        }
        page.add(&g);
    }

    // Przyciski okna.
    if let Some(s) = store.wm.as_ref().filter(|s| tweaks::has(s, "button-layout")) {
        let g = adw::PreferencesGroup::builder().title(tr("win.group")).build();
        g.add(&combo(
            tr("win.layout"),
            &[tr("win.close"), tr("win.right"), tr("win.left")],
            s,
            "button-layout",
            |s| tweaks::index_of(&tweaks::BUTTONS, &s.string("button-layout"), 1),
            |s, i| {
                let _ = s.set_string("button-layout", tweaks::BUTTONS[i as usize]);
            },
        ));
        page.add(&g);
    }

    // Czcionki.
    if let Some(s) = &store.interface {
        let g = adw::PreferencesGroup::builder().title(tr("font.group")).build();
        for (key, title) in tweaks::FONTS {
            if tweaks::has(s, key) {
                g.add(&font_row(s, key, tr(title)));
            }
        }
        page.add(&g);
    }

    let g = adw::PreferencesGroup::new();
    let reset = gtk::Button::builder()
        .label(tr("reset"))
        .halign(gtk::Align::Center)
        .css_classes(["pill"])
        .build();
    let weak = Rc::downgrade(app);
    let store = store.clone();
    reset.connect_clicked(move |_| {
        store.reset();
        if let Some(app) = weak.upgrade() {
            app.toast(tr("reset.done"));
        }
    });
    g.add(&reset);
    page.add(&g);
}

fn accent_row(s: &gio::Settings) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(tr("look.accent")).build();
    let dots = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    dots.set_valign(gtk::Align::Center);
    let mut buttons: Vec<gtk::ToggleButton> = vec![];
    for (name, _) in tweaks::ACCENTS {
        let key: &'static str = match name {
            "blue" => "accent.blue",
            "teal" => "accent.teal",
            "green" => "accent.green",
            "yellow" => "accent.yellow",
            "orange" => "accent.orange",
            "red" => "accent.red",
            "pink" => "accent.pink",
            "purple" => "accent.purple",
            _ => "accent.slate",
        };
        let b = gtk::ToggleButton::builder()
            .tooltip_text(tr(key))
            .css_classes(["accent-dot", &format!("accent-{name}")])
            .build();
        b.update_property(&[gtk::accessible::Property::Label(tr(key))]);
        if let Some(first) = buttons.first() {
            b.set_group(Some(first));
        }
        dots.append(&b);
        buttons.push(b);
    }
    let buttons = Rc::new(buttons);
    let busy = Rc::new(Cell::new(false));
    let sync = {
        let buttons = buttons.clone();
        let busy = busy.clone();
        move |s: &gio::Settings| {
            busy.set(true);
            let i = tweaks::index_of(&tweaks::ACCENTS.map(|a| a.0), &s.string("accent-color"), 0);
            buttons[i as usize].set_active(true);
            busy.set(false);
        }
    };
    sync(s);
    for (i, b) in buttons.iter().enumerate() {
        let s = s.clone();
        let busy = busy.clone();
        b.connect_toggled(move |b| {
            if b.is_active() && !busy.get() {
                let _ = s.set_string("accent-color", tweaks::ACCENTS[i].0);
            }
        });
    }
    s.connect_changed(Some("accent-color"), move |s, _| sync(s));
    row.add_suffix(&dots);
    row
}

fn font_row(s: &gio::Settings, key: &'static str, title: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    let b = gtk::FontDialogButton::new(Some(gtk::FontDialog::new()));
    b.set_valign(gtk::Align::Center);
    // GTK 4.14 zgłasza błąd przy czcionce, której rodziny nie ma w systemie: wtedy przycisk zostaje pusty.
    let families: Vec<String> = b
        .pango_context()
        .list_families()
        .iter()
        .map(|f| f.name().to_lowercase())
        .collect();
    let known = move |v: &str| {
        let d = gtk::pango::FontDescription::from_string(v);
        d.family().is_some_and(|f| families.contains(&f.to_lowercase())).then_some(d)
    };
    let busy = Rc::new(Cell::new(true));
    if let Some(d) = known(&s.string(key)) {
        b.set_font_desc(&d);
    }
    busy.set(false);
    {
        let s = s.clone();
        let busy = busy.clone();
        b.connect_font_desc_notify(move |b| {
            if busy.get() {
                return;
            }
            if let Some(d) = b.font_desc() {
                let _ = s.set_string(key, &d.to_str());
            }
        });
    }
    let weak = b.downgrade();
    s.connect_changed(Some(key), move |s, _| {
        if let (Some(b), Some(d)) = (weak.upgrade(), known(&s.string(key))) {
            busy.set(true);
            b.set_font_desc(&d);
            busy.set(false);
        }
    });
    row.add_suffix(&b);
    row
}
