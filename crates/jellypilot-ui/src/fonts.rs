//! Bundled typefaces used by the JellyPilot iced frontend.

use std::{borrow::Cow, io, sync::LazyLock};

use iced::{
    advanced::graphics::text::{self, cosmic_text},
    font::Weight,
    Font,
};
use unicode_script::Script;

use cosmic_text::{fontdb, Fallback, FontSystem, PlatformFallback};

/// Unmodified Manrope V5 variable font, including its original 200-weight default.
pub const MANROPE: &[u8] = include_bytes!("../assets/fonts/ManropeV5VF.ttf");
/// Unmodified MiSans variable font, including its named Regular at weight 330.
pub const MISANS: &[u8] = include_bytes!("../assets/fonts/MiSansVF.ttf");

/// Primary body typeface, rendered at weight 300 rather than the font's default 200.
pub const BODY_FONT: Font = Font {
    weight: Weight::Light,
    ..Font::new("Manrope V5")
};
/// Retains the application's pre-fork line spacing during the native-corner trial.
pub const DEFAULT_LINE_HEIGHT: iced::advanced::text::LineHeight =
    iced::advanced::text::LineHeight::Relative(1.3);
/// Display typeface for page-level and hero titles, rendered at weight 400:
/// lighter than small headings because the size already carries hierarchy.
pub const DISPLAY_FONT: Font = Font {
    weight: Weight::Normal,
    ..BODY_FONT
};
/// Primary heading typeface, rendered at weight 500.
pub const HEADING_FONT: Font = Font {
    weight: Weight::Medium,
    ..BODY_FONT
};

/// Attribution retained in the application's font notices.
pub const FONT_ATTRIBUTIONS: &str = "\
Manrope V5 by Mikhail Sharanda. © 2025 Mikhail Sharanda. All Rights Reserved.\n\
This software uses MiSans fonts.\n\
Copyright © 2020-2025 Beijing Xiaomi Mobile Software Co.,Ltd. All Rights Reserved.";

/// Full original font agreements and copyright notices for the application's legal view.
pub const FONT_LICENSES: &str = concat!(
    include_str!("../assets/fonts/MANROPE-LICENSE.txt"),
    "\n\n",
    include_str!("../assets/fonts/MISANS-LICENSE.txt"),
);

/// Returns both bundled typefaces, independent of the interface language.
pub const fn fonts() -> [&'static [u8]; 2] {
    [MANROPE, MISANS]
}

/// Registers bundled fonts and deterministic Chinese fallback in iced's font system.
///
/// Call before constructing the application, before any paragraphs or renderer caches
/// exist. Subsequent calls are harmless. Loading the bytes alone does not configure
/// fallback or make variable weights eligible for cosmic-text's font matching.
pub fn initialize() -> Result<(), io::Error> {
    static INITIALIZED: LazyLock<Result<(), &'static str>> = LazyLock::new(|| {
        let mut system = text::font_system()
            .write()
            .map_err(|_| "iced font system lock is poisoned")?;
        for bytes in fonts() {
            system.load_font(Cow::Borrowed(bytes));
        }
        configure(system.raw())
    });
    (*INITIALIZED).map_err(io::Error::other)
}

fn configure(system: &mut FontSystem) -> Result<(), &'static str> {
    // fontdb reads OS/2 weight, not the variable axis range. cosmic-text 0.15
    // only considers exact-weight faces for primary and configured fallback
    // families. Register runtime matching descriptors for our three semantic
    // weights; the shaper and rasterizer apply the real wght axis to the shared,
    // unchanged source. Keep the original face's weight and names intact.
    for family in ["Manrope V5", "MiSans VF"] {
        let db = system.db_mut();
        let original = db
            .faces()
            .filter(|face| face.families.iter().any(|(name, _)| name == family))
            .last()
            .cloned()
            .ok_or("bundled font could not be registered")?;
        // Prefer the just-loaded bundled source over installed copies with the
        // same family name, regardless of system enumeration order.
        let conflicting: Vec<_> = db
            .faces()
            .filter(|face| face.families.iter().any(|(name, _)| name == family))
            .map(|face| face.id)
            .collect();
        for id in conflicting {
            db.remove_face(id);
        }
        for font in [BODY_FONT, DISPLAY_FONT, HEADING_FONT] {
            let mut instance = original.clone();
            instance.id = fontdb::ID::dummy();
            instance.weight = text::to_attributes(font).weight;
            db.push_face_info(instance);
        }
        let mut original = original;
        original.id = fontdb::ID::dummy();
        db.push_face_info(original);
    }
    let locale = system.locale().to_owned();
    let fallback = BundledFallback::new(&locale);
    let db = std::mem::take(system.db_mut());
    *system = FontSystem::new_with_locale_and_db_and_fallback(locale, db, fallback);
    Ok(())
}

struct BundledFallback {
    common: Vec<&'static str>,
    han: Vec<&'static str>,
}

impl BundledFallback {
    fn new(locale: &str) -> Self {
        let mut common = vec!["MiSans VF"];
        common.extend_from_slice(PlatformFallback.common_fallback());
        let mut han = vec!["MiSans VF"];
        han.extend_from_slice(PlatformFallback.script_fallback(Script::Han, locale));
        Self { common, han }
    }
}

impl Fallback for BundledFallback {
    fn common_fallback(&self) -> &[&'static str] {
        &self.common
    }

    fn forbidden_fallback(&self) -> &[&'static str] {
        PlatformFallback.forbidden_fallback()
    }

    fn script_fallback(&self, script: Script, locale: &str) -> &[&'static str] {
        if script == Script::Han {
            &self.han
        } else {
            PlatformFallback.script_fallback(script, locale)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use cosmic_text::{Buffer, Metrics, SwashCache};

    #[test]
    fn mixed_text_uses_bundled_families_and_real_semantic_weight_variations() {
        let mut db = fontdb::Database::new();
        // Include real installed competitors, but also work on fontless CI hosts.
        db.load_system_fonts();
        for bytes in fonts() {
            db.load_font_source(fontdb::Source::Binary(Arc::new(bytes)));
        }
        let mut system = FontSystem::new_with_locale_and_db("en-US".to_owned(), db);
        configure(&mut system).expect("bundled font setup");
        let sample = "Hello中文，";
        let mut cache = SwashCache::new();
        for font in [BODY_FONT, DISPLAY_FONT, HEADING_FONT] {
            let attrs = text::to_attributes(font);
            let mut buffer = Buffer::new(&mut system, Metrics::new(32.0, 40.0));
            buffer.set_text(
                sample,
                &attrs,
                text::to_shaping(iced::advanced::text::Shaping::default(), sample),
                None,
            );
            buffer.shape_until_scroll(&mut system, false);
            let glyphs: Vec<_> = buffer
                .layout_runs()
                .flat_map(|run| run.glyphs.iter().cloned())
                .collect();
            for (cluster, family, default_weight) in
                [("H", "Manrope V5", 200), ("中", "MiSans VF", 330)]
            {
                let glyph = glyphs
                    .iter()
                    .find(|glyph| &sample[glyph.start..glyph.end] == cluster)
                    .expect("requested character has a shaped glyph");
                let face = system.db().face(glyph.font_id).expect("selected face");
                assert!(
                    face.families.iter().any(|(name, _)| name == family),
                    "{cluster} selected {:?}, expected {family}",
                    face.families
                );
                assert_eq!(glyph.font_weight, attrs.weight);
                assert_ne!(glyph.glyph_id, 0, "missing glyph for {cluster}");
                let key = glyph.physical((0.0, 0.0), 1.0).cache_key;
                let image = cache
                    .get_image_uncached(&mut system, key)
                    .expect("semantic-weight glyph rasterizes");
                let default_image = cache
                    .get_image_uncached(
                        &mut system,
                        cosmic_text::CacheKey {
                            font_weight: fontdb::Weight(default_weight),
                            ..key
                        },
                    )
                    .expect("default-weight glyph rasterizes");
                assert_ne!(
                    image.data, default_image.data,
                    "{family} must render the requested weight, not its axis default"
                );
            }
            // Basic ASCII layout uses default variable coordinates while rasterizing
            // at the requested weight. Default widget layout must use matching metrics.
            let mut measure_ascii = |shaping| {
                let mut paragraph = Buffer::new(&mut system, Metrics::new(32.0, 40.0));
                paragraph.set_text("HiW", &attrs, text::to_shaping(shaping, "HiW"), None);
                paragraph.shape_until_scroll(&mut system, false);
                paragraph.layout_runs().map(|run| run.line_w).sum::<f32>()
            };
            let default_width = measure_ascii(iced::advanced::text::Shaping::default());
            let shaped_width = measure_ascii(iced::advanced::text::Shaping::Advanced);
            assert!(
                (default_width - shaped_width).abs() < 0.001,
                "ASCII layout and glyph rendering must use the same variable-font coordinates"
            );
        }
    }
}
