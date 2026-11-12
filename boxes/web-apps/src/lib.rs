//! Aplikacje internetowe w Shrimp Apps.
//!
//! * [`catalog`]: 150 popularnych aplikacji webowych (`data/webapps.json`) z wbudowanymi ikonami,
//! * [`launcher`]: tworzenie aplikacji jednym kliknięciem (plik `.desktop` + ikona w katalogu użytkownika),
//! * [`page`] (funkcja `gtk`): gotowa strona GTK4/libadwaita z listą i przyciskami „Utwórz”.
//!
//! Shrimp Apps osadza stronę tak: `let w = shrimp_webapps::page();` i owija ją we własny
//! `AdwToolbarView` z nagłówkiem.

pub mod catalog;
pub mod i18n;
pub mod launcher;
#[cfg(feature = "gtk")]
mod page;

pub use catalog::{catalog, WebApp, CATEGORIES};
#[cfg(feature = "gtk")]
pub use page::page;
