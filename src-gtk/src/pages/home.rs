use std::collections::HashMap;

use jellypilot_media_server::{VideoHome, VideoLibraryItem, VideoLibraryShortcut};
use relm4::gtk::prelude::*;
use relm4::{gtk, Sender};

use crate::artwork::DecodedArtwork;
use jellypilot_core::artwork_binder::{ArtworkBinder, ArtworkSlot, ArtworkSurface};

use crate::pages::cards::{
  apply_decoded_artwork, clear_box, dim_label, featured_hero, library_shortcut_card, loading_view,
  poster_card, register_artwork, scrolled_page, state_view, ArtworkTarget,
};
use crate::shell::AppMessage;
use jellypilot_auth::login::ConnectionPhase;
use jellypilot_core::request_gate::{HomeToken, RequestGate};
use jellypilot_core::LoadState;
use jellypilot_mpv::playback::{Playable, PlaybackStartPosition};

pub(crate) struct HomePage {
  root: gtk::Widget,
  content: gtk::Box,
  sender: Sender<AppMessage>,
  home: LoadState<VideoHome>,
  shortcuts: Vec<VideoLibraryShortcut>,
  shortcuts_error: Option<String>,
  artwork_targets: HashMap<ArtworkSlot, ArtworkTarget>,
}

pub(crate) struct HomeContext<'a> {
  pub gate: &'a mut RequestGate,
  pub binder: &'a mut ArtworkBinder,
  pub connection: ConnectionPhase,
  pub playback_enabled: bool,
}

#[derive(Debug)]
pub(crate) enum Message {
  Load,
  Retry,
  SelectItem(VideoLibraryItem),
  Play(VideoLibraryItem, PlaybackStartPosition),
  OpenLibrary(VideoLibraryShortcut),
}

pub(crate) enum HomeEvent {
  Loaded {
    token: HomeToken,
    result: (
      Result<VideoHome, String>,
      Result<Vec<VideoLibraryShortcut>, String>,
    ),
  },
}

pub(crate) enum HomeEffect {
  BeginArtworkView,
  ArtworkLoad {
    surface: ArtworkSurface,
    slot: ArtworkSlot,
    image_id: String,
  },
  HomeLoad {
    token: HomeToken,
  },
  OpenDetail(VideoLibraryItem),
  OpenLibrary(VideoLibraryShortcut),
  PlayItem(Playable, PlaybackStartPosition),
  RenderIfVisible,
}

impl HomePage {
  pub(crate) fn build(sender: &Sender<AppMessage>) -> Self {
    let content = gtk::Box::builder()
      .orientation(gtk::Orientation::Vertical)
      .spacing(24)
      .margin_top(24)
      .margin_bottom(24)
      .margin_start(24)
      .margin_end(24)
      .build();
    let root = scrolled_page(
      "Video Home",
      "Recently added and in-progress video from this server.",
      &content,
    );
    Self {
      root,
      content,
      sender: sender.clone(),
      home: LoadState::Idle,
      shortcuts: Vec::new(),
      shortcuts_error: None,
      artwork_targets: HashMap::new(),
    }
  }

  pub(crate) fn root(&self) -> &gtk::Widget {
    &self.root
  }

  pub(crate) fn shortcuts(&self) -> &[VideoLibraryShortcut] {
    &self.shortcuts
  }

  pub(crate) fn shortcuts_error(&self) -> Option<&str> {
    self.shortcuts_error.as_deref()
  }

  pub(crate) fn reset(&mut self) {
    self.home = LoadState::Idle;
    self.shortcuts.clear();
    self.shortcuts_error = None;
    self.artwork_targets.clear();
  }

  pub(crate) fn prepare_connected_session(&mut self) {
    self.home = LoadState::Loading;
    self.shortcuts.clear();
    self.shortcuts_error = None;
  }
  /// Shows a connection failure on Video Home (auth failed before any content).
  pub(crate) fn show_failure(&mut self, message: &str) {
    self.home = LoadState::Failed(message.to_owned());
  }

  pub(crate) fn handle(&mut self, message: Message, cx: &mut HomeContext<'_>) -> Vec<HomeEffect> {
    match message {
      Message::Load | Message::Retry => self.load(cx),
      Message::SelectItem(item) => vec![HomeEffect::OpenDetail(item)],
      Message::Play(item, position) => vec![HomeEffect::PlayItem(Playable::from(item), position)],
      Message::OpenLibrary(shortcut) => vec![HomeEffect::OpenLibrary(shortcut)],
    }
  }

  pub(crate) fn handle_event(
    &mut self,
    event: HomeEvent,
    cx: &mut HomeContext<'_>,
  ) -> Vec<HomeEffect> {
    match event {
      HomeEvent::Loaded { token, result } => {
        if !cx.gate.finish_home(token) || !matches!(cx.connection, ConnectionPhase::Connected) {
          return Vec::new();
        }
        self.home = match result.0 {
          Ok(home) => LoadState::Ready(home),
          Err(message) => LoadState::Failed(message),
        };
        match result.1 {
          Ok(shortcuts) => {
            self.shortcuts = shortcuts;
            self.shortcuts_error = None;
          }
          Err(message) => {
            self.shortcuts.clear();
            self.shortcuts_error = Some(message);
          }
        }
        vec![HomeEffect::RenderIfVisible]
      }
    }
  }

  pub(crate) fn render(&mut self, cx: &mut HomeContext<'_>) -> Vec<HomeEffect> {
    cx.binder.begin_view(ArtworkSurface::Home);
    self.artwork_targets.clear();
    let mut effects = vec![HomeEffect::BeginArtworkView];
    clear_box(&self.content);
    match &self.home {
      LoadState::Idle => self.content.append(&state_view(
        "Connect to browse your libraries",
        "Sign in to Jellyfin or Emby to load Video Home.",
        "network-offline-symbolic",
      )),
      LoadState::Loading => self.content.append(&loading_view("Loading Video Home…")),
      LoadState::Failed(message) => {
        self.content.append(&state_view(
          "Video Home could not load",
          message.as_str(),
          "dialog-error-symbolic",
        ));
        let retry = gtk::Button::with_label("Retry");
        let sender = self.sender.clone();
        retry.connect_clicked(move |_| sender.emit(AppMessage::Home(Message::Retry)));
        self.content.append(&retry);
      }
      LoadState::Ready(home) => {
        let continue_watching = home.continue_watching.clone();
        let next_up = home.next_up.clone();
        let latest_movies = home.latest_movies.clone();
        let latest_episodes = home.latest_episodes.clone();
        let hero_item = continue_watching
          .first()
          .or(next_up.first())
          .or(latest_movies.first())
          .cloned();
        if let Some(item) = hero_item {
          let hero = self.featured_hero(&item, cx, &mut effects);
          self.content.append(&hero);
        }
        let shelves = [
          ("Continue Watching", continue_watching),
          ("Next Up", next_up),
          ("Latest Movies", latest_movies),
          ("Latest Episodes", latest_episodes),
        ];
        for (title, items) in shelves {
          let shelf = self.media_shelf(title, &items, cx, &mut effects);
          self.content.append(&shelf);
        }
        let libraries = self.library_shortcuts_section(cx, &mut effects);
        self.content.append(&libraries);
      }
    }
    effects
  }

  pub(crate) fn apply_artwork(&mut self, slot: ArtworkSlot, decoded: DecodedArtwork) -> bool {
    apply_decoded_artwork(&mut self.artwork_targets, slot, decoded)
  }

  fn load(&mut self, cx: &mut HomeContext<'_>) -> Vec<HomeEffect> {
    let token = cx.gate.begin_home();
    self.home = LoadState::Loading;
    vec![HomeEffect::HomeLoad { token }, HomeEffect::RenderIfVisible]
  }

  fn featured_hero(
    &mut self,
    item: &VideoLibraryItem,
    cx: &mut HomeContext<'_>,
    effects: &mut Vec<HomeEffect>,
  ) -> gtk::Widget {
    let sender = self.sender.clone();
    let play_sender = self.sender.clone();
    let select_item = item.clone();
    let play_item = item.clone();
    let (widget, artwork) = featured_hero(
      item,
      cx.playback_enabled,
      move || {
        sender.emit(AppMessage::Home(Message::SelectItem(select_item.clone())));
      },
      move |position| {
        play_sender.emit(AppMessage::Home(Message::Play(play_item.clone(), position)));
      },
    );
    self.push_artwork(cx.binder, artwork, effects);
    widget
  }

  fn media_shelf(
    &mut self,
    title: &str,
    items: &[VideoLibraryItem],
    cx: &mut HomeContext<'_>,
    effects: &mut Vec<HomeEffect>,
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
      let sender = self.sender.clone();
      let select_item = item.clone();
      let (widget, artwork) = poster_card(item, move || {
        sender.emit(AppMessage::Home(Message::SelectItem(select_item.clone())));
      });
      self.push_artwork(cx.binder, artwork, effects);
      row.append(&widget);
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

  fn library_shortcuts_section(
    &mut self,
    cx: &mut HomeContext<'_>,
    effects: &mut Vec<HomeEffect>,
  ) -> gtk::Widget {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let title = gtk::Label::new(Some("Libraries"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    section.append(&title);
    if let Some(message) = &self.shortcuts_error {
      section.append(&dim_label(message));
      return section.upcast();
    }
    if self.shortcuts.is_empty() {
      section.append(&dim_label("No video libraries available."));
      return section.upcast();
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let shortcuts = self.shortcuts.clone();
    for shortcut in &shortcuts {
      let sender = self.sender.clone();
      let opened = shortcut.clone();
      let (widget, artwork) = library_shortcut_card(shortcut, move || {
        sender.emit(AppMessage::Home(Message::OpenLibrary(opened.clone())));
      });
      self.push_artwork(cx.binder, artwork, effects);
      row.append(&widget);
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

  fn push_artwork(
    &mut self,
    binder: &mut ArtworkBinder,
    artwork: Option<crate::pages::cards::ArtworkBind>,
    effects: &mut Vec<HomeEffect>,
  ) {
    if let Some((slot, image_id)) = register_artwork(
      &mut self.artwork_targets,
      binder,
      ArtworkSurface::Home,
      artwork,
    ) {
      effects.push(HomeEffect::ArtworkLoad {
        surface: ArtworkSurface::Home,
        slot,
        image_id,
      });
    }
  }
}
