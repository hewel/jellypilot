//! Shared account presentation for the sidebar popover and Settings.

use iced::widget::{
  column, container, image, modal, row, scrollable, space, text, text_input, Column,
};
use iced::{Alignment, Background, Border, Color, Element, Fill, Length};
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_media_server::MediaServerProvider;
use jellypilot_session::RemoteControlState;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT, MONO_FONT};
use jellypilot_ui::icons::{icon_with_color, Icon, IconControlState, IconSize};
use jellypilot_ui::overlay::{
  focus_tooltip, popover, Alignment as PopoverAlignment, Placement, PopoverAppearance,
  PopoverOptions, TooltipOptions,
};
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::{control_button, control_button_content};
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::rounded_image::{full_radius, rounded_image};
use jellypilot_ui::widgets::settings as settings_style;
use jellypilot_ui::widgets::sidebar;
use jellypilot_ui::widgets::switch::switch;

use crate::app::accounts::{self, AccountView, ConfirmationKind, CopyStatus};
use crate::app::login::{CandidateMessage, CandidateSurface};
use crate::app::message::{Message, SettingsMessage, ShellMessage};
use crate::app::shell::{
  profile_action_id, ACCOUNT_ADD_TRIGGER_ID, ACCOUNT_DISCONNECT_TRIGGER_ID, ACCOUNT_TRIGGER_ID,
};
use crate::app::state::{LoginMethod, QuickConnectState, State};
use crate::i18n::{Localizer, UiText};

const POPOVER_WIDTH: f32 = 320.0;
const PROFILE_LIST_HEIGHT: f32 = 192.0;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Presentation {
  Sidebar,
  Settings,
}

impl Presentation {
  fn action_style(
    self,
  ) -> fn(&iced::Theme, ButtonVariant, iced::widget::button::Status) -> iced::widget::button::Style
  {
    match self {
      Self::Sidebar => sidebar::action,
      Self::Settings => {
        |theme, variant, status| jellypilot_ui::theme::button_variant(theme, status, variant)
      }
    }
  }

  fn text<'a>(
    self,
    content: impl iced::widget::text::IntoFragment<'a>,
    size: f32,
    color: Color,
    font: Option<iced::Font>,
    settings_width: Length,
  ) -> Element<'a, Message> {
    match self {
      Self::Sidebar => {
        let mut label = ellipsis_text(content).size(size).color(color);
        if let Some(font) = font {
          label = label.font(font);
        }
        container(label).width(Fill).into()
      }
      Self::Settings => {
        let mut label = text(content)
          .size(size)
          .color(color)
          .width(settings_width)
          .wrapping(iced::widget::text::Wrapping::WordOrGlyph);
        if let Some(font) = font {
          label = label.font(font);
        }
        label.into()
      }
    }
  }
}

fn account_tooltip<'a>(
  trigger: impl Into<Element<'a, Message>>,
  content: String,
  presentation: Presentation,
) -> Element<'a, Message> {
  match presentation {
    Presentation::Sidebar => focus_tooltip(trigger, content, TooltipOptions::default()),
    Presentation::Settings => trigger.into(),
  }
}

/// The sidebar identity card and its account-scoped, anchored popover.
pub fn sidebar_popover(state: &State, compact: bool) -> Element<'_, Message> {
  let account = accounts::view(state);
  let trigger = identity_card(state, &account, compact);
  let content = quick_menu(state, &account);
  popover(
    trigger,
    content,
    state.shell.account_popover_open,
    PopoverOptions {
      placement: Placement::Above,
      alignment: PopoverAlignment::Start,
      width: Some(POPOVER_WIDTH),
      appearance: PopoverAppearance::Account,
      ..PopoverOptions::default()
    },
    Message::Shell(ShellMessage::DismissAccountPopover),
  )
}

/// The Account Settings category reuses the same saved-login and lifecycle UI.
pub fn management(state: &State) -> Element<'_, Message> {
  let account = accounts::view(state);
  management_content(state, &account)
}

/// Whether the account modal logically shields the base layer right now.
///
/// The shield follows the open state, not the exit animation: once dismissed,
/// the retained presentation keeps drawing but stops capturing input.
pub fn modal_open(state: &State) -> bool {
  accounts::blocking_modal(&state.accounts)
}

/// Whether this message can close the account modal. Snapshot capture keys off
/// this so the retained exit presentation is only built for real close-capable
/// actions, not per-keystroke candidate updates or unrelated settlements.
pub(crate) fn may_close_modal(message: &Message) -> bool {
  matches!(
    message,
    Message::Account(
      accounts::Message::CancelConfirmation
        | accounts::Message::CloseAddAccount
        | accounts::Message::Confirm
    )
  )
}

/// Snapshots the currently open account modal for its exit presentation.
///
/// The snapshot is sanitized: the password field reduces to its length so the
/// retained dialog can mask it without keeping the secret.
pub(crate) fn retained_modal_snapshot(state: &State) -> Option<super::motion::RetainedModal> {
  let account = accounts::view(state);
  if let Some(confirmation) = account.confirmation {
    return Some(super::motion::RetainedModal::Confirmation {
      kind: confirmation.kind,
      account: confirmation.account.map(str::to_owned),
      delete_watchlist: confirmation.delete_watchlist,
      active_profile: confirmation.active_profile,
    });
  }
  account.add_account.map(|candidate| {
    super::motion::RetainedModal::AddAccount(super::motion::RetainedAddAccount {
      provider: candidate.flow.provider,
      method: candidate.flow.method,
      server_url: candidate.flow.server_url.clone(),
      username: candidate.flow.username.clone(),
      password_len: candidate.flow.password.len(),
      remember: candidate.flow.remember,
      quick_connect: candidate.flow.quick_connect.clone(),
      busy: candidate.busy(),
      error: candidate.flow.error.clone(),
    })
  })
}

/// Adds a blurred-backdrop, focus-contained full-window layer for account
/// confirmation and new-account authentication. Hiding the presentation never cancels an
/// in-flight candidate or handoff; the account reducer owns that work.
///
/// The layer stays mounted so `motion::reveal` can animate the exit: while the
/// modal is logically closed it draws the retained snapshot captured at
/// dismissal, then settles to an empty layout.
pub fn modal_layer(state: &State) -> Element<'_, Message> {
  let account = accounts::view(state);
  let content: Element<'_, Message> = if let Some(confirmation) = account.confirmation {
    full_window_modal(
      state,
      confirmation_modal(state, confirmation),
      Message::Account(accounts::Message::CancelConfirmation),
    )
  } else if let Some(candidate) = account.add_account {
    full_window_modal(
      state,
      add_account_modal(state, AddAccountView::live(candidate)),
      Message::Account(accounts::Message::CloseAddAccount),
    )
  } else {
    match state.motion.retained_modal.as_ref() {
      Some(super::motion::RetainedModal::Confirmation {
        kind,
        account,
        delete_watchlist,
        active_profile,
      }) => full_window_modal(
        state,
        confirmation_modal(
          state,
          accounts::ConfirmationView {
            kind: *kind,
            account: account.as_deref(),
            delete_watchlist: *delete_watchlist,
            active_profile: *active_profile,
          },
        ),
        Message::Account(accounts::Message::CancelConfirmation),
      ),
      Some(super::motion::RetainedModal::AddAccount(retained)) => full_window_modal(
        state,
        add_account_modal(state, AddAccountView::retained(retained)),
        Message::Account(accounts::Message::CloseAddAccount),
      ),
      None => space().into(),
    }
  };
  super::motion::reveal(content, modal_open(state))
}

fn identity_card<'a>(
  state: &'a State,
  account: &AccountView<'a>,
  compact: bool,
) -> Element<'a, Message> {
  let (name, server, provider) = account.current.as_ref().map_or_else(
    || {
      (
        state.t("account-title"),
        state.t("account-not-signed-in"),
        None,
      )
    },
    |current| {
      (
        current.user_name.to_owned(),
        current.server_name.unwrap_or(current.server_url).to_owned(),
        Some(provider_name(current.provider)),
      )
    },
  );
  let photo = account
    .active_key
    .and_then(|key| state.kernel.profile_avatars.get(key))
    .cloned();
  let identity = if account.current.is_some() {
    format!("{name}@{server}")
  } else {
    name.clone()
  };
  let subtitle = provider.map_or_else(
    || server.clone(),
    |provider| format!("{provider} · {server}"),
  );
  let full_identity = format!("{name} · {subtitle}");
  if compact {
    return focus_tooltip(
      control_button_content(
        move |_| {
          container(avatar(
            &identity,
            photo.clone(),
            28.0,
            TOKENS.radii.lg,
            false,
          ))
          .center_x(Fill)
          .into()
        },
        ButtonVariant::Text,
      )
      .style(sidebar::identity)
      .id(ACCOUNT_TRIGGER_ID)
      .padding([8, 0])
      .width(Fill)
      .min_height(44.0)
      .on_press(Message::Shell(ShellMessage::ToggleAccountPopover)),
      full_identity,
      TooltipOptions::default(),
    );
  }

  let metadata = state.palette().text.metadata;
  focus_tooltip(
    control_button_content(
      move |_| {
        let subtitle_line =
          container(ellipsis_text(subtitle.clone()).size(10).color(metadata)).width(Fill);
        row![
          avatar(&identity, photo.clone(), 36.0, TOKENS.radii.lg, false),
          column![
            container(ellipsis_text(name.clone()).font(HEADING_FONT).size(12)).width(Fill),
            subtitle_line,
          ]
          .spacing(TOKENS.spacing.s0_5)
          .width(Fill),
          icon_with_color(Icon::ChevronUp, IconSize::Xs, metadata),
        ]
        .spacing(TOKENS.spacing.s2_5)
        .align_y(Alignment::Center)
        .into()
      },
      ButtonVariant::Text,
    )
    .style(sidebar::identity)
    .id(ACCOUNT_TRIGGER_ID)
    .padding([8, 10])
    .width(Fill)
    .min_height(54.0)
    .on_press(Message::Shell(ShellMessage::ToggleAccountPopover)),
    full_identity,
    TooltipOptions::default(),
  )
}

fn quick_menu<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  let palette = state.palette();
  let mut content = Column::new().spacing(TOKENS.spacing.s2_5).width(Fill);
  if let Some(current) = &account.current {
    let server = current.server_name.unwrap_or(current.server_url);
    let provider = provider_name(current.provider);
    let (copy_icon, copy_hint) = match account.copy_status {
      CopyStatus::Idle => (Icon::Copy, state.t("account-copy-server-address")),
      CopyStatus::Copied => (Icon::Check, state.t("account-address-copied")),
      CopyStatus::Failed => (Icon::Warning, state.t("account-copy-failed")),
    };
    content = content.push(focus_tooltip(
      container(
        row![
          icon_with_color(Icon::Server, IconSize::Xs, palette.text.metadata),
          Presentation::Sidebar.text(current.server_url, 12.0, palette.text.metadata, None, Fill),
          control_button(Some(copy_icon), None, ButtonVariant::Icon)
            .id("account-address-copy")
            .style(sidebar::menu_action)
            .icon_size(IconSize::Xs)
            .padding([0, 0])
            .width(Length::Fixed(40.0))
            .min_height(40.0)
            .content_centered(true)
            .on_press(Message::Account(accounts::Message::CopyServerAddress)),
          connection_badge(state),
        ]
        .spacing(TOKENS.spacing.s2)
        .align_y(Alignment::Center),
      )
      .padding([1, 10])
      .style(sidebar::address),
      format!(
        "{copy_hint}\n{} · {provider} · {server}\n{}",
        current.user_name, current.server_url
      ),
      TooltipOptions::default(),
    ));
  }
  content = content.extend(account_feedback(state, account, Presentation::Sidebar));
  if account.loading
    || account
      .profiles
      .iter()
      .any(|profile| account.current.is_none() || account.active_key != Some(profile.key()))
  {
    content = content.push(
      column![
        container(
          text(if account.current.is_some() {
            state.t("account-switch")
          } else {
            state.t("account-saved")
          })
          .size(12)
          .color(palette.text.metadata),
        )
        .padding(iced::Padding {
          top: 0.0,
          right: TOKENS.spacing.s2_5,
          bottom: TOKENS.spacing.s1_5,
          left: TOKENS.spacing.s2_5,
        }),
        saved_profiles(state, account, Presentation::Sidebar),
      ]
      .spacing(TOKENS.spacing.s0_5),
    );
  }
  let mut actions = column![
    menu_action(
      Icon::User,
      &if account.handoff_blocking {
        state.t("account-switching")
      } else {
        state.t("account-add")
      }
    )
    .id(ACCOUNT_ADD_TRIGGER_ID)
    .on_press_maybe(
      (!account.handoff_blocking).then_some(Message::Account(accounts::Message::AddAccount))
    ),
    menu_action(Icon::Settings, &state.t("account-manage-accounts"))
      .id("account-settings")
      .on_press(Message::Settings(SettingsMessage::OpenAccounts)),
  ]
  .spacing(TOKENS.spacing.s0_5);
  if account.current.is_some() {
    actions = actions.push(
      menu_action(Icon::Close, &state.t("account-disconnect"))
        .id(ACCOUNT_DISCONNECT_TRIGGER_ID)
        .on_press_maybe(
          (!account.handoff_blocking).then_some(Message::Account(accounts::Message::Disconnect)),
        ),
    );
  }
  content = content.push(
    column![
      container(space::horizontal())
        .width(Fill)
        .height(1.0)
        .style(sidebar::divider),
      actions,
    ]
    .spacing(TOKENS.spacing.s2_5),
  );
  scrollable(content)
    .height(Length::Fit)
    .style(jellypilot_ui::theme::scrollable)
    .into()
}

fn menu_action(icon: Icon, label: &str) -> jellypilot_ui::ControlButton<'static, Message> {
  control_button(Some(icon), Some(label.to_owned()), ButtonVariant::Tonal)
    .style(sidebar::menu_action)
    .icon_size(IconSize::Sm)
    .label_size(12.0)
    .spacing(TOKENS.spacing.s2_5)
    .padding([8, 10])
    .min_height(40.0)
    .width(Fill)
}

fn account_feedback<'a>(
  state: &State,
  account: &AccountView<'a>,
  presentation: Presentation,
) -> Option<Element<'a, Message>> {
  if account.error.is_none()
    && !account.can_retry_handoff_cleanup
    && !account.can_retry_watchlist_cleanup
  {
    return None;
  }
  let mut feedback = Column::new().spacing(TOKENS.spacing.s2);
  if let Some(error) = account.error {
    feedback = feedback.push(
      row![
        text(state.kernel.locale.message(error))
          .size(12)
          .color(state.palette().colors.error)
          .width(Fill),
        control_button(Some(Icon::Close), None, ButtonVariant::Icon)
          .style(presentation.action_style())
          .width(Length::Fixed(40.0))
          .min_height(40.0)
          .content_centered(true)
          .on_press(Message::Account(accounts::Message::DismissError)),
      ]
      .align_y(Alignment::Center),
    );
  }
  if account.can_retry_handoff_cleanup {
    feedback = feedback.push(
      control_button(
        Some(Icon::Refresh),
        Some(state.t("account-retry-handoff")),
        ButtonVariant::Primary,
      )
      .style(presentation.action_style())
      .icon_size(IconSize::Xs)
      .spacing(TOKENS.spacing.s1_5)
      .padding([6, 10])
      .on_press(Message::Account(accounts::Message::RetryHandoffCleanup)),
    );
  }
  if account.can_retry_watchlist_cleanup {
    feedback = feedback.push(
      control_button(
        Some(Icon::Refresh),
        Some(state.t("account-retry-watchlist")),
        ButtonVariant::Tonal,
      )
      .style(presentation.action_style())
      .icon_size(IconSize::Xs)
      .spacing(TOKENS.spacing.s1_5)
      .padding([6, 10])
      .on_press(Message::Account(accounts::Message::RetryWatchlistCleanup)),
    );
  }
  Some(feedback.into())
}

fn management_content<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  let presentation = Presentation::Settings;
  let palette = state.palette();
  let mut content = Column::new().spacing(TOKENS.spacing.s5).width(Fill);
  if let Some(current) = &account.current {
    let server = current.server_name.unwrap_or(current.server_url);
    let photo = account
      .active_key
      .and_then(|key| state.kernel.profile_avatars.get(key))
      .cloned();
    let identity = format!("{}@{}", current.user_name, server);
    let tile = settings_avatar(&identity, photo, 44.0, TOKENS.radii.lg);
    let name_column = column![
      row![
        presentation.text(
          current.user_name,
          TOKENS.font_sizes.s16,
          palette.text.heading,
          Some(HEADING_FONT),
          Fill,
        ),
        presentation.text(
          provider_name(current.provider),
          TOKENS.font_sizes.s12,
          palette.colors.secondary,
          None,
          Length::Fit,
        ),
      ]
      .spacing(TOKENS.spacing.s2)
      .align_y(Alignment::Center),
      presentation.text(
        server,
        TOKENS.font_sizes.s12,
        palette.text.metadata,
        None,
        Fill,
      ),
    ]
    .spacing(TOKENS.spacing.s0_5)
    .width(Fill);
    let badges = row![
      connection_badge(state),
      remote_status(state.kernel.locale, account.remote_control),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center);
    let card = row![tile, name_column, badges]
      .spacing(TOKENS.spacing.s3)
      .align_y(Alignment::Center);
    content = content.push(
      container(card)
        .width(Fill)
        .padding(TOKENS.spacing.s3_5)
        .style(settings_style::account_identity),
    );

    let copy_label = match account.copy_status {
      CopyStatus::Idle => state.t("account-copy-address"),
      CopyStatus::Copied => state.t("account-copied"),
      CopyStatus::Failed => state.t("account-retry-copy"),
    };
    let copy_icon = match account.copy_status {
      CopyStatus::Idle => Icon::Copy,
      CopyStatus::Copied => Icon::Check,
      CopyStatus::Failed => Icon::Warning,
    };
    content = content.push(
      container(
        row![
          presentation.text(
            current.server_url,
            TOKENS.font_sizes.s12,
            palette.text.body,
            Some(MONO_FONT),
            Fill,
          ),
          account_tooltip(
            control_button(Some(copy_icon), Some(copy_label), ButtonVariant::Tonal)
              .style(presentation.action_style())
              .icon_size(IconSize::Xs)
              .label_size(TOKENS.font_sizes.s12)
              .spacing(TOKENS.spacing.s1_5)
              .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
              .radius(TOKENS.radii.lg)
              .on_press(Message::Account(accounts::Message::CopyServerAddress)),
            format!("{} · {server} · {}", current.user_name, current.server_url),
            presentation,
          ),
        ]
        .spacing(TOKENS.spacing.s2_5)
        .align_y(Alignment::Center),
      )
      .width(Fill)
      .padding([TOKENS.spacing.s2_5, TOKENS.spacing.s3])
      .style(settings_style::account_address),
    );
  }

  content = content.extend(account_feedback(state, account, presentation));

  content = content.push(
    column![
      profile_header(state, account),
      saved_profiles(state, account, presentation),
    ]
    .spacing(TOKENS.spacing.s2)
    .width(Fill),
  );
  content = content.push(auto_login(state, account.auto_login));

  let add_label = if account.handoff_blocking {
    state.t("account-switching")
  } else {
    state.t("account-add")
  };
  let add = control_button(Some(Icon::UserCheck), Some(add_label), ButtonVariant::Tonal)
    .style(presentation.action_style())
    .id(ACCOUNT_ADD_TRIGGER_ID)
    .icon_size(IconSize::Xs)
    .label_size(TOKENS.font_sizes.s12)
    .spacing(TOKENS.spacing.s1_5)
    .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
    .radius(TOKENS.radii.xl)
    .on_press_maybe(
      (!account.handoff_blocking).then_some(Message::Account(accounts::Message::AddAccount)),
    );
  let disconnect = control_button(
    None,
    Some(state.t("account-disconnect")),
    ButtonVariant::Text,
  )
  .style(presentation.action_style())
  .id(ACCOUNT_DISCONNECT_TRIGGER_ID)
  .label_size(TOKENS.font_sizes.s12)
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
  .on_press_maybe(
    account
      .current
      .is_some()
      .then_some(Message::Account(accounts::Message::Disconnect)),
  );
  let active_sign_out = account
    .active_key
    .map(|key| Message::Account(accounts::Message::AskSignOut(key.clone())));
  let sign_out = control_button_content(
    move |status| {
      let color = control_content_color(status, palette.colors.error, palette.colors.error);
      text(state.t("account-sign-out"))
        .size(TOKENS.font_sizes.s12)
        .color(color)
        .into()
    },
    ButtonVariant::Text,
  )
  .style(presentation.action_style())
  .padding([TOKENS.spacing.s2, TOKENS.spacing.s3])
  .on_press_maybe(active_sign_out);
  let footer = row![add, space::horizontal(), disconnect, sign_out]
    .spacing(TOKENS.spacing.s1)
    .align_y(Alignment::Center);
  content.push(footer).into()
}

/// Settings identity tile: the shared avatar plus the Paper `imageOutline`
/// edge when a real photo is present. Fallback initial tiles stay borderless.
fn settings_avatar<'a>(
  identity: &str,
  photo: Option<image::Handle>,
  size: f32,
  radius: f32,
) -> Element<'a, Message> {
  if photo.is_none() {
    return avatar(identity, None, size, radius, false);
  }
  container(avatar(identity, photo, size - 2.0, radius - 1.0, false))
    .padding(1)
    .style(move |theme| settings_style::avatar_outline(theme, radius))
    .into()
}

fn connection_badge(state: &State) -> Element<'static, Message> {
  let variant = match state.kernel.connection {
    ConnectionPhase::Connected => BadgeVariant::Success,
    ConnectionPhase::Connecting => BadgeVariant::Warning,
    ConnectionPhase::SignedOut => BadgeVariant::Neutral,
    ConnectionPhase::Failed => BadgeVariant::Error,
  };
  let label = match state.kernel.connection {
    ConnectionPhase::Connected => "account-connected",
    ConnectionPhase::Connecting => "account-connecting",
    ConnectionPhase::SignedOut => "account-signed-out",
    ConnectionPhase::Failed => "account-connection-failed",
  };
  jellypilot_ui::theme::status_tag(state.t(label), variant)
}

fn remote_status(locale: Localizer, state: RemoteControlState) -> Element<'static, Message> {
  let (label, variant) = match state {
    RemoteControlState::Available => ("account-remote-available", BadgeVariant::Success),
    RemoteControlState::Connecting => ("account-remote-connecting", BadgeVariant::Warning),
    RemoteControlState::Lost => ("account-remote-lost", BadgeVariant::Warning),
    RemoteControlState::Unavailable => ("account-remote-unavailable", BadgeVariant::Neutral),
  };
  jellypilot_ui::theme::status_tag(locale.text(label), variant)
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

/// Account identity tile: the user's server-side photo when loaded, else a
/// local initial-tile fallback tinted by a stable hash of `identity`.
/// `identity` supplies both the initial (first char) and the tint seed, so
/// pass a full `user@server` string to keep same-name accounts distinct.
fn avatar<'a>(
  identity: &str,
  photo: Option<image::Handle>,
  size: f32,
  radius: f32,
  disabled: bool,
) -> Element<'a, Message> {
  match photo {
    Some(handle) => {
      let mut tile = rounded_image(handle, full_radius(radius))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size));
      if disabled {
        tile = tile.opacity(0.5_f32);
      }
      tile.into()
    }
    None => {
      let initial = identity
        .chars()
        .next()
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned());
      let accent = fallback_accent(identity);
      container(text(initial).font(HEADING_FONT).size(size * 0.42))
        .width(Length::Fixed(size))
        .height(Length::Fixed(size))
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(move |theme| {
          let colors = jellypilot_ui::tokens::palette(theme).colors;
          let (fill, ink) = match accent {
            0 => (colors.primary, colors.onPrimary),
            1 => (colors.secondary, colors.onSecondary),
            _ => (colors.tertiary, colors.onTertiary),
          };
          container::Style {
            background: Some(Background::Color(with_disabled_alpha(fill, disabled))),
            text_color: Some(with_disabled_alpha(ink, disabled)),
            border: Border {
              smoothing: jellypilot_ui::widgets::container::SURFACE_SMOOTHING,
              radius: radius.into(),
              color: Color::TRANSPARENT,
              width: 0.0,
            },
            ..container::Style::default()
          }
        })
        .into()
    }
  }
}

/// Stable token-accent index for a fallback tile, so two same-initial
/// accounts read as distinct without a photo. Hashes the identity string
/// (FNV-1a); the palette triplet stays inside the theme (ADR 0027).
fn fallback_accent(label: &str) -> u8 {
  let mut hash: u32 = 0x811c_9dc5;
  for byte in label.as_bytes() {
    hash ^= u32::from(*byte);
    hash = hash.wrapping_mul(0x0100_0193);
  }
  (hash % 3) as u8
}

fn profile_header<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  let locale = state.kernel.locale;
  let palette = state.palette();
  let availability = if account.loading {
    locale.text("common-loading")
  } else {
    locale.format(
      "account-available",
      &[("count", account.profiles.len().into())],
    )
  };
  let manage_label = if account.management_open {
    locale.text("account-done")
  } else {
    locale.text("account-manage")
  };
  row![
    column![
      text(locale.text("account-switch-server"))
        .font(HEADING_FONT)
        .size(TOKENS.font_sizes.s14)
        .color(palette.text.secondary),
      text(availability)
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.metadata),
    ]
    .spacing(TOKENS.spacing.s0_5)
    .width(Fill),
    control_button_content(
      move |status| {
        let color =
          control_content_color(status, palette.colors.secondary, palette.colors.secondary);
        row![
          icon_with_color(Icon::Sliders, IconSize::Xs, color),
          text(manage_label.clone())
            .size(TOKENS.font_sizes.s12)
            .color(color),
        ]
        .spacing(TOKENS.spacing.s1_5)
        .align_y(Alignment::Center)
        .into()
      },
      ButtonVariant::Text,
    )
    .style(Presentation::Settings.action_style())
    .padding([TOKENS.spacing.s1, TOKENS.spacing.s2])
    .on_press_maybe(
      (!account.handoff_blocking).then_some(Message::Account(accounts::Message::ToggleManagement)),
    ),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn saved_profiles<'a>(
  state: &'a State,
  account: &AccountView<'a>,
  presentation: Presentation,
) -> Element<'a, Message> {
  if account.loading {
    return text(state.t("account-loading-saved")).size(12).into();
  }
  if account.profiles.is_empty() {
    return text(state.t("account-no-saved")).size(12).into();
  }
  match presentation {
    Presentation::Sidebar => sidebar_profile_list(state, account),
    Presentation::Settings => settings_profile_list(state, account),
  }
}

/// Sidebar popover rows: only alternatives to the current account, compact
/// `user_name` titles, and the shared menu-action treatment.
fn sidebar_profile_list<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  let presentation = Presentation::Sidebar;
  let palette = state.palette();
  let mut profiles = Column::new().spacing(TOKENS.spacing.s0_5);
  for (index, profile) in account.profiles.iter().enumerate() {
    let active = account.current.is_some() && account.active_key == Some(profile.key());
    if active {
      continue;
    }
    let busy = account.busy_key == Some(profile.key());
    let action = Message::Account(accounts::Message::SwitchProfile(profile.key().clone()));
    let profile_title = profile.user_name().to_owned();
    let profile_server = profile
      .server_name
      .as_deref()
      .unwrap_or(profile.server_url())
      .to_owned();
    let full_identity = format!(
      "{} · {} · {} · {}",
      profile.user_name(),
      provider_name(profile.provider()),
      profile_server,
      profile.server_url()
    );
    let profile_subtitle = format!("{} · {profile_server}", provider_name(profile.provider()));
    let photo = state.kernel.profile_avatars.get(profile.key()).cloned();
    let profile_control = control_button_content(
      move |status| {
        let disabled = status == IconControlState::Disabled;
        let title_color = control_content_color(status, palette.text.body, palette.text.heading);
        let metadata_color =
          control_content_color(status, palette.text.metadata, palette.text.secondary);
        let indicator_color =
          control_content_color(status, palette.text.metadata, palette.text.heading);
        let title = presentation.text(
          if busy {
            state.t("account-working")
          } else {
            profile_title.clone()
          },
          12.0,
          title_color,
          Some(HEADING_FONT),
          Fill,
        );
        row![
          avatar(
            &profile.title(),
            photo.clone(),
            30.0,
            TOKENS.radii.lg,
            disabled
          ),
          column![
            title,
            presentation.text(profile_subtitle.clone(), 12.0, metadata_color, None, Fill),
          ]
          .spacing(TOKENS.spacing.s0_5)
          .width(Fill),
          icon_with_color(Icon::ChevronRight, IconSize::Xs, indicator_color),
        ]
        .spacing(TOKENS.spacing.s2_5)
        .align_y(Alignment::Center)
        .into()
      },
      ButtonVariant::Text,
    )
    .id(profile_action_id(index, "switch"))
    .padding([6, 10])
    .width(Fill)
    .min_height(47.0)
    .on_press_maybe((!busy && !account.handoff_blocking).then_some(action))
    .style(sidebar::menu_action);
    profiles = profiles.push(account_tooltip(
      profile_control,
      full_identity,
      presentation,
    ));
  }
  let profiles = scrollable(profiles)
    .height(Length::Fit)
    .style(jellypilot_ui::theme::scrollable);
  container(profiles)
    .height(Length::Fit.max(PROFILE_LIST_HEIGHT))
    .into()
}

/// Settings rows: the current account stays listed with the Paper
/// `primaryContainer`/`secondary` selected treatment; alternatives keep the
/// quiet row. Manage mode swaps the trailing affordance for the sign-out icon
/// and retargets the press to the confirmation flow.
fn settings_profile_list<'a>(state: &'a State, account: &AccountView<'a>) -> Element<'a, Message> {
  let presentation = Presentation::Settings;
  let palette = state.palette();
  let management_open = account.management_open;
  let mut profiles = Column::new().spacing(TOKENS.spacing.s2);
  for (index, profile) in account.profiles.iter().enumerate() {
    let active = account.current.is_some() && account.active_key == Some(profile.key());
    let busy = account.busy_key == Some(profile.key());
    let selected = active && !management_open;
    let action = if management_open {
      Message::Account(accounts::Message::AskSignOut(profile.key().clone()))
    } else {
      Message::Account(accounts::Message::SwitchProfile(profile.key().clone()))
    };
    let profile_title = profile.title();
    let profile_server = profile.server_url().to_owned();
    let provider = profile.provider();
    let photo = state.kernel.profile_avatars.get(profile.key()).cloned();
    let profile_control = control_button_content(
      move |status| {
        let disabled = status == IconControlState::Disabled && !selected;
        let title_color = if selected {
          palette.text.heading
        } else {
          control_content_color(status, palette.text.secondary, palette.text.heading)
        };
        let metadata_color = if selected {
          palette.text.metadata
        } else {
          control_content_color(status, palette.text.metadata, palette.text.secondary)
        };
        let provider_color = if selected {
          palette.colors.secondary
        } else {
          control_content_color(status, palette.text.metadata, palette.text.secondary)
        };
        let indicator: Element<'_, Message> = if management_open {
          icon_with_color(
            Icon::Trash,
            IconSize::Xs,
            control_content_color(status, palette.colors.error, palette.colors.error),
          )
          .into()
        } else if active {
          icon_with_color(Icon::Check, IconSize::Xs, palette.colors.secondary).into()
        } else {
          icon_with_color(
            Icon::ChevronRight,
            IconSize::Xs,
            control_content_color(status, palette.text.muted, palette.text.secondary),
          )
          .into()
        };
        row![
          avatar(
            &profile.title(),
            photo.clone(),
            30.0,
            TOKENS.radii.lg,
            disabled
          ),
          column![
            presentation.text(
              if busy {
                state.t("account-working")
              } else {
                profile_title.clone()
              },
              TOKENS.font_sizes.s12,
              title_color,
              Some(HEADING_FONT),
              Fill,
            ),
            presentation.text(
              profile_server.clone(),
              TOKENS.font_sizes.s12,
              metadata_color,
              None,
              Fill,
            ),
          ]
          .spacing(TOKENS.spacing.px)
          .width(Fill),
          presentation.text(
            provider_name(provider),
            TOKENS.font_sizes.s12,
            provider_color,
            None,
            Length::Fit,
          ),
          indicator,
        ]
        .spacing(TOKENS.spacing.s2_5)
        .align_y(Alignment::Center)
        .into()
      },
      if selected {
        ButtonVariant::Secondary
      } else {
        ButtonVariant::Text
      },
    )
    .id(profile_action_id(
      index,
      if management_open { "signout" } else { "switch" },
    ))
    .padding([TOKENS.spacing.s2_5, TOKENS.spacing.s3])
    .width(Fill)
    .on_press_maybe(
      (!busy && !account.handoff_blocking && !(active && !management_open)).then_some(action),
    );
    // The selected row is deliberately non-pressable; keep its Paper
    // primary-container treatment instead of the disabled control fill.
    let profile_control = if selected {
      profile_control.style(|theme, variant, _status| {
        settings_style::navigation_button(theme, variant, iced::widget::button::Status::Active)
      })
    } else {
      profile_control.style(settings_style::navigation_button)
    };
    profiles = profiles.push(profile_control);
  }
  profiles.into()
}

fn auto_login(state: &State, auto_login: bool) -> Element<'static, Message> {
  let locale = state.kernel.locale;
  let palette = state.palette();
  row![
    column![
      text(locale.text("account-auto-login"))
        .font(HEADING_FONT)
        .size(TOKENS.font_sizes.s14)
        .color(palette.text.secondary),
      text(locale.text("account-auto-login-description"))
        .size(TOKENS.font_sizes.s12)
        .color(palette.text.body)
        .width(Fill),
    ]
    .spacing(TOKENS.spacing.s0_5)
    .width(Fill),
    switch(auto_login).on_press(Message::Settings(SettingsMessage::AutoLoginToggled)),
  ]
  .spacing(TOKENS.spacing.s2)
  .align_y(Alignment::Center)
  .into()
}

fn full_window_modal<'a>(
  state: &'a State,
  content: Element<'a, Message>,
  dismiss: Message,
) -> Element<'a, Message> {
  modal(
    TOKENS.modal.backdrop_blur_sigma,
    container(super::modal_dismiss::dismissible(
      container(content)
        .width(Length::Fill.max(640.0))
        .padding(TOKENS.spacing.s5)
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Floating)),
      dismiss,
    ))
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .center_y(Fill)
    .padding(TOKENS.spacing.s4)
    .style(move |_theme| container::Style {
      background: Some(iced::Background::Color(
        state.palette().colors.surface.scale_alpha(0.6),
      )),
      ..container::Style::default()
    }),
  )
}

fn confirmation_modal<'a>(
  state: &'a State,
  confirmation: accounts::ConfirmationView<'a>,
) -> Element<'a, Message> {
  let palette = state.palette();
  let locale = state.kernel.locale;
  let account = confirmation
    .account
    .map_or_else(|| state.t("account-this-account"), str::to_owned);
  let (title, detail) = match confirmation.kind {
    ConfirmationKind::SwitchAccount => (
      state.t("account-switch"),
      locale.format("account-confirm-switch", &[("account", account.into())]),
    ),
    ConfirmationKind::ConnectAndSwitch => (
      state.t("account-connect-switch"),
      state.t("account-confirm-connect-switch"),
    ),
    ConfirmationKind::Disconnect => (
      state.t("account-disconnect"),
      state.t("account-confirm-disconnect"),
    ),
    ConfirmationKind::SignOut => (
      state.t("account-sign-out"),
      locale.format(
        if confirmation.active_profile {
          "account-confirm-signout-active"
        } else {
          "account-confirm-signout"
        },
        &[("account", account.into())],
      ),
    ),
  };
  let mut body = column![
    text(title)
      .font(DISPLAY_FONT)
      .size(24)
      .color(palette.text.heading),
    text(detail).size(14).color(palette.text.body),
  ]
  .spacing(TOKENS.spacing.s3);
  if confirmation.kind == ConfirmationKind::SignOut {
    body = body.push(
      row![
        text(state.t("account-delete-watchlist"))
          .size(13)
          .width(Fill),
        switch(confirmation.delete_watchlist)
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
        Some(state.t("common-cancel")),
        ButtonVariant::Tonal
      )
      .icon_size(IconSize::Sm)
      .spacing(TOKENS.spacing.s1_5)
      .padding([7, 12])
      .on_press(Message::Account(accounts::Message::CancelConfirmation)),
      space::horizontal(),
      control_button(
        Some(Icon::Check),
        Some(state.t("account-confirm")),
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

/// Display facts for the add-account dialog. The live path borrows the
/// candidate; the retained exit presentation borrows the sanitized snapshot.
struct AddAccountView<'a> {
  provider: MediaServerProvider,
  method: LoginMethod,
  server_url: &'a str,
  username: &'a str,
  /// The password field's displayed value: the real draft while interactive,
  /// mask bullets while exiting so the secret is never retained.
  password: std::borrow::Cow<'a, str>,
  remember: bool,
  quick_connect: &'a QuickConnectState,
  busy: bool,
  password_busy: bool,
  error: Option<&'a UiText>,
  interactive: bool,
}

impl<'a> AddAccountView<'a> {
  fn live(candidate: &'a CandidateSurface) -> Self {
    Self {
      provider: candidate.flow.provider,
      method: candidate.flow.method,
      server_url: &candidate.flow.server_url,
      username: &candidate.flow.username,
      password: std::borrow::Cow::Borrowed(candidate.flow.password.as_str()),
      remember: candidate.flow.remember,
      quick_connect: &candidate.flow.quick_connect,
      busy: candidate.busy(),
      password_busy: candidate.password_busy,
      error: candidate.flow.error.as_ref(),
      interactive: true,
    }
  }

  fn retained(retained: &'a super::motion::RetainedAddAccount) -> Self {
    Self {
      provider: retained.provider,
      method: retained.method,
      server_url: &retained.server_url,
      username: &retained.username,
      password: std::borrow::Cow::Owned("•".repeat(retained.password_len)),
      remember: retained.remember,
      quick_connect: &retained.quick_connect,
      busy: retained.busy,
      password_busy: retained.busy,
      error: retained.error.as_ref(),
      interactive: false,
    }
  }
}

fn add_account_modal<'a>(state: &'a State, view: AddAccountView<'a>) -> Element<'a, Message> {
  let palette = state.palette();
  let interactive = view.interactive;
  let provider = row![
    candidate_button(
      "Jellyfin",
      MediaServerProvider::Jellyfin,
      view.provider,
      interactive
    ),
    candidate_button(
      "Emby",
      MediaServerProvider::Emby,
      view.provider,
      interactive
    ),
  ]
  .spacing(TOKENS.spacing.s2);
  let server = text_input("https://media.example.com", view.server_url)
    .on_input_maybe(
      interactive.then_some(|value| account_message(CandidateMessage::ServerUrlChanged(value))),
    )
    .padding([8, 12])
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let method: Element<'_, Message> = if view.provider == MediaServerProvider::Jellyfin {
    row![
      candidate_method(
        state.t("login-quick-connect"),
        LoginMethod::QuickConnect,
        view.method,
        interactive
      ),
      candidate_method(
        state.t("login-password"),
        LoginMethod::Password,
        view.method,
        interactive
      ),
    ]
    .spacing(TOKENS.spacing.s2)
    .into()
  } else {
    text(state.t("login-emby-password")).size(13).into()
  };
  let sign_in = candidate_sign_in(state.kernel.locale, &view);
  let mut form = column![
    row![
      column![
        text(state.t("account-add"))
          .font(DISPLAY_FONT)
          .size(24)
          .color(palette.text.heading),
        text(state.t("account-add-description"))
          .size(13)
          .color(palette.text.metadata),
      ]
      .spacing(TOKENS.spacing.s0_5)
      .width(Fill),
      control_button(Some(Icon::Close), None, ButtonVariant::Tonal)
        .padding([5, 8])
        .on_press_maybe(
          interactive.then_some(Message::Account(accounts::Message::CloseAddAccount))
        ),
    ]
    .align_y(Alignment::Center),
    provider,
    text(state.t("login-server-url"))
      .size(12)
      .color(palette.text.metadata),
    server,
    method,
    sign_in,
  ]
  .spacing(TOKENS.spacing.s3);
  if let Some(error) = view.error {
    form = form.push(
      text(state.kernel.locale.message(error))
        .size(13)
        .color(palette.colors.error),
    );
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
  interactive: bool,
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
  .on_press_maybe(
    interactive.then_some(account_message(CandidateMessage::ProviderSelected(
      provider,
    ))),
  )
  .into()
}

fn candidate_method<'a>(
  label: String,
  method: LoginMethod,
  selected: LoginMethod,
  interactive: bool,
) -> Element<'a, Message> {
  control_button(
    Some(if method == LoginMethod::QuickConnect {
      Icon::QrCode
    } else {
      Icon::Lock
    }),
    Some(label),
    if method == selected {
      ButtonVariant::Secondary
    } else {
      ButtonVariant::Text
    },
  )
  .icon_size(IconSize::Sm)
  .spacing(TOKENS.spacing.s1_5)
  .padding([6, 10])
  .on_press_maybe(interactive.then_some(account_message(CandidateMessage::MethodSelected(method))))
  .into()
}

fn candidate_sign_in<'a>(locale: Localizer, view: &AddAccountView<'a>) -> Element<'a, Message> {
  let interactive = view.interactive;
  match view.method {
    LoginMethod::QuickConnect => match view.quick_connect {
      QuickConnectState::Idle | QuickConnectState::Failed => control_button(
        Some(Icon::QrCode),
        Some(locale.text("login-request-code")),
        ButtonVariant::Primary,
      )
      .spacing(TOKENS.spacing.s2)
      .padding([8, 14])
      .on_press_maybe(
        (interactive && !view.busy)
          .then_some(account_message(CandidateMessage::QuickConnectSubmitted)),
      )
      .into(),
      QuickConnectState::Requesting => text(locale.text("login-requesting-code")).size(13).into(),
      QuickConnectState::Waiting(code) => column![
        text(code.clone()).font(HEADING_FONT).size(30),
        text(locale.text("account-approve-code")).size(13),
        control_button(
          Some(Icon::Close),
          Some(locale.text("common-cancel")),
          ButtonVariant::Tonal
        )
        .icon_size(IconSize::Xs)
        .spacing(TOKENS.spacing.s1)
        .padding([6, 10])
        .on_press_maybe(
          interactive.then_some(account_message(CandidateMessage::QuickConnectCancelled)),
        ),
      ]
      .spacing(TOKENS.spacing.s2)
      .into(),
      QuickConnectState::Approving => text(locale.text("login-approving")).size(13).into(),
    },
    LoginMethod::Password => {
      let username = text_input(locale.text("login-username"), view.username)
        .on_input_maybe(
          interactive.then_some(|value| account_message(CandidateMessage::UsernameChanged(value))),
        )
        .padding([8, 12])
        .style(|theme, status| {
          jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
        });
      let password = text_input(locale.text("login-password"), view.password.clone())
        .on_input_maybe(
          interactive.then_some(|value| account_message(CandidateMessage::PasswordChanged(value))),
        )
        .secure(true)
        .on_submit_maybe(
          interactive.then_some(account_message(CandidateMessage::PasswordSubmitted)),
        )
        .padding([8, 12])
        .style(|theme, status| {
          jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
        });
      let remember = control_button(
        None,
        Some(if view.remember {
          locale.text("account-remember-on")
        } else {
          locale.text("account-remember-off")
        }),
        if view.remember {
          ButtonVariant::TonalActive
        } else {
          ButtonVariant::Tonal
        },
      )
      .padding([6, 10])
      .on_press_maybe(interactive.then_some(account_message(CandidateMessage::RememberToggled)));
      let submit = control_button(
        None,
        Some(if view.password_busy {
          locale.text("login-signing-in")
        } else {
          locale.text("account-connect-switch")
        }),
        ButtonVariant::Primary,
      )
      .spacing(TOKENS.spacing.s2)
      .padding([8, 14])
      .on_press_maybe(
        (interactive && !view.busy).then_some(account_message(CandidateMessage::PasswordSubmitted)),
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

#[cfg(test)]
mod tests {
  use iced::advanced::{layout, renderer, renderer::Headless, widget::Tree};
  use iced::widget::image;
  use iced::{Font, Size};

  use super::{avatar, TOKENS};

  #[tokio::test]
  async fn profile_avatars_share_one_layout_box_across_states() {
    let renderer = iced::Renderer::new(
      renderer::Settings {
        font: Font::DEFAULT,
        text_size: 14.0.into(),
        line_height: jellypilot_ui::fonts::DEFAULT_LINE_HEIGHT,
        metrics_hinting: false,
      },
      Some("tiny-skia"),
    )
    .await
    .expect("software layout renderer");
    let mut sizes = Vec::new();
    for photo in [None, Some(image::Handle::from_rgba(2, 2, vec![0; 16]))] {
      let mut avatar = avatar(
        "Long profile name@server",
        photo,
        28.0,
        TOKENS.radii.md,
        false,
      );
      let mut tree = Tree::new(&avatar);
      tree.diff(avatar.as_widget_mut());
      let node = avatar.as_widget_mut().layout(
        &mut tree,
        &renderer,
        &layout::Limits::new(Size::ZERO, Size::new(336.0, 600.0)),
      );
      sizes.push(node.size());
    }
    let [fallback, photo] = [sizes[0], sizes[1]];
    // A photo arriving must not shift the row: both states occupy one box,
    // exactly the tile's square.
    assert_eq!(
      fallback, photo,
      "photo and fallback tiles must share the layout box"
    );
    assert_eq!(
      fallback,
      Size::new(28.0, 28.0),
      "avatar box stays tile-sized"
    );
  }
}
