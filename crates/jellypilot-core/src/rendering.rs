//! Display-free rendering preferences and platform policy, not GPU capability probes.

use serde::{Deserialize, Serialize};

/// Application renderer preference. Changes apply on the next application launch.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RenderBackend {
    #[default]
    Auto,
    Dx12,
    Vulkan,
    Metal,
    OpenGl,
    Software,
}

impl RenderBackend {
    /// Stable identifier used by settings, launch arguments and diagnostic markers.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dx12 => "dx12",
            Self::Vulkan => "vulkan",
            Self::Metal => "metal",
            Self::OpenGl => "open-gl",
            Self::Software => "software",
        }
    }

    /// Parses canonical identifiers and the established GL/tiny-skia aliases.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "auto" => Some(Self::Auto),
            "dx12" => Some(Self::Dx12),
            "vulkan" => Some(Self::Vulkan),
            "metal" => Some(Self::Metal),
            "open-gl" | "gl" | "opengl" => Some(Self::OpenGl),
            "software" | "tiny-skia" => Some(Self::Software),
            _ => None,
        }
    }

    /// Platform choices, independent of installed adapters and drivers.
    pub fn available(platform: &str) -> &'static [Self] {
        match platform {
            "windows" => &[
                Self::Auto,
                Self::Dx12,
                Self::Vulkan,
                Self::OpenGl,
                Self::Software,
            ],
            "linux" => &[Self::Auto, Self::Vulkan, Self::OpenGl, Self::Software],
            "macos" => &[Self::Auto, Self::Metal, Self::OpenGl, Self::Software],
            _ => &[Self::Auto, Self::Software],
        }
    }

    /// Whether platform policy permits this choice; embedded playback requires Vulkan.
    pub fn supported(self, platform: &str, embedded: bool) -> bool {
        Self::available(platform).contains(&self)
            && (!embedded || matches!(self, Self::Auto | Self::Vulkan))
    }

    /// Resolves Auto deterministically without probing or silently replacing explicit choices.
    pub fn resolve_auto(self, platform: &str, embedded: bool) -> Self {
        if self != Self::Auto {
            return self;
        }
        if embedded {
            return Self::Vulkan;
        }
        match platform {
            "windows" => Self::Dx12,
            "macos" => Self::Metal,
            "linux" => Self::Vulkan,
            _ => Self::Software,
        }
    }
}

impl<'de> Deserialize<'de> for RenderBackend {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // A future or malformed preference must not discard unrelated account settings.
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(value.as_str().and_then(Self::parse).unwrap_or_default())
    }
}

/// Reads renderer launch overrides without interpreting other application arguments.
/// A standalone `--` ends option parsing. Invalid values never enter error messages.
pub fn parse_renderer_override(
    arguments: &[String],
) -> Result<Option<RenderBackend>, &'static str> {
    let mut selected = None;
    let mut arguments = arguments.iter();
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            break;
        }
        let value = if argument == "--renderer" {
            arguments
                .next()
                .map(String::as_str)
                .ok_or("Missing value for --renderer.")?
        } else if let Some(value) = argument.strip_prefix("--renderer=") {
            value
        } else {
            continue;
        };
        if selected.is_some() {
            return Err("Duplicate --renderer option.");
        }
        selected = Some(RenderBackend::parse(value).ok_or("Invalid --renderer value.")?);
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_choices_and_auto_resolution_obey_admission_policy() {
        for platform in ["windows", "linux", "macos", "unknown"] {
            for &backend in RenderBackend::available(platform) {
                assert!(backend.supported(platform, false));
                assert!(backend
                    .resolve_auto(platform, false)
                    .supported(platform, false));
                assert_eq!(
                    backend.supported(platform, true),
                    matches!(backend, RenderBackend::Auto | RenderBackend::Vulkan)
                );
            }
            assert!(RenderBackend::Auto.resolve_auto(platform, true) == RenderBackend::Vulkan);
        }
        assert!(!RenderBackend::Dx12.supported("linux", false));
        assert!(!RenderBackend::Metal.supported("windows", false));
        assert!(!RenderBackend::Vulkan.supported("macos", true));
    }

    #[test]
    fn renderer_arguments_reject_ambiguous_or_incomplete_overrides() {
        for arguments in [
            vec!["--renderer"],
            vec!["--renderer="],
            vec!["--renderer", "--external"],
            vec!["--renderer=untrusted-private-token"],
            vec!["--renderer=vulkan", "--renderer", "software"],
        ] {
            let arguments: Vec<_> = arguments.into_iter().map(str::to_owned).collect();
            let error = parse_renderer_override(&arguments).unwrap_err();
            assert!(!error.contains("untrusted-private-token"));
        }
    }

    #[test]
    fn renderer_arguments_respect_option_terminator_and_unrelated_flags() {
        let arguments: Vec<_> = [
            "--external",
            "--renderer",
            "gl",
            "--",
            "--renderer=software",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        assert_eq!(
            parse_renderer_override(&arguments),
            Ok(Some(RenderBackend::OpenGl))
        );
        assert_eq!(
            parse_renderer_override(&["--".to_owned(), "--renderer".to_owned()]),
            Ok(None)
        );
    }

    #[test]
    fn explicit_incompatible_choices_are_never_replaced_by_auto_resolution() {
        for backend in [
            RenderBackend::Dx12,
            RenderBackend::Metal,
            RenderBackend::Software,
        ] {
            assert_eq!(backend.resolve_auto("linux", true), backend);
            assert!(!backend.supported("linux", true));
        }
    }
}
