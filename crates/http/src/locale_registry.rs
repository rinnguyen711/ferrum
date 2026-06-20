//! In-memory cache of the locale set, mirroring `RoleRegistry`. Hydrated at
//! boot from `_locales` and reloaded on every mutation.

use rustapi_sql::Locale;
use tokio::sync::RwLock;

#[derive(Debug, Default)]
struct Inner {
    locales: Vec<Locale>,
    default_code: String,
}

#[derive(Debug, Default)]
pub struct LocaleRegistry {
    inner: RwLock<Inner>,
}

impl LocaleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace the cache contents. The default is the `is_default` locale, or
    /// the first locale, or "en" if empty.
    pub async fn set(&self, locales: Vec<Locale>) {
        let default_code = locales
            .iter()
            .find(|l| l.is_default)
            .or_else(|| locales.first())
            .map(|l| l.code.clone())
            .unwrap_or_else(|| "en".to_string());
        let mut w = self.inner.write().await;
        w.locales = locales;
        w.default_code = default_code;
    }

    /// True if `code` is a known locale.
    pub async fn contains(&self, code: &str) -> bool {
        self.inner
            .read()
            .await
            .locales
            .iter()
            .any(|l| l.code == code)
    }

    /// The default locale code.
    pub async fn default_code(&self) -> String {
        self.inner.read().await.default_code.clone()
    }

    /// Resolve the requested locale (or the default when `None`). Returns
    /// `None` if `requested` is a non-empty unknown code (caller → 422).
    pub async fn resolve(&self, requested: Option<&str>) -> Option<String> {
        let r = self.inner.read().await;
        match requested {
            None => Some(r.default_code.clone()),
            Some(code) => {
                if r.locales.iter().any(|l| l.code == code) {
                    Some(code.to_string())
                } else {
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loc(code: &str, def: bool) -> Locale {
        Locale {
            code: code.into(),
            name: code.into(),
            is_default: def,
            position: 0,
        }
    }

    #[tokio::test]
    async fn resolve_default_and_known_and_unknown() {
        let reg = LocaleRegistry::new();
        reg.set(vec![loc("en", true), loc("fr", false)]).await;
        assert_eq!(reg.default_code().await, "en");
        assert_eq!(reg.resolve(None).await.as_deref(), Some("en"));
        assert_eq!(reg.resolve(Some("fr")).await.as_deref(), Some("fr"));
        assert_eq!(reg.resolve(Some("de")).await, None);
        assert!(reg.contains("fr").await);
        assert!(!reg.contains("de").await);
    }

    #[tokio::test]
    async fn default_falls_back_to_en_when_empty() {
        let reg = LocaleRegistry::new();
        reg.set(vec![]).await;
        assert_eq!(reg.default_code().await, "en");
    }
}
