use std::collections::HashMap;
use std::sync::Arc;

use jellypilot_media_server::{
  JellyfinClient, VideoDetailMetadata, VideoItemDetail, VideoItemStreams, VideoLibraryItem,
  VideoSeason, VideoSeasonEpisodesPage, VideoSeasonEpisodesPageRequest, VideoShowDetail,
  VideoUserDataAction, VideoUserDataUpdate,
};
use relm4::adw::prelude::*;
use relm4::{adw, gtk, Sender};

use crate::artwork::DecodedArtwork;
use crate::artwork_binder::{ArtworkBinder, ArtworkSlot, ArtworkSurface};
use crate::pages::cards::{
  apply_decoded_artwork, backdrop_artwork, clear_box, dim_label, loading_view, poster_card,
  register_artwork, row_card, scrolled_page, state_view, ArtworkTarget,
};
use crate::pages::LoadState;
use crate::playback::{Playable, PlaybackStartPosition};
use crate::request_gate::{DetailAuxKind, DetailAuxToken, DetailToken, RequestGate};
use crate::shell::AppMessage;

pub(crate) const SEASON_EPISODE_PAGE_SIZE: i32 = 30;

#[derive(Clone)]
pub(crate) enum DetailContent {
  Item(VideoItemDetail),
  Show(VideoShowDetail),
}

#[derive(Clone)]
struct SeasonSelection {
  season: VideoSeason,
  episodes: LoadState<VideoSeasonEpisodesPage>,
  requested_start_index: i32,
}

#[derive(Clone)]
struct DetailParent {
  content: DetailContent,
  season: Option<SeasonSelection>,
}

pub(crate) struct DetailPage {
  root: gtk::Widget,
  content: gtk::Box,
  sender: Sender<AppMessage>,
  detail: LoadState<DetailContent>,
  detail_selection: Option<VideoLibraryItem>,
  detail_origin: Option<String>,
  detail_parent: Option<DetailParent>,
  streams: LoadState<VideoItemStreams>,
  season_neighbors: LoadState<Vec<VideoLibraryItem>>,
  season: Option<SeasonSelection>,
  recommendations: LoadState<Vec<VideoLibraryItem>>,
  user_data_busy: bool,
  user_data_error: Option<String>,
  artwork_targets: HashMap<ArtworkSlot, ArtworkTarget>,
}

pub(crate) struct DetailContext<'a> {
  pub gate: &'a mut RequestGate,
  pub binder: &'a mut ArtworkBinder,
  pub playback_enabled: bool,
  pub current_page: Option<String>,
}

#[derive(Debug)]
pub(crate) enum Message {
  Open(VideoLibraryItem),
  Retry,
  Back,
  SelectSeason(VideoSeason),
  PreviousSeasonEpisodePage,
  NextSeasonEpisodePage,
  RetrySeason,
  BackFromSeason,
  UpdateUserData {
    item_id: String,
    action: VideoUserDataAction,
  },
  PlayDetail(VideoItemDetail, PlaybackStartPosition),
  PlayLibrary(VideoLibraryItem, PlaybackStartPosition),
}

pub(crate) enum DetailEvent {
  Loaded {
    token: DetailToken,
    result: Box<Result<DetailContent, String>>,
  },
  Recommendations {
    token: DetailAuxToken,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  Streams {
    token: DetailAuxToken,
    result: Result<VideoItemStreams, String>,
  },
  SeasonNeighbors {
    token: DetailAuxToken,
    result: Result<Vec<VideoLibraryItem>, String>,
  },
  SeasonEpisodes {
    token: DetailToken,
    season_id: String,
    result: Result<VideoSeasonEpisodesPage, String>,
  },
  UserData {
    token: DetailAuxToken,
    result: Result<VideoUserDataUpdate, String>,
  },
}

pub(crate) enum DetailEffect {
  BeginArtworkView,
  ArtworkLoad {
    surface: ArtworkSurface,
    slot: ArtworkSlot,
    image_id: String,
  },
  DetailLoad {
    token: DetailToken,
    item: VideoLibraryItem,
  },
  Recommendations {
    token: DetailAuxToken,
    item_id: String,
  },
  Streams {
    token: DetailAuxToken,
    item_id: String,
  },
  SeasonNeighbors {
    token: DetailAuxToken,
    item_id: String,
    series_id: String,
    season_number: i32,
  },
  SeasonPage {
    token: DetailToken,
    season_id: String,
    request: VideoSeasonEpisodesPageRequest,
  },
  UserDataAction {
    token: DetailAuxToken,
    item_id: String,
    action: VideoUserDataAction,
  },
  PlayItem(Playable, PlaybackStartPosition),
  ShowDetail,
  Back {
    origin: String,
  },
  Render,
}

impl DetailPage {
  pub(crate) fn build(sender: &Sender<AppMessage>) -> Self {
    let content = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(12)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(24)
      .margin_end(24)
      .build();
    let root = scrolled_page("Item Details", "", &content);
    Self {
      root,
      content,
      sender: sender.clone(),
      detail: LoadState::Idle,
      detail_selection: None,
      detail_origin: None,
      detail_parent: None,
      streams: LoadState::Idle,
      season_neighbors: LoadState::Idle,
      season: None,
      recommendations: LoadState::Idle,
      user_data_busy: false,
      user_data_error: None,
      artwork_targets: HashMap::new(),
    }
  }

  pub(crate) fn root(&self) -> &gtk::Widget {
    &self.root
  }

  pub(crate) fn reset(&mut self) {
    self.detail = LoadState::Idle;
    self.detail_selection = None;
    self.detail_origin = None;
    self.detail_parent = None;
    self.streams = LoadState::Idle;
    self.season_neighbors = LoadState::Idle;
    self.season = None;
    self.recommendations = LoadState::Idle;
    self.user_data_busy = false;
    self.user_data_error = None;
    self.artwork_targets.clear();
  }

  pub(crate) fn handle(
    &mut self,
    message: Message,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    match message {
      Message::Open(item) => self.load_detail(item, cx),
      Message::Retry => self.retry_detail(cx),
      Message::Back => self.back_from_detail(cx),
      Message::SelectSeason(season) => self.load_season(season, cx),
      Message::PreviousSeasonEpisodePage => self.change_season_episode_page(-1, cx),
      Message::NextSeasonEpisodePage => self.change_season_episode_page(1, cx),
      Message::RetrySeason => self.retry_season(cx),
      Message::BackFromSeason => {
        cx.gate.cancel_detail_loads();
        self.season = None;
        vec![DetailEffect::Render]
      }
      Message::UpdateUserData { item_id, action } => {
        self.start_user_data_update(item_id, action, cx)
      }
      Message::PlayDetail(item, position) => {
        vec![DetailEffect::PlayItem(Playable::from(item), position)]
      }
      Message::PlayLibrary(item, position) => {
        vec![DetailEffect::PlayItem(Playable::from(item), position)]
      }
    }
  }

  pub(crate) fn handle_event(
    &mut self,
    event: DetailEvent,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    match event {
      DetailEvent::Loaded { token, result } => {
        if !cx.gate.finish_detail(token) {
          return Vec::new();
        }
        self.detail = match *result {
          Ok(detail) => LoadState::Ready(detail),
          Err(message) => LoadState::Failed(message),
        };
        vec![DetailEffect::Render]
      }
      DetailEvent::Recommendations { token, result } => {
        if !cx.gate.finish_detail_aux(token) {
          return Vec::new();
        }
        self.recommendations = match result {
          Ok(items) => LoadState::Ready(items),
          Err(message) => LoadState::Failed(message),
        };
        vec![DetailEffect::Render]
      }
      DetailEvent::Streams { token, result } => {
        if !cx.gate.finish_detail_aux(token) {
          return Vec::new();
        }
        self.streams = match result {
          Ok(streams) => LoadState::Ready(streams),
          Err(message) => LoadState::Failed(message),
        };
        vec![DetailEffect::Render]
      }
      DetailEvent::SeasonNeighbors { token, result } => {
        if !cx.gate.finish_detail_aux(token) {
          return Vec::new();
        }
        self.season_neighbors = match result {
          Ok(items) => LoadState::Ready(items),
          Err(message) => LoadState::Failed(message),
        };
        vec![DetailEffect::Render]
      }
      DetailEvent::SeasonEpisodes {
        token,
        season_id,
        result,
      } => {
        if !cx.gate.finish_detail(token) {
          return Vec::new();
        }
        let Some(selection) = self
          .season
          .as_mut()
          .filter(|selection| selection.season.id == season_id)
        else {
          return Vec::new();
        };
        selection.episodes = match result {
          Ok(episodes) => LoadState::Ready(episodes),
          Err(message) => LoadState::Failed(message),
        };
        vec![DetailEffect::Render]
      }
      DetailEvent::UserData { token, result } => self.finish_user_data_update(token, result, cx),
    }
  }

  pub(crate) fn render(&mut self, cx: &mut DetailContext<'_>) -> Vec<DetailEffect> {
    cx.binder.begin_view(ArtworkSurface::Detail);
    self.artwork_targets.clear();
    let mut effects = vec![DetailEffect::BeginArtworkView];
    clear_box(&self.content);
    let back = gtk::Button::with_label("Back");
    let sender = self.sender.clone();
    back.connect_clicked(move |_| sender.emit(AppMessage::Detail(Message::Back)));
    self.content.append(&back);
    if let Some(message) = &self.user_data_error {
      let status = dim_label(message);
      status.set_accessible_role(gtk::AccessibleRole::Status);
      status.set_wrap(true);
      self.content.append(&status);
    }
    match &self.detail {
      LoadState::Idle => self.content.append(&state_view(
        "Select an item",
        "Choose a movie or episode to inspect its details.",
        "view-more-symbolic",
      )),
      LoadState::Loading => self.content.append(&loading_view("Loading details…")),
      LoadState::Failed(message) => {
        self.content.append(&state_view(
          "Details could not load",
          message.as_str(),
          "dialog-error-symbolic",
        ));
        let retry = gtk::Button::with_label("Retry");
        retry.set_sensitive(self.detail_selection.is_some());
        let sender = self.sender.clone();
        retry.connect_clicked(move |_| sender.emit(AppMessage::Detail(Message::Retry)));
        self.content.append(&retry);
      }
      LoadState::Ready(DetailContent::Item(detail)) => {
        let detail = detail.clone();
        let view = self.detail_view(&detail, cx, &mut effects);
        self.content.append(&view);
      }
      LoadState::Ready(DetailContent::Show(detail)) => {
        let detail = detail.clone();
        let view = self.show_detail_view(&detail, cx, &mut effects);
        self.content.append(&view);
      }
    }
    effects
  }

  pub(crate) fn apply_artwork(&mut self, slot: ArtworkSlot, decoded: DecodedArtwork) -> bool {
    apply_decoded_artwork(&mut self.artwork_targets, slot, decoded)
  }

  fn load_detail(
    &mut self,
    item: VideoLibraryItem,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    self.invalidate_user_data_update(cx);
    if cx.current_page.as_deref() != Some("detail") {
      self.detail_origin = cx.current_page.clone();
      self.detail_parent = None;
    } else if let LoadState::Ready(content @ DetailContent::Show(_)) = &self.detail {
      self.detail_parent = Some(DetailParent {
        content: content.clone(),
        season: self.season.clone(),
      });
    }
    self.detail_selection = Some(item.clone());
    self.season = None;
    cx.gate.set_detail_item(Some(item.id.clone()));
    self.recommendations = LoadState::Loading;
    self.streams = LoadState::Loading;
    let season_neighbor_request = item
      .series_id
      .clone()
      .zip(item.season_number)
      .filter(|_| item.item_type.eq_ignore_ascii_case("episode"));
    self.season_neighbors = if season_neighbor_request.is_some() {
      LoadState::Loading
    } else {
      LoadState::Idle
    };
    let token = cx.gate.begin_detail();
    let recommendation_token = cx.gate.begin_detail_aux(DetailAuxKind::Recommendations);
    let stream_token = cx.gate.begin_detail_aux(DetailAuxKind::Streams);
    let season_neighbor_token = cx.gate.begin_detail_aux(DetailAuxKind::SeasonNeighbors);
    self.detail = LoadState::Loading;
    let mut effects = vec![
      DetailEffect::ShowDetail,
      DetailEffect::Render,
      DetailEffect::DetailLoad {
        token,
        item: item.clone(),
      },
    ];
    if let Some(token) = recommendation_token {
      effects.push(DetailEffect::Recommendations {
        token,
        item_id: item.id.clone(),
      });
    }
    if let Some(token) = stream_token {
      effects.push(DetailEffect::Streams {
        token,
        item_id: item.id.clone(),
      });
    }
    if let (Some((series_id, season_number)), Some(token)) =
      (season_neighbor_request, season_neighbor_token)
    {
      effects.push(DetailEffect::SeasonNeighbors {
        token,
        item_id: item.id.clone(),
        series_id,
        season_number,
      });
    }
    effects
  }

  fn retry_detail(&mut self, cx: &mut DetailContext<'_>) -> Vec<DetailEffect> {
    let Some(item) = self.detail_selection.clone() else {
      return Vec::new();
    };
    self.load_detail(item, cx)
  }

  fn back_from_detail(&mut self, cx: &mut DetailContext<'_>) -> Vec<DetailEffect> {
    self.invalidate_user_data_update(cx);
    if let Some(parent) = self.detail_parent.take() {
      cx.gate.cancel_detail_loads();
      self.detail = LoadState::Ready(parent.content);
      self.season = parent.season;
      cx.gate
        .set_detail_item(self.current_detail_identity().map(str::to_owned));
      self.recommendations = LoadState::Loading;
      self.streams = LoadState::Idle;
      self.season_neighbors = LoadState::Idle;
      let _ = cx.gate.begin_detail_aux(DetailAuxKind::Streams);
      let _ = cx.gate.begin_detail_aux(DetailAuxKind::SeasonNeighbors);
      let mut effects = vec![DetailEffect::Render];
      if let Some(token) = cx.gate.begin_detail_aux(DetailAuxKind::Recommendations) {
        let item_id = self
          .current_detail_identity()
          .expect("parent detail item was just recorded")
          .to_owned();
        effects.push(DetailEffect::Recommendations { token, item_id });
      }
      return effects;
    }
    if self.season.is_some() {
      cx.gate.cancel_detail_loads();
      self.season = None;
      return vec![DetailEffect::Render];
    }
    let origin = self
      .detail_origin
      .clone()
      .unwrap_or_else(|| "home".to_owned());
    vec![DetailEffect::Back { origin }]
  }

  fn invalidate_user_data_update(&mut self, cx: &mut DetailContext<'_>) {
    cx.gate.invalidate_detail_aux(DetailAuxKind::UserData);
    self.user_data_busy = false;
    self.user_data_error = None;
  }

  fn start_user_data_update(
    &mut self,
    item_id: String,
    action: VideoUserDataAction,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    if self.user_data_busy || self.current_detail_item_id() != Some(item_id.as_str()) {
      return Vec::new();
    }
    let Some(token) = cx.gate.begin_detail_aux(DetailAuxKind::UserData) else {
      return Vec::new();
    };
    self.user_data_busy = true;
    self.user_data_error = None;
    vec![
      DetailEffect::Render,
      DetailEffect::UserDataAction {
        token,
        item_id,
        action,
      },
    ]
  }

  fn finish_user_data_update(
    &mut self,
    token: DetailAuxToken,
    result: Result<VideoUserDataUpdate, String>,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    if !cx.gate.finish_detail_aux(token) {
      return Vec::new();
    }
    self.user_data_busy = false;
    match result {
      Ok(update) => {
        let updated = apply_user_data_update(&mut self.detail, &update);
        debug_assert!(
          updated,
          "current detail identity was checked before applying user data"
        );
        if let Some(selection) = self
          .detail_selection
          .as_mut()
          .filter(|selection| selection.id == update.item_id)
        {
          selection.played = update.played;
          selection.favorite = update.favorite;
        }
        self.user_data_error = None;
      }
      Err(message) => self.user_data_error = Some(message),
    }
    vec![DetailEffect::Render]
  }

  fn current_detail_item_id(&self) -> Option<&str> {
    self.current_detail_identity()
  }

  fn current_detail_identity(&self) -> Option<&str> {
    match &self.detail {
      LoadState::Ready(DetailContent::Item(detail)) => Some(detail.id.as_str()),
      LoadState::Ready(DetailContent::Show(detail)) => Some(detail.id.as_str()),
      _ => None,
    }
  }

  fn load_season(&mut self, season: VideoSeason, cx: &mut DetailContext<'_>) -> Vec<DetailEffect> {
    self.load_season_page(season, 0, cx)
  }

  fn load_season_page(
    &mut self,
    season: VideoSeason,
    start_index: i32,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    let Some(series_id) = self.current_show().map(|detail| detail.id.clone()) else {
      return Vec::new();
    };
    let start_index = start_index.max(0);
    let token = cx.gate.begin_detail();
    let season_id = season.id.clone();
    let request = season_page_request(&series_id, &season, start_index);
    self.season = Some(SeasonSelection {
      season,
      episodes: LoadState::Loading,
      requested_start_index: start_index,
    });
    vec![
      DetailEffect::Render,
      DetailEffect::SeasonPage {
        token,
        season_id,
        request,
      },
    ]
  }

  fn retry_season(&mut self, cx: &mut DetailContext<'_>) -> Vec<DetailEffect> {
    let Some((season, start_index)) = self
      .season
      .as_ref()
      .map(|selection| (selection.season.clone(), selection.requested_start_index))
    else {
      return Vec::new();
    };
    self.load_season_page(season, start_index, cx)
  }

  fn change_season_episode_page(
    &mut self,
    direction: i8,
    cx: &mut DetailContext<'_>,
  ) -> Vec<DetailEffect> {
    let Some(selection) = self.season.as_ref() else {
      return Vec::new();
    };
    let LoadState::Ready(page) = &selection.episodes else {
      return Vec::new();
    };
    let next_start_index = if direction < 0 {
      if page.start_index <= 0 {
        return Vec::new();
      }
      page.start_index.saturating_sub(page.limit.max(1))
    } else {
      if !page.has_more || page.next_start_index <= page.start_index {
        return Vec::new();
      }
      page.next_start_index
    };
    let season = selection.season.clone();
    self.load_season_page(season, next_start_index, cx)
  }

  fn current_show(&self) -> Option<&VideoShowDetail> {
    match &self.detail {
      LoadState::Ready(DetailContent::Show(detail)) => Some(detail),
      _ => None,
    }
  }

  fn detail_view(
    &mut self,
    detail: &VideoItemDetail,
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let backdrop_container = gtk::Overlay::new();
    let (backdrop_artwork, artwork) = backdrop_artwork(
      detail
        .backdrop_image_id
        .as_deref()
        .or(detail.artwork_image_id.as_deref()),
    );
    self.push_artwork(cx.binder, artwork, effects);
    backdrop_container.set_child(Some(&backdrop_artwork));
    let gradient = gtk::Box::new(gtk::Orientation::Vertical, 0);
    gradient.add_css_class("osd");
    gradient.set_hexpand(true);
    gradient.set_vexpand(true);
    gradient.set_valign(gtk::Align::End);
    let info = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(8)
      .margin_top(20)
      .margin_bottom(20)
      .margin_start(24)
      .margin_end(24)
      .build();
    let title = gtk::Label::new(Some(&detail.name));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_wrap(true);
    info.append(&title);
    let metadata = dim_label(&detail_metadata(detail));
    metadata.set_wrap(true);
    info.append(&metadata);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let play = gtk::Button::with_label("Play");
    play.add_css_class("suggested-action");
    let item = detail.clone();
    let sender = self.sender.clone();
    play.connect_clicked(move |_| {
      sender.emit(AppMessage::Detail(Message::PlayDetail(
        item.clone(),
        PlaybackStartPosition::Beginning,
      )))
    });
    play.set_sensitive(cx.playback_enabled && detail.can_play);
    actions.append(&play);
    if detail.can_resume {
      let resume = gtk::Button::with_label("Resume");
      let item = detail.clone();
      let sender = self.sender.clone();
      resume.connect_clicked(move |_| {
        sender.emit(AppMessage::Detail(Message::PlayDetail(
          item.clone(),
          PlaybackStartPosition::Resume,
        )))
      });
      resume.set_sensitive(cx.playback_enabled);
      actions.append(&resume);
    }
    actions.append(&self.user_data_controls(&detail.id, detail.played, detail.favorite));
    info.append(&actions);
    gradient.append(&info);
    backdrop_container.add_overlay(&gradient);
    column.append(&backdrop_container);
    let body = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(12)
      .margin_top(16)
      .margin_start(24)
      .margin_end(24)
      .margin_bottom(24)
      .build();
    if let Some(overview) = &detail.overview {
      let overview_label = gtk::Label::new(Some("Synopsis"));
      overview_label.add_css_class("heading");
      overview_label.set_xalign(0.0);
      body.append(&overview_label);
      let overview = gtk::Label::new(Some(overview));
      overview.set_xalign(0.0);
      overview.set_wrap(true);
      overview.set_selectable(true);
      body.append(&overview);
    }
    if let Some(metadata) = detail_metadata_section(&detail.metadata, &detail.genres) {
      body.append(&metadata);
    }
    body.append(&self.stream_metadata_view());
    if let Some(neighbors) = self.season_neighbors_view(detail, cx, effects) {
      body.append(&neighbors);
    }
    if let Some(recommendations) = self.recommendations_view(cx, effects) {
      body.append(&recommendations);
    }
    column.append(&body);
    column.upcast()
  }

  fn show_detail_view(
    &mut self,
    detail: &VideoShowDetail,
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let backdrop_container = gtk::Overlay::new();
    let (backdrop_artwork, artwork) = backdrop_artwork(
      detail
        .backdrop_image_id
        .as_deref()
        .or(detail.artwork_image_id.as_deref()),
    );
    self.push_artwork(cx.binder, artwork, effects);
    backdrop_container.set_child(Some(&backdrop_artwork));
    let gradient = gtk::Box::new(gtk::Orientation::Vertical, 0);
    gradient.add_css_class("osd");
    gradient.set_hexpand(true);
    gradient.set_vexpand(true);
    gradient.set_valign(gtk::Align::End);
    let info = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(8)
      .margin_top(20)
      .margin_bottom(20)
      .margin_start(24)
      .margin_end(24)
      .build();
    let title = gtk::Label::new(Some(&detail.name));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_wrap(true);
    info.append(&title);
    let metadata = dim_label(&show_detail_metadata(detail));
    metadata.set_wrap(true);
    info.append(&metadata);
    info.append(&self.user_data_controls(&detail.id, detail.played, detail.favorite));
    gradient.append(&info);
    backdrop_container.add_overlay(&gradient);
    column.append(&backdrop_container);
    let body = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(12)
      .margin_top(16)
      .margin_start(24)
      .margin_end(24)
      .margin_bottom(24)
      .build();
    if let Some(overview) = &detail.overview {
      let overview_label = gtk::Label::new(Some("Synopsis"));
      overview_label.add_css_class("heading");
      overview_label.set_xalign(0.0);
      body.append(&overview_label);
      let overview = gtk::Label::new(Some(overview));
      overview.set_xalign(0.0);
      overview.set_wrap(true);
      overview.set_selectable(true);
      body.append(&overview);
    }
    if let Some(episode) = &detail.next_episode {
      let heading = gtk::Label::new(Some("Next Episode"));
      heading.add_css_class("heading");
      heading.set_xalign(0.0);
      body.append(&heading);
      body.append(&self.media_button(episode, false, cx, effects));
    }
    let seasons_heading = gtk::Label::new(Some("Seasons"));
    seasons_heading.add_css_class("heading");
    seasons_heading.set_xalign(0.0);
    body.append(&seasons_heading);
    if detail.seasons.is_empty() {
      body.append(&dim_label("No seasons are available."));
    } else {
      let seasons = gtk::ListBox::new();
      seasons.set_selection_mode(gtk::SelectionMode::Single);
      seasons.set_activate_on_single_click(true);
      let selected_season_id = self
        .season
        .as_ref()
        .map(|selection| selection.season.id.as_str());
      let available_seasons = detail.seasons.clone();
      seasons.connect_row_activated({
        let sender = self.sender.clone();
        move |_, row| {
          let Ok(index) = usize::try_from(row.index()) else {
            return;
          };
          if let Some(season) = available_seasons.get(index) {
            sender.emit(AppMessage::Detail(Message::SelectSeason(season.clone())));
          }
        }
      });
      for season in &detail.seasons {
        let row = adw::ActionRow::new();
        row.set_title(&season.name);
        row.set_subtitle(
          &season
            .season_number
            .map(|number| format!("Season {number}"))
            .unwrap_or_else(|| "Season".to_owned()),
        );
        row.set_activatable(true);
        row.set_tooltip_text(Some(&format!("Browse episodes in {}", season.name)));
        seasons.append(&row);
        if selected_season_id == Some(season.id.as_str()) {
          seasons.select_row(Some(&row));
        }
      }
      body.append(&seasons);
    }
    if let Some(metadata) = detail_metadata_section(&detail.metadata, &detail.genres) {
      body.append(&metadata);
    }
    if let Some(recommendations) = self.recommendations_view(cx, effects) {
      body.append(&recommendations);
    }
    if let Some(selection) = self.season.clone() {
      let section = self.season_episodes_view(&selection, cx, effects);
      body.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
      body.append(&section);
    }
    column.append(&body);
    column.upcast()
  }

  fn stream_metadata_view(&self) -> gtk::Widget {
    match &self.streams {
      LoadState::Idle => stream_metadata_status(),
      LoadState::Loading => loading_view("Loading audio and subtitle metadata…"),
      LoadState::Failed(message) => state_view(
        "Stream metadata unavailable",
        message,
        "dialog-warning-symbolic",
      ),
      LoadState::Ready(streams) => {
        let audio = streams.audio_streams.len();
        let subtitles = streams.subtitle_streams.len();
        state_view(
          "Audio and subtitles",
          &format!("{audio} audio stream(s) · {subtitles} subtitle stream(s) available."),
          "audio-x-generic-symbolic",
        )
      }
    }
  }

  fn season_neighbors_view(
    &mut self,
    detail: &VideoItemDetail,
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> Option<gtk::Widget> {
    let season_number = detail.season_number?;
    match self.season_neighbors.clone() {
      LoadState::Idle => None,
      LoadState::Loading => Some(loading_view(&format!(
        "Loading more from Season {season_number}…"
      ))),
      LoadState::Failed(message) => Some(state_view(
        "Season episodes unavailable",
        &message,
        "dialog-warning-symbolic",
      )),
      LoadState::Ready(items) if items.is_empty() => None,
      LoadState::Ready(items) => Some(self.media_shelf(
        &format!("More from Season {season_number}"),
        &items,
        cx,
        effects,
      )),
    }
  }

  fn recommendations_view(
    &mut self,
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> Option<gtk::Widget> {
    match self.recommendations.clone() {
      LoadState::Idle => None,
      LoadState::Loading => Some(loading_view("Loading recommendations…")),
      LoadState::Failed(message) => Some(state_view(
        "Recommendations unavailable",
        &message,
        "dialog-warning-symbolic",
      )),
      LoadState::Ready(items) if items.is_empty() => None,
      LoadState::Ready(items) => Some(self.media_shelf("More like this", &items, cx, effects)),
    }
  }

  fn user_data_controls(&self, item_id: &str, played: bool, favorite: bool) -> gtk::Box {
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let favorite_action = if favorite {
      VideoUserDataAction::Unfavorite
    } else {
      VideoUserDataAction::Favorite
    };
    let favorite_button = gtk::Button::with_label(if favorite {
      "Remove from Favorites"
    } else {
      "Add to Favorites"
    });
    favorite_button.set_sensitive(!self.user_data_busy);
    let favorite_id = item_id.to_owned();
    let favorite_sender = self.sender.clone();
    favorite_button.connect_clicked(move |_| {
      favorite_sender.emit(AppMessage::Detail(Message::UpdateUserData {
        item_id: favorite_id.clone(),
        action: favorite_action,
      }))
    });
    controls.append(&favorite_button);

    let played_action = if played {
      VideoUserDataAction::MarkUnplayed
    } else {
      VideoUserDataAction::MarkPlayed
    };
    let played_button = gtk::Button::with_label(if played {
      "Mark Unwatched"
    } else {
      "Mark Watched"
    });
    played_button.set_sensitive(!self.user_data_busy);
    let played_id = item_id.to_owned();
    let played_sender = self.sender.clone();
    played_button.connect_clicked(move |_| {
      played_sender.emit(AppMessage::Detail(Message::UpdateUserData {
        item_id: played_id.clone(),
        action: played_action,
      }))
    });
    controls.append(&played_button);
    controls
  }

  fn season_episodes_view(
    &mut self,
    selection: &SeasonSelection,
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let heading_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let heading = gtk::Label::new(Some(&selection.season.name));
    heading.add_css_class("title-3");
    heading.set_hexpand(true);
    heading.set_xalign(0.0);
    let all_seasons = gtk::Button::with_label("All seasons");
    let sender = self.sender.clone();
    all_seasons.connect_clicked(move |_| sender.emit(AppMessage::Detail(Message::BackFromSeason)));
    heading_row.append(&heading);
    heading_row.append(&all_seasons);
    section.append(&heading_row);
    match &selection.episodes {
      LoadState::Idle | LoadState::Loading => {
        section.append(&loading_view("Loading episodes…"));
      }
      LoadState::Failed(message) => {
        section.append(&state_view(
          "Episodes could not load",
          message,
          "dialog-error-symbolic",
        ));
        let retry = gtk::Button::with_label("Retry");
        let sender = self.sender.clone();
        retry.connect_clicked(move |_| sender.emit(AppMessage::Detail(Message::RetrySeason)));
        section.append(&retry);
      }
      LoadState::Ready(page) if page.episodes.is_empty() => {
        let (title, message) = if page.total_record_count == 0 {
          (
            "No episodes available",
            "This season does not contain any visible episodes.",
          )
        } else {
          (
            "No episodes on this page",
            "The server returned no visible episodes for this page.",
          )
        };
        section.append(&state_view(title, message, "folder-videos-symbolic"));
        let can_go_previous = page.start_index > 0;
        let can_go_next = page.has_more && page.next_start_index > page.start_index;
        if can_go_previous || can_go_next {
          let navigation = gtk::Box::new(gtk::Orientation::Horizontal, 8);
          navigation.set_halign(gtk::Align::Center);
          let previous = gtk::Button::with_label("Previous episode page");
          previous.set_sensitive(can_go_previous);
          let previous_sender = self.sender.clone();
          previous.connect_clicked(move |_| {
            previous_sender.emit(AppMessage::Detail(Message::PreviousSeasonEpisodePage));
          });
          let next = gtk::Button::with_label("Next episode page");
          next.set_sensitive(can_go_next);
          let next_sender = self.sender.clone();
          next.connect_clicked(move |_| {
            next_sender.emit(AppMessage::Detail(Message::NextSeasonEpisodePage));
          });
          navigation.append(&previous);
          navigation.append(&next);
          section.append(&navigation);
        }
      }
      LoadState::Ready(page) => {
        let start = page.start_index.max(0);
        let end = page
          .next_start_index
          .max(start)
          .min(page.total_record_count);
        let pagination = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let previous = gtk::Button::from_icon_name("go-previous-symbolic");
        previous.set_tooltip_text(Some("Previous episode page"));
        previous.update_property(&[gtk::accessible::Property::Label("Previous episode page")]);
        previous.set_sensitive(start > 0);
        let sender = self.sender.clone();
        previous.connect_clicked(move |_| {
          sender.emit(AppMessage::Detail(Message::PreviousSeasonEpisodePage))
        });
        let page_status = gtk::Label::new(Some(&format!(
          "Episodes {}–{} of {}",
          start.saturating_add(1),
          end,
          page.total_record_count,
        )));
        page_status.set_hexpand(true);
        page_status.set_xalign(0.5);
        let next = gtk::Button::from_icon_name("go-next-symbolic");
        next.set_tooltip_text(Some("Next episode page"));
        next.update_property(&[gtk::accessible::Property::Label("Next episode page")]);
        next.set_sensitive(page.has_more);
        let sender = self.sender.clone();
        next.connect_clicked(move |_| {
          sender.emit(AppMessage::Detail(Message::NextSeasonEpisodePage))
        });
        pagination.append(&previous);
        pagination.append(&page_status);
        pagination.append(&next);
        section.append(&pagination);
        section.append(&self.media_list(&page.episodes, cx, effects));
      }
    }
    section.upcast()
  }

  fn media_shelf(
    &mut self,
    title: &str,
    items: &[VideoLibraryItem],
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("title-2");
    title_label.set_xalign(0.0);
    section.append(&title_label);
    if items.is_empty() {
      section.append(&dim_label("Nothing available."));
      return section.upcast();
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    for item in items {
      row.append(&self.media_button(item, true, cx, effects));
    }
    let scroll = gtk::ScrolledWindow::builder()
      .child(&row)
      .hscrollbar_policy(gtk::PolicyType::Automatic)
      .vscrollbar_policy(gtk::PolicyType::Never)
      .propagate_natural_width(true)
      .build();
    section.append(&scroll);
    section.upcast()
  }

  fn media_list(
    &mut self,
    items: &[VideoLibraryItem],
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    for item in items {
      list.append(&self.media_button(item, false, cx, effects));
    }
    list.upcast()
  }

  fn media_button(
    &mut self,
    item: &VideoLibraryItem,
    compact: bool,
    cx: &mut DetailContext<'_>,
    effects: &mut Vec<DetailEffect>,
  ) -> gtk::Widget {
    let sender = self.sender.clone();
    let play_sender = self.sender.clone();
    let select_item = item.clone();
    let play_item = item.clone();
    let (widget, artwork) = if compact {
      poster_card(item, move || {
        sender.emit(AppMessage::Detail(Message::Open(select_item.clone())));
      })
    } else {
      row_card(
        item,
        cx.playback_enabled,
        move || {
          sender.emit(AppMessage::Detail(Message::Open(select_item.clone())));
        },
        move |position| {
          play_sender.emit(AppMessage::Detail(Message::PlayLibrary(
            play_item.clone(),
            position,
          )));
        },
      )
    };
    self.push_artwork(cx.binder, artwork, effects);
    widget
  }

  fn push_artwork(
    &mut self,
    binder: &mut ArtworkBinder,
    artwork: Option<crate::pages::cards::ArtworkBind>,
    effects: &mut Vec<DetailEffect>,
  ) {
    if let Some((slot, image_id)) = register_artwork(
      &mut self.artwork_targets,
      binder,
      ArtworkSurface::Detail,
      artwork,
    ) {
      effects.push(DetailEffect::ArtworkLoad {
        surface: ArtworkSurface::Detail,
        slot,
        image_id,
      });
    }
  }
}

pub(crate) async fn load_detail_content(
  client: Arc<JellyfinClient>,
  item: VideoLibraryItem,
) -> Result<DetailContent, String> {
  if item.item_type.eq_ignore_ascii_case("series") {
    client
      .library()
      .show_detail(item.id)
      .await
      .map(DetailContent::Show)
      .map_err(|error| error.to_string())
  } else {
    client
      .library()
      .item_detail(item.id)
      .await
      .map(DetailContent::Item)
      .map_err(|error| error.to_string())
  }
}

pub(crate) async fn load_season_neighbors(
  client: Arc<JellyfinClient>,
  item_id: String,
  series_id: String,
  season_number: i32,
) -> Result<Vec<VideoLibraryItem>, String> {
  client
    .library()
    .season_episodes_page(VideoSeasonEpisodesPageRequest {
      series_id,
      season_id: None,
      season_number: Some(season_number),
      start_index: 0,
      limit: SEASON_EPISODE_PAGE_SIZE,
    })
    .await
    .map(|page| {
      page
        .episodes
        .into_iter()
        .filter(|episode| episode.id != item_id)
        .collect()
    })
    .map_err(|error| error.to_string())
}

fn detail_metadata_section(
  metadata: &VideoDetailMetadata,
  genres: &[String],
) -> Option<gtk::Widget> {
  let rating = match (&metadata.community_rating, &metadata.official_rating) {
    (Some(community), Some(official)) => format!("Community rating {community:.1} · {official}"),
    (Some(community), None) => format!("Community rating {community:.1}"),
    (None, Some(official)) => official.clone(),
    (None, None) => String::new(),
  };
  if rating.is_empty()
    && genres.is_empty()
    && metadata.creators.is_empty()
    && metadata.cast.is_empty()
  {
    return None;
  }
  let group = adw::PreferencesGroup::new();
  group.set_title("Details");
  if !rating.is_empty() {
    group.add(&dim_label(&rating));
  }
  if !genres.is_empty() {
    group.add(&dim_label(&format!("Genres: {}", genres.join(", "))));
  }
  if !metadata.creators.is_empty() {
    group.add(&dim_label(&format!(
      "Creators: {}",
      metadata.creators.join(", ")
    )));
  }
  if !metadata.cast.is_empty() {
    let cast = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    for name in metadata.cast.iter().take(12) {
      let label = gtk::Label::new(Some(name));
      label.add_css_class("caption");
      label.set_max_width_chars(18);
      label.set_ellipsize(gtk::pango::EllipsizeMode::End);
      cast.append(&label);
    }
    let cast_scroll = gtk::ScrolledWindow::builder()
      .child(&cast)
      .hscrollbar_policy(gtk::PolicyType::Automatic)
      .vscrollbar_policy(gtk::PolicyType::Never)
      .build();
    group.add(&cast_scroll);
  }
  Some(group.upcast())
}

fn stream_metadata_status() -> gtk::Widget {
  state_view(
    "Audio and subtitles",
    "Stream metadata is available when playback starts; no stream details were requested yet.",
    "audio-x-generic-symbolic",
  )
}

fn detail_metadata(detail: &VideoItemDetail) -> String {
  let mut details = Vec::new();
  if let Some(year) = detail.production_year {
    details.push(year.to_string());
  }
  details.push(detail.item_type.clone());
  if !detail.genres.is_empty() {
    details.push(detail.genres.join(", "));
  }
  if detail.favorite {
    details.push("Favorite".to_owned());
  }
  details.join(" · ")
}

fn show_detail_metadata(detail: &VideoShowDetail) -> String {
  let mut details = Vec::new();
  if let Some(year) = detail.production_year {
    details.push(year.to_string());
  }
  details.push("Series".to_owned());
  if !detail.genres.is_empty() {
    details.push(detail.genres.join(", "));
  }
  if detail.favorite {
    details.push("Favorite".to_owned());
  }
  details.join(" · ")
}

fn season_page_request(
  series_id: &str,
  season: &VideoSeason,
  start_index: i32,
) -> VideoSeasonEpisodesPageRequest {
  VideoSeasonEpisodesPageRequest {
    series_id: series_id.to_owned(),
    season_id: Some(season.id.clone()),
    season_number: season.season_number,
    start_index: start_index.max(0),
    limit: SEASON_EPISODE_PAGE_SIZE,
  }
}

fn apply_user_data_update(
  detail: &mut LoadState<DetailContent>,
  update: &VideoUserDataUpdate,
) -> bool {
  match detail {
    LoadState::Ready(DetailContent::Item(item)) if item.id == update.item_id => {
      item.played = update.played;
      item.favorite = update.favorite;
      true
    }
    LoadState::Ready(DetailContent::Show(show)) if show.id == update.item_id => {
      show.played = update.played;
      show.favorite = update.favorite;
      true
    }
    _ => false,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn season_page_request_uses_exact_identity_and_a_bounded_window() {
    let season = VideoSeason {
      id: "season-2".to_owned(),
      name: "Season 2".to_owned(),
      season_number: Some(2),
      played: false,
      favorite: false,
      artwork_image_id: None,
    };

    let request = season_page_request("show-1", &season, 60);

    assert_eq!(request.series_id, "show-1");
    assert_eq!(request.season_id.as_deref(), Some("season-2"));
    assert_eq!(request.season_number, Some(2));
    assert_eq!(request.start_index, 60);
    assert_eq!(request.limit, 30);
  }

  #[test]
  fn user_data_completion_updates_only_the_matching_detail() {
    let mut detail = LoadState::Ready(DetailContent::Show(VideoShowDetail {
      id: "show-1".to_owned(),
      name: "Show".to_owned(),
      overview: None,
      production_year: None,
      genres: Vec::new(),
      played: false,
      favorite: false,
      can_play: false,
      artwork_image_id: None,
      backdrop_image_id: None,
      next_episode: None,
      seasons: Vec::new(),
      metadata: Default::default(),
    }));
    let stale = VideoUserDataUpdate {
      item_id: "show-2".to_owned(),
      played: true,
      favorite: true,
    };
    assert!(!apply_user_data_update(&mut detail, &stale));
    let current = VideoUserDataUpdate {
      item_id: "show-1".to_owned(),
      played: true,
      favorite: true,
    };
    assert!(apply_user_data_update(&mut detail, &current));
    assert!(matches!(
      detail,
      LoadState::Ready(DetailContent::Show(VideoShowDetail {
        played: true,
        favorite: true,
        ..
      }))
    ));
  }
}
