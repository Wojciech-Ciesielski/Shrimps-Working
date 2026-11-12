//! Okno: pasek boczny (Odkrywaj, Zainstalowane, Aplikacje internetowe, kategorie,
//! wyszukiwanie) i treść w `AdwNavigationView` (strony aplikacji wjeżdżają na wierzch).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use adw::prelude::*;
use gtk::{gio, glib};

use crate::backend::{Event, OpKind};
use crate::catalog::{self, App};
use crate::categories::Category;
use crate::details;
use crate::store::{Store, ANY};
use crate::widgets::{self, app_row};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Page {
    Discover,
    Installed,
    Web,
    Category(Category),
    Search,
}

pub struct Shell {
    pub window: adw::ApplicationWindow,
    pub store: Rc<Store>,
    nav: adw::NavigationView,
    split: adw::NavigationSplitView,
    sidebar: gtk::ListBox,
    search: gtk::SearchEntry,
    toasts: adw::ToastOverlay,
    pages: RefCell<Vec<Page>>, // indeks wiersza paska → strona
    current: Cell<Page>,
    discover: RefCell<Option<adw::NavigationPage>>,
    status: RefCell<Option<(String, bool)>>, // (tekst, błąd) gdy katalog jeszcze niegotowy
    banner_css: gtk::CssProvider,
    search_timeout: RefCell<Option<glib::SourceId>>,
}

impl Shell {
    pub fn new(app: &adw::Application, store: Rc<Store>) -> Rc<Shell> {
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Shrimp Apps")
            .default_width(1100)
            .default_height(720)
            .width_request(360)
            .height_request(294)
            .build();

        // Pasek boczny
        let sidebar = gtk::ListBox::new();
        sidebar.add_css_class("navigation-sidebar");
        let mut pages = Vec::new();
        let add = |sidebar: &gtk::ListBox, pages: &mut Vec<Page>, icon: &str, title: &str, page: Page| {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            b.append(&gtk::Image::from_icon_name(icon));
            b.append(&gtk::Label::builder().label(title).xalign(0.0).build());
            let row = gtk::ListBoxRow::builder().child(&b).build();
            sidebar.append(&row);
            pages.push(page);
        };
        add(&sidebar, &mut pages, widgets::pick_icon(widgets::ICON_DISCOVER), crate::i18n::tr("page.discover"), Page::Discover);
        add(&sidebar, &mut pages, widgets::pick_icon(widgets::ICON_INSTALLED), crate::i18n::tr("page.installed"), Page::Installed);
        add(&sidebar, &mut pages, widgets::pick_icon(widgets::ICON_WEB), crate::i18n::tr("page.web"), Page::Web);
        {
            let l = gtk::Label::builder().label(crate::i18n::tr("sidebar.categories")).xalign(0.0).build();
            l.add_css_class("sidebar-heading");
            l.add_css_class("dim-label");
            let row = gtk::ListBoxRow::builder().child(&l).selectable(false).activatable(false).build();
            sidebar.append(&row);
            pages.push(Page::Search); // nieużywany
        }
        for c in Category::ALL {
            add(&sidebar, &mut pages, c.icon(), c.title(), Page::Category(c));
        }
        let side_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&sidebar)
            .build();

        let search = gtk::SearchEntry::builder().placeholder_text(crate::i18n::tr("search.placeholder")).build();
        search.update_property(&[gtk::accessible::Property::Label(crate::i18n::tr("search.placeholder"))]);
        let search_bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        search_bar.add_css_class("sidebar-search");
        search_bar.append(&search);

        let side_header = adw::HeaderBar::builder().show_title(false).build();
        let help = gtk::Button::from_icon_name(widgets::pick_icon(widgets::ICON_HELP));
        help.set_tooltip_text(Some(crate::i18n::tr("help")));
        help.set_action_name(Some("app.help"));
        side_header.pack_start(&help);

        let side_view = adw::ToolbarView::new();
        side_view.add_top_bar(&side_header);
        side_view.set_content(Some(&side_scroll));
        side_view.add_bottom_bar(&search_bar);
        let side_page = adw::NavigationPage::builder().title("Shrimp Apps").child(&side_view).build();

        // Treść
        let nav = adw::NavigationView::new();
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&nav));
        let content_page = adw::NavigationPage::builder().title("Shrimp Apps").child(&toasts).build();

        let split = adw::NavigationSplitView::builder()
            .sidebar(&side_page)
            .content(&content_page)
            .min_sidebar_width(220.0)
            .max_sidebar_width(260.0)
            .build();
        window.set_content(Some(&split));

        let bp = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 720sp").unwrap());
        bp.add_setter(&split, "collapsed", Some(&true.to_value()));
        window.add_breakpoint(bp);

        let banner_css = gtk::CssProvider::new();
        gtk::style_context_add_provider_for_display(
            &gtk::prelude::WidgetExt::display(&window),
            &banner_css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );

        let shell = Rc::new(Shell {
            window,
            store,
            nav,
            split,
            sidebar,
            search,
            toasts,
            pages: RefCell::new(pages),
            current: Cell::new(Page::Discover),
            discover: RefCell::new(None),
            status: RefCell::new(Some((crate::i18n::tr("status.loading-catalog").into(), false))),
            banner_css,
            search_timeout: RefCell::new(None),
        });

        {
            let s = Rc::downgrade(&shell);
            shell.sidebar.connect_row_activated(move |_, row| {
                if let Some(s) = s.upgrade() {
                    let idx = row.index() as usize;
                    let page = s.pages.borrow().get(idx).copied();
                    if let Some(p) = page {
                        if p != Page::Search {
                            s.search.set_text("");
                            s.show(p);
                        }
                    }
                }
            });
        }
        {
            let s = Rc::downgrade(&shell);
            shell.search.connect_search_changed(move |_| {
                if let Some(s) = s.upgrade() {
                    s.search_changed();
                }
            });
        }
        {
            let s = Rc::downgrade(&shell);
            shell.search.connect_stop_search(move |e| {
                e.set_text("");
                if let Some(s) = s.upgrade() {
                    s.show(Page::Discover);
                }
            });
        }
        {
            let s = Rc::downgrade(&shell);
            adw::StyleManager::default().connect_dark_notify(move |_| {
                if let Some(s) = s.upgrade() {
                    s.update_banner_css();
                }
            });
        }
        // Zainstalowane: odświeżaj stronę przy zmianach.
        {
            let s = Rc::downgrade(&shell);
            shell.store.watch(ANY, &shell.window, move || {
                if let Some(s) = s.upgrade() {
                    if s.current.get() == Page::Installed && s.nav.navigation_stack().n_items() == 1 {
                        s.show(Page::Installed);
                    }
                }
            });
        }

        shell.select_row(Page::Discover);
        shell.show(Page::Discover);
        shell
    }

    pub fn focus_search(&self) {
        self.split.set_show_content(false);
        self.search.grab_focus();
    }

    fn select_row(&self, page: Page) {
        let idx = self.pages.borrow().iter().position(|p| *p == page);
        match idx.and_then(|i| self.sidebar.row_at_index(i as i32)) {
            Some(r) => self.sidebar.select_row(Some(&r)),
            None => self.sidebar.unselect_all(),
        }
    }

    fn opener(self: &Rc<Self>) -> Rc<dyn Fn(&str)> {
        let s = Rc::downgrade(self);
        Rc::new(move |id: &str| {
            if let Some(s) = s.upgrade() {
                s.open_app(id);
            }
        })
    }

    pub fn open_app(self: &Rc<Self>, id: &str) {
        let Some(app) = self.store.app(id) else { return };
        let page = details::page(&self.store, &app, self.window.upcast_ref());
        self.nav.push(&page);
        self.split.set_show_content(true);
    }

    pub fn show(self: &Rc<Self>, page: Page) {
        self.current.set(page);
        if page == Page::Search {
            self.sidebar.unselect_all();
        } else {
            self.select_row(page);
        }
        let p = match page {
            Page::Discover => self.discover_page(),
            Page::Installed => self.installed_page(),
            Page::Web => self.web_page(),
            Page::Category(c) => self.category_page(c),
            Page::Search => self.search_page(),
        };
        self.nav.replace(&[p]);
        self.split.set_show_content(true);
    }

    fn search_changed(self: &Rc<Self>) {
        if let Some(id) = self.search_timeout.take() {
            id.remove();
        }
        let s = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(std::time::Duration::from_millis(150), move || {
            if let Some(s) = s.upgrade() {
                s.search_timeout.replace(None);
                if s.search.text().trim().is_empty() {
                    if s.current.get() == Page::Search {
                        s.show(Page::Discover);
                    }
                } else {
                    s.show(Page::Search);
                }
            }
        });
        self.search_timeout.replace(Some(id));
    }

    // ------------------------------------------------------------ zdarzenia backendu

    pub fn handle(self: &Rc<Self>, ev: Event) {
        match ev {
            Event::Status(t) => {
                if self.store.apps.borrow().is_empty() {
                    self.status.replace(Some((t, false)));
                    self.refresh_root_if(&[Page::Discover]);
                }
            }
            Event::Catalog(apps) => {
                self.store.set_catalog(apps);
                self.status.replace(None);
                self.discover.replace(None);
                self.update_banner_css();
                self.refresh_root();
            }
            Event::CatalogFailed(e) => {
                eprintln!("shrimp-apps: katalog: {e}");
                if self.store.apps.borrow().is_empty() {
                    self.status.replace(Some((e, true)));
                    self.discover.replace(None);
                    self.refresh_root();
                } else {
                    self.toast(crate::i18n::tr("error.refresh"), None);
                }
            }
            Event::CatalogStale(e) => {
                eprintln!("shrimp-apps: katalog z pamięci, odświeżanie nie wyszło: {e}");
            }
            Event::Installed(list) => {
                self.store.set_installed(list);
            }
            Event::Progress { app_id, fraction } => {
                let kind = match self.store.status(&app_id) {
                    crate::store::Status::Queued(k) | crate::store::Status::Busy(k, _) => k,
                    _ => OpKind::Install,
                };
                self.store.set_op(&app_id, Some(crate::store::Status::Busy(kind, fraction)));
            }
            Event::Done { app_id, kind, error, cancelled } => {
                self.store.set_op(&app_id, None);
                let name = self.store.display_name(&app_id);
                match (error, cancelled) {
                    (_, true) => {}
                    (Some(e), _) => {
                        eprintln!("shrimp-apps: {kind:?} {app_id}: {e}");
                        let t = match kind {
                            OpKind::Install => crate::i18n::trf("error.install", &[("name", &name)]),
                            OpKind::Update => crate::i18n::trf("error.update", &[("name", &name)]),
                            OpKind::Uninstall => crate::i18n::trf("error.remove", &[("name", &name)]),
                        };
                        self.toast(&t, None);
                    }
                    (None, _) => {
                        if kind == OpKind::Install {
                            self.toast(&crate::i18n::trf("toast.installed", &[("name", &name)]), Some(&app_id));
                        }
                    }
                }
            }
            Event::Size { app_id, download, installed } => {
                self.store.sizes.borrow_mut().insert(app_id.clone(), (download, installed));
                self.store.notify(&app_id);
            }
            Event::LaunchFailed { app_id, error } => {
                eprintln!("shrimp-apps: uruchomienie {app_id}: {error}");
                let name = self.store.display_name(&app_id);
                self.toast(&crate::i18n::trf("error.launch", &[("name", &name)]), None);
            }
        }
    }

    pub fn popular_loaded(self: &Rc<Self>, ids: Vec<String>) {
        if ids.is_empty() {
            return;
        }
        self.store.set_popular(ids);
        self.discover.replace(None);
        self.update_banner_css();
        self.refresh_root_if(&[Page::Discover]);
    }

    fn toast(&self, title: &str, open_id: Option<&str>) {
        let t = adw::Toast::new(title);
        if let Some(id) = open_id {
            t.set_button_label(Some(crate::i18n::tr("button.open")));
            t.set_action_name(Some("app.launch"));
            t.set_action_target_value(Some(&id.to_variant()));
        }
        self.toasts.add_toast(t);
    }

    fn refresh_root(self: &Rc<Self>) {
        if self.nav.navigation_stack().n_items() <= 1 {
            self.show(self.current.get());
        }
    }

    fn refresh_root_if(self: &Rc<Self>, pages: &[Page]) {
        if pages.contains(&self.current.get()) {
            self.refresh_root();
        }
    }

    // ------------------------------------------------------------ strony

    fn root_page(&self, title: &str, tag: &str, content: &impl IsA<gtk::Widget>, show_title: bool) -> adw::NavigationPage {
        let header = adw::HeaderBar::builder().show_title(show_title).build();
        let menu = gio::Menu::new();
        menu.append(Some(crate::i18n::tr("menu.refresh")), Some("app.refresh"));
        menu.append(Some(crate::i18n::tr("help")), Some("app.help"));
        menu.append(Some(crate::i18n::tr("menu.about")), Some("app.about"));
        let mb = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .primary(true)
            .tooltip_text(crate::i18n::tr("menu.main"))
            .build();
        header.pack_end(&mb);
        let tv = adw::ToolbarView::new();
        tv.add_top_bar(&header);
        tv.set_content(Some(content));
        adw::NavigationPage::builder().title(title).tag(tag).child(&tv).build()
    }

    fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
        gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(child)
            .build()
    }

    /// Strona stanu (ładowanie, błąd), gdy katalogu jeszcze nie ma.
    fn status_widget(&self) -> Option<gtk::Widget> {
        let st = self.status.borrow().clone()?;
        let (text, error) = st;
        let page = adw::StatusPage::new();
        if error {
            page.set_icon_name(Some("network-offline-symbolic"));
            page.set_title(crate::i18n::tr("offline.title"));
            page.set_description(Some(crate::i18n::tr("offline.body")));
            let b = gtk::Button::builder()
                .label(crate::i18n::tr("button.retry"))
                .halign(gtk::Align::Center)
                .action_name("app.refresh")
                .build();
            b.add_css_class("pill");
            b.add_css_class("suggested-action");
            page.set_child(Some(&b));
        } else {
            let sp = gtk::Spinner::builder().spinning(true).width_request(32).height_request(32).build();
            page.set_child(Some(&sp));
            page.set_title(&text);
        }
        Some(page.upcast())
    }

    fn discover_page(self: &Rc<Self>) -> adw::NavigationPage {
        if let Some(w) = self.status_widget() {
            return self.root_page(crate::i18n::tr("page.discover"), "discover", &w, false);
        }
        if let Some(p) = self.discover.borrow().as_ref() {
            return p.clone();
        }
        let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
        col.add_css_class("discover");
        col.append(&self.banners());
        for c in Category::ALL {
            let apps: Vec<Rc<App>> = self.store.in_category(c).into_iter().take(27).collect();
            if apps.len() >= 3 {
                col.append(&self.section(c, apps));
            }
        }
        let clamp = adw::Clamp::builder().maximum_size(1200).tightening_threshold(900).child(&col).build();
        let page = self.root_page(crate::i18n::tr("page.discover"), "discover", &Self::scrolled(&clamp), false);
        self.discover.replace(Some(page.clone()));
        page
    }

    /// Aplikacje do karuzeli „Wybór tygodnia”: popularne z kolorem marki i zrzutami,
    /// co tydzień inny zestaw.
    fn picks(&self) -> Vec<Rc<App>> {
        let apps = self.store.apps.borrow();
        let pool: Vec<Rc<App>> = apps
            .iter()
            .filter(|a| a.brand_light.is_some() && !a.screenshots.is_empty() && a.main_category().is_some())
            .take(60)
            .cloned()
            .collect();
        if pool.is_empty() {
            return apps.iter().take(4).cloned().collect();
        }
        let week = glib::DateTime::now_local().map(|d| d.week_of_year() + d.year() * 53).unwrap_or(0) as usize;
        let n = pool.len().min(5);
        (0..n).map(|i| pool[(week * 5 + i * 7) % pool.len()].clone()).fold(Vec::new(), |mut v, a| {
            if !v.iter().any(|x: &Rc<App>| x.id == a.id) {
                v.push(a);
            }
            v
        })
    }

    fn update_banner_css(&self) {
        let dark = adw::StyleManager::default().is_dark();
        let mut css = String::new();
        for (i, a) in self.picks().iter().enumerate() {
            let bg = if dark { a.brand_dark.as_ref().or(a.brand_light.as_ref()) } else { a.brand_light.as_ref() };
            let bg = bg.filter(|c| is_hex_color(c)).cloned().unwrap_or_else(|| "#3584e4".into());
            let fg = if luminance(&bg) > 0.3 { "rgba(0,0,0,0.87)" } else { "#ffffff" };
            let btn_fg = if luminance(&bg) > 0.3 { "#ffffff" } else { bg.as_str() };
            let btn_bg = if luminance(&bg) > 0.3 { "rgba(0,0,0,0.87)" } else { "#ffffff" };
            css.push_str(&format!(
                ".banner-{i} {{ background-color: {bg}; color: {fg}; }}\n\
                 .banner-{i} button.banner-get {{ background-color: {btn_bg}; color: {btn_fg}; }}\n"
            ));
        }
        self.banner_css.load_from_string(&css);
    }

    fn banners(self: &Rc<Self>) -> gtk::Widget {
        let picks = self.picks();
        let carousel = adw::Carousel::builder().spacing(12).allow_scroll_wheel(false).build();
        carousel.add_css_class("banners");
        for (i, a) in picks.iter().enumerate() {
            let b = gtk::Box::new(gtk::Orientation::Horizontal, 24);
            b.add_css_class("banner");
            b.add_css_class(&format!("banner-{i}"));
            b.set_hexpand(true);
            b.append(&widgets::app_icon(Some(a), &a.id, 96));
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 4);
            texts.set_valign(gtk::Align::Center);
            texts.set_hexpand(true);
            let kicker = gtk::Label::builder().label(crate::i18n::tr("discover.kicker")).xalign(0.0).build();
            kicker.add_css_class("banner-kicker");
            let name = gtk::Label::builder().label(&a.name).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build();
            name.add_css_class("title-1");
            let summary = widgets::wrap_label(&a.summary, &["banner-summary"]);
            summary.set_lines(2);
            summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.append(&kicker);
            texts.append(&name);
            texts.append(&summary);
            b.append(&texts);
            let get = widgets::action_button(&self.store, &a.id);
            get.remove_css_class("flat");
            get.remove_css_class("get-button");
            get.add_css_class("pill");
            get.add_css_class("banner-get");
            b.append(&get);
            // Klik w tekst banera otwiera stronę aplikacji (przycisk obsługuje się sam).
            let click = gtk::GestureClick::new();
            let open = self.opener();
            let id = a.id.clone();
            click.connect_released(move |_, _, _, _| open(&id));
            texts.add_controller(click);
            let cursor = gtk::gdk::Cursor::from_name("pointer", None);
            texts.set_cursor(cursor.as_ref());
            carousel.append(&b);
        }
        let dots = adw::CarouselIndicatorDots::builder().carousel(&carousel).build();
        let v = gtk::Box::new(gtk::Orientation::Vertical, 6);
        v.add_css_class("banner-area");
        v.append(&carousel);
        v.append(&dots);

        // Co 7 s następny baner (bez animacji, gdy użytkownik wyłączył animacje).
        let weak = carousel.downgrade();
        glib::timeout_add_seconds_local(7, move || {
            let Some(c) = weak.upgrade() else { return glib::ControlFlow::Break };
            let n = c.n_pages();
            if n > 1 && c.is_mapped() {
                let next = (c.position().round() as u32 + 1) % n;
                c.scroll_to(&c.nth_page(next), true);
            }
            glib::ControlFlow::Continue
        });
        v.upcast()
    }

    /// Sekcja kategorii: tytuł z przejściem do kategorii, strzałki i siatka 3 kolumny × 3 wiersze.
    fn section(self: &Rc<Self>, c: Category, apps: Vec<Rc<App>>) -> gtk::Widget {
        let v = gtk::Box::new(gtk::Orientation::Vertical, 6);
        v.add_css_class("section");
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let title_btn = gtk::Button::new();
        title_btn.add_css_class("flat");
        title_btn.add_css_class("section-title");
        let tb = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let tl = gtk::Label::new(Some(c.title()));
        tl.add_css_class("title-3");
        tb.append(&tl);
        tb.append(&gtk::Image::from_icon_name("go-next-symbolic"));
        title_btn.set_child(Some(&tb));
        {
            let s = Rc::downgrade(self);
            title_btn.connect_clicked(move |_| {
                if let Some(s) = s.upgrade() {
                    s.show(Page::Category(c));
                }
            });
        }
        head.append(&title_btn);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        head.append(&spacer);

        let carousel = adw::Carousel::builder().spacing(24).allow_scroll_wheel(false).build();
        let prev = gtk::Button::from_icon_name("go-previous-symbolic");
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        prev.set_tooltip_text(Some(crate::i18n::tr("discover.previous")));
        next.set_tooltip_text(Some(crate::i18n::tr("discover.next")));
        for b in [&prev, &next] {
            b.add_css_class("flat");
            b.add_css_class("circular");
            head.append(b);
        }
        let step = |c: &adw::Carousel, d: i32| {
            let n = c.n_pages() as i32;
            let p = (c.position().round() as i32 + d).clamp(0, (n - 1).max(0));
            c.scroll_to(&c.nth_page(p as u32), true);
        };
        {
            let c = carousel.clone();
            prev.connect_clicked(move |_| step(&c, -1));
            let c = carousel.clone();
            next.connect_clicked(move |_| step(&c, 1));
        }
        {
            let (p, n) = (prev.clone(), next.clone());
            let upd = move |c: &adw::Carousel| {
                let pos = c.position();
                p.set_sensitive(pos > 0.01);
                n.set_sensitive(pos < c.n_pages() as f64 - 1.01);
            };
            carousel.connect_position_notify(upd.clone());
            carousel.connect_n_pages_notify(upd);
        }

        let open = self.opener();
        for chunk in apps.chunks(9) {
            let grid = gtk::Grid::builder().column_spacing(24).column_homogeneous(true).hexpand(true).build();
            for (i, a) in chunk.iter().enumerate() {
                let (col, row) = ((i / 3) as i32, (i % 3) as i32);
                let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
                cell.append(&app_row(&self.store, a, None, open.clone()));
                let rows_in_col = (chunk.len() - (i / 3) * 3).min(3);
                if (row as usize) < rows_in_col - 1 {
                    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
                    sep.add_css_class("row-separator");
                    cell.append(&sep);
                }
                grid.attach(&cell, col, row, 1, 1);
            }
            // Pusta kolumna, żeby niepełna strona miała te same szerokości.
            for col in (chunk.len() as i32 + 2) / 3..3 {
                grid.attach(&gtk::Box::new(gtk::Orientation::Vertical, 0), col, 0, 1, 1);
            }
            carousel.append(&grid);
        }
        v.append(&head);
        v.append(&carousel);
        v.upcast()
    }

    /// Siatka wszystkich aplikacji (kategoria, wyniki wyszukiwania).
    fn app_grid(self: &Rc<Self>, apps: Vec<Rc<App>>) -> gtk::Widget {
        let model = gio::ListStore::new::<glib::BoxedAnyObject>();
        for a in apps {
            model.append(&glib::BoxedAnyObject::new(a));
        }
        let factory = gtk::SignalListItemFactory::new();
        let store = self.store.clone();
        let open = self.opener();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
            let app: Rc<App> = obj.borrow::<Rc<App>>().clone();
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
            cell.append(&app_row(&store, &app, None, open.clone()));
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.add_css_class("row-separator");
            cell.append(&sep);
            item.set_child(Some(&cell));
            item.set_activatable(false);
            item.set_focusable(false);
        });
        factory.connect_unbind(|_, item| {
            item.downcast_ref::<gtk::ListItem>().unwrap().set_child(gtk::Widget::NONE);
        });
        let grid = gtk::GridView::builder()
            .model(&gtk::NoSelection::new(Some(model)))
            .factory(&factory)
            .min_columns(1)
            .max_columns(3)
            .build();
        grid.add_css_class("app-grid");
        let clamp = adw::Clamp::builder().maximum_size(1200).tightening_threshold(900).child(&grid).build();
        // GridView przewija się sam (wirtualizacja), więc Clamp trzymamy w środku ScrolledWindow.
        gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&clamp)
            .build()
            .upcast()
    }

    fn category_page(self: &Rc<Self>, c: Category) -> adw::NavigationPage {
        if let Some(w) = self.status_widget() {
            return self.root_page(c.title(), c.key(), &w, true);
        }
        let apps = self.store.in_category(c);
        let g = self.app_grid(apps);
        self.root_page(c.title(), c.key(), &g, true)
    }

    fn search_page(self: &Rc<Self>) -> adw::NavigationPage {
        let q = self.search.text().to_string();
        let terms = catalog::search_terms(&q);
        let mut hits: Vec<(u32, usize, Rc<App>)> = self
            .store
            .apps
            .borrow()
            .iter()
            .filter_map(|a| {
                let s = a.search_score(&terms);
                (s > 0).then(|| (s, self.store.rank(&a.id), a.clone()))
            })
            .collect();
        // Najpierw dopasowanie nazwy, potem popularność.
        hits.sort_by(|a, b| (b.0 / 20).cmp(&(a.0 / 20)).then(a.1.cmp(&b.1)));
        let title = crate::i18n::trf("search.results", &[("query", q.trim())]);
        if hits.is_empty() {
            let st = adw::StatusPage::builder()
                .icon_name("system-search-symbolic")
                .title(crate::i18n::tr("search.none"))
                .description(crate::i18n::tr("search.none-body"))
                .build();
            if let Some(w) = self.status_widget() {
                return self.root_page(&title, "search", &w, true);
            }
            return self.root_page(&title, "search", &st, true);
        }
        let g = self.app_grid(hits.into_iter().map(|h| h.2).collect());
        self.root_page(&title, "search", &g, true)
    }

    fn web_page(self: &Rc<Self>) -> adw::NavigationPage {
        // Katalog i tworzenie aplikacji internetowych: box web-apps.
        let content = shrimp_webapps::page();
        self.root_page(crate::i18n::tr("page.web"), "web", &content, true)
    }

    fn installed_page(self: &Rc<Self>) -> adw::NavigationPage {
        let installed: Vec<crate::backend::Installed> = {
            let mut v: Vec<_> = self.store.installed.borrow().values().cloned().collect();
            v.sort_by_key(|i| catalog::fold(&i.name));
            v
        };
        if installed.is_empty() {
            let st = adw::StatusPage::builder()
                .icon_name(widgets::pick_icon(widgets::ICON_INSTALLED))
                .title(crate::i18n::tr("installed.none"))
                .description(crate::i18n::tr("installed.none-body"))
                .build();
            return self.root_page(crate::i18n::tr("page.installed"), "installed", &st, true);
        }
        let col = gtk::Box::new(gtk::Orientation::Vertical, 6);
        col.add_css_class("installed");
        let open = self.opener();
        let row_for = |i: &crate::backend::Installed| -> gtk::Widget {
            let app = self.store.app(&i.id).unwrap_or_else(|| {
                Rc::new(App {
                    id: i.id.clone(),
                    flatpak_ref: i.flatpak_ref.clone(),
                    name: i.name.clone(),
                    summary: i.summary.clone(),
                    ..Default::default()
                })
            });
            let mut sub = Vec::new();
            if let Some(v) = &i.version {
                sub.push(crate::i18n::trf("installed.version", &[("version", v)]));
            }
            if i.installed_size > 0 {
                sub.push(widgets::format_size(i.installed_size));
            }
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let in_catalog = self.store.app(&i.id).is_some();
            let open2 = open.clone();
            let open_fn: Rc<dyn Fn(&str)> = if in_catalog {
                open2
            } else {
                let store = self.store.clone();
                Rc::new(move |id: &str| store.launch(id))
            };
            cell.append(&app_row(&self.store, &app, Some(sub.join(" · ")), open_fn));
            let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
            sep.add_css_class("row-separator");
            cell.append(&sep);
            cell.upcast()
        };

        let updates: Vec<_> = installed.iter().filter(|i| i.has_update).collect();
        if !updates.is_empty() {
            let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let l = gtk::Label::builder()
                .label(crate::i18n::trf("installed.updates", &[("count", &updates.len().to_string())]))
                .xalign(0.0)
                .hexpand(true)
                .build();
            l.add_css_class("title-4");
            head.append(&l);
            let all = gtk::Button::builder().label(crate::i18n::tr("installed.update-all")).build();
            all.add_css_class("pill");
            all.add_css_class("suggested-action");
            let busy = updates.iter().any(|u| self.store.status(&u.id).busy());
            all.set_sensitive(!busy);
            let store = self.store.clone();
            all.connect_clicked(move |_| store.update_all());
            head.append(&all);
            col.append(&head);
            for u in &updates {
                col.append(&row_for(u));
            }
            let l2 = gtk::Label::builder().label(crate::i18n::tr("installed.all")).xalign(0.0).build();
            l2.add_css_class("title-4");
            l2.set_margin_top(18);
            col.append(&l2);
        }
        for i in installed.iter().filter(|i| !i.has_update) {
            col.append(&row_for(i));
        }
        let clamp = adw::Clamp::builder().maximum_size(900).child(&col).build();
        self.root_page(crate::i18n::tr("page.installed"), "installed", &Self::scrolled(&clamp), true)
    }
}

fn is_hex_color(c: &str) -> bool {
    let h = c.trim_start_matches('#');
    c.starts_with('#') && (h.len() == 6 || h.len() == 3) && h.chars().all(|x| x.is_ascii_hexdigit())
}

fn luminance(hex: &str) -> f64 {
    let h = hex.trim_start_matches('#');
    let h = if h.len() == 3 { h.chars().flat_map(|c| [c, c]).collect::<String>() } else { h.to_string() };
    let ch = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64 / 255.0;
    let lin = |v: f64| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
    if h.len() < 6 {
        return 0.0;
    }
    0.2126 * lin(ch(0)) + 0.7152 * lin(ch(2)) + 0.0722 * lin(ch(4))
}
