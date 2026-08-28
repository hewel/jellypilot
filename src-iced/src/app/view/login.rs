use iced::widget::{button, column, container, row, scrollable, text, text_input, Column};
use iced::{Alignment, Element, Fill, Length};
use jellypilot_auth::login::{can_start_login, ConnectionPhase};
use jellypilot_media_server::MediaServerProvider;
use jellypilot_ui::fonts::SPACE_GROTESK_FONT;
use jellypilot_ui::tokens::TOKENS;
use jellypilot_ui::variants::{BadgeVariant, ButtonVariant, FieldVariant, SurfaceVariant};

use crate::app::message::{LoginMessage, Message};
use crate::app::state::{LoginMethod, QuickConnectState, State};

pub fn view(state: &State) -> Element<'_, Message> {
  let login = &state.login;
  let title = text("Sign in to JellyPilot")
    .font(SPACE_GROTESK_FONT)
    .size(38)
    .color(TOKENS.colors.onSurface);
  let subtitle = text("Connect directly to your own media server.")
    .size(16)
    .color(TOKENS.colors.onSurfaceVariant);

  let provider_row = row![
    provider_button("Jellyfin", MediaServerProvider::Jellyfin, state),
    provider_button("Emby", MediaServerProvider::Emby, state),
  ]
  .spacing(10);

  let server_field = text_input("https://media.example.com", &login.server_url)
    .on_input(|value| Message::Login(LoginMessage::ServerUrlChanged(value)))
    .padding(14)
    .size(16)
    .style(move |theme, status| {
      if login.error.is_some() && login.server_url.trim().is_empty() {
        jellypilot_ui::theme::error_field_variant(theme, status, FieldVariant::Filled)
      } else {
        jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
      }
    });
  let fields = column![
    text("Server URL").color(TOKENS.colors.onSurfaceVariant),
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
      .color(TOKENS.colors.onSurfaceVariant)
      .into()
  };

  let method: Element<'_, Message> = match login.method {
    LoginMethod::QuickConnect => quick_connect(state),
    LoginMethod::Password => password(state),
  };

  let mut form = column![title, subtitle, provider_row, fields, method_tabs, method]
    .spacing(18)
    .width(Fill);
  if let Some(error) = &login.error {
    form = form.push(text(error).size(15).color(TOKENS.colors.error));
  }
  if login.profiles_loading {
    form = form.push(text("Loading saved sign-ins…").color(TOKENS.colors.onSurfaceVariant));
  } else if !login.profiles.is_empty() {
    form = form.push(saved_profiles(state));
  }

  let card = container(form)
    .width(Length::Fixed(640.0))
    .padding(36)
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Elevated));
  container(scrollable(card).style(jellypilot_ui::theme::scrollable))
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .padding([48, 24])
    .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled))
    .into()
}

fn provider_button<'a>(
  label: &'a str,
  provider: MediaServerProvider,
  state: &'a State,
) -> Element<'a, Message> {
  let selected = state.login.provider == provider;
  button(text(label))
    .padding([10, 18])
    .on_press(Message::Login(LoginMessage::ProviderSelected(provider)))
    .style(move |theme, status| {
      jellypilot_ui::theme::button_variant(
        theme,
        status,
        if selected {
          ButtonVariant::Primary
        } else {
          ButtonVariant::Outlined
        },
      )
    })
    .into()
}

fn method_button<'a>(
  label: &'a str,
  method: LoginMethod,
  state: &'a State,
) -> Element<'a, Message> {
  let selected = state.login.method == method;
  button(text(label))
    .padding([10, 18])
    .on_press(Message::Login(LoginMessage::MethodSelected(method)))
    .style(move |theme, status| {
      jellypilot_ui::theme::button_variant(
        theme,
        status,
        if selected {
          ButtonVariant::Secondary
        } else {
          ButtonVariant::Text
        },
      )
    })
    .into()
}

fn quick_connect(state: &State) -> Element<'_, Message> {
  let login = &state.login;
  let content: Element<'_, Message> = match &login.quick_connect {
    QuickConnectState::Idle | QuickConnectState::Failed => {
      let label = if matches!(login.quick_connect, QuickConnectState::Failed) {
        "Request a new code"
      } else {
        "Request Quick Connect code"
      };
      let request = button(text(label))
        .padding([12, 20])
        .style(|theme, status| {
          jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
        });
      if can_start_login(state.connection) {
        request
          .on_press(Message::Login(LoginMessage::QuickConnectSubmitted))
          .into()
      } else {
        request.into()
      }
    }
    QuickConnectState::Requesting => quick_connect_progress("Requesting a code…", None),
    QuickConnectState::Waiting(code) => {
      let code_badge = container(
        text(code)
          .font(SPACE_GROTESK_FONT)
          .size(32)
          .color(TOKENS.colors.onSurface),
      )
      .padding([12, 20])
      .style(|theme| jellypilot_ui::theme::badge_variant(theme, BadgeVariant::Neutral));
      column![
        text("Enter this code in your Jellyfin dashboard, then approve JellyPilot.")
          .color(TOKENS.colors.onSurfaceVariant),
        code_badge,
        cancel_button(),
      ]
      .align_x(Alignment::Start)
      .spacing(14)
      .into()
    }
    QuickConnectState::Approving => quick_connect_progress("Approval received. Signing in…", None),
  };

  column![
    text("Quick Connect avoids sending your password to this app.")
      .color(TOKENS.colors.onSurfaceVariant),
    content,
  ]
  .spacing(14)
  .into()
}

fn quick_connect_progress<'a>(label: &'a str, _code: Option<&'a str>) -> Element<'a, Message> {
  column![
    container(text(label).color(TOKENS.colors.onSurface))
      .padding([8, 12])
      .style(|theme| jellypilot_ui::theme::badge_variant(theme, BadgeVariant::Warning)),
    cancel_button(),
  ]
  .spacing(12)
  .into()
}

fn cancel_button<'a>() -> Element<'a, Message> {
  button(text("Cancel"))
    .padding([10, 16])
    .on_press(Message::Login(LoginMessage::QuickConnectCancelled))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Outlined)
    })
    .into()
}

fn password(state: &State) -> Element<'_, Message> {
  let login = &state.login;
  let username = text_input("Username", &login.username)
    .on_input(|value| Message::Login(LoginMessage::UsernameChanged(value)))
    .padding(14)
    .size(16)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let password = text_input("Password", &login.password)
    .on_input(|value| Message::Login(LoginMessage::PasswordChanged(value)))
    .secure(true)
    .padding(14)
    .size(16)
    .style(|theme, status| {
      jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
    });
  let password = if can_start_login(state.connection) {
    password.on_submit(Message::Login(LoginMessage::PasswordSubmitted))
  } else {
    password
  };
  let remember_label = if login.remember {
    "Remember server and username: On"
  } else {
    "Remember server and username: Off"
  };
  let remember = button(text(remember_label))
    .padding([10, 14])
    .on_press(Message::Login(LoginMessage::RememberToggled))
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
    });
  let submit = button(text(if state.connection == ConnectionPhase::Connecting {
    "Signing in…"
  } else {
    "Sign in"
  }))
  .padding([12, 20])
  .style(|theme, status| {
    jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Primary)
  });
  let submit = if can_start_login(state.connection) {
    submit.on_press(Message::Login(LoginMessage::PasswordSubmitted))
  } else {
    submit
  };

  column![username, password, remember, submit]
    .spacing(12)
    .into()
}

fn saved_profiles(state: &State) -> Element<'_, Message> {
  let mut profiles = Column::new().spacing(12).push(
    text("Saved sign-ins")
      .size(20)
      .color(TOKENS.colors.onSurface),
  );
  for profile in &state.login.profiles {
    let key = profile.key().clone();
    let is_busy = state.login.busy_profile.as_ref() == Some(&key);
    let restore = button(text(if is_busy {
      "Checking saved sign-in…".to_owned()
    } else {
      profile.title()
    }))
    .padding([10, 14])
    .style(|theme, status| {
      jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Secondary)
    });
    let restore = if is_busy {
      restore
    } else {
      restore.on_press(Message::Login(LoginMessage::RestoreProfile(key.clone())))
    };
    let forget = button(text("Forget"))
      .padding([10, 14])
      .style(|theme, status| {
        jellypilot_ui::theme::button_variant(theme, status, ButtonVariant::Text)
      });
    let forget = if state.login.busy_profile.is_none() {
      forget.on_press(Message::Login(LoginMessage::AskForgetProfile(key.clone())))
    } else {
      forget
    };
    let mut profile_content = column![
      row![restore, forget].spacing(10).align_y(Alignment::Center),
      text(profile.subtitle()).color(TOKENS.colors.onSurfaceVariant),
    ]
    .spacing(8);
    if state.login.busy_profile.is_none() && state.login.forget_confirmation.as_ref() == Some(&key)
    {
      profile_content = profile_content.push(
        column![
          text(profile.forget_confirmation()).color(TOKENS.colors.warning),
          row![
            button(text("Keep sign-in"))
              .padding([8, 12])
              .on_press(Message::Login(LoginMessage::CancelForgetProfile))
              .style(|theme, status| jellypilot_ui::theme::button_variant(
                theme,
                status,
                ButtonVariant::Outlined,
              )),
            button(text("Forget sign-in"))
              .padding([8, 12])
              .on_press(Message::Login(LoginMessage::ConfirmForgetProfile(
                key.clone(),
              )))
              .style(|theme, status| jellypilot_ui::theme::button_variant(
                theme,
                status,
                ButtonVariant::Primary,
              )),
          ]
          .spacing(10),
        ]
        .spacing(10),
      );
    }
    profiles = profiles.push(
      container(profile_content)
        .padding(16)
        .style(|theme| jellypilot_ui::theme::surface_variant(theme, SurfaceVariant::Filled)),
    );
  }
  profiles.into()
}
