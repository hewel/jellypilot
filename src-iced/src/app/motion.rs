//! Presentation-only motion state. Window and playback lifecycles remain in their surfaces.

use std::time::Instant;

use jellypilot_ui::theme::ThemeMode;
use jellypilot_ui::tokens::TOKENS;

use super::state::ToastNotice;

#[derive(Default)]
pub(crate) struct State {
  theme: ThemeTransition,
  pub retained_modal: Option<super::view::motion::RetainedModal>,
  pub toast: Option<ToastNotice>,
}

impl State {
  pub fn sync(&mut self, mode: ThemeMode, enabled: bool, now: Instant) {
    self.theme.sync(mode, enabled, now);
  }

  pub fn theme_progress(&self, mode: ThemeMode) -> f32 {
    self
      .theme
      .target
      .map_or(theme_value(mode), |_| self.theme.value)
  }

  pub fn active(&self) -> bool {
    self.theme.started.is_some()
  }
}

#[derive(Default)]
struct ThemeTransition {
  target: Option<ThemeMode>,
  from: f32,
  value: f32,
  started: Option<Instant>,
  enabled: bool,
}

fn theme_value(mode: ThemeMode) -> f32 {
  match mode {
    ThemeMode::Dark => 0.0,
    ThemeMode::Light => 1.0,
  }
}

impl ThemeTransition {
  fn sync(&mut self, mode: ThemeMode, enabled: bool, now: Instant) {
    let target = theme_value(mode);
    if !enabled || !self.enabled || self.target.is_none() {
      self.target = Some(mode);
      self.value = target;
      self.from = target;
      self.started = None;
    } else {
      if let Some(started) = self.started {
        let elapsed = now.saturating_duration_since(started);
        let progress = elapsed.as_secs_f32() / TOKENS.durations.ms200.as_secs_f32();
        let end = self.target.map_or(target, theme_value);
        self.value = self.from + (end - self.from) * TOKENS.easings.standard.sample(progress);
        if progress >= 1.0 {
          self.value = end;
          self.started = None;
        }
      }
      if self.target != Some(mode) {
        self.target = Some(mode);
        self.from = self.value;
        self.started = (self.value != target).then_some(now);
      }
    }
    self.enabled = enabled;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  #[test]
  fn rapid_theme_reversal_continues_from_the_displayed_color_and_settles() {
    let now = Instant::now();
    let mut state = State::default();
    state.sync(ThemeMode::Dark, true, now);
    state.sync(ThemeMode::Light, true, now);
    let middle = now + Duration::from_millis(70);
    state.sync(ThemeMode::Light, true, middle);
    let displayed = state.theme_progress(ThemeMode::Light);
    assert!(displayed > 0.0 && displayed < 1.0);
    state.sync(ThemeMode::Dark, true, middle);
    assert_eq!(state.theme_progress(ThemeMode::Dark), displayed);
    state.sync(ThemeMode::Dark, true, middle + TOKENS.durations.ms200);
    assert_eq!(state.theme_progress(ThemeMode::Dark), 0.0);
    assert!(!state.active());
  }

  #[test]
  fn disabling_motion_discards_the_old_transition_before_window_restore() {
    let now = Instant::now();
    let mut state = State::default();
    state.sync(ThemeMode::Dark, true, now);
    state.sync(ThemeMode::Light, true, now);
    state.sync(ThemeMode::Light, false, now + Duration::from_millis(50));
    assert_eq!(state.theme_progress(ThemeMode::Light), 1.0);
    assert!(!state.active());
    state.sync(ThemeMode::Dark, false, now + Duration::from_millis(60));
    state.sync(ThemeMode::Dark, true, now + Duration::from_secs(1));
    assert_eq!(state.theme_progress(ThemeMode::Dark), 0.0);
    assert!(!state.active());
  }
}
