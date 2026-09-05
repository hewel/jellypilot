use crate::app::message::{LoginMessage, Message};
use crate::app::shell::profile_action_id;
use crate::app::state::{LoginMethod, QuickConnectState, State};
use iced::widget::{column, container, row, scrollable, text, text_input, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_auth::login::{can_start_login, ConnectionPhase};
use jellypilot_media_server::MediaServerProvider;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::icons::{icon_with_color, Icon, IconSize};
use jellypilot_ui::tokens::{ThemePalette, TOKENS};
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant};
use jellypilot_ui::widgets::control_button::control_button;

pub fn view(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let login = &state.login.flow;
  let title = text("Sign in to JellyPilot")
    .font(SPACE_GROTESK_FONT)
    .size(32)
    .color(palette.text.heading);
  let subtitle = text("Connect directly to your own media server.")
    .size(14)
    .color(palette.text.secondary);

  let provider_row = row![
    provider_button("Jellyfin", MediaServerProvider::Jellyfin, state),
    provider_button("Emby", MediaServerProvider::Emby, state),
  ]
  .spacing(10);

  let server_field = text_input("https://media.example.com", &login.server_url)
    .on_input(|value| Message::Login(LoginMessage::ServerUrlChanged(value)))
    .padding([8, 12])
    .size(14)
    .style(move |theme, status| {
      if login.error.is_some() && login.server_url.trim().is_empty() {
        jellypilot_ui::theme::error_field_variant(theme, status, FieldVariant::Filled)
      } else {
        jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
      }
    });
  let fields = column![
    text("Server URL").color(palette.text.metadata),
    server_field,
  ]
  .spacing(8);

  let method_tabs: Element<'_, Message> = if login.provider == MediaServerProvider::Jellyfin {
    row![
      method_button("Quick Connect", LoginMethod::QuickConnect, state),
      method_button("Password", LoginMethod::Password, state),
    ]
    .spacing(10)
    .into()
  } else {
    text("Emby uses password sign-in.")
      .color(palette.text.body)
      .into()
  };

  let method: Element<'_, Message> = match login.method {
    LoginMethod::QuickConnect => quick_connect(state),
    LoginMethod::Password => password(state),
  };

  let mut form = column![title, subtitle, provider_row, fields, method_tabs, method]
    .spacing(14)
    .width(Fill);
  if let Some(error) = &login.error {
    form = form.push(text(error).size(15).color(palette.colors.error));
  }
  if login.profiles_loading {
    form = form.push(text("Loading saved sign-ins…").color(palette.text.muted));
  } else if !login.profiles.is_empty() {
    form = form.push(saved_profiles(state));
  }

  let card = container(form)
    .width(Length::Fixed(640.0))
    .padding(28)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Raised));
  container(scrollable(card).style(jellypilot_ui::theme::scrollable))
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .padding([36, 24])
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas))
    .into()
}

fn provider_button<'a>(
  label: &'a str,
  provider: MediaServerProvider,
  state: &'a State,
) -> Element<'a, Message> {
  let selected = state.login.flow.provider == provider;
  let variant = if selected {
    ButtonVariant::Primary
  } else {
    ButtonVariant::Tonal
  };
  control_button(Some(Icon::Server), Some(label.to_owned()), variant)
    .spacing(TOKENS.spacing.s2)
    .padding([7, 14])
    .on_press(Message::Login(LoginMessage::ProviderSelected(provider)))
    .into()
}

fn method_button<'a>(
  label: &'a str,
  method: LoginMethod,
  state: &'a State,
) -> Element<'a, Message> {
  let selected = state.login.flow.method == method;
  let icon = match method {
    LoginMethod::QuickConnect => Icon::QrCode,
    LoginMethod::Password => Icon::Lock,
  };
  let variant = if selected {
    ButtonVariant::Secondary
  } else {
    ButtonVariant::Text
  };
  control_button(Some(icon), Some(label.to_owned()), variant)
    .icon_size(IconSize::Sm)
    .spacing(TOKENS.spacing.s1_5)
    .padding([7, 14])
    .on_press(Message::Login(LoginMessage::MethodSelected(method)))
    .into()
}

fn quick_connect(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let login = &state.login.flow;
  let content: Element<'_, Message> = match &login.quick_connect {
    QuickConnectState::Idle | QuickConnectState::Failed => {
      let label = if matches!(login.quick_connect, QuickConnectState::Failed) {
        "Request a new code"
      } else {
        "Request Quick Connect code"
      };
      let can_login = can_start_login(state.kernel.connection);
      control_button(
        Some(Icon::QrCode),
        Some(label.to_owned()),
        ButtonVariant::Primary,
      )
      .spacing(TOKENS.spacing.s2)
      .padding([8, 16])
      .on_press_maybe(can_login.then_some(Message::Login(LoginMessage::QuickConnectSubmitted)))
      .into()
    }
    QuickConnectState::Requesting => quick_connect_progress(palette, "Requesting a code…", None),
    QuickConnectState::Waiting(code) => {
      let code_badge = container(
        row![
          icon_with_color(Icon::QrCode, IconSize::X2l, palette.colors.primary),
          text(code)
            .font(SPACE_GROTESK_FONT)
            .size(32)
            .color(palette.text.heading),
        ]
        .spacing(TOKENS.spacing.s3)
        .align_y(Alignment::Center),
      )
      .padding([12, 20])
      .style(|theme| jellypilot_ui::theme::badge_variant(theme, BadgeVariant::Neutral));
      column![
        text("Enter this code in your Jellyfin dashboard, then approve JellyPilot.")
          .color(palette.text.body),
        code_badge,
        cancel_button(),
      ]
      .align_x(Alignment::Start)
      .spacing(14)
      .into()
    }
    QuickConnectState::Approving => {
      quick_connect_progress(palette, "Approval received. Signing in…", None)
    }
  };

  column![
    text("Quick Connect avoids sending your password to this app.").color(palette.text.body),
    content,
  ]
  .spacing(14)
  .into()
}

fn quick_connect_progress<'a>(
  palette: &ThemePalette,
  label: &'a str,
  _code: Option<&'a str>,
) -> Element<'a, Message> {
  column![
    container(text(label).color(palette.colors.onSurface))
      .padding([8, 12])
      .style(|theme| jellypilot_ui::theme::badge_variant(theme, BadgeVariant::Warning)),
    cancel_button(),
  ]
  .spacing(12)
  .into()
}

fn cancel_button<'a>() -> Element<'a, Message> {
  control_button(
    Some(Icon::Close),
    Some("Cancel".to_owned()),
    ButtonVariant::Tonal,
  )
  .icon_size(IconSize::Xs)
  .spacing(TOKENS.spacing.s1_5)
  .padding([7, 12])
  .on_press(Message::Login(LoginMessage::QuickConnectCancelled))
  .into()
}

fn password(state: &State) -> Element<'_, Message> {
  let login = &state.login.flow;
  let username = text_input("Username", &login.username)
    .on_input(|value| Message::Login(LoginMessage::UsernameChanged(value)))
    .padding([8, 12])
    .size(14)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let password = text_input("Password", &login.password)
    .on_input(|value| Message::Login(LoginMessage::PasswordChanged(value)))
    .secure(true)
    .padding([8, 12])
    .size(14)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let password = if can_start_login(state.kernel.connection) {
    password.on_submit(Message::Login(LoginMessage::PasswordSubmitted))
  } else {
    password
  };
  let remember_label = if login.remember {
    "Remember server and username: On"
  } else {
    "Remember server and username: Off"
  };
  let remember = control_button(None, Some(remember_label.to_owned()), ButtonVariant::Text)
    .padding([6, 12])
    .on_press(Message::Login(LoginMessage::RememberToggled));
  let can_login = can_start_login(state.kernel.connection);
  let submit = control_button(
    Some(Icon::UserCheck),
    Some(
      if state.kernel.connection == ConnectionPhase::Connecting {
        "Signing in…"
      } else {
        "Sign in"
      }
      .to_owned(),
    ),
    ButtonVariant::Primary,
  )
  .spacing(TOKENS.spacing.s2)
  .padding([8, 16])
  .on_press_maybe(can_login.then_some(Message::Login(LoginMessage::PasswordSubmitted)));

  column![username, password, remember, submit]
    .spacing(12)
    .into()
}

fn saved_profiles(state: &State) -> Element<'_, Message> {
  let palette = state.palette();
  let mut profiles = Column::new().spacing(12).push(
    row![
      icon_with_color(Icon::User, IconSize::Lg, palette.colors.primary),
      text("Saved sign-ins").size(18).color(palette.text.heading),
    ]
    .spacing(TOKENS.spacing.s2)
    .align_y(Alignment::Center),
  );
  for (index, profile) in state.login.flow.profiles.iter().enumerate() {
    let key = profile.key().clone();
    let is_busy = state.login.flow.busy_profile.as_ref() == Some(&key);
    let restore_label = if is_busy {
      "Checking saved sign-in…".to_owned()
    } else {
      profile.title()
    };
    let restore = control_button(Some(Icon::User), Some(restore_label), ButtonVariant::Tonal)
      .icon_size(IconSize::Sm)
      .id(profile_action_id(index, "switch"))
      .spacing(TOKENS.spacing.s2)
      .padding([6, 12])
      .on_press_maybe(
        (!is_busy).then_some(Message::Login(LoginMessage::RestoreProfile(key.clone()))),
      );
    let sign_out = control_button(
      Some(Icon::Trash),
      Some("Sign Out".to_owned()),
      ButtonVariant::Text,
    )
    .icon_size(IconSize::Xs)
    .id(profile_action_id(index, "signout"))
    .spacing(TOKENS.spacing.s1)
    .padding([6, 12])
    .on_press_maybe(
      state
        .login
        .flow
        .busy_profile
        .is_none()
        .then_some(Message::Account(crate::app::accounts::Message::AskSignOut(
          key.clone(),
        ))),
    );
    let profile_content = column![
      row![restore, sign_out]
        .spacing(10)
        .align_y(Alignment::Center),
      text(profile.subtitle()).color(palette.text.metadata),
    ]
    .spacing(8);
    profiles = profiles.push(
      container(profile_content)
        .padding(16)
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Canvas)),
    );
  }
  profiles.into()
}
