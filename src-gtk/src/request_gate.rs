/// Session identity returned when authentication starts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SessionToken(u64);

/// Identity of one Home request inside an authenticated session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HomeToken {
  session: SessionToken,
  sequence: u64,
}

/// Identity of one detail request inside an authenticated session.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DetailToken {
  session: SessionToken,
  sequence: u64,
}

/// Display-free gate for authentication and content-detail async completions.
#[derive(Clone, Debug, Default)]
pub(crate) struct RequestGate {
  session: u64,
  login: Option<SessionToken>,
  home_sequence: u64,
  home: Option<HomeToken>,
  detail_sequence: u64,
  detail: Option<DetailToken>,
}

impl RequestGate {
  pub(crate) fn begin_login(&mut self) -> SessionToken {
    self.advance_session();
    let token = SessionToken(self.session);
    self.login = Some(token);
    token
  }

  #[must_use]
  pub(crate) fn finish_login(&mut self, token: SessionToken) -> bool {
    if self.login != Some(token) {
      return false;
    }
    self.login = None;
    true
  }

  #[must_use]
  pub(crate) fn is_current_login(&self, token: SessionToken) -> bool {
    self.login == Some(token) && token == SessionToken(self.session)
  }

  pub(crate) fn disconnect(&mut self) {
    self.advance_session();
  }

  pub(crate) fn begin_home(&mut self) -> HomeToken {
    self.home_sequence = self.home_sequence.saturating_add(1);
    let token = HomeToken {
      session: SessionToken(self.session),
      sequence: self.home_sequence,
    };
    self.home = Some(token);
    token
  }

  #[must_use]
  pub(crate) fn finish_home(&mut self, token: HomeToken) -> bool {
    if self.home != Some(token) || token.session != SessionToken(self.session) {
      return false;
    }
    self.home = None;
    true
  }

  pub(crate) fn begin_detail(&mut self) -> DetailToken {
    self.detail_sequence = self.detail_sequence.saturating_add(1);
    let token = DetailToken {
      session: SessionToken(self.session),
      sequence: self.detail_sequence,
    };
    self.detail = Some(token);
    token
  }

  #[must_use]
  pub(crate) fn finish_detail(&mut self, token: DetailToken) -> bool {
    if self.detail != Some(token) || token.session != SessionToken(self.session) {
      return false;
    }
    self.detail = None;
    true
  }

  pub(crate) fn navigate(&mut self) {
    self.detail = None;
  }

  #[must_use]
  pub(crate) const fn session_generation(&self) -> u64 {
    self.session
  }

  fn advance_session(&mut self) {
    self.session = self.session.saturating_add(1);
    self.login = None;
    self.home = None;
    self.detail = None;
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn login_completion_after_disconnect_is_rejected() {
    let mut gate = RequestGate::default();
    let login = gate.begin_login();
    gate.disconnect();

    assert!(!gate.finish_login(login));
  }

  #[test]
  fn earlier_login_completion_after_relogin_is_rejected() {
    let mut gate = RequestGate::default();
    let earlier = gate.begin_login();
    let current = gate.begin_login();

    assert!(!gate.finish_login(earlier));
    assert!(gate.finish_login(current));
  }

  #[test]
  fn login_progress_is_current_only_for_the_active_login_generation() {
    let mut gate = RequestGate::default();
    let earlier = gate.begin_login();
    let current = gate.begin_login();

    assert!(!gate.is_current_login(earlier));
    assert!(gate.is_current_login(current));

    gate.disconnect();
    assert!(!gate.is_current_login(current));
  }

  #[test]
  fn detail_completion_after_navigation_is_rejected() {
    let mut gate = RequestGate::default();
    let detail = gate.begin_detail();
    gate.navigate();

    assert!(!gate.finish_detail(detail));
  }

  #[test]
  fn earlier_detail_completion_after_new_selection_is_rejected() {
    let mut gate = RequestGate::default();
    let earlier = gate.begin_detail();
    let current = gate.begin_detail();

    assert!(!gate.finish_detail(earlier));
    assert!(gate.finish_detail(current));
  }

  #[test]
  fn earlier_home_completion_after_retry_is_rejected() {
    let mut gate = RequestGate::default();
    let earlier = gate.begin_home();
    let current = gate.begin_home();

    assert!(!gate.finish_home(earlier));
    assert!(gate.finish_home(current));
  }

  #[test]
  fn home_completion_after_disconnect_is_rejected() {
    let mut gate = RequestGate::default();
    let home = gate.begin_home();
    gate.disconnect();

    assert!(!gate.finish_home(home));
  }

  #[test]
  fn detail_token_can_gate_a_nested_season_request() {
    let mut gate = RequestGate::default();
    let show = gate.begin_detail();
    assert!(gate.finish_detail(show));

    let season = gate.begin_detail();
    assert!(gate.finish_detail(season));
  }
}
