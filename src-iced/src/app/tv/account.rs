//! TV account presentation. Authentication, persistence and handoff remain in their reducers.

use iced::widget::{button, column, container, opaque, responsive, row, scrollable, space, text};
use iced::{Element, Fill, Task};
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::config::UiMode;
use jellypilot_core::tv_navigation::Input;
use jellypilot_media_server::MediaServerProvider;
use jellypilot_ui::fonts::{DISPLAY_FONT, HEADING_FONT};
use jellypilot_ui::tv as style;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::tv_focus::focus;

use crate::app::accounts::{self, ConfirmationKind};
use crate::app::login::{self, CandidateMessage};
use crate::app::message::{LoginMessage, Message as AppMessage};
use crate::app::state::{LoginMethod, LoginState, QuickConnectState, State};

use super::text_entry;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Field {
  Server,
  Username,
  Password,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Control {
  #[default]
  Server,
  Username,
  Password,
  Jellyfin,
  Emby,
  QuickConnect,
  PasswordMethod,
  Remember,
  ShowPassword,
  Submit,
  CancelQuickConnect,
  Close,
  Profile(usize),
  Remove(usize),
  Confirm,
  Cancel,
  DeleteWatchlist,
  ExitTv,
}

#[derive(Clone, PartialEq, Eq)]
enum Context {
  Login,
  Add,
  Confirmation(ConfirmationKind, Option<String>),
}

#[derive(Default)]
pub struct Surface {
  context: Option<Context>,
  focus: Control,
  form_focus: Control,
  editing: Option<Field>,
  keyboard: text_entry::Surface,
  show_password: bool,
  epoch: u64,
  offset: f32,
}

#[derive(Clone)]
pub enum Message {
  Activate(Control),
  Entry(text_entry::Message),
  Scrolled(f32),
  Revealed {
    epoch: u64,
    control: Control,
    offset: f32,
  },
}

fn message(message: Message) -> AppMessage {
  AppMessage::Tv(super::Message::Account(message))
}

pub fn modal_open(state: &State) -> bool {
  accounts::blocking_modal(&state.accounts)
}

fn context(state: &State) -> Option<Context> {
  let account = accounts::view(state);
  if let Some(confirmation) = account.confirmation {
    Some(Context::Confirmation(
      confirmation.kind,
      confirmation.account.map(str::to_owned),
    ))
  } else if account.add_account.is_some() {
    Some(Context::Add)
  } else if state.kernel.connection != ConnectionPhase::Connected {
    Some(Context::Login)
  } else {
    None
  }
}

fn flow(state: &State) -> &LoginState {
  state
    .accounts
    .add_account
    .as_ref()
    .map_or(&state.login.flow, |candidate| &candidate.flow)
}

fn busy(state: &State) -> bool {
  state.accounts.add_account.as_ref().map_or_else(
    || !login::can_start_authentication(&state.login, &state.kernel),
    |candidate| candidate.busy() || accounts::view(state).handoff_blocking,
  )
}

fn quick_connect_live(state: &State) -> bool {
  matches!(
    flow(state).quick_connect,
    QuickConnectState::Requesting | QuickConnectState::Waiting(_) | QuickConnectState::Approving
  )
}

fn controls(state: &State) -> Vec<Vec<Control>> {
  if let Some(confirmation) = accounts::view(state).confirmation {
    let mut rows = vec![vec![Control::Cancel, Control::Confirm]];
    if confirmation.kind == ConfirmationKind::SignOut {
      rows.insert(0, vec![Control::DeleteWatchlist]);
    }
    return rows;
  }
  let mut rows = Vec::new();
  if !busy(state) {
    rows.extend([
      vec![Control::Jellyfin, Control::Emby],
      vec![Control::Server],
    ]);
    if flow(state).provider == MediaServerProvider::Jellyfin {
      rows.push(vec![Control::QuickConnect, Control::PasswordMethod]);
    }
    if flow(state).method == LoginMethod::Password {
      rows.extend([
        vec![Control::Username, Control::Password],
        vec![Control::Remember, Control::ShowPassword],
      ]);
    }
    rows.push(vec![Control::Submit]);
  } else if quick_connect_live(state) {
    rows.push(vec![Control::CancelQuickConnect]);
  }
  if state.accounts.add_account.is_some() {
    rows.push(vec![Control::Close]);
  } else {
    if !busy(state) {
      rows.extend(
        state
          .login
          .flow
          .profiles
          .iter()
          .enumerate()
          .map(|(index, _)| vec![Control::Profile(index), Control::Remove(index)]),
      );
    }
    rows.push(vec![Control::ExitTv]);
  }
  rows
}

pub fn reconcile(state: &mut State) -> Task<AppMessage> {
  let next = context(state);
  if !state.tv_mode() || next.is_none() {
    if state.tv.account.context.is_some() {
      state.tv.account = Surface::default();
      return text_entry::unfocus();
    }
    return Task::none();
  }
  let previous_focus = state.tv.account.focus;
  let changed_context = state.tv.account.context != next;
  if changed_context {
    let previous_form = matches!(
      state.tv.account.context,
      Some(Context::Add | Context::Login)
    );
    if previous_form {
      state.tv.account.form_focus = state.tv.account.focus;
    }
    state.tv.account.focus = if matches!(next, Some(Context::Confirmation(..))) {
      Control::Cancel
    } else if matches!(state.tv.account.context, Some(Context::Confirmation(..))) {
      state.tv.account.form_focus
    } else {
      Control::Server
    };
    state.tv.account.context = next;
    state.tv.account.editing = None;
    state.tv.account.show_password = false;
    state.tv.account.offset = 0.0;
    state.tv.account.epoch = state.tv.account.epoch.wrapping_add(1);
  }
  let rows = controls(state);
  if !rows
    .iter()
    .flatten()
    .any(|control| *control == state.tv.account.focus)
  {
    state.tv.account.focus = rows
      .first()
      .and_then(|row| row.first())
      .copied()
      .unwrap_or(Control::Close);
  }
  if busy(state) {
    state.tv.account.editing = None;
  }
  if changed_context || previous_focus != state.tv.account.focus {
    Task::batch([text_entry::unfocus(), reveal(state)])
  } else {
    Task::none()
  }
}

fn form_message(state: &State, input: CandidateMessage, login: LoginMessage) -> AppMessage {
  if state.accounts.add_account.is_some() {
    AppMessage::Account(accounts::Message::AddLogin(input))
  } else {
    AppMessage::Login(login)
  }
}

fn field_value(state: &State, field: Field) -> &str {
  match field {
    Field::Server => &flow(state).server_url,
    Field::Username => &flow(state).username,
    Field::Password => flow(state).password.as_str(),
  }
}

fn field_changed(state: &State, field: Field, value: String) -> AppMessage {
  match field {
    Field::Server => form_message(
      state,
      CandidateMessage::ServerUrlChanged(value.clone()),
      LoginMessage::ServerUrlChanged(value),
    ),
    Field::Username => form_message(
      state,
      CandidateMessage::UsernameChanged(value.clone()),
      LoginMessage::UsernameChanged(value),
    ),
    Field::Password => form_message(
      state,
      CandidateMessage::PasswordChanged(value.clone()),
      LoginMessage::PasswordChanged(value),
    ),
  }
}

fn apply_entry(
  state: &mut State,
  field: Field,
  action: Option<text_entry::Action>,
  task: Task<text_entry::Message>,
) -> Task<AppMessage> {
  let effect = match action {
    Some(text_entry::Action::Changed(value)) => {
      let event = field_changed(state, field, value);
      crate::app::update::route_message(state, event)
    }
    Some(text_entry::Action::Done | text_entry::Action::Cancel) => {
      state.tv.account.editing = None;
      reveal(state)
    }
    None => Task::none(),
  };
  Task::batch([task.map(|entry| message(Message::Entry(entry))), effect])
}

pub fn input(state: &mut State, input: Input) -> Task<AppMessage> {
  drop(reconcile(state));
  if let Some(field) = state.tv.account.editing {
    let value = zeroize::Zeroizing::new(field_value(state, field).to_owned());
    let (action, task) = text_entry::input(&mut state.tv.account.keyboard, &value, input);
    return apply_entry(state, field, action, task);
  }
  if input == Input::Back {
    let event = if accounts::view(state).confirmation.is_some() {
      AppMessage::Account(accounts::Message::CancelConfirmation)
    } else if state.accounts.add_account.is_some() {
      AppMessage::Account(accounts::Message::CloseAddAccount)
    } else if quick_connect_live(state) {
      AppMessage::Login(LoginMessage::QuickConnectCancelled)
    } else {
      AppMessage::UiModeSelected(UiMode::Desktop)
    };
    return crate::app::update::route_message(state, event);
  }
  if input == Input::Confirm {
    return activate(state, state.tv.account.focus);
  }
  let rows = controls(state);
  let Some((row, column)) = rows.iter().enumerate().find_map(|(row, controls)| {
    controls
      .iter()
      .position(|control| *control == state.tv.account.focus)
      .map(|column| (row, column))
  }) else {
    return Task::none();
  };
  let (next_row, next_column) = match input {
    Input::Up => (row.saturating_sub(1), column),
    Input::Down => ((row + 1).min(rows.len() - 1), column),
    Input::Left => (row, column.saturating_sub(1)),
    Input::Right => (row, (column + 1).min(rows[row].len() - 1)),
    _ => return Task::none(),
  };
  state.tv.account.focus = rows[next_row][next_column.min(rows[next_row].len() - 1)];
  reveal(state)
}

pub fn update(state: &mut State, event: Message) -> Task<AppMessage> {
  if !state.tv_mode() || context(state).is_none() {
    return Task::none();
  }
  match event {
    Message::Activate(control) => activate(state, control),
    Message::Entry(entry) => {
      let Some(field) = state.tv.account.editing else {
        return Task::none();
      };
      if busy(state) {
        return Task::none();
      }
      let value = zeroize::Zeroizing::new(field_value(state, field).to_owned());
      let (action, task) = text_entry::update(&mut state.tv.account.keyboard, &value, entry);
      apply_entry(state, field, action, task)
    }
    Message::Scrolled(offset) => {
      state.tv.account.offset = offset;
      Task::none()
    }
    Message::Revealed {
      epoch,
      control,
      offset,
    } => {
      if epoch != state.tv.account.epoch || control != state.tv.account.focus {
        return Task::none();
      }
      state.tv.account.offset = offset;
      iced::widget::operation::scroll_to(
        "tv-account-scroll",
        iced::widget::operation::AbsoluteOffset { x: 0.0, y: offset },
      )
    }
  }
}

fn activate(state: &mut State, control: Control) -> Task<AppMessage> {
  if !controls(state)
    .iter()
    .flatten()
    .any(|candidate| *candidate == control)
  {
    return Task::none();
  }
  state.tv.account.focus = control;
  let event = match control {
    Control::Server | Control::Username | Control::Password => {
      state.tv.account.editing = Some(match control {
        Control::Username => Field::Username,
        Control::Password => Field::Password,
        _ => Field::Server,
      });
      state.tv.account.keyboard = text_entry::Surface::default();
      return text_entry::unfocus();
    }
    Control::Jellyfin | Control::Emby => {
      let provider = if control == Control::Jellyfin {
        MediaServerProvider::Jellyfin
      } else {
        MediaServerProvider::Emby
      };
      form_message(
        state,
        CandidateMessage::ProviderSelected(provider),
        LoginMessage::ProviderSelected(provider),
      )
    }
    Control::QuickConnect | Control::PasswordMethod => {
      let method = if control == Control::QuickConnect {
        LoginMethod::QuickConnect
      } else {
        LoginMethod::Password
      };
      form_message(
        state,
        CandidateMessage::MethodSelected(method),
        LoginMessage::MethodSelected(method),
      )
    }
    Control::Remember => form_message(
      state,
      CandidateMessage::RememberToggled,
      LoginMessage::RememberToggled,
    ),
    Control::ShowPassword => {
      state.tv.account.show_password = !state.tv.account.show_password;
      return Task::none();
    }
    Control::Submit => {
      if flow(state).method == LoginMethod::Password {
        form_message(
          state,
          CandidateMessage::PasswordSubmitted,
          LoginMessage::PasswordSubmitted,
        )
      } else {
        form_message(
          state,
          CandidateMessage::QuickConnectSubmitted,
          LoginMessage::QuickConnectSubmitted,
        )
      }
    }
    Control::CancelQuickConnect => form_message(
      state,
      CandidateMessage::QuickConnectCancelled,
      LoginMessage::QuickConnectCancelled,
    ),
    Control::Close => AppMessage::Account(accounts::Message::CloseAddAccount),
    Control::Cancel => AppMessage::Account(accounts::Message::CancelConfirmation),
    Control::Confirm => AppMessage::Account(accounts::Message::Confirm),
    Control::DeleteWatchlist => AppMessage::Account(accounts::Message::ToggleDeleteWatchlist),
    Control::ExitTv => AppMessage::UiModeSelected(UiMode::Desktop),
    Control::Profile(index) | Control::Remove(index) => {
      let Some(profile) = state.login.flow.profiles.get(index) else {
        return Task::none();
      };
      if matches!(control, Control::Remove(_)) {
        AppMessage::Account(accounts::Message::AskSignOut(profile.key().clone()))
      } else {
        AppMessage::Login(LoginMessage::RestoreProfile(profile.key().clone()))
      }
    }
  };
  // Admit the action against the surface the user actually confirmed. Delaying
  // an unscoped Confirm could apply it to a subsequently opened confirmation.
  crate::app::update::route_message(state, event)
}

fn control_id(control: Control) -> iced::widget::Id {
  iced::widget::Id::from(format!("tv-account-{control:?}"))
}

fn action<'a>(
  state: &State,
  control: Control,
  label: String,
  selected: bool,
  scale: f32,
) -> Element<'a, AppMessage> {
  let focused = state.tv.account.focus == control;
  let enabled = controls(state)
    .iter()
    .flatten()
    .any(|candidate| *candidate == control);
  container(focus(focused && enabled, move |progress| {
    button(
      ellipsis_text(label.clone())
        .size(style::BODY * scale)
        .color(style::foreground(style::PALETTE, progress, selected)),
    )
    .width(Fill)
    .height(style::CONTROL * scale)
    .padding([12.0 * scale, 20.0 * scale])
    .style(style::button_progress(style::PALETTE, progress, selected))
    .on_press_maybe(enabled.then_some(message(Message::Activate(control))))
    .into()
  }))
  .id(control_id(control))
  .width(Fill)
  .into()
}

fn heading<'a>(label: String, scale: f32) -> Element<'a, AppMessage> {
  text(label)
    .size(style::TITLE * scale)
    .font(DISPLAY_FONT)
    .color(style::PALETTE.text.heading)
    .into()
}

fn field_label(state: &State, field: Field) -> String {
  state.t(match field {
    Field::Server => "login-server-url",
    Field::Username => "login-username",
    Field::Password => "tv-account-password-optional",
  })
}

fn field<'a>(state: &State, field: Field, control: Control, scale: f32) -> Element<'a, AppMessage> {
  let value = field_value(state, field);
  let label = if value.is_empty() {
    state.t("tv-account-enter")
  } else if field == Field::Password && !state.tv.account.show_password {
    "•".repeat(value.chars().count())
  } else {
    value.to_owned()
  };
  column![
    text(field_label(state, field))
      .size(style::META * scale)
      .color(style::PALETTE.text.metadata),
    action(state, control, label, false, scale)
  ]
  .spacing(8.0 * scale)
  .width(Fill)
  .into()
}

fn form(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let login = flow(state);
  let add = state.accounts.add_account.is_some();
  if let Some(field) = state.tv.account.editing {
    return column![
      heading(field_label(state, field), scale),
      text_entry::view(
        &state.tv.account.keyboard,
        state.kernel.locale,
        field_label(state, field),
        field_value(state, field),
        field == Field::Password && !state.tv.account.show_password,
        true,
        scale
      )
      .map(|entry| message(Message::Entry(entry)))
    ]
    .spacing(style::GAP * scale)
    .into();
  }
  let mut form = column![
    heading(
      state.t(if add { "account-add" } else { "login-title" }),
      scale
    ),
    text(state.t(if add {
      "account-add-description"
    } else {
      "login-subtitle"
    }))
    .size(style::META * scale)
    .color(style::PALETTE.text.metadata),
    row![
      action(
        state,
        Control::Jellyfin,
        "Jellyfin".to_owned(),
        login.provider == MediaServerProvider::Jellyfin,
        scale
      ),
      action(
        state,
        Control::Emby,
        "Emby".to_owned(),
        login.provider == MediaServerProvider::Emby,
        scale
      )
    ]
    .spacing(16.0 * scale),
    field(state, Field::Server, Control::Server, scale),
  ]
  .spacing(20.0 * scale)
  .width(Fill);
  if login.provider == MediaServerProvider::Jellyfin {
    form = form.push(
      row![
        action(
          state,
          Control::QuickConnect,
          state.t("login-quick-connect"),
          login.method == LoginMethod::QuickConnect,
          scale
        ),
        action(
          state,
          Control::PasswordMethod,
          state.t("login-password"),
          login.method == LoginMethod::Password,
          scale
        )
      ]
      .spacing(16.0 * scale),
    );
  }
  if login.method == LoginMethod::Password {
    form = form.push(
      row![
        field(state, Field::Username, Control::Username, scale),
        field(state, Field::Password, Control::Password, scale)
      ]
      .spacing(20.0 * scale),
    );
    form = form.push(
      row![
        action(
          state,
          Control::Remember,
          state.t(if login.remember {
            "login-remember-on"
          } else {
            "login-remember-off"
          }),
          login.remember,
          scale
        ),
        action(
          state,
          Control::ShowPassword,
          state.t(if state.tv.account.show_password {
            "tv-account-hide-password"
          } else {
            "tv-account-show-password"
          }),
          state.tv.account.show_password,
          scale
        )
      ]
      .spacing(16.0 * scale),
    );
  } else {
    form = form.push(
      text(state.t("login-quick-connect-description"))
        .size(style::META * scale)
        .color(style::PALETTE.text.metadata),
    );
    match &login.quick_connect {
      QuickConnectState::Waiting(code) => {
        form = form
          .push(
            text(code)
              .font(HEADING_FONT)
              .size(56.0 * scale)
              .color(style::PALETTE.text.heading),
          )
          .push(
            text(state.t("login-code-instructions"))
              .size(style::BODY * scale)
              .color(style::PALETTE.text.body),
          );
      }
      QuickConnectState::Requesting | QuickConnectState::Approving => {
        form = form.push(
          text(
            state.t(if login.quick_connect == QuickConnectState::Requesting {
              "login-requesting-code"
            } else {
              "login-approving"
            }),
          )
          .size(style::BODY * scale),
        );
      }
      _ => {}
    }
  }
  if let Some(error) = login.error.as_ref().or(state.accounts.error.as_ref()) {
    form = form.push(
      text(state.kernel.locale.message(error))
        .size(style::META * scale)
        .color(style::PALETTE.colors.error),
    );
  }
  if quick_connect_live(state) {
    form = form.push(action(
      state,
      Control::CancelQuickConnect,
      state.t("common-cancel"),
      false,
      scale,
    ));
  } else {
    let label = if busy(state) {
      "login-signing-in"
    } else if login.method == LoginMethod::QuickConnect {
      "login-request-code"
    } else if add {
      "account-connect-switch"
    } else {
      "login-sign-in"
    };
    form = form.push(action(state, Control::Submit, state.t(label), true, scale));
  }
  form = form.push(action(
    state,
    if add { Control::Close } else { Control::ExitTv },
    state.t(if add {
      "common-cancel"
    } else {
      "tv-account-desktop"
    }),
    false,
    scale,
  ));
  if !add {
    if login.profiles_loading {
      form = form.push(text(state.t("login-loading-saved")).size(style::META * scale));
    }
    if !login.profiles.is_empty() {
      form = form.push(
        text(state.t("login-saved"))
          .size(style::SECTION * scale)
          .font(HEADING_FONT),
      );
      for (index, profile) in login.profiles.iter().enumerate() {
        form = form.push(
          column![
            row![
              action(
                state,
                Control::Profile(index),
                if login.busy_profile.as_ref() == Some(profile.key()) {
                  state.t("login-checking-saved")
                } else {
                  profile.title()
                },
                false,
                scale
              ),
              container(action(
                state,
                Control::Remove(index),
                state.t("account-sign-out"),
                false,
                scale
              ))
              .width(220.0 * scale)
            ]
            .spacing(16.0 * scale),
            text(profile.subtitle())
              .size(style::META * scale)
              .color(style::PALETTE.text.metadata)
          ]
          .spacing(8.0 * scale),
        );
      }
    }
  }
  form.into()
}

fn confirmation(state: &State, scale: f32) -> Element<'_, AppMessage> {
  let Some(confirmation) = accounts::view(state).confirmation else {
    return column![].into();
  };
  let account = confirmation
    .account
    .map_or_else(|| state.t("account-this-account"), str::to_owned);
  let (title, detail) = match confirmation.kind {
    ConfirmationKind::SwitchAccount => (
      state.t("account-switch"),
      state.format("account-confirm-switch", &[("account", account.into())]),
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
      state.format(
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
    heading(title, scale),
    text(detail)
      .size(style::BODY * scale)
      .color(style::PALETTE.text.body)
  ]
  .spacing(style::GAP * scale);
  if confirmation.kind == ConfirmationKind::SignOut {
    body = body.push(action(
      state,
      Control::DeleteWatchlist,
      state.t("account-delete-watchlist"),
      confirmation.delete_watchlist,
      scale,
    ));
    body = body.push(
      text(state.t(if confirmation.delete_watchlist {
        "tv-account-watchlist-delete"
      } else {
        "tv-account-watchlist-keep"
      }))
      .size(style::META * scale)
      .color(style::PALETTE.text.metadata),
    );
  }
  body
    .push(
      row![
        action(
          state,
          Control::Cancel,
          state.t("common-cancel"),
          false,
          scale
        ),
        action(
          state,
          Control::Confirm,
          state.t("account-confirm"),
          false,
          scale
        )
      ]
      .spacing(20.0 * scale),
    )
    .into()
}

pub fn login_view(state: &State) -> Element<'_, AppMessage> {
  responsive(move |bounds| -> Element<'_, AppMessage> {
    let scale = style::scale(bounds.width);
    container(
      scrollable(container(form(state, scale)).width(iced::Length::Fill.max(1120.0 * scale)))
        .id("tv-account-scroll")
        .on_scroll(|viewport| message(Message::Scrolled(viewport.absolute_offset().y)))
        .width(Fill)
        .height(Fill)
        .style(jellypilot_ui::theme::scrollable),
    )
    .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .style(style::canvas)
    .into()
  })
  .into()
}

pub fn modal_layer(state: &State) -> Element<'_, AppMessage> {
  if !modal_open(state) {
    return space().into();
  }
  opaque(responsive(move |bounds| -> Element<'_, AppMessage> {
    let scale = style::scale(bounds.width);
    let content = if accounts::view(state).confirmation.is_some() {
      confirmation(state, scale)
    } else {
      form(state, scale)
    };
    container(
      scrollable(
        container(content)
          .width(iced::Length::Fill.max(1120.0 * scale))
          .padding(36.0 * scale)
          .style(style::panel),
      )
      .id("tv-account-scroll")
      .on_scroll(|viewport| message(Message::Scrolled(viewport.absolute_offset().y)))
      .style(jellypilot_ui::theme::scrollable),
    )
    .padding([style::SAFE_Y * scale, style::SAFE_X * scale])
    .width(Fill)
    .height(Fill)
    .center_x(Fill)
    .center_y(Fill)
    .style(style::canvas)
    .into()
  }))
}

fn reveal(state: &State) -> Task<AppMessage> {
  use iced::advanced::widget;
  struct Reveal {
    epoch: u64,
    control: Control,
    offset: f32,
    target: Option<iced::Rectangle>,
    viewport: Option<(iced::Rectangle, iced::Rectangle)>,
  }
  impl widget::Operation<AppMessage> for Reveal {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<AppMessage>)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&control_id(self.control)) {
        self.target = Some(bounds);
      }
    }
    fn scrollable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: iced::Rectangle,
      content: iced::Rectangle,
      _: iced::Vector,
      _: &mut dyn widget::operation::Scrollable,
    ) {
      if id == Some(&widget::Id::new("tv-account-scroll")) {
        self.viewport = Some((bounds, content));
      }
    }
    fn finish(&self) -> widget::operation::Outcome<AppMessage> {
      let (Some(target), Some((viewport, content))) = (self.target, self.viewport) else {
        return widget::operation::Outcome::None;
      };
      let top = target.y - content.y;
      let offset = jellypilot_core::tv_navigation::reveal_offset(
        top,
        top + target.height,
        self.offset,
        viewport.height,
      );
      widget::operation::Outcome::Some(message(Message::Revealed {
        epoch: self.epoch,
        control: self.control,
        offset,
      }))
    }
  }
  widget::operate(Reveal {
    epoch: state.tv.account.epoch,
    control: state.tv.account.focus,
    offset: state.tv.account.offset,
    target: None,
    viewport: None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::futures::StreamExt;
  use jellypilot_auth::SensitiveSavedSession;

  fn state() -> State {
    let mut state = crate::app::update::tests::test_state();
    state.shell.ui_mode = UiMode::Tv;
    state.login.flow.profiles_loading = false;
    state
  }

  fn account_command(state: &mut State, command: accounts::Message) {
    let update = accounts::update(
      &mut state.accounts,
      &mut state.login.flow,
      &mut state.kernel,
      accounts::RuntimeFacts {
        playback_active: false,
        quit_requested: false,
      },
      command,
    );
    assert!(
      update.effect.is_none(),
      "presentation-only input must not commit a handoff"
    );
    assert!(
      iced_runtime::task::into_stream(update.task).is_none(),
      "presentation-only input must not start an SDK operation"
    );
  }

  async fn sign_out_completion(state: &mut State, task: Task<AppMessage>) -> AppMessage {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
      let mut stream = iced_runtime::task::into_stream(task).expect("sign-out task");
      while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
          match message {
            AppMessage::Account(accounts::Message::SignOutFinished { .. }) => return message,
            AppMessage::ProfileAvatarLoaded { .. } => {
              drop(crate::app::update::route_message(state, message));
            }
            _ => panic!("unexpected deferred sign-out command: {message:?}"),
          }
        }
      }
      panic!("sign-out task completed without its settlement");
    })
    .await
    .expect("sign-out settlement within timeout")
  }

  #[tokio::test]
  async fn sign_out_starts_on_cancel_and_back_never_deletes_selected_local_data() {
    let mut state = state();
    let (key, profiles) = state
      .kernel
      .auth_store
      .save_session(SensitiveSavedSession::from_saved_session(
        jellypilot_media_server::SavedSession {
          provider: MediaServerProvider::Jellyfin,
          server_url: "https://tv.example.test".to_owned(),
          access_token: "saved-token".to_owned(),
          user_id: "user".to_owned(),
          user_name: "TV user".to_owned(),
          server_name: None,
          device_id: None,
        },
      ))
      .await
      .expect("seed isolated credential store");
    state.login.flow.profiles = profiles;
    account_command(&mut state, accounts::Message::AskSignOut(key.clone()));
    drop(reconcile(&mut state));
    assert_eq!(state.tv.account.focus, Control::Cancel);
    account_command(&mut state, accounts::Message::ToggleDeleteWatchlist);
    drop(input(&mut state, Input::Right));
    assert_eq!(state.tv.account.focus, Control::Confirm);
    drop(input(&mut state, Input::Back));
    assert!(!modal_open(&state));
    assert_eq!(
      state
        .kernel
        .auth_store
        .load_profiles()
        .await
        .expect("saved profiles")[0]
        .key(),
      &key
    );
    assert!(state.kernel.sdk.active_profile().is_none());
  }

  #[tokio::test]
  async fn blank_password_can_submit_and_back_cancels_add_account_without_adoption() {
    let mut state = state();
    account_command(&mut state, accounts::Message::AddAccount);
    account_command(
      &mut state,
      accounts::Message::AddLogin(CandidateMessage::ProviderSelected(
        MediaServerProvider::Emby,
      )),
    );
    account_command(
      &mut state,
      accounts::Message::AddLogin(CandidateMessage::ServerUrlChanged(
        "https://tv.example.test/emby".to_owned(),
      )),
    );
    account_command(
      &mut state,
      accounts::Message::AddLogin(CandidateMessage::UsernameChanged("user".to_owned())),
    );
    drop(reconcile(&mut state));
    assert!(flow(&state).password.is_empty());
    state.playback.view.lifecycle.playback_active = true;
    drop(update(&mut state, Message::Activate(Control::Submit)));
    assert_eq!(
      accounts::view(&state)
        .confirmation
        .expect("existing handoff confirmation")
        .kind,
      ConfirmationKind::ConnectAndSwitch
    );
    drop(input(&mut state, Input::Back));
    assert!(state.accounts.add_account.is_some());
    drop(input(&mut state, Input::Back));
    assert!(state.accounts.add_account.is_none());
    assert!(state.kernel.sdk.active_profile().is_none());
  }

  #[tokio::test]
  async fn returning_from_field_editor_does_not_submit_or_leave_the_form() {
    let mut state = state();
    drop(reconcile(&mut state));
    drop(update(&mut state, Message::Activate(Control::Server)));
    drop(input(&mut state, Input::Back));
    assert!(state.tv.account.editing.is_none());
    assert_eq!(state.tv.account.focus, Control::Server);
    assert_eq!(state.kernel.connection, ConnectionPhase::SignedOut);
  }

  #[tokio::test]
  async fn confirmation_is_admitted_now_and_its_completion_cannot_confirm_another_account() {
    let mut state = state();
    let mut keys = Vec::new();
    for user in ["first", "second"] {
      let (key, profiles) = state
        .kernel
        .auth_store
        .save_session(SensitiveSavedSession::from_saved_session(
          jellypilot_media_server::SavedSession {
            provider: MediaServerProvider::Jellyfin,
            server_url: "https://tv.example.test".to_owned(),
            access_token: "token".to_owned(),
            user_id: user.to_owned(),
            user_name: user.to_owned(),
            server_name: None,
            device_id: None,
          },
        ))
        .await
        .expect("isolated profile");
      state.kernel.profile_avatars.insert(
        key.clone(),
        iced::widget::image::Handle::from_rgba(1, 1, vec![0; 4]),
      );
      keys.push(key);
      state.login.flow.profiles = profiles;
    }
    account_command(&mut state, accounts::Message::AskSignOut(keys[0].clone()));
    drop(reconcile(&mut state));
    drop(input(&mut state, Input::Right));
    let pending = input(&mut state, Input::Confirm);
    assert!(
      !modal_open(&state),
      "confirmation is consumed synchronously"
    );
    account_command(&mut state, accounts::Message::CancelConfirmation);
    account_command(&mut state, accounts::Message::AskSignOut(keys[1].clone()));
    assert!(
      !modal_open(&state),
      "SDK operation blocks a replacement confirmation"
    );
    let completion = sign_out_completion(&mut state, pending).await;
    assert!(matches!(
      completion,
      AppMessage::Account(accounts::Message::SignOutFinished { .. })
    ));
    drop(crate::app::update::route_message(
      &mut state,
      completion.clone(),
    ));
    account_command(&mut state, accounts::Message::AskSignOut(keys[1].clone()));
    drop(crate::app::update::route_message(&mut state, completion));
    assert_eq!(
      accounts::view(&state)
        .confirmation
        .expect("second confirmation still open")
        .kind,
      ConfirmationKind::SignOut
    );
    assert_eq!(
      state
        .kernel
        .auth_store
        .load_profiles()
        .await
        .expect("remaining profiles")[0]
        .key(),
      &keys[1]
    );
  }

  #[tokio::test]
  async fn field_changes_cannot_escape_into_a_replacement_candidate() {
    let mut state = state();
    account_command(&mut state, accounts::Message::AddAccount);
    account_command(
      &mut state,
      accounts::Message::AddLogin(CandidateMessage::ProviderSelected(
        MediaServerProvider::Emby,
      )),
    );
    drop(reconcile(&mut state));
    drop(update(&mut state, Message::Activate(Control::Password)));
    let task = update(
      &mut state,
      Message::Entry(text_entry::Message::Changed("old secret".to_owned())),
    );
    assert_eq!(flow(&state).password.as_str(), "old secret");
    account_command(&mut state, accounts::Message::CloseAddAccount);
    account_command(&mut state, accounts::Message::AddAccount);
    // A batch may have an empty stream. Consume actual deferred outputs after
    // replacing the candidate instead of relying on Task's representation.
    if let Some(mut stream) = iced_runtime::task::into_stream(task) {
      while let Some(action) = stream.next().await {
        if let iced_runtime::Action::Output(message) = action {
          assert!(
            !matches!(message, AppMessage::Login(_) | AppMessage::Account(_)),
            "field write emitted an unscoped auth message: {message:?}"
          );
          drop(crate::app::update::route_message(&mut state, message));
        }
      }
    }
    assert!(flow(&state).password.is_empty());
  }
}
