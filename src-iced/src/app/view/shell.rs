use iced::widget::Widget as _;
use std::borrow::Cow;

use super::{account, browse, detail, home, motion, personal_lists, player, settings};
use crate::app::message::{BrowseMessage, HomeMessage, Message, SettingsMessage, ShellMessage};
use crate::app::personal_lists::Route;
use crate::app::shell::{SEARCH_INPUT_ID, SEARCH_TRIGGER_ID, SETTINGS_TRIGGER_ID};
use crate::app::state::{Destination, State};
use crate::i18n::Localizer;
use iced::widget::{
  button, column, container, modal, row, scrollable, space, stack, text, text_input, Column, Id,
};
use iced::{Alignment, Background, Element, Fill, Length};
use jellypilot_core::config::AppMode;
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::{BODY_FONT, HEADING_FONT};
use jellypilot_ui::icons::{
  icon_for_control_state, icon_with_color, Icon, IconControlState, IconSize,
};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{focus_tooltip, popover, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::{SETTINGS_MODAL_HEIGHT, SETTINGS_MODAL_WIDTH, TOKENS};
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::escape_input::clear_on_escape;
use jellypilot_ui::widgets::inert::inert;
use jellypilot_ui::widgets::search_field::search_field;
use jellypilot_ui::widgets::skeleton::skeleton_block;
use jellypilot_ui::widgets::{settings as settings_style, sidebar};
pub(crate) const SIDEBAR_WIDTH: f32 = 220.0;
pub(crate) const SIDEBAR_RAIL_WIDTH: f32 = 72.0;
/// Width of the two shell hairlines (sidebar edge and player-bar edge).
pub(crate) const HAIRLINE_WIDTH: f32 = 1.0;

fn platform_search_hint() -> &'static str {
  if cfg!(target_os = "macos") {
    "⌘K"
  } else {
    "Ctrl K"
  }
}

/// Content width before the shell hairline. The expanded sidebar is 220px
/// including that edge; the compact rail retains its existing content width.
pub(crate) fn sidebar_width(class: SizeClass) -> f32 {
  match class {
    SizeClass::Compact => SIDEBAR_RAIL_WIDTH,
    SizeClass::Standard | SizeClass::Wide => SIDEBAR_WIDTH - HAIRLINE_WIDTH,
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  if state.app_mode() == AppMode::ControlOnly {
    return control_only_view(state);
  }
  if crate::embedded::enabled()
    && state.shell.destination == Destination::NowPlaying
    && state.playback.view.lifecycle.playback_active
  {
    let player = player::embedded(state);
    return stack![
      if state.shell.settings_open {
        inert(player)
      } else {
        player
      },
      settings_layer(state)
    ]
    .width(Fill)
    .height(Fill)
    .boxed();
  }
  let palette = state.palette();
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let class = SizeClass::from_width(state.shell.window_size.width);
  // Discrete collapse animates the structural width; continuous window sizing
  // keeps the same key and stays direct.
  let sidebar = motion::resize(
    sidebar(state, class, skeleton_phase, reduced_motion)
      .width(Length::Fixed(sidebar_width(class))),
    sidebar_width(class) as u64,
    motion::Axis::Horizontal,
  );
  let content: Element<'_, Message> = match &state.shell.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::PersonalLists(_) => personal_lists::view(state),
    Destination::SavedBrowse => super::saved_browse::page(state),
    Destination::Detail(_) => detail::view(state),
    Destination::NowPlaying if crate::embedded::enabled() => player::full(state),
    Destination::NowPlaying => home::view(state),
  };
  let content = motion::transition(
    super::scroll_memory::remember(content, &state.shell.scroll_memory),
    motion::page_key(&state.shell.destination),
  );
  // One of the two shell hairlines: 1px between the sidebar and the content.
  let sidebar_divider = container(space::vertical())
    .width(HAIRLINE_WIDTH)
    .height(Fill)
    .style(move |_| {
      iced::widget::container::Style::default().background(palette.colors.outlineVariant)
    });
  // The sidebar docks full-height so its bottom (Settings, user) never moves
  // when the player bar appears; the bar docks under the content region only.
  let mut right = Column::<Element<'_, Message>>::new().spacing(0.0).push(
    (container(stack![content, super::undo_notice::view(state)])
      .width(Fill)
      .height(Fill))
    .boxed(),
  );
  // The docked bar is a player-control surface: it collapses over the surface
  // duration so the content above reclaims its slot as it exits. When playback
  // stops, the retained snapshot keeps drawing the real bar through the exit.
  let player_bar = player::bar(state);
  let player_bar_visible = player_bar.is_some();
  let player_bar: Element<'_, Message> =
    if let Some(player_bar) = player_bar.or_else(|| player::retained_bar(state)) {
      Column::<Element<'_, Message>>::new()
        .spacing(0.0)
        .push(
          (container(space::horizontal())
            .width(Fill)
            .height(HAIRLINE_WIDTH)
            .style(move |_| {
              iced::widget::container::Style::default().background(palette.colors.outlineVariant)
            }))
          .boxed(),
        )
        .push((player_bar).boxed())
        .boxed()
    } else {
      space().boxed()
    };
  let player_bar = motion::collapse(player_bar, player_bar_visible);
  right = right.push((player_bar).boxed());
  let body = row![sidebar, sidebar_divider, right]
    .spacing(0.0)
    .width(Fill)
    .height(Fill);

  let base_view = container(body)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas));

  // The modal layer stays mounted so its exit can animate; the inert shield
  // follows the logical open state, not the animation.
  let layers = stack![
    if state.shell.settings_open {
      inert(base_view)
    } else {
      base_view.boxed()
    },
    settings_layer(state)
  ];
  layers.width(Fill).height(Fill).boxed()
}

/// The Settings dialog wrapped in the shared surface reveal so open/close
/// animate over the surface duration.
fn settings_layer(state: &State) -> Element<'_, Message> {
  motion::reveal(settings_modal(state), state.shell.settings_open)
}

/// Control-Only shell: full-window Now Playing without browser chrome,
/// with the same Settings dialog used by the full shell.
fn control_only_view(state: &State) -> Element<'_, Message> {
  let player = player::full(state);
  let content = stack![
    if state.shell.settings_open {
      inert(player)
    } else {
      player
    },
    settings_layer(state)
  ];
  container(content)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .boxed()
}

fn sidebar(
  state: &State,
  class: SizeClass,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> container::Container<'_, Element<'_, Message>> {
  match class {
    SizeClass::Compact => sidebar_compact(state),
    SizeClass::Standard | SizeClass::Wide => sidebar_full(state, skeleton_phase, reduced_motion),
  }
}

fn sidebar_full(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> container::Container<'_, Element<'_, Message>> {
  let search_draft = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .browse
    .search_input;
  let search_slot =
    unified_search_field(state.kernel.locale, search_draft, Some(SEARCH_TRIGGER_ID));
  let personal_destination = Destination::PersonalLists(Route::Overview);
  let personal_active = matches!(state.shell.destination, Destination::PersonalLists(_));
  let personal_navigation = Column::<Element<'_, Message>>::new()
    .spacing(TOKENS.spacing.s0_5)
    .push(
      (destination_button(
        Icon::Home,
        state.t("common-home"),
        Destination::Home,
        state.shell.destination == Destination::Home,
      ))
      .boxed(),
    )
    .push(
      (destination_button(
        Icon::Heart,
        state.t("shell-personal-lists"),
        personal_destination,
        personal_active,
      ))
      .boxed(),
    )
    .push(
      (destination_button(
        Icon::Filter,
        state.t("saved-filters-title"),
        Destination::SavedBrowse,
        state.shell.destination == Destination::SavedBrowse,
      ))
      .boxed(),
    );

  let libraries = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Idle | LoadState::Loading => Column::<Element<'_, Message>>::new()
      .spacing(TOKENS.spacing.s0_5)
      .push((shortcut_skeleton(skeleton_phase, reduced_motion)).boxed())
      .push((shortcut_skeleton(skeleton_phase, reduced_motion)).boxed()),
    LoadState::Ready(shortcuts) => {
      let mut libraries = Column::<Element<'_, Message>>::new().spacing(TOKENS.spacing.s0_5);
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        libraries = libraries.push(
          (destination_button(
            Icon::for_collection_type(&shortcut.collection_type),
            &shortcut.name,
            destination,
            active,
          ))
          .boxed(),
        );
      }
      libraries
    }
    LoadState::Failed(_) => Column::<Element<'_, Message>>::new().push(
      (text(state.t("shell-libraries-unavailable"))
        .size(12)
        .color(state.palette().colors.warning))
      .boxed(),
    ),
  };

  let mut library_heading = row![text(state.t("shell-libraries"))
    .size(12)
    .color(state.palette().text.metadata)]
  .width(Fill)
  .align_y(Alignment::Center);
  if let LoadState::Ready(shortcuts) = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    library_heading = library_heading.push((space::horizontal()).boxed()).push(
      (text(shortcuts.len().to_string())
        .size(12)
        .color(state.palette().text.metadata))
      .boxed(),
    );
  }
  let library_heading = container(library_heading).padding(iced::Padding {
    top: 0.0,
    right: TOKENS.spacing.s3,
    bottom: TOKENS.spacing.s1_5,
    left: TOKENS.spacing.s3,
  });
  let main = column![search_slot, personal_navigation, library_heading]
    .spacing(TOKENS.spacing.s2)
    .width(Fill);
  let libraries = scrollable(libraries)
    .smooth_scroll(false)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable);
  let bottom = sidebar_footer(state, false);
  let content = column![main, libraries, bottom]
    .spacing(TOKENS.spacing.s2)
    .width(Fill)
    .height(Fill);

  container(content.boxed())
    .padding([TOKENS.spacing.s4, TOKENS.spacing.s3])
    .height(Fill)
    .style(|_theme| container::Style {
      background: Some(Background::Color(state.palette().colors.sidebarBg)),
      ..container::Style::default()
    })
}

/// One shared neutral field keeps the full sidebar and compact-search popover
/// visually and behaviorally identical. The compact rail owns the trigger ID;
/// the expanded field owns it directly.
fn unified_search_field<'a>(
  locale: Localizer,
  search_draft: &'a str,
  trigger_id: Option<&'static str>,
) -> Element<'a, Message> {
  let leading = control_button(Some(Icon::Search), None, ButtonVariant::Text)
    .style(sidebar::search_action)
    .icon_size(IconSize::Sm)
    .min_height(30.0)
    .padding([7, 8])
    .on_press(Message::Browse(BrowseMessage::SearchSubmitted));
  let leading = match trigger_id {
    Some(id) => leading.id(id),
    None => leading,
  };
  let input = clear_on_escape(
    text_input(locale.text("shell-search-placeholder"), search_draft)
      .on_input(|value| Message::Browse(BrowseMessage::SearchInputChanged(value)))
      .on_submit(Message::Browse(BrowseMessage::SearchSubmitted))
      .id(Id::new(SEARCH_INPUT_ID))
      .padding([5, 0])
      .size(12)
      .width(Fill)
      .style(sidebar::search_input),
    Message::Shell(ShellMessage::ClearSearch),
  );
  let keycap = container(
    text(platform_search_hint())
      .size(11)
      .style(|theme| text::Style {
        color: Some(jellypilot_ui::tokens::palette(theme).text.metadata),
        ..text::Style::default()
      }),
  )
  .center_x(Length::Fixed(44.0))
  .center_y(Length::Fixed(30.0));
  let trailing: Element<'_, Message> = if search_draft.is_empty() {
    keycap.boxed()
  } else {
    focus_tooltip(
      control_button(Some(Icon::Close), None, ButtonVariant::Text)
        .style(sidebar::search_action)
        .icon_size(IconSize::Sm)
        .min_height(30.0)
        .padding([7, 8])
        .width(Length::Fixed(44.0))
        .content_centered(true)
        .on_press(Message::Shell(ShellMessage::ClearSearch)),
      locale.text("shell-clear-search"),
      TooltipOptions::default(),
    )
  };

  search_field(leading, input, trailing).boxed()
}

fn sidebar_compact(state: &State) -> container::Container<'_, Element<'_, Message>> {
  let search_trigger = focus_tooltip(
    control_button(Some(Icon::Search), None, ButtonVariant::Tonal)
      .style(sidebar::personal)
      .id(SEARCH_TRIGGER_ID)
      .min_height(36.0)
      .padding([7, 0])
      .width(Fill)
      .content_centered(true)
      .on_press(Message::Shell(ShellMessage::ToggleCompactSearch)),
    state.t("common-search"),
    TooltipOptions::default(),
  );
  let search_draft = &state
    .full
    .as_ref()
    .expect("FullUi required")
    .browse
    .search_input;
  let compact_search_content = unified_search_field(state.kernel.locale, search_draft, None);
  let compact_search = popover(
    search_trigger,
    compact_search_content,
    state.shell.compact_search_open,
    PopoverOptions {
      width: Some(288.0),
      ..PopoverOptions::default()
    },
    Message::Shell(ShellMessage::DismissCompactSearch),
  );
  let personal_destination = Destination::PersonalLists(Route::Overview);
  let personal_active = matches!(state.shell.destination, Destination::PersonalLists(_));
  let personal_navigation = Column::<Element<'_, Message>>::new()
    .spacing(TOKENS.spacing.s1_5)
    .align_x(Alignment::Center)
    .width(Fill)
    .push(
      (compact_destination_button(
        Icon::Home,
        state.t("common-home"),
        Destination::Home,
        state.shell.destination == Destination::Home,
      ))
      .boxed(),
    );
  let libraries = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Idle | LoadState::Loading => Column::<Element<'_, Message>>::new()
      .spacing(TOKENS.spacing.s2)
      .align_x(Alignment::Center)
      .push(
        (shortcut_skeleton(
          state.shell.skeleton_phase,
          state.kernel.settings.snapshot().reduced_motion(),
        ))
        .boxed(),
      )
      .push(
        (shortcut_skeleton(
          state.shell.skeleton_phase,
          state.kernel.settings.snapshot().reduced_motion(),
        ))
        .boxed(),
      ),
    LoadState::Ready(shortcuts) => {
      let mut libraries = Column::<Element<'_, Message>>::new()
        .spacing(TOKENS.spacing.s1_5)
        .align_x(Alignment::Center);
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        libraries = libraries.push(
          (compact_destination_button(
            Icon::for_collection_type(&shortcut.collection_type),
            &shortcut.name,
            destination,
            active,
          ))
          .boxed(),
        );
      }
      libraries
    }
    LoadState::Failed(_) => Column::<Element<'_, Message>>::new()
      .push((icon_with_color(Icon::Warning, IconSize::Sm, state.palette().colors.warning)).boxed()),
  };
  let personal_navigation = personal_navigation
    .push(
      (compact_destination_button(
        Icon::Heart,
        state.t("shell-personal-lists"),
        personal_destination,
        personal_active,
      ))
      .boxed(),
    )
    .push(
      (compact_destination_button(
        Icon::Filter,
        state.t("saved-filters-title"),
        Destination::SavedBrowse,
        state.shell.destination == Destination::SavedBrowse,
      ))
      .boxed(),
    );
  let bottom = sidebar_footer(state, true);

  let content = column![
    compact_search,
    personal_navigation,
    scrollable(libraries).smooth_scroll(false).height(Fill),
    bottom
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill)
  .height(Fill);

  container(content.boxed())
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|_theme| container::Style {
      background: Some(Background::Color(state.palette().colors.sidebarBg)),
      ..container::Style::default()
    })
}

fn settings_modal(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let modal_content = container(settings::view(state)).width(Fill).height(Fill);

  let dialog = container(super::modal_dismiss::dismissible(
    modal_content
      .width(Length::Fixed(
        (state.shell.window_size.width - 48.0).clamp(0.0, SETTINGS_MODAL_WIDTH),
      ))
      .height(Length::Fixed(
        (state.shell.window_size.height - 48.0).clamp(0.0, SETTINGS_MODAL_HEIGHT),
      ))
      .padding(1)
      .clip(true)
      .style(settings_style::dialog),
    Message::Settings(SettingsMessage::Close),
  ))
  .width(Fill)
  .height(Fill)
  .padding(24)
  .center_x(Fill)
  .center_y(Fill)
  .style(move |_| container::Style::default().background(palette.colors.surface.scale_alpha(0.6)));
  // The account layer blurs the complete scene, including Settings, once.
  if crate::app::accounts::blocking_modal(&state.accounts) {
    dialog.boxed()
  } else {
    modal(TOKENS.modal.backdrop_blur_sigma, dialog).boxed()
  }
}

fn sidebar_footer(state: &State, compact: bool) -> Element<'_, Message> {
  let account = account::sidebar_popover(state);
  let tools = footer_toolbar(state, compact);
  let main = if compact {
    column![account, tools]
      .spacing(TOKENS.spacing.s1)
      .width(Fill)
      .boxed()
  } else {
    row![account, tools]
      .spacing(TOKENS.spacing.s1)
      .align_y(Alignment::Center)
      .width(Fill)
      .boxed()
  };
  let mut content = column![main].spacing(TOKENS.spacing.s2).width(Fill);
  if !crate::app::shell::refresh_busy(state) {
    if let Some(failure) = crate::app::shell::refresh_failure(state) {
      let detail = state.kernel.locale.message(failure);
      let feedback: Element<'_, Message> = if compact {
        focus_tooltip(
          control_button(Some(Icon::Refresh), None, ButtonVariant::Text)
            .id("shell.refresh-retry")
            .style(sidebar::footer_retry)
            .icon_size(IconSize::Sm)
            .padding(0)
            .min_height(40.0)
            .width(Fill)
            .content_centered(true)
            .on_press(Message::Shell(ShellMessage::RefreshCurrent)),
          format!("{} · {detail}", state.t("common-retry")),
          TooltipOptions::default(),
        )
      } else {
        container(
          row![
            text(state.t("shell-refresh-failed"))
              .size(12)
              .line_height(iced::Pixels(16.0))
              .color(state.palette().colors.error)
              .width(Fill),
            focus_tooltip(
              control_button(None, Some(state.t("common-retry")), ButtonVariant::Tonal)
                .id("shell.refresh-retry")
                .style(sidebar::footer_retry)
                .label_size(12.0)
                .padding(TOKENS.spacing.s2)
                .width(Length::Fixed(56.0))
                .min_height(40.0)
                .content_centered(true)
                .on_press(Message::Shell(ShellMessage::RefreshCurrent)),
              detail,
              TooltipOptions::default(),
            ),
          ]
          .spacing(TOKENS.spacing.s2)
          .align_y(Alignment::Center),
        )
        .padding([0.0, TOKENS.spacing.s2])
        .boxed()
      };
      content = content.push(feedback);
    }
  }
  container(content).id("shell.footer").boxed()
}

fn footer_toolbar(state: &State, compact: bool) -> Element<'_, Message> {
  let settings = focus_tooltip(
    control_button(Some(Icon::Settings), None, ButtonVariant::Text)
      .icon_size(IconSize::Sm)
      .content_centered(true)
      .style(sidebar::footer_action)
      .id(SETTINGS_TRIGGER_ID)
      .padding(0)
      .min_height(40.0)
      .width(Fill)
      .on_press(Message::Settings(SettingsMessage::Open)),
    state.t("common-settings"),
    TooltipOptions::default(),
  );
  let refreshing = crate::app::shell::refresh_busy(state);
  let refresh = focus_tooltip(
    control_button(Some(Icon::Refresh), None, ButtonVariant::Text)
      .id("shell.refresh")
      .style(sidebar::footer_action)
      .icon_size(IconSize::Sm)
      .padding(0)
      .min_height(40.0)
      .width(Fill)
      .content_centered(true)
      .on_press_maybe((!refreshing).then_some(Message::Shell(ShellMessage::RefreshCurrent))),
    if refreshing {
      state.t("shell-refreshing")
    } else {
      state.t("common-refresh")
    },
    TooltipOptions::default(),
  );
  let control = focus_tooltip(
    control_button(Some(Icon::ControlMode), None, ButtonVariant::Text)
      .id("shell.control-mode")
      .style(sidebar::footer_action)
      .icon_size(IconSize::Sm)
      .padding(0)
      .min_height(40.0)
      .width(Fill)
      .content_centered(true)
      .on_press(Message::Settings(SettingsMessage::AppModeSelected(
        AppMode::ControlOnly,
      ))),
    state.t("shell-control-mode"),
    TooltipOptions::default(),
  );
  let tools = if compact {
    column![settings, refresh, control]
      .spacing(TOKENS.spacing.s1)
      .align_x(Alignment::Center)
      .width(Fill)
      .boxed()
  } else {
    row![settings, refresh, control]
      .spacing(TOKENS.spacing.s1)
      .align_y(Alignment::Center)
      .width(Fill)
      .boxed()
  };
  container(tools)
    .width(if compact {
      Fill
    } else {
      Length::Fill.min(144.0)
    })
    .padding(iced::Padding {
      left: if compact { 0.0 } else { TOKENS.spacing.s2 },
      right: if compact { 0.0 } else { TOKENS.spacing.s2 },
      ..iced::Padding::ZERO
    })
    .boxed()
}

fn destination_button<'a>(
  icon: Icon,
  label: impl Into<Cow<'a, str>>,
  destination: Destination,
  active: bool,
) -> Element<'a, Message> {
  let label = label.into();
  let tooltip_label = label.clone();
  let is_library = matches!(&destination, Destination::Library { .. });
  let variant = if active {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  let btn = control_button_content(
    move |state| {
      let status = match state {
        IconControlState::Rest => button::Status::Active,
        IconControlState::Hovered => button::Status::Hovered,
        IconControlState::Disabled => button::Status::Disabled,
      };
      row![
        icon_for_control_state(icon, IconSize::Md, variant, state),
        container(
          ellipsis_text(label.clone())
            .size(14)
            .font(if active { HEADING_FONT } else { BODY_FONT })
            .style(move |theme| text::Style {
              color: Some(jellypilot_ui::widgets::button::style(theme, variant, status).text_color),
              ..text::Style::default()
            })
        )
        .width(Fill),
      ]
      .spacing(TOKENS.spacing.s2_5)
      .align_y(Alignment::Center)
      .width(Fill)
      .boxed()
    },
    variant,
  )
  .style(if is_library {
    sidebar::library
  } else {
    sidebar::personal
  })
  .min_height(38.0)
  .padding([9, 12])
  .width(Fill)
  .on_press(Message::Home(HomeMessage::Navigate(destination)));

  focus_tooltip(btn, tooltip_label, TooltipOptions::default())
}
fn shortcut_skeleton<'a>(skeleton_phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  skeleton_block(Length::Fill, 38.0, skeleton_phase, reduced_motion).boxed()
}

fn compact_destination_button<'a>(
  icon: Icon,
  label: impl Into<String>,
  destination: Destination,
  active: bool,
) -> Element<'a, Message> {
  let is_library = matches!(&destination, Destination::Library { .. });
  let variant = if active {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  let btn = control_button(Some(icon), None, variant)
    .style(if is_library {
      sidebar::library
    } else {
      sidebar::personal
    })
    .min_height(38.0)
    .padding([7, 0])
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Home(HomeMessage::Navigate(destination)));

  focus_tooltip(btn, label, TooltipOptions::default())
}

#[cfg(test)]
mod tests {
  use iced::advanced::{layout, renderer::Headless, widget, Widget as _};
  use iced::Size;

  use super::*;

  #[tokio::test]
  async fn compact_library_loading_preserves_ready_row_geometry() {
    let renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let limits = layout::Limits::new(Size::ZERO, Size::new(40.0, 400.0));
    let sizes = [
      shortcut_skeleton(0.0, true),
      compact_destination_button(
        Icon::Home,
        "Library",
        Destination::Library {
          library_id: "library".into(),
          collection_type: "movies".into(),
        },
        false,
      ),
    ]
    .map(|mut element| {
      let mut tree = widget::Tree::new(&element);
      tree.diff(&mut element);
      element.layout(&mut tree, &renderer, &limits);
      tree.size
    });
    assert_eq!(sizes[0], sizes[1], "loading must not shift library rows");
  }

  #[tokio::test]
  async fn identity_disclosure_wraps_without_resizing_and_menu_tracks_feedback_anchor() {
    use crate::app::shell::ACCOUNT_TRIGGER_ID;
    use iced::advanced::widget::operation::focusable;
    use iced::{Point, Rectangle, Vector};

    #[derive(Default)]
    struct Panels(Vec<(Option<widget::Id>, Rectangle)>);
    impl widget::Operation for Panels {
      fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation)) {
        visit(self);
      }
      fn container(&mut self, id: Option<&widget::Id>, bounds: Rectangle, _: &Rectangle) {
        self.0.push((id.cloned(), bounds));
      }
    }
    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut state = State::boot(false);
    state.kernel.settings = jellypilot_core::config::SettingsStore::default();
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.kernel.connected_identity = Some(crate::app::state::ConnectedIdentity {
      user_name: "LongAccountNameWithoutSpaces".repeat(3),
      provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
      server_url: "https://media.example.test/long-server-address".into(),
      server_name: None,
    });
    let viewport = Rectangle::with_size(Size::new(800.0, 700.0));
    for menu_open in [false, true] {
      state.shell.account_popover_open = menu_open;
      for failed in [false, true] {
        state.kernel.notice = failed.then(|| crate::i18n::UiText::new("home-refresh-failed"));
        let mut footer = sidebar_footer(&state, false);
        let mut tree = widget::Tree::new(&footer);
        tree.diff(&mut footer);
        footer.layout(
          &mut tree,
          &renderer,
          &layout::Limits::new(Size::ZERO, Size::new(195.0, 650.0)),
        );
        let size = tree.size;
        let anchor_top = viewport.height - 16.0 - size.height;
        let layout = iced::advanced::Layout::new(size).move_to(Point::new(12.0, anchor_top));
        footer.operate(
          &mut tree,
          layout,
          &viewport,
          &renderer,
          &mut focusable::focus::<()>(widget::Id::new(ACCOUNT_TRIGGER_ID)),
        );
        let mut overlays = footer.overlay(
          &mut tree,
          layout,
          &renderer,
          &viewport,
          Vector::ZERO,
          viewport.size(),
        );
        assert_eq!(
          overlays.len(),
          1,
          "the menu and identity hint must never overlap"
        );
        let mut panels = Panels::default();
        overlays[0].as_overlay_mut().operate(&renderer, &mut panels);
        let hint = panels
          .0
          .iter()
          .find(|(id, _)| id.as_ref() == Some(&widget::Id::new("account-identity-hint")));
        if menu_open {
          assert!(hint.is_none());
          assert!(panels
            .0
            .iter()
            .any(|(id, _)| id.as_ref() == Some(&widget::Id::new("account-settings"))));
          let panel = panels
            .0
            .iter()
            .map(|(_, bounds)| bounds)
            .filter(|bounds| (bounds.width - 340.0).abs() < 0.01)
            .max_by(|a, b| a.height.total_cmp(&b.height))
            .expect("account menu frame");
          assert!((panel.y + panel.height - (anchor_top - 8.0)).abs() < 0.01);
        } else {
          let (_, hint) = hint.expect("focused avatar exposes full identity");
          assert_eq!(hint.width, 195.0);
          assert!(
            hint.height > 54.0,
            "long names and addresses wrap instead of truncating"
          );
          assert!((hint.y + hint.height - (anchor_top - 8.0)).abs() < 0.01);
        }
        overlays[0].as_overlay_mut().draw(
          &mut renderer,
          &iced::Theme::Dark,
          &iced::advanced::renderer::Style {
            text_color: state.palette().text.body,
          },
          iced::mouse::Cursor::Unavailable,
        );
        drop(overlays);
        assert_eq!(tree.size, size, "disclosure must not enlarge the footer");
      }
    }
  }

  #[tokio::test]
  async fn footer_keeps_long_identity_bounded_and_shortcuts_reachable_in_tab_order() {
    use crate::app::shell::ACCOUNT_TRIGGER_ID;
    use crate::app::view::viewing_queue::tests::{click, Bounds};
    use iced::advanced::widget::operation::focusable;
    use iced_runtime::user_interface::{Cache, UserInterface};
    use jellypilot_core::locale::UiLanguage;

    let mut renderer = iced::Renderer::new(
      iced::advanced::renderer::Settings::default(),
      Some("tiny-skia"),
    )
    .await
    .expect("software renderer");
    let mut state = State::boot(false);
    state.kernel.settings = jellypilot_core::config::SettingsStore::default();
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.kernel.connected_identity = Some(crate::app::state::ConnectedIdentity {
      user_name: "A very long account name 用户名称".repeat(8),
      provider: jellypilot_media_server::MediaServerProvider::Jellyfin,
      server_url: "https://media.example.test".into(),
      server_name: Some("A very long server name 服务器名称".repeat(8)),
    });
    for language in [UiLanguage::English, UiLanguage::SimplifiedChinese] {
      state.kernel.locale = Localizer::new(language);
      for compact in [false, true] {
        let width = if compact { 40.0 } else { 195.0 };
        let size = Size::new(width, 400.0);
        let mut cache = Cache::new();
        for expected in [
          ACCOUNT_TRIGGER_ID,
          SETTINGS_TRIGGER_ID,
          "shell.refresh",
          "shell.control-mode",
        ] {
          let mut ui =
            UserInterface::build(sidebar_footer(&state, compact), size, cache, &mut renderer);
          let mut operation: Box<dyn widget::Operation> = Box::new(focusable::focus_next());
          loop {
            ui.operate(&renderer, operation.as_mut());
            if let widget::operation::Outcome::Chain(next) = operation.finish() {
              operation = next;
            } else {
              break;
            }
          }
          let mut bounds = Bounds::default();
          ui.operate(&renderer, &mut bounds);
          assert_eq!(bounds.1, Some(widget::Id::new(expected)));
          let account = bounds.get(ACCOUNT_TRIGGER_ID);
          let settings = bounds.get(SETTINGS_TRIGGER_ID);
          let refresh = bounds.get("shell.refresh");
          let control = bounds.get("shell.control-mode");
          assert_eq!(account.size(), Size::new(40.0, 40.0));
          if compact {
            assert_eq!(settings.y - account.y - account.height, 4.0);
          } else {
            assert_eq!(settings.y, account.y);
          }
          for target in [settings, refresh, control] {
            assert_eq!(target.height, 40.0);
            assert!(target.x >= 0.0 && target.x + target.width <= width);
          }
          if !compact {
            assert_eq!(bounds.get("shell.footer").height, 40.0);
            assert_eq!(settings.width, refresh.width);
            assert_eq!(refresh.width, control.width);
            assert!(settings.width >= 40.0);
            assert_eq!(settings.x - account.x - account.width, 12.0);
            assert!((control.x + control.width - (width - 8.0)).abs() < 0.01);
            assert!((refresh.x - settings.x - settings.width - 4.0).abs() < 0.01);
            assert!((control.x - refresh.x - refresh.width - 4.0).abs() < 0.01);
          }
          let messages = click(&mut ui, &mut renderer, bounds.get(expected));
          assert!(
            match expected {
              ACCOUNT_TRIGGER_ID => matches!(
                messages.as_slice(),
                [Message::Shell(ShellMessage::ToggleAccountPopover)]
              ),
              SETTINGS_TRIGGER_ID => matches!(
                messages.as_slice(),
                [Message::Settings(SettingsMessage::Open)]
              ),
              "shell.refresh" => matches!(
                messages.as_slice(),
                [Message::Shell(ShellMessage::RefreshCurrent)]
              ),
              _ => matches!(
                messages.as_slice(),
                [Message::Settings(SettingsMessage::AppModeSelected(
                  AppMode::ControlOnly
                ))]
              ),
            },
            "each footer hit must emit only its own action: {messages:?}"
          );
          // Clicking retires keyboard focus; restore the current stop before
          // carrying this widget tree into the next traversal.
          ui.operate(
            &renderer,
            &mut focusable::focus::<()>(widget::Id::new(expected)),
          );
          cache = ui.into_cache();
        }
      }
    }

    state.shell.destination = Destination::SavedBrowse;
    state.saved_browse.pending = Some(crate::app::saved_browse::PendingAction::Load);
    let mut ui = UserInterface::build(
      sidebar_footer(&state, false),
      Size::new(195.0, 400.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    assert!(click(&mut ui, &mut renderer, bounds.get("shell.refresh")).is_empty());
    drop(ui);
    state.saved_browse.pending = None;
    state.kernel.notice = Some(crate::i18n::UiText::new("home-refresh-failed"));
    let mut ui = UserInterface::build(
      sidebar_footer(&state, false),
      Size::new(195.0, 400.0),
      Cache::new(),
      &mut renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(&renderer, &mut bounds);
    let retry = bounds.get("shell.refresh-retry");
    let tools = bounds.get("shell.refresh");
    assert_eq!(retry.y - tools.y - tools.height, 8.0);
    assert!(matches!(
      click(&mut ui, &mut renderer, retry).as_slice(),
      [Message::Shell(ShellMessage::RefreshCurrent)]
    ));
  }
}
