//! Saved UI language preference and pure, ordered system-language negotiation.

use std::borrow::Cow;

use icu_locale_core::Locale;

/// A language shipped by the application, independent of system locale syntax.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum UiLanguage {
    English,
    SimplifiedChinese,
}

impl UiLanguage {
    pub const fn tag(self) -> &'static str {
        match self {
            Self::English => "en-US",
            Self::SimplifiedChinese => "zh-CN",
        }
    }
}

/// The saved choice, not the language that System resolved on a previous run.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum LanguagePreference {
    #[default]
    System,
    Fixed(UiLanguage),
}

#[cfg(feature = "native")]
impl serde::Serialize for LanguagePreference {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::System => "system",
            Self::Fixed(language) => language.tag(),
        })
    }
}

#[cfg(feature = "native")]
impl<'de> serde::Deserialize<'de> for LanguagePreference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <serde_json::Value as serde::Deserialize>::deserialize(deserializer)?;
        Ok(match value.as_str() {
            Some("en-US") => Self::Fixed(UiLanguage::English),
            Some("zh-CN") => Self::Fixed(UiLanguage::SimplifiedChinese),
            _ => Self::System,
        })
    }
}

/// Resolves the first supported system candidate, with English as the final fallback.
/// Explicit Chinese scripts take precedence over regions. Valid extensions do not
/// affect matching; malformed tags are skipped rather than matched by a prefix.
pub fn resolve_language<S: AsRef<str>>(
    preference: LanguagePreference,
    system: impl IntoIterator<Item = S>,
) -> UiLanguage {
    if let LanguagePreference::Fixed(language) = preference {
        return language;
    }
    system
        .into_iter()
        .find_map(|candidate| match_system_language(candidate.as_ref()))
        .unwrap_or(UiLanguage::English)
}

fn match_system_language(candidate: &str) -> Option<UiLanguage> {
    // POSIX encodings are not BCP 47 subtags. Reject malformed suffixes instead
    // of turning arbitrary trailing text into an otherwise supported locale.
    let tag = if let Some((tag, encoding)) = candidate.split_once('.') {
        if encoding.is_empty()
            || !encoding
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return None;
        }
        tag
    } else {
        candidate
    };
    let tag = if tag.contains('_') {
        Cow::Owned(tag.replace('_', "-"))
    } else {
        Cow::Borrowed(tag)
    };
    let locale = Locale::try_from_str(&tag).ok()?;
    match locale.id.language.as_str() {
        "en" => Some(UiLanguage::English),
        "zh" => match locale.id.script.as_ref().map(|script| script.as_str()) {
            Some("Hans") => Some(UiLanguage::SimplifiedChinese),
            Some(_) => None,
            None => match locale.id.region.as_ref().map(|region| region.as_str()) {
                None | Some("CN" | "SG") => Some(UiLanguage::SimplifiedChinese),
                Some(_) => None,
            },
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_script_overrides_conflicting_region() {
        assert_eq!(
            resolve_language(LanguagePreference::System, ["zh-Hant-CN", "en-GB", "zh-CN"]),
            UiLanguage::English
        );
        assert_eq!(
            resolve_language(LanguagePreference::System, ["zh-Hans-TW", "en-US"]),
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn unsupported_candidates_continue_to_supported_secondary_preference() {
        assert_eq!(
            resolve_language(
                LanguagePreference::System,
                [
                    "fr-FR",
                    "zh-TW",
                    "zh-HK",
                    "zh-MO",
                    "zh-US",
                    "zh-Latn-CN",
                    "zh-SG",
                    "en-US"
                ]
            ),
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn extensions_are_validated_before_matching_and_do_not_override_script() {
        assert_eq!(
            resolve_language(
                LanguagePreference::System,
                [
                    "en-US-u",
                    "zh-Hant-CN-u-nu-latn",
                    "zh-CN-u-nu-hanidec-x-app"
                ]
            ),
            UiLanguage::SimplifiedChinese
        );
        assert_eq!(
            resolve_language(LanguagePreference::System, ["en-GB-u-ca-gregory", "zh-CN"]),
            UiLanguage::English
        );
    }

    #[test]
    fn posix_encoding_and_separators_preserve_explicit_script() {
        assert_eq!(
            resolve_language(
                LanguagePreference::System,
                ["zh_Hant_CN.UTF-8", "en_US.UTF-8", "zh"]
            ),
            UiLanguage::English
        );
        assert_eq!(
            resolve_language(LanguagePreference::System, ["zh_CN.UTF-8", "en-US"]),
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn malformed_candidates_do_not_hide_later_supported_language() {
        assert_eq!(
            resolve_language(
                LanguagePreference::System,
                [
                    "",
                    "C",
                    "POSIX",
                    "en--US",
                    "en_US.",
                    "en_US.UTF-8@invalid",
                    "en-US-!",
                    "中文",
                    "zh"
                ]
            ),
            UiLanguage::SimplifiedChinese
        );
    }

    #[test]
    fn fixed_preference_does_not_consume_system_candidates() {
        let candidates = std::iter::from_fn(|| -> Option<&str> {
            panic!("fixed language must not inspect system candidates")
        });
        assert_eq!(
            resolve_language(
                LanguagePreference::Fixed(UiLanguage::SimplifiedChinese),
                candidates
            ),
            UiLanguage::SimplifiedChinese
        );
    }
}
