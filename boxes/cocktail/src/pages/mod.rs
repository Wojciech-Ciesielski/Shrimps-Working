pub mod devices;
pub mod history;
pub mod packages;
pub mod updates;

use gtk::prelude::*;

/// Przewijana treść o ograniczonej szerokości (jak w makiecie).
pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(760);
    clamp.set_margin_top(18);
    clamp.set_margin_bottom(18);
    clamp.set_margin_start(12);
    clamp.set_margin_end(12);
    clamp.set_child(Some(child));
    let sw = gtk::ScrolledWindow::new();
    sw.set_hscrollbar_policy(gtk::PolicyType::Never);
    sw.set_vexpand(true);
    sw.set_child(Some(&clamp));
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
