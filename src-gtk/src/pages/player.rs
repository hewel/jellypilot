use std::cell::Cell;
use std::rc::Rc;

use jellypilot_media_server::MediaItem;
use jellypilot_session::IntroSkipKind;
use relm4::gtk::prelude::*;
use relm4::{gtk, Sender};

use crate::artwork::{ArtworkAdapter, DecodedArtwork, FALLBACK_ARTWORK_ICON};
use crate::pages::cards::{clear_box, dim_label};
use crate::shell::AppMessage;
use jellypilot_core::artwork_binder::{ArtworkBinder, ArtworkSlot, ArtworkSurface};
use jellypilot_mpv::playback::{Playable, TrackInfo};
use jellypilot_mpv::playback_session::{
  AdjacentAvailability, AdjacentDirection, PlaybackNotice, SessionView, TracksView,
};
use jellypilot_mpv::player::{format_duration, runtime_seconds_to_ticks, TrackKind};

const PLAYER_THUMB_SIZE: i32 = 36;

pub(crate) struct PlayerPage {
  root: gtk::Box,
  artwork: gtk::Image,
  artwork_fallback: gtk::Image,
  title: gtk::Label,
  subtitle: gtk::Label,
  status_icon: gtk::Image,
  status_label: gtk::Label,
  info: gtk::Stack,
  position_label: gtk::Label,
  duration_label: gtk::Label,
  previous_button: gtk::Button,
  pause_button: gtk::Button,
  next_button: gtk::Button,
  stop_button: gtk::Button,
  seek: gtk::Scale,
  volume: gtk::Scale,
  mute_button: gtk::ToggleButton,
  audio_button: gtk::MenuButton,
  subtitle_button: gtk::MenuButton,
  audio_track_list: gtk::Box,
  subtitle_track_list: gtk::Box,
  controls_syncing: Rc<Cell<bool>>,
  sender: Sender<AppMessage>,
  item: Option<MediaItem>,
  artwork_image_id: Option<String>,
  engine_error: Option<String>,
}

pub(crate) struct PlayerContext<'a> {
  pub artwork: &'a ArtworkAdapter,
  pub binder: &'a mut ArtworkBinder,
}

#[derive(Debug)]
pub(crate) enum Message {
  TogglePaused,
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioTrack(i64),
  SelectSubtitleTrack(Option<i64>),
  PlayAdjacent(AdjacentDirection),
  Stop,
}

pub(crate) enum PlayerEvent<'a> {
  Started(&'a Playable),
  Stopped,
  RefreshArtwork,
  EngineAvailable,
  EngineUnavailable(String),
}

pub(crate) enum PlayerEffect {
  TogglePaused,
  Seek(f64),
  SetVolume(f64),
  SetMuted(bool),
  SelectAudioTrack(i64),
  SelectSubtitleTrack(Option<i64>),
  PlayAdjacent(AdjacentDirection),
  Stop,
  ArtworkLoad {
    surface: ArtworkSurface,
    slot: ArtworkSlot,
    image_id: String,
  },
}

impl PlayerPage {
  pub(crate) fn build(sender: &Sender<AppMessage>) -> Self {
    let controls_syncing = Rc::new(Cell::new(false));
    let artwork = gtk::Image::new();
    artwork.set_pixel_size(PLAYER_THUMB_SIZE);
    let artwork_fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
    artwork_fallback.set_pixel_size(16);
    artwork_fallback.set_halign(gtk::Align::Center);
    artwork_fallback.set_valign(gtk::Align::Center);
    let artwork_frame = gtk::Overlay::new();
    artwork_frame.add_css_class("jellypilot-rounded");
    artwork_frame.add_css_class("jellypilot-playerbar-thumb");
    artwork_frame.set_overflow(gtk::Overflow::Hidden);
    artwork_frame.set_size_request(PLAYER_THUMB_SIZE, PLAYER_THUMB_SIZE);
    artwork_frame.set_valign(gtk::Align::Center);
    artwork_frame.set_child(Some(&artwork));
    artwork_frame.add_overlay(&artwork_fallback);
    let title = gtk::Label::new(None);
    title.add_css_class("heading");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_hexpand(true);
    let subtitle = dim_label("");
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let playback_meta = gtk::Box::new(gtk::Orientation::Vertical, 0);
    playback_meta.set_valign(gtk::Align::Center);
    playback_meta.set_hexpand(true);
    playback_meta.append(&title);
    playback_meta.append(&subtitle);
    let status_icon = gtk::Image::from_icon_name("content-loading-symbolic");
    status_icon.set_pixel_size(16);
    let status_label = gtk::Label::new(None);
    status_label.set_xalign(0.0);
    status_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status_label.set_hexpand(true);
    let playback_status = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    playback_status.set_valign(gtk::Align::Center);
    playback_status.append(&status_icon);
    playback_status.append(&status_label);
    let info = gtk::Stack::new();
    info.set_hexpand(true);
    info.set_hhomogeneous(true);
    info.add_named(&playback_meta, Some("meta"));
    info.add_named(&playback_status, Some("status"));
    info.set_visible_child_name("meta");
    let previous_button = gtk::Button::from_icon_name("media-skip-backward-symbolic");
    previous_button.add_css_class("flat");
    previous_button.add_css_class("circular");
    previous_button.set_tooltip_text(Some("Previous episode is unavailable."));
    previous_button.update_property(&[gtk::accessible::Property::Label("Previous episode")]);
    previous_button.set_sensitive(false);
    previous_button.connect_clicked({
      let sender = sender.clone();
      move |_| {
        sender.emit(AppMessage::Player(Message::PlayAdjacent(
          AdjacentDirection::Previous,
        )))
      }
    });
    let pause_button = gtk::Button::from_icon_name("media-playback-start-symbolic");
    pause_button.add_css_class("flat");
    pause_button.add_css_class("circular");
    pause_button.set_tooltip_text(Some("Pause or resume playback"));
    pause_button.update_property(&[gtk::accessible::Property::Label("Pause or resume playback")]);
    pause_button.set_sensitive(false);
    pause_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Player(Message::TogglePaused))
    });
    let next_button = gtk::Button::from_icon_name("media-skip-forward-symbolic");
    next_button.add_css_class("flat");
    next_button.add_css_class("circular");
    next_button.set_tooltip_text(Some("Next episode is unavailable."));
    next_button.update_property(&[gtk::accessible::Property::Label("Next episode")]);
    next_button.set_sensitive(false);
    next_button.connect_clicked({
      let sender = sender.clone();
      move |_| {
        sender.emit(AppMessage::Player(Message::PlayAdjacent(
          AdjacentDirection::Next,
        )))
      }
    });
    let stop_button = gtk::Button::from_icon_name("media-playback-stop-symbolic");
    stop_button.add_css_class("flat");
    stop_button.add_css_class("circular");
    stop_button.set_tooltip_text(Some("Stop playback"));
    stop_button.update_property(&[gtk::accessible::Property::Label("Stop playback")]);
    stop_button.set_sensitive(false);
    stop_button.connect_clicked({
      let sender = sender.clone();
      move |_| sender.emit(AppMessage::Player(Message::Stop))
    });
    let transport = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    transport.set_valign(gtk::Align::Center);
    transport.append(&previous_button);
    transport.append(&pause_button);
    transport.append(&next_button);
    transport.append(&stop_button);
    let position_label = playback_time_label();
    let duration_label = playback_time_label();
    let time = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    time.set_valign(gtk::Align::Center);
    time.append(&position_label);
    time.append(&gtk::Label::new(Some("/")));
    time.append(&duration_label);
    let seek = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 1.0);
    seek.add_css_class("jellypilot-bar-seek");
    seek.set_draw_value(false);
    seek.set_sensitive(false);
    seek.set_hexpand(true);
    seek.set_halign(gtk::Align::Fill);
    seek.set_valign(gtk::Align::Center);
    seek.update_property(&[gtk::accessible::Property::Label("Playback position")]);
    seek.connect_change_value({
      let sender = sender.clone();
      let controls_syncing = Rc::clone(&controls_syncing);
      move |_, _, value| {
        if !controls_syncing.get() {
          sender.emit(AppMessage::Player(Message::Seek(value)));
        }
        gtk::glib::Propagation::Proceed
      }
    });
    let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    volume.add_css_class("jellypilot-bar-volume");
    volume.set_draw_value(false);
    volume.set_sensitive(false);
    volume.set_hexpand(false);
    volume.set_vexpand(false);
    volume.set_valign(gtk::Align::Center);
    volume.set_size_request(140, -1);
    volume.connect_change_value({
      let sender = sender.clone();
      let controls_syncing = Rc::clone(&controls_syncing);
      move |_, _, value| {
        if !controls_syncing.get() {
          sender.emit(AppMessage::Player(Message::SetVolume(value)));
        }
        gtk::glib::Propagation::Proceed
      }
    });
    let mute_button = gtk::ToggleButton::new();
    mute_button.set_icon_name("audio-volume-high-symbolic");
    mute_button.add_css_class("flat");
    mute_button.set_tooltip_text(Some("Mute"));
    mute_button.set_sensitive(false);
    mute_button.update_property(&[gtk::accessible::Property::Label("Mute")]);
    mute_button.connect_toggled({
      let sender = sender.clone();
      let controls_syncing = Rc::clone(&controls_syncing);
      move |button| {
        if !controls_syncing.get() {
          sender.emit(AppMessage::Player(Message::SetMuted(button.is_active())));
        }
      }
    });
    let audio_track_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    audio_track_list.add_css_class("jellypilot-track-list");
    let audio_popover = gtk::Popover::new();
    audio_popover.set_child(Some(&audio_track_list));
    let audio_button = gtk::MenuButton::new();
    audio_button.add_css_class("flat");
    audio_button.add_css_class("circular");
    audio_button.set_icon_name("audio-x-generic-symbolic");
    audio_button.set_tooltip_text(Some("Audio track"));
    audio_button.set_sensitive(false);
    audio_button.set_popover(Some(&audio_popover));
    audio_button.update_property(&[gtk::accessible::Property::Label("Audio track")]);
    let subtitle_track_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    subtitle_track_list.add_css_class("jellypilot-track-list");
    let subtitle_popover = gtk::Popover::new();
    subtitle_popover.set_child(Some(&subtitle_track_list));
    let subtitle_button = gtk::MenuButton::new();
    subtitle_button.add_css_class("flat");
    subtitle_button.add_css_class("circular");
    subtitle_button.set_icon_name("media-view-subtitles-symbolic");
    subtitle_button.set_tooltip_text(Some("Subtitle track"));
    subtitle_button.set_sensitive(false);
    subtitle_button.set_popover(Some(&subtitle_popover));
    subtitle_button.update_property(&[gtk::accessible::Property::Label("Subtitle track")]);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_top(4);
    row.set_margin_bottom(8);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.set_valign(gtk::Align::Center);
    row.append(&artwork_frame);
    row.append(&info);
    row.append(&transport);
    row.append(&time);
    row.append(&volume);
    row.append(&mute_button);
    row.append(&audio_button);
    row.append(&subtitle_button);
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("jellypilot-playerbar");
    root.set_visible(false);
    root.append(&seek);
    root.append(&row);

    Self {
      root,
      artwork,
      artwork_fallback,
      title,
      subtitle,
      status_icon,
      status_label,
      info,
      position_label,
      duration_label,
      previous_button,
      pause_button,
      next_button,
      stop_button,
      seek,
      volume,
      mute_button,
      audio_button,
      subtitle_button,
      audio_track_list,
      subtitle_track_list,
      controls_syncing,
      sender: sender.clone(),
      item: None,
      artwork_image_id: None,
      engine_error: None,
    }
  }

  pub(crate) fn root(&self) -> &gtk::Box {
    &self.root
  }

  pub(crate) fn item(&self) -> Option<&MediaItem> {
    self.item.as_ref()
  }

  pub(crate) fn handle(
    &mut self,
    message: Message,
    _cx: &mut PlayerContext<'_>,
  ) -> Vec<PlayerEffect> {
    vec![match message {
      Message::TogglePaused => PlayerEffect::TogglePaused,
      Message::Seek(position) => PlayerEffect::Seek(position),
      Message::SetVolume(volume) => PlayerEffect::SetVolume(volume),
      Message::SetMuted(muted) => PlayerEffect::SetMuted(muted),
      Message::SelectAudioTrack(id) => PlayerEffect::SelectAudioTrack(id),
      Message::SelectSubtitleTrack(id) => PlayerEffect::SelectSubtitleTrack(id),
      Message::PlayAdjacent(direction) => PlayerEffect::PlayAdjacent(direction),
      Message::Stop => PlayerEffect::Stop,
    }]
  }

  pub(crate) fn handle_event(
    &mut self,
    event: PlayerEvent<'_>,
    cx: &mut PlayerContext<'_>,
  ) -> Vec<PlayerEffect> {
    match event {
      PlayerEvent::Started(item) => {
        self.item = Some(media_item_from_playable(item));
        let Some(image_id) = playable_artwork_image_id(item) else {
          return Vec::new();
        };
        self.artwork_image_id = Some(image_id);
        self.queue_artwork(cx)
      }
      PlayerEvent::Stopped => {
        self.item = None;
        self.artwork_image_id = None;
        self.queue_artwork(cx)
      }
      PlayerEvent::RefreshArtwork => {
        if self.artwork_image_id.is_some() {
          self.queue_artwork(cx)
        } else {
          Vec::new()
        }
      }
      PlayerEvent::EngineAvailable => {
        self.engine_error = None;
        Vec::new()
      }
      PlayerEvent::EngineUnavailable(message) => {
        self.engine_error = Some(message);
        Vec::new()
      }
    }
  }

  pub(crate) fn render(&self, view: &SessionView) {
    let now_playing = view.now_playing.as_ref();
    self.root.set_visible(now_playing.is_some());
    let title = now_playing
      .map(|playing| playing.item.title.as_str())
      .unwrap_or("");
    self.title.set_label(title);
    if let Some(prompt) = view.intro_prompt {
      self.title.set_tooltip_text(Some(&format!(
        "{} skip available",
        intro_skip_label(prompt.kind)
      )));
    } else {
      self.title.set_tooltip_text(None::<&str>);
    }
    let subtitle = playback_meta_subtitle(self.item.as_ref());
    self.subtitle.set_label(&subtitle);
    self.subtitle.set_visible(!subtitle.is_empty());
    let error = view.notice.as_ref().and_then(|notice| match notice {
      PlaybackNotice::Failed(_) => playback_notice(notice),
      _ => None,
    });
    let status = playback_bar_status(error.as_deref(), self.engine_error.as_deref(), view.busy);
    match status {
      Some((icon, message)) => {
        self.status_icon.set_icon_name(Some(icon));
        self.status_label.set_label(message);
        self.info.set_visible_child_name("status");
      }
      None => self.info.set_visible_child_name("meta"),
    }
    let active = now_playing.is_some() && view.engine_available && !view.busy;
    self.pause_button.set_sensitive(active);
    self.stop_button.set_sensitive(active);
    self.seek.set_sensitive(active);
    self.volume.set_sensitive(active);
    self.mute_button.set_sensitive(active);
    if let Some(playing) = now_playing {
      let duration = playing.duration_seconds.unwrap_or(playing.position_seconds);
      self
        .position_label
        .set_label(&format_duration(playing.position_seconds));
      self.duration_label.set_label(&format_duration(duration));
      self.pause_button.set_icon_name(if playing.paused {
        "media-playback-start-symbolic"
      } else {
        "media-playback-pause-symbolic"
      });
      self.pause_button.set_tooltip_text(Some(if playing.paused {
        "Resume playback"
      } else {
        "Pause playback"
      }));
      self.mute_button.set_icon_name(if playing.muted {
        "audio-volume-muted-symbolic"
      } else {
        "audio-volume-high-symbolic"
      });
      self
        .mute_button
        .set_tooltip_text(Some(if playing.muted { "Unmute" } else { "Mute" }));
      self.controls_syncing.set(true);
      self.seek.set_range(0.0, duration.max(1.0));
      let position = playing.position_seconds.clamp(0.0, duration.max(1.0));
      if (self.seek.value() - position).abs() > f64::EPSILON {
        self.seek.set_value(position);
      }
      let volume = playing.volume.clamp(0.0, 100.0);
      if (self.volume.value() - volume).abs() > f64::EPSILON {
        self.volume.set_value(volume);
      }
      if self.mute_button.is_active() != playing.muted {
        self.mute_button.set_active(playing.muted);
      }
      self.controls_syncing.set(false);
    } else {
      self.position_label.set_label("00:00");
      self.duration_label.set_label("00:00");
    }
    self.render_track_controls(active, view);
    self.render_adjacent_controls(active, view);
  }

  pub(crate) fn apply_artwork(&mut self, _slot: ArtworkSlot, decoded: DecodedArtwork) -> bool {
    match decoded.texture() {
      Ok(texture) => {
        self.artwork.set_paintable(Some(&texture));
        self.artwork_fallback.set_visible(false);
        true
      }
      Err(_) => false,
    }
  }

  fn queue_artwork(&mut self, cx: &mut PlayerContext<'_>) -> Vec<PlayerEffect> {
    let slot = cx.binder.bind_player_bar();
    self.artwork.set_paintable(None::<&gtk::gdk::Paintable>);
    self.artwork_fallback.set_visible(true);
    let Some(image_id) = self.artwork_image_id.clone() else {
      return Vec::new();
    };
    if let Some(decoded) = cx.artwork.cached(&image_id) {
      if let Ok(texture) = decoded.texture() {
        self.artwork.set_paintable(Some(&texture));
        self.artwork_fallback.set_visible(false);
        return Vec::new();
      }
    }
    vec![PlayerEffect::ArtworkLoad {
      surface: ArtworkSurface::PlayerBar,
      slot,
      image_id,
    }]
  }

  fn render_track_controls(&self, active: bool, view: &SessionView) {
    self.controls_syncing.set(true);
    match &view.tracks {
      TracksView::Ready { tracks, .. } => {
        let audio = tracks
          .iter()
          .filter(|track| track.track_type == "audio")
          .collect::<Vec<_>>();
        let subtitles = tracks
          .iter()
          .filter(|track| track.track_type == "sub")
          .collect::<Vec<_>>();
        populate_track_list(
          &self.audio_track_list,
          audio.iter().copied(),
          None,
          TrackKind::Audio,
          &self.controls_syncing,
          &self.sender,
        );
        populate_track_list(
          &self.subtitle_track_list,
          subtitles.iter().copied(),
          Some("Off"),
          TrackKind::Subtitle,
          &self.controls_syncing,
          &self.sender,
        );
        let audio_available = !audio.is_empty();
        let subtitle_available = !subtitles.is_empty();
        self.audio_button.set_sensitive(active && audio_available);
        self
          .subtitle_button
          .set_sensitive(active && subtitle_available);
        self.audio_button.set_tooltip_text(Some(if audio_available {
          "Select the MPV audio track"
        } else {
          "MPV reported no audio tracks."
        }));
        self
          .subtitle_button
          .set_tooltip_text(Some(if subtitle_available {
            "Select an MPV subtitle track or turn subtitles off"
          } else {
            "MPV reported no subtitle tracks."
          }));
      }
      TracksView::Loading => {
        self.clear_track_lists();
        self
          .audio_button
          .set_tooltip_text(Some("Audio tracks are loading."));
        self
          .subtitle_button
          .set_tooltip_text(Some("Subtitle tracks are loading."));
      }
      TracksView::Unavailable => {
        self.clear_track_lists();
        let reason = if !view.engine_available {
          self
            .engine_error
            .as_deref()
            .unwrap_or("Playback controller is unavailable.")
        } else {
          "Track controls require active playback."
        };
        self.audio_button.set_tooltip_text(Some(reason));
        self.subtitle_button.set_tooltip_text(Some(reason));
      }
    }
    self.controls_syncing.set(false);
  }

  fn clear_track_lists(&self) {
    clear_box(&self.audio_track_list);
    clear_box(&self.subtitle_track_list);
    self.audio_button.set_sensitive(false);
    self.subtitle_button.set_sensitive(false);
  }

  fn render_adjacent_controls(&self, active: bool, view: &SessionView) {
    let previous = &view.adjacent.previous;
    let next = &view.adjacent.next;
    let previous_available = matches!(previous, AdjacentAvailability::Available { .. });
    let next_available = matches!(next, AdjacentAvailability::Available { .. });
    self
      .previous_button
      .set_sensitive(active && previous_available);
    self.next_button.set_sensitive(active && next_available);
    let busy_reason = view
      .busy
      .then_some("Another playback operation is in progress.");
    let previous_reason =
      busy_reason.unwrap_or_else(|| adjacent_control_reason(previous, AdjacentDirection::Previous));
    let next_reason =
      busy_reason.unwrap_or_else(|| adjacent_control_reason(next, AdjacentDirection::Next));
    self.previous_button.set_tooltip_text(Some(previous_reason));
    self.next_button.set_tooltip_text(Some(next_reason));
  }
}

fn playback_time_label() -> gtk::Label {
  let label = gtk::Label::new(Some("00:00"));
  label.add_css_class("dim-label");
  label.add_css_class("monospace");
  label
}

fn playback_meta_subtitle(item: Option<&MediaItem>) -> String {
  let Some(item) = item else {
    return String::new();
  };
  if !item.item_type.eq_ignore_ascii_case("episode") {
    return String::new();
  }
  let series = item
    .series_name
    .as_deref()
    .map(str::trim)
    .filter(|name| !name.is_empty());
  match (series, item.parent_index_number, item.index_number) {
    (Some(series), Some(season), Some(episode)) => {
      format!("{series} · S{season} E{episode}")
    }
    (Some(series), _, _) => series.to_owned(),
    (_, Some(season), Some(episode)) => format!("S{season} E{episode} · {}", item.name),
    _ => item.name.clone(),
  }
}

fn playback_notice(notice: &PlaybackNotice) -> Option<String> {
  Some(match notice {
    PlaybackNotice::Finished => "Playback finished.".to_owned(),
    PlaybackNotice::Stopped => "Playback stopped.".to_owned(),
    PlaybackNotice::Failed(error) => format!("{error}."),
    PlaybackNotice::Warnings(warnings) => {
      let details = warnings
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
      format!("Playback is active, but {details}.")
    }
  })
}

const fn intro_skip_label(kind: IntroSkipKind) -> &'static str {
  match kind {
    IntroSkipKind::Introduction => "Intro",
    IntroSkipKind::Credits => "Credits",
  }
}

fn track_label(track: &TrackInfo) -> String {
  match (track.title.as_deref(), track.language.as_deref()) {
    (Some(title), Some(language)) if !title.eq_ignore_ascii_case(language) => {
      format!("{title} · {language}")
    }
    (Some(title), _) => title.to_owned(),
    (None, Some(language)) => language.to_owned(),
    (None, None) => format!("Track {}", track.id),
  }
}

fn populate_track_list<'a>(
  list: &gtk::Box,
  tracks: impl Iterator<Item = &'a TrackInfo>,
  off_label: Option<&str>,
  kind: TrackKind,
  syncing: &Rc<Cell<bool>>,
  sender: &Sender<AppMessage>,
) {
  clear_box(list);
  let tracks = tracks.collect::<Vec<_>>();
  let off_selected = off_label.is_some() && tracks.iter().all(|track| !track.selected);
  let mut group = None;
  if let Some(label) = off_label {
    let off = gtk::CheckButton::with_label(label);
    off.set_active(off_selected);
    off.connect_toggled({
      let sender = sender.clone();
      let syncing = Rc::clone(syncing);
      move |button| {
        if !syncing.get() && button.is_active() {
          sender.emit(AppMessage::Player(Message::SelectSubtitleTrack(None)));
        }
      }
    });
    group = Some(off.clone());
    list.append(&off);
  }
  for track in tracks {
    let row = gtk::CheckButton::with_label(&track_label(track));
    if let Some(group) = &group {
      row.set_group(Some(group));
    } else {
      group = Some(row.clone());
    }
    row.set_active(track.selected);
    let id = track.id;
    row.connect_toggled({
      let sender = sender.clone();
      let syncing = Rc::clone(syncing);
      move |button| {
        if !syncing.get() && button.is_active() {
          match kind {
            TrackKind::Audio => {
              sender.emit(AppMessage::Player(Message::SelectAudioTrack(id)));
            }
            TrackKind::Subtitle => {
              sender.emit(AppMessage::Player(Message::SelectSubtitleTrack(Some(id))));
            }
          }
        }
      }
    });
    list.append(&row);
  }
}

fn playback_bar_status<'a>(
  error: Option<&'a str>,
  unavailable: Option<&'a str>,
  busy: bool,
) -> Option<(&'static str, &'a str)> {
  if let Some(error) = error {
    return Some(("dialog-error-symbolic", error));
  }
  if let Some(unavailable) = unavailable {
    return Some(("dialog-warning-symbolic", unavailable));
  }
  if busy {
    return Some(("content-loading-symbolic", "Buffering…"));
  }
  None
}

fn adjacent_control_reason(
  availability: &AdjacentAvailability,
  direction: AdjacentDirection,
) -> &str {
  match availability {
    AdjacentAvailability::Loading => "Checking adjacent episodes…",
    AdjacentAvailability::Available { .. } => match direction {
      AdjacentDirection::Previous => "Play previous episode",
      AdjacentDirection::Next => "Play next episode",
    },
    AdjacentAvailability::Unavailable => match direction {
      AdjacentDirection::Previous => "No previous episode is available.",
      AdjacentDirection::Next => "No next episode is available.",
    },
    AdjacentAvailability::Idle => "Episode navigation requires active episode playback.",
  }
}

fn playable_artwork_image_id(item: &Playable) -> Option<String> {
  match item {
    Playable::Library(item) => item
      .series_poster_image_id
      .clone()
      .or_else(|| item.artwork_image_id.clone()),
    Playable::Detail(item) => item
      .series_poster_image_id
      .clone()
      .or_else(|| item.artwork_image_id.clone()),
    Playable::Media(_) => None,
  }
}

fn media_item_from_playable(item: &Playable) -> MediaItem {
  match item {
    Playable::Library(item) => MediaItem {
      id: item.id.clone(),
      name: item.name.clone(),
      item_type: item.item_type.clone(),
      series_id: item.series_id.clone(),
      series_name: item.series_name.clone(),
      season_name: None,
      index_number: item.episode_number,
      parent_index_number: item.season_number,
      run_time_ticks: runtime_seconds_to_ticks(item.runtime_seconds),
      overview: item.overview.clone(),
      series_primary_image_tag: None,
    },
    Playable::Detail(item) => MediaItem {
      id: item.id.clone(),
      name: item.name.clone(),
      item_type: item.item_type.clone(),
      series_id: item.series_id.clone(),
      series_name: item.series_name.clone(),
      season_name: None,
      index_number: item.episode_number,
      parent_index_number: item.season_number,
      run_time_ticks: runtime_seconds_to_ticks(item.runtime_seconds),
      overview: item.overview.clone(),
      series_primary_image_tag: None,
    },
    Playable::Media(item) => item.clone(),
  }
}
