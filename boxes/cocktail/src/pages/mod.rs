pub mod devices;
pub mod history;
pub mod packages;
pub mod terminal;
pub mod updates;

use gtk::prelude::*;

/// Przewijana treść na całą szerokość okna, wiersze bez zaokrągleń (jak w Panko).
pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    child.set_margin_bottom(12);
    let sw = gtk::ScrolledWindow::new();
    sw.set_hscrollbar_policy(gtk::PolicyType::Never);
    sw.set_vexpand(true);
    sw.set_child(Some(child));
    sw
}

/// „Nic nie pasuje do tego wyszukiwania.”
pub fn no_results() -> gtk::Label {
    let l = gtk::Label::new(Some(crate::i18n::tr("search.none")));
    l.add_css_class("dim-label");
    l.set_margin_top(24);
    l.set_visible(false);
    l
}

/// Tekst ładowania / stanu pustego.
pub fn note(text: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class("dim-label");
    l.set_margin_top(24);
    l.set_wrap(true);
    l
}
