#![forbid(unsafe_code)]

//! In-memory immutable translation catalog mapping typed `Key` to zero-allocation static strings.

use std::collections::HashMap;

use crate::i18n::{Key, Language, ALL_KEYS};

/// An immutable, indexed catalog of translations for a specific language.
#[derive(Debug, Clone)]
pub struct Catalog {
    language: Language,
    display_name: &'static str,
    entries: Box<[&'static str; Key::COUNT]>,
    extra: HashMap<String, &'static str>,
}

impl Catalog {
    /// Constructs a catalog from a JSON string representing a map of `{ "KeyName": "Translated text" }`.
    ///
    /// Any keys missing from the JSON payload fall back to the provided `fallback` catalog,
    /// or to the key's identifier name if no fallback is available.
    /// Any additional dynamic keys (e.g., `"Preset.AsShot.Name"`) are stored for zero-allocation lookup.
    pub fn from_json_str(
        language: Language,
        json_str: &str,
        fallback: Option<&Catalog>,
    ) -> Result<Self, String> {
        let mut map: HashMap<String, String> =
            serde_json::from_str(json_str).map_err(|e| format!("i18n JSON parse error: {e}"))?;

        let mut entries: Box<[&'static str; Key::COUNT]> = Box::new([""; Key::COUNT]);

        for (i, &key) in ALL_KEYS.iter().enumerate() {
            let key_name = key.as_str();
            let text: &'static str = if let Some(val) = map.remove(key_name) {
                Box::leak(val.into_boxed_str())
            } else if let Some(fb) = fallback {
                fb.get(key)
            } else {
                key_name
            };
            entries[i] = text;
        }

        let display_name: &'static str = match language {
            Language::Fr => "Français",
            Language::En => "English",
            Language::Custom(_) => {
                if let Some(name_str) = map.remove("_language_name") {
                    Box::leak(name_str.into_boxed_str())
                } else {
                    language.code()
                }
            }
        };

        let mut extra = fallback.map_or_else(HashMap::new, |fb| fb.extra.clone());
        for (k, v) in map {
            if !k.starts_with('_') {
                extra.insert(k, Box::leak(v.into_boxed_str()));
            }
        }

        Ok(Self {
            language,
            display_name,
            entries,
            extra,
        })
    }

    /// The language of this catalog.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// Full human-readable display name of this language catalog.
    #[must_use]
    pub fn display_name(&self) -> &'static str {
        self.display_name
    }

    /// Constructs an empty fallback catalog with default key identifiers.
    #[must_use]
    pub fn empty(language: Language) -> Self {
        let mut entries: Box<[&'static str; Key::COUNT]> = Box::new([""; Key::COUNT]);
        for (i, &key) in ALL_KEYS.iter().enumerate() {
            entries[i] = key.as_str();
        }
        let display_name = match language {
            Language::Fr => "Français",
            Language::En => "English",
            Language::Custom(_) => language.code(),
        };
        Self {
            language,
            display_name,
            entries,
            extra: HashMap::new(),
        }
    }

    /// Retrieves the static string translation for the specified typed `Key` with zero heap allocation.
    #[inline]
    #[must_use]
    pub fn get(&self, key: Key) -> &'static str {
        let idx = key.index();
        if idx < Key::COUNT {
            self.entries[idx]
        } else {
            key.as_str()
        }
    }

    /// Retrieves a dynamic custom translation entry (e.g. preset names/descriptions or rendering intents) if defined in the JSON catalog.
    #[inline]
    #[must_use]
    pub fn get_custom(&self, custom_key: &str) -> Option<&'static str> {
        self.extra.get(custom_key).copied()
    }
}
