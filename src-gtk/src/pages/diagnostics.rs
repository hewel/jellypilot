use std::cell::RefCell;
use std::collections::HashMap;

use relm4::adw::prelude::*;
use relm4::{adw, gtk, Sender};

use crate::diagnostics::{
  DiagnosticChange, DiagnosticLevel, DiagnosticRow, Diagnostics, DiagnosticsViewState,
};
use crate::shell::AppMessage;

pub(crate) struct DiagnosticsPage {
  root: adw::PreferencesPage,
  list: gtk::ListBox,
  rows: RefCell<HashMap<u64, DiagnosticRowWidgets>>,
  empty: gtk::Label,
  count: gtk::Label,
  scroll: gtk::ScrolledWindow,
  copy: gtk::Button,
  clear: gtk::Button,
  status: gtk::Label,
}

pub(crate) struct DiagnosticsContext<'a> {
  pub diagnostics: &'a mut Diagnostics,
}

#[derive(Debug)]
pub(crate) enum Message {
  Copy,
  Clear,
  Refresh,
}

struct DiagnosticRowWidgets {
  row: gtk::ListBoxRow,
  message: gtk::Label,
}

impl DiagnosticsPage {
  pub(crate) fn build(sender: &Sender<AppMessage>) -> Self {
    let count = dim_label("0 sanitized runtime events");
    count.set_xalign(0.0);
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    let scroll = gtk::ScrolledWindow::builder()
      .child(&list)
      .min_content_height(360)
      .vexpand(true)
      .build();
    scroll.set_visible(false);
    let empty = dim_label("No diagnostic events yet");
    empty.set_xalign(0.0);
    empty.set_wrap(true);
    let status = dim_label("");
    status.set_visible(false);
    status.set_accessible_role(gtk::AccessibleRole::Status);
    let copy = gtk::Button::with_label("Copy diagnostics");
    copy.set_sensitive(false);
    copy.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Diagnostics(Message::Copy))
    });
    let clear = gtk::Button::with_label("Clear");
    clear.add_css_class("destructive-action");
    clear.set_sensitive(false);
    clear.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Diagnostics(Message::Clear))
    });
    let root = diagnostics_page(&count, &scroll, &empty, &status, &copy, &clear);
    Self {
      root,
      list,
      rows: RefCell::new(HashMap::new()),
      empty,
      count,
      scroll,
      copy,
      clear,
      status,
    }
  }

  pub(crate) fn root(&self) -> &adw::PreferencesPage {
    &self.root
  }

  pub(crate) fn handle(&mut self, message: Message, cx: &mut DiagnosticsContext<'_>) {
    match message {
      Message::Copy => self.copy(cx.diagnostics),
      Message::Clear => {
        cx.diagnostics.clear();
        self.status.set_label("");
        self.status.set_visible(false);
        self.render(cx.diagnostics);
      }
      Message::Refresh => self.render(cx.diagnostics),
    }
  }

  pub(crate) fn apply_change(&self, change: DiagnosticChange, diagnostics: &Diagnostics) {
    match change {
      DiagnosticChange::Added { id, dropped_id } => {
        if let Some(dropped_id) = dropped_id {
          if let Some(row) = self.rows.borrow_mut().remove(&dropped_id) {
            self.list.remove(&row.row);
          }
        }
        let Some(row) = diagnostics.row(id) else {
          self.render(diagnostics);
          return;
        };
        self.append_row(row);
        self.update_summary(diagnostics.view_state());
      }
      DiagnosticChange::Updated { id } => {
        let Some(row) = diagnostics.row(id) else {
          self.render(diagnostics);
          return;
        };
        let message = self.rows.borrow().get(&id).map(|row| row.message.clone());
        if let Some(message) = message {
          message.set_label(row.message);
        } else {
          self.render(diagnostics);
        }
      }
    }
  }

  pub(crate) fn render(&self, diagnostics: &Diagnostics) {
    clear_list_box(&self.list);
    self.rows.borrow_mut().clear();
    for row in diagnostics.rows() {
      self.append_row(row);
    }
    self.update_summary(diagnostics.view_state());
  }

  fn copy(&self, diagnostics: &Diagnostics) {
    let text = diagnostics.export_text(format_diagnostic_time);
    let Some(display) = gtk::gdk::Display::default() else {
      self
        .status
        .set_label("Copy failed: no display clipboard is available.");
      self.status.set_visible(true);
      return;
    };
    display.clipboard().set_text(&text);
    self.status.set_label("Copied");
    self.status.set_visible(true);
  }

  fn append_row(&self, diagnostic: DiagnosticRow<'_>) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    content.set_margin_top(8);
    content.set_margin_bottom(8);
    content.set_margin_start(10);
    content.set_margin_end(10);
    let time = gtk::Label::new(Some(&format_diagnostic_time(diagnostic.timestamp_seconds)));
    time.add_css_class("dim-label");
    time.add_css_class("monospace");
    time.set_valign(gtk::Align::Start);
    content.append(&time);
    let level = gtk::Label::new(Some(diagnostic.level.label()));
    level.add_css_class("caption-heading");
    level.add_css_class(match diagnostic.level {
      DiagnosticLevel::Info => "accent",
      DiagnosticLevel::Warning => "warning",
      DiagnosticLevel::Error => "error",
    });
    level.set_valign(gtk::Align::Start);
    content.append(&level);
    let category = gtk::Label::new(Some(diagnostic.category.label()));
    category.add_css_class("dim-label");
    category.set_valign(gtk::Align::Start);
    content.append(&category);
    let message = gtk::Label::new(Some(diagnostic.message));
    message.add_css_class("monospace");
    message.set_hexpand(true);
    message.set_wrap(true);
    message.set_xalign(0.0);
    message.set_selectable(true);
    content.append(&message);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&content));
    self.list.append(&row);
    self
      .rows
      .borrow_mut()
      .insert(diagnostic.id, DiagnosticRowWidgets { row, message });
  }

  fn update_summary(&self, state: DiagnosticsViewState) {
    let count = match state {
      DiagnosticsViewState::Empty => 0,
      DiagnosticsViewState::Events { count } => count,
    };
    self.count.set_label(&format!(
      "{count} sanitized runtime event{}",
      if count == 1 { "" } else { "s" }
    ));
    let populated = count > 0;
    self.empty.set_visible(!populated);
    self.scroll.set_visible(populated);
    self.copy.set_sensitive(populated);
    self.clear.set_sensitive(populated);
    if populated {
      let adjustment = self.scroll.vadjustment();
      gtk::glib::idle_add_local_once(move || {
        adjustment.set_value((adjustment.upper() - adjustment.page_size()).max(0.0));
      });
    }
  }
}

fn diagnostics_page(
  count: &gtk::Label,
  scroll: &gtk::ScrolledWindow,
  empty: &gtk::Label,
  status: &gtk::Label,
  copy: &gtk::Button,
  clear: &gtk::Button,
) -> adw::PreferencesPage {
  let page = adw::PreferencesPage::new();
  page.set_title("Diagnostics");
  page.set_icon_name(Some("dialog-information-symbolic"));
  let group = adw::PreferencesGroup::new();
  group.set_title("Sanitized runtime events");
  group.set_description(Some(
    "Connection, authentication, playback, remote-control, artwork, and configuration events useful for support.",
  ));
  let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
  content.append(count);
  content.append(empty);
  content.append(scroll);
  content.append(status);
  let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
  actions.set_halign(gtk::Align::End);
  actions.append(copy);
  actions.append(clear);
  content.append(&actions);
  group.add(&content);
  page.add(&group);
  page
}

fn dim_label(text: &str) -> gtk::Label {
  let label = gtk::Label::new(Some(text));
  label.add_css_class("dim-label");
  label.set_xalign(0.0);
  label
}

fn clear_list_box(container: &gtk::ListBox) {
  while let Some(child) = container.first_child() {
    container.remove(&child);
  }
}

fn format_diagnostic_time(timestamp_seconds: u64) -> String {
  i64::try_from(timestamp_seconds)
    .ok()
    .and_then(|timestamp| gtk::glib::DateTime::from_unix_utc(timestamp).ok())
    .and_then(|timestamp| timestamp.format("%Y-%m-%d %H:%M:%S UTC").ok())
    .map(|timestamp| timestamp.to_string())
    .unwrap_or_else(|| format!("{timestamp_seconds} UTC"))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn diagnostic_timestamp_includes_date_and_explicit_utc_zone() {
    assert_eq!(format_diagnostic_time(0), "1970-01-01 00:00:00 UTC");
  }
}
