//! TV detail projection and remote traversal over the shared detail reducer.

use iced::Task;
use jellypilot_core::detail::DetailContent;
use jellypilot_core::home_hero::has_resume_position;
use jellypilot_core::tv_navigation::Input;
use jellypilot_core::LoadState;
use jellypilot_media_server::VideoSeasonEpisodesPage;

use super::{navigation, AppMessage, Focus, Message, State};
use crate::app::message::DetailMessage;
use crate::app::state::Destination;
use crate::i18n::UiText;

pub(super) fn episodes(state: &State) -> Option<&LoadState<VideoSeasonEpisodesPage, UiText>> {
  let data = &state.full.as_ref()?.detail.data;
  match &data.content {
    LoadState::Ready(DetailContent::Show(_)) => Some(&data.season_episodes),
    LoadState::Ready(DetailContent::Item(item))
      if item.item_type.eq_ignore_ascii_case("episode") =>
    {
      Some(&data.season_neighbors)
    }
    _ => None,
  }
}

pub(super) fn append(state: &State) -> Option<&LoadState<(), UiText>> {
  let data = &state.full.as_ref()?.detail.data;
  match &data.content {
    LoadState::Ready(DetailContent::Show(_)) => Some(&data.season_append),
    LoadState::Ready(DetailContent::Item(item))
      if item.item_type.eq_ignore_ascii_case("episode") =>
    {
      Some(&data.neighbors_append)
    }
    _ => None,
  }
}

pub(super) fn has_more(state: &State) -> bool {
  matches!(episodes(state), Some(LoadState::Ready(page)) if page.has_more)
}

pub(super) fn retry_episodes(state: &State) -> bool {
  matches!(episodes(state), Some(LoadState::Failed(_)))
}

pub(super) fn episode_message(state: &State, more: bool) -> DetailMessage {
  let is_show = state.full.as_ref().is_some_and(|full| {
    matches!(
      full.detail.data.content,
      LoadState::Ready(DetailContent::Show(_))
    )
  });
  match (is_show, more) {
    (true, true) => DetailMessage::LoadMoreSeason,
    (true, false) => DetailMessage::RetrySeason,
    (false, true) => DetailMessage::LoadMoreNeighbors,
    (false, false) => DetailMessage::RetryNeighbors,
  }
}

pub(super) fn overview(state: &State) -> Option<&str> {
  let content = &state.full.as_ref()?.detail.data.content;
  let copy = match content {
    LoadState::Ready(DetailContent::Item(item)) => item.overview.as_deref(),
    LoadState::Ready(DetailContent::Show(show)) => show.overview.as_deref(),
    _ => None,
  };
  copy.filter(|value| !value.trim().is_empty())
}

pub(super) fn overview_expanded(state: &State) -> bool {
  state
    .full
    .as_ref()
    .is_some_and(|full| full.detail.data.overview_expanded)
}

fn actions(state: &State) -> Vec<Focus> {
  if state
    .full
    .as_ref()
    .is_none_or(|full| !matches!(full.detail.data.content, LoadState::Ready(_)))
  {
    return Vec::new();
  }
  navigation::detail_playable(state)
    .map(|_| Focus::DetailPlay)
    .into_iter()
    .chain([
      Focus::DetailWatchlist,
      Focus::DetailFavorite,
      Focus::DetailMenu,
    ])
    .collect()
}

pub(super) fn first_action(state: &State) -> Focus {
  actions(state).first().copied().unwrap_or_else(|| {
    if state
      .full
      .as_ref()
      .is_some_and(|full| matches!(full.detail.data.content, LoadState::Failed(_)))
    {
      Focus::Retry
    } else {
      Focus::DetailBack
    }
  })
}

fn selected_season(state: &State) -> Option<Focus> {
  let selected = state
    .full
    .as_ref()?
    .detail
    .data
    .selected_season_id
    .as_deref();
  let seasons = navigation::seasons(state);
  (!seasons.is_empty()).then(|| {
    Focus::Season(
      seasons
        .iter()
        .position(|season| Some(season.id.as_str()) == selected)
        .unwrap_or(0),
    )
  })
}

fn episode_entry(state: &State) -> Option<Focus> {
  if !navigation::episodes(state).is_empty() {
    Some(Focus::Episode(0))
  } else if has_more(state) {
    Some(Focus::DetailEpisodesMore)
  } else if retry_episodes(state) {
    Some(Focus::DetailEpisodesRetry)
  } else {
    None
  }
}

fn below_actions(state: &State) -> Option<Focus> {
  selected_season(state).or_else(|| episode_entry(state))
}

pub(super) fn normalize(state: &State, focus: Focus) -> Focus {
  match focus {
    Focus::DetailPlay | Focus::DetailWatchlist | Focus::DetailFavorite | Focus::DetailMenu
      if !actions(state).contains(&focus) =>
    {
      first_action(state)
    }
    Focus::DetailOverview if overview(state).is_none() => first_action(state),
    Focus::Season(index) => selected_season(state).map_or_else(
      || first_action(state),
      |_| Focus::Season(index.min(navigation::seasons(state).len() - 1)),
    ),
    Focus::Episode(index) => {
      let items = navigation::episodes(state);
      if items.is_empty() {
        below_actions(state).unwrap_or_else(|| first_action(state))
      } else {
        let index = state
          .tv
          .focused_item
          .as_ref()
          .and_then(|id| items.iter().position(|item| &item.id == id))
          .unwrap_or(index.min(items.len() - 1));
        Focus::Episode(index)
      }
    }
    Focus::DetailEpisodesMore if !has_more(state) => navigation::episodes(state)
      .len()
      .checked_sub(1)
      .map(Focus::Episode)
      .or_else(|| below_actions(state))
      .unwrap_or_else(|| first_action(state)),
    Focus::DetailEpisodesRetry if !retry_episodes(state) => {
      below_actions(state).unwrap_or_else(|| first_action(state))
    }
    Focus::Retry
      if !state
        .full
        .as_ref()
        .is_some_and(|full| matches!(full.detail.data.content, LoadState::Failed(_))) =>
    {
      first_action(state)
    }
    other => other,
  }
}

pub(super) fn navigate(state: &State, input: Input) -> Focus {
  let focus = state.tv.focus;
  let actions = actions(state);
  if let Some(index) = actions.iter().position(|action| action == &focus) {
    return match input {
      Input::Left => index
        .checked_sub(1)
        .map_or(Focus::DetailBack, |index| actions[index]),
      Input::Right => actions.get(index + 1).copied().unwrap_or(focus),
      Input::Up => {
        if overview(state).is_some() {
          Focus::DetailOverview
        } else {
          Focus::DetailBack
        }
      }
      Input::Down => below_actions(state).unwrap_or(focus),
      _ => focus,
    };
  }
  match (focus, input) {
    (Focus::DetailBack, Input::Down | Input::Right) => first_action(state),
    (Focus::DetailOverview, Input::Down | Input::Right) => first_action(state),
    (Focus::DetailOverview, Input::Up | Input::Left) => Focus::DetailBack,
    (Focus::Season(index), Input::Left) => Focus::Season(index.saturating_sub(1)),
    (Focus::Season(index), Input::Right) => {
      Focus::Season((index + 1).min(navigation::seasons(state).len().saturating_sub(1)))
    }
    (Focus::Season(_), Input::Up) => first_action(state),
    (Focus::Season(_), Input::Down) => episode_entry(state).unwrap_or(focus),
    (Focus::Episode(index), Input::Left) => Focus::Episode(index.saturating_sub(1)),
    (Focus::Episode(index), Input::Right) => {
      if index + 1 < navigation::episodes(state).len() {
        Focus::Episode(index + 1)
      } else if has_more(state) {
        Focus::DetailEpisodesMore
      } else {
        focus
      }
    }
    (Focus::Episode(_), Input::Down) if has_more(state) => Focus::DetailEpisodesMore,
    (Focus::DetailEpisodesMore, Input::Left | Input::Up) => navigation::episodes(state)
      .len()
      .checked_sub(1)
      .map(Focus::Episode)
      .or_else(|| selected_season(state))
      .unwrap_or_else(|| first_action(state)),
    (Focus::Episode(_) | Focus::DetailEpisodesRetry, Input::Up) => {
      selected_season(state).unwrap_or_else(|| first_action(state))
    }
    (Focus::Retry, Input::Up | Input::Left) => Focus::DetailBack,
    _ => focus,
  }
}

pub(super) fn reconcile_page(state: &mut State) -> bool {
  let Destination::Detail(id) = &state.shell.destination else {
    state.tv.detail_page = None;
    state.tv.detail_overview_expanded = false;
    return false;
  };
  let season = state
    .full
    .as_ref()
    .and_then(|full| full.detail.data.selected_season_id.clone());
  let count = navigation::episodes(state).len();
  if let Some((old_id, old_season, previous_count)) = &state.tv.detail_page {
    if id == old_id
      && &season == old_season
      && count > *previous_count
      && state.tv.focus == Focus::DetailEpisodesMore
    {
      state.tv.focus = Focus::Episode(*previous_count);
      state.tv.focused_item = None;
    }
  }
  state.tv.detail_page = Some((id.clone(), season, count));
  let expanded = overview_expanded(state);
  let changed = state.tv.detail_overview_expanded != expanded;
  state.tv.detail_overview_expanded = expanded;
  changed
}

pub(super) fn play_label(state: &State) -> String {
  let episode_label = |season: Option<i32>, episode: Option<i32>| match (season, episode) {
    (Some(season), Some(episode)) => format!("S{season:02}E{episode:02}"),
    _ => state.t("detail-episode"),
  };
  match state.full.as_ref().map(|full| &full.detail.data.content) {
    Some(LoadState::Ready(DetailContent::Show(show))) => show.next_episode.as_ref().map_or_else(
      || state.t("detail-play"),
      |episode| {
        state.format(
          if has_resume_position(episode) {
            "detail-continue-episode"
          } else {
            "detail-play-episode"
          },
          &[(
            "episode",
            episode_label(episode.season_number, episode.episode_number).into(),
          )],
        )
      },
    ),
    Some(LoadState::Ready(DetailContent::Item(item)))
      if item.item_type.eq_ignore_ascii_case("episode") =>
    {
      state.format(
        if item.can_resume {
          "detail-continue-episode"
        } else {
          "detail-play-episode"
        },
        &[(
          "episode",
          episode_label(item.season_number, item.episode_number).into(),
        )],
      )
    }
    Some(LoadState::Ready(DetailContent::Item(item))) if item.can_resume => {
      state.t("detail-resume")
    }
    _ => state.t("detail-play"),
  }
}

/// Uses the rendered paragraph bounds, so long translations remain readable by remote.
/// Confirm still collapses; Left/Right leaves reading immediately.
pub(super) fn read_overview(state: &State, direction: Input) -> Task<AppMessage> {
  use iced::advanced::widget;
  struct Read {
    destination: Destination,
    session: jellypilot_core::request_gate::SessionToken,
    direction: Input,
    overview: Option<iced::Rectangle>,
    trigger: Option<iced::Rectangle>,
    viewport: Option<(iced::Rectangle, iced::Rectangle)>,
    offset: f32,
  }
  impl widget::Operation<AppMessage> for Read {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn widget::Operation<AppMessage>)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&widget::Id::new("tv-detail-overview")) {
        self.overview = Some(bounds);
      }
      if id == Some(&navigation::focus_id(Focus::DetailOverview)) {
        self.trigger = Some(bounds);
      }
    }
    fn scrollable(
      &mut self,
      id: Option<&widget::Id>,
      bounds: iced::Rectangle,
      content: iced::Rectangle,
      _translation: iced::Vector,
      _state: &mut dyn widget::operation::Scrollable,
    ) {
      if id == Some(&navigation::scroll_id()) {
        self.viewport = Some((bounds, content));
      }
    }
    fn finish(&self) -> widget::operation::Outcome<AppMessage> {
      let (Some(overview), Some((viewport, content))) = (self.overview, self.viewport) else {
        return widget::operation::Outcome::None;
      };
      let start = (self.trigger.map_or(overview.y, |trigger| trigger.y) - content.y).max(0.0);
      let end = (overview.y + overview.height - content.y - viewport.height).max(start);
      let offset = match self.direction {
        Input::Down if self.offset + 1.0 < end => {
          Some((self.offset + viewport.height * 0.7).min(end))
        }
        Input::Up if self.offset > start + 1.0 => {
          Some((self.offset - viewport.height * 0.7).max(start))
        }
        _ => None,
      };
      widget::operation::Outcome::Some(AppMessage::Tv(Message::OverviewMoved {
        destination: self.destination.clone(),
        session: self.session,
        direction: self.direction,
        offset,
      }))
    }
  }
  widget::operate(Read {
    destination: state.shell.destination.clone(),
    session: state.kernel.request_gate.current_session(),
    direction,
    overview: None,
    trigger: None,
    viewport: None,
    offset: state.tv.offset,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use iced::futures::StreamExt;
  use jellypilot_core::detail::{append_season_page, next_season_page_request};
  use jellypilot_media_server::{VideoItemDetail, VideoSeason, VideoShowDetail};

  fn state() -> State {
    let mut state = State::boot(true);
    state.full = Some(crate::app::state::FullUi::default());
    state.shell.ui_mode = jellypilot_core::config::UiMode::Tv;
    state.kernel.connection = jellypilot_auth::login::ConnectionPhase::Connected;
    state.shell.destination = Destination::Detail("show".to_owned());
    state.playback.view.engine_available = true;
    state
      .full
      .as_mut()
      .expect("full")
      .personal_lists
      .membership_loaded = true;
    state
  }

  fn show_state() -> State {
    let mut state = state();
    let mut next = super::super::view::tests::item("next");
    next.item_type = "Episode".to_owned();
    next.season_number = Some(2);
    next.episode_number = Some(3);
    next.resume_position_seconds = Some(1860.0);
    let data = &mut state.full.as_mut().expect("full").detail.data;
    data.content = LoadState::Ready(DetailContent::Show(Box::new(VideoShowDetail {
      id: "show".to_owned(),
      name: "Show".to_owned(),
      overview: Some("The full overview.".to_owned()),
      production_year: None,
      genres: vec![],
      played: false,
      favorite: false,
      can_play: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      logo_image_id: None,
      next_episode: Some(next),
      seasons: (1..=2)
        .map(|number| VideoSeason {
          id: format!("season-{number}"),
          name: format!("Season {number}"),
          season_number: Some(number),
          played: false,
          favorite: false,
          artwork_image_id: None,
        })
        .collect(),
      metadata: Default::default(),
      original_language: None,
    })));
    data.selected_season_id = Some("season-2".to_owned());
    state
  }

  fn episode_state() -> State {
    let mut state = state();
    state.shell.destination = Destination::Detail("self".to_owned());
    state.full.as_mut().expect("full").detail.data.content =
      LoadState::Ready(DetailContent::Item(Box::new(VideoItemDetail {
        id: "self".to_owned(),
        name: "Episode".to_owned(),
        item_type: "Episode".to_owned(),
        overview: None,
        original_language: None,
        production_year: None,
        runtime_seconds: Some(4080.0),
        series_id: Some("show".to_owned()),
        series_name: Some("Show".to_owned()),
        season_number: Some(2),
        episode_number: Some(3),
        genres: vec![],
        played: false,
        favorite: false,
        played_percentage: None,
        resume_position_seconds: Some(1860.0),
        can_resume: true,
        can_play: true,
        artwork_image_id: None,
        backdrop_image_id: None,
        logo_image_id: None,
        series_poster_image_id: None,
        media_info: None,
        metadata: Default::default(),
      })));
    state
  }

  fn page(start: i32, end: i32, total: i32) -> VideoSeasonEpisodesPage {
    VideoSeasonEpisodesPage {
      series_id: "show".to_owned(),
      season_id: Some("season-2".to_owned()),
      season_number: Some(2),
      start_index: start,
      limit: 30,
      total_record_count: total,
      next_start_index: end,
      has_more: end < total,
      episodes: (start..end)
        .map(|index| super::super::view::tests::item(&format!("episode-{index}")))
        .collect(),
    }
  }

  fn move_focus(state: &mut State, input: Input) {
    drop(navigation::input(state, input));
    navigation::reconcile_focus(state);
  }

  async fn activation(state: &mut State, focus: Focus) -> Option<AppMessage> {
    let task = navigation::activate(state, focus);
    let mut stream = iced_runtime::task::into_stream(task)?;
    match stream.next().await {
      Some(iced_runtime::Action::Output(message)) => Some(message),
      _ => None,
    }
  }

  #[test]
  fn no_play_target_keeps_all_rendered_actions_and_selected_season_reachable() {
    let mut state = show_state();
    state.tv.focus = Focus::DetailBack;
    move_focus(&mut state, Input::Down);
    assert_eq!(state.tv.focus, Focus::DetailWatchlist);
    move_focus(&mut state, Input::Right);
    assert_eq!(state.tv.focus, Focus::DetailFavorite);
    move_focus(&mut state, Input::Right);
    assert_eq!(state.tv.focus, Focus::DetailMenu);
    move_focus(&mut state, Input::Left);
    move_focus(&mut state, Input::Left);
    move_focus(&mut state, Input::Down);
    assert_eq!(state.tv.focus, Focus::Season(1));
    move_focus(&mut state, Input::Up);
    assert_eq!(state.tv.focus, Focus::DetailWatchlist);
    move_focus(&mut state, Input::Up);
    assert_eq!(state.tv.focus, Focus::DetailOverview);
    move_focus(&mut state, Input::Up);
    assert_eq!(state.tv.focus, Focus::DetailBack);
  }

  #[tokio::test]
  async fn initial_loading_empty_and_failure_have_distinct_reachable_actions() {
    let mut state = show_state();
    state.tv.focus = Focus::Season(1);
    for load in [LoadState::Loading, LoadState::Ready(page(0, 0, 0))] {
      state
        .full
        .as_mut()
        .expect("full")
        .detail
        .data
        .season_episodes = load;
      move_focus(&mut state, Input::Down);
      assert_eq!(
        state.tv.focus,
        Focus::Season(1),
        "no false retry or phantom episode"
      );
    }
    state
      .full
      .as_mut()
      .expect("full")
      .detail
      .data
      .season_episodes = LoadState::Failed(UiText::new("detail-season-error"));
    move_focus(&mut state, Input::Down);
    assert_eq!(state.tv.focus, Focus::DetailEpisodesRetry);
    assert!(matches!(
      activation(&mut state, Focus::DetailEpisodesRetry).await,
      Some(AppMessage::Detail(DetailMessage::RetrySeason))
    ));
    state.full.as_mut().expect("full").detail.data.content = LoadState::Loading;
    navigation::reconcile_focus(&mut state);
    assert_eq!(state.tv.focus, Focus::DetailBack);
    state.full.as_mut().expect("full").detail.data.content =
      LoadState::Failed(UiText::new("detail-load-error"));
    move_focus(&mut state, Input::Down);
    assert_eq!(state.tv.focus, Focus::Retry);
  }

  #[tokio::test]
  async fn paging_preserves_busy_focus_retries_in_place_and_enters_first_new_card() {
    let mut state = show_state();
    state
      .full
      .as_mut()
      .expect("full")
      .detail
      .data
      .season_episodes = LoadState::Ready(page(0, 30, 35));
    reconcile_page(&mut state);
    state.tv.focus = Focus::Episode(29);
    move_focus(&mut state, Input::Right);
    assert_eq!(state.tv.focus, Focus::DetailEpisodesMore);
    assert!(matches!(
      activation(&mut state, Focus::DetailEpisodesMore).await,
      Some(AppMessage::Detail(DetailMessage::LoadMoreSeason))
    ));
    state.full.as_mut().expect("full").detail.data.season_append = LoadState::Loading;
    navigation::reconcile_focus(&mut state);
    assert_eq!(state.tv.focus, Focus::DetailEpisodesMore);
    assert!(activation(&mut state, Focus::DetailEpisodesMore)
      .await
      .is_none());
    assert_eq!(navigation::episodes(&state).len(), 30);
    state.full.as_mut().expect("full").detail.data.season_append =
      LoadState::Failed(UiText::new("detail-more-episodes-error"));
    assert!(matches!(
      activation(&mut state, Focus::DetailEpisodesMore).await,
      Some(AppMessage::Detail(DetailMessage::LoadMoreSeason))
    ));
    let data = &mut state.full.as_mut().expect("full").detail.data;
    let LoadState::Ready(loaded) = &mut data.season_episodes else {
      panic!("loaded page")
    };
    assert_eq!(
      next_season_page_request(loaded)
        .expect("next cursor")
        .start_index,
      30
    );
    assert!(append_season_page(loaded, page(30, 35, 35)));
    data.season_append = LoadState::Idle;
    reconcile_page(&mut state);
    navigation::reconcile_focus(&mut state);
    assert_eq!(state.tv.focus, Focus::Episode(30));
    state.tv.focus = Focus::Episode(34);
    move_focus(&mut state, Input::Right);
    assert_eq!(state.tv.focus, Focus::Episode(34));
    move_focus(&mut state, Input::Up);
    assert_eq!(state.tv.focus, Focus::Season(1));
  }

  #[test]
  fn a_filtered_page_without_new_cards_keeps_more_until_the_actual_end() {
    let mut state = show_state();
    state
      .full
      .as_mut()
      .expect("full")
      .detail
      .data
      .season_episodes = LoadState::Ready(page(0, 30, 90));
    state.tv.focus = Focus::DetailEpisodesMore;
    reconcile_page(&mut state);
    for (start, end) in [(30, 60), (60, 90)] {
      let mut next = page(start, end, 90);
      next.episodes.clear();
      let LoadState::Ready(loaded) = &mut state
        .full
        .as_mut()
        .expect("full")
        .detail
        .data
        .season_episodes
      else {
        panic!("page")
      };
      assert!(append_season_page(loaded, next));
      reconcile_page(&mut state);
      navigation::reconcile_focus(&mut state);
      assert_eq!(
        state.tv.focus,
        if end < 90 {
          Focus::DetailEpisodesMore
        } else {
          Focus::Episode(29)
        }
      );
    }
  }

  #[tokio::test]
  async fn episode_neighbors_use_the_shared_filtered_page_and_its_original_cursor() {
    let mut state = episode_state();
    let mut neighbors = page(0, 30, 35);
    // The shared loader excluded the current episode without changing the server cursor.
    neighbors.episodes.remove(3);
    state
      .full
      .as_mut()
      .expect("full")
      .detail
      .data
      .season_neighbors = LoadState::Ready(neighbors);
    assert_eq!(navigation::episodes(&state).len(), 29);
    assert!(!navigation::episodes(&state)
      .iter()
      .any(|item| item.id == "self"));
    state.tv.focus = Focus::DetailPlay;
    move_focus(&mut state, Input::Down);
    assert_eq!(state.tv.focus, Focus::Episode(0));
    state.tv.focus = Focus::Episode(28);
    move_focus(&mut state, Input::Right);
    assert!(matches!(
      activation(&mut state, Focus::DetailEpisodesMore).await,
      Some(AppMessage::Detail(DetailMessage::LoadMoreNeighbors))
    ));
    let Some(LoadState::Ready(loaded)) = episodes(&state) else {
      panic!("neighbors")
    };
    assert_eq!(
      next_season_page_request(loaded)
        .expect("next cursor")
        .start_index,
      30
    );
    state
      .full
      .as_mut()
      .expect("full")
      .detail
      .data
      .season_neighbors = LoadState::Failed(UiText::new("detail-season-error"));
    navigation::reconcile_focus(&mut state);
    assert_eq!(state.tv.focus, Focus::DetailEpisodesRetry);
    assert!(matches!(
      activation(&mut state, Focus::DetailEpisodesRetry).await,
      Some(AppMessage::Detail(DetailMessage::RetryNeighbors))
    ));
  }

  #[tokio::test]
  async fn labels_and_playback_intent_use_the_real_resume_target_and_disabled_guard() {
    use crate::app::message::PlaybackMessage;
    use jellypilot_mpv::playback::PlaybackStartPosition;
    use jellypilot_mpv::playback_session::PlaybackIntent;
    let mut state = episode_state();
    assert!(play_label(&state).contains("S02E03"));
    let Some(AppMessage::Playback(PlaybackMessage::Intent(intent))) =
      activation(&mut state, Focus::DetailPlay).await
    else {
      panic!("playback")
    };
    assert!(
      matches!(*intent, PlaybackIntent::Start { ref item, position: PlaybackStartPosition::Resume, .. } if item.item_id() == "self")
    );
    state.playback.view.engine_available = false;
    assert!(activation(&mut state, Focus::DetailPlay).await.is_none());
    state
      .full
      .as_mut()
      .expect("full")
      .personal_lists
      .membership_loaded = false;
    assert!(activation(&mut state, Focus::DetailWatchlist)
      .await
      .is_none());
    state.playback.view.engine_available = true;
    let LoadState::Ready(DetailContent::Item(item)) =
      &mut state.full.as_mut().expect("full").detail.data.content
    else {
      panic!("episode")
    };
    item.can_resume = false;
    let Some(AppMessage::Playback(PlaybackMessage::Intent(intent))) =
      activation(&mut state, Focus::DetailPlay).await
    else {
      panic!("playback")
    };
    assert!(matches!(
      *intent,
      PlaybackIntent::Start {
        position: PlaybackStartPosition::Beginning,
        ..
      }
    ));
  }

  #[tokio::test]
  async fn show_play_label_and_start_share_the_core_resume_duration_boundary() {
    use crate::app::message::PlaybackMessage;
    use jellypilot_mpv::playback::PlaybackStartPosition;
    use jellypilot_mpv::playback_session::PlaybackIntent;
    let mut state = show_state();
    for (runtime, position, played, resume) in [
      (Some(7200.0), 7200.0, false, false),
      (Some(7200.0), 7201.0, false, false),
      (Some(7200.0), 7199.0, false, true),
      (None, 7201.0, false, true),
      (Some(0.0), 7201.0, false, true),
      (Some(f64::NAN), 7201.0, false, true),
      (None, 7201.0, true, false),
    ] {
      let LoadState::Ready(DetailContent::Show(show)) =
        &mut state.full.as_mut().expect("full").detail.data.content
      else {
        panic!("show")
      };
      show.can_play = true;
      let episode = show.next_episode.as_mut().expect("real next episode");
      episode.runtime_seconds = runtime;
      episode.resume_position_seconds = Some(position);
      episode.played = played;
      assert_eq!(
        play_label(&state),
        state.format(
          if resume {
            "detail-continue-episode"
          } else {
            "detail-play-episode"
          },
          &[("episode", "S02E03".into())],
        ),
        "label for runtime={runtime:?}, position={position}, played={played}"
      );
      let Some(AppMessage::Playback(PlaybackMessage::Intent(intent))) =
        activation(&mut state, Focus::DetailPlay).await
      else {
        panic!("playback")
      };
      let PlaybackIntent::Start {
        item,
        position: start,
        ..
      } = *intent
      else {
        panic!("start")
      };
      assert_eq!(item.item_id(), "next");
      assert_eq!(
        matches!(start, PlaybackStartPosition::Resume),
        resume,
        "start for runtime={runtime:?}, position={position}, played={played}"
      );
    }
  }

  #[derive(Default)]
  struct Bounds {
    controls: Vec<(iced::widget::Id, iced::Rectangle)>,
    overview: Option<iced::Rectangle>,
    scroll: Option<(iced::Rectangle, iced::Rectangle, iced::Vector)>,
  }

  impl iced::advanced::widget::Operation for Bounds {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation)) {
      visit(self);
    }
    fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
      if id == Some(&iced::widget::Id::new("tv-detail-overview")) {
        self.overview = Some(bounds);
      }
      if let Some(id) = id {
        self.controls.push((id.clone(), bounds));
      }
    }
    fn scrollable(
      &mut self,
      id: Option<&iced::widget::Id>,
      bounds: iced::Rectangle,
      content: iced::Rectangle,
      translation: iced::Vector,
      _state: &mut dyn iced::advanced::widget::operation::Scrollable,
    ) {
      if id == Some(&navigation::scroll_id()) {
        self.scroll = Some((bounds, content, translation));
      }
    }
  }

  impl Bounds {
    fn control(&self, focus: Focus) -> Option<iced::Rectangle> {
      self
        .controls
        .iter()
        .find(|(id, _)| id == &navigation::focus_id(focus))
        .map(|(_, bounds)| *bounds)
    }
  }

  async fn drive_layout(
    state: &mut State,
    renderer: &mut iced::Renderer,
    mut cache: iced_runtime::user_interface::Cache,
    task: Task<AppMessage>,
  ) -> iced_runtime::user_interface::Cache {
    let mut pending = std::collections::VecDeque::from([task]);
    while let Some(task) = pending.pop_front() {
      let mut ui = iced_runtime::UserInterface::build(
        super::super::view::view(state),
        state.shell.window_size,
        cache,
        renderer,
      );
      let mut messages = Vec::new();
      if let Some(mut stream) = iced_runtime::task::into_stream(task) {
        while let Some(action) = stream.next().await {
          match action {
            iced_runtime::Action::Widget(mut operation) => {
              ui.operate(renderer, operation.as_mut());
              drop(operation.finish());
            }
            iced_runtime::Action::Output(message) => messages.push(message),
            _ => panic!("unexpected effect in detail layout"),
          }
        }
      }
      cache = ui.into_cache();
      for message in messages {
        match message {
          AppMessage::Tv(message) => pending.push_back(super::super::update(state, message)),
          AppMessage::Detail(DetailMessage::OverviewToggled) => {
            pending.push_back(crate::app::detail::update(
              &mut state.full.as_mut().expect("full").detail,
              &mut state.kernel,
              Some("show"),
              DetailMessage::OverviewToggled,
            ));
            if reconcile_page(state) {
              pending.push_back(navigation::reveal_measured(state));
            }
          }
          _ => panic!("unexpected output in detail layout"),
        }
      }
    }
    cache
  }

  fn measure(
    state: &State,
    renderer: &mut iced::Renderer,
    cache: iced_runtime::user_interface::Cache,
  ) -> (Bounds, iced_runtime::user_interface::Cache) {
    let mut ui = iced_runtime::UserInterface::build(
      super::super::view::view(state),
      state.shell.window_size,
      cache,
      renderer,
    );
    let mut bounds = Bounds::default();
    ui.operate(renderer, &mut bounds);
    (bounds, ui.into_cache())
  }

  #[tokio::test]
  async fn long_synopsis_reflows_below_the_backdrop_and_remote_reads_every_page_then_collapses() {
    use iced::advanced::{renderer, renderer::Headless};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless renderer");
    for size in [
      iced::Size::new(1920.0, 1080.0),
      iced::Size::new(1280.0, 720.0),
    ] {
      let mut state = show_state();
      state.shell.window_size = size;
      let LoadState::Ready(DetailContent::Show(show)) =
        &mut state.full.as_mut().expect("full").detail.data.content
      else {
        panic!("show")
      };
      show.overview = Some("一段足够长的中文简介与英文内容。 A translated synopsis remains complete and readable from the sofa. ".repeat(80));
      state.tv.focus = Focus::DetailOverview;
      let scale = jellypilot_ui::tv::scale(size.width);
      let (collapsed, mut cache) = measure(&state, &mut renderer, Default::default());
      assert_eq!(
        collapsed.overview.expect("collapsed synopsis").height,
        128.0 * scale
      );
      let task = navigation::activate(&mut state, Focus::DetailOverview);
      cache = drive_layout(&mut state, &mut renderer, cache, task).await;
      let (expanded, next_cache) = measure(&state, &mut renderer, cache);
      cache = next_cache;
      let copy = expanded.overview.expect("complete synopsis");
      let (viewport, _, _) = expanded.scroll.expect("detail scroll");
      assert!(copy.height > viewport.height * 3.0);
      assert!(expanded.control(Focus::DetailWatchlist).expect("actions").y >= copy.y + copy.height);
      let mut pages = 0;
      while state.tv.focus == Focus::DetailOverview && pages < 50 {
        let before = state.tv.offset;
        let task = navigation::input(&mut state, Input::Down);
        cache = drive_layout(&mut state, &mut renderer, cache, task).await;
        if state.tv.focus == Focus::DetailOverview {
          assert!(
            state.tv.offset > before,
            "each read advances instead of trapping focus"
          );
          assert!(
            state.tv.offset - before <= viewport.height * 0.7 + 1.0,
            "adjacent pages overlap"
          );
        }
        pages += 1;
      }
      assert!(pages > 3 && pages < 50);
      assert_eq!(state.tv.focus, Focus::DetailWatchlist);
      let task = navigation::input(&mut state, Input::Up);
      cache = drive_layout(&mut state, &mut renderer, cache, task).await;
      assert_eq!(state.tv.focus, Focus::DetailOverview);
      let task = navigation::activate(&mut state, Focus::DetailOverview);
      cache = drive_layout(&mut state, &mut renderer, cache, task).await;
      let (collapsed, _) = measure(&state, &mut renderer, cache);
      assert!(!overview_expanded(&state));
      assert_eq!(
        collapsed.overview.expect("collapsed synopsis").height,
        128.0 * scale
      );
      let trigger = collapsed
        .control(Focus::DetailOverview)
        .expect("collapse trigger retained");
      let (viewport, _, translation) = collapsed.scroll.expect("scroll");
      assert!(trigger.y - translation.y >= viewport.y - 1.0);
      assert!(trigger.y + trigger.height - translation.y <= viewport.y + viewport.height + 1.0);
    }
  }

  #[tokio::test]
  async fn rendered_loading_and_empty_details_do_not_expose_retry_or_phantom_cards() {
    use iced::advanced::{renderer, renderer::Headless};
    let mut renderer = iced::Renderer::new(renderer::Settings::default(), Some("tiny-skia"))
      .await
      .expect("headless renderer");
    let mut state = show_state();
    state.shell.window_size = iced::Size::new(1920.0, 1080.0);
    for load in [
      LoadState::Loading,
      LoadState::Ready(page(0, 0, 0)),
      LoadState::Failed(UiText::new("detail-season-error")),
    ] {
      let failed = matches!(load, LoadState::Failed(_));
      state
        .full
        .as_mut()
        .expect("full")
        .detail
        .data
        .season_episodes = load;
      let (bounds, _) = measure(&state, &mut renderer, Default::default());
      assert!(bounds.control(Focus::DetailPlay).is_none());
      assert!(bounds.control(Focus::Episode(0)).is_none());
      assert!(bounds.control(Focus::DetailEpisodesMore).is_none());
      assert_eq!(bounds.control(Focus::DetailEpisodesRetry).is_some(), failed);
      assert!(bounds.control(Focus::Retry).is_none());
    }
    for content in [
      LoadState::Loading,
      LoadState::Failed(UiText::new("detail-load-error")),
    ] {
      let failed = matches!(content, LoadState::Failed(_));
      state.full.as_mut().expect("full").detail.data.content = content;
      let (bounds, _) = measure(&state, &mut renderer, Default::default());
      assert_eq!(bounds.control(Focus::Retry).is_some(), failed);
      assert!(bounds.control(Focus::DetailBack).is_some());
    }
  }
}
