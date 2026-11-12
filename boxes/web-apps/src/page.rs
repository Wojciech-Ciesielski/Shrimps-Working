//! Strona „Aplikacje internetowe”: wyszukiwarka, „Utwórz własną” i lista katalogu
//! pogrupowana według kategorii. Każdy wiersz ma przycisk „Utwórz” (po utworzeniu „Otwórz”
//! i przycisk usuwania). Bez własnego paska tytułu: Shrimp Apps owija stronę w AdwToolbarView.
use crate::catalog::{self, WebApp, CATEGORIES};
use crate::i18n::{tr, trf};
use crate::launcher;
use adw::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

const CSS: &str = "
.webapps-list { background: none; }
.webapps-list > row { border-radius: 12px; }
.webapps-header { margin: 18px 12px 6px 12px; }
.get-button { color: @accent_color; font-weight: bold; }
";

struct Ctx {
    list: gtk::ListBox,
    toasts: adw::ToastOverlay,
}

/// Strona do wstawienia w Shrimp Apps (np. jako treść AdwToolbarView).
pub fn page() -> gtk::Widget {
    if let Some(display) = gdk::Display::default() {
        let css = gtk::CssProvider::new();
        css.load_from_string(CSS);
        gtk::style_context_add_provider_for_display(&display, &css, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION - 1);
    }

    let search = gtk::SearchEntry::builder().placeholder_text(tr("search.placeholder")).hexpand(true).build();
    let custom = gtk::Button::builder().label(tr("custom.button")).build();
    custom.add_css_class("pill");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    top.append(&search);
    top.append(&custom);
    top.set_margin_top(12);
    top.set_margin_start(12);
    top.set_margin_end(12);

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("webapps-list");
    list.set_placeholder(Some(
        &adw::StatusPage::builder()
            .icon_name("system-search-symbolic")
            .title(tr("empty.title"))
            .description(tr("empty.body"))
            .build(),
    ));
    list.set_header_func(|row, before| {
        let cat = category_of(row);
        if before.map(category_of).as_deref() == Some(cat.as_str()) {
            row.set_header(None::<&gtk::Widget>);
            return;
        }
        let label = gtk::Label::builder()
            .label(if cat == "custom" { tr("custom.section") } else { tr(&format!("cat.{cat}")) })
            .xalign(0.0)
            .build();
        label.add_css_class("heading");
        label.add_css_class("webapps-header");
        row.set_header(Some(&label));
    });

    let toasts = adw::ToastOverlay::new();
    let ctx = Rc::new(Ctx { list: list.clone(), toasts: toasts.clone() });

    // Własne aplikacje (spoza katalogu) na górze, potem katalog w kolejności kategorii.
    for c in launcher::created().into_iter().filter(|c| catalog::find(&c.id).is_none()) {
        let app = WebApp { id: c.id, name: c.name, url: c.url, category: "custom".into(), names: Default::default(), icon: None };
        list.append(&row(&ctx, app));
    }
    for cat in CATEGORIES {
        for app in catalog::catalog().iter().filter(|a| a.category == cat) {
            list.append(&row(&ctx, app.clone()));
        }
    }

    let query = Rc::new(RefCell::new(String::new()));
    {
        let query = query.clone();
        list.set_filter_func(move |row| {
            let q = query.borrow();
            q.is_empty() || search_key(row).contains(q.as_str())
        });
    }
    {
        let list = list.clone();
        search.connect_search_changed(move |s| {
            *query.borrow_mut() = s.text().to_lowercase();
            list.invalidate_filter();
            list.invalidate_headers();
        });
    }
    {
        let ctx = ctx.clone();
        custom.connect_clicked(move |b| custom_dialog(&ctx, b.upcast_ref()));
    }

    let clamp = adw::Clamp::builder().maximum_size(860).child(&list).build();
    clamp.set_margin_bottom(24);
    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).child(&clamp).build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let top_clamp = adw::Clamp::builder().maximum_size(860).child(&top).build();
    content.append(&top_clamp);
    content.append(&scroll);
    toasts.set_child(Some(&content));
    toasts.upcast()
}

// Kategorię, id i tekst wyszukiwania trzymamy w nazwie widżetu wiersza: "kategoria|id|tekst".
fn part(row: &gtk::ListBoxRow, i: usize) -> String {
    row.widget_name().splitn(3, '|').nth(i).unwrap_or("").to_string()
}

fn category_of(row: &gtk::ListBoxRow) -> String {
    part(row, 0)
}

fn id_of(row: &gtk::ListBoxRow) -> String {
    part(row, 1)
}

fn search_key(row: &gtk::ListBoxRow) -> String {
    part(row, 2)
}

fn icon_widget(app: &WebApp) -> gtk::Image {
    let bytes = match app.icon_png() {
        Some(b) => glib::Bytes::from_static(b),
        None => glib::Bytes::from_owned(app.letter_svg().into_bytes()),
    };
    let img = match gdk::Texture::from_bytes(&bytes) {
        Ok(t) => gtk::Image::from_paintable(Some(&t)),
        Err(_) => gtk::Image::from_icon_name("web-browser-symbolic"),
    };
    img.set_pixel_size(48);
    img.add_css_class("icon-dropshadow");
    img
}

fn row(ctx: &Rc<Ctx>, app: WebApp) -> gtk::ListBoxRow {
    let name = gtk::Label::builder().label(app.display_name()).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
    name.add_css_class("heading");
    let host = gtk::Label::builder().label(app.host()).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
    host.add_css_class("caption");
    host.add_css_class("dim-label");
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 2);
    labels.set_valign(gtk::Align::Center);
    labels.set_hexpand(true);
    labels.append(&name);
    labels.append(&host);

    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
    remove.set_tooltip_text(Some(&tr("button.remove.tooltip")));
    remove.add_css_class("flat");
    remove.add_css_class("circular");
    remove.set_valign(gtk::Align::Center);
    let action = gtk::Button::new();
    action.add_css_class("flat");
    action.add_css_class("get-button");
    action.set_valign(gtk::Align::Center);

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    line.set_margin_top(6);
    line.set_margin_bottom(6);
    line.set_margin_start(12);
    line.set_margin_end(12);
    line.append(&icon_widget(&app));
    line.append(&labels);
    line.append(&remove);
    line.append(&action);

    let row = gtk::ListBoxRow::builder().child(&line).activatable(false).build();
    row.set_widget_name(&format!(
        "{}|{}|{} {} {}",
        app.category,
        app.id,
        app.display_name().to_lowercase(),
        app.name.to_lowercase(),
        app.host()
    ));

    let app = Rc::new(app);
    let refresh = {
        let (app, action, remove) = (app.clone(), action.clone(), remove.clone());
        move || {
            let created = launcher::is_created(&app.id);
            action.set_label(&tr(if created { "button.open" } else { "button.create" }));
            remove.set_visible(created);
        }
    };
    refresh();
    let refresh = Rc::new(refresh);

    {
        let (ctx, app, refresh) = (ctx.clone(), app.clone(), refresh.clone());
        action.connect_clicked(move |_| {
            if launcher::is_created(&app.id) {
                open(&ctx, &app);
                return;
            }
            match launcher::create(&app) {
                Ok(_) => toast(&ctx, &trf("toast.created", &[("name", app.display_name())])),
                Err(e) => toast(&ctx, &trf("error.create", &[("name", app.display_name()), ("error", &e.to_string())])),
            }
            refresh();
        });
    }
    {
        let (ctx, app, row) = (ctx.clone(), app.clone(), row.clone());
        remove.connect_clicked(move |_| {
            if let Err(e) = launcher::remove(&app.id) {
                toast(&ctx, &e.to_string());
                return;
            }
            toast(&ctx, &trf("toast.removed", &[("name", app.display_name())]));
            if app.category == "custom" {
                ctx.list.remove(&row);
            } else {
                refresh();
            }
        });
    }
    row
}

fn open(ctx: &Ctx, app: &WebApp) {
    let file = launcher::Dirs::from_env().desktop_file(&app.id);
    match gio::DesktopAppInfo::from_filename(&file) {
        Some(info) => {
            if let Err(e) = info.launch(&[], None::<&gio::AppLaunchContext>) {
                toast(ctx, &e.to_string());
            }
        }
        None => toast(ctx, &file.display().to_string()),
    }
}

fn toast(ctx: &Ctx, text: &str) {
    ctx.toasts.add_toast(adw::Toast::builder().title(glib::markup_escape_text(text)).timeout(3).build());
}

fn custom_dialog(ctx: &Rc<Ctx>, parent: &gtk::Widget) {
    let url = gtk::Entry::builder().placeholder_text(tr("custom.url")).input_purpose(gtk::InputPurpose::Url).activates_default(true).build();
    let name = gtk::Entry::builder().placeholder_text(tr("custom.name")).activates_default(true).build();
    let fields = gtk::Box::new(gtk::Orientation::Vertical, 12);
    fields.append(&url);
    fields.append(&name);

    let dialog = adw::AlertDialog::builder().heading(tr("custom.title")).body(tr("custom.body")).extra_child(&fields).build();
    dialog.add_responses(&[("cancel", &tr("button.cancel")), ("create", &tr("button.create"))]);
    dialog.set_response_appearance("create", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("create"));
    dialog.set_close_response("cancel");
    dialog.set_response_enabled("create", false);

    // Nazwa podpowiada się z adresu, dopóki użytkownik jej nie zmieni.
    let touched = Rc::new(std::cell::Cell::new(false));
    {
        let touched = touched.clone();
        name.connect_changed(move |e| {
            if e.has_focus() {
                touched.set(true);
            }
        });
    }
    {
        let (dialog, name, touched) = (dialog.clone(), name.clone(), touched.clone());
        url.connect_changed(move |e| {
            let ok = catalog::normalize_url(&e.text());
            dialog.set_response_enabled("create", ok.is_some());
            if !touched.get() {
                name.set_text(&ok.map(|u| catalog::name_from_url(&u)).unwrap_or_default());
            }
        });
    }
    {
        let ctx = ctx.clone();
        dialog.connect_response(Some("create"), move |_, _| {
            let Some(u) = catalog::normalize_url(&url.text()) else { return };
            let n = name.text().trim().to_string();
            let n = if n.is_empty() { catalog::name_from_url(&u) } else { n };
            let app = WebApp::custom(&n, &u);
            match launcher::create(&app) {
                Ok(_) => {
                    toast(&ctx, &trf("toast.created", &[("name", &app.name)]));
                    // Nowa własna aplikacja na górę listy (zastępuje starszą o tym samym adresie).
                    let mut i = 0;
                    while let Some(r) = ctx.list.row_at_index(i) {
                        if category_of(&r) != "custom" {
                            break;
                        }
                        if id_of(&r) == app.id {
                            ctx.list.remove(&r);
                            continue;
                        }
                        i += 1;
                    }
                    ctx.list.prepend(&row(&ctx, app));
                }
                Err(e) => toast(&ctx, &trf("error.create", &[("name", &app.name), ("error", &e.to_string())])),
            }
        });
    }
    dialog.present(Some(parent));
}
