//! Single-line text that truncates with an ellipsis ("…") to fit its bounds.
//!
//! [`iced::widget::text`] with [`Wrapping::None`] reports
//! a clamped node but still paints the full unwrapped line, so long titles
//! bleed past fixed-width cards. [`EllipsisText`] measures the content during
//! layout and shortens it to the longest prefix (plus an ellipsis) that fits
//! the available width, keeping single-line card copy inside its frame.

use std::borrow::Cow;

use iced::advanced::text::{self, paragraph};
use iced::advanced::widget::{tree, Operation, Tree};
use iced::advanced::{layout, renderer, Layout, Widget};
use iced::mouse;
use iced::widget::text::{Catalog, Format, State, Style, StyleFn, Wrapping};
use iced::{Color, Element, Length, Pixels, Rectangle, Size};

const ELLIPSIS: &str = "…";

/// Returns `content` unchanged when it fits `max_width`; otherwise the longest
/// char-boundary prefix that fits with an ellipsis appended.
///
/// `measure` reports the single-line width of a candidate string. Trailing
/// whitespace is trimmed before the ellipsis. When not even the ellipsis fits,
/// the ellipsis alone is returned — a degenerate bound, but still bounded.
#[must_use]
pub fn truncate_to_fit<'a>(
    content: &'a str,
    max_width: f32,
    mut measure: impl FnMut(&str) -> f32,
) -> Cow<'a, str> {
    if content.is_empty()
        || !max_width.is_finite()
        || max_width <= 0.0
        || measure(content) <= max_width
    {
        return Cow::Borrowed(content);
    }

    // Start byte of each char; `boundaries[k]` ends a prefix of exactly k chars.
    let boundaries: Vec<usize> = content.char_indices().map(|(index, _)| index).collect();

    // Widest fitting prefix is monotone in prefix length: binary search.
    let mut best: Option<usize> = None;
    let mut lo = 0;
    let mut hi = boundaries.len();
    while lo < hi {
        let kept = lo + (hi - lo) / 2;
        let prefix = content[..boundaries[kept]].trim_end();
        let candidate = format!("{prefix}{ELLIPSIS}");
        if measure(&candidate) <= max_width {
            best = Some(kept);
            lo = kept + 1;
        } else {
            hi = kept;
        }
    }

    Cow::Owned(match best {
        Some(kept) => {
            format!("{}{}", content[..boundaries[kept]].trim_end(), ELLIPSIS)
        }
        None => ELLIPSIS.to_string(),
    })
}

/// A single-line [`iced::widget::Text`] replacement that
/// truncates overflowing content with an ellipsis instead of painting past
/// its layout bounds.
pub struct EllipsisText<'a, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fragment: text::Fragment<'a>,
    format: Format<Renderer::Font>,
    class: Theme::Class<'a>,
}

impl<'a, Theme, Renderer> EllipsisText<'a, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    /// Creates a new [`EllipsisText`] with the given content.
    pub fn new(fragment: impl text::IntoFragment<'a>) -> Self {
        Self {
            fragment: fragment.into_fragment(),
            format: Format {
                wrapping: Wrapping::None,
                ..Format::default()
            },
            class: Theme::default(),
        }
    }

    /// Sets the size of the text.
    #[must_use]
    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        self.format.size = Some(size.into());
        self
    }

    /// Sets the [`Color`] of the text.
    #[must_use]
    pub fn color(self, color: impl Into<Color>) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.color_maybe(Some(color))
    }

    /// Sets the [`Color`] of the text, if `Some`.
    #[must_use]
    pub fn color_maybe(self, color: Option<impl Into<Color>>) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        let color = color.map(Into::into);
        self.style(move |_theme| Style { color })
    }

    /// Sets the style class of the text.
    #[must_use]
    pub fn style(mut self, style: impl Fn(&Theme) -> Style + 'a) -> Self
    where
        Theme::Class<'a>: From<StyleFn<'a, Theme>>,
    {
        self.class = (Box::new(style) as StyleFn<'a, Theme>).into();
        self
    }
}

/// Convenience constructor for [`EllipsisText`].
pub fn ellipsis_text<'a, Theme, Renderer>(
    fragment: impl text::IntoFragment<'a>,
) -> EllipsisText<'a, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    EllipsisText::new(fragment)
}

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for EllipsisText<'_, Theme, Renderer>
where
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State<Renderer::Paragraph>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(paragraph::Plain::<Renderer::Paragraph>::default())
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.format.width,
            height: self.format.height,
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let state = tree.state.downcast_mut::<State<Renderer::Paragraph>>();

        layout::sized(limits, self.format.width, self.format.height, |limits| {
            let bounds = limits.max();
            let size = self.format.size.unwrap_or_else(|| renderer.default_size());
            let font = self.format.font.unwrap_or_else(|| renderer.default_font());

            let truncated = truncate_to_fit(&self.fragment, bounds.width, |content| {
                let _ = state.update(text::Text {
                    content,
                    bounds,
                    size,
                    line_height: self.format.line_height,
                    font,
                    align_x: self.format.align_x,
                    align_y: self.format.align_y,
                    shaping: self.format.shaping,
                    wrapping: Wrapping::None,
                });
                state.min_bounds().width
            });

            // The last measured candidate is not necessarily the chosen one;
            // leave the paragraph holding exactly what draw() will paint.
            let _ = state.update(text::Text {
                content: &truncated,
                bounds,
                size,
                line_height: self.format.line_height,
                font,
                align_x: self.format.align_x,
                align_y: self.format.align_y,
                shaping: self.format.shaping,
                wrapping: Wrapping::None,
            });

            state.min_bounds()
        })
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        defaults: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State<Renderer::Paragraph>>();
        let style = theme.style(&self.class);

        iced::widget::text::draw(
            renderer,
            defaults,
            layout.bounds(),
            state.raw(),
            style,
            viewport,
        );
    }

    fn operate(
        &mut self,
        _tree: &mut Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.text(None, layout.bounds(), &self.fragment);
    }
}

impl<'a, Message, Theme, Renderer> From<EllipsisText<'a, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(text: EllipsisText<'a, Theme, Renderer>) -> Self {
        Element::new(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fake measure: every char is 10px wide.
    fn measure(content: &str) -> f32 {
        content.chars().count() as f32 * 10.0
    }

    #[test]
    fn content_that_fits_is_returned_unchanged() {
        let result = truncate_to_fit("Alive", 160.0, measure);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result, "Alive");
    }

    #[test]
    fn content_at_exact_bound_is_returned_unchanged() {
        let result = truncate_to_fit("1234567890", 100.0, measure);
        assert_eq!(result, "1234567890");
    }

    #[test]
    fn overflowing_content_is_truncated_to_fit() {
        let result = truncate_to_fit("12345678901234567890", 100.0, measure);
        assert!(result.ends_with(ELLIPSIS));
        assert!(measure(&result) <= 100.0);
        // 9 chars + ellipsis = 10 chars = 100px exactly.
        assert_eq!(result, "123456789…");
    }

    #[test]
    fn truncation_prefers_the_longest_fitting_prefix() {
        let result = truncate_to_fit("12345678901234567890", 105.0, measure);
        assert_eq!(result, "123456789…");
    }

    #[test]
    fn multibyte_content_truncates_on_char_boundaries() {
        // 20 CJK chars at 10px each; bound fits 10 chars total incl. ellipsis.
        let content = "机动战士高达剧场版机动战士高达剧场版II补";
        let result = truncate_to_fit(content, 100.0, measure);
        assert_eq!(result.chars().count(), 10);
        assert!(result.ends_with(ELLIPSIS));
        assert!(content.starts_with(result.trim_end_matches(ELLIPSIS)));
    }

    #[test]
    fn trailing_whitespace_is_trimmed_before_the_ellipsis() {
        // 6 chars fit (incl. ellipsis); prefix "1234  " trims to "1234".
        let result = truncate_to_fit("1234  567890", 60.0, measure);
        assert_eq!(result, "1234…");
    }

    #[test]
    fn degenerate_bound_still_returns_just_the_ellipsis() {
        let result = truncate_to_fit("does not matter", 5.0, measure);
        assert_eq!(result, ELLIPSIS);
    }

    #[test]
    fn empty_and_non_finite_inputs_are_passed_through() {
        assert_eq!(truncate_to_fit("", 100.0, measure), "");
        assert_eq!(truncate_to_fit("abc", f32::INFINITY, measure), "abc");
        assert_eq!(truncate_to_fit("abc", f32::NAN, measure), "abc");
        assert_eq!(truncate_to_fit("abc", 0.0, measure), "abc");
    }
}
