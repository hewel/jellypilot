//! Curated Tabler outline/filled SVG icon catalog and iced widget helpers.

use std::sync::LazyLock;

use iced::widget::button;
use iced::widget::svg::{self, Handle, Svg};
use iced::{Color, Length, Theme};

use crate::tokens::palette;
use crate::variants::ButtonVariant;

/// Default icon dimension in logical pixels (18.0px).
pub const DEFAULT_ICON_SIZE: f32 = IconSize::MD;

/// Semantic icon size constants in logical pixels.
pub const ICON_SIZE_XS: f32 = IconSize::XS;
pub const ICON_SIZE_SM: f32 = IconSize::SM;
pub const ICON_SIZE_MD: f32 = IconSize::MD;
pub const ICON_SIZE_LG: f32 = IconSize::LG;
pub const ICON_SIZE_XL: f32 = IconSize::XL;
pub const ICON_SIZE_2XL: f32 = IconSize::X2L;

/// Semantic icon sizes in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IconSize {
    /// 14.0px — Compact inline icons, badges, and dense actions.
    Xs,
    /// 16.0px — Standard buttons, menu items, and input adornments.
    Sm,
    /// 18.0px — Primary navigation, prominent actions, default size.
    Md,
    /// 20.0px — Transport controls, section headers.
    Lg,
    /// 24.0px — Large display icons.
    Xl,
    /// 28.0px — Hero display icons (e.g. QR codes, splash indicators).
    X2l,
    /// Custom size in logical pixels.
    Custom(f32),
}

impl IconSize {
    pub const XS: f32 = 14.0;
    pub const SM: f32 = 16.0;
    pub const MD: f32 = 18.0;
    pub const LG: f32 = 20.0;
    pub const XL: f32 = 24.0;
    pub const X2L: f32 = 28.0;

    /// Logical pixel dimension for this size variant.
    #[must_use]
    pub const fn pixels(self) -> f32 {
        match self {
            Self::Xs => Self::XS,
            Self::Sm => Self::SM,
            Self::Md => Self::MD,
            Self::Lg => Self::LG,
            Self::Xl => Self::XL,
            Self::X2l => Self::X2L,
            Self::Custom(px) => px,
        }
    }
}

impl From<IconSize> for f32 {
    fn from(size: IconSize) -> Self {
        size.pixels()
    }
}

impl From<f32> for IconSize {
    fn from(px: f32) -> Self {
        if (px - IconSize::XS).abs() < f32::EPSILON {
            Self::Xs
        } else if (px - IconSize::SM).abs() < f32::EPSILON {
            Self::Sm
        } else if (px - IconSize::MD).abs() < f32::EPSILON {
            Self::Md
        } else if (px - IconSize::LG).abs() < f32::EPSILON {
            Self::Lg
        } else if (px - IconSize::XL).abs() < f32::EPSILON {
            Self::Xl
        } else if (px - IconSize::X2L).abs() < f32::EPSILON {
            Self::X2l
        } else {
            Self::Custom(px)
        }
    }
}

const ICON_COUNT: usize = 51;
static ICON_HANDLES: LazyLock<[Handle; ICON_COUNT]> = LazyLock::new(|| {
    let mut handles = std::array::from_fn(|_| Handle::from_memory(&[]));
    for icon in Icon::all() {
        handles[icon.index()] = Handle::from_memory(icon.bytes());
    }
    handles
});

/// Semantic icon variants vendored from Tabler icons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    // Media transport
    Play,
    Pause,
    Stop,
    Previous,
    Next,
    VolumeHigh,
    VolumeMute,
    AudioTrack,
    Subtitles,
    IntroSkip,

    // Navigation & library types
    Home,
    Movie,
    Tv,
    Music,
    Photo,
    Folder,
    Search,
    Settings,

    // Actions & state toggles
    Heart,
    HeartFilled,
    Check,
    CircleCheck,
    Circle,
    CircleDot,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronDown,
    ArrowUp,
    ArrowDown,
    SortAscending,
    SortDescending,
    Filter,
    Refresh,
    Trash,
    Close,

    // Settings & diagnostics
    Server,
    Cpu,
    Sliders,
    Keyboard,
    Database,
    Activity,
    Info,
    Warning,
    Error,

    // Login & authentication
    QrCode,
    Lock,
    User,
    UserCheck,
    // App mode switching
    PictureInPicture,
    ArrowsMaximize,
}

impl Icon {
    /// Returns the raw SVG byte slice for this icon.
    #[must_use]
    pub const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Play => include_bytes!("../assets/icons/player-play.svg"),
            Self::Pause => include_bytes!("../assets/icons/player-pause.svg"),
            Self::Stop => include_bytes!("../assets/icons/player-stop.svg"),
            Self::Previous => include_bytes!("../assets/icons/player-skip-back.svg"),
            Self::Next => include_bytes!("../assets/icons/player-skip-forward.svg"),
            Self::VolumeHigh => include_bytes!("../assets/icons/volume.svg"),
            Self::VolumeMute => include_bytes!("../assets/icons/volume-off.svg"),
            Self::AudioTrack => include_bytes!("../assets/icons/headphones.svg"),
            Self::Subtitles => include_bytes!("../assets/icons/subtitles.svg"),
            Self::IntroSkip => include_bytes!("../assets/icons/sparkles.svg"),

            Self::Home => include_bytes!("../assets/icons/home.svg"),
            Self::Movie => include_bytes!("../assets/icons/movie.svg"),
            Self::Tv => include_bytes!("../assets/icons/device-tv.svg"),
            Self::Music => include_bytes!("../assets/icons/music.svg"),
            Self::Photo => include_bytes!("../assets/icons/photo.svg"),
            Self::Folder => include_bytes!("../assets/icons/folder.svg"),
            Self::Search => include_bytes!("../assets/icons/search.svg"),
            Self::Settings => include_bytes!("../assets/icons/settings.svg"),

            Self::Heart => include_bytes!("../assets/icons/heart.svg"),
            Self::HeartFilled => include_bytes!("../assets/icons/heart-filled.svg"),
            Self::Check => include_bytes!("../assets/icons/check.svg"),
            Self::CircleCheck => include_bytes!("../assets/icons/circle-check.svg"),
            Self::Circle => include_bytes!("../assets/icons/circle.svg"),
            Self::CircleDot => include_bytes!("../assets/icons/circle-dot.svg"),
            Self::ChevronLeft => include_bytes!("../assets/icons/chevron-left.svg"),
            Self::ChevronRight => include_bytes!("../assets/icons/chevron-right.svg"),
            Self::ChevronUp => include_bytes!("../assets/icons/chevron-up.svg"),
            Self::ChevronDown => include_bytes!("../assets/icons/chevron-down.svg"),
            Self::ArrowUp => include_bytes!("../assets/icons/arrow-up.svg"),
            Self::ArrowDown => include_bytes!("../assets/icons/arrow-down.svg"),
            Self::SortAscending => include_bytes!("../assets/icons/sort-ascending.svg"),
            Self::SortDescending => include_bytes!("../assets/icons/sort-descending.svg"),
            Self::Filter => include_bytes!("../assets/icons/filter.svg"),
            Self::Refresh => include_bytes!("../assets/icons/refresh.svg"),
            Self::Trash => include_bytes!("../assets/icons/trash.svg"),
            Self::Close => include_bytes!("../assets/icons/x.svg"),

            Self::Server => include_bytes!("../assets/icons/server.svg"),
            Self::Cpu => include_bytes!("../assets/icons/cpu.svg"),
            Self::Sliders => include_bytes!("../assets/icons/adjustments.svg"),
            Self::Keyboard => include_bytes!("../assets/icons/keyboard.svg"),
            Self::Database => include_bytes!("../assets/icons/database.svg"),
            Self::Activity => include_bytes!("../assets/icons/activity.svg"),
            Self::Info => include_bytes!("../assets/icons/info-circle.svg"),
            Self::Warning => include_bytes!("../assets/icons/alert-triangle.svg"),
            Self::Error => include_bytes!("../assets/icons/alert-circle.svg"),

            Self::QrCode => include_bytes!("../assets/icons/qrcode.svg"),
            Self::Lock => include_bytes!("../assets/icons/lock.svg"),
            Self::User => include_bytes!("../assets/icons/user.svg"),
            Self::UserCheck => include_bytes!("../assets/icons/user-check.svg"),
            Self::PictureInPicture => include_bytes!("../assets/icons/picture-in-picture.svg"),
            Self::ArrowsMaximize => include_bytes!("../assets/icons/arrows-maximize.svg"),
        }
    }

    /// Returns the zero-based index of this icon variant.
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Returns a cached iced SVG handle cloned for this icon.
    #[must_use]
    pub fn handle(self) -> Handle {
        ICON_HANDLES[self.index()].clone()
    }

    /// Resolves an appropriate icon for a media library collection type string.
    #[must_use]
    pub fn for_collection_type(collection_type: &str) -> Self {
        let normalized = collection_type.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "movies" | "movie" | "films" | "film" | "videos" | "video" => Self::Movie,
            "tvshows" | "tvshow" | "series" | "shows" | "show" | "tv" => Self::Tv,
            "music" | "songs" | "audio" | "audiobooks" => Self::Music,
            "photos" | "photo" | "homevideos" | "pictures" => Self::Photo,
            _ => Self::Folder,
        }
    }

    /// Slice of all defined [`Icon`] variants.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Play,
            Self::Pause,
            Self::Stop,
            Self::Previous,
            Self::Next,
            Self::VolumeHigh,
            Self::VolumeMute,
            Self::AudioTrack,
            Self::Subtitles,
            Self::IntroSkip,
            Self::Home,
            Self::Movie,
            Self::Tv,
            Self::Music,
            Self::Photo,
            Self::Folder,
            Self::Search,
            Self::Settings,
            Self::Heart,
            Self::HeartFilled,
            Self::Check,
            Self::CircleCheck,
            Self::Circle,
            Self::CircleDot,
            Self::ChevronLeft,
            Self::ChevronRight,
            Self::ChevronUp,
            Self::ChevronDown,
            Self::ArrowUp,
            Self::ArrowDown,
            Self::SortAscending,
            Self::SortDescending,
            Self::Filter,
            Self::Refresh,
            Self::Trash,
            Self::Close,
            Self::Server,
            Self::Cpu,
            Self::Sliders,
            Self::Keyboard,
            Self::Database,
            Self::Activity,
            Self::Info,
            Self::Warning,
            Self::Error,
            Self::QrCode,
            Self::Lock,
            Self::User,
            Self::UserCheck,
            Self::PictureInPicture,
            Self::ArrowsMaximize,
        ]
    }
}

/// Creates a standard-sized iced `Svg` widget with default surface text color.
pub fn icon<'a>(icon: Icon) -> Svg<'a, Theme> {
    icon_sized(icon, IconSize::Md)
}

/// Creates an iced `Svg` widget with explicit size and default surface text color.
pub fn icon_sized<'a>(icon: Icon, size: impl Into<IconSize>) -> Svg<'a, Theme> {
    let px = size.into().pixels();
    Svg::new(icon.handle())
        .width(Length::Fixed(px))
        .height(Length::Fixed(px))
        .style(|theme: &Theme, _status| svg::Style {
            color: Some(palette(theme).colors.onSurface),
        })
}

/// Creates an iced `Svg` widget with explicit size and color styling.
pub fn icon_with_color<'a>(icon: Icon, size: impl Into<IconSize>, color: Color) -> Svg<'a, Theme> {
    let px = size.into().pixels();
    Svg::new(icon.handle())
        .width(Length::Fixed(px))
        .height(Length::Fixed(px))
        .style(move |_theme: &Theme, _status| svg::Style { color: Some(color) })
}

/// Creates an iced `Svg` widget with colors matching a button variant.
pub fn icon_for_variant<'a>(
    icon: Icon,
    size: impl Into<IconSize>,
    variant: ButtonVariant,
) -> Svg<'a, Theme> {
    icon_for_variant_disabled(icon, size, variant, false)
}

/// Creates an iced `Svg` widget with colors matching a button variant and disabled status.
pub fn icon_for_variant_disabled<'a>(
    icon: Icon,
    size: impl Into<IconSize>,
    variant: ButtonVariant,
    disabled: bool,
) -> Svg<'a, Theme> {
    let px = size.into().pixels();
    Svg::new(icon.handle())
        .width(Length::Fixed(px))
        .height(Length::Fixed(px))
        .style(move |theme: &Theme, _status| {
            let colors = palette(theme).colors;
            let mut color = match variant {
                ButtonVariant::Primary => colors.onPrimary,
                ButtonVariant::Secondary => colors.onSecondaryContainer,
                ButtonVariant::Tonal | ButtonVariant::TonalActive => colors.onSurface,
                ButtonVariant::Text => colors.secondary,
                ButtonVariant::Icon => colors.onSurfaceVariant,
            };
            if disabled {
                color.a *= 0.5;
            }
            svg::Style { color: Some(color) }
        })
}

/// Creates an iced `Svg` widget with colors matching a button variant and interaction status.
pub fn icon_for_variant_status<'a>(
    icon: Icon,
    size: impl Into<IconSize>,
    variant: ButtonVariant,
    status: button::Status,
) -> Svg<'a, Theme> {
    icon_for_variant_disabled(
        icon,
        size,
        variant,
        matches!(status, button::Status::Disabled),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_count_matches_variant_slice() {
        assert_eq!(Icon::all().len(), ICON_COUNT);
        for (idx, icon) in Icon::all().iter().enumerate() {
            assert_eq!(icon.index(), idx);
        }
    }

    #[test]
    fn every_icon_variant_has_valid_non_empty_svg_bytes() {
        for icon in Icon::all() {
            let bytes = icon.bytes();
            assert!(!bytes.is_empty(), "Icon {icon:?} must not have empty bytes");
            let text = std::str::from_utf8(bytes)
                .unwrap_or_else(|_| panic!("Icon {icon:?} must be valid UTF-8"));

            // XML structure verification via roxmltree
            let doc = roxmltree::Document::parse(text)
                .unwrap_or_else(|err| panic!("Icon {icon:?} failed XML parsing: {err}"));
            assert_eq!(
                doc.root_element().tag_name().name(),
                "svg",
                "Icon {icon:?} root element must be <svg>"
            );
            assert!(
                doc.root_element().default_namespace() == Some("http://www.w3.org/2000/svg")
                    || doc
                        .root_element()
                        .namespaces()
                        .any(|ns| ns.uri() == "http://www.w3.org/2000/svg"),
                "Icon {icon:?} must declare xmlns=\"http://www.w3.org/2000/svg\""
            );
            assert!(
                doc.root_element().attribute("viewBox").is_some(),
                "Icon {icon:?} must have a viewBox attribute"
            );

            // Production SVG parser verification (usvg/resvg pipeline used by iced)
            let tree = usvg::Tree::from_data(bytes, &usvg::Options::default())
                .unwrap_or_else(|err| panic!("Icon {icon:?} failed usvg parse: {err}"));
            assert!(
                tree.size().width() > 0.0 && tree.size().height() > 0.0,
                "Icon {icon:?} must have non-zero dimensions in usvg"
            );
        }
    }

    #[test]
    fn cached_handles_reuse_backing_storage() {
        for icon in Icon::all() {
            let handle1 = icon.handle();
            let handle2 = icon.handle();
            assert_eq!(
                format!("{handle1:?}"),
                format!("{handle2:?}"),
                "Icon {icon:?} cached handle must be stable across calls"
            );
        }
    }

    #[test]
    fn collection_type_mapping_returns_appropriate_icons() {
        assert_eq!(Icon::for_collection_type("movies"), Icon::Movie);
        assert_eq!(Icon::for_collection_type("tvshows"), Icon::Tv);
        assert_eq!(Icon::for_collection_type("music"), Icon::Music);
        assert_eq!(Icon::for_collection_type("photos"), Icon::Photo);
        assert_eq!(
            Icon::for_collection_type("unknown_collection"),
            Icon::Folder
        );
    }

    #[test]
    fn icon_helpers_construct_svg_widgets() {
        let _ = icon(Icon::Play);
        let _ = icon_sized(Icon::Home, IconSize::Xl);
        let _ = icon_with_color(
            Icon::Heart,
            IconSize::Sm,
            crate::tokens::DARK_PALETTE.colors.error,
        );
        let _ = icon_for_variant(Icon::Play, IconSize::Md, ButtonVariant::Primary);
        let _ = icon_for_variant_disabled(Icon::Play, IconSize::Md, ButtonVariant::Primary, true);
        let _ = icon_for_variant_status(
            Icon::Play,
            IconSize::Md,
            ButtonVariant::Primary,
            button::Status::Disabled,
        );
    }
}
