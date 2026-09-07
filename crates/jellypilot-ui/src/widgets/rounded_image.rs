//! Rounded image widget and radius helpers for JellyPilot design tokens.

use iced::border::Radius;
use iced::widget::image::{FilterMethod, Handle, Image};
use iced::{ContentFit, Element, Length, Rectangle, Rotation};

use crate::widgets::container::SURFACE_SMOOTHING;

/// An image widget with explicit corner radii and cover-fit default.
#[derive(Debug, Clone)]
pub struct RoundedImage<H = Handle> {
    handle: H,
    radius: Radius,
    border_smoothing: f32,
    snap: bool,
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
            border_smoothing: SURFACE_SMOOTHING,
            snap: false,
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

    /// Sets corner smoothing in `0.0..=1.0`, clamped by the renderer.
    ///
    /// Defaults to `0.6`. Use `0.0` for strict circles and capsules.
    #[must_use]
    pub fn border_smoothing(mut self, smoothing: f32) -> Self {
        self.border_smoothing = smoothing;
        self
    }

    /// Sets physical-pixel snapping for image content and display bounds.
    ///
    /// Defaults to `false`, preserving fractional positioning.
    #[must_use]
    pub fn snap(mut self, snap: bool) -> Self {
        self.snap = snap;
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
        let mut image_widget = Image::new(self.handle.clone())
            .border_radius(self.radius)
            .border_smoothing(self.border_smoothing)
            .snap(self.snap)
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
