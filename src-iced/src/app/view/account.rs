//! Shared account presentation for the sidebar popover and Settings.

use iced::widget::text::Wrapping;
use iced::widget::{column, container, row, scrollable, space, stack, text, text_input, Column};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length};
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_media_server::MediaServerProvider;
use jellypilot_session::RemoteControlState;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconControlState, IconSize};
use jellypilot_ui::overlay::{
  popover, tooltip, Alignment as PopoverAlignment, Placement, PopoverOptions, TooltipOptions,
};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};

use crate::app::accounts::{self, AccountView, ConfirmationKind, CopyStatus};
use crate::app::login::{CandidateMessage, CandidateSurface};
use crate::app::message::{Message, SettingsMessage, ShellMessage};
use crate::app::shell::{
  profile_action_id, ACCOUNT_ADD_TRIGGER_ID, ACCOUNT_DISCONNECT_TRIGGER_ID, ACCOUNT_TRIGGER_ID,
};
use crate::app::state::{LoginMethod, QuickConnectState, State};

const POPOVER_WIDTH: f32 = 368.0;
const POPOVER_CONTENT_HEIGHT: f32 = 520.0;
const PROFILE_LIST_HEIGHT: f32 = 192.0;

#[derive(Clone, Copy)]
enum AvatarShape {
  Circle,
  RoundedSquare,
}

/// The sidebar identity card and its account-scoped, anchored popover.
pub fn sidebar_popover(state: &State, compact: bool) -> Element<'_, Message> {
  let account = accounts::view(state);
  let trigger = identity_card(state, &account, compact);
  let content = popover_content(state, &account);
  popover(
    trigger,
    content,
    state.shell.account_popover_open,
    PopoverOptions {
      placement: Placement::Above,
      alignment: PopoverAlignment::Start,
      width: Some(POPOVER_WIDTH),
      ..PopoverOptions::default()
    },
    Message::Shell(ShellMessage::DismissAccountPopover),
  )
}

/// The Account Settings category reuses the same saved-login and lifecycle UI.
pub fn management(state: &State) -> Element<'_, Message> {
  let account = accounts::view(state);
  popover_content(state, &account)
}

/// Adds an opaque, focus-contained full-window layer for account confirmation
/// and new-account authentication. Hiding the presentation never cancels an
/// in-flight candidate or handoff; the account reducer owns that work.
pub fn modal_layer(state: &State) -> Option<Element<'_, Message>> {
  let account = accounts::view(state);
  if let Some(confirmation) = account.confirmation {
    return Some(full_window_modal(
      state,
      confirmation_modal(state, confirmation),
    ));
  }
  if let Some(candidate) = account.add_account {
    return Some(full_window_modal(
      state,
      add_account_modal(state, candidate),
    ));
  }
  None
}

fn identity_card<'a>(
  state: &'a State,
  account: &AccountView<'a>,
  compact: bool,
) -> Element<'a, Message> {
  let (name, server, provider) = account.current.as_ref().map_or_else(
    || ("Account".to_owned(), "Not signed in".to_owned(), None),
    |current| {
      (
        current.user_name.to_owned(),
        current.server_name.unwrap_or(current.server_url).to_owned(),
        Some(provider_name(current.provider)),
      )
    },
  );
  let connected = state.kernel.connection == ConnectionPhase::Connected;
  if compact {
    return container(
      control_button_content(
        move |_| avatar(&name, connected, 36.0, AvatarShape::RoundedSquare, false),
        ButtonVariant::Text,
      )
      .id(ACCOUNT_TRIGGER_ID)
      .padding(0)
      .width(Fill)
      .min_height(40.0)
      .on_press(Message::Shell(ShellMessage::ToggleAccountPopover)),
    )
    .width(Fill)
    .height(40.0)
    .center_x(Fill)
    .center_y(Fill)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised))
    .into();
  }

  let metadata = state.palette().text.metadata;
  let subtitle = provider.map_or_else(
    || server.clone(),
    |provider| format!("{provider} · {server}"),
  );
  let full_identity = format!("{name} · {subtitle}");
  container(tooltip(
    control_button_content(
      move |_| {
        row![
          avatar(&name, connected, 36.0, AvatarShape::RoundedSquare, false),
          column![
            row![
              text(name.clone())
                .font(SPACE_GROTESK_FONT)
                .size(15)
                .wrapping(Wrapping::None)
                .width(Fill),
              column![
                icon_with_color(Icon::ChevronUp, IconSize::Xs, metadata),
                icon_with_color(Icon::ChevronDown, IconSize::Xs, metadata),
              ]
              .spacing(0)
              .align_x(Alignment::Center),
            ]
            .spacing(TOKENS.spacing.s0_5)
            .align_y(Alignment::Center),
            text(subtitle.clone())
              .size(11)
              .color(metadata)
              .wrapping(Wrapping::None)
              .width(Fill),
          ]
          .spacing(TOKENS.spacing.s0_5)
          .width(Fill),
        ]
        .spacing(TOKENS.spacing.s2)
        .align_y(Alignment::Center)
        .into()
      },
      ButtonVariant::Text,
    )
    .id(ACCOUNT_TRIGGER_ID)
    .padding([6, 8])
    .width(Fill)
    .min_height(52.0)
    .on_press(Message::Shell(ShellMessage::ToggleAccountPopover)),
    full_identity,
    TooltipOptions::default(),
  ))
  .width(Fill)
  .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised))
  .into()
}

fn popover_content<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  let palette = state.palette();
  let mut content = Column::new().spacing(TOKENS.spacing.s3).width(Fill);
  if let Some(current) = &account.current {
    let server = current.server_name.unwrap_or(current.server_url);
    let copy_label = match account.copy_status {
      CopyStatus::Idle => "Copy address",
      CopyStatus::Copied => "Copied",
      CopyStatus::Failed => "Retry copy",
    };
    content = content.push(
      column![
        row![
          avatar(
            current.user_name,
            state.kernel.connection == ConnectionPhase::Connected,
            36.0,
            AvatarShape::Circle,
            false,
          ),
          column![
            row![
              text(current.user_name)
                .font(SPACE_GROTESK_FONT)
                .size(16)
                .color(palette.text.heading),
              provider_badge(
                provider_name(current.provider),
                IconControlState::Rest,
                palette,
              ),
            ]
            .spacing(TOKENS.spacing.s1)
            .align_y(Alignment::Center),
            text(server).size(12).color(palette.text.metadata),
          ]
          .spacing(TOKENS.spacing.s0_5)
          .width(Fill),
          connection_badge(state),
        ]
        .spacing(TOKENS.spacing.s2)
        .align_y(Alignment::Center),
        container(
          row![
            text(current.server_url)
              .size(12)
              .color(palette.text.body)
              .width(Fill),
            control_button(
              Some(Icon::Copy),
              Some(copy_label.to_owned()),
              ButtonVariant::Text
            )
            .icon_size(IconSize::Xs)
            .spacing(TOKENS.spacing.s1)
            .padding([5, 7])
            .on_press(Message::Account(accounts::Message::CopyServerAddress)),
          ]
          .spacing(TOKENS.spacing.s1)
          .align_y(Alignment::Center),
        )
        .padding([6, 8])
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Block)),
        remote_status(account.remote_control),
      ]
      .spacing(TOKENS.spacing.s2),
    );
  }

  if let Some(error) = account.error {
    content = content.push(
      row![
        text(error).size(12).color(palette.colors.error).width(Fill),
        control_button(Some(Icon::Close), None, ButtonVariant::Text)
          .padding([4, 6])
          .on_press(Message::Account(accounts::Message::DismissError)),
      ]
      .align_y(Alignment::Center),
    );
  }
  if account.can_retry_handoff_cleanup {
    content = content.push(
      control_button(
        Some(Icon::Refresh),
        Some("Retry account handoff cleanup".to_owned()),
        ButtonVariant::Primary,
      )
      .icon_size(IconSize::Xs)
      .spacing(TOKENS.spacing.s1_5)
      .padding([6, 10])
      .on_press(Message::Account(accounts::Message::RetryHandoffCleanup)),
    );
  }

  content = content.push(profile_header(account));
  content = content.push(saved_profiles(state, account));
  content = content.push(auto_login(account.auto_login));
  if account.can_retry_watchlist_cleanup {
    content = content.push(
      control_button(
        Some(Icon::Refresh),
        Some("Retry Watchlist cleanup".to_owned()),
        ButtonVariant::Tonal,
      )
      .icon_size(IconSize::Xs)
      .spacing(TOKENS.spacing.s1_5)
      .padding([6, 10])
      .on_press(Message::Account(accounts::Message::RetryWatchlistCleanup)),
    );
  }

  let add_label = if account.handoff_blocking {
    "Switching account…"
  } else {
    "Add account"
  };
  content = content.push(
    control_button(
      Some(Icon::UserCheck),
      Some(add_label.to_owned()),
      ButtonVariant::Tonal,
    )
    .id(ACCOUNT_ADD_TRIGGER_ID)
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([8, 12])
    .width(Fill)
    .content_centered(true)
    .on_press_maybe(
      (!account.handoff_blocking).then_some(Message::Account(accounts::Message::AddAccount)),
    ),
  );
  let active_sign_out = account
    .active_key
    .map(|key| Message::Account(accounts::Message::AskSignOut(key.clone())));
  content = content.push(
    row![
      control_button(
        Some(Icon::Close),
        Some("Disconnect".to_owned()),
        ButtonVariant::Tonal,
      )
      .id(ACCOUNT_DISCONNECT_TRIGGER_ID)
      .icon_size(IconSize::Xs)
      .spacing(TOKENS.spacing.s1)
      .padding([7, 8])
      .width(Fill)
      .content_centered(true)
      .on_press_maybe(
        account
          .current
          .is_some()
          .then_some(Message::Account(accounts::Message::Disconnect)),
      ),
      control_button_content(
        move |status| {
          let color = control_content_color(status, palette.colors.error, palette.colors.error);
          row![
            space::horizontal(),
            icon_with_color(Icon::Trash, IconSize::Xs, color),
            text("Sign Out").size(14).color(color),
            space::horizontal(),
          ]
          .spacing(TOKENS.spacing.s1)
          .align_y(Alignment::Center)
          .into()
        },
        ButtonVariant::Text,
      )
      .padding([7, 8])
      .width(Fill)
      .min_height(36.0)
      .on_press_maybe(active_sign_out),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  );
  scrollable(content)
    .height(Length::Fixed(POPOVER_CONTENT_HEIGHT))
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn connection_badge(state: &State) -> Element<'static, Message> {
  let variant = match state.kernel.connection {
    ConnectionPhase::Connected => BadgeVariant::Success,
    ConnectionPhase::Connecting => BadgeVariant::Warning,
    ConnectionPhase::SignedOut | ConnectionPhase::Failed => BadgeVariant::Neutral,
  };
  let label = match state.kernel.connection {
    ConnectionPhase::Connected => "Connected",
    ConnectionPhase::Connecting => "Connecting",
    ConnectionPhase::SignedOut => "Signed out",
    ConnectionPhase::Failed => "Connection failed",
  };
  status_badge(label, variant)
}

fn remote_status(state: RemoteControlState) -> Element<'static, Message> {
  let (label, variant) = match state {
    RemoteControlState::Available => ("Remote control: available", BadgeVariant::Success),
    RemoteControlState::Connecting => ("Remote control: connecting", BadgeVariant::Warning),
    RemoteControlState::Lost => ("Remote control: connection lost", BadgeVariant::Warning),
    RemoteControlState::Unavailable => ("Remote control: unavailable", BadgeVariant::Neutral),
  };
  status_badge(label, variant)
}

fn status_badge(label: &'static str, variant: BadgeVariant) -> Element<'static, Message> {
  container(text(label).size(11))
    .padding([3, 6])
    .style(move |theme| jellypilot_ui::theme::badge_variant(theme, variant))
    .into()
}

fn provider_badge(
  label: &'static str,
  status: IconControlState,
  palette: &'static ThemePalette,
) -> Element<'static, Message> {
  let disabled = status == IconControlState::Disabled;
  let text_color = control_content_color(status, palette.text.metadata, palette.text.secondary);
  container(text(label).size(10).color(text_color))
    .padding([2, 5])
    .style(move |_| container::Style {
      background: Some(Background::Color(with_disabled_alpha(
        palette.colors.surfaceContainerHigh,
        disabled,
      ))),
      text_color: Some(text_color),
      border: Border {
        radius: TOKENS.radii.md.into(),
        color: Color::TRANSPARENT,
        width: 0.0,
      },
      ..container::Style::default()
    })
    .into()
}

fn control_content_color(status: IconControlState, rest: Color, hovered: Color) -> Color {
  match status {
    IconControlState::Rest => rest,
    IconControlState::Hovered => hovered,
    IconControlState::Disabled => with_disabled_alpha(rest, true),
  }
}

fn with_disabled_alpha(color: Color, disabled: bool) -> Color {
  if disabled {
    Color {
      a: color.a * 0.5,
      ..color
    }
  } else {
    color
  }
}

fn avatar<'a>(
  label: &str,
  connected: bool,
  size: f32,
  shape: AvatarShape,
  disabled: bool,
) -> Element<'a, Message> {
  let initial = label
    .chars()
    .next()
    .map(|character| character.to_uppercase().to_string())
    .unwrap_or_else(|| "?".to_owned());
  let base = container(text(initial).font(SPACE_GROTESK_FONT).size(size * 0.42))
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .center_x(Fill)
    .center_y(Fill)
    .style(move |theme| {
      let colors = jellypilot_ui::tokens::palette(theme).colors;
      container::Style {
        background: Some(Background::Color(with_disabled_alpha(
          colors.primary,
          disabled,
        ))),
        text_color: Some(with_disabled_alpha(colors.onPrimary, disabled)),
        border: Border {
          radius: match shape {
            AvatarShape::Circle => TOKENS.radii.full,
            AvatarShape::RoundedSquare => TOKENS.radii.md,
          }
          .into(),
          color: Color::TRANSPARENT,
          width: 0.0,
        },
        ..container::Style::default()
      }
    });
  if !connected {
    return base.into();
  }
  let dot_size = (size * 0.28).max(8.0);
  stack![
    base,
    container(
      container(space::horizontal())
        .width(Length::Fixed(dot_size))
        .height(Length::Fixed(dot_size))
        .style(move |theme| {
          let colors = jellypilot_ui::tokens::palette(theme).colors;
          container::Style {
            background: Some(Background::Color(with_disabled_alpha(
              theme.palette().success,
              disabled,
            ))),
            border: Border {
              radius: TOKENS.radii.full.into(),
              color: colors.surfaceContainerHigh,
              width: 2.0,
            },
            ..container::Style::default()
          }
        }),
    )
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .align_x(iced::alignment::Horizontal::Right)
    .align_y(iced::alignment::Vertical::Bottom),
  ]
  .width(Length::Fixed(size))
  .height(Length::Fixed(size))
  .into()
}

fn profile_header<'a>(account: &AccountView<'a>) -> Element<'a, Message> {
  let availability = if account.loading {
    "Loading…".to_owned()
  } else {
    format!("{} available", account.profiles.len())
  };
  row![
    column![
      text("Switch server / account")
        .font(SPACE_GROTESK_FONT)
        .size(14),
      text(availability).size(11),
    ]
    .spacing(TOKENS.spacing.s0_5)
    .width(Fill),
    control_button(
      Some(Icon::Sliders),
      Some(
        if account.management_open {
          "Done"
        } else {
          "Manage"
        }
        .to_owned(),
      ),
      ButtonVariant::Text,
    )
    .icon_size(IconSize::Xs)
    .spacing(TOKENS.spacing.s1)
    .padding([5, 6])
    .on_press_maybe(
      (!account.handoff_blocking).then_some(Message::Account(accounts::Message::ToggleManagement)),
    ),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn saved_profiles<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  if account.loading {
    return text("Loading saved accounts…").size(12).into();
  }
  if account.profiles.is_empty() {
    return text("No saved accounts yet.").size(12).into();
  }
  let mut profiles = Column::new().spacing(TOKENS.spacing.s1);
  let palette = state.palette();
  let connected = state.kernel.connection == ConnectionPhase::Connected;
  for (index, profile) in account.profiles.iter().enumerate() {
    let active = account.active_key == Some(profile.key());
    let busy = account.busy_key == Some(profile.key());
    let action = if account.management_open {
      Message::Account(accounts::Message::AskSignOut(profile.key().clone()))
    } else {
      Message::Account(accounts::Message::SwitchProfile(profile.key().clone()))
    };
    let profile_title = profile.title();
    let profile_server = profile.server_url().to_owned();
    let provider = profile.provider();
    let management_open = account.management_open;
    profiles = profiles.push(
      control_button_content(
        move |status| {
          let disabled = status == IconControlState::Disabled;
          let title_color = control_content_color(status, palette.text.body, palette.text.heading);
          let metadata_color =
            control_content_color(status, palette.text.metadata, palette.text.secondary);
          let indicator_color =
            control_content_color(status, palette.text.metadata, palette.text.heading);
          let indicator: Element<'_, Message> = if management_open {
            icon_with_color(Icon::Trash, IconSize::Xs, indicator_color).into()
          } else if active {
            icon_with_color(Icon::Check, IconSize::Sm, indicator_color).into()
          } else {
            icon_with_color(Icon::ChevronRight, IconSize::Xs, indicator_color).into()
          };
          row![
            avatar(
              &profile_title,
              active && connected,
              28.0,
              AvatarShape::Circle,
              disabled,
            ),
            column![
              row![
                text(if busy {
                  "Working…".to_owned()
                } else {
                  profile_title.clone()
                })
                .font(SPACE_GROTESK_FONT)
                .size(13)
                .color(title_color)
                .width(Fill),
                provider_badge(provider_name(provider), status, palette),
              ]
              .spacing(TOKENS.spacing.s1)
              .align_y(Alignment::Center),
              text(profile_server.clone())
                .size(11)
                .color(metadata_color)
                .width(Fill),
            ]
            .spacing(TOKENS.spacing.s0_5)
            .width(Fill),
            indicator,
          ]
          .spacing(TOKENS.spacing.s2)
          .align_y(Alignment::Center)
          .into()
        },
        if active && !account.management_open {
          ButtonVariant::TonalActive
        } else {
          ButtonVariant::Text
        },
      )
      .id(profile_action_id(
        index,
        if account.management_open {
          "signout"
        } else {
          "switch"
        },
      ))
      .padding([7, 8])
      .width(Fill)
      .min_height(44.0)
      .on_press_maybe((!busy && !(active && !account.management_open)).then_some(action)),
    );
  }
  scrollable(profiles)
    .height(Length::Fixed(PROFILE_LIST_HEIGHT))
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn auto_login(auto_login: bool) -> Element<'static, Message> {
  row![
    column![
      text("Automatic sign-in").font(SPACE_GROTESK_FONT).size(13),
      text("Reconnect to the last-used account at startup")
        .size(11)
        .width(Fill),
    ]
    .spacing(TOKENS.spacing.s0_5)
    .width(Fill),
    control_button(
      None,
      Some(if auto_login { "On" } else { "Off" }.to_owned()),
      if auto_login {
        ButtonVariant::TonalActive
      } else {
        ButtonVariant::Tonal
      },
    )
    .padding([5, 8])
    .on_press(Message::Settings(SettingsMessage::AutoLoginToggled)),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn full_window_modal<'a>(state: &'a State, content: Element<'a, Message>) -> Element<'a, Message> {
  container(
    container(content)
      .max_width(640.0)
      .width(Fill)
      .padding(TOKENS.spacing.s5)
      .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised)),
  )
  .width(Fill)
  .height(Fill)
  .center_x(Fill)
  .center_y(Fill)
  .padding(TOKENS.spacing.s4)
  .style(move |_theme| container::Style {
    background: Some(iced::Background::Color(state.palette().colors.surface)),
    ..container::Style::default()
  })
  .into()
}

fn confirmation_modal<'a>(
  state: &'a State,
  confirmation: accounts::ConfirmationView<'a>,
) -> Element<'a, Message> {
  let palette = state.palette();
  let (title, detail) = match confirmation.kind {
    ConfirmationKind::SwitchAccount => (
      "Switch account",
      format!(
        "Switch to {}? The current playback session will end.",
        confirmation.account.unwrap_or("this account")
      ),
    ),
    ConfirmationKind::ConnectAndSwitch => (
      "Connect and switch",
      "The current playback session will end before this account is adopted.".to_owned(),
    ),
    ConfirmationKind::Disconnect => (
      "Disconnect",
      "End the current connection and playback session?".to_owned(),
    ),
    ConfirmationKind::SignOut => (
      "Sign Out",
      format!(
        "Remove {} from this device{}?",
        confirmation.account.unwrap_or("this account"),
        if confirmation.active_profile {
          " and end the current session"
        } else {
          ""
        }
      ),
    ),
  };
  let mut body = column![
    text(title)
      .font(SPACE_GROTESK_FONT)
      .size(24)
      .color(palette.text.heading),
    text(detail).size(14).color(palette.text.body),
  ]
  .spacing(TOKENS.spacing.s3);
  if confirmation.kind == ConfirmationKind::SignOut {
    body = body.push(
      row![
        text("Also delete this account's local Watchlist")
          .size(13)
          .width(Fill),
        control_button(
          None,
          Some(
            if confirmation.delete_watchlist {
              "On"
            } else {
              "Off"
            }
            .to_owned()
          ),
          if confirmation.delete_watchlist {
            ButtonVariant::TonalActive
          } else {
            ButtonVariant::Tonal
          },
        )
        .padding([5, 8])
        .on_press(Message::Account(accounts::Message::ToggleDeleteWatchlist)),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
    );
  }
  body = body.push(
    row![
      control_button(
        Some(Icon::Close),
        Some("Cancel".to_owned()),
        ButtonVariant::Tonal
      )
      .icon_size(IconSize::Sm)
      .spacing(TOKENS.spacing.s1_5)
      .padding([7, 12])
      .on_press(Message::Account(accounts::Message::CancelConfirmation)),
      space::horizontal(),
      control_button(
        Some(Icon::Check),
        Some("Confirm".to_owned()),
        ButtonVariant::Primary
      )
      .icon_size(IconSize::Sm)
      .spacing(TOKENS.spacing.s1_5)
      .padding([7, 12])
      .on_press(Message::Account(accounts::Message::Confirm)),
    ]
    .align_y(Alignment::Center),
  );
  body.into()
}

fn add_account_modal<'a>(
  state: &'a State,
  candidate: &'a CandidateSurface,
) -> Element<'a, Message> {
  let palette = state.palette();
  let flow = &candidate.flow;
  let provider = row![
    candidate_button("Jellyfin", MediaServerProvider::Jellyfin, flow.provider),
    candidate_button("Emby", MediaServerProvider::Emby, flow.provider),
  ]
  .spacing(TOKENS.spacing.s2);
  let server = text_input("https://media.example.com", &flow.server_url)
    .on_input(|value| account_message(CandidateMessage::ServerUrlChanged(value)))
    .padding([8, 12])
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let method: Element<'_, Message> = if flow.provider == MediaServerProvider::Jellyfin {
    row![
      candidate_method("Quick Connect", LoginMethod::QuickConnect, flow.method),
      candidate_method("Password", LoginMethod::Password, flow.method),
    ]
    .spacing(TOKENS.spacing.s2)
    .into()
  } else {
    text("Emby uses password sign-in.").size(13).into()
  };
  let sign_in = candidate_sign_in(candidate);
  let mut form = column![
    row![
      column![
        text("Add account")
          .font(SPACE_GROTESK_FONT)
          .size(24)
          .color(palette.text.heading),
        text("Authenticate first, then connect and switch.")
          .size(13)
          .color(palette.text.metadata),
      ]
      .spacing(TOKENS.spacing.s0_5)
      .width(Fill),
      control_button(Some(Icon::Close), None, ButtonVariant::Tonal)
        .padding([5, 8])
        .on_press(Message::Account(accounts::Message::CloseAddAccount)),
    ]
    .align_y(Alignment::Center),
    provider,
    text("Server URL").size(12).color(palette.text.metadata),
    server,
    method,
    sign_in,
  ]
  .spacing(TOKENS.spacing.s3);
  if let Some(error) = &flow.error {
    form = form.push(text(error).size(13).color(palette.colors.error));
  }
  scrollable(form)
    .height(Fill)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn candidate_button<'a>(
  label: &'a str,
  provider: MediaServerProvider,
  selected: MediaServerProvider,
) -> Element<'a, Message> {
  control_button(
    Some(Icon::Server),
    Some(label.to_owned()),
    if selected == provider {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Tonal
    },
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(account_message(CandidateMessage::ProviderSelected(
    provider,
  )))
  .into()
}

fn candidate_method<'a>(
  label: &'a str,
  method: LoginMethod,
  selected: LoginMethod,
) -> Element<'a, Message> {
  control_button(
    Some(if method == LoginMethod::QuickConnect {
      Icon::QrCode
    } else {
      Icon::Lock
    }),
    Some(label.to_owned()),
    if method == selected {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    },
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press(account_message(CandidateMessage::MethodSelected(method)))
  .into()
}

fn candidate_sign_in<'a>(candidate: &'a CandidateSurface) -> Element<'a, Message> {
  let flow = &candidate.flow;
  match flow.method {
    LoginMethod::QuickConnect => match &flow.quick_connect {
      QuickConnectState::Idle | QuickConnectState::Failed => control_button(
        Some(Icon::QrCode),
        Some("Request Quick Connect code".to_owned()),
        ButtonVariant::Primary,
      )
      .spacing(TOKENS.spacing.s2)
      .padding([8, 14])
      .on_press_maybe(
        (!candidate.busy()).then_some(account_message(CandidateMessage::QuickConnectSubmitted)),
      )
      .into(),
      QuickConnectState::Requesting => text("Requesting a code…").size(13).into(),
      QuickConnectState::Waiting(code) => column![
        text(code).font(SPACE_GROTESK_FONT).size(30),
        text("Approve this code in your Jellyfin dashboard.").size(13),
        control_button(
          Some(Icon::Close),
          Some("Cancel".to_owned()),
          ButtonVariant::Tonal
        )
        .icon_size(IconSize::Xs)
        .spacing(TOKENS.spacing.s1)
        .padding([6, 10])
        .on_press(account_message(CandidateMessage::QuickConnectCancelled)),
      ]
      .spacing(TOKENS.spacing.s2)
      .into(),
      QuickConnectState::Approving => text("Approval received. Signing in…").size(13).into(),
    },
    LoginMethod::Password => {
      let username = text_input("Username", &flow.username)
        .on_input(|value| account_message(CandidateMessage::UsernameChanged(value)))
        .padding([8, 12])
        .style(|theme, status| {
          jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
        });
      let password = text_input("Password", &flow.password)
        .on_input(|value| account_message(CandidateMessage::PasswordChanged(value)))
        .secure(true)
        .on_submit(account_message(CandidateMessage::PasswordSubmitted))
        .padding([8, 12])
        .style(|theme, status| {
          jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
        });
      let remember = control_button(
        None,
        Some(
          if flow.remember {
            "Remember login inputs: On"
          } else {
            "Remember login inputs: Off"
          }
          .to_owned(),
        ),
        if flow.remember {
          ButtonVariant::TonalActive
        } else {
          ButtonVariant::Tonal
        },
      )
      .padding([6, 10])
      .on_press(account_message(CandidateMessage::RememberToggled));
      let submit = control_button(
        Some(Icon::UserCheck),
        Some(
          if candidate.password_busy {
            "Signing in…"
          } else {
            "Connect and switch"
          }
          .to_owned(),
        ),
        ButtonVariant::Primary,
      )
      .spacing(TOKENS.spacing.s2)
      .padding([8, 14])
      .on_press_maybe(
        (!candidate.busy()).then_some(account_message(CandidateMessage::PasswordSubmitted)),
      );
      column![username, password, remember, submit]
        .spacing(TOKENS.spacing.s2)
        .into()
    }
  }
}

fn account_message(message: CandidateMessage) -> Message {
  Message::Account(accounts::Message::AddLogin(message))
}

const fn provider_name(provider: MediaServerProvider) -> &'static str {
  match provider {
    MediaServerProvider::Jellyfin => "Jellyfin",
    MediaServerProvider::Emby => "Emby",
  }
}
