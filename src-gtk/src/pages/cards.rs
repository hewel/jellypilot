use std::collections::HashMap;

use jellypilot_media_server::{VideoLibraryItem, VideoLibraryKind, VideoLibraryShortcut};
use relm4::adw::prelude::*;
use relm4::{adw, gtk};

use crate::artwork::FALLBACK_ARTWORK_ICON;
use crate::artwork_binder::{ArtworkBinder, ArtworkSlot, ArtworkSurface};
use crate::playback::PlaybackStartPosition;

pub(crate) const HOME_HERO_HEIGHT: i32 = 340;
pub(crate) const POSTER_FRAME_WIDTH: i32 = 160;
pub(crate) const POSTER_FRAME_HEIGHT: i32 = 240;
pub(crate) const THUMB_FRAME_WIDTH: i32 = 240;
pub(crate) const THUMB_FRAME_HEIGHT: i32 = 135;

pub(crate) struct ArtworkTarget {
  pub picture: gtk::Picture,
  pub fallback: gtk::Image,
}

pub(crate) struct ArtworkBind {
  pub image_id: String,
  pub target: ArtworkTarget,
}

pub(crate) fn register_artwork(
  targets: &mut HashMap<ArtworkSlot, ArtworkTarget>,
  binder: &mut ArtworkBinder,
  surface: ArtworkSurface,
  bind: Option<ArtworkBind>,
) -> Option<(ArtworkSlot, String)> {
  let bind = bind?;
  let slot = binder.bind(surface);
  targets.insert(slot, bind.target);
  Some((slot, bind.image_id))
}

pub(crate) fn apply_decoded_artwork(
  targets: &mut HashMap<ArtworkSlot, ArtworkTarget>,
  slot: ArtworkSlot,
  decoded: crate::artwork::DecodedArtwork,
) -> bool {
  let Some(target) = targets.remove(&slot) else {
    return true;
  };
  match decoded.texture() {
    Ok(texture) => {
      target.picture.set_paintable(Some(&texture));
      target.fallback.set_visible(false);
      true
    }
    Err(_) => false,
  }
}

pub(crate) fn poster_card(
  item: &VideoLibraryItem,
  on_select: impl Fn() + 'static,
) -> (gtk::Widget, Option<ArtworkBind>) {
  let (width, height) = card_frame_size(item);
  let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
  card.set_width_request(width);
  let button = gtk::Button::new();
  button.set_has_frame(false);
  let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
  let (overlay, artwork) = poster_overlay(item, width, height, 48);
  column.append(&overlay);
  let text = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(2)
    .build();
  let title = gtk::Label::new(Some(&item.name));
  title.set_xalign(0.0);
  title.set_ellipsize(gtk::pango::EllipsizeMode::End);
  title.set_max_width_chars(18);
  text.append(&title);
  let details = dim_label(&item_caption(item));
  details.set_ellipsize(gtk::pango::EllipsizeMode::End);
  details.set_max_width_chars(18);
  text.append(&details);
  column.append(&text);
  button.set_child(Some(&column));
  let accessible_label = format!("Open details for {}", item.name);
  button.set_tooltip_text(Some(&accessible_label));
  button.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
  button.connect_clicked(move |_| on_select());
  card.append(&button);
  (card.upcast(), artwork)
}

pub(crate) fn row_card(
  item: &VideoLibraryItem,
  playback_enabled: bool,
  on_select: impl Fn() + 'static,
  on_play: impl Fn(PlaybackStartPosition) + 'static,
) -> (gtk::Widget, Option<ArtworkBind>) {
  let button = gtk::Button::new();
  button.set_has_frame(false);
  let row = gtk::Box::builder()
    .orientation(gtk::Orientation::Horizontal)
    .spacing(12)
    .margin_top(6)
    .margin_bottom(6)
    .margin_start(8)
    .margin_end(8)
    .build();
  let (width, height) = if is_episode_item(item) {
    (128, 72)
  } else {
    (72, 108)
  };
  let (overlay, artwork) = poster_overlay(item, width, height, 32);
  row.append(&overlay);
  let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
  text.set_hexpand(true);
  text.set_valign(gtk::Align::Center);
  let title = gtk::Label::new(Some(&item.name));
  title.set_xalign(0.0);
  title.set_ellipsize(gtk::pango::EllipsizeMode::End);
  title.set_max_width_chars(64);
  text.append(&title);
  let details = dim_label(&item_caption(item));
  details.set_ellipsize(gtk::pango::EllipsizeMode::End);
  text.append(&details);
  row.append(&text);
  if matches!(item.item_type.as_str(), "Movie" | "Episode") {
    let has_resume = item.resume_position_seconds.unwrap_or_default() > 0.0;
    let action = gtk::Button::from_icon_name("media-playback-start-symbolic");
    action.add_css_class("flat");
    action.add_css_class("suggested-action");
    action.set_valign(gtk::Align::Center);
    let action_label = if has_resume { "Resume" } else { "Play" };
    action.set_tooltip_text(Some(action_label));
    action.update_property(&[gtk::accessible::Property::Label(action_label)]);
    action.set_sensitive(playback_enabled);
    let position = if has_resume {
      PlaybackStartPosition::Resume
    } else {
      PlaybackStartPosition::Beginning
    };
    action.connect_clicked(move |_| on_play(position));
    row.append(&action);
  }
  button.set_child(Some(&row));
  let accessible_label = format!("Open details for {}", item.name);
  button.set_tooltip_text(Some(&accessible_label));
  button.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
  button.connect_clicked(move |_| on_select());
  (button.upcast(), artwork)
}

pub(crate) fn featured_hero(
  item: &VideoLibraryItem,
  playback_enabled: bool,
  on_select: impl Fn() + 'static,
  on_play: impl Fn(PlaybackStartPosition) + 'static,
) -> (gtk::Widget, Option<ArtworkBind>) {
  let container = gtk::Overlay::new();
  container.add_css_class("jellypilot-rounded");
  container.add_css_class("jellypilot-hero");
  container.set_overflow(gtk::Overflow::Hidden);
  container.set_hexpand(true);
  container.set_size_request(-1, HOME_HERO_HEIGHT);
  let backdrop = cover_picture(-1, HOME_HERO_HEIGHT);
  let fallback = gtk::Image::from_icon_name("image-missing-symbolic");
  fallback.set_pixel_size(64);
  fallback.set_halign(gtk::Align::Center);
  fallback.set_valign(gtk::Align::Center);
  let backdrop_overlay = gtk::Overlay::new();
  backdrop_overlay.set_hexpand(true);
  backdrop_overlay.set_vexpand(true);
  backdrop_overlay.set_child(Some(&backdrop));
  backdrop_overlay.add_overlay(&fallback);
  container.set_child(Some(&backdrop_overlay));
  let artwork = bind_image(item.artwork_image_id.as_deref(), backdrop, fallback);
  let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
  scrim.add_css_class("jellypilot-hero-scrim");
  scrim.set_hexpand(true);
  scrim.set_vexpand(true);
  scrim.set_valign(gtk::Align::Fill);
  let hero_text = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(8)
    .margin_top(48)
    .margin_bottom(24)
    .margin_start(28)
    .margin_end(28)
    .valign(gtk::Align::End)
    .vexpand(true)
    .build();
  let title = gtk::Label::new(Some(&hero_headline(item)));
  title.add_css_class("title-1");
  title.set_xalign(0.0);
  title.set_ellipsize(gtk::pango::EllipsizeMode::End);
  title.set_max_width_chars(60);
  hero_text.append(&title);
  let metadata = gtk::Label::new(Some(&hero_metadata(item)));
  metadata.add_css_class("dim-label");
  metadata.set_xalign(0.0);
  metadata.set_ellipsize(gtk::pango::EllipsizeMode::End);
  hero_text.append(&metadata);
  if let Some(overview) = &item.overview {
    let synopsis = gtk::Label::new(Some(overview));
    synopsis.set_xalign(0.0);
    synopsis.set_wrap(true);
    synopsis.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    synopsis.set_lines(3);
    synopsis.set_ellipsize(gtk::pango::EllipsizeMode::End);
    synopsis.set_max_width_chars(80);
    synopsis.add_css_class("dim-label");
    hero_text.append(&synopsis);
  }
  let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
  let has_resume = item.resume_position_seconds.unwrap_or_default() > 0.0;
  let primary_label = if has_resume { "Resume" } else { "Play" };
  let primary = gtk::Button::with_label(primary_label);
  primary.add_css_class("suggested-action");
  primary.add_css_class("pill");
  let primary_position = if has_resume {
    PlaybackStartPosition::Resume
  } else {
    PlaybackStartPosition::Beginning
  };
  primary.connect_clicked(move |_| on_play(primary_position));
  primary.set_sensitive(playback_enabled);
  actions.append(&primary);
  let details = gtk::Button::with_label("Details");
  details.add_css_class("pill");
  details.add_css_class("osd");
  details.connect_clicked(move |_| on_select());
  actions.append(&details);
  hero_text.append(&actions);
  scrim.append(&hero_text);
  container.add_overlay(&scrim);
  (container.upcast(), artwork)
}

pub(crate) fn library_shortcut_card(
  shortcut: &VideoLibraryShortcut,
  on_open: impl Fn() + 'static,
) -> (gtk::Widget, Option<ArtworkBind>) {
  let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
  card.set_width_request(POSTER_FRAME_WIDTH);
  let button = gtk::Button::new();
  button.set_has_frame(false);
  let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
  let artwork_overlay = gtk::Overlay::new();
  artwork_overlay.add_css_class("jellypilot-poster");
  artwork_overlay.set_overflow(gtk::Overflow::Hidden);
  artwork_overlay.set_size_request(POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT);
  let picture = cover_picture(POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT);
  let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
  fallback.set_pixel_size(48);
  fallback.set_halign(gtk::Align::Center);
  fallback.set_valign(gtk::Align::Center);
  artwork_overlay.set_child(Some(&picture));
  artwork_overlay.add_overlay(&fallback);
  let artwork = bind_image(shortcut.artwork_image_id.as_deref(), picture, fallback);
  column.append(&artwork_overlay);
  let text = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(2)
    .build();
  let title = gtk::Label::new(Some(&shortcut.name));
  title.set_xalign(0.0);
  title.set_ellipsize(gtk::pango::EllipsizeMode::End);
  title.set_max_width_chars(18);
  text.append(&title);
  let details = dim_label(&library_shortcut_caption(shortcut));
  details.set_ellipsize(gtk::pango::EllipsizeMode::End);
  details.set_max_width_chars(18);
  text.append(&details);
  column.append(&text);
  button.set_child(Some(&column));
  let accessible_label = format!("Open library {}", shortcut.name);
  button.set_tooltip_text(Some(&accessible_label));
  button.update_property(&[gtk::accessible::Property::Label(&accessible_label)]);
  button.connect_clicked(move |_| on_open());
  card.append(&button);
  (card.upcast(), artwork)
}

pub(crate) fn backdrop_artwork(image_id: Option<&str>) -> (gtk::Widget, Option<ArtworkBind>) {
  let overlay = gtk::Overlay::new();
  let picture = gtk::Picture::new();
  picture.set_can_shrink(true);
  picture.set_content_fit(gtk::ContentFit::Cover);
  picture.set_hexpand(true);
  picture.set_size_request(-1, 220);
  let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
  fallback.set_pixel_size(32);
  fallback.set_halign(gtk::Align::Center);
  fallback.set_valign(gtk::Align::Center);
  overlay.set_child(Some(&picture));
  overlay.add_overlay(&fallback);
  let artwork = bind_image(image_id, picture, fallback);
  (overlay.upcast(), artwork)
}

pub(crate) fn card_frame_size(item: &VideoLibraryItem) -> (i32, i32) {
  if is_episode_item(item) {
    (THUMB_FRAME_WIDTH, THUMB_FRAME_HEIGHT)
  } else {
    (POSTER_FRAME_WIDTH, POSTER_FRAME_HEIGHT)
  }
}

pub(crate) fn cover_picture(width: i32, height: i32) -> gtk::Picture {
  let picture = gtk::Picture::new();
  picture.set_can_shrink(true);
  picture.set_content_fit(gtk::ContentFit::Cover);
  picture.set_hexpand(true);
  picture.set_vexpand(true);
  picture.set_halign(gtk::Align::Fill);
  picture.set_valign(gtk::Align::Fill);
  picture.set_size_request(width, height);
  picture
}

pub(crate) fn item_caption(item: &VideoLibraryItem) -> String {
  match item.production_year {
    Some(year) => format!("{year} · {}", item.item_type),
    None => item.item_type.clone(),
  }
}

pub(crate) fn hero_headline(item: &VideoLibraryItem) -> String {
  if is_episode_item(item) {
    item
      .series_name
      .as_deref()
      .map(str::trim)
      .filter(|name| !name.is_empty())
      .map(ToOwned::to_owned)
      .unwrap_or_else(|| item.name.clone())
  } else {
    item.name.clone()
  }
}

pub(crate) fn hero_metadata(item: &VideoLibraryItem) -> String {
  if is_episode_item(item) {
    match (item.season_number, item.episode_number) {
      (Some(season), Some(number)) => format!("S{season} E{number} · {}", item.name),
      _ => format!("Episode · {}", item.name),
    }
  } else {
    item_caption(item)
  }
}

pub(crate) fn status_badge(item: &VideoLibraryItem) -> Option<gtk::Label> {
  let text = if item.played {
    "Played"
  } else if item.favorite {
    "Favorite"
  } else {
    return None;
  };
  let badge = gtk::Label::new(Some(text));
  badge.add_css_class("jellypilot-badge");
  badge.set_halign(gtk::Align::End);
  badge.set_valign(gtk::Align::Start);
  Some(badge)
}

pub(crate) fn resume_progress_bar(item: &VideoLibraryItem) -> Option<gtk::ProgressBar> {
  let percentage = item
    .played_percentage
    .filter(|value| *value > 0.0 && *value < 100.0)?;
  let progress = gtk::ProgressBar::new();
  progress.set_fraction(percentage / 100.0);
  progress.set_show_text(false);
  progress.set_valign(gtk::Align::End);
  progress.set_hexpand(true);
  progress.add_css_class("jellypilot-progress-overlay");
  Some(progress)
}

pub(crate) fn dim_label(text: &str) -> gtk::Label {
  let label = gtk::Label::new(Some(text));
  label.add_css_class("dim-label");
  label.set_xalign(0.0);
  label
}

pub(crate) fn clear_box(container: &gtk::Box) {
  while let Some(child) = container.first_child() {
    container.remove(&child);
  }
}

pub(crate) fn state_view(title: &str, copy: &str, icon_name: &str) -> gtk::Widget {
  let status = adw::StatusPage::new();
  status.set_title(title);
  status.set_description(Some(copy));
  status.set_icon_name(Some(icon_name));
  status.set_vexpand(true);
  status.upcast()
}

pub(crate) fn loading_view(copy: &str) -> gtk::Widget {
  let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
  column.set_halign(gtk::Align::Center);
  column.set_valign(gtk::Align::Center);
  column.set_accessible_role(gtk::AccessibleRole::Status);
  let spinner = gtk::Spinner::new();
  spinner.start();
  column.append(&spinner);
  column.append(&dim_label(copy));
  column.upcast()
}

pub(crate) fn scrolled_page(title: &str, subtitle: &str, content: &gtk::Box) -> gtk::Widget {
  let page = gtk::Box::builder()
    .orientation(gtk::Orientation::Vertical)
    .spacing(18)
    .margin_top(24)
    .margin_bottom(24)
    .margin_start(24)
    .margin_end(24)
    .build();
  let title = gtk::Label::new(Some(title));
  title.add_css_class("title-1");
  title.set_xalign(0.0);
  page.append(&title);
  if !subtitle.is_empty() {
    let subtitle = dim_label(subtitle);
    subtitle.set_wrap(true);
    page.append(&subtitle);
  }
  page.append(content);
  let clamp = adw::Clamp::new();
  clamp.set_maximum_size(960);
  clamp.set_child(Some(&page));
  let scroll = gtk::ScrolledWindow::builder()
    .child(&clamp)
    .vexpand(true)
    .build();
  scroll.upcast()
}

pub(crate) fn library_kind(collection_type: &str) -> VideoLibraryKind {
  if collection_type.eq_ignore_ascii_case("tvshows") || collection_type.eq_ignore_ascii_case("tv") {
    VideoLibraryKind::TvShows
  } else {
    VideoLibraryKind::Movies
  }
}

pub(crate) fn library_shortcut_caption(shortcut: &VideoLibraryShortcut) -> String {
  let kind = match library_kind(&shortcut.collection_type) {
    VideoLibraryKind::TvShows => "TV Shows",
    VideoLibraryKind::Movies => "Movies",
  };
  match shortcut.item_count {
    Some(count) => format!("{kind} · {count}"),
    None => kind.to_owned(),
  }
}

fn poster_overlay(
  item: &VideoLibraryItem,
  width: i32,
  height: i32,
  fallback_pixel_size: i32,
) -> (gtk::Overlay, Option<ArtworkBind>) {
  let artwork_overlay = gtk::Overlay::new();
  artwork_overlay.add_css_class("jellypilot-poster");
  artwork_overlay.set_overflow(gtk::Overflow::Hidden);
  artwork_overlay.set_size_request(width, height);
  let picture = cover_picture(width, height);
  let fallback = gtk::Image::from_icon_name(FALLBACK_ARTWORK_ICON);
  fallback.set_pixel_size(fallback_pixel_size);
  fallback.set_halign(gtk::Align::Center);
  fallback.set_valign(gtk::Align::Center);
  artwork_overlay.set_child(Some(&picture));
  artwork_overlay.add_overlay(&fallback);
  if let Some(badge) = status_badge(item) {
    artwork_overlay.add_overlay(&badge);
  }
  if let Some(progress) = resume_progress_bar(item) {
    artwork_overlay.add_overlay(&progress);
  }
  let artwork = bind_image(item.artwork_image_id.as_deref(), picture, fallback);
  (artwork_overlay, artwork)
}

fn bind_image(
  image_id: Option<&str>,
  picture: gtk::Picture,
  fallback: gtk::Image,
) -> Option<ArtworkBind> {
  Some(ArtworkBind {
    image_id: image_id?.to_owned(),
    target: ArtworkTarget { picture, fallback },
  })
}

fn is_episode_item(item: &VideoLibraryItem) -> bool {
  item.item_type.eq_ignore_ascii_case("Episode")
}
