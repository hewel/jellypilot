//! JellyPilot status tags (recast of legacy badges).

use iced::font::Weight;
use iced::widget::{container, row, space, text};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Theme};

use crate::fonts::BODY_FONT;
use crate::tokens::{palette, ThemePalette, TOKENS};
use crate::variants::BadgeVariant;

fn variant_colors(variant: BadgeVariant, palette: &ThemePalette) -> (Color, Color) {
    let colors = palette.colors;
    match variant {
        BadgeVariant::Success => (colors.tertiaryContainer, colors.tertiary),
        BadgeVariant::Warning => (colors.warningContainer, colors.warning),
        BadgeVariant::Neutral => (colors.surfaceContainerHigh, palette.text.secondary),
        BadgeVariant::Error => (colors.errorContainer, colors.error),
    }
}

/// Resolves a legacy status badge variant to an iced container style.
///
/// Prefer [`status_tag`] for new call sites; this style exists only for
/// callers that have not yet migrated to the dot+label tag form.
pub fn style(theme: &Theme, variant: BadgeVariant) -> container::Style {
    let (background, text_color) = variant_colors(variant, palette(theme));

    container::Style {
        background: Some(Background::Color(background)),
        text_color: Some(text_color),
        border: Border {
            smoothing: 0.0,
            radius: TOKENS.radii.md.into(),
            color: Color::TRANSPARENT,
            width: 0.0,
        },
        ..container::Style::default()
    }
}

/// Builds a complete status tag: 6px dot + 12px/600 label in a 24px-high,
/// fully rounded container with 10px inline padding and an opaque semantic
/// container fill. No border.
pub fn status_tag<'a, Message: 'a>(
    label: impl ToString,
    variant: BadgeVariant,
) -> Element<'a, Message> {
    let dot = container(space()).width(6).height(6).style(move |theme| {
        let (_, content) = variant_colors(variant, palette(theme));
        container::Style {
            background: Some(Background::Color(content)),
            border: Border::default().rounded(TOKENS.radii.full),
            ..container::Style::default()
        }
    });
    let label = text(label.to_string())
        .size(TOKENS.font_sizes.s12)
        .font(Font {
            weight: Weight::Semibold,
            ..BODY_FONT
        });

    container(
        row![dot, label]
            .spacing(TOKENS.spacing.s1_5)
            .align_y(Alignment::Center),
    )
    .height(Length::Fixed(24.0))
    .padding([0.0, TOKENS.spacing.s2_5])
    .align_y(Alignment::Center)
    .style(move |theme| {
        let (background, text_color) = variant_colors(variant, palette(theme));
        container::Style {
            background: Some(Background::Color(background)),
            text_color: Some(text_color),
            border: Border {
                smoothing: 0.0,
                radius: TOKENS.radii.full.into(),
                color: Color::TRANSPARENT,
                width: 0.0,
            },
            ..container::Style::default()
        }
    })
    .into()
}

#[cfg(test)]
mod tests {
    use iced::border::Radius;

    use super::*;
    use crate::theme::ThemeMode;
    use crate::tokens::DARK_PALETTE;

    #[test]
    fn badge_variants_use_md_radius_and_no_border() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        for variant in [
            BadgeVariant::Success,
            BadgeVariant::Warning,
            BadgeVariant::Neutral,
            BadgeVariant::Error,
        ] {
            let style = style(&theme, variant);
            assert_eq!(
                style.border.radius,
                Radius::from(TOKENS.radii.md),
                "Badge variant {variant:?} must use md (6px) radius token"
            );
            assert_eq!(style.border.width, 0.0);
        }
    }

    #[test]
    fn badge_fills_are_opaque_container_tones() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        let expected = [
            (BadgeVariant::Success, DARK_PALETTE.colors.tertiaryContainer),
            (BadgeVariant::Warning, DARK_PALETTE.colors.warningContainer),
            (
                BadgeVariant::Neutral,
                DARK_PALETTE.colors.surfaceContainerHigh,
            ),
            (BadgeVariant::Error, DARK_PALETTE.colors.errorContainer),
        ];
        for (variant, fill) in expected {
            let style = style(&theme, variant);
            assert_eq!(
                style.background,
                Some(Background::Color(fill)),
                "Badge variant {variant:?} must use its opaque container fill"
            );
        }
    }

    #[test]
    fn error_badge_uses_error_container_fill_and_error_content() {
        let theme = crate::theme::theme(ThemeMode::Dark);
        let style = style(&theme, BadgeVariant::Error);
        assert_eq!(
            style.background,
            Some(Background::Color(DARK_PALETTE.colors.errorContainer))
        );
        assert_eq!(style.text_color, Some(DARK_PALETTE.colors.error));
    }

    #[test]
    fn status_tag_centers_its_dot_and_label_in_the_24px_frame() {
        use iced::advanced::layout::{self, Layout};
        use iced::advanced::renderer::{Headless, Settings as RendererSettings};
        use iced::advanced::widget::Tree;
        use iced::Size;

        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            RendererSettings {
                font: iced::Font::DEFAULT,
                text_size: 14.0.into(),
                line_height: crate::fonts::DEFAULT_LINE_HEIGHT,
                metrics_hinting: false,
            },
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");

        let mut tag = status_tag::<()>("Live", BadgeVariant::Error);
        let mut tree = Tree::new(tag.as_widget());
        tag.as_widget_mut().diff(&mut tree);
        let limits = layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0));
        let node = tag.as_widget_mut().layout(&mut tree, &renderer, &limits);
        let container_layout = Layout::new(&node);
        let content_row = container_layout.children().next().expect("content row");
        let dot_layout = content_row.children().next().expect("dot").bounds();

        assert_eq!(node.size().height, 24.0, "status tag must be 24px tall");
        assert!(
            (dot_layout.y + dot_layout.height / 2.0 - 12.0).abs() < 0.01,
            "6px dot must be vertically centered in the 24px frame"
        );
    }
}
