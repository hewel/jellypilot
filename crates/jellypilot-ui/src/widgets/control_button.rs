//! Status-aware icon and label button.

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard::{key, Key};
use iced::touch;
use iced::widget::{button, space, text, Row};
use iced::{Background, Element, Event, Length, Padding, Rectangle, Size, Theme};

use crate::icons::{icon_for_control_state, Icon, IconControlState, IconSize};
use crate::tokens::TOKENS;
use crate::variants::ButtonVariant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct State {
    is_pressed: bool,
    status: Option<button::Status>,
    is_focused: bool,
}

impl widget::operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.is_focused
    }

    fn focus(&mut self) {
        self.is_focused = true;
    }

    fn unfocus(&mut self) {
        self.is_focused = false;
    }
}

type ContentFactory<'a, Message, Renderer> =
    dyn Fn(IconControlState) -> Element<'a, Message, Theme, Renderer> + 'a;

/// A button whose icon and label share the interaction status of its full bounds.
pub struct ControlButton<'a, Message, Renderer = iced::Renderer> {
    contents: [Element<'a, Message, Theme, Renderer>; 3],
    custom_content: Option<Box<ContentFactory<'a, Message, Renderer>>>,
    icon: Option<Icon>,
    label: Option<String>,
    variant: ButtonVariant,
    icon_size: IconSize,
    trailing_icon: bool,
    label_size: f32,
    spacing: f32,
    label_fill: bool,
    content_centered: bool,
    padding: Padding,
    width: Length,
    min_height: f32,
    id: Option<widget::Id>,
    on_press: Option<Message>,
}

impl<'a, Message, Renderer> ControlButton<'a, Message, Renderer>
where
    Message: Clone + 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    /// Creates a status-aware control button.
    pub fn new(icon: Option<Icon>, label: Option<String>, variant: ButtonVariant) -> Self {
        let icon_size = IconSize::Md;
        let trailing_icon = false;
        let label_size = 16.0;
        let spacing = TOKENS.spacing.s1_5;
        let label_fill = false;
        let content_centered = false;
        let contents = build_contents(
            icon,
            label.as_deref(),
            variant,
            icon_size,
            trailing_icon,
            label_size,
            spacing,
            label_fill,
            content_centered,
        );

        Self {
            contents,
            custom_content: None,
            icon,
            label,
            variant,
            icon_size,
            trailing_icon,
            label_size,
            spacing,
            label_fill,
            content_centered,
            padding: button::DEFAULT_PADDING,
            width: Length::Shrink,
            min_height: 0.0,
            id: None,
            on_press: None,
        }
    }

    fn with_content(
        build: impl Fn(IconControlState) -> Element<'a, Message, Theme, Renderer> + 'a,
        variant: ButtonVariant,
    ) -> Self {
        let mut control = Self::new(None, None, variant);
        control.custom_content = Some(Box::new(build));
        control.rebuild_contents();
        control
    }

    /// Sets the icon size.
    #[must_use]
    pub fn icon_size(mut self, icon_size: IconSize) -> Self {
        self.icon_size = icon_size;
        self.rebuild_contents();
        self
    }

    /// Places the icon after the label when set.
    #[must_use]
    pub fn trailing_icon(mut self, trailing_icon: bool) -> Self {
        self.trailing_icon = trailing_icon;
        self.rebuild_contents();
        self
    }

    /// Sets the label size in logical pixels.
    #[must_use]
    pub fn label_size(mut self, label_size: f32) -> Self {
        self.label_size = label_size;
        self.rebuild_contents();
        self
    }

    /// Sets the horizontal gap between the icon and label.
    #[must_use]
    pub fn spacing(mut self, spacing: f32) -> Self {
        self.spacing = spacing;
        self.rebuild_contents();
        self
    }

    /// Lets the label fill the available row width.
    #[must_use]
    pub fn label_fill(mut self, label_fill: bool) -> Self {
        self.label_fill = label_fill;
        self.rebuild_contents();
        self
    }

    /// Centers the content row horizontally within the control.
    #[must_use]
    pub fn content_centered(mut self, content_centered: bool) -> Self {
        self.content_centered = content_centered;
        self.rebuild_contents();
        self
    }

    /// Sets the padding around the content.
    #[must_use]
    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Sets the control width.
    #[must_use]
    pub fn width(mut self, width: Length) -> Self {
        self.width = width;
        self
    }

    /// Ensures the interactive bounds meet a surface-specific minimum height.
    #[must_use]
    pub fn min_height(mut self, min_height: f32) -> Self {
        self.min_height = min_height.max(0.0);
        self
    }

    /// Sets a stable widget identifier for programmatic focus restoration.
    #[must_use]
    pub fn id(mut self, id: impl Into<widget::Id>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets the message produced by a completed press.
    #[must_use]
    pub fn on_press(mut self, on_press: Message) -> Self {
        self.on_press = Some(on_press);
        self
    }

    /// Sets an optional press message. `None` disables the control.
    #[must_use]
    pub fn on_press_maybe(mut self, on_press: Option<Message>) -> Self {
        self.on_press = on_press;
        self
    }

    fn rebuild_contents(&mut self) {
        if let Some(build) = &self.custom_content {
            self.contents = [
                IconControlState::Rest,
                IconControlState::Hovered,
                IconControlState::Disabled,
            ]
            .map(build);
            return;
        }
        self.contents = build_contents(
            self.icon,
            self.label.as_deref(),
            self.variant,
            self.icon_size,
            self.trailing_icon,
            self.label_size,
            self.spacing,
            self.label_fill,
            self.content_centered,
        );
    }
}

/// Creates a status-aware control button.
pub fn control_button<'a, Message: Clone + 'a>(
    icon: Option<Icon>,
    label: Option<String>,
    variant: ButtonVariant,
) -> ControlButton<'a, Message> {
    ControlButton::new(icon, label, variant)
}

/// Creates a keyboard-focusable button with caller-composed, non-interactive
/// content for each visual state. The factory owns the internal layout and
/// colors; icon/label layout builders do not alter custom content.
pub fn control_button_content<'a, Message: Clone + 'a>(
    build: impl Fn(IconControlState) -> Element<'a, Message> + 'a,
    variant: ButtonVariant,
) -> ControlButton<'a, Message> {
    ControlButton::with_content(build, variant)
}

#[expect(
    clippy::too_many_arguments,
    reason = "content rows mirror the public visual builders"
)]
fn build_contents<'a, Message, Renderer>(
    icon: Option<Icon>,
    label: Option<&str>,
    variant: ButtonVariant,
    icon_size: IconSize,
    trailing_icon: bool,
    label_size: f32,
    spacing: f32,
    label_fill: bool,
    content_centered: bool,
) -> [Element<'a, Message, Theme, Renderer>; 3]
where
    Message: 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    [
        build_content(
            icon,
            label,
            variant,
            icon_size,
            trailing_icon,
            label_size,
            spacing,
            label_fill,
            content_centered,
            IconControlState::Rest,
        ),
        build_content(
            icon,
            label,
            variant,
            icon_size,
            trailing_icon,
            label_size,
            spacing,
            label_fill,
            content_centered,
            IconControlState::Hovered,
        ),
        build_content(
            icon,
            label,
            variant,
            icon_size,
            trailing_icon,
            label_size,
            spacing,
            label_fill,
            content_centered,
            IconControlState::Disabled,
        ),
    ]
}

#[expect(
    clippy::too_many_arguments,
    reason = "content rows mirror the public visual builders"
)]
fn build_content<'a, Message, Renderer>(
    icon: Option<Icon>,
    label: Option<&str>,
    variant: ButtonVariant,
    icon_size: IconSize,
    trailing_icon: bool,
    label_size: f32,
    spacing: f32,
    label_fill: bool,
    content_centered: bool,
    state: IconControlState,
) -> Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    let mut content = Row::new().spacing(spacing).align_y(iced::Alignment::Center);

    if content_centered {
        content = content.push(space::horizontal());
    }

    let icon = icon.map(|icon| icon_for_control_state(icon, icon_size, variant, state));
    let label = label.map(|label| {
        let status = match state {
            IconControlState::Rest => button::Status::Active,
            IconControlState::Hovered => button::Status::Hovered,
            IconControlState::Disabled => button::Status::Disabled,
        };
        let mut label = text(label.to_owned())
            .size(label_size)
            .style(move |theme: &Theme| text::Style {
                color: Some(crate::widgets::button::style(theme, variant, status).text_color),
            });
        if label_fill {
            label = label.width(Length::Fill);
        }
        label
    });

    if trailing_icon {
        if let Some(label) = label {
            content = content.push(label);
        }
        if let Some(icon) = icon {
            content = content.push(icon);
        }
    } else {
        if let Some(icon) = icon {
            content = content.push(icon);
        }
        if let Some(label) = label {
            content = content.push(label);
        }
    }

    if content_centered {
        content = content.push(space::horizontal()).width(Length::Fill);
    }

    content.into()
}

fn status<Message>(
    control: &ControlButton<'_, Message, impl iced::advanced::Renderer>,
    state: &State,
    bounds: Rectangle,
    cursor: mouse::Cursor,
) -> button::Status {
    if control.on_press.is_none() {
        button::Status::Disabled
    } else if state.is_pressed {
        button::Status::Pressed
    } else if cursor.is_over(bounds) {
        button::Status::Hovered
    } else {
        button::Status::Active
    }
}

fn content_index(status: button::Status) -> usize {
    match status {
        button::Status::Active | button::Status::Pressed => 0,
        button::Status::Hovered => 1,
        button::Status::Disabled => 2,
    }
}

impl<'a, Message, Renderer> Widget<Message, Theme, Renderer>
    for ControlButton<'a, Message, Renderer>
where
    Message: Clone + 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn children(&self) -> Vec<Tree> {
        self.contents.iter().map(Tree::new).collect()
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&self.contents);
    }

    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: if self.min_height > 0.0 {
                Length::Fixed(self.min_height)
            } else {
                Length::Shrink
            },
        }
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let height = if self.min_height > 0.0 {
            Length::Fixed(self.min_height)
        } else {
            Length::Shrink
        };
        layout::padded(limits, self.width, height, self.padding, |limits| {
            let node =
                self.contents[0]
                    .as_widget_mut()
                    .layout(&mut tree.children[0], renderer, limits);
            for index in 1..self.contents.len() {
                let _ = self.contents[index].as_widget_mut().layout(
                    &mut tree.children[index],
                    renderer,
                    limits,
                );
            }
            node
        })
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let index = content_index(state.status.unwrap_or(button::Status::Active));
        operation.focusable(self.id.as_ref(), layout.bounds(), state);
        operation.container(self.id.as_ref(), layout.bounds());
        if let Some(content_layout) = layout.children().next() {
            operation.traverse(&mut |operation| {
                self.contents[index].as_widget_mut().operate(
                    &mut tree.children[index],
                    content_layout,
                    renderer,
                    operation,
                );
            });
        }
    }

    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();

        if state.is_focused
            && self.on_press.is_some()
            && matches!(
                event,
                Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: Key::Named(key::Named::Enter | key::Named::Space),
                    ..
                })
            )
        {
            if let Some(on_press) = &self.on_press {
                shell.publish(on_press.clone());
                shell.capture_event();
            }
        }

        if self.on_press.is_none() {
            state.is_pressed = false;
        } else {
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Touch(touch::Event::FingerPressed { .. })
                    if cursor.is_over(bounds) =>
                {
                    state.is_pressed = true;
                    shell.capture_event();
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                | Event::Touch(touch::Event::FingerLifted { .. })
                    if state.is_pressed =>
                {
                    state.is_pressed = false;
                    if cursor.is_over(bounds) {
                        if let Some(on_press) = &self.on_press {
                            shell.publish(on_press.clone());
                        }
                    }
                    shell.capture_event();
                }
                Event::Touch(touch::Event::FingerLost { .. }) => {
                    state.is_pressed = false;
                }
                _ => {}
            }
        }

        let current_status = status(self, state, bounds, cursor);
        if matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            state.status = Some(current_status);
        } else if state.status != Some(current_status) {
            state.status = Some(current_status);
            shell.request_redraw();
        }
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        renderer_style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let status = status(self, tree.state.downcast_ref::<State>(), bounds, cursor);
        let mut style = crate::widgets::button::style(theme, self.variant, status);
        if tree.state.downcast_ref::<State>().is_focused {
            style.border.color = crate::tokens::palette(theme).colors.primary;
            style.border.width = 2.0;
        }

        if style.background.is_some() || style.border.width > 0.0 || style.shadow.color.a > 0.0 {
            renderer.fill_quad(
                renderer::Quad {
                    bounds,
                    border: style.border,
                    shadow: style.shadow,
                    snap: style.snap,
                },
                style
                    .background
                    .unwrap_or(Background::Color(iced::Color::TRANSPARENT)),
            );
        }

        if let Some(content_layout) = layout.children().next() {
            let index = content_index(status);
            self.contents[index].as_widget().draw(
                &tree.children[index],
                renderer,
                theme,
                renderer_style,
                content_layout,
                cursor,
                viewport,
            );
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        if self.on_press.is_some() && cursor.is_over(layout.bounds()) {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

impl<'a, Message, Renderer> From<ControlButton<'a, Message, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Renderer: 'a
        + iced::advanced::Renderer
        + iced::advanced::text::Renderer
        + iced::advanced::svg::Renderer,
{
    fn from(control: ControlButton<'a, Message, Renderer>) -> Self {
        Element::new(control)
    }
}

#[cfg(test)]
mod tests {
    use iced::advanced::layout::{self, Layout};
    use iced::advanced::mouse;
    use iced::advanced::widget::operation::focusable;
    use iced::advanced::widget::{Id, Operation, Tree, Widget};
    use iced::advanced::Shell;
    use iced::keyboard::{key, Event as KeyboardEvent, Key, Location, Modifiers};
    use iced::{Event, Length, Point, Rectangle, Size, Theme};

    use super::{control_button, ControlButton, State};
    use crate::icons::{Icon, IconSize};
    use crate::tokens::TOKENS;
    use crate::variants::ButtonVariant;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestMessage {
        Clicked,
    }

    fn test_button() -> ControlButton<'static, TestMessage, ()> {
        ControlButton::new(
            Some(Icon::Home),
            Some("Home".to_owned()),
            ButtonVariant::Tonal,
        )
    }

    #[test]
    fn construction_builds_all_three_content_states() {
        let public_button: ControlButton<'_, TestMessage> = control_button(
            Some(Icon::Home),
            Some("Home".to_owned()),
            ButtonVariant::Tonal,
        );
        let generic_button = test_button();
        assert_eq!(public_button.label_size, 16.0);
        assert_eq!(public_button.spacing, TOKENS.spacing.s1_5);

        let custom_spacing = test_button().spacing(TOKENS.spacing.s2);
        assert_eq!(custom_spacing.spacing, TOKENS.spacing.s2);
        assert_eq!(Widget::children(&custom_spacing).len(), 3);

        assert_eq!(Widget::children(&public_button).len(), 3);
        assert_eq!(Widget::children(&generic_button).len(), 3);
    }

    #[test]
    fn width_and_padding_determine_layout_geometry() {
        let mut button: ControlButton<'_, TestMessage, ()> =
            ControlButton::new(Some(Icon::Home), None, ButtonVariant::Tonal)
                .icon_size(IconSize::Md)
                .width(Length::Fixed(120.0))
                .padding([6.0, 10.0]);
        let mut tree = Tree::new(&button as &dyn Widget<TestMessage, Theme, ()>);
        let limits = layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0));

        let node = button.layout(&mut tree, &(), &limits);

        assert_eq!(node.size(), Size::new(120.0, IconSize::MD + 12.0));
        assert_eq!(node.children().len(), 1);
        assert_eq!(
            node.children()[0].bounds().position(),
            Point::new(10.0, 6.0)
        );
    }

    #[test]
    fn spacing_builder_changes_icon_label_geometry() {
        let spacing = TOKENS.spacing.s2;
        let mut button: ControlButton<'_, TestMessage, ()> = ControlButton::new(
            Some(Icon::Home),
            Some("Home".to_owned()),
            ButtonVariant::Tonal,
        )
        .spacing(spacing)
        .padding(0);
        let mut tree = Tree::new(&button as &dyn Widget<TestMessage, Theme, ()>);
        let limits = layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0));

        let node = button.layout(&mut tree, &(), &limits);

        assert_eq!(node.size(), Size::new(IconSize::MD + spacing, IconSize::MD));
    }

    #[test]
    fn press_state_publishes_inside_and_cancels_outside() {
        let mut button: ControlButton<'_, TestMessage, ()> =
            ControlButton::new(Some(Icon::Home), None, ButtonVariant::Tonal)
                .width(Length::Fixed(80.0))
                .padding(0)
                .on_press(TestMessage::Clicked);
        let mut tree = Tree::new(&button as &dyn Widget<TestMessage, Theme, ()>);
        let limits = layout::Limits::new(Size::ZERO, Size::new(100.0, 100.0));
        let node = button.layout(&mut tree, &(), &limits);
        let layout = Layout::new(&node);
        let viewport = Rectangle::with_size(Size::new(100.0, 100.0));
        let inside = mouse::Cursor::Available(Point::new(5.0, 5.0));
        let outside = mouse::Cursor::Available(Point::new(90.0, 90.0));
        let mut clipboard = iced::advanced::clipboard::Null;

        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        button.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            inside,
            &(),
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        assert!(tree.state.downcast_ref::<State>().is_pressed);

        let mut cancelled = Vec::new();
        let mut shell = Shell::new(&mut cancelled);
        button.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            outside,
            &(),
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        assert!(cancelled.is_empty());
        assert!(!tree.state.downcast_ref::<State>().is_pressed);

        let mut published = Vec::new();
        let mut shell = Shell::new(&mut published);
        button.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            layout,
            inside,
            &(),
            &mut clipboard,
            &mut shell,
            &viewport,
        );
        button.update(
            &mut tree,
            &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            layout,
            inside,
            &(),
            &mut clipboard,
            &mut shell,
            &viewport,
        );

        assert_eq!(published, vec![TestMessage::Clicked]);
    }

    #[test]
    fn stable_id_receives_focus_and_activates_with_enter() {
        for mut button in [
            ControlButton::new(Some(Icon::Home), None, ButtonVariant::Tonal),
            ControlButton::with_content(
                |_| iced::widget::Space::new().width(36.0).height(36.0).into(),
                ButtonVariant::Tonal,
            )
            .spacing(8.0),
        ]
        .map(|button: ControlButton<'_, TestMessage, ()>| {
            button
                .width(Length::Fixed(80.0))
                .padding(0)
                .id("control-button-test")
                .on_press(TestMessage::Clicked)
        }) {
            let mut tree = Tree::new(&button as &dyn Widget<TestMessage, Theme, ()>);
            let limits = layout::Limits::new(Size::ZERO, Size::new(100.0, 100.0));
            let node = button.layout(&mut tree, &(), &limits);
            let layout = Layout::new(&node);
            let mut focus: Box<dyn Operation> =
                Box::new(focusable::focus::<()>(Id::new("control-button-test")));
            button.operate(&mut tree, layout, &(), focus.as_mut());
            assert!(tree.state.downcast_ref::<State>().is_focused);

            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            let mut clipboard = iced::advanced::clipboard::Null;
            button.update(
                &mut tree,
                &Event::Keyboard(KeyboardEvent::KeyPressed {
                    key: Key::Named(key::Named::Enter),
                    modified_key: Key::Named(key::Named::Enter),
                    physical_key: key::Physical::Code(key::Code::Enter),
                    location: Location::Standard,
                    modifiers: Modifiers::default(),
                    text: None,
                    repeat: false,
                }),
                layout,
                mouse::Cursor::Unavailable,
                &(),
                &mut clipboard,
                &mut shell,
                &Rectangle::with_size(Size::new(100.0, 100.0)),
            );
            assert_eq!(messages, vec![TestMessage::Clicked]);
        }
    }
}
