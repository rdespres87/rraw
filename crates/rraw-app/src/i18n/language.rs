#![forbid(unsafe_code)]

//! Language identifier representation and metadata for `rraw-app`.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::i18n::I18nEngine;

/// Supported application display languages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    /// French (Français) - Default display language.
    #[default]
    Fr,
    /// English.
    En,
    /// Dynamic external language identified by short ISO code (up to 7 ASCII characters).
    Custom([u8; 8]),
}

static INTERNED_CODES: OnceLock<Mutex<HashMap<[u8; 8], &'static str>>> = OnceLock::new();

impl Language {
    /// Built-in static languages.
    pub const ALL: [Self; 2] = [Self::Fr, Self::En];

    /// Returns true if this language represents Spanish (`es` / `es-*`).
    #[inline]
    #[must_use]
    pub const fn is_spanish(self) -> bool {
        matches!(self, Self::Custom([b'e', b's', ..]))
    }

    /// Returns the uppercase two-letter or short code (e.g. "FR", "EN", "ES").
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::Fr => "FR",
            Self::En => "EN",
            Self::Custom(bytes) => {
                let intern_map = INTERNED_CODES.get_or_init(|| Mutex::new(HashMap::new()));
                let mut guard = match intern_map.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                if let Some(&cached) = guard.get(&bytes) {
                    return cached;
                }
                let len = bytes.iter().position(|&b| b == 0).unwrap_or(8);
                let raw_str = String::from_utf8_lossy(&bytes[..len]);
                let upper: &'static str = Box::leak(raw_str.to_uppercase().into_boxed_str());
                guard.insert(bytes, upper);
                upper
            }
        }
    }

    /// Full human-readable display name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fr => "Français",
            Self::En => "English",
            Self::Custom(_) => I18nEngine::global().language_name(self),
        }
    }

    /// Cycle to the next registered language.
    #[must_use]
    pub fn next(self) -> Self {
        let available = I18nEngine::global().available_languages();
        if available.is_empty() {
            return match self {
                Self::Fr => Self::En,
                Self::En => Self::Fr,
                Self::Custom(_) => Self::Fr,
            };
        }
        if let Some(pos) = available.iter().position(|&l| l == self) {
            let next_idx = (pos + 1) % available.len();
            available[next_idx]
        } else {
            Self::Fr
        }
    }

    /// Constructs a `Language::Custom` from a string code and human-readable name.
    #[must_use]
    pub fn custom(code: &str, _name: &str) -> Self {
        let trimmed = code.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "fr" => Self::Fr,
            "en" => Self::En,
            _ => {
                let mut buf = [0u8; 8];
                let bytes = trimmed.as_bytes();
                let copy_len = bytes.len().min(7);
                buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                Self::Custom(buf)
            }
        }
    }

    /// Converts this language code to a lowercase string slice (e.g. "fr", "en", "es").
    #[must_use]
    pub fn as_lowercase_str(self) -> &'static str {
        match self {
            Self::Fr => "fr",
            Self::En => "en",
            Self::Custom(_) => {
                let upper = self.code();
                let intern_map = INTERNED_CODES.get_or_init(|| Mutex::new(HashMap::new()));
                let mut guard = match intern_map.lock() {
                    Ok(g) => g,
                    Err(poisoned) => poisoned.into_inner(),
                };
                let mut buf = [0u8; 8];
                let bytes = upper.as_bytes();
                let copy_len = bytes.len().min(7);
                buf[..copy_len].copy_from_slice(&bytes[..copy_len]);
                buf[7] = 1; // differentiation tag for lowercase cache
                if let Some(&cached) = guard.get(&buf) {
                    return cached;
                }
                let lower: &'static str = Box::leak(upper.to_lowercase().into_boxed_str());
                guard.insert(buf, lower);
                lower
            }
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl Serialize for Language {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_lowercase_str())
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Self::custom(&s, &s))
    }
}
