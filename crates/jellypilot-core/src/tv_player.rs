//! Remote-controlled playback chrome, independent of the player engine and UI toolkit.

use std::time::{Duration, Instant};

use crate::tv_navigation::Input;

const IDLE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Back,
    Queue,
    Upcoming,
    Information,
    Previous,
    Backward,
    PlayPause,
    Forward,
    Next,
    Audio,
    Subtitles,
    Settings,
    Skip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Queue,
    Audio,
    Subtitles,
    Information,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SeekPreview {
    pub origin: f64,
    pub candidate: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PanelState {
    pub kind: Panel,
    pub focused: usize,
    count: usize,
    origin_full: bool,
    trigger: Control,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    TogglePaused,
    Seek(f64),
    Exit,
    Previous,
    Next,
    OpenPanel(Panel),
    OpenUpcoming,
    ApplyChoice(usize),
    Skip,
}

/// Authoritative transport facts. Missing seek ranges disable seeking rather than
/// turning a duration estimate into a seekability claim.
#[derive(Clone, Copy, Debug)]
pub struct Observation {
    pub generation: u64,
    pub paused: bool,
    pub position: f64,
    pub seek_range: Option<(f64, f64)>,
    pub busy: bool,
}

#[derive(Debug)]
pub struct Player {
    generation: Option<u64>,
    full: bool,
    focus: Control,
    panel: Option<PanelState>,
    seek: Option<SeekPreview>,
    paused: bool,
    accessibility_focus: bool,
    last_input: Instant,
}

impl Default for Player {
    fn default() -> Self {
        Self::new(Instant::now())
    }
}

impl Player {
    pub fn new(now: Instant) -> Self {
        Self {
            generation: None,
            full: true,
            focus: Control::PlayPause,
            panel: None,
            seek: None,
            paused: false,
            accessibility_focus: false,
            last_input: now,
        }
    }

    pub fn full(&self) -> bool {
        self.full
    }

    /// A panel and seek transaction each own the single visible focus.
    pub fn focused(&self) -> Option<Control> {
        (self.full && self.panel.is_none() && self.seek.is_none()).then_some(self.focus)
    }

    pub fn panel(&self) -> Option<PanelState> {
        self.panel
    }

    pub fn seek(&self) -> Option<SeekPreview> {
        self.seek
    }

    pub fn observe(&mut self, observation: Observation, now: Instant) -> bool {
        let replaced = self.generation != Some(observation.generation);
        if replaced {
            *self = Self::new(now);
            self.generation = Some(observation.generation);
        }
        if observation.paused && !self.paused {
            self.reveal(now);
        } else if self.paused && !observation.paused {
            self.last_input = now;
        }
        self.paused = observation.paused;
        if let Some(seek) = &mut self.seek {
            if let Some((start, end)) = valid_range(observation.seek_range) {
                seek.candidate = seek.candidate.clamp(start, end);
            } else {
                self.seek = None;
                self.focus = Control::PlayPause;
            }
        }
        replaced
    }

    pub fn reveal(&mut self, now: Instant) {
        if !self.full {
            self.full = true;
            self.focus = Control::PlayPause;
        }
        self.last_input = now;
    }

    pub fn set_accessibility_focus(&mut self, focused: bool, now: Instant) {
        self.accessibility_focus = focused;
        self.last_input = now;
        if focused {
            self.reveal(now);
        }
    }

    pub fn deadline(&self) -> Option<Instant> {
        (self.full
            && !self.paused
            && self.panel.is_none()
            && self.seek.is_none()
            && !self.accessibility_focus)
            .then_some(self.last_input + IDLE)
    }

    pub fn expire(&mut self, now: Instant) {
        if self.deadline().is_some_and(|deadline| now >= deadline) {
            self.full = false;
        }
    }

    pub fn open_panel(&mut self, kind: Panel, selected: usize, count: usize, now: Instant) {
        self.seek = None;
        self.panel = Some(PanelState {
            kind,
            focused: selected.min(count.saturating_sub(1)),
            count,
            origin_full: self.full,
            trigger: self.focus,
        });
        self.last_input = now;
    }

    pub fn close_panel(&mut self, now: Instant) {
        if let Some(panel) = self.panel.take() {
            self.full = panel.origin_full;
            self.focus = panel.trigger;
            self.last_input = now;
        }
    }

    pub fn focus_choice(&mut self, index: usize) {
        if let Some(panel) = &mut self.panel {
            panel.focused = index.min(panel.count.saturating_sub(1));
        }
    }

    /// Updates live choices without replacing the panel's source state or trigger.
    pub fn replace_choices(&mut self, focused: usize, count: usize) {
        if let Some(panel) = &mut self.panel {
            panel.count = count;
            panel.focused = focused.min(count.saturating_sub(1));
        }
    }

    pub fn begin_seek(&mut self, observation: Observation, now: Instant) {
        if self.panel.is_some() || observation.busy || !observation.position.is_finite() {
            return;
        }
        if let Some((start, end)) = valid_range(observation.seek_range) {
            self.reveal(now);
            let candidate = observation.position.clamp(start, end);
            self.seek = Some(SeekPreview {
                origin: candidate,
                candidate,
            });
        }
    }

    /// Activates an explicit pointer target without borrowing the current remote focus.
    pub fn activate(
        &mut self,
        control: Control,
        observation: Observation,
        now: Instant,
    ) -> Option<Action> {
        if self.panel.is_some() || observation.busy {
            return None;
        }
        self.last_input = now;
        self.focus = control;
        match control {
            Control::Back => Some(Action::Exit),
            Control::PlayPause => Some(Action::TogglePaused),
            Control::Previous => Some(Action::Previous),
            Control::Next => Some(Action::Next),
            Control::Skip => Some(Action::Skip),
            Control::Queue => Some(Action::OpenPanel(Panel::Queue)),
            Control::Upcoming => Some(Action::OpenUpcoming),
            Control::Information => Some(Action::OpenPanel(Panel::Information)),
            Control::Audio => Some(Action::OpenPanel(Panel::Audio)),
            Control::Subtitles => Some(Action::OpenPanel(Panel::Subtitles)),
            Control::Settings => Some(Action::OpenPanel(Panel::Settings)),
            Control::Backward | Control::Forward => {
                if !observation.position.is_finite() {
                    return None;
                }
                let (start, end) = valid_range(observation.seek_range)?;
                let delta = if control == Control::Backward {
                    -10.0
                } else {
                    10.0
                };
                Some(Action::Seek(
                    (observation.position + delta).clamp(start, end),
                ))
            }
        }
    }

    /// Returns engine work only for a committed action; navigation never changes
    /// playback intent or applies an unconfirmed track/seek candidate.
    pub fn input(
        &mut self,
        input: Input,
        observation: Observation,
        controls: &[Control],
        now: Instant,
    ) -> Option<Action> {
        self.last_input = now;
        if input == Input::PlayPause {
            return (!observation.busy).then_some(Action::TogglePaused);
        }
        if let Some(panel) = &mut self.panel {
            match input {
                Input::Back => self.close_panel(now),
                Input::Up if panel.kind == Panel::Settings => {
                    panel.focused = if panel.focused >= 6 {
                        5
                    } else if panel.focused == 5 {
                        1
                    } else {
                        panel.focused
                    };
                }
                Input::Down if panel.kind == Panel::Settings => {
                    panel.focused = if panel.focused < 5 {
                        5
                    } else {
                        6.min(panel.count.saturating_sub(1))
                    };
                }
                Input::Left if panel.kind == Panel::Settings => {
                    if panel.focused < 5 {
                        panel.focused = panel.focused.saturating_sub(1);
                    }
                }
                Input::Right if panel.kind == Panel::Settings => {
                    if panel.focused < 5 {
                        panel.focused = (panel.focused + 1).min(4);
                    }
                }
                Input::Up | Input::Left => panel.focused = panel.focused.saturating_sub(1),
                Input::Down | Input::Right => {
                    panel.focused = (panel.focused + 1).min(panel.count.saturating_sub(1));
                }
                Input::Confirm if panel.count > 0 && !observation.busy => {
                    return Some(Action::ApplyChoice(panel.focused));
                }
                _ => {}
            }
            return None;
        }
        if !self.full {
            if input == Input::Back {
                return (!observation.busy).then_some(Action::Exit);
            }
            self.reveal(now);
            return None;
        }
        if let Some(seek) = &mut self.seek {
            match input {
                Input::Left | Input::Right => {
                    if let Some((start, end)) = valid_range(observation.seek_range) {
                        let delta = if input == Input::Left { -10.0 } else { 10.0 };
                        seek.candidate = (seek.candidate + delta).clamp(start, end);
                    }
                }
                Input::Confirm if !observation.busy => {
                    let candidate = valid_range(observation.seek_range)
                        .map(|(start, end)| seek.candidate.clamp(start, end));
                    self.seek = None;
                    self.focus = Control::PlayPause;
                    return candidate.map(Action::Seek);
                }
                Input::Back | Input::Down => {
                    self.seek = None;
                    self.focus = Control::PlayPause;
                }
                Input::Up => {
                    self.seek = None;
                    self.focus = Control::Back;
                }
                _ => {}
            }
            return None;
        }
        if !controls.contains(&self.focus) && self.focus != Control::Back {
            self.focus = Control::PlayPause;
        }
        match input {
            Input::Back => self.full = false,
            Input::Right
                if self.focus == Control::Back && controls.contains(&Control::Upcoming) =>
            {
                self.focus = Control::Upcoming;
            }
            Input::Left if self.focus == Control::Upcoming => self.focus = Control::Back,
            Input::Down if self.focus == Control::Upcoming => self.focus = Control::PlayPause,
            Input::Up | Input::Right if self.focus == Control::Upcoming => {}
            Input::Up if self.focus != Control::Back && !observation.busy => {
                self.begin_seek(observation, now);
                if self.seek.is_none() {
                    self.focus = Control::Back;
                }
            }
            Input::Down if self.focus == Control::Back => self.focus = Control::PlayPause,
            Input::Left | Input::Right => {
                if let Some(index) = controls.iter().position(|control| *control == self.focus) {
                    let next = if input == Input::Left {
                        controls[..index]
                            .iter()
                            .rev()
                            .find(|control| **control != Control::Upcoming)
                    } else {
                        controls[index + 1..]
                            .iter()
                            .find(|control| **control != Control::Upcoming)
                    };
                    if let Some(next) = next {
                        self.focus = *next;
                    }
                }
            }
            Input::Confirm => return self.activate(self.focus, observation, now),
            _ => {}
        }
        None
    }
}

fn valid_range(range: Option<(f64, f64)>) -> Option<(f64, f64)> {
    range
        .filter(|(start, end)| start.is_finite() && end.is_finite() && *start >= 0.0 && end > start)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTROLS: &[Control] = &[Control::Information, Control::PlayPause, Control::Subtitles];

    fn observation(paused: bool) -> Observation {
        Observation {
            generation: 1,
            paused,
            position: 30.0,
            seek_range: Some((20.0, 60.0)),
            busy: false,
        }
    }

    #[test]
    fn upcoming_queue_uses_top_row_without_replacing_season_navigation() {
        let now = Instant::now();
        let mut player = Player::new(now);
        let observation = observation(true);
        let controls = [
            Control::Queue,
            Control::Upcoming,
            Control::Information,
            Control::PlayPause,
        ];
        player.observe(observation, now);
        player.input(Input::Up, observation, &controls, now);
        player.input(Input::Up, observation, &controls, now);
        assert_eq!(player.focused(), Some(Control::Back));
        player.input(Input::Right, observation, &controls, now);
        assert_eq!(player.focused(), Some(Control::Upcoming));
        assert_eq!(
            player.input(Input::Confirm, observation, &controls, now),
            Some(Action::OpenUpcoming)
        );
        player.input(Input::Down, observation, &controls, now);
        player.input(Input::Left, observation, &controls, now);
        player.input(Input::Left, observation, &controls, now);
        assert_eq!(player.focused(), Some(Control::Queue));
        assert_eq!(
            player.input(Input::Confirm, observation, &controls, now),
            Some(Action::OpenPanel(Panel::Queue))
        );
    }

    #[test]
    fn first_remote_event_reveals_without_pausing_or_seeking() {
        for input in [
            Input::Up,
            Input::Down,
            Input::Left,
            Input::Right,
            Input::Confirm,
        ] {
            let now = Instant::now();
            let mut player = Player::new(now);
            player.observe(observation(false), now);
            player.expire(now + IDLE);
            assert_eq!(
                player.input(input, observation(false), CONTROLS, now + IDLE),
                None
            );
            assert_eq!(player.focused(), Some(Control::PlayPause));
            assert_eq!(player.seek(), None);
        }
    }

    #[test]
    fn paused_seek_is_a_single_commit_and_cancel_does_not_change_transport() {
        let now = Instant::now();
        let mut player = Player::new(now);
        let observation = observation(true);
        player.observe(observation, now);
        player.input(Input::Up, observation, CONTROLS, now);
        for _ in 0..8 {
            player.input(Input::Right, observation, CONTROLS, now);
        }
        assert_eq!(player.seek().map(|seek| seek.candidate), Some(60.0));
        assert_eq!(
            player.input(Input::Confirm, observation, CONTROLS, now),
            Some(Action::Seek(60.0))
        );
        assert_eq!(player.seek(), None);
        assert_eq!(player.deadline(), None);
        player.input(Input::Up, observation, CONTROLS, now);
        player.input(Input::Left, observation, CONTROLS, now);
        assert_eq!(player.input(Input::Back, observation, CONTROLS, now), None);
        assert_eq!(player.focused(), Some(Control::PlayPause));
    }

    #[test]
    fn paused_full_can_hide_and_second_back_exits_without_resuming() {
        let now = Instant::now();
        let mut player = Player::new(now);
        player.observe(observation(true), now);
        assert_eq!(
            player.input(Input::Back, observation(true), CONTROLS, now),
            None
        );
        player.observe(observation(true), now + IDLE);
        assert!(!player.full());
        assert_eq!(
            player.input(Input::Back, observation(true), CONTROLS, now),
            Some(Action::Exit)
        );
    }

    #[test]
    fn track_candidates_are_not_applied_by_navigation_or_cancel() {
        let now = Instant::now();
        let mut player = Player::new(now);
        player.activate(Control::Subtitles, observation(false), now);
        player.open_panel(Panel::Subtitles, 2, 4, now);
        assert_eq!(
            player.input(Input::Down, observation(false), CONTROLS, now),
            None
        );
        assert_eq!(player.panel().map(|panel| panel.focused), Some(3));
        assert_eq!(
            player.input(Input::Back, observation(false), CONTROLS, now),
            None
        );
        assert_eq!(player.focused(), Some(Control::Subtitles));
        player.open_panel(Panel::Subtitles, 2, 4, now);
        assert_eq!(
            player.input(Input::Confirm, observation(false), CONTROLS, now),
            Some(Action::ApplyChoice(2))
        );
    }

    #[test]
    fn minimal_origin_panel_returns_to_pure_video() {
        let now = Instant::now();
        let mut player = Player::new(now);
        player.expire(now + IDLE);
        player.open_panel(Panel::Information, 0, 1, now + IDLE);
        assert_eq!(player.deadline(), None);
        player.close_panel(now + IDLE);
        assert!(!player.full());
        assert_eq!(player.focused(), None);
    }

    #[test]
    fn replacement_and_loss_of_seekability_retire_uncommitted_seek() {
        let now = Instant::now();
        let mut player = Player::new(now);
        player.observe(observation(false), now);
        player.input(Input::Up, observation(false), CONTROLS, now);
        player.observe(
            Observation {
                seek_range: None,
                ..observation(false)
            },
            now,
        );
        assert_eq!(player.seek(), None);
        player.open_panel(Panel::Subtitles, 2, 4, now);
        player.observe(
            Observation {
                generation: 2,
                ..observation(false)
            },
            now,
        );
        assert_eq!(player.panel(), None);
        assert_eq!(player.focused(), Some(Control::PlayPause));
    }

    #[test]
    fn auto_hide_waits_for_panel_seek_and_accessibility_focus() {
        let now = Instant::now();
        let mut player = Player::new(now);
        player.observe(observation(false), now);
        player.input(Input::Up, observation(false), CONTROLS, now);
        assert_eq!(player.deadline(), None);
        player.input(Input::Back, observation(false), CONTROLS, now);
        player.open_panel(Panel::Settings, 0, 1, now);
        player.expire(now + Duration::from_secs(30));
        assert!(player.full());
        player.close_panel(now);
        player.set_accessibility_focus(true, now);
        player.expire(now + Duration::from_secs(30));
        assert!(player.full());
        player.set_accessibility_focus(false, now + Duration::from_secs(30));
        player.expire(now + Duration::from_secs(35));
        assert!(!player.full());
    }
}
