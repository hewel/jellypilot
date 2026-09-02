//! Rounded image widget and radius helpers for JellyPilot design tokens.

use iced::border::Radius;
use iced::widget::image::{FilterMethod, Handle, Image};
use iced::{ContentFit, Element, Length, Rectangle, Rotation};

/// An image widget with explicit corner radii and cover-fit default.
#[derive(Debug, Clone)]
pub struct RoundedImage<H = Handle> {
    handle: H,
    radius: Radius,
    content_fit: ContentFit,
    width: Length,
    height: Length,
    filter_method: FilterMethod,
    rotation: Rotation,
    opacity: f32,
    scale: f32,
    crop: Option<Rectangle<u32>>,
}

impl<H> RoundedImage<H> {
    /// Creates a new [`RoundedImage`] with the given handle and corner radius.
    pub fn new(handle: H, radius: impl Into<Radius>) -> Self {
        Self {
            handle,
            radius: radius.into(),
            content_fit: ContentFit::Cover,
            width: Length::Fill,
            height: Length::Fill,
            filter_method: FilterMethod::Linear,
            rotation: Rotation::default(),
            opacity: 1.0,
            scale: 1.0,
            crop: None,
        }
    }

    /// Sets the corner [`Radius`].
    #[must_use]
    pub fn radius(mut self, radius: impl Into<Radius>) -> Self {
        self.radius = radius.into();
        self
    }

    /// Sets the [`ContentFit`] mode.
    #[must_use]
    pub fn content_fit(mut self, content_fit: ContentFit) -> Self {
        self.content_fit = content_fit;
        self
    }

    /// Sets the widget width.
    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the widget height.
    #[must_use]
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the image [`FilterMethod`].
    #[must_use]
    pub fn filter_method(mut self, filter_method: FilterMethod) -> Self {
        self.filter_method = filter_method;
        self
    }

    /// Sets the opacity factor.
    #[must_use]
    pub fn opacity(mut self, opacity: impl Into<f32>) -> Self {
        self.opacity = opacity.into();
        self
    }

    /// Sets the image [`Rotation`].
    #[must_use]
    pub fn rotation(mut self, rotation: impl Into<Rotation>) -> Self {
        self.rotation = rotation.into();
        self
    }

    /// Sets the scale factor.
    #[must_use]
    pub fn scale(mut self, scale: impl Into<f32>) -> Self {
        self.scale = scale.into();
        self
    }

    /// Crops the image to the specified pixel rectangle.
    #[must_use]
    pub fn crop(mut self, crop: Rectangle<u32>) -> Self {
        self.crop = Some(crop);
        self
    }

    /// Returns the configured corner radius.
    #[must_use]
    pub fn get_radius(&self) -> Radius {
        self.radius
    }

    /// Returns the configured [`ContentFit`] mode.
    #[must_use]
    pub fn get_content_fit(&self) -> ContentFit {
        self.content_fit
    }

    /// Builds the underlying iced [`Image`] widget with the configured properties.
    pub fn to_widget(&self) -> Image<H>
    where
        H: Clone,
    {
        // Note: In iced 0.14 wgpu image shader (image.wgsl vs quad.wgsl), the position vector is not
        // negated prior to rounded_box_sdf, causing corner radii to be evaluated diagonally inverted
        // (top_left <-> bottom_right, top_right <-> bottom_left). We translate the logical radius
        // to the inverted representation expected by iced's image shader so that the rendered corners
        // match the requested top/bottom/left/right positions.
        let iced_radius = Radius {
            top_left: self.radius.bottom_right,
            top_right: self.radius.bottom_left,
            bottom_right: self.radius.top_left,
            bottom_left: self.radius.top_right,
        };
        let mut image_widget = Image::new(self.handle.clone())
            .border_radius(iced_radius)
            .content_fit(self.content_fit)
            .width(self.width)
            .height(self.height)
            .filter_method(self.filter_method)
            .opacity(self.opacity)
            .rotation(self.rotation)
            .scale(self.scale);

        if let Some(crop) = self.crop {
            image_widget = image_widget.crop(crop);
        }

        image_widget
    }
}

impl<'a, Message, Theme, Renderer, H> From<RoundedImage<H>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: iced::advanced::image::Renderer<Handle = H>,
    H: Clone + 'a,
{
    fn from(image: RoundedImage<H>) -> Self {
        image.to_widget().into()
    }
}

/// Convenience constructor for a [`RoundedImage`].
pub fn rounded_image<H>(handle: H, radius: impl Into<Radius>) -> RoundedImage<H> {
    RoundedImage::new(handle, radius)
}

/// Radii configuration for card-top full-bleed images (top corners rounded to match card frame, bottom square).
pub fn card_top_radius(radius: f32) -> Radius {
    Radius {
        top_left: radius,
        top_right: radius,
        bottom_right: 0.0,
        bottom_left: 0.0,
    }
}

/// Radii configuration for standalone images where all 4 corners are rounded.
pub fn full_radius(radius: f32) -> Radius {
    Radius::from(radius)
}

#[cfg(test)]
mod tests {
    use iced::border::Radius;
    use iced::widget::image::Handle;
    use iced::{ContentFit, Length, Rectangle};

    use super::*;

    #[test]
    fn card_top_radius_rounds_only_top_corners() {
        let radius = card_top_radius(16.0);
        assert_eq!(
            radius,
            Radius {
                top_left: 16.0,
                top_right: 16.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            }
        );
    }

    #[test]
    fn full_radius_rounds_all_four_corners() {
        let radius = full_radius(16.0);
        assert_eq!(
            radius,
            Radius {
                top_left: 16.0,
                top_right: 16.0,
                bottom_right: 16.0,
                bottom_left: 16.0,
            }
        );
    }

    #[test]
    fn zero_radius_fallback_produces_zero_radii() {
        let radius = full_radius(0.0);
        assert_eq!(
            radius,
            Radius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            }
        );

        let top_zero = card_top_radius(0.0);
        assert_eq!(
            top_zero,
            Radius {
                top_left: 0.0,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: 0.0,
            }
        );
    }

    #[test]
    fn rounded_image_builder_preserves_content_fit_and_dimensions() {
        let handle = Handle::from_bytes(vec![0u8; 4]);
        let widget = rounded_image(handle, card_top_radius(16.0))
            .content_fit(ContentFit::Contain)
            .width(Length::Fixed(200.0))
            .height(Length::Fixed(300.0))
            .crop(Rectangle {
                x: 0,
                y: 0,
                width: 100,
                height: 100,
            });

        assert_eq!(widget.get_content_fit(), ContentFit::Contain);
        assert_eq!(widget.get_radius(), card_top_radius(16.0));
        assert_eq!(widget.width, Length::Fixed(200.0));
        assert_eq!(widget.height, Length::Fixed(300.0));
        assert!(widget.crop.is_some());
    }

    #[test]
    fn content_fit_modes_are_supported() {
        let handle = Handle::from_bytes(vec![0u8; 4]);
        for fit in [
            ContentFit::Cover,
            ContentFit::Contain,
            ContentFit::ScaleDown,
            ContentFit::None,
        ] {
            let img = rounded_image(handle.clone(), 8.0).content_fit(fit);
            assert_eq!(img.get_content_fit(), fit);
        }
    }
}
