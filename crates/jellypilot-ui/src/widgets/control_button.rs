//! Status-aware icon and label button.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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

#[derive(Debug, Default)]
struct State {
    is_pressed: bool,
    status: Option<button::Status>,
    is_focused: bool,
    pointer_interaction: bool,
    focus_visibility: Option<FocusVisibility>,
    focus_generation: u64,
}

impl State {
    fn has_focus(&self) -> bool {
        self.is_focused
            && self
                .focus_visibility
                .as_ref()
                .is_none_or(|visibility| self.focus_generation == visibility.pointer_generation())
    }

    fn is_focus_visible(&self) -> bool {
        self.has_focus()
            && self
                .focus_visibility
                .as_ref()
                .map_or(!self.pointer_interaction, FocusVisibility::is_keyboard)
    }
}

/// Window-scoped input state for [`super::focus_scope::focus_scope`].
/// Captured pointer presses invalidate old control focus, not text inputs.
#[derive(Debug, Clone, Default)]
pub struct FocusVisibility(Arc<FocusContext>);

#[derive(Debug, Default)]
struct FocusContext {
    keyboard: AtomicBool,
    pointer_generation: AtomicU64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FocusSnapshot {
    keyboard: bool,
    pointer_generation: u64,
}

impl FocusVisibility {
    /// Records the current input method, including events captured by overlays.
    pub(super) fn set_keyboard(&self, keyboard: bool) {
        self.0.keyboard.store(keyboard, Ordering::Relaxed);
        if !keyboard {
            // Opaque overlays can prevent the old control from receiving the press.
            self.0.pointer_generation.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Whether keyboard navigation should reveal control focus.
    #[must_use]
    pub(super) fn is_keyboard(&self) -> bool {
        self.0.keyboard.load(Ordering::Relaxed)
    }

    pub(super) fn snapshot(&self) -> FocusSnapshot {
        FocusSnapshot {
            keyboard: self.is_keyboard(),
            pointer_generation: self.pointer_generation(),
        }
    }

    pub(super) fn restore(&self, snapshot: FocusSnapshot) {
        self.0.keyboard.store(snapshot.keyboard, Ordering::Relaxed);
        self.0
            .pointer_generation
            .store(snapshot.pointer_generation, Ordering::Relaxed);
    }

    fn pointer_generation(&self) -> u64 {
        self.0.pointer_generation.load(Ordering::Relaxed)
    }
}

impl<T> Operation<T> for FocusVisibility {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation<T>)) {
        visit(self);
    }

    fn custom(&mut self, _id: Option<&widget::Id>, _bounds: Rectangle, state: &mut dyn Any) {
        let Some(state) = state.downcast_mut::<State>() else {
            return;
        };
        if state
            .focus_visibility
            .as_ref()
            .is_none_or(|visibility| !Arc::ptr_eq(&visibility.0, &self.0))
        {
            state.focus_visibility = Some(self.clone());
        }
    }
}

pub(crate) fn visible_focus(state: &dyn Any) -> Option<bool> {
    state
        .downcast_ref::<State>()
        .filter(|state| state.has_focus())
        .map(State::is_focus_visible)
}

impl widget::operation::Focusable for State {
    fn is_focused(&self) -> bool {
        self.has_focus()
    }

    fn focus(&mut self) {
        self.is_focused = self
            .focus_visibility
            .as_ref()
            .is_none_or(FocusVisibility::is_keyboard);
        self.focus_generation = self
            .focus_visibility
            .as_ref()
            .map_or(0, FocusVisibility::pointer_generation);
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
    style: fn(&Theme, ButtonVariant, button::Status) -> button::Style,
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
            style: crate::widgets::button::style,
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

    /// Selects a semantic Catalog style without changing content colors or focus feedback.
    #[must_use]
    pub fn style(
        mut self,
        style: fn(&Theme, ButtonVariant, button::Status) -> button::Style,
    ) -> Self {
        self.style = style;
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
        layout::positioned(
            limits,
            self.width,
            height,
            self.padding,
            |limits| {
                // Measure intrinsic content before centering it in the taller hit area.
                let limits = limits.loose();
                let node = self.contents[0].as_widget_mut().layout(
                    &mut tree.children[0],
                    renderer,
                    &limits,
                );
                for index in 1..self.contents.len() {
                    let _ = self.contents[index].as_widget_mut().layout(
                        &mut tree.children[index],
                        renderer,
                        &limits,
                    );
                }
                node
            },
            |content, size| content.align(iced::Alignment::Start, iced::Alignment::Center, size),
        )
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
        operation.custom(self.id.as_ref(), layout.bounds(), state);
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

        if matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(_))
                | Event::Touch(touch::Event::FingerPressed { .. })
        ) {
            if state.has_focus() {
                shell.request_redraw();
            }
            state.is_focused = false;
            state.pointer_interaction = true;
        } else if matches!(
            event,
            Event::Keyboard(iced::keyboard::Event::KeyPressed { .. })
        ) {
            state.pointer_interaction = false;
        }

        if state.has_focus()
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
        let mut style = (self.style)(theme, self.variant, status);
        if tree.state.downcast_ref::<State>().is_focus_visible() {
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

    use super::{ControlButton, State};
    use crate::icons::{Icon, IconSize};
    use crate::tokens::TOKENS;
    use crate::variants::ButtonVariant;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum TestMessage {
        Clicked,
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
    fn tall_icon_control_centers_content_in_its_hit_area() {
        let mut button: ControlButton<'_, TestMessage, ()> =
            ControlButton::new(Some(Icon::Settings), None, ButtonVariant::Text)
                .min_height(40.0)
                .width(Length::Fixed(100.0))
                .content_centered(true);
        let mut tree = Tree::new(&button as &dyn Widget<TestMessage, Theme, ()>);
        let limits = layout::Limits::new(Size::ZERO, Size::new(200.0, 200.0));

        let node = button.layout(&mut tree, &(), &limits);
        let content = Layout::new(&node)
            .children()
            .next()
            .expect("content row")
            .children()
            .nth(1)
            .expect("icon between horizontal spacers")
            .bounds();

        assert_eq!(content.y + content.height / 2.0, node.size().height / 2.0);
    }

    #[test]
    fn selected_library_row_centers_composed_icon_and_text() {
        use iced::advanced::renderer::Headless;
        use iced::widget::{container, row};
        use iced::{Alignment, Font};

        let renderer = iced::futures::executor::block_on(iced::Renderer::new(
            Font::DEFAULT,
            14.0.into(),
            Some("tiny-skia"),
        ))
        .expect("software layout renderer");
        let mut button = super::control_button_content(
            |state| {
                row![
                    crate::icons::icon_for_control_state(
                        Icon::for_collection_type("movies"),
                        IconSize::Md,
                        ButtonVariant::Secondary,
                        state,
                    ),
                    container(crate::widgets::ellipsis_text::ellipsis_text("电影").size(14))
                        .width(Length::Fill),
                ]
                .spacing(TOKENS.spacing.s2_5)
                .align_y(Alignment::Center)
                .width(Length::Fill)
                .into()
            },
            ButtonVariant::Secondary,
        )
        .min_height(32.0)
        .padding([4, 12])
        .width(Length::Fill)
        .on_press(TestMessage::Clicked);
        let mut tree = Tree::new(&button as &dyn Widget<TestMessage, Theme, iced::Renderer>);
        let limits = layout::Limits::new(Size::ZERO, Size::new(216.0, 100.0));

        let node = button.layout(&mut tree, &renderer, &limits);
        let row = Layout::new(&node).children().next().expect("content row");
        let mut children = row.children();
        let icon = children.next().expect("library icon").bounds();
        let label = children
            .next()
            .expect("label container")
            .children()
            .next()
            .expect("library label")
            .bounds();
        for bounds in [icon, label] {
            let center = bounds.y + bounds.height / 2.0;
            assert!(
                (center - node.size().height / 2.0).abs() < 0.01,
                "selected row content center {center} must match its {}px hit area",
                node.size().height,
            );
        }
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
            for event in [
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Keyboard(KeyboardEvent::KeyPressed {
                    key: Key::Named(key::Named::Enter),
                    modified_key: Key::Named(key::Named::Enter),
                    physical_key: key::Physical::Code(key::Code::Enter),
                    location: Location::Standard,
                    modifiers: Modifiers::default(),
                    text: None,
                    repeat: false,
                }),
            ] {
                button.update(
                    &mut tree,
                    &event,
                    layout,
                    mouse::Cursor::Available(Point::new(99.0, 99.0)),
                    &(),
                    &mut clipboard,
                    &mut shell,
                    &Rectangle::with_size(Size::new(100.0, 100.0)),
                );
            }
            assert_eq!(messages, vec![TestMessage::Clicked]);
        }
    }
}
