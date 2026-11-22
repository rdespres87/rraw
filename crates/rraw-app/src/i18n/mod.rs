#![forbid(unsafe_code)]

//! Strongly-typed, scalable internationalization (i18n) engine for `rraw-app`.
//!
//! # Architecture & Guarantees
//! - **Zero Runtime Allocation**: All translations return `&'static str` literals with zero heap allocations during the render loop.
//! - **Scalable External Resource-Based Catalogs**: Translations are stored in external JSON catalogs under `assets/i18n/`.
//!   Default catalogs (FR, EN) are bundled at compile time for fully standalone operation.
//! - **Dynamic Zero-Code Language Addition**: New languages (e.g. Spanish, German, Japanese) can be added simply by dropping
//!   a JSON translation file into `assets/i18n/` or user configuration directory without modifying core Rust code.
//! - **Fallback Guarantee**: Incomplete translation files automatically fall back to English with zero runtime panics.
//! - **Reactive O(1) Switching**: UI switches language instantly by mutating `app.layout_config.language`.
//! - **Persistence**: Language selection is saved to `~/.config/rraw/layout.json` via Serde.

pub mod catalog;
pub mod engine;
pub mod key;
pub mod language;

pub use catalog::Catalog;
pub use engine::I18nEngine;
pub use key::{Key, ALL_KEYS};
pub use language::Language;

/// Global helper to translate a key for a given language.
#[inline]
#[must_use]
pub fn t(lang: Language, key: Key) -> &'static str {
    key.translate(lang)
}
