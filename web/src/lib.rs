//! Web-App (Dashboard).
//!
//! Die reine Logik (Filtern, Sortieren, Formatieren) liegt in eigenen Modulen
//! und ist auf dem Host testbar. Die Oberfläche (`app`) gibt es nur für den Browser.

pub mod filter;
pub mod format;

#[cfg(target_arch = "wasm32")]
pub mod app;
