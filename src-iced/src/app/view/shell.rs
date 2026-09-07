use std::borrow::Cow;

use super::{account, browse, detail, home, personal_lists, player, settings};
use crate::app::message::{BrowseMessage, HomeMessage, Message, SettingsMessage, ShellMessage};
use crate::app::personal_lists::Route;
use crate::app::shell::{SEARCH_INPUT_ID, SEARCH_TRIGGER_ID, SETTINGS_TRIGGER_ID};
use crate::app::state::{Destination, State};
use crate::i18n::Localizer;
use iced::widget::{
  button, column, container, modal, row, scrollable, space, stack, text, text_input, Column, Id,
};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_core::config::AppMode;
use jellypilot_core::LoadState;
use jellypilot_ui::fonts::DISPLAY_FONT;
use jellypilot_ui::icons::{
  icon_for_control_state, icon_with_color, Icon, IconControlState, IconSize,
};
use jellypilot_ui::layout::SizeClass;
use jellypilot_ui::overlay::{focus_tooltip, popover, tooltip, PopoverOptions, TooltipOptions};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{ButtonVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::escape_input::clear_on_escape;
use jellypilot_ui::widgets::inert::inert;
use jellypilot_ui::widgets::search_field::search_field;
use jellypilot_ui::widgets::sidebar;
use jellypilot_ui::widgets::skeleton::skeleton_block;
pub(crate) const SIDEBAR_WIDTH: f32 = 240.0;
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

/// Returns the sidebar width corresponding to the given window-width [`SizeClass`].
///
/// Compact windows collapse the sidebar to a 72px icon rail to maximize screen
/// real estate for media content, while Standard and Wide windows use the full 240px panel.
pub(crate) fn sidebar_width(class: SizeClass) -> f32 {
  match class {
    SizeClass::Compact => SIDEBAR_RAIL_WIDTH,
    SizeClass::Standard | SizeClass::Wide => SIDEBAR_WIDTH,
  }
}

pub fn view(state: &State) -> Element<'_, Message> {
  if state.app_mode() == AppMode::ControlOnly {
    return control_only_view(state);
  }
  let palette = state.palette();
  let skeleton_phase = state.shell.skeleton_phase;
  let reduced_motion = state.kernel.settings.snapshot().reduced_motion();
  let class = SizeClass::from_width(state.shell.window_size.width);
  let sidebar = sidebar(state, class, skeleton_phase, reduced_motion)
    .width(Length::Fixed(sidebar_width(class)));
  let content: Element<'_, Message> = match &state.shell.destination {
    Destination::Home => home::view(state),
    Destination::Library { .. } | Destination::Search(_) => browse::view(state),
    Destination::PersonalLists(_) => personal_lists::view(state),
    Destination::Detail(_) => detail::view(state),
    // Now Playing is the Control-Only root; the router never routes here in
    // Full mode, where the player is a bar.
    Destination::NowPlaying => home::view(state),
  };
  // One of the two shell hairlines: 1px between the sidebar and the content.
  let sidebar_divider = container(space::vertical())
    .width(HAIRLINE_WIDTH)
    .height(Fill)
    .style(move |_| {
      iced::widget::container::Style::default().background(palette.colors.outlineVariant)
    });
  // The sidebar docks full-height so its bottom (Settings, user) never moves
  // when the player bar appears; the bar docks under the content region only.
  let mut right = Column::new()
    .spacing(0.0)
    .push(container(content).width(Fill).height(Fill));
  if let Some(player_bar) = player::bar(state) {
    // The second shell hairline: 1px above the player bar.
    let player_divider = container(space::horizontal())
      .width(Fill)
      .height(HAIRLINE_WIDTH)
      .style(move |_| {
        iced::widget::container::Style::default().background(palette.colors.outlineVariant)
      });
    right = right.push(player_divider).push(player_bar);
  }
  let body = row![sidebar, sidebar_divider, right]
    .spacing(0.0)
    .width(Fill)
    .height(Fill);

  let base_view = container(body)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas));

  if state.shell.settings_open {
    // Keep the shell visible below Settings while removing it from input,
    // overlays, and focus traversal.
    let modal_stack = stack![inert(base_view), settings_modal(state)]
      .width(Fill)
      .height(Fill);
    container(modal_stack)
      .width(Fill)
      .height(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into()
  } else {
    base_view.into()
  }
}

/// Control-Only shell: no sidebar, no hairlines, no player bar — the compact
/// full-window Now Playing view, or full-window Settings.
fn control_only_view(state: &State) -> Element<'_, Message> {
  let content: Element<'_, Message> = if state.shell.settings_open {
    settings_modal(state)
  } else {
    player::full(state)
  };
  container(content)
    .width(Fill)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .into()
}

fn sidebar(
  state: &State,
  class: SizeClass,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> container::Container<'_, Message> {
  match class {
    SizeClass::Compact => sidebar_compact(state),
    SizeClass::Standard | SizeClass::Wide => sidebar_full(state, skeleton_phase, reduced_motion),
  }
}

fn sidebar_full(
  state: &State,
  skeleton_phase: f32,
  reduced_motion: bool,
) -> container::Container<'_, Message> {
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
  let personal_navigation = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .push(destination_button(
      Icon::Home,
      state.t("common-home"),
      Destination::Home,
      state.shell.destination == Destination::Home,
    ))
    .push(destination_button(
      Icon::Heart,
      state.t("shell-personal-lists"),
      personal_destination,
      personal_active,
    ));

  let libraries = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Idle | LoadState::Loading => Column::new()
      .spacing(TOKENS.spacing.s1_5)
      .push(shortcut_skeleton(skeleton_phase, reduced_motion))
      .push(shortcut_skeleton(skeleton_phase, reduced_motion)),
    LoadState::Ready(shortcuts) => {
      let mut libraries = Column::new().spacing(TOKENS.spacing.s1_5);
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        libraries = libraries.push(destination_button(
          Icon::for_collection_type(&shortcut.collection_type),
          &shortcut.name,
          destination,
          active,
        ));
      }
      libraries
    }
    LoadState::Failed(_) => Column::new().push(
      text(state.t("shell-libraries-unavailable"))
        .size(12)
        .color(state.palette().colors.warning),
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
    library_heading = library_heading.push(space::horizontal()).push(
      container(text(shortcuts.len().to_string()).size(11))
        .padding([2, 6])
        .style(sidebar::count_badge),
    );
  }
  let main = column![search_slot, personal_navigation, library_heading,]
    .spacing(TOKENS.spacing.s4)
    .width(Fill);
  let libraries = scrollable(libraries)
    .width(Fill)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable);
  let bottom = column![
    account::sidebar_popover(state, false),
    footer_toolbar(state),
  ]
  .spacing(TOKENS.spacing.s3);
  let content = column![main, libraries, bottom]
    .spacing(TOKENS.spacing.s4)
    .width(Fill)
    .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s3)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
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
    .min_height(40.0)
    .padding([7, 12])
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
      .padding([8, 2])
      .size(14)
      .width(Fill)
      .style(sidebar::search_input),
    Message::Shell(ShellMessage::ClearSearch),
  );
  let keycap = container(
    container(text(platform_search_hint()).size(11))
      .padding([2, 4])
      .center_x(Length::Fixed(48.0))
      .center_y(Length::Fixed(32.0))
      .style(sidebar::inset),
  )
  .padding(iced::Padding {
    top: 0.0,
    right: 2.0,
    bottom: 0.0,
    left: 0.0,
  });
  let trailing: Element<'_, Message> = if search_draft.is_empty() {
    keycap.into()
  } else {
    tooltip(
      control_button(Some(Icon::Close), None, ButtonVariant::Text)
        .style(sidebar::search_action)
        .min_height(40.0)
        .padding([4, 4])
        .width(Length::Fixed(48.0))
        .content_centered(true)
        .on_press(Message::Shell(ShellMessage::ClearSearch)),
      locale.text("shell-clear-search"),
      TooltipOptions::default(),
    )
  };

  search_field(leading, input, trailing).into()
}

fn sidebar_compact(state: &State) -> container::Container<'_, Message> {
  let search_trigger = tooltip(
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
  let personal_navigation = Column::new()
    .spacing(TOKENS.spacing.s1_5)
    .align_x(Alignment::Center)
    .width(Fill)
    .push(compact_destination_button(
      Icon::Home,
      state.t("common-home"),
      Destination::Home,
      state.shell.destination == Destination::Home,
    ));
  let libraries = match &state
    .full
    .as_ref()
    .expect("FullUi required")
    .home
    .data
    .shortcuts
  {
    LoadState::Idle | LoadState::Loading => Column::new()
      .spacing(TOKENS.spacing.s2)
      .align_x(Alignment::Center)
      .push(shortcut_skeleton(
        state.shell.skeleton_phase,
        state.kernel.settings.snapshot().reduced_motion(),
      ))
      .push(shortcut_skeleton(
        state.shell.skeleton_phase,
        state.kernel.settings.snapshot().reduced_motion(),
      )),
    LoadState::Ready(shortcuts) => {
      let mut libraries = Column::new()
        .spacing(TOKENS.spacing.s1_5)
        .align_x(Alignment::Center);
      for shortcut in shortcuts {
        let destination = Destination::Library {
          library_id: shortcut.id.clone(),
          collection_type: shortcut.collection_type.clone(),
        };
        let active = state.shell.destination == destination;
        libraries = libraries.push(compact_destination_button(
          Icon::for_collection_type(&shortcut.collection_type),
          &shortcut.name,
          destination,
          active,
        ));
      }
      libraries
    }
    LoadState::Failed(_) => Column::new().push(icon_with_color(
      Icon::Warning,
      IconSize::Sm,
      state.palette().colors.warning,
    )),
  };
  let personal_navigation = personal_navigation.push(compact_destination_button(
    Icon::Heart,
    state.t("shell-personal-lists"),
    personal_destination,
    personal_active,
  ));
  let refresh = tooltip(
    control_button(Some(Icon::Refresh), None, ButtonVariant::Tonal)
      .style(sidebar::action)
      .min_height(36.0)
      .padding([7, 0])
      .width(Fill)
      .content_centered(true)
      .on_press_maybe(
        (!state.shell.refresh_busy).then_some(Message::Shell(ShellMessage::RefreshCurrent)),
      ),
    if state.shell.refresh_busy {
      state.t("shell-refreshing")
    } else {
      state.t("common-refresh")
    },
    TooltipOptions::default(),
  );

  let bottom = column![
    account::sidebar_popover(state, true),
    compact_settings_button(state.kernel.locale),
    refresh,
    tooltip(
      control_button(Some(Icon::PictureInPicture), None, ButtonVariant::Tonal,)
        .style(sidebar::action)
        .min_height(36.0)
        .padding([7, 12])
        .width(Fill)
        .content_centered(true)
        .on_press(Message::Settings(SettingsMessage::AppModeSelected(
          AppMode::ControlOnly,
        ))),
      state.t("shell-control-mode"),
      TooltipOptions::default(),
    ),
  ]
  .spacing(TOKENS.spacing.s3)
  .align_x(Alignment::Center)
  .width(Fill);

  let content = column![
    compact_search,
    personal_navigation,
    scrollable(libraries).height(Fill),
    bottom
  ]
  .spacing(TOKENS.spacing.s4)
  .width(Fill)
  .height(Fill);

  container(content)
    .padding(TOKENS.spacing.s4)
    .height(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block))
}

fn settings_modal(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let close_button = control_button(Some(Icon::Close), None, ButtonVariant::Tonal)
    .padding([6, 10])
    .on_press(Message::Settings(SettingsMessage::Close));

  let header = row![
    column![
      text(state.t("common-settings"))
        .font(DISPLAY_FONT)
        .size(28)
        .color(palette.text.heading),
      text(state.t("shell-settings-save-hint"))
        .size(13)
        .color(palette.text.body),
    ]
    .spacing(TOKENS.spacing.s0_5),
    space::horizontal(),
    tooltip(
      close_button,
      state.t("common-close"),
      TooltipOptions::default()
    ),
  ]
  .width(Fill)
  .align_y(Alignment::Center);

  let modal_content = column![header, settings::view(state),]
    .spacing(TOKENS.spacing.s4)
    .padding([TOKENS.spacing.s4, TOKENS.spacing.s6])
    .width(Fill)
    .height(Fill);

  let narrow = state.app_mode() == AppMode::ControlOnly
    || SizeClass::from_width(state.shell.window_size.width) == SizeClass::Compact;
  if narrow {
    return container(modal_content)
      .width(Fill)
      .height(Fill)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
      .into();
  }

  let dialog = container(
    container(modal_content)
      .width(Length::Fixed(896.0))
      .height(Length::Fixed(
        (state.shell.window_size.height - 48.0).clamp(0.0, 620.0),
      ))
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Dialog)),
  )
  .width(Fill)
  .height(Fill)
  .padding(24)
  .center_x(Fill)
  .center_y(Fill)
  .style(move |_| container::Style::default().background(palette.colors.surface.scale_alpha(0.6)));
  // The account layer blurs the complete scene, including Settings, once.
  if crate::app::accounts::blocking_modal(&state.accounts) {
    dialog.into()
  } else {
    modal(12.0, dialog)
  }
}

fn settings_button<'a>() -> Element<'a, Message> {
  control_button(Some(Icon::Settings), None, ButtonVariant::Text)
    .style(sidebar::action)
    .id(SETTINGS_TRIGGER_ID)
    .min_height(40.0)
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Settings(SettingsMessage::Open))
    .into()
}

fn compact_settings_button<'a>(locale: Localizer) -> Element<'a, Message> {
  // Action, not navigation — neutral Tonal, never the ghost vocabulary.
  let btn = control_button(Some(Icon::Settings), None, ButtonVariant::Tonal)
    .style(sidebar::action)
    .id(SETTINGS_TRIGGER_ID)
    .min_height(36.0)
    .padding([7, 0])
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Settings(SettingsMessage::Open));

  tooltip(
    btn,
    locale.text("common-settings"),
    TooltipOptions::default(),
  )
}

/// The full sidebar groups its three global actions into one compact control
/// strip so the account trigger remains the clear visual anchor at the bottom.
fn footer_toolbar(state: &State) -> Element<'_, Message> {
  let divider = || {
    container(space::horizontal())
      .width(1.0)
      .height(24.0)
      .style(sidebar::divider)
  };
  let settings = tooltip(
    settings_button(),
    state.t("common-settings"),
    TooltipOptions::default(),
  );
  let refresh = tooltip(
    control_button(Some(Icon::Refresh), None, ButtonVariant::Text)
      .style(sidebar::action)
      .min_height(40.0)
      .width(Fill)
      .content_centered(true)
      .on_press_maybe(
        (!state.shell.refresh_busy).then_some(Message::Shell(ShellMessage::RefreshCurrent)),
      ),
    if state.shell.refresh_busy {
      state.t("shell-refreshing")
    } else {
      state.t("common-refresh")
    },
    TooltipOptions::default(),
  );
  let control = tooltip(
    control_button(Some(Icon::PictureInPicture), None, ButtonVariant::Text)
      .style(sidebar::action)
      .min_height(40.0)
      .width(Fill)
      .content_centered(true)
      .on_press(Message::Settings(SettingsMessage::AppModeSelected(
        AppMode::ControlOnly,
      ))),
    state.t("shell-control-mode"),
    TooltipOptions::default(),
  );

  container(
    row![
      container(settings).width(Length::FillPortion(1)),
      divider(),
      container(refresh).width(Length::FillPortion(1)),
      divider(),
      container(control).width(Length::FillPortion(1)),
    ]
    .align_y(Alignment::Center),
  )
  .padding(3)
  .width(Fill)
  .style(sidebar::toolbar)
  .into()
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
            .style(move |theme| text::Style {
              color: Some(jellypilot_ui::widgets::button::style(theme, variant, status).text_color),
            })
        )
        .width(Fill),
      ]
      .spacing(TOKENS.spacing.s2_5)
      .align_y(Alignment::Center)
      .width(Fill)
      .into()
    },
    variant,
  )
  .style(if is_library {
    sidebar::library
  } else {
    sidebar::personal
  })
  .min_height(if is_library { 32.0 } else { 38.0 })
  .padding(if is_library { [4, 12] } else { [7, 12] })
  .width(Fill)
  .on_press(Message::Home(HomeMessage::Navigate(destination)));

  focus_tooltip(btn, tooltip_label, TooltipOptions::default())
}
fn shortcut_skeleton<'a>(skeleton_phase: f32, reduced_motion: bool) -> Element<'a, Message> {
  skeleton_block(Length::Fill, 34.0, skeleton_phase, reduced_motion).into()
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
    .min_height(if is_library { 32.0 } else { 38.0 })
    .padding(if is_library { [4, 0] } else { [7, 0] })
    .width(Fill)
    .content_centered(true)
    .on_press(Message::Home(HomeMessage::Navigate(destination)));

  focus_tooltip(btn, label, TooltipOptions::default())
}
