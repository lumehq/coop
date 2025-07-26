use rust_i18n::Backend;

rust_i18n::i18n!("../../locales");

pub struct I18nBackend;

impl Backend for I18nBackend {
    fn available_locales(&self) -> Vec<&str> {
        _RUST_I18N_BACKEND.available_locales()
    }

    fn translate(&self, locale: &str, key: &str) -> Option<&str> {
        let val = _RUST_I18N_BACKEND.translate(locale, key);
        if val.is_none() {
            _RUST_I18N_BACKEND.translate("en", key)
        } else {
            val
        }
    }
}

#[macro_export]
macro_rules! init {
    () => {
        rust_i18n::i18n!(backend = i18n::I18nBackend);
    };
}

#[macro_export]
macro_rules! shared_t {
    ($key:expr) => {
        SharedString::new(t!($key))
    };
    ($key:expr, $($param:ident = $value:expr),+) => {
        SharedString::new(t!($key, $($param = $value),+))
    };
}

pub use rust_i18n::{set_locale, t};
