#![forbid(unsafe_code)]

//! Central thread-safe internationalization runtime engine and external catalog loader.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

use crate::i18n::{Catalog, Key, Language};

static FR_JSON: &str = include_str!("../../../../assets/i18n/fr.json");
static EN_JSON: &str = include_str!("../../../../assets/i18n/en.json");

static GLOBAL_ENGINE: OnceLock<I18nEngine> = OnceLock::new();

/// Thread-safe registry and runtime engine for application translations.
pub struct I18nEngine {
    catalogs: RwLock<HashMap<Language, Arc<Catalog>>>,
    default_fr: Arc<Catalog>,
    default_en: Arc<Catalog>,
}

impl I18nEngine {
    /// Returns the global singleton instance of the `I18nEngine`.
    #[must_use]
    pub fn global() -> &'static Self {
        GLOBAL_ENGINE.get_or_init(Self::init_default)
    }

    fn init_default() -> Self {
        let default_en = match Catalog::from_json_str(Language::En, EN_JSON, None) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                eprintln!("rraw-app i18n: failed to load default EN catalog: {e}");
                Arc::new(Catalog::empty(Language::En))
            }
        };

        let default_fr = match Catalog::from_json_str(Language::Fr, FR_JSON, Some(&default_en)) {
            Ok(c) => Arc::new(c),
            Err(e) => {
                eprintln!("rraw-app i18n: failed to load default FR catalog: {e}");
                default_en.clone()
            }
        };

        let mut map = HashMap::new();
        map.insert(Language::En, default_en.clone());
        map.insert(Language::Fr, default_fr.clone());

        Self {
            catalogs: RwLock::new(map),
            default_fr,
            default_en,
        }
    }

    /// Registers or overrides a translation catalog.
    pub fn register_catalog(&self, catalog: Catalog) {
        let lang = catalog.language();
        let arc_cat = Arc::new(catalog);
        let mut guard = match self.catalogs.write() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        guard.insert(lang, arc_cat);
    }

    /// Returns the active catalog for the requested language, falling back to English.
    #[must_use]
    pub fn catalog(&self, lang: Language) -> Arc<Catalog> {
        match lang {
            Language::Fr => self.default_fr.clone(),
            Language::En => self.default_en.clone(),
            Language::Custom(_) => {
                let guard = match self.catalogs.read() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                guard
                    .get(&lang)
                    .cloned()
                    .unwrap_or_else(|| self.default_en.clone())
            }
        }
    }

    /// Returns the human-readable display name for the language.
    #[must_use]
    pub fn language_name(&self, lang: Language) -> &'static str {
        match lang {
            Language::Fr => "Français",
            Language::En => "English",
            Language::Custom(_) => {
                let guard = match self.catalogs.read() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(cat) = guard.get(&lang) {
                    cat.display_name()
                } else {
                    lang.code()
                }
            }
        }
    }

    /// Translates a typed `Key` into the specified language with zero heap allocation.
    #[inline]
    #[must_use]
    pub fn translate(&self, lang: Language, key: Key) -> &'static str {
        match lang {
            Language::Fr => self.default_fr.get(key),
            Language::En => self.default_en.get(key),
            Language::Custom(_) => {
                let guard = match self.catalogs.read() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(cat) = guard.get(&lang) {
                    cat.get(key)
                } else {
                    self.default_en.get(key)
                }
            }
        }
    }

    /// Returns all currently registered and available languages.
    #[must_use]
    pub fn available_languages(&self) -> Vec<Language> {
        let guard = match self.catalogs.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut list: Vec<Language> = guard.keys().copied().collect();
        // Ensure deterministic order: FR first, then EN, then customs sorted by code
        list.sort_by(|a, b| match (a, b) {
            (Language::Fr, _) => std::cmp::Ordering::Less,
            (_, Language::Fr) => std::cmp::Ordering::Greater,
            (Language::En, _) => std::cmp::Ordering::Less,
            (_, Language::En) => std::cmp::Ordering::Greater,
            _ => a.code().cmp(b.code()),
        });
        list
    }

    /// Scans a directory for external translation files (`*.json`) and registers them.
    ///
    /// File names should correspond to the language code (e.g. `es.json`, `de.json`).
    /// Returns the number of successfully loaded external catalogs.
    pub fn load_from_directory(&self, dir: &Path) -> Result<usize, String> {
        if !dir.is_dir() {
            return Ok(0);
        }
        let read_dir = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        let mut count = 0;

        for entry in read_dir.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) == Some("json") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let code = stem.to_ascii_lowercase();
                    // Skip if matching default bundled languages
                    if code == "fr" || code == "en" {
                        continue;
                    }
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let lang = Language::custom(&code, &code);
                        if let Ok(catalog) =
                            Catalog::from_json_str(lang, &content, Some(&self.default_en))
                        {
                            self.register_catalog(catalog);
                            count += 1;
                        }
                    }
                }
            }
        }

        Ok(count)
    }
}
