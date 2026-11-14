//! Cocktail: aktualizacje systemu, urządzenia, pakiety i historia dla Shrimp (nakładka na zyppera
//! i transactional-update). GTK4 + libadwaita.

mod app;
mod devices;
mod help;
mod history;
mod i18n;
mod pages;
mod tree;
mod zypper;

use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

const APP_ID: &str = "org.shrimp.Cocktail";

const CSS: &str = "
.cocktail-side > headerbar, .cocktail-content > headerbar { background: transparent; box-shadow: none; }
.cocktail-tree, .cocktail-list { background: none; }
.cocktail-tree > row, .cocktail-list > row { border-radius: 0; padding: 0; }
.cocktail-tree > row.st-b, .cocktail-list .st-b { background-color: shade(@window_bg_color, 1.07); }
.cocktail-tree > row:hover { background-color: alpha(@window_fg_color, 0.05); }
.cocktail-tree button, .cocktail-list button { min-height: 24px; padding: 0 10px; }
.cocktail-pkg { min-height: 28px; padding: 1px 24px; }
.cocktail-new { color: @accent_color; }
.cocktail-main { min-width: 160px; min-height: 34px; padding-left: 20px; padding-right: 20px; }
.cocktail-term { font-family: monospace; padding: 12px 24px; }
.cocktail-term-line { padding: 8px 24px 12px 24px; }
.cocktail-prefix { font-family: monospace; opacity: 0.7; }
.cocktail-term-line entry { font-family: monospace; }
";

/// Zakładki paska bocznego: identyfikator, ikona, nazwa (klucz tłumaczenia).
const SECTIONS: [(&str, &str, &str); 5] = [
    ("aktualizacje", "emblem-synchronizing-symbolic", "section.updates"),
    ("urzadzenia", "computer-symbolic", "section.devices"),
    ("pakiety", "package-x-generic-symbolic", "section.packages"),
    ("terminal", "list-add-symbolic", "section.install"),
    ("historia", "document-open-recent-symbolic", "section.history"),
];

fn main() -> glib::ExitCode {
    let application = adw::Application::builder().application_id(APP_ID).build();
    application.connect_startup(|_| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(CSS);
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
        }
    });
    application.connect_activate(|application| {
        if let Some(w) = application.active_window() {
            w.present();
            return;
        }
        build(application);
    });
    application.run()
}

fn sidebar_row(icon: &str, title: &str) -> gtk::ListBoxRow {
    let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    b.append(&gtk::Image::from_icon_name(icon));
    let l = gtk::Label::new(Some(title));
    l.set_xalign(0.0);
    b.append(&l);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&b));
    row
}

fn build(application: &adw::Application) {
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .title("Cocktail")
        .default_width(880)
        .default_height(640)
        .width_request(360)
        .height_request(300)
        .build();

    let banner = adw::Banner::new(crate::i18n::tr("updates.restart-to-finish"));
    banner.set_button_label(Some(crate::i18n::tr("button.restart")));
    banner.connect_button_clicked(|_| app::reboot());
    let toasts = adw::ToastOverlay::new();

    let app = Rc::new(app::App {
        window: window.clone(),
        banner: banner.clone(),
        toasts: toasts.clone(),
        pending: RefCell::new(HashMap::new()),
        busy: Cell::new(false),
        on_change: RefCell::new(Vec::new()),
    });

    let updates = pages::updates::Updates::new(&app);
    let devices = pages::devices::Devices::new();
    let packages = pages::packages::Packages::new(&app);
    let history = pages::history::History::new(&app);
    let terminal = pages::terminal::Terminal::new(&app);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.add_named(&updates.widget, Some("aktualizacje"));
    stack.add_named(&devices.widget, Some("urzadzenia"));
    stack.add_named(&packages.widget, Some("pakiety"));
    stack.add_named(&terminal.widget, Some("terminal"));
    stack.add_named(&history.widget, Some("historia"));

    // Pasek boczny: zakładki i wyszukiwanie na dole (ukryte w Aktualizacjach, jak w makiecie).
    let nav = gtk::ListBox::new();
    nav.add_css_class("navigation-sidebar");
    for (_, icon, title) in SECTIONS {
        nav.append(&sidebar_row(icon, crate::i18n::tr(title)));
    }
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some(crate::i18n::tr("search.placeholder")));
    search.set_margin_start(8);
    search.set_margin_end(8);
    search.set_margin_top(8);
    search.set_margin_bottom(8);
    search.set_visible(false);
    search.set_key_capture_widget(Some(&window));

    // Pasek boczny bez tytułu (jak w Panko, Shrimp Apps i Filerze); chowa go przycisk w pasku treści.
    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.set_show_title(false);
    sidebar_header.set_show_end_title_buttons(false);
    let sidebar_view = adw::ToolbarView::new();
    sidebar_view.add_css_class("cocktail-side");
    sidebar_view.add_top_bar(&sidebar_header);
    let nav_scroll = gtk::ScrolledWindow::new();
    nav_scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    nav_scroll.set_child(Some(&nav));
    sidebar_view.set_content(Some(&nav_scroll));
    sidebar_view.add_bottom_bar(&search);

    // Treść: menu główne (☰) w pasku tytułu.
    let menu = gio::Menu::new();
    menu.append(Some(crate::i18n::tr("menu.advanced")), Some("win.advanced"));
    menu.append(Some(crate::i18n::tr("menu.about")), Some("app.about"));
    // Tryb zaawansowany: polecenie i jego wyjście w Instalowaniu (w trybie prostym są ukryte).
    let advanced = gio::SimpleAction::new_stateful("advanced", None, &app::advanced().to_variant());
    {
        let terminal = terminal.clone();
        advanced.connect_change_state(move |a, v| {
            let Some(on) = v.and_then(|v| v.get::<bool>()) else { return };
            a.set_state(&on.to_variant());
            app::set_advanced(on);
            terminal.set_advanced(on);
        });
    }
    window.add_action(&advanced);
    let menu_button = gtk::MenuButton::new();
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button.set_menu_model(Some(&menu));
    menu_button.set_tooltip_text(Some(crate::i18n::tr("menu.main")));
    let content_header = adw::HeaderBar::new();
    content_header.set_title_widget(Some(&adw::Bin::new()));
    content_header.pack_end(&menu_button);
    let side_btn = gtk::ToggleButton::builder().icon_name("sidebar-show-symbolic").tooltip_text(crate::i18n::tr("view.sidebar")).css_classes(["flat"]).build();
    content_header.pack_start(&side_btn);
    let content_view = adw::ToolbarView::new();
    content_view.add_css_class("cocktail-content");
    content_view.add_top_bar(&content_header);
    content_view.add_top_bar(&banner);
    toasts.set_child(Some(&stack));
    content_view.set_content(Some(&toasts));

    let split = adw::OverlaySplitView::new();
    split.set_sidebar(Some(&sidebar_view));
    split.set_content(Some(&content_view));
    split.set_min_sidebar_width(200.0);
    split.set_max_sidebar_width(240.0);
    split.bind_property("show-sidebar", &side_btn, "active").bidirectional().sync_create().build();

    let bp = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        560.0,
        adw::LengthUnit::Sp,
    ));
    bp.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(bp);
    window.set_content(Some(&split));

    let current = Rc::new(RefCell::new("aktualizacje".to_string()));
    {
        let (stack, search, split, current) = (stack.clone(), search.clone(), split.clone(), current.clone());
        let (devices, packages, history, terminal) = (devices.clone(), packages.clone(), history.clone(), terminal.clone());
        nav.connect_row_selected(move |_, row| {
            let Some(row) = row else { return };
            let (id, _, _) = SECTIONS[row.index() as usize];
            *current.borrow_mut() = id.to_string();
            stack.set_visible_child_name(id);
            search.set_visible(id != "aktualizacje" && id != "terminal");
            search.set_text("");
            match id {
                "urzadzenia" => devices.load(),
                "pakiety" => packages.load(),
                "historia" => history.load(),
                "terminal" => terminal.focus(),
                _ => {}
            }
            // Wąskie okno: panel boczny leży nad treścią, po wyborze znika.
            if split.is_collapsed() {
                split.set_show_sidebar(false);
            }
        });
    }
    // COCKTAIL_PAGE=pakiety otwiera wybraną zakładkę (zrzuty ekranu, praca nad kodem).
    let start = std::env::var("COCKTAIL_PAGE").ok().and_then(|p| SECTIONS.iter().position(|s| s.0 == p)).unwrap_or(0);
    nav.select_row(nav.row_at_index(start as i32).as_ref());

    {
        let current = current.clone();
        search.connect_search_changed(move |e| {
            let q = e.text();
            match current.borrow().as_str() {
                "urzadzenia" => devices.search(&q),
                "pakiety" => packages.search(&q),
                "historia" => history.search(&q),
                _ => {}
            }
        });
    }

    // „O Cocktailu” otwiera stronę programu na stronie Shrimp (bez okna pomocy w aplikacji).
    let about = gio::SimpleAction::new("about", None);
    let w = window.clone();
    about.connect_activate(move |_, _| {
        if let Some(url) = help::url() {
            gtk::UriLauncher::new(&url).launch(Some(&w), gio::Cancellable::NONE, |_| {});
        }
    });
    application.add_action(&about);
    let quit = gio::SimpleAction::new("quit", None);
    let a = application.clone();
    quit.connect_activate(move |_, _| a.quit());
    application.add_action(&quit);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
    application.set_accels_for_action("window.close", &["<Control>w"]);

    let _ = updates;
    window.present();
}
