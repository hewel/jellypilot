//! Passive progressive blur behind the bottom of browsing artwork.

use iced::advanced::Renderer;
use iced::widget::{backdrop, container, space};
use iced::{border::Radius, Alignment, Blur, Element, Length, Theme};

use crate::tokens::ArtworkBlurProfile;

/// Places one bounded, scene-sampled blur band at the bottom of an artwork frame.
///
/// Callers must add this only for ready artwork, after the image and before its
/// scrim, labels, controls, and outline. The effect samples the previously drawn
/// scene, including neighboring pixels; it does not isolate the image source.
/// Keep the existing scrim for renderers without backdrop support. This layer
/// supplies no tint, input handling, animation, or redraw requests.
pub fn bottom<'a, Message: 'a, R: Renderer + 'a>(
    height: f32,
    profile: ArtworkBlurProfile,
    radius: Radius,
) -> Element<'a, Message, Theme, R> {
    let band_height = (height * profile.fraction).min(profile.max_height);
    let bottom_radius = Radius {
        bottom_left: radius.bottom_left,
        bottom_right: radius.bottom_right,
        ..Radius::default()
    };
    container(
        backdrop(
            Blur::vertical_gradient(0.0, profile.sigma),
            container(space::horizontal())
                .width(Length::Fill)
                .height(band_height),
        )
        .border_radius(bottom_radius)
        .border_smoothing(super::container::SURFACE_SMOOTHING),
    )
    .width(Length::Fill)
    .height(height)
    .align_y(Alignment::End)
    .into()
}

#[cfg(test)]
mod tests {
    use iced::advanced::{image, layout, mouse, renderer, shell, widget, Layout, Shell};
    use iced::{Background, Event, Point, Rectangle, Size, Transformation};

    use super::*;

    #[derive(Default)]
    struct Recorder {
        clips: Vec<Rectangle>,
        backdrops: Vec<(renderer::Backdrop, Rectangle)>,
    }

    impl Renderer for Recorder {
        fn blur_backdrop(&mut self, _radius: f32) {
            panic!("artwork requires a bounded backdrop");
        }

        fn draw_backdrop(&mut self, backdrop: renderer::Backdrop) {
            self.backdrops
                .push((backdrop, *self.clips.last().expect("backdrop clip")));
        }

        fn start_layer(&mut self, bounds: Rectangle) {
            self.clips.push(bounds);
        }

        fn end_layer(&mut self) {
            self.clips.pop();
        }

        fn start_transformation(&mut self, _transformation: Transformation) {}

        fn end_transformation(&mut self) {}

        fn fill_quad(&mut self, _quad: renderer::Quad, _background: impl Into<Background>) {}

        fn allocate_image(
            &self,
            _handle: &image::Handle,
            _callback: impl FnOnce(Result<image::Allocation, image::Error>) + Send + 'static,
        ) {
            panic!("the passive layer does not allocate artwork");
        }

        fn hint(&mut self, _scale: renderer::Scale) {}

        fn scale(&self) -> Option<renderer::Scale> {
            None
        }

        fn reset(&mut self, _new_bounds: Rectangle) {
            self.clips.clear();
            self.backdrops.clear();
        }

        fn settings(&self) -> renderer::Settings {
            renderer::Settings::default()
        }
    }

    #[test]
    fn scrolling_clips_the_bottom_band_without_restarting_its_profile() {
        let profile = ArtworkBlurProfile {
            sigma: 8.0,
            fraction: 0.5,
            max_height: 80.0,
        };
        let radius = Radius::default()
            .top(20.0)
            .bottom_left(12.0)
            .bottom_right(16.0);
        for (height, expected_band_height) in [(100.0, 50.0), (300.0, 80.0)] {
            let mut layer = bottom::<(), Recorder>(height, profile, radius);
            let mut tree = widget::Tree::new(layer.as_widget());
            let mut renderer = Recorder::default();
            let node = layer
                .as_widget_mut()
                .layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(Size::ZERO, Size::new(280.0, height + 120.0)),
                )
                .move_to(Point::new(10.0, 20.0));
            let frame = node.bounds();
            assert_eq!(
                frame.height, height,
                "the band belongs to the artwork, not spare parent height"
            );
            layer.as_widget().draw(
                &tree,
                &mut renderer,
                &Theme::Dark,
                &renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &frame,
            );
            let (complete, _) = renderer.backdrops.pop().expect("visible band");
            assert_eq!(complete.bounds.height, expected_band_height);
            assert_eq!(
                complete.bounds.y + complete.bounds.height,
                frame.y + frame.height
            );
            assert_eq!(complete.bounds.width, frame.width);
            assert_eq!(complete.border_radius.top_left, 0.0);
            assert_eq!(complete.border_radius.top_right, 0.0);
            assert_eq!(complete.border_radius.bottom_left, radius.bottom_left);
            assert_eq!(complete.border_radius.bottom_right, radius.bottom_right);

            let viewport = Rectangle {
                y: complete.bounds.center_y(),
                height: complete.bounds.height / 2.0,
                ..complete.bounds
            };
            layer.as_widget().draw(
                &tree,
                &mut renderer,
                &Theme::Dark,
                &renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &viewport,
            );
            let (clipped, clip) = renderer.backdrops.pop().expect("partly visible band");
            assert_eq!(clipped, complete);
            assert_eq!(clip, viewport);
            assert!(clipped.blur.radius_at(viewport.position(), clipped.bounds) > 0.0);

            let offscreen =
                Rectangle::new(Point::new(0.0, frame.y + frame.height + 1.0), frame.size());
            layer.as_widget().draw(
                &tree,
                &mut renderer,
                &Theme::Dark,
                &renderer::Style::default(),
                Layout::new(&node),
                mouse::Cursor::Unavailable,
                &offscreen,
            );
            assert!(renderer.backdrops.is_empty());
            assert!(renderer.clips.is_empty());
        }
    }

    #[test]
    fn passive_layer_preserves_layout_and_the_underlying_click_target() {
        use iced::widget::{button, stack, Space};

        let size = Size::new(280.0, 160.0);
        let viewport = Rectangle::with_size(size);
        let limits = layout::Limits::new(Size::ZERO, size);
        let renderer = Recorder::default();
        let cursor = mouse::Cursor::Available(Point::new(100.0, 140.0));
        let mut layer = bottom::<(), Recorder>(
            size.height,
            crate::tokens::TOKENS.artwork_blur.landscape,
            12.0.into(),
        );
        let mut tree = widget::Tree::new(layer.as_widget());
        let node = layer.as_widget_mut().layout(&mut tree, &renderer, &limits);
        assert_eq!(node.size(), size);
        assert_eq!(
            layer.as_widget().mouse_interaction(
                &tree,
                Layout::new(&node),
                cursor,
                &viewport,
                &renderer
            ),
            mouse::Interaction::None
        );
        let mut bus = shell::Bus::default();
        let mut shell = Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus);
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
        ] {
            layer.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                cursor,
                &renderer,
                &mut shell,
                &viewport,
            );
        }
        assert!(!shell.is_event_captured());
        assert!(shell.is_empty());
        assert_eq!(shell.redraw_request(), iced::window::RedrawRequest::Wait);

        let target = button(Space::new().width(Length::Fill).height(size.height))
            .padding(0)
            .on_press(());
        let mut content: Element<'_, (), Theme, Recorder> = stack![target, layer].into();
        let mut tree = widget::Tree::new(content.as_widget());
        content.as_widget_mut().diff(&mut tree);
        let node = content
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        assert_eq!(node.size(), size);
        for event in [
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ] {
            let mut shell = Shell::new(&iced::window::Headless, shell::Waker::noop(), &mut bus);
            content.as_widget_mut().update(
                &mut tree,
                &event,
                Layout::new(&node),
                cursor,
                &renderer,
                &mut shell,
                &viewport,
            );
        }
        assert_eq!(bus.len(), 1, "the artwork's click target remains reachable");
    }
}
