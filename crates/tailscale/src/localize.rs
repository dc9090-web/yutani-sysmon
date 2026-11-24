//! Fluent strings from `i18n/`, embedded in the binary.

use std::sync::LazyLock;

use i18n_embed::fluent::{FluentLanguageLoader, fluent_language_loader};
use i18n_embed::{DefaultLocalizer, LanguageLoader, Localizer};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

pub static LANGUAGE_LOADER: LazyLock<FluentLanguageLoader> = LazyLock::new(|| {
    let loader: FluentLanguageLoader = fluent_language_loader!();
    loader.load_fallback_language(&Localizations).expect("the fallback language is embedded");
    // No Unicode isolation marks around placeables: they show up as boxes
    // in the panel tooltip.
    loader.set_use_isolating(false);
    loader
});

#[macro_export]
macro_rules! fl {
    ($id:literal) => {{ i18n_embed_fl::fl!($crate::localize::LANGUAGE_LOADER, $id) }};
    ($id:literal, $($args:expr),*) => {{ i18n_embed_fl::fl!($crate::localize::LANGUAGE_LOADER, $id, $($args),*) }};
}

pub fn localize() {
    let localizer = DefaultLocalizer::new(&*LANGUAGE_LOADER, &Localizations);
    let requested = i18n_embed::DesktopLanguageRequester::requested_languages();
    if let Err(e) = localizer.select(&requested) {
        tracing::warn!("loading languages: {e}");
    }
}
