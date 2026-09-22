//! A remote keyboard and native IME editor; the caller owns the text draft.

use iced::widget::{button, column, container, row, text, text_input, Column};
use iced::{Element, Fill, Task};
use jellypilot_core::tv_navigation::Input;
use jellypilot_ui::tv as style;
use jellypilot_ui::variants::FieldVariant;
use jellypilot_ui::widgets::ellipsis_text::ellipsis_text;
use jellypilot_ui::widgets::tv_focus::focus;

use crate::i18n::Localizer;

#[derive(Default)]
pub struct Surface {
  row: usize,
  column: usize,
  shifted: bool,
  symbols: bool,
  native: bool,
}

#[derive(Clone)]
pub enum Message {
  Key(usize, usize),
  Changed(String),
  Native,
  Done,
}

pub enum Action {
  Changed(String),
  Done,
  Cancel,
}

#[derive(Clone, Copy)]
enum Key {
  Character(char),
  Symbols,
  Shift,
  Space,
  Delete,
  Native,
  Done,
}

fn rows(surface: &Surface) -> Vec<Vec<Key>> {
  let letters: &[&str] = if surface.symbols {
    &[
      "1234567890",
      ":/.@-_~?#%",
      "!$&'()*+,;",
      "=\\\"[]{}<>|",
      "^`",
    ]
  } else {
    &["1234567890", "qwertyuiop", "asdfghjkl", "zxcvbnm"]
  };
  let mut rows: Vec<_> = letters
    .iter()
    .map(|row| {
      row
        .chars()
        .map(|character| {
          Key::Character(if surface.shifted {
            character.to_ascii_uppercase()
          } else {
            character
          })
        })
        .collect()
    })
    .collect();
  rows.push(vec![
    Key::Symbols,
    Key::Shift,
    Key::Space,
    Key::Delete,
    Key::Native,
    Key::Done,
  ]);
  rows
}

pub fn unfocus<Message: Send + 'static>() -> Task<Message> {
  iced::advanced::widget::operate(iced::advanced::widget::operation::focusable::unfocus())
}

pub fn update(
  surface: &mut Surface,
  value: &str,
  message: Message,
) -> (Option<Action>, Task<Message>) {
  match message {
    Message::Changed(value) => (Some(Action::Changed(value)), Task::none()),
    Message::Done => {
      surface.native = false;
      (Some(Action::Done), unfocus())
    }
    Message::Native => {
      surface.native = true;
      (None, iced::widget::operation::focus("tv-text-entry"))
    }
    Message::Key(row, column) => {
      let Some(key) = rows(surface)
        .get(row)
        .and_then(|keys| keys.get(column))
        .copied()
      else {
        return (None, Task::none());
      };
      surface.row = row;
      surface.column = column;
      surface.native = false;
      let action = match key {
        Key::Character(character) => Some(Action::Changed(format!("{value}{character}"))),
        Key::Space => Some(Action::Changed(format!("{value} "))),
        Key::Delete => {
          let mut value = value.to_owned();
          value.pop();
          Some(Action::Changed(value))
        }
        Key::Symbols => {
          surface.symbols = !surface.symbols;
          None
        }
        Key::Shift => {
          surface.shifted = !surface.shifted;
          None
        }
        Key::Native => return update(surface, value, Message::Native),
        Key::Done => Some(Action::Done),
      };
      (action, unfocus())
    }
  }
}

pub fn input(surface: &mut Surface, value: &str, input: Input) -> (Option<Action>, Task<Message>) {
  if input == Input::Back {
    if surface.native {
      surface.native = false;
      return (None, unfocus());
    }
    return (Some(Action::Cancel), unfocus());
  }
  if input == Input::Confirm {
    return update(surface, value, Message::Key(surface.row, surface.column));
  }
  let rows = rows(surface);
  match input {
    Input::Up => surface.row = surface.row.saturating_sub(1),
    Input::Down => surface.row = (surface.row + 1).min(rows.len() - 1),
    Input::Left => surface.column = surface.column.saturating_sub(1),
    Input::Right => surface.column = (surface.column + 1).min(rows[surface.row].len() - 1),
    _ => return (None, Task::none()),
  }
  surface.column = surface.column.min(rows[surface.row].len() - 1);
  surface.native = false;
  (None, unfocus())
}

pub fn view<'a>(
  surface: &Surface,
  locale: Localizer,
  label: String,
  value: &'a str,
  secure: bool,
  active: bool,
  scale: f32,
) -> Element<'a, Message> {
  let field: Element<'a, Message> = if surface.native {
    text_input(label, value)
      .id("tv-text-entry")
      .size(style::BODY * scale)
      .padding([18.0 * scale, 20.0 * scale])
      .secure(secure)
      .on_input(Message::Changed)
      .on_submit(Message::Done)
      .style(|theme, status| {
        jellypilot_ui::theme::field_variant(theme, status, FieldVariant::Filled)
      })
      .into()
  } else {
    let shown = if value.is_empty() {
      label
    } else if secure {
      "•".repeat(value.chars().count())
    } else {
      value.to_owned()
    };
    button(ellipsis_text(shown).size(style::BODY * scale))
      .width(Fill)
      .height(style::CONTROL * scale)
      .padding([18.0 * scale, 20.0 * scale])
      .style(style::button(style::PALETTE, false, false))
      .on_press(Message::Native)
      .into()
  };
  let mut keyboard = Column::new().spacing(12.0 * scale).width(Fill);
  for (row_index, keys) in rows(surface).into_iter().enumerate() {
    let mut row = row![].spacing(12.0 * scale).width(Fill);
    for (column_index, key) in keys.into_iter().enumerate() {
      let label = match key {
        Key::Character(character) => character.to_string(),
        Key::Symbols => if surface.symbols { "ABC" } else { "#+=" }.to_owned(),
        Key::Shift => locale.text("tv-entry-shift"),
        Key::Space => locale.text("tv-entry-space"),
        Key::Delete => locale.text("tv-entry-delete"),
        Key::Native => locale.text("tv-entry-keyboard"),
        Key::Done => locale.text("account-done"),
      };
      let focused =
        active && !surface.native && surface.row == row_index && surface.column == column_index;
      row = row.push(
        container(focus(focused, move |progress| {
          button(
            text(label.clone())
              .size(style::META * scale)
              .color(style::foreground(style::PALETTE, progress, false)),
          )
          .width(Fill)
          .height(style::CONTROL * scale)
          .style(style::button_progress(style::PALETTE, progress, false))
          .on_press(Message::Key(row_index, column_index))
          .into()
        }))
        .width(Fill),
      );
    }
    keyboard = keyboard.push(row);
  }
  column![
    field,
    text(locale.text("tv-entry-help"))
      .size(style::META * scale)
      .color(style::PALETTE.text.metadata),
    keyboard,
  ]
  .spacing(20.0 * scale)
  .into()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn back_leaves_native_editing_before_cancelling_and_never_commits() {
    let mut surface = Surface {
      native: true,
      ..Surface::default()
    };
    assert!(input(&mut surface, "draft", Input::Back).0.is_none());
    assert!(matches!(
      input(&mut surface, "draft", Input::Back).0,
      Some(Action::Cancel)
    ));
  }

  #[test]
  fn remote_keyboard_handles_short_rows_and_deletes_unicode_safely() {
    let mut surface = Surface {
      row: 3,
      column: 6,
      ..Surface::default()
    };
    drop(input(&mut surface, "", Input::Down));
    assert_eq!(surface.column, 5);
    assert!(
      matches!(update(&mut surface, "用户名", Message::Key(4, 3)).0, Some(Action::Changed(value)) if value == "用户")
    );
  }
}
